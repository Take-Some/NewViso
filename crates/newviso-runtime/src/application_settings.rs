use super::*;

impl EngineApplication {
    pub(super) fn configure_runtime(&mut self, patch: &Value) -> Result<(), String> {
        // Decode and validate every field before changing any live subsystem.
        let next = self
            .settings
            .patched(patch)
            .map_err(|error| error.to_string())?;
        let policy = streaming_policy_from_project(&next.streaming);
        policy.validate()?;
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.configure_event_queue(next.scripting.event_queue_capacity)?;
        }
        if patch.get("camera").is_some() {
            self.scene.configure_orbit_controls(
                next.camera.rotate_button,
                next.camera.min_pitch_degrees,
                next.camera.max_pitch_degrees,
            )?;
            self.scene.configure_orbit(
                next.camera.rotate_sensitivity,
                next.camera.zoom_sensitivity,
                next.camera.min_distance,
                next.camera.max_distance,
            )?;
        }
        self.asset_streamer.set_policy(policy)?;
        self.settings = next;
        self.project_context["runtime"] = serde_json::to_value(&self.settings)
            .map_err(|error| format!("runtime settings encode failed: {error}"))?;
        host::publish_event_json(
            "runtime.settings.changed",
            "newviso.runtime",
            json!({
                "settings": self.settings
            }),
        )?;
        Ok(())
    }
}
