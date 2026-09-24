use super::*;
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub(super) struct CliOverrides {
    pub(super) config_path: Option<PathBuf>,
    pub(super) project_path: Option<PathBuf>,
    pub(super) values: HashMap<String, String>,
    pub(super) applied: Vec<String>,
}

impl CliOverrides {
    pub(super) fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    pub(super) fn path(&self, key: &str) -> Option<PathBuf> {
        self.get(key).map(PathBuf::from)
    }

    pub(super) fn bool(&self, key: &str) -> Result<Option<bool>, BootstrapConfigError> {
        let Some(value) = self.get(key) else {
            return Ok(None);
        };
        parse_bool(value).map(Some).ok_or_else(|| {
            BootstrapConfigError::Cli(format!("'{key}' expects a boolean, got '{value}'"))
        })
    }

    pub(super) fn u64(&self, key: &str) -> Result<Option<u64>, BootstrapConfigError> {
        let Some(value) = self.get(key) else {
            return Ok(None);
        };
        value.parse::<u64>().map(Some).map_err(|_| {
            BootstrapConfigError::Cli(format!(
                "'{key}' expects an unsigned integer, got '{value}'"
            ))
        })
    }
}

pub(super) fn parse_cli<I, S>(args: I) -> Result<CliOverrides, BootstrapConfigError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into).peekable();
    let mut out = CliOverrides::default();

    while let Some(raw) = args.next() {
        let arg = raw.to_string_lossy().into_owned();

        if let Some(value) = arg.strip_prefix("--project=") {
            out.project_path = Some(PathBuf::from(value));
            out.applied.push(format!("project={value}"));
            continue;
        }
        if arg == "--project" {
            let value = next_arg(&mut args, "--project")?;
            out.project_path = Some(PathBuf::from(&value));
            out.applied.push(format!("project={value}"));
            continue;
        }

        if let Some(value) = arg.strip_prefix("--config=") {
            out.config_path = Some(PathBuf::from(value));
            out.applied.push(format!("config={value}"));
            continue;
        }
        if arg == "--config" {
            let value = next_arg(&mut args, "--config")?;
            out.config_path = Some(PathBuf::from(&value));
            out.applied.push(format!("config={value}"));
            continue;
        }

        if let Some(value) = arg.strip_prefix("--set=") {
            apply_set(&mut out, value)?;
            continue;
        }
        if arg == "--set" {
            let value = next_arg(&mut args, "--set")?;
            apply_set(&mut out, &value)?;
            continue;
        }

        let alias = match arg.as_str() {
            "--base-dir" => Some("paths.base"),
            "--provider-dir" => Some("paths.providers"),
            "--assets-dir" => Some("paths.assets"),
            "--content-dir" => Some("paths.content"),
            "--cache-dir" => Some("paths.cache"),
            "--codecs-dir" => Some("paths.codecs"),
            "--max-frames" => Some("runtime.max_frames"),
            "--logging-provider" => Some("providers.logging"),
            "--input-provider" => Some("providers.input"),
            "--assets-provider" => Some("providers.assets"),
            "--ecs-provider" => Some("providers.ecs"),
            "--platform-provider" => Some("providers.platform"),
            "--renderer-provider" => Some("providers.renderer"),
            _ => None,
        };

        if let Some(key) = alias {
            let value = next_arg(&mut args, &arg)?;
            set_override(&mut out, key, value)?;
            continue;
        }

        if arg == "--skip-platform" {
            set_override(&mut out, "runtime.skip_platform", "true".to_owned())?;
            continue;
        }
        if arg == "--run-platform" {
            set_override(&mut out, "runtime.skip_platform", "false".to_owned())?;
            continue;
        }

        return Err(BootstrapConfigError::Cli(format!(
            "unknown argument '{arg}'. Use --set key=value for bootstrap overrides"
        )));
    }

    Ok(out)
}

fn next_arg<I>(
    args: &mut std::iter::Peekable<I>,
    flag: &str,
) -> Result<String, BootstrapConfigError>
where
    I: Iterator<Item = OsString>,
{
    args.next()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BootstrapConfigError::Cli(format!("'{flag}' requires a value")))
}

fn apply_set(out: &mut CliOverrides, assignment: &str) -> Result<(), BootstrapConfigError> {
    let Some((key, value)) = assignment.split_once('=') else {
        return Err(BootstrapConfigError::Cli(format!(
            "'--set' expects key=value, got '{assignment}'"
        )));
    };
    set_override(out, key.trim(), value.to_owned())
}

fn set_override(
    out: &mut CliOverrides,
    key: &str,
    value: String,
) -> Result<(), BootstrapConfigError> {
    const SUPPORTED: &[&str] = &[
        "paths.base",
        "paths.providers",
        "paths.assets",
        "paths.content",
        "paths.cache",
        "paths.codecs",
        "runtime.max_frames",
        "runtime.skip_platform",
        "providers.logging",
        "providers.input",
        "providers.assets",
        "providers.ecs",
        "providers.platform",
        "providers.renderer",
    ];

    if !SUPPORTED.contains(&key) {
        return Err(BootstrapConfigError::Cli(format!(
            "unsupported bootstrap variable '{key}'"
        )));
    }

    out.values.insert(key.to_owned(), value.clone());
    out.applied.push(format!("{key}={value}"));
    Ok(())
}
