use newviso_assets_client::AssetClient;
use newviso_compat_abi::platform::{PlatformSurfaceMetricsV1, PlatformWindowReadyV1};
use newviso_config::ResolvedBootstrapConfig;
use newviso_core::{EngineState, RuntimePhase};
use newviso_host as host;
use newviso_platform::{run_platform, PlatformApplication, PlatformRunConfig, PlatformRunReport};
use newviso_project::{
    ProjectEnvironment, ProjectRuntimeSettings, ProjectScripts, ResolvedProject,
};
use newviso_provider_runtime::{
    probe_directory, probe_lifecycle, ProviderInfo, RootSymbol, RunningProvider,
};
use newviso_scene::{Scene3dLoadReport, Scene3dRuntime};
use newviso_scripting::{ScriptModuleSpec, ScriptPermission, ScriptRuntime};
use serde_json::{json, Value};
use std::{env, fs, path::PathBuf};

pub struct RuntimeReport {
    pub provider_count: usize,
    pub scene: Scene3dLoadReport,
    pub platform: Option<PlatformRunReport>,
}

#[derive(Clone, Debug)]
struct ProviderRoles {
    logging: String,
    input: String,
    assets: String,
    ecs: String,
    platform: String,
    renderer: String,
}

impl ProviderRoles {
    fn resolve(bootstrap: &ResolvedBootstrapConfig, project: Option<&ResolvedProject>) -> Self {
        let overrides = project.map(|project| &project.manifest.providers);
        Self {
            logging: overrides
                .and_then(|providers| providers.logging.clone())
                .unwrap_or_else(|| bootstrap.logging_provider.clone()),
            input: overrides
                .and_then(|providers| providers.input.clone())
                .unwrap_or_else(|| bootstrap.input_provider.clone()),
            assets: overrides
                .and_then(|providers| providers.assets.clone())
                .unwrap_or_else(|| bootstrap.assets_provider.clone()),
            ecs: overrides
                .and_then(|providers| providers.ecs.clone())
                .unwrap_or_else(|| bootstrap.ecs_provider.clone()),
            platform: overrides
                .and_then(|providers| providers.platform.clone())
                .unwrap_or_else(|| bootstrap.platform_provider.clone()),
            renderer: overrides
                .and_then(|providers| providers.renderer.clone())
                .unwrap_or_else(|| bootstrap.renderer_provider.clone()),
        }
    }
}

#[derive(Clone, Debug)]
struct LoadedProjectFiles {
    runtime: ProjectRuntimeSettings,
    environment: ProjectEnvironment,
    scripts: Option<ProjectScripts>,
    context: Value,
}

struct EngineApplication {
    renderer_path: PathBuf,
    renderer: Option<RunningProvider>,
    scene: Scene3dRuntime,
    scripts: Option<ScriptRuntime>,
    project_context: Value,
    elapsed_seconds: f64,
    ready: bool,
    exit_requested: bool,
}

impl EngineApplication {
    fn new(
        renderer_path: PathBuf,
        scene: Scene3dRuntime,
        scripts: Option<ScriptRuntime>,
        project_context: Value,
    ) -> Self {
        Self {
            renderer_path,
            renderer: None,
            scene,
            scripts,
            project_context,
            elapsed_seconds: 0.0,
            ready: false,
            exit_requested: false,
        }
    }
}

impl PlatformApplication for EngineApplication {
    fn on_window_ready(&mut self, _ready: PlatformWindowReadyV1) -> Result<(), String> {
        let mut renderer = RunningProvider::load_current(&self.renderer_path)?;
        renderer.initialize()?;

        host::info(
            "newviso.runtime",
            format!("renderer '{}' initialized on live window", renderer.id()),
        );

        self.renderer = Some(renderer);
        self.scene.initialize_renderer()?;

        if let Some(scripts) = self.scripts.as_mut() {
            self.exit_requested |= scripts.start(&self.project_context)?.exit_requested;
        }

        self.ready = true;

        host::info(
            "newviso.runtime",
            format!("3D scene '{}' initialized", self.scene.title()),
        );
        Ok(())
    }

