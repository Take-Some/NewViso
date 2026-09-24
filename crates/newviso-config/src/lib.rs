use serde::{Deserialize, Serialize};
use std::{
    env,
    ffi::OsString,
    fmt, fs,
    path::{Path, PathBuf},
};

pub const CONFIG_FILE_NAME: &str = "config.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BootstrapConfig {
    pub paths: PathConfig,
    pub providers: ProviderSelectionConfig,
    pub runtime: RuntimeConfig,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            paths: PathConfig::default(),
            providers: ProviderSelectionConfig::default(),
            runtime: RuntimeConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PathConfig {
    pub base: PathBuf,
    pub providers: PathBuf,
    pub assets: PathBuf,
    pub content: PathBuf,
    pub cache: PathBuf,
    pub codecs: PathBuf,
}

impl Default for PathConfig {
    fn default() -> Self {
        Self {
            base: PathBuf::from("."),
            providers: PathBuf::from("runtime/providers"),
            assets: PathBuf::from("assets"),
            content: PathBuf::from("content"),
            cache: PathBuf::from("cache"),
            codecs: PathBuf::from("runtime/providers/codecs"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderSelectionConfig {
    pub logging: String,
    pub input: String,
    pub assets: String,
    pub ecs: String,
    pub platform: String,
    pub renderer: String,
}

impl Default for ProviderSelectionConfig {
    fn default() -> Self {
        Self {
            logging: "engine.logging.chronicle".to_owned(),
            input: "engine.input.compass".to_owned(),
            assets: "engine.assets.starvault".to_owned(),
            ecs: "engine.ecs.constellation".to_owned(),
            platform: "engine.platform.winit".to_owned(),
            renderer: "engine.render.vulkan".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub max_frames: Option<u64>,
    pub skip_platform: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_frames: None,
            skip_platform: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedBootstrapConfig {
    pub executable: PathBuf,
    pub executable_dir: PathBuf,
    pub config_path: PathBuf,
    pub config_loaded: bool,
    pub base_dir: PathBuf,
    pub provider_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub content_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub codecs_dir: PathBuf,
    pub logging_provider: String,
    pub input_provider: String,
    pub assets_provider: String,
    pub ecs_provider: String,
    pub platform_provider: String,
    pub renderer_provider: String,
    pub max_frames: Option<u64>,
    pub skip_platform: bool,
    pub project_path: Option<PathBuf>,
    pub cli_overrides: Vec<String>,
}

#[derive(Debug)]
pub enum BootstrapConfigError {
    CurrentExecutable(std::io::Error),
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    Cli(String),
}

impl fmt::Display for BootstrapConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CurrentExecutable(error) => {
                write!(f, "failed to resolve current executable: {error}")
            }
            Self::Read { path, source } => {
                write!(
                    f,
                    "failed to read bootstrap config '{}': {source}",
                    path.display()
                )
            }
            Self::Parse { path, source } => {
                write!(f, "invalid bootstrap config '{}': {source}", path.display())
            }
            Self::Cli(message) => write!(f, "invalid NewViso command line: {message}"),
        }
    }
}

impl std::error::Error for BootstrapConfigError {}

mod cli;
use cli::*;

impl ResolvedBootstrapConfig {
    pub fn load() -> Result<Self, BootstrapConfigError> {
        let executable = env::current_exe().map_err(BootstrapConfigError::CurrentExecutable)?;
        Self::load_for_executable_and_args(executable, env::args_os().skip(1))
    }

    pub fn load_for_executable(
        executable: impl Into<PathBuf>,
    ) -> Result<Self, BootstrapConfigError> {
        Self::load_for_executable_and_args(executable, std::iter::empty::<OsString>())
    }

    pub fn load_for_executable_and_args<I, S>(
        executable: impl Into<PathBuf>,
        args: I,
    ) -> Result<Self, BootstrapConfigError>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let executable = executable.into();
        let executable_dir = executable
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        let cli = parse_cli(args)?;

        let config_path = cli
            .config_path
            .clone()
            .or_else(|| env::var_os("NEWVISO_CONFIG").map(PathBuf::from))
            .unwrap_or_else(|| executable_dir.join(CONFIG_FILE_NAME));

        let (config, config_loaded) = if config_path.is_file() {
            let bytes = fs::read(&config_path).map_err(|source| BootstrapConfigError::Read {
                path: config_path.clone(),
                source,
            })?;
            let config = serde_json::from_slice::<BootstrapConfig>(&bytes).map_err(|source| {
                BootstrapConfigError::Parse {
                    path: config_path.clone(),
                    source,
                }
            })?;
            (config, true)
        } else {
            (BootstrapConfig::default(), false)
        };

        let project_path = cli
            .project_path
            .clone()
            .or_else(|| env::var_os("NEWVISO_PROJECT").map(PathBuf::from));

        let configured_base = cli
            .path("paths.base")
            .or_else(|| env_path("NEWVISO_BASE_DIR"))
            .unwrap_or_else(|| config.paths.base.clone());

        let default_anchor = if config_loaded {
            executable_dir.clone()
        } else {
            env::current_dir().unwrap_or_else(|_| executable_dir.clone())
        };
        let base_dir = resolve_from(&default_anchor, &configured_base);

        let provider_dir = resolve_layered_path(
            cli.path("paths.providers"),
            "NEWVISO_PROVIDER_DIR",
            &base_dir,
            &config.paths.providers,
        );
        let assets_dir = resolve_layered_path(
            cli.path("paths.assets"),
            "NEWVISO_ASSETS_DIR",
            &base_dir,
            &config.paths.assets,
        );
        let content_dir = resolve_layered_path(
            cli.path("paths.content"),
            "NEWVISO_CONTENT_DIR",
            &base_dir,
            &config.paths.content,
        );
        let cache_dir = resolve_layered_path(
            cli.path("paths.cache"),
            "NEWVISO_CACHE_DIR",
            &base_dir,
            &config.paths.cache,
        );
        let codecs_dir = resolve_layered_path(
            cli.path("paths.codecs"),
            "NEWVISO_CODECS_DIR",
            &base_dir,
            &config.paths.codecs,
        );

        let max_frames = cli
            .u64("runtime.max_frames")?
            .or_else(|| {
                env::var("NEWVISO_MAX_FRAMES")
                    .ok()
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .or(config.runtime.max_frames)
            .map(|value| value.max(1));

        let skip_platform = cli
            .bool("runtime.skip_platform")?
            .or_else(|| env_bool("NEWVISO_SKIP_PLATFORM"))
            .unwrap_or(config.runtime.skip_platform);

        let logging_provider = resolve_layered_string(
            cli.get("providers.logging"),
            "NEWVISO_LOGGING_PROVIDER",
            &config.providers.logging,
        );
        let input_provider = resolve_layered_string(
            cli.get("providers.input"),
            "NEWVISO_INPUT_PROVIDER",
            &config.providers.input,
        );
        let assets_provider = resolve_layered_string(
            cli.get("providers.assets"),
            "NEWVISO_ASSETS_PROVIDER",
            &config.providers.assets,
        );
        let ecs_provider = resolve_layered_string(
            cli.get("providers.ecs"),
            "NEWVISO_ECS_PROVIDER",
            &config.providers.ecs,
        );
        let platform_provider = resolve_layered_string(
            cli.get("providers.platform"),
            "NEWVISO_PLATFORM_PROVIDER",
            &config.providers.platform,
        );
        let renderer_provider = resolve_layered_string(
            cli.get("providers.renderer"),
            "NEWVISO_RENDERER_PROVIDER",
            &config.providers.renderer,
        );

        Ok(Self {
            executable,
            executable_dir,
            config_path,
            config_loaded,
            base_dir,
            provider_dir,
            assets_dir,
            content_dir,
            cache_dir,
            codecs_dir,
            logging_provider,
            input_provider,
            assets_provider,
            ecs_provider,
            platform_provider,
            renderer_provider,
            max_frames,
            skip_platform,
            project_path,
            cli_overrides: cli.applied,
        })
    }
}

fn env_string(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn resolve_layered_string(cli: Option<&str>, env_name: &str, configured: &str) -> String {
    cli.map(str::to_owned)
        .or_else(|| env_string(env_name))
        .unwrap_or_else(|| configured.to_owned())
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn resolve_layered_path(
    cli: Option<PathBuf>,
    env_name: &str,
    base: &Path,
    configured: &Path,
) -> PathBuf {
    let path = cli
        .or_else(|| env_path(env_name))
        .unwrap_or_else(|| configured.to_path_buf());
    resolve_from(base, &path)
}

fn resolve_from(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn env_bool(name: &str) -> Option<bool> {
    env::var(name).ok().and_then(|value| parse_bool(&value))
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
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
        let path = env::temp_dir().join(format!("newviso-{label}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn missing_config_keeps_launch_directory_defaults() {
        let root = temp_dir("bootstrap-default");
        let exe = root.join("bin").join("newviso.exe");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();

        let config = ResolvedBootstrapConfig::load_for_executable(&exe).unwrap();

        assert_eq!(config.executable_dir, root.join("bin"));
        assert!(!config.config_loaded);
        assert!(config.provider_dir.ends_with("runtime/providers"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn adjacent_config_overrides_defaults() {
        let root = temp_dir("bootstrap-config");
        let exe_dir = root.join("pack");
        fs::create_dir_all(&exe_dir).unwrap();
        let exe = exe_dir.join("newviso.exe");

        fs::write(
            exe_dir.join(CONFIG_FILE_NAME),
            r#"{
              "paths": {
                "base": "game",
                "providers": "bin/providers",
                "assets": "data/assets",
                "cache": "var/cache"
              },
              "runtime": {
                "max_frames": 77,
                "skip_platform": true
              }
            }"#,
        )
        .unwrap();

        let config = ResolvedBootstrapConfig::load_for_executable(&exe).unwrap();

        assert!(config.config_loaded);
        assert_eq!(config.base_dir, exe_dir.join("game"));
        assert_eq!(config.provider_dir, exe_dir.join("game/bin/providers"));
        assert_eq!(config.assets_dir, exe_dir.join("game/data/assets"));
        assert_eq!(config.cache_dir, exe_dir.join("game/var/cache"));
        assert_eq!(config.max_frames, Some(77));
        assert!(config.skip_platform);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cli_set_overrides_adjacent_config() {
        let root = temp_dir("bootstrap-cli");
        let exe_dir = root.join("pack");
        fs::create_dir_all(&exe_dir).unwrap();
        let exe = exe_dir.join("newviso.exe");

        fs::write(
            exe_dir.join(CONFIG_FILE_NAME),
            r#"{
              "paths": {
                "base": ".",
                "providers": "runtime/providers",
                "cache": "cache"
              },
              "runtime": {
                "max_frames": 30
              }
            }"#,
        )
        .unwrap();

        let config = ResolvedBootstrapConfig::load_for_executable_and_args(
            &exe,
            [
                "--set",
                "paths.providers=alternate/providers",
                "--set=runtime.max_frames=144",
                "--cache-dir",
                "fast-cache",
                "--skip-platform",
                "--set",
                "providers.renderer=engine.render.test",
            ],
        )
        .unwrap();

        assert_eq!(
            config.provider_dir,
            exe_dir.join(".").join("alternate/providers")
        );
        assert_eq!(config.cache_dir, exe_dir.join(".").join("fast-cache"));
        assert_eq!(config.max_frames, Some(144));
        assert!(config.skip_platform);
        assert_eq!(config.renderer_provider, "engine.render.test");
        assert_eq!(config.cli_overrides.len(), 5);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unknown_cli_variable_is_rejected() {
        let root = temp_dir("bootstrap-cli-invalid");
        let exe = root.join("newviso.exe");

        let error = ResolvedBootstrapConfig::load_for_executable_and_args(
            &exe,
            ["--set", "paths.magic=somewhere"],
        )
        .unwrap_err();

        assert!(error.to_string().contains("unsupported bootstrap variable"));

        let _ = fs::remove_dir_all(root);
    }
}
