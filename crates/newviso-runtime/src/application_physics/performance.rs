//! Opt-in CPU stage timings; the physics provider owns its native/GPU timing.
#![allow(clippy::struct_field_names)]
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct PhysicsFramePerf {
    pub fixed_steps: usize,
    pub endorphin_ms: f64,
    pub frame_build_ms: f64,
    pub provider_ms: f64,
    pub post_step_ms: f64,
    pub submitted_bodies: usize,
    pub submitted_joints: usize,
    pub returned_events: usize,
}

pub(crate) fn performance_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NEWVISO_PERF_TRACE").as_deref() == Ok("1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_performance_record_is_safe_without_profiling() {
        let record = PhysicsFramePerf::default();
        assert_eq!(record.fixed_steps, 0);
        assert_eq!(record.provider_ms, 0.0);
    }
}

impl super::PhysicsRuntime {
    pub(crate) fn frame_perf(&self) -> PhysicsFramePerf {
        self.frame_perf
    }

    pub(crate) fn simulated_body_count(&self) -> usize {
        self.bodies.len()
    }
}