    fn step(&mut self, dt: f32, surface: PlatformSurfaceMetricsV1) -> Result<bool, String> {
        if !self.ready {
            return Ok(false);
        }

        self.elapsed_seconds += f64::from(dt);

        if let Some(scripts) = self.scripts.as_mut() {
            self.exit_requested |= scripts
                .frame(dt, self.elapsed_seconds, &self.project_context)?
                .exit_requested;
        }

        if self.exit_requested {
            return Ok(true);
        }

        self.scene.update_input()?;
        self.scene.render_frame(surface.width, surface.height)?;

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.update(dt)?;
            renderer.render(dt)?;
        }

        Ok(false)
    }

    fn shutdown(&mut self) {
        if let Some(scripts) = self.scripts.as_mut() {
            if let Err(error) = scripts.shutdown(&self.project_context) {
                host::warn(
                    "newviso.scripting",
                    format!("script shutdown hook failed: {error}"),
                );
            }
        }

        self.scene.shutdown_renderer();
        if let Some(mut renderer) = self.renderer.take() {
            renderer.shutdown();
        }
        self.ready = false;
    }
}

pub fn run(bootstrap: ResolvedBootstrapConfig) -> Result<RuntimeReport, String> {
    host::reset();

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

        let providers = probe_directory(&bootstrap.provider_dir)
            .map_err(|error| format!("provider discovery failed: {error}"))?;
        log_provider_inventory(&providers);

        host::debug("newviso.runtime", "probing provider lifecycle ABI");
        for provider in &providers {
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

        let loaded_project = if let Some(project) = &project {
            mount_project_files(project)?;
            Some(load_project_files(project)?)
        } else {
            None
        };

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
                scripts.take(),
                project_context,
            ));
            state.set_phase(RuntimePhase::Running);

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
    }

    state.set_phase(RuntimePhase::Shutdown);
    host::info("newviso.runtime", "shutdown");

    for provider in bootstrap_providers.iter_mut().rev() {
        provider.shutdown();
    }

    result
}

fn mount_project_files(project: &ResolvedProject) -> Result<(), String> {
    let assets = AssetClient::new();

    assets.mount_filesystem(&project.root, "", 1000)?;
    host::info(
        "newviso.project",
        format!(
            "mounted project root at VFS root path={} priority=1000",
            project.root.display()
        ),
    );

    if project.assets_dir.is_dir() {
        assets.mount_filesystem(&project.assets_dir, "", 1100)?;
        host::info(
            "newviso.project",
            format!(
                "mounted project assets overlay at VFS root path={} priority=1100",
                project.assets_dir.display()
            ),
        );
    }

    Ok(())
}

fn load_project_files(project: &ResolvedProject) -> Result<LoadedProjectFiles, String> {
    let assets = AssetClient::new();

    let runtime = ProjectRuntimeSettings::from_value(
        assets
            .json(&project.manifest.files.runtime)
            .map_err(|error| format!("runtime settings asset load failed: {error}"))?,
    )
    .map_err(|error| error.to_string())?;

    let environment = ProjectEnvironment::from_value(
        assets
            .json(&project.manifest.files.environment)
            .map_err(|error| format!("environment asset load failed: {error}"))?,
    )
    .map_err(|error| error.to_string())?;

    let scripts = project
        .manifest
        .files
        .scripts
        .as_deref()
        .map(|logical_path| {
            assets
                .json(logical_path)
                .map_err(|error| format!("scripts config asset load failed: {error}"))
                .and_then(|value| {
                    ProjectScripts::from_value(value).map_err(|error| error.to_string())
                })
        })
        .transpose()?;

    let context = json!({
        "identity": {
            "id": project.manifest.project.id,
            "name": project.manifest.project.name,
            "version": project.manifest.project.version,
        },
        "files": project.manifest.files,
        "runtime": runtime,
        "environment": environment,
    });

    host::info(
        "newviso.project",
        format!(
            "project files loaded runtime='{}' environment='{}' scene='{}' scripts={}",
            project.manifest.files.runtime,
            project.manifest.files.environment,
            project.manifest.files.scene,
            project
                .manifest
                .files
                .scripts
                .as_deref()
                .unwrap_or("<none>")
        ),
    );

    Ok(LoadedProjectFiles {
        runtime,
        environment,
        scripts,
        context,
    })
}

