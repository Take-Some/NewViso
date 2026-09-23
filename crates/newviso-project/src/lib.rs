use serde::{Deserialize, Serialize};
use std::{
    fmt, fs,
    path::{Component, Path, PathBuf},
};

pub const PROJECT_MANIFEST_NAME: &str = "project.json";
pub const PROJECT_SCHEMA_V1: &str = "newviso.project.v1";
pub const RUNTIME_SETTINGS_SCHEMA_V1: &str = "newviso.project.runtime.v1";
pub const ENVIRONMENT_SCHEMA_V1: &str = "newviso.environment.v1";
pub const SCRIPTS_SCHEMA_V1: &str = "newviso.scripts.v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub schema: String,
    pub project: ProjectIdentity,
    #[serde(default)]
    pub paths: ProjectPaths,
    pub files: ProjectFiles,
    /// Project-owned scripting graph. The manifest names one root entrypoint;
    /// relative child imports are resolved by the selected scripting provider.
    #[serde(default)]
    pub scripts: Option<ProjectScriptEntrypoint>,
    #[serde(default)]
    pub capabilities: ProjectCapabilities,
    #[serde(default)]
    pub providers: ProjectProviders,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectIdentity {
    pub id: String,
    pub name: String,
    #[serde(default = "default_project_version")]
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectPaths {
    pub assets: PathBuf,
    pub content: PathBuf,
    pub cache: PathBuf,
}

impl Default for ProjectPaths {
    fn default() -> Self {
        Self {
            assets: PathBuf::from("assets"),
            content: PathBuf::from("content"),
            cache: PathBuf::from(".newviso/cache"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectFiles {
    pub runtime: String,
    pub environment: String,
    pub scene: String,
    #[serde(default)]
    pub scripts: Option<String>,
    #[serde(default)]
    pub ui: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectScriptEntrypoint {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub provider: String,
    pub entrypoint: String,
    #[serde(default)]
    pub lifecycle: ProjectScriptLifecycle,
    #[serde(default)]
    pub permissions: Vec<ProjectScriptPermission>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectScriptLifecycle {
    pub start: Option<String>,
    pub frame: Option<String>,
    pub shutdown: Option<String>,
}

impl Default for ProjectScriptLifecycle {
    fn default() -> Self {
        Self {
            start: Some("on_start".to_owned()),
            frame: Some("on_frame".to_owned()),
            shutdown: Some("on_shutdown".to_owned()),
        }
    }
}

impl ProjectScriptEntrypoint {
    pub fn as_runtime_config(&self) -> ProjectScripts {
        ProjectScripts {
            schema: SCRIPTS_SCHEMA_V1.to_owned(),
            enabled: self.enabled,
            provider: self.provider.clone(),
            modules: vec![ProjectScriptModule {
                asset: self.entrypoint.clone(),
                on_start: self.lifecycle.start.clone(),
                on_frame: self.lifecycle.frame.clone(),
                on_shutdown: self.lifecycle.shutdown.clone(),
                permissions: self.permissions.clone(),
            }],
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectCapabilities {
    pub required: Vec<ProjectCapabilityRequest>,
    pub optional: Vec<ProjectCapabilityRequest>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectCapabilityRequest {
    pub id: String,
    #[serde(default = "default_capability_version")]
    pub min_version: u32,
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectProviders {
    pub logging: Option<String>,
    pub input: Option<String>,
    pub assets: Option<String>,
    pub ecs: Option<String>,
    pub platform: Option<String>,
    pub renderer: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectRuntimeSettings {
    pub schema: String,
    #[serde(default)]
    pub window: ProjectWindowSettings,
    #[serde(default)]
    pub camera: ProjectCameraSettings,
    #[serde(default)]
    pub streaming: ProjectStreamingSettings,
}

impl Default for ProjectRuntimeSettings {
    fn default() -> Self {
        Self {
            schema: RUNTIME_SETTINGS_SCHEMA_V1.to_owned(),
            window: ProjectWindowSettings::default(),
            camera: ProjectCameraSettings::default(),
            streaming: ProjectStreamingSettings::default(),
        }
    }
}

impl ProjectRuntimeSettings {
    pub fn from_value(value: serde_json::Value) -> Result<Self, ProjectError> {
        let settings: Self = serde_json::from_value(value).map_err(|error| {
            ProjectError::Asset(format!("runtime settings decode failed: {error}"))
        })?;
        if settings.schema != RUNTIME_SETTINGS_SCHEMA_V1 {
            return Err(ProjectError::Asset(format!(
                "unsupported runtime settings schema '{}', expected '{}'",
                settings.schema, RUNTIME_SETTINGS_SCHEMA_V1
            )));
        }
        if settings.window.width == 0 || settings.window.height == 0 {
            return Err(ProjectError::Asset(
                "runtime window width and height must be greater than zero".to_owned(),
            ));
        }
        if settings.camera.min_distance <= 0.0
            || settings.camera.max_distance < settings.camera.min_distance
        {
            return Err(ProjectError::Asset(
                "camera distance limits are invalid".to_owned(),
            ));
        }
        if !settings.streaming.dependency_priority_scale.is_finite()
            || !(0.0..=1.0).contains(&settings.streaming.dependency_priority_scale)
        {
            return Err(ProjectError::Asset(
                "streaming dependency_priority_scale must be finite and in 0..=1".to_owned(),
            ));
        }
        Ok(settings)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectStreamingSettings {
    /// Unique source-container residency budget in MiB. Zero disables the cap.
    pub max_resident_mb: u64,
    /// Maximum generic resources decoded by one streamer pump. Zero is unlimited.
    pub max_loads_per_tick: usize,
    /// Approximate source bytes decoded by one pump in MiB. Zero is unlimited.
    pub max_source_mb_per_tick: u64,
    /// Frames an unrequested resident resource is retained to prevent churn.
    pub eviction_grace_frames: u64,
    /// Frames before a requested failed resource is retried.
    pub failed_retry_frames: u64,
    /// Priority inherited by declared resource dependencies.
    pub dependency_priority_scale: f32,
}

impl Default for ProjectStreamingSettings {
    fn default() -> Self {
        Self {
            max_resident_mb: 512,
            max_loads_per_tick: 8,
            max_source_mb_per_tick: 32,
            eviction_grace_frames: 120,
            failed_retry_frames: 120,
            dependency_priority_scale: 0.95,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectWindowSettings {
    pub title: Option<String>,
    pub width: u32,
    pub height: u32,
}

impl Default for ProjectWindowSettings {
    fn default() -> Self {
        Self {
            title: None,
            width: 1280,
            height: 720,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectCameraSettings {
    pub rotate_sensitivity: f32,
    pub zoom_sensitivity: f32,
    pub min_distance: f32,
    pub max_distance: f32,
}

impl Default for ProjectCameraSettings {
    fn default() -> Self {
        Self {
            rotate_sensitivity: 0.005,
            zoom_sensitivity: 0.0015,
            min_distance: 2.0,
            max_distance: 40.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectSkyEnvironment {
    pub model: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectEnvironment {
    pub schema: String,
    #[serde(default = "default_clear_color")]
    pub clear_color: [f32; 4],
    #[serde(default)]
    pub sky: Option<ProjectSkyEnvironment>,
}

impl Default for ProjectEnvironment {
    fn default() -> Self {
        Self {
            schema: ENVIRONMENT_SCHEMA_V1.to_owned(),
            clear_color: default_clear_color(),
            sky: None,
        }
    }
}

impl ProjectEnvironment {
    pub fn from_value(value: serde_json::Value) -> Result<Self, ProjectError> {
        let environment: Self = serde_json::from_value(value)
            .map_err(|error| ProjectError::Asset(format!("environment decode failed: {error}")))?;
        if environment.schema != ENVIRONMENT_SCHEMA_V1 {
            return Err(ProjectError::Asset(format!(
                "unsupported environment schema '{}', expected '{}'",
                environment.schema, ENVIRONMENT_SCHEMA_V1
            )));
        }
        if environment
            .clear_color
            .iter()
            .any(|value| !value.is_finite())
        {
            return Err(ProjectError::Asset(
                "environment clear_color contains a non-finite value".to_owned(),
            ));
        }
        if let Some(sky) = &environment.sky {
            validate_logical_asset_path("environment.sky.model", &sky.model)?;
            if !sky.model.contains('@') {
                return Err(ProjectError::Asset(
                    "environment.sky.model must address a specific semantic entry".to_owned(),
                ));
            }
        }
        Ok(environment)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectScripts {
    pub schema: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub provider: String,
    #[serde(default)]
    pub modules: Vec<ProjectScriptModule>,
}

impl ProjectScripts {
    pub fn from_value(value: serde_json::Value) -> Result<Self, ProjectError> {
        let scripts: Self = serde_json::from_value(value).map_err(|error| {
            ProjectError::Asset(format!("scripts config decode failed: {error}"))
        })?;
        if scripts.schema != SCRIPTS_SCHEMA_V1 {
            return Err(ProjectError::Asset(format!(
                "unsupported scripts schema '{}', expected '{}'",
                scripts.schema, SCRIPTS_SCHEMA_V1
            )));
        }
        if scripts.enabled && scripts.provider.trim().is_empty() {
            return Err(ProjectError::Asset(
                "scripts provider must not be empty when scripting is enabled".to_owned(),
            ));
        }
        for module in &scripts.modules {
            validate_logical_asset_path("scripts.modules[].asset", &module.asset)?;
        }
        Ok(scripts)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectScriptModule {
    pub asset: String,
    #[serde(default)]
    pub on_start: Option<String>,
    #[serde(default)]
    pub on_frame: Option<String>,
    #[serde(default)]
    pub on_shutdown: Option<String>,
    #[serde(default)]
    pub permissions: Vec<ProjectScriptPermission>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectScriptPermission {
    pub id: String,
    #[serde(default)]
    pub scope: String,
}

#[derive(Clone, Debug)]
pub struct ResolvedProject {
    pub manifest_path: PathBuf,
    pub root: PathBuf,
    pub manifest: ProjectManifest,
    pub assets_dir: PathBuf,
    pub content_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl ResolvedProject {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ProjectError> {
        let requested = path.as_ref();
        let requested = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(ProjectError::CurrentDirectory)?
                .join(requested)
        };

        let manifest_path = if requested.is_dir() {
            requested.join(PROJECT_MANIFEST_NAME)
        } else {
            requested
        };

        if !manifest_path.is_file() {
            return Err(ProjectError::ManifestMissing(manifest_path));
        }

        let bytes = fs::read(&manifest_path).map_err(|source| ProjectError::Read {
            path: manifest_path.clone(),
            source,
        })?;

        let manifest = serde_json::from_slice::<ProjectManifest>(&bytes).map_err(|source| {
            ProjectError::Parse {
                path: manifest_path.clone(),
                source,
            }
        })?;

        validate_manifest(&manifest)?;

        let root = manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        let assets_dir = resolve_project_path(&root, &manifest.paths.assets);
        let content_dir = resolve_project_path(&root, &manifest.paths.content);
        let cache_dir = resolve_project_path(&root, &manifest.paths.cache);

        Ok(Self {
            manifest_path,
            root,
            manifest,
            assets_dir,
            content_dir,
            cache_dir,
        })
    }
}

#[derive(Debug)]
pub enum ProjectError {
    CurrentDirectory(std::io::Error),
    ManifestMissing(PathBuf),
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    Invalid(String),
    Asset(String),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CurrentDirectory(error) => {
                write!(f, "failed to resolve launch directory: {error}")
            }
            Self::ManifestMissing(path) => {
                write!(f, "project manifest not found: {}", path.display())
            }
            Self::Read { path, source } => {
                write!(
                    f,
                    "failed to read project manifest '{}': {source}",
                    path.display()
                )
            }
            Self::Parse { path, source } => {
                write!(f, "invalid project manifest '{}': {source}", path.display())
            }
            Self::Invalid(message) => write!(f, "invalid project manifest: {message}"),
            Self::Asset(message) => write!(f, "invalid project asset: {message}"),
        }
    }
}

impl std::error::Error for ProjectError {}

fn validate_manifest(manifest: &ProjectManifest) -> Result<(), ProjectError> {
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

fn validate_project_relative_path(name: &str, path: &Path) -> Result<(), ProjectError> {
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

fn validate_logical_asset_path(name: &str, value: &str) -> Result<(), ProjectError> {
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

fn resolve_project_path(root: &Path, path: &Path) -> PathBuf {
    root.join(path)
}

fn default_project_version() -> String {
    "0.1.0".to_owned()
}

fn default_capability_version() -> u32 {
    1
}

fn default_clear_color() -> [f32; 4] {
    [0.025, 0.032, 0.045, 1.0]
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("newviso-project-{label}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn directory_resolves_project_json_and_paths() {
        let root = temp_dir("load");
        fs::write(
            root.join(PROJECT_MANIFEST_NAME),
            r#"{
              "schema": "newviso.project.v1",
              "project": {"id": "demo", "name": "Demo"},
              "files": {
                "runtime": "config/runtime.json",
                "environment": "environments/default.environment.json",
                "scene": "scenes/main.scene.json"
              }
            }"#,
        )
        .unwrap();

        let project = ResolvedProject::load(&root).unwrap();
        assert_eq!(project.manifest.project.id, "demo");
        assert_eq!(project.assets_dir, root.join("assets"));
        assert_eq!(project.manifest.files.scene, "scenes/main.scene.json");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn first_fps_manifest_declares_script_entrypoint() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../projects/FirstFPS");
        let project = ResolvedProject::load(&root)
            .unwrap_or_else(|error| panic!("FirstFPS manifest must load: {error}"));
        let scripts = project
            .manifest
            .scripts
            .as_ref()
            .expect("FirstFPS must declare scripts in project.json");
        assert_eq!(scripts.provider, "engine.scripting.typescript");
        assert_eq!(scripts.entrypoint, "scripts/main.ysc");
        assert!(project.manifest.files.scripts.is_none());
    }

    #[test]
    fn rejects_escaping_project_asset_reference() {
        let root = temp_dir("escape");
        fs::write(
            root.join(PROJECT_MANIFEST_NAME),
            r#"{
              "schema": "newviso.project.v1",
              "project": {"id": "demo", "name": "Demo"},
              "files": {
                "runtime": "../runtime.json",
                "environment": "environment.json",
                "scene": "scene.json"
              }
            }"#,
        )
        .unwrap();

        assert!(ResolvedProject::load(&root).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
