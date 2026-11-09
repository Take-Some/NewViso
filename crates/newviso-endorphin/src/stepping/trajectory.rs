//! Swing goals meet the planted goal with zero relative velocity and acceleration.
use super::SwingStep;

impl SwingStep {
    pub(super) fn target(&self) -> [f32; 3] {
        let t = self.progress.clamp(0.0, 1.0);
        let u = 1.0 - t;
        let ease = |v: f32| v * v * v * (10.0 + v * (-15.0 + 6.0 * v));
        // The symmetric evaluation avoids cancellation near touchdown.
        let smooth = if t <= 0.5 { ease(t) } else { 1.0 - ease(u) };
        let mut target = std::array::from_fn(|i| {
            self.origin[i] + (self.landing.position[i] - self.origin[i]) * smooth
        });
        target[1] += 64.0 * t * t * t * u * u * u * self.clearance;
        target
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StepGround, math};

    #[test]
    fn swing_meets_both_contacts_without_a_velocity_or_acceleration_jump() {
        for scale in [0.1, 1.0, 3.0] {
            let mut step = SwingStep {
                foot: 0,
                origin: [0.0; 3],
                landing: StepGround {
                    position: [0.3 * scale, 0.05 * scale, 0.1 * scale],
                    normal: [0.0, 1.0, 0.0],
                    velocity: [0.0; 3],
                    support: None,
                },
                elapsed: 0.0,
                duration: 1.0,
                clearance: 0.1 * scale,
                progress: 0.0,
                left_ground: true,
                released: true,
            };
            for end in [0.0, 1.0] {
                step.progress = end;
                let contact = step.target();
                step.progress = if end == 0.0 { 0.001 } else { 0.999 };
                let near = step.target();
                assert!(
                    math::length(math::sub(near, contact)) / 0.001f32.powi(2) < 0.1 * scale,
                    "contact-relative displacement must vanish faster than quadratic"
                );
            }
            step.progress = 0.5;
            let middle = step.target();
            assert!((middle[1] - 0.125 * scale).abs() < 1e-6 * scale);
            let mut previous = step.origin[0];
            for i in 0..=1000 {
                step.progress = i as f32 / 1000.0;
                let goal = step.target();
                assert!(goal.iter().all(|v| v.is_finite()));
                assert!(goal[0] >= previous && goal[0] <= step.landing.position[0]);
                previous = goal[0];
            }
        }
    }
}
