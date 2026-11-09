//! Physical velocity observations use the fixed clock; render frames do not
//! create fresh motion evidence between native physics steps.
#[derive(Clone, Debug)]
pub(super) struct BodyMotionSample {
    tick: u64,
    linear_velocity: [f32; 3],
    angular_velocity: [f32; 3],
    pub(super) acceleration: [f32; 3],
    pub(super) angular_acceleration: [f32; 3],
}
impl BodyMotionSample {
    pub(super) fn observe(
        previous: Option<&Self>,
        tick: u64,
        fixed_dt: f32,
        linear_velocity: [f32; 3],
        angular_velocity: [f32; 3],
        response_seconds: f32,
    ) -> Self {
        let mut sample = Self {
            tick,
            linear_velocity,
            angular_velocity,
            acceleration: [0.0; 3],
            angular_acceleration: [0.0; 3],
        };
        let Some(previous) = previous else {
            return sample;
        };
        if tick < previous.tick {
            return sample;
        }
        if tick == previous.tick {
            return previous.clone();
        }
        let dt = (tick - previous.tick) as f32 * fixed_dt;
        let weight = 1.0 - (-dt / response_seconds).exp();
        for i in 0..3 {
            sample.acceleration[i] = previous.acceleration[i]
                + weight
                    * ((linear_velocity[i] - previous.linear_velocity[i]) / dt
                        - previous.acceleration[i]);
            sample.angular_acceleration[i] = previous.angular_acceleration[i]
                + weight
                    * ((angular_velocity[i] - previous.angular_velocity[i]) / dt
                        - previous.angular_acceleration[i]);
        }
        sample
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extra_render_frames_cannot_inflate_or_erase_physical_acceleration() {
        let run = |render_subdivisions| {
            let mut sample =
                BodyMotionSample::observe(None, 0, 1.0 / 120.0, [0.0; 3], [0.0; 3], 0.08);
            for tick in 1..=120 {
                for _ in 1..render_subdivisions {
                    let repeated = BodyMotionSample::observe(
                        Some(&sample),
                        tick - 1,
                        1.0 / 120.0,
                        sample.linear_velocity,
                        sample.angular_velocity,
                        0.08,
                    );
                    assert_eq!(sample.acceleration, repeated.acceleration);
                    assert_eq!(sample.angular_acceleration, repeated.angular_acceleration);
                    sample = repeated;
                }
                let time = tick as f32 / 120.0;
                sample = BodyMotionSample::observe(
                    Some(&sample),
                    tick,
                    1.0 / 120.0,
                    [3.0 * time, 0.0, 0.0],
                    [0.0, -4.0 * time, 0.0],
                    0.08,
                );
            }
            sample
        };
        let a = run(1);
        for divisions in [2, 7] {
            let b = run(divisions);
            assert_eq!(a.acceleration, b.acceleration);
            assert_eq!(a.angular_acceleration, b.angular_acceleration);
        }
    }
    #[test]
    fn skipped_fixed_observations_use_the_whole_physical_interval() {
        let mut fine = BodyMotionSample::observe(None, 0, 1.0 / 120.0, [0.0; 3], [0.0; 3], 0.08);
        let mut coarse = fine.clone();
        for tick in 1..=120 {
            let time = tick as f32 / 120.0;
            fine = BodyMotionSample::observe(
                Some(&fine),
                tick,
                1.0 / 120.0,
                [3.0 * time, 0.0, 0.0],
                [0.0, -4.0 * time, 0.0],
                0.08,
            );
            if tick % 4 == 0 {
                coarse = BodyMotionSample::observe(
                    Some(&coarse),
                    tick,
                    1.0 / 120.0,
                    [3.0 * time, 0.0, 0.0],
                    [0.0, -4.0 * time, 0.0],
                    0.08,
                );
            }
        }
        assert!((fine.acceleration[0] - coarse.acceleration[0]).abs() < 1e-5);
        assert!((fine.angular_acceleration[1] - coarse.angular_acceleration[1]).abs() < 1e-5);
        assert!((fine.acceleration[0] - 3.0 * (1.0 - (-1.0f32 / 0.08).exp())).abs() < 1e-5);
    }
    #[test]
    fn replacing_the_physics_clock_does_not_turn_old_velocity_into_an_impulse() {
        let before = BodyMotionSample::observe(
            None,
            100,
            1.0 / 120.0,
            [8.0, 0.0, 0.0],
            [0.0, 6.0, 0.0],
            0.08,
        );
        let reset =
            BodyMotionSample::observe(Some(&before), 0, 1.0 / 120.0, [0.0; 3], [0.0; 3], 0.08);
        assert_eq!(reset.acceleration, [0.0; 3]);
        assert_eq!(reset.angular_acceleration, [0.0; 3]);
    }
}
