//! Finite ground traction for native character drives.
#[derive(Clone, Copy, Debug)]
pub struct GroundDrive {
    pub entity: u64,
    tick: u64,
    pub velocity: [f32; 2],
}
impl GroundDrive {
    pub fn observe(
        previous: Option<&Self>,
        entity: u64,
        tick: u64,
        fixed_dt: f32,
        current: [f32; 2],
        desired: [f32; 2],
        acceleration: f32,
    ) -> Self {
        let previous = previous.filter(|p| p.entity == entity && p.tick <= tick);
        if let Some(p) = previous.filter(|p| p.tick == tick) {
            return *p;
        }
        let ticks = previous.map_or(1, |p| tick - p.tick);
        let change = acceleration.max(0.0) * ticks as f32 * fixed_dt;
        // Native contact friction has already reduced the observed velocity.
        // Recover only the bounded drag along the previous drive direction;
        // collision losses and external impulses remain physical observations.
        let mut base = current;
        if let Some(previous) = previous {
            let speed = previous.velocity[0].hypot(previous.velocity[1]);
            if speed > f32::EPSILON {
                let axis = previous.velocity.map(|v| v / speed);
                let lost = ((previous.velocity[0] - current[0]) * axis[0]
                    + (previous.velocity[1] - current[1]) * axis[1])
                    .clamp(0.0, change);
                for i in 0..2 {
                    base[i] += axis[i] * lost;
                }
            }
        }
        let delta = [desired[0] - base[0], desired[1] - base[1]];
        let length = delta[0].hypot(delta[1]);
        let fraction = if length > 0.0 {
            (change / length).min(1.0)
        } else {
            1.0
        };
        Self {
            entity,
            tick,
            velocity: std::array::from_fn(|i| base[i] + delta[i] * fraction),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn braking_has_finite_traction_and_conserves_stopping_distance() {
        let dt = 1.0 / 120.0;
        let acceleration = 0.5 * 9.81;
        let mut velocity = [2.2, 0.0];
        let mut drive = None;
        let mut distance = 0.0;
        for tick in 0..120 {
            let next = GroundDrive::observe(
                drive.as_ref(),
                7,
                tick,
                dt,
                velocity,
                [0.0; 2],
                acceleration,
            );
            assert!((next.velocity[0] - velocity[0]).abs() <= acceleration * dt + 1e-6);
            assert!(next.velocity[0] >= 0.0);
            distance += (velocity[0] + next.velocity[0]) * 0.5 * dt;
            velocity = next.velocity;
            drive = Some(next);
        }
        assert_eq!(velocity, [0.0; 2]);
        assert!((distance - 2.2f32.powi(2) / (2.0 * acceleration)).abs() < 2.2 * dt);
    }
    #[test]
    fn extra_render_frames_cannot_multiply_ground_acceleration() {
        let first = GroundDrive::observe(None, 7, 30, 1.0 / 120.0, [0.0; 2], [2.0, 0.0], 4.0);
        let mut repeated = first;
        for _ in 0..12 {
            repeated = GroundDrive::observe(
                Some(&repeated),
                7,
                30,
                1.0 / 120.0,
                repeated.velocity,
                [2.0, 0.0],
                4.0,
            );
        }
        assert_eq!(first.velocity, repeated.velocity);
        let next = GroundDrive::observe(
            Some(&repeated),
            7,
            31,
            1.0 / 120.0,
            first.velocity,
            [2.0, 0.0],
            4.0,
        );
        assert!((next.velocity[0] - 2.0 * 4.0 / 120.0).abs() < 1e-6);
    }
    #[test]
    fn zero_friction_preserves_momentum_and_a_new_body_has_a_fresh_clock() {
        let old = GroundDrive::observe(None, 7, 100, 1.0 / 120.0, [1.0, 2.0], [0.0; 2], 0.0);
        assert_eq!(old.velocity, [1.0, 2.0]);
        let new = GroundDrive::observe(Some(&old), 8, 10, 1.0 / 120.0, [0.0; 2], [2.0, 0.0], 4.0);
        assert!((new.velocity[0] - 4.0 / 120.0).abs() < 1e-6);
    }
    #[test]
    fn native_contact_drag_does_not_cancel_drive_or_store_a_wall_launch() {
        let dt = 1.0 / 120.0;
        let friction_acceleration = 0.5 * 9.81;
        let mut drive = None;
        let mut velocity = [0.0; 2];
        for tick in 0..120 {
            let next = GroundDrive::observe(
                drive.as_ref(),
                7,
                tick,
                dt,
                velocity,
                [2.0, 0.0],
                friction_acceleration,
            );
            // A blocked native body has no forward velocity. Its command
            // cannot accumulate an unbounded take-off speed behind the wall.
            assert!(next.velocity[0] <= 2.0 * friction_acceleration * dt + 1e-5);
            drive = Some(next);
        }
        for tick in 120..240 {
            let next = GroundDrive::observe(
                drive.as_ref(),
                7,
                tick,
                dt,
                velocity,
                [2.0, 0.0],
                friction_acceleration,
            );
            let actual = (next.velocity[0] - friction_acceleration * dt).max(0.0);
            assert!(actual - velocity[0] <= friction_acceleration * dt + 1e-5);
            velocity = [actual, 0.0];
            drive = Some(next);
        }
        assert!(
            velocity[0] > 1.9,
            "walking stalled against finite contact drag: {velocity:?}"
        );
    }
}
