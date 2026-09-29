use super::{ProjectError, ProjectWorldPersistence, RUNTIME_SETTINGS_SCHEMA_V1};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

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
    /// Maximum new model instances committed per frame.
    /// CPU-heavy model/collision preparation is asynchronous; owner-thread commits
    /// are additionally protected by the runtime frame-time slice.
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
    pub world_snapshot_interval_seconds: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSchedulingSettings {
    pub max_physics_frame_seconds: f32,
    pub scene_focus_observer: bool,
    pub native_navigation_enabled: bool,
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
    /// Decode a complete runtime document. Engine startup normally uses
    /// from_base_and_override(), because Shared Assets own the complete base.
    pub fn from_value(value: Value) -> Result<Self, ProjectError> {
        if value.get("schema").and_then(Value::as_str) != Some(RUNTIME_SETTINGS_SCHEMA_V1) {
            return Err(ProjectError::Asset(
                "unsupported or missing runtime settings schema".into(),
            ));
        }
        Self::decode(value)
    }

    /// Build effective runtime configuration from authoritative Shared Assets
    /// plus a project-owned partial override.
    pub fn from_base_and_override(base: Value, project: Value) -> Result<Self, ProjectError> {
        if base.get("schema").and_then(Value::as_str) != Some(RUNTIME_SETTINGS_SCHEMA_V1)
            || project.get("schema").and_then(Value::as_str) != Some(RUNTIME_SETTINGS_SCHEMA_V1)
        {
            return Err(ProjectError::Asset(
                "unsupported or missing runtime settings schema".into(),
            ));
        }
        let mut merged = base;
        merge(&mut merged, &project);
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
        if !self.scripting.world_snapshot_interval_seconds.is_finite()
            || !(0.0..=5.0).contains(&self.scripting.world_snapshot_interval_seconds)
        {
            return Err(invalid(
                "scripting world_snapshot_interval_seconds must be finite and in 0..=5",
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

    fn complete_base() -> Value {
        json!({
            "schema": RUNTIME_SETTINGS_SCHEMA_V1,
            "window": {"title": null, "width": 1280, "height": 720},
            "camera": {
                "rotate_sensitivity": 0.005,
                "zoom_sensitivity": 0.0015,
                "min_distance": 2.0,
                "max_distance": 40.0,
                "rotate_button": 1,
                "min_pitch_degrees": -85.0,
                "max_pitch_degrees": 85.0
            },
            "streaming": {
                "max_resident_mb": 512,
                "max_loads_per_tick": 8,
                "parallel_loads": 4,
                "max_model_materializations_per_frame": 32,
                "max_source_mb_per_tick": 32,
                "eviction_grace_frames": 120,
                "failed_retry_frames": 120,
                "dependency_priority_scale": 0.95
            },
            "scripting": {
                "event_queue_capacity": 4096,
                "world_snapshot_interval_seconds": 0.2
            },
            "scheduling": {
                "max_physics_frame_seconds": 0.05,
                "scene_focus_observer": true,
                "native_navigation_enabled": true
            },
            "world_persistence": {
                "save_path": null,
                "load_on_start": true,
                "save_on_shutdown": true,
                "autosave_interval_seconds": 0.0
            },
            "variables": {},
            "startup_commands": []
        })
    }

    #[test]
    fn shared_base_and_project_patch_preserve_current_values() {
        let mut settings = ProjectRuntimeSettings::from_base_and_override(
            complete_base(),
            json!({
                "schema": RUNTIME_SETTINGS_SCHEMA_V1,
                "camera": {"max_distance": 900.0},
                "variables": {"weather": {"wind": 12}}
            }),
        )
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
        let settings = ProjectRuntimeSettings::from_value(complete_base()).unwrap();
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
    fn incomplete_invalid_or_unknown_launch_settings_fail() {
        for value in [
            json!({}),
            json!({"schema": RUNTIME_SETTINGS_SCHEMA_V1, "straming": {}}),
            json!({"schema": RUNTIME_SETTINGS_SCHEMA_V1, "startup_commands": [{}]}),
        ] {
            assert!(ProjectRuntimeSettings::from_value(value).is_err());
        }
    }
}
