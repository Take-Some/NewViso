use super::*;
use std::path::Component;

pub(super) fn validate_manifest(manifest: &ProjectManifest) -> Result<(), ProjectError> {
    if manifest.schema != PROJECT_SCHEMA_V1 {
        return Err(ProjectError::Invalid(format!(
            "unsupported schema '{}', expected '{}'",
            manifest.schema, PROJECT_SCHEMA_V1
        )));
    }
    if manifest.project.id.trim().is_empty() {
        return Err(ProjectError::Invalid(
            "project.id must not be empty".to_owned(),
        ));
    }
    if manifest.project.name.trim().is_empty() {
        return Err(ProjectError::Invalid(
            "project.name must not be empty".to_owned(),
        ));
    }

    validate_project_relative_path("paths.assets", &manifest.paths.assets)?;
    validate_project_relative_path("paths.content", &manifest.paths.content)?;
    validate_project_relative_path("paths.cache", &manifest.paths.cache)?;

    validate_logical_asset_path("files.runtime", &manifest.files.runtime)?;
    validate_logical_asset_path("files.environment", &manifest.files.environment)?;
    validate_logical_asset_path("files.scene", &manifest.files.scene)?;
    if let Some(path) = &manifest.files.scripts {
        validate_logical_asset_path("files.scripts", path)?;
    }
    if let Some(scripts) = &manifest.scripts {
        if manifest.files.scripts.is_some() {
            return Err(ProjectError::Invalid(
                "project manifest cannot define both 'scripts' and legacy 'files.scripts'"
                    .to_owned(),
            ));
        }
        if scripts.enabled && scripts.provider.trim().is_empty() {
            return Err(ProjectError::Invalid(
                "scripts.provider must not be empty when scripting is enabled".to_owned(),
            ));
        }
        validate_logical_asset_path("scripts.entrypoint", &scripts.entrypoint)?;
        for (name, operation) in [
            (
                "scripts.lifecycle.start",
                scripts.lifecycle.start.as_deref(),
            ),
            (
                "scripts.lifecycle.frame",
                scripts.lifecycle.frame.as_deref(),
            ),
            (
                "scripts.lifecycle.shutdown",
                scripts.lifecycle.shutdown.as_deref(),
            ),
        ] {
            if operation.is_some_and(|value| value.trim().is_empty()) {
                return Err(ProjectError::Invalid(format!("{name} must not be empty")));
            }
        }
        for permission in &scripts.permissions {
            if permission.id.trim().is_empty() {
                return Err(ProjectError::Invalid(
                    "scripts.permissions[].id must not be empty".to_owned(),
                ));
            }
        }
    }
    if let Some(path) = &manifest.files.ui {
        validate_logical_asset_path("files.ui", path)?;
    }

    for request in manifest
        .capabilities
        .required
        .iter()
        .chain(manifest.capabilities.optional.iter())
    {
        if request.id.trim().is_empty() {
            return Err(ProjectError::Invalid(
                "capability id must not be empty".to_owned(),
            ));
        }
        if request.min_version == 0 {
            return Err(ProjectError::Invalid(format!(
                "capability '{}' min_version must be greater than zero",
                request.id
            )));
        }
    }

    Ok(())
}
pub(super) fn validate_project_relative_path(name: &str, path: &Path) -> Result<(), ProjectError> {
    if path.as_os_str().is_empty() {
        return Err(ProjectError::Invalid(format!("{name} must not be empty")));
    }
    if path.is_absolute() {
        return Err(ProjectError::Invalid(format!(
            "{name} must be relative to the project root"
        )));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(ProjectError::Invalid(format!(
            "{name} must stay inside the project root"
        )));
    }
    Ok(())
}
pub(super) fn validate_logical_asset_path(name: &str, value: &str) -> Result<(), ProjectError> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Err(ProjectError::Invalid(format!("{name} must not be empty")));
    }
    if Path::new(&normalized).is_absolute()
        || normalized.split('/').any(|component| component == "..")
    {
        return Err(ProjectError::Invalid(format!(
            "{name} must be a logical path inside the project"
        )));
    }
    Ok(())
}
pub(super) fn resolve_project_path(root: &Path, path: &Path) -> PathBuf {
    root.join(path)
}
