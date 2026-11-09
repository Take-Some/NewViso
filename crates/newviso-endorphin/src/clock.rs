/// Body observations are authoritative only after a new fixed physics step.
/// Render-only frames must neither lose contacts nor integrate recovery twice.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ObservationClock {
    last_tick: Option<u64>,
    observations: u64,
}
impl ObservationClock {
    pub fn sample(&mut self, tick: u64, fixed_dt: f32) -> Option<f32> {
        if tick == 0 || !fixed_dt.is_finite() || fixed_dt <= 0.0 {
            return None;
        }
        let ticks = match self.last_tick {
            Some(previous) if tick == previous => return None,
            Some(previous) if tick > previous => tick - previous,
            _ => 1,
        };
        self.last_tick = Some(tick);
        self.observations += 1;
        Some(ticks as f32 * fixed_dt)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_only_frames_do_not_produce_physical_observations() {
        let mut clock = ObservationClock::default();
        assert_eq!(clock.sample(0, 1.0 / 60.0), None);
        assert_eq!(clock.sample(1, 1.0 / 60.0), Some(1.0 / 60.0));
        for _ in 0..100 {
            assert_eq!(clock.sample(1, 1.0 / 60.0), None);
        }
        assert!((clock.sample(4, 1.0 / 60.0).unwrap() - 0.05).abs() < 1e-6);
        assert_eq!(clock.observations, 2);
        assert_eq!(clock.sample(2, 1.0 / 60.0), Some(1.0 / 60.0));
    }
    #[test]
    fn invalid_step_and_explicit_reset_do_not_reuse_old_elapsed_time() {
        let mut clock = ObservationClock::default();
        assert_eq!(clock.sample(1, f32::NAN), None);
        assert_eq!(clock.sample(1, 0.0), None);
        clock.sample(100, 1.0 / 60.0);
        clock.reset();
        assert_eq!(clock.sample(200, 1.0 / 60.0), Some(1.0 / 60.0));
    }
}
