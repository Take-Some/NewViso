//! Opt-in frame-stage trace. Normal frame logging stays unchanged.
//! The render wall-time includes host/provider waits; it is NOT a GPU timer.

use super::*;

pub(super) fn trace_interval() -> Option<u64> {
    static INTERVAL: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *INTERVAL.get_or_init(|| {
        if !application_physics::performance_trace_enabled() {
            return None;
        }
        let interval = std::env::var("NEWVISO_PERF_TRACE_INTERVAL")
            .ok()
            .and_then(|text| text.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(30)
            .clamp(1, 100_000);
        Some(interval)
    })
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FrameTrace {
    pub frame: u64,
    pub total_ms: f64,
    pub dt_ms: f64,
    pub pre_physics_ms: f64,
    pub cold_ms: f64,
    pub physics_ms: f64,
    pub scripts_ms: f64,
    pub world_native_ms: f64,
    pub presentations_ms: f64,
    pub script_state_ms: f64,
    pub quickjs_ms: f64,
    pub script_commands_ms: f64,
    pub scene_tick_ms: f64,
    pub streaming_ms: f64,
    pub interest_ms: f64,
    pub pump_ms: f64,
    pub publish_ms: f64,
    pub scene_render_ms: f64,
    pub provider_ms: f64,
    pub active_ragdolls: usize,
    pub simulated_bodies: usize,
    pub queued_assets: usize,
    pub loading_assets: usize,
    pub resident_assets: usize,
    pub resident_bytes: u64,
    pub failed_assets: usize,
    pub over_asset_budget: bool,
    pub scene_prepare_queued: usize,
    pub physics: Option<application_physics::PhysicsFramePerf>,
}

impl FrameTrace {
    pub fn emit(self) {
        let p = self.physics.unwrap_or_default();
        host::info("newviso.perf.trace", format!(
            "frame={} dt_ms={:.4} total_ms={:.4} pre_physics_ms={:.4} cold_ms={:.4} physics_ms={:.4} endorphin_ms={:.4} physics_build_ms={:.4} physics_provider_ms={:.4} physics_post_ms={:.4} physics_steps={} bodies={} joints={} physics_events={} active_ragdolls={} scene_bodies={} scripts_ms={:.4} world_native_ms={:.4} presentations_ms={:.4} script_state_ms={:.4} quickjs_ms={:.4} script_commands_ms={:.4} scene_tick_ms={:.4} streaming_ms={:.4} interest_ms={:.4} pump_ms={:.4} publish_ms={:.4} scene_render_wall_ms={:.4} render_provider_wall_ms={:.4} queued_assets={} loading_assets={} resident_assets={} resident_bytes={} failed_assets={} over_asset_budget={} scene_prepare_queued={}",
            self.frame, self.dt_ms, self.total_ms, self.pre_physics_ms, self.cold_ms, self.physics_ms,
            p.endorphin_ms, p.frame_build_ms, p.provider_ms, p.post_step_ms,
            p.fixed_steps, p.submitted_bodies, p.submitted_joints, p.returned_events,
            self.active_ragdolls, self.simulated_bodies,
            self.scripts_ms, self.world_native_ms, self.presentations_ms,
            self.script_state_ms, self.quickjs_ms, self.script_commands_ms,
            self.scene_tick_ms, self.streaming_ms, self.interest_ms, self.pump_ms,
            self.publish_ms, self.scene_render_ms, self.provider_ms,
            self.queued_assets, self.loading_assets, self.resident_assets,
            self.resident_bytes, self.failed_assets, self.over_asset_budget,
            self.scene_prepare_queued
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_metrics_are_defined_for_no_physics_project() {
        let perf = application_physics::PhysicsFramePerf::default();
        assert_eq!(perf.fixed_steps, 0);
        assert_eq!(perf.endorphin_ms, 0.0);
        assert_eq!(perf.provider_ms, 0.0);
    }
}
