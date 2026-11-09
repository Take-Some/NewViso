use crate::{validation::validate_logical_asset_path, ProjectError};
use newviso_config::{ConfigurationStack, SettingOverride};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectConfiguration {
    pub defaults: Vec<String>,
    pub platforms: BTreeMap<String, Vec<String>>,
    pub profiles: BTreeMap<String, RuntimeProfile>,
    pub default_profile: Option<String>,
    /// Explicit opt-in; a declared user layer must exist and be valid.
    pub user_settings: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeProfile {
    pub inherits: Vec<String>,
    pub layers: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeConfigurationRequest {
    pub platform: String,
    pub profile: Option<String>,
    pub layers: Vec<String>,
    pub overrides: Vec<SettingOverride>,
}

pub(crate) fn invalid(message: impl Into<String>) -> ProjectError {
    ProjectError::Invalid(message.into())
}

impl ProjectConfiguration {
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.profiles.len() > 128 {
            return Err(invalid("configuration supports at most 128 profiles"));
        }
        for (name, profile) in &self.profiles {
            if name.is_empty()
                || name.len() > 128
                || !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
            {
                return Err(invalid(format!(
                    "invalid configuration profile name '{name}'"
                )));
            }
            for path in &profile.layers {
                validate_logical_asset_path("configuration.profiles.layers", path)?;
            }
        }
        for path in self
            .defaults
            .iter()
            .chain(self.platforms.values().flatten())
            .chain(self.user_settings.iter())
        {
            validate_logical_asset_path("configuration layer", path)?;
        }
        if let Some(name) = &self.default_profile {
            if !self.profiles.contains_key(name) {
                return Err(invalid(format!("unknown default profile '{name}'")));
            }
        }
        for name in self.profiles.keys() {
            self.profile_order(name)?;
        }
        Ok(())
    }

    pub fn profile_order(&self, name: &str) -> Result<Vec<String>, ProjectError> {
        fn visit(
            config: &ProjectConfiguration,
            name: &str,
            active: &mut Vec<String>,
            done: &mut BTreeSet<String>,
            order: &mut Vec<String>,
        ) -> Result<(), ProjectError> {
            if done.contains(name) {
                return Ok(());
            }
            if active.iter().any(|s| s == name) {
                return Err(invalid(format!(
                    "configuration profile cycle: {} -> {name}",
                    active.join(" -> ")
                )));
            }
            if active.len() >= 32 {
                return Err(invalid(
                    "configuration profile inheritance exceeds 32 levels",
                ));
            }
            let profile = config
                .profiles
                .get(name)
                .ok_or_else(|| invalid(format!("unknown configuration profile '{name}'")))?;
            active.push(name.to_owned());
            for parent in &profile.inherits {
                visit(config, parent, active, done, order)?;
            }
            active.pop();
            done.insert(name.to_owned());
            order.push(name.to_owned());
            Ok(())
        }
        let mut order = Vec::new();
        visit(
            self,
            name,
            &mut Vec::new(),
            &mut BTreeSet::new(),
            &mut order,
        )?;
        Ok(order)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeConfigurationReport {
    pub schema: String,
    pub platform: String,
    pub profile: Option<String>,
    pub available_profiles: Vec<String>,
    pub layers: Vec<newviso_config::ConfigurationLayer>,
    pub field_sources: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct ResolvedRuntimeConfiguration {
    pub settings: crate::ProjectRuntimeSettings,
    pub report: RuntimeConfigurationReport,
}

impl ResolvedRuntimeConfiguration {
    /// The reader is supplied by the host: live launches use AssetClient/VFS;
    /// offline authoring tools can use their filesystem mount adapter.
    pub fn resolve(
        config: &ProjectConfiguration,
        runtime_asset: &str,
        shared_base: serde_json::Value,
        shared_source: &str,
        request: &RuntimeConfigurationRequest,
        mut read: impl FnMut(&str) -> Result<serde_json::Value, String>,
    ) -> Result<Self, ProjectError> {
        config.validate()?;
        let mut stack = ConfigurationStack::default();
        crate::ProjectRuntimeSettings::from_value(shared_base.clone())?;
        stack.apply(shared_source, "engine", &shared_base);
        let mut plan = Vec::<(String, String)>::new();
        for path in &config.defaults {
            plan.push((path.clone(), "defaults".into()));
        }
        plan.push((runtime_asset.to_owned(), "project".into()));
        if let Some(paths) = config.platforms.get(&request.platform) {
            for path in paths {
                plan.push((path.clone(), format!("platform:{}", request.platform)));
            }
        }
        let profile = request
            .profile
            .as_ref()
            .or(config.default_profile.as_ref())
            .cloned();
        if let Some(name) = profile.as_deref() {
            for parent in config.profile_order(name)? {
                for path in &config.profiles[&parent].layers {
                    plan.push((path.clone(), format!("profile:{parent}")));
                }
            }
        }
        if let Some(path) = &config.user_settings {
            plan.push((path.clone(), "user".into()));
        }
        for path in &request.layers {
            plan.push((path.clone(), "command_line_layer".into()));
        }
        if plan.len() + request.overrides.len() > 256 {
            return Err(invalid("configuration exceeds 256 layers/overrides"));
        }
        for (path, kind) in plan {
            validate_logical_asset_path("runtime configuration layer", &path)?;
            let value = read(&path).map_err(|error| {
                invalid(format!("configuration layer '{path}' ({kind}): {error}"))
            })?;
            if value.get("schema").and_then(serde_json::Value::as_str)
                != Some(crate::RUNTIME_SETTINGS_SCHEMA_V1)
            {
                return Err(invalid(format!(
                    "configuration layer '{path}' requires schema '{}'",
                    crate::RUNTIME_SETTINGS_SCHEMA_V1
                )));
            }
            stack.apply(&path, &kind, &value);
        }
        for (index, setting) in request.overrides.iter().enumerate() {
            let parts = newviso_config::setting_path(&setting.path).map_err(invalid)?;
            let mut current = Some(&stack.value);
            for part in &parts {
                if current.is_some_and(serde_json::Value::is_array) {
                    return Err(invalid(format!(
                        "runtime override '{}' traverses an array; replace the complete array",
                        setting.path
                    )));
                }
                current = current.and_then(|value| value.get(part));
            }
            let patch = setting.patch().map_err(invalid)?;
            if patch.get("schema").is_some() {
                return Err(invalid("schema cannot be changed by a runtime override"));
            }
            stack.apply(
                &format!("command_line:{index}:{}", setting.path),
                "command_line_override",
                &patch,
            );
        }
        let settings = crate::ProjectRuntimeSettings::from_value(stack.value).map_err(|e| {
            invalid(format!(
                "effective runtime configuration rejected (profile={profile:?}; layers={}): {e}",
                stack
                    .layers
                    .iter()
                    .map(|l| l.source.as_str())
                    .collect::<Vec<_>>()
                    .join(" -> ")
            ))
        })?;
        Ok(Self {
            settings,
            report: RuntimeConfigurationReport {
                schema: "newviso.runtime.configuration.v1".into(),
                platform: request.platform.clone(),
                profile,
                available_profiles: config.profiles.keys().cloned().collect(),
                layers: stack.layers,
                field_sources: stack.field_sources,
            },
        })
    }
}

#[cfg(test)]
#[path = "configuration_tests.rs"]
mod tests;
