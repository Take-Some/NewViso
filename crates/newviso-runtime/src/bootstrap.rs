use super::bootstrap_support::*;
use super::*;

pub fn run(bootstrap: ResolvedBootstrapConfig) -> Result<RuntimeReport, String> {
    bugtrap::set_phase("runtime.bootstrap");
    host::reset();
    host::ensure_asset_types_registry()?;

    let mut state = EngineState::default();
    let mut bootstrap_providers = Vec::<RunningProvider>::new();

    let result = (|| -> Result<RuntimeReport, String> {
        log_bootstrap(&bootstrap);

        let project = bootstrap
            .project_path
            .as_ref()
            .map(ResolvedProject::load)
            .transpose()
            .map_err(|error| format!("project load failed: {error}"))?;

        let effective_cache_dir = project
            .as_ref()
            .map(|project| project.cache_dir.clone())
            .unwrap_or_else(|| bootstrap.cache_dir.clone());

        fs::create_dir_all(&effective_cache_dir).map_err(|error| {
            format!(
                "failed to create effective cache '{}': {error}",
                effective_cache_dir.display()
            )
        })?;

        env::set_var("NEWENGINE_CACHE_FILES", &effective_cache_dir);
        env::set_var("CACHE_FILES", &effective_cache_dir);
        env::set_var("NEWVISO_CACHE_DIR", &effective_cache_dir);

        if let Some(project) = &project {
            host::info(
                "newviso.project",
                format!(
                    "loaded project '{}' id={} version={} root={} manifest={}",
                    project.manifest.project.name,
                    project.manifest.project.id,
                    project.manifest.project.version,
                    project.root.display(),
                    project.manifest_path.display()
                ),
            );
        }

        host::info(
            "newviso.runtime",
            format!("effective cache root={}", effective_cache_dir.display()),
        );

        let roles = ProviderRoles::resolve(&bootstrap, project.as_ref());

        if !bootstrap.base_dir.is_dir() {
            return Err(format!(
                "bootstrap base directory does not exist: {}",
                bootstrap.base_dir.display()
            ));
        }

        env::set_current_dir(&bootstrap.base_dir).map_err(|error| {
            format!(
                "failed to switch process base directory to '{}': {error}",
                bootstrap.base_dir.display()
            )
        })?;

        state.set_phase(RuntimePhase::Boot);
        bugtrap::set_phase("providers.discovery");

        let providers = probe_directory(&bootstrap.provider_dir)
            .map_err(|error| format!("provider discovery failed: {error}"))?;
        log_provider_inventory(&providers);

        host::debug("newviso.runtime", "probing provider lifecycle ABI");
        for provider in &providers {
            bugtrap::set_context("provider_probe", provider.path.display().to_string());
            bugtrap::checkpoint("providers.probe");
            if provider.root_symbol == RootSymbol::Legacy {
                host::warn(
                    "newviso.providers",
                    format!(
                        "legacy provider '{}' version={} root={} deferred to legacy adapter",
                        provider.id,
                        provider.version,
                        provider.root_symbol.label()
                    ),
                );
                continue;
            }

            let report = probe_lifecycle(&provider.path).map_err(|error| {
                format!("provider ABI probe failed for {}: {error}", provider.id)
            })?;

            host::debug(
                "newviso.providers",
                format!(
                    "ABI probe passed provider='{}' version={} capabilities={} defaults={} bytes={} format={}",
                    report.descriptor_id,
                    report.descriptor_version,
                    report.capability_count,
                    report.default_content_type,
                    report.default_bytes,
                    report.default_format_version
                ),
            );
        }

        state.set_phase(RuntimePhase::Engine);

        for id in [
            roles.logging.as_str(),
            roles.input.as_str(),
            roles.assets.as_str(),
            roles.ecs.as_str(),
        ] {
            let info = require_provider(&providers, id)?;
            bugtrap::set_context("provider_load", info.path.display().to_string());
            bugtrap::checkpoint("providers.bootstrap_load");
            let mut running = RunningProvider::load_current(&info.path)?;
            running.initialize()?;

            host::info(
                "newviso.providers",
                format!(
                    "started provider '{}' from {}",
                    running.id(),
                    running.path().display()
                ),
            );

            bootstrap_providers.push(running);
        }

        host::debug(
            "newviso.host",
            format!(
                "registry ready services={:?} event_sinks={}",
                host::registered_service_ids(),
                host::event_sink_count()
            ),
        );

        let resolved_capabilities = if let Some(project) = &project {
            mount_project_files(project, &bootstrap.assets_dir)?;
            let resolved = resolve_project_capabilities(project, &providers)?;
            activate_project_capabilities(&resolved, &mut bootstrap_providers)?;
            resolved
        } else {
            Vec::new()
        };

        let loaded_project = if let Some(project) = &project {
            Some(load_project_files(project)?)
        } else {
            None
        };

        let ui_template = loaded_project
            .as_ref()
            .and_then(|files| files.ui_surface.clone());

        let ui_backend_active = resolved_capabilities.iter().any(|item| {
            item.candidate
                .as_ref()
                .and_then(|candidate| candidate.engine_gateway.as_deref())
                == Some("engine.ui")
        });
        let physics_backend_active = resolved_capabilities.iter().any(|item| {
            item.candidate
                .as_ref()
                .and_then(|candidate| candidate.engine_gateway.as_deref())
                == Some("engine.physics")
        });
        let physics_runtime = physics_backend_active
            .then(PhysicsRuntime::connect)
            .transpose()?;

        let content_manager = if ui_backend_active {
            project
                .as_ref()
                .map(|project| ContentManager::open(&project.root))
                .transpose()?
        } else {
            None
        };

        if content_manager.is_some() {
            host::info("newviso.content", "project content manager activated");
        }

        if let Some(project) = &project {
            if let Some(ui_path) = project.manifest.files.ui.as_deref() {
                host::info(
                    "newviso.ui",
                    format!("loaded project UI template asset='{ui_path}'"),
                );
            }
        }

        log_resolved_capabilities(&resolved_capabilities);

        let mut scripts = if let Some(files) = &loaded_project {
            start_project_scripting(files.scripts.as_ref(), &providers, &mut bootstrap_providers)?
        } else {
            None
        };

        let runtime_settings = loaded_project
            .as_ref()
            .map(|files| files.runtime.clone())
            .unwrap_or_default();
        let environment = loaded_project
            .as_ref()
            .map(|files| files.environment.clone())
            .unwrap_or_default();

        let (mut scene, scene_report) = match &project {
            Some(project) => Scene3dRuntime::load_from_asset(&project.manifest.files.scene)
                .map_err(|error| format!("startup scene load failed: {error}"))?,
            None => Scene3dRuntime::load_first_scene()
                .map_err(|error| format!("3D scene load failed: {error}"))?,
        };

        scene.set_clear_color(environment.clear_color);
        if let Some(sky_config) = environment.sky.as_ref() {
            scene.set_sky_dome(load_environment_sky(sky_config)?)?;
        }
        scene.configure_orbit(
            runtime_settings.camera.rotate_sensitivity,
            runtime_settings.camera.zoom_sensitivity,
            runtime_settings.camera.min_distance,
            runtime_settings.camera.max_distance,
        )?;

        host::info(
            "newviso.scene",
            format!(
                "loaded scene '{}' entities={} camera='{}' mesh='{}'",
                scene_report.title,
                scene_report.entity_count,
                scene_report.camera_name,
                scene_report.mesh_name
            ),
        );

        let project_context = loaded_project
            .as_ref()
            .map(|files| files.context.clone())
            .unwrap_or_else(|| json!({"project": null}));

        let platform_report = if bootstrap.skip_platform {
            host::info(
                "newviso.runtime",
                "platform runtime skipped by configuration",
            );
            None
        } else {
            let platform = require_provider(&providers, &roles.platform)?;
            let renderer = require_provider(&providers, &roles.renderer)?;

            state.set_phase(RuntimePhase::Platform);
            let app = Box::new(EngineApplication::new(
                renderer.path.clone(),
                scene,
                physics_runtime,
                scripts.take(),
                project_context,
                ui_template,
                content_manager,
                streaming_policy_from_project(&runtime_settings.streaming),
            )?);
            state.set_phase(RuntimePhase::Running);

            bugtrap::set_context("platform_provider", platform.path.display().to_string());
            bugtrap::checkpoint("platform.run");
            let report = run_platform(
                &platform.path,
                PlatformRunConfig {
                    title: runtime_settings
                        .window
                        .title
                        .clone()
                        .or_else(|| {
                            project
                                .as_ref()
                                .map(|project| project.manifest.project.name.clone())
                        })
                        .unwrap_or_else(|| "NewViso".to_owned()),
                    width: runtime_settings.window.width,
                    height: runtime_settings.window.height,
                    frame_limit: bootstrap.max_frames,
                },
                app,
            )?;

            if !report.window_ready {
                return Err("platform runtime exited before window_ready".to_owned());
            }
            if let Some(error) = &report.app_error {
                return Err(format!("runtime application failed: {error}"));
            }

            host::info(
                "newviso.runtime",
                format!(
                    "platform stopped frames={} events={} backend={:?} surface={}x{}",
                    report.frames,
                    report.emitted_events,
                    report.backend,
                    report.width,
                    report.height
                ),
            );

            Some(report)
        };

        Ok(RuntimeReport {
            provider_count: providers.len(),
            scene: scene_report,
            platform: platform_report,
        })
    })();

    if let Err(error) = &result {
        host::error("newviso.runtime", error.clone());
        bugtrap::set_phase("runtime.error");
        bugtrap::breadcrumb("runtime.error", error.clone());
    }

    state.set_phase(RuntimePhase::Shutdown);
    host::info("newviso.runtime", "shutdown");

    // Every provider shutdown runs while all provider DLLs are still resident.
    for provider in bootstrap_providers.iter_mut().rev() {
        bugtrap::set_context("provider_shutdown", provider.path().display().to_string());
        bugtrap::checkpoint("provider.shutdown");
        provider.shutdown();
    }

    // Host ABI objects (services/event sinks) may contain vtables implemented
    // inside provider DLLs. Destroy those objects before unloading any library.
    bugtrap::checkpoint("host.registry.release");
    host::reset();

    bugtrap::checkpoint("providers.unload");
    bootstrap_providers.clear();

    result
}
