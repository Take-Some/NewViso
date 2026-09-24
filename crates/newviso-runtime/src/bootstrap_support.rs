use super::*;

pub(super) fn builtin_vfs_mount_policy() -> Result<VfsMountPolicy, String> {
    let policy: VfsMountPolicy = serde_json::from_str(BUILTIN_VFS_MOUNTS_JSON)
        .map_err(|error| format!("invalid built-in VFS mount policy: {error}"))?;
    if policy.schema != "newviso.runtime.vfs_mount_policy.v1" {
        return Err(format!(
            "unsupported built-in VFS mount policy schema '{}'",
            policy.schema
        ));
    }
    Ok(policy)
}
pub(super) fn resolve_project_capabilities(
    project: &ResolvedProject,
    providers: &[ProviderInfo],
) -> Result<Vec<ResolvedCapability>, String> {
    let required = project
        .manifest
        .capabilities
        .required
        .iter()
        .map(|request| CapabilityNeed {
            id: request.id.clone(),
            min_version: request.min_version,
            provider: request.provider.clone(),
            required: true,
        });
    let optional = project
        .manifest
        .capabilities
        .optional
        .iter()
        .map(|request| CapabilityNeed {
            id: request.id.clone(),
            min_version: request.min_version,
            provider: request.provider.clone(),
            required: false,
        });

    resolve_capabilities(providers, required.chain(optional))
}
pub(super) fn activate_project_capabilities(
    resolved: &[ResolvedCapability],
    running_providers: &mut Vec<RunningProvider>,
) -> Result<(), String> {
    for item in resolved {
        let Some(candidate) = &item.candidate else {
            host::warn(
                "newviso.capabilities",
                format!(
                    "optional capability '{}' version>={} unavailable",
                    item.need.id, item.need.min_version
                ),
            );
            continue;
        };

        if candidate.bootstrap_phase == BootstrapPhase::Platform
            || candidate.engine_gateway.as_deref() == Some("engine.platform")
            || candidate.engine_gateway.as_deref() == Some("engine.render")
        {
            host::info(
                "newviso.capabilities",
                format!(
                    "capability '{}' resolved provider='{}' gateway={} phase={} activation=deferred",
                    candidate.capability_id,
                    candidate.provider_id,
                    candidate.engine_gateway.as_deref().unwrap_or("<none>"),
                    candidate.bootstrap_phase.label()
                ),
            );
            continue;
        }

        let already_running = running_providers
            .iter()
            .any(|provider| provider.id() == candidate.provider_id);

        if !already_running {
            let mut running = RunningProvider::load_current(&candidate.provider_path)?;
            running.initialize()?;
            host::info(
                "newviso.capabilities",
                format!(
                    "activated capability '{}' provider='{}' version={} priority={} path={}",
                    candidate.capability_id,
                    running.id(),
                    candidate.version,
                    candidate.priority,
                    candidate.provider_path.display()
                ),
            );
            running_providers.push(running);
        }

        if let (Some(gateway), Some(service_id)) =
            (&candidate.engine_gateway, &candidate.service_id)
        {
            let registered = host::registered_service_ids()
                .iter()
                .any(|registered| registered == service_id);
            if !registered {
                let message = format!(
                    "capability '{}' provider='{}' did not register descriptor service '{}'",
                    candidate.capability_id, candidate.provider_id, service_id
                );
                if item.need.required {
                    return Err(message);
                }
                host::warn("newviso.capabilities", message);
                continue;
            }

            host::add_alias(gateway.clone(), service_id.clone());
            host::info(
                "newviso.capabilities",
                format!(
                    "bound capability '{}' gateway='{}' -> service='{}' provider='{}'",
                    candidate.capability_id, gateway, service_id, candidate.provider_id
                ),
            );
        }
    }

    Ok(())
}
pub(super) fn log_resolved_capabilities(resolved: &[ResolvedCapability]) {
    for item in resolved {
        match &item.candidate {
            Some(candidate) => host::info(
                "newviso.capabilities",
                format!(
                    "resolved {} capability='{}' min_version={} provider='{}' version={} priority={} gateway={} service={}",
                    if item.need.required { "required" } else { "optional" },
                    item.need.id,
                    item.need.min_version,
                    candidate.provider_id,
                    candidate.version,
                    candidate.priority,
                    candidate.engine_gateway.as_deref().unwrap_or("<none>"),
                    candidate.service_id.as_deref().unwrap_or("<none>")
                ),
            ),
            None => host::warn(
                "newviso.capabilities",
                format!(
                    "unresolved optional capability='{}' min_version={}",
                    item.need.id, item.need.min_version
                ),
            ),
        }
    }
}
pub(super) fn mount_project_files(
    project: &ResolvedProject,
    shared_assets_dir: &Path,
) -> Result<(), String> {
    let assets = AssetClient::new();
    let policy = builtin_vfs_mount_policy()?;

    if shared_assets_dir.is_dir() {
        assets.mount_filesystem(
            shared_assets_dir,
            &policy.shared_assets.mount,
            policy.shared_assets.priority,
        )?;
        host::info(
            "newviso.project",
            format!(
                "mounted engine Shared Assets at VFS root path={} priority={}",
                shared_assets_dir.display(),
                policy.shared_assets.priority
            ),
        );
    } else {
        host::warn(
            "newviso.project",
            format!(
                "engine Shared Assets directory is missing path={}",
                shared_assets_dir.display()
            ),
        );
    }

    assets.mount_filesystem(
        &project.root,
        &policy.project_root.mount,
        policy.project_root.priority,
    )?;
    host::info(
        "newviso.project",
        format!(
            "mounted project root at VFS root path={} priority={}",
            project.root.display(),
            policy.project_root.priority
        ),
    );

    if project.assets_dir.is_dir() {
        assets.mount_filesystem(
            &project.assets_dir,
            &policy.project_assets.mount,
            policy.project_assets.priority,
        )?;
        host::info(
            "newviso.project",
            format!(
                "mounted project assets overlay at VFS root path={} priority={}",
                project.assets_dir.display(),
                policy.project_assets.priority
            ),
        );
    }

    Ok(())
}
pub(super) fn load_project_files(project: &ResolvedProject) -> Result<LoadedProjectFiles, String> {
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

    let scripts = if let Some(manifest_scripts) = project.manifest.scripts.as_ref() {
        Some(manifest_scripts.as_runtime_config())
    } else {
        project
            .manifest
            .files
            .scripts
            .as_deref()
            .map(|logical_path| {
                assets
                    .json(logical_path)
                    .map_err(|error| format!("legacy scripts config asset load failed: {error}"))
                    .and_then(|value| {
                        ProjectScripts::from_value(value).map_err(|error| error.to_string())
                    })
            })
            .transpose()?
    };

    let ui_surface = project
        .manifest
        .files
        .ui
        .as_deref()
        .map(|logical_path| {
            assets
                .decode_json(logical_path, "ui.surface.v1", Value::Null)
                .map_err(|error| format!("UI semantic asset load failed: {error}"))
        })
        .transpose()?;

    let context = json!({
        "identity": {
            "id": project.manifest.project.id,
            "name": project.manifest.project.name,
            "version": project.manifest.project.version,
        },
        "files": project.manifest.files,
        "scripts": project.manifest.scripts,
        "runtime": runtime,
        "environment": environment,
        "capabilities": project.manifest.capabilities,
    });

    host::info(
        "newviso.project",
        format!(
            "project files loaded runtime='{}' environment='{}' scene='{}' scripts={} ui={}",
            project.manifest.files.runtime,
            project.manifest.files.environment,
            project.manifest.files.scene,
            project
                .manifest
                .scripts
                .as_ref()
                .map(|scripts| scripts.entrypoint.as_str())
                .or_else(|| project.manifest.files.scripts.as_deref())
                .unwrap_or("<none>"),
            project.manifest.files.ui.as_deref().unwrap_or("<none>")
        ),
    );

    Ok(LoadedProjectFiles {
        runtime,
        environment,
        scripts,
        ui_surface,
        context,
    })
}
pub(super) fn start_project_scripting(
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
            on_event: module.on_event.clone(),
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
        format!(
            "loaded project script entrypoint='{}'",
            scripts
                .modules
                .first()
                .map(|module| module.asset.as_str())
                .unwrap_or("<none>")
        ),
    );
    Ok(Some(runtime))
}
pub(super) fn require_provider<'a>(
    providers: &'a [ProviderInfo],
    id: &str,
) -> Result<&'a ProviderInfo, String> {
    providers
        .iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| format!("required provider '{id}' is missing"))
}
pub(super) fn log_provider_inventory(providers: &[ProviderInfo]) {
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
pub(super) fn log_bootstrap(bootstrap: &ResolvedBootstrapConfig) {
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