fn start_project_scripting(
    scripts: Option<&ProjectScripts>,
    providers: &[ProviderInfo],
    running_providers: &mut Vec<RunningProvider>,
) -> Result<Option<ScriptRuntime>, String> {
    let Some(scripts) = scripts else {
        return Ok(None);
    };
    if !scripts.enabled || scripts.modules.is_empty() {
        host::info("newviso.scripting", "project scripting disabled or empty");
        return Ok(None);
    }

    let provider = require_provider(providers, &scripts.provider).map_err(|_| {
        format!(
            "project requires scripting provider '{}' but its DLL is not deployed",
            scripts.provider
        )
    })?;

    let mut running = RunningProvider::load_current(&provider.path)?;
    running.initialize()?;
    host::info(
        "newviso.scripting",
        format!(
            "started scripting provider '{}' from {}",
            running.id(),
            running.path().display()
        ),
    );
    running_providers.push(running);

    let modules = scripts
        .modules
        .iter()
        .map(|module| ScriptModuleSpec {
            asset: module.asset.clone(),
            on_start: module.on_start.clone(),
            on_frame: module.on_frame.clone(),
            on_shutdown: module.on_shutdown.clone(),
            permissions: module
                .permissions
                .iter()
                .map(|permission| ScriptPermission {
                    id: permission.id.clone(),
                    scope: permission.scope.clone(),
                })
                .collect(),
        })
        .collect();

    let runtime = ScriptRuntime::load(modules)?;
    host::info(
        "newviso.scripting",
        format!("loaded {} project script modules", scripts.modules.len()),
    );
    Ok(Some(runtime))
}

fn require_provider<'a>(
    providers: &'a [ProviderInfo],
    id: &str,
) -> Result<&'a ProviderInfo, String> {
    providers
        .iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| format!("required provider '{id}' is missing"))
}

fn log_provider_inventory(providers: &[ProviderInfo]) {
    host::info(
        "newviso.providers",
        format!("discovered {} providers", providers.len()),
    );

    for provider in providers {
        host::debug(
            "newviso.providers",
            format!(
                "provider='{}' version={} kind={} phase={} root={} descriptor_v2={} path={}",
                provider.id,
                provider.version,
                provider.kind.label(),
                provider.bootstrap_phase.label(),
                provider.root_symbol.label(),
                provider.has_descriptor_v2,
                provider.path.display()
            ),
        );
    }
}

fn log_bootstrap(bootstrap: &ResolvedBootstrapConfig) {
    let config_source = if bootstrap.config_loaded {
        bootstrap.config_path.display().to_string()
    } else {
        format!("defaults ({})", bootstrap.config_path.display())
    };

    host::info(
        "newviso.bootstrap",
        format!(
            "starting config={} base={} providers={}",
            config_source,
            bootstrap.base_dir.display(),
            bootstrap.provider_dir.display()
        ),
    );

    if let Some(project_path) = &bootstrap.project_path {
        host::info(
            "newviso.bootstrap",
            format!("project request={}", project_path.display()),
        );
    }

    if let Some(max_frames) = bootstrap.max_frames {
        host::warn(
            "newviso.bootstrap",
            format!("runtime frame limit enabled explicitly: {max_frames}"),
        );
    }

    for item in &bootstrap.cli_overrides {
        host::debug("newviso.bootstrap", format!("CLI override {item}"));
    }
}
