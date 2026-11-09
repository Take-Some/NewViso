use super::bootstrap_support::{load_shared_engine_xml, SHARED_RUNTIME_DEFAULTS};
use super::*;
use newviso_project::{ResolvedRuntimeConfiguration, RuntimeConfigurationRequest};

pub(super) fn load_effective_runtime_settings(
    project: &ResolvedProject,
    bootstrap: &ResolvedBootstrapConfig,
) -> Result<ResolvedRuntimeConfiguration, String> {
    let shared = load_shared_engine_xml(&bootstrap.assets_dir, SHARED_RUNTIME_DEFAULTS, "runtime")?;
    let request = RuntimeConfigurationRequest {
        platform: std::env::consts::OS.to_owned(),
        profile: bootstrap.runtime_profile.clone(),
        layers: bootstrap.runtime_layers.clone(),
        overrides: bootstrap.runtime_overrides.clone(),
    };
    let result = ResolvedRuntimeConfiguration::resolve(
        &project.manifest.configuration,
        &project.manifest.files.runtime,
        shared,
        SHARED_RUNTIME_DEFAULTS,
        &request,
        |path| {
            AssetClient::new()
                .json(path)
                .map_err(|error| error.to_string())
        },
    )
    .map_err(|error| error.to_string())?;
    host::info(
        "newviso.configuration",
        format!(
            "resolved runtime profile={} platform={} layers={}",
            result.report.profile.as_deref().unwrap_or("<none>"),
            result.report.platform,
            result
                .report
                .layers
                .iter()
                .map(|l| l.source.as_str())
                .collect::<Vec<_>>()
                .join(" -> ")
        ),
    );
    bugtrap::set_context(
        "runtime_profile",
        result.report.profile.as_deref().unwrap_or("<none>"),
    );
    Ok(result)
}

pub(super) fn load_settings_authoring(
    project: &ResolvedProject,
    bootstrap: &ResolvedBootstrapConfig,
) -> Result<(newviso_project::RuntimeSettingsDocument, Option<String>), String> {
    let shared = load_shared_engine_xml(&bootstrap.assets_dir, SHARED_RUNTIME_DEFAULTS, "runtime")?;
    let request = RuntimeConfigurationRequest {
        platform: std::env::consts::OS.to_owned(),
        profile: bootstrap.runtime_profile.clone(),
        layers: bootstrap.runtime_layers.clone(),
        overrides: bootstrap.runtime_overrides.clone(),
    };
    let document = newviso_project::RuntimeSettingsDocument::capture(
        &project.manifest.configuration,
        &project.manifest.files.runtime,
        shared,
        SHARED_RUNTIME_DEFAULTS,
        &request,
        |path| AssetClient::new().json(path),
    )?;
    // Match the VFS mount ordering, including project assets overlays. Never
    // write a same-named lower-priority file or a shared-engine authority file.
    let policy = super::bootstrap_support::shared_vfs_mount_policy(&bootstrap.assets_dir)?;
    let mut mounts = [
        (&policy.shared_assets, &bootstrap.assets_dir, 0),
        (&policy.project_root, &project.root, 1),
        (&policy.project_assets, &project.assets_dir, 2),
    ];
    mounts.sort_by(|a, b| b.0.priority.cmp(&a.0.priority).then(b.2.cmp(&a.2)));
    let logical = document.target();
    let mut relative_path = None;
    for (mount, root, owner) in mounts {
        let prefix = mount.mount.trim_matches('/');
        let relative = if prefix.is_empty() {
            Some(logical)
        } else {
            logical.strip_prefix(&format!("{prefix}/"))
        };
        let Some(relative) = relative else {
            continue;
        };
        let path = root.join(relative);
        if !path.is_file() {
            continue;
        }
        let canonical = path.canonicalize().map_err(|e| e.to_string())?;
        if !canonical.starts_with(root.canonicalize().map_err(|e| e.to_string())?) {
            return Err("Settings source escapes its mount".into());
        }
        let project_root = project.root.canonicalize().map_err(|e| e.to_string())?;
        if owner != 0 && canonical.starts_with(&project_root) {
            relative_path = Some(
                canonical
                    .strip_prefix(project_root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
        break;
    }
    Ok((document, relative_path))
}
