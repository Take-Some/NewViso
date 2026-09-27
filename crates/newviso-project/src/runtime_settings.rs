use super::{ProjectError, ProjectWorldPersistence, RUNTIME_SETTINGS_SCHEMA_V1};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{Map, Value};

const DEFAULTS: &str = include_str!("assets/runtime_defaults.json");

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRuntimeSettings {
    pub schema: String,
    pub window: ProjectWindowSettings,
    pub camera: ProjectCameraSettings,
    pub streaming: ProjectStreamingSettings,
    pub scripting: ProjectScriptingSettings,
    pub scheduling: ProjectSchedulingSettings,
    pub world_persistence: ProjectWorldPersistence,
    /// Project-owned data, available to every script lifecycle invocation.
    pub variables: Map<String, Value>,
    /// Capability commands applied once, before script on_start.
    pub startup_commands: Vec<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectWindowSettings {
    pub title: Option<String>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectCameraSettings {
    pub rotate_sensitivity: f32,
    pub zoom_sensitivity: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub rotate_button: u64,
    pub min_pitch_degrees: f32,
    pub max_pitch_degrees: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectStreamingSettings {
    /// MiB; zero means unlimited for residency and source-byte budgets.
    pub max_resident_mb: u64,
    /// Zero means unlimited.
    pub max_loads_per_tick: usize,
    pub parallel_loads: usize,
    /// Maximum new model instances uploaded/materialized per frame.
    /// Zero means unlimited; collision meshes are never delayed by this budget.
    pub max_model_materializations_per_frame: usize,
    pub max_source_mb_per_tick: u64,
    pub eviction_grace_frames: u64,
    pub failed_retry_frames: u64,
    pub dependency_priority_scale: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectScriptingSettings {
    pub event_queue_capacity: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSchedulingSettings {
    pub max_physics_frame_seconds: f32,
    pub scene_focus_observer: bool,
    pub native_navigation_enabled: bool,
}

fn defaults() -> Value {
    serde_json::from_str(DEFAULTS).expect("packaged runtime defaults must be valid JSON")
}

fn default_section<T: DeserializeOwned>(key: &str) -> T {
    serde_json::from_value(defaults()[key].clone())
        .expect("packaged runtime defaults must match their schema")
}

macro_rules! section_default {
    ($kind:ty, $key:literal) => {
        impl Default for $kind {
            fn default() -> Self {
                default_section($key)
            }
        }
    };
}
section_default!(ProjectWindowSettings, "window");
section_default!(ProjectCameraSettings, "camera");
section_default!(ProjectStreamingSettings, "streaming");
section_default!(ProjectScriptingSettings, "scripting");
section_default!(ProjectSchedulingSettings, "scheduling");

impl Default for ProjectRuntimeSettings {
    fn default() -> Self {
        serde_json::from_value(defaults()).expect("packaged runtime defaults must match schema")
    }
}

fn merge(target: &mut Value, patch: &Value) {
    if let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            match target.get_mut(key) {
                Some(current) => merge(current, value),
                None => {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
    } else {
        *target = patch.clone();
    }
}

impl ProjectRuntimeSettings {
    pub fn from_value(value: Value) -> Result<Self, ProjectError> {
        // Require the document's schema, even when all other fields use defaults.
        if value.get("schema").and_then(Value::as_str) != Some(RUNTIME_SETTINGS_SCHEMA_V1) {
            return Err(ProjectError::Asset(
                "unsupported or missing runtime settings schema".into(),
            ));
        }
        let mut merged = defaults();
        merge(&mut merged, &value);
        Self::decode(merged)
    }

    fn decode(value: Value) -> Result<Self, ProjectError> {
        let settings: Self = serde_json::from_value(value)
            .map_err(|e| ProjectError::Asset(format!("runtime settings decode failed: {e}")))?;
        settings.validate()?;
        Ok(settings)
    }

    /// Merge a live patch into current state, never back into launch defaults.
    /// Cold settings are rejected explicitly; a typo cannot silently do nothing.
    pub fn patched(&self, patch: &Value) -> Result<Self, ProjectError> {
        let object = patch.as_object().ok_or_else(|| {
            ProjectError::Asset("runtime.configure settings must be an object".into())
        })?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "camera" | "streaming" | "scripting" | "scheduling" | "variables"
            ) {
                return Err(ProjectError::Asset(format!(
                    "runtime setting '{key}' is unknown or requires restart"
                )));
            }
        }
        let mut next =
            serde_json::to_value(self).map_err(|e| ProjectError::Asset(e.to_string()))?;
        merge(&mut next, patch);
        Self::decode(next)
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        let invalid = |message: &str| ProjectError::Asset(message.to_owned());
        if self.window.width == 0 || self.window.height == 0 {
            return Err(invalid("window dimensions must be greater than zero"));
        }
        let c = &self.camera;
        if [
            c.rotate_sensitivity,
            c.zoom_sensitivity,
            c.min_distance,
            c.max_distance,
        ]
        .iter()
        .any(|x| !x.is_finite() || *x <= 0.0)
            || c.max_distance < c.min_distance
            || !c.min_pitch_degrees.is_finite()
            || !c.max_pitch_degrees.is_finite()
            || c.min_pitch_degrees <= -90.0
            || c.max_pitch_degrees >= 90.0
            || c.min_pitch_degrees >= c.max_pitch_degrees
        {
            return Err(invalid(
                "camera sensitivities, distance or pitch limits are invalid",
            ));
        }
        if self.streaming.parallel_loads == 0 || self.streaming.parallel_loads > 32 {
            return Err(invalid("streaming parallel_loads must be in 1..=32"));
        }
        let scale = self.streaming.dependency_priority_scale;
        if !scale.is_finite() || !(0.0..=1.0).contains(&scale) {
            return Err(invalid(
                "streaming dependency_priority_scale must be finite and in 0..=1",
            ));
        }
        for mb in [
            self.streaming.max_resident_mb,
            self.streaming.max_source_mb_per_tick,
        ] {
            if mb.checked_mul(1024 * 1024).is_none() {
                return Err(invalid("streaming byte budget overflows u64"));
            }
        }
        if self.scripting.event_queue_capacity == 0 {
            return Err(invalid(
                "scripting event_queue_capacity must be greater than zero",
            ));
        }
        let dt = self.scheduling.max_physics_frame_seconds;
        if !dt.is_finite() || dt <= 0.0 {
            return Err(invalid(
                "max_physics_frame_seconds must be finite and greater than zero",
            ));
        }
        self.world_persistence
            .validate()
            .map_err(ProjectError::Asset)?;
        for command in &self.startup_commands {
            if command
                .get("op")
                .and_then(Value::as_str)
                .is_none_or(|op| op.trim().is_empty())
            {
                return Err(invalid(
                    "startup_commands entries require a non-empty string op",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn partial_launch_and_live_patch_preserve_current_values() {
        let mut settings = ProjectRuntimeSettings::from_value(json!({
            "schema": RUNTIME_SETTINGS_SCHEMA_V1,
            "camera": {"max_distance": 900.0}, "variables": {"weather": {"wind": 12}}
        }))
        .unwrap();
        settings = settings
            .patched(&json!({"streaming": {"max_loads_per_tick": 3}}))
            .unwrap();
        assert_eq!(settings.camera.max_distance, 900.0);
        assert_eq!(settings.streaming.max_loads_per_tick, 3);
        assert_eq!(settings.variables["weather"]["wind"], 12);
        settings.validate().unwrap();
    }

    #[test]
    fn rejects_typos_invalid_ranges_and_cold_live_changes() {
        let settings = ProjectRuntimeSettings::default();
        for patch in [
            json!({"camera": {"max_distnce": 2}}),
            json!({"camera": {"min_distance": 1000}}),
            json!({"camera": {"min_pitch_degrees": -90}}),
            json!({"scripting": {"event_queue_capacity": 0}}),
            json!({"streaming": {"dependency_priority_scale": 1.1}}),
            json!({"scheduling": {"max_physics_frame_seconds": 0}}),
            json!({"window": {"width": 1920}}),
            json!(null),
        ] {
            assert!(settings.patched(&patch).is_err(), "accepted {patch}");
        }
        assert_eq!(settings.camera.max_distance, 40.0);
    }

    #[test]
    fn invalid_or_unknown_launch_settings_fail() {
        for value in [
            json!({}),
            json!({"schema": RUNTIME_SETTINGS_SCHEMA_V1, "straming": {}}),
            json!({"schema": RUNTIME_SETTINGS_SCHEMA_V1, "startup_commands": [{}]}),
        ] {
            assert!(ProjectRuntimeSettings::from_value(value).is_err());
        }
    }
}
