//! Surface-local foot targets. Observed support poses own transport; elapsed
//! render time never extrapolates a resident moving body's old velocity.
use crate::{math, BodyPose};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct SupportAnchor {
    pub entity: u64,
    pub local_contact: [f32; 3],
    pub local_normal: [f32; 3],
    pub clearance: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct StepGround {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub velocity: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support: Option<SupportAnchor>,
}

pub(super) fn valid_frame(p: BodyPose) -> bool {
    p.position
        .iter()
        .chain(&p.rotation)
        .chain(&p.linear_velocity)
        .chain(&p.angular_velocity)
        .all(|v| v.is_finite())
        && p.rotation.iter().map(|v| v * v).sum::<f32>() > f32::EPSILON
}

impl StepGround {
    /// The radius belongs to the probe, not the platform's centre of mass.
    pub fn bind(mut self, entity: u64, frame: BodyPose, clearance: f32) -> Option<Self> {
        if !valid_frame(frame)
            || !clearance.is_finite()
            || clearance < 0.0
            || !self.position.iter().all(|v| v.is_finite())
        {
            return None;
        }
        self.normal = math::unit(self.normal).ok()?;
        let inverse = math::inverse(math::normalize(frame.rotation));
        let contact = math::sub(self.position, self.normal.map(|v| v * clearance));
        self.support = Some(SupportAnchor {
            entity,
            local_contact: math::rotate(inverse, math::sub(contact, frame.position)),
            local_normal: math::rotate(inverse, self.normal),
            clearance,
        });
        self.refresh(0.0, |_| Some(frame)).then_some(self)
    }

    /// A missing resident support invalidates the target; it is never replaced
    /// by a frozen world-space anchor. Unbound authored surfaces may translate.
    pub fn refresh(&mut self, dt: f32, mut pose: impl FnMut(u64) -> Option<BodyPose>) -> bool {
        if !dt.is_finite() || dt < 0.0 {
            return false;
        }
        if let Some(anchor) = &self.support {
            let Some(frame) = pose(anchor.entity).filter(|p| valid_frame(*p)) else {
                return false;
            };
            let rotation = math::normalize(frame.rotation);
            let contact = math::add(frame.position, math::rotate(rotation, anchor.local_contact));
            self.normal = math::rotate(rotation, anchor.local_normal);
            self.position = math::add(contact, self.normal.map(|v| v * anchor.clearance));
            self.velocity = math::add(
                frame.linear_velocity,
                math::cross(frame.angular_velocity, math::sub(contact, frame.position)),
            );
        } else {
            self.position = math::add(self.position, self.velocity.map(|v| v * dt));
        }
        self.position
            .iter()
            .chain(&self.normal)
            .chain(&self.velocity)
            .all(|v| v.is_finite())
    }

    pub(super) fn replant(
        &mut self,
        position: [f32; 3],
        mut pose: impl FnMut(u64) -> Option<BodyPose>,
    ) -> bool {
        if !position.iter().all(|v| v.is_finite()) {
            return false;
        }
        if let Some(anchor) = &self.support {
            let Some(frame) = pose(anchor.entity) else {
                return false;
            };
            let Some(rebound) =
                self.clone()
                    .with_position(position)
                    .bind(anchor.entity, frame, anchor.clearance)
            else {
                return false;
            };
            *self = rebound;
        } else {
            self.position = position;
        }
        true
    }

    fn with_position(mut self, position: [f32; 3]) -> Self {
        self.position = position;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> BodyPose {
        BodyPose {
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            linear_velocity: [0.0; 3],
            angular_velocity: [0.0; 3],
        }
    }
    fn ground(scale: f32) -> StepGround {
        StepGround {
            position: [scale, 0.05 * scale, 0.0],
            normal: [0.0, 1.0, 0.0],
            velocity: [0.0; 3],
            support: None,
        }
    }
    fn near(a: [f32; 3], b: [f32; 3], scale: f32) {
        assert!(
            math::length(math::sub(a, b)) < 1e-5 * scale,
            "{a:?} vs {b:?}"
        );
    }
    #[test]
    fn rotation_translation_and_probe_clearance_follow_the_observed_support_at_all_scales() {
        for scale in [0.1, 1.0, 3.0] {
            let mut g = ground(scale).bind(42, frame(), 0.05 * scale).unwrap();
            let mut p = frame();
            p.position = [2.0 * scale, 0.3 * scale, -scale];
            p.rotation = math::axis_angle([0.3, 0.8, -0.2]);
            p.linear_velocity = [0.2, 0.4, -0.6];
            p.angular_velocity = [0.4, -0.2, 0.5];
            assert!(g.refresh(1.0 / 60.0, |id| (id == 42).then_some(p)));
            let contact = math::add(p.position, math::rotate(p.rotation, [scale, 0.0, 0.0]));
            near(
                g.position,
                math::add(contact, math::rotate(p.rotation, [0.0, 0.05 * scale, 0.0])),
                scale,
            );
            near(g.normal, math::rotate(p.rotation, [0.0, 1.0, 0.0]), 1.0);
            near(
                g.velocity,
                math::add(
                    p.linear_velocity,
                    math::cross(p.angular_velocity, math::sub(contact, p.position)),
                ),
                scale,
            );
        }
    }
    #[test]
    fn acceleration_and_stopping_do_not_integrate_stale_surface_velocity() {
        let mut g = ground(1.0).bind(42, frame(), 0.05).unwrap();
        let mut p = frame();
        p.position = [3.0, 1.0, -2.0];
        p.linear_velocity = [8.0, -2.0, 3.0];
        assert!(g.refresh(0.02, |_| Some(p)));
        p.linear_velocity = [0.0; 3];
        for dt in [0.0, 1.0 / 30.0, 1.0 / 144.0, 0.2] {
            assert!(g.refresh(dt, |_| Some(p)));
            near(g.position, [4.0, 1.05, -2.0], 1.0);
            near(g.velocity, [0.0; 3], 1.0);
        }
    }
    #[test]
    fn retired_or_invalid_support_cannot_leave_a_ghost_target() {
        let original = ground(1.0).bind(42, frame(), 0.05).unwrap();
        let mut g = original.clone();
        assert!(!g.refresh(0.02, |_| None));
        let mut p = frame();
        p.position[0] = f32::NAN;
        assert!(!g.refresh(0.02, |_| Some(p)));
        assert!(!g.refresh(f32::NAN, |_| Some(frame())));
        assert!(ground(1.0).bind(42, p, 0.05).is_none());
        p = frame();
        p.rotation = [0.0; 4];
        assert!(ground(1.0).bind(42, p, 0.05).is_none());
        near(g.position, original.position, 1.0);
    }
    #[test]
    fn landing_rebinds_the_measured_foot_instead_of_snapping_back_to_the_requested_target() {
        let mut g = ground(1.0).bind(42, frame(), 0.05).unwrap();
        assert!(g.replant([1.2, 0.06, 0.1], |_| Some(frame())));
        let mut p = frame();
        p.position = [2.0, 0.0, 0.0];
        assert!(g.refresh(0.02, |_| Some(p)));
        near(g.position, [3.2, 0.06, 0.1], 1.0);
    }
    #[test]
    fn unbound_authored_surface_preserves_its_explicit_translation_contract() {
        let mut g = ground(1.0);
        g.velocity = [2.0, 0.0, -1.0];
        assert!(g.refresh(0.5, |_| None));
        near(g.position, [2.0, 0.05, -0.5], 1.0);
    }
}
