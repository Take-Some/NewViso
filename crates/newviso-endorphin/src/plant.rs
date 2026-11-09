//! Contact-informed foot placement through internal Jacobian-transpose torques.
use crate::{math, BalanceGains, BodyPose, RigState, SupportContact};
use serde::Serialize;
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize)]
pub struct FootPlant {
    pub position: [f32; 3],
    pub confidence: f32,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct FootPlantState {
    pub targets: BTreeMap<usize, FootPlant>,
}
impl FootPlantState {
    /// Update support memory without evaluating a Jacobian-transpose task.
    /// The step planner already owns these limbs when procedural legs are active.
    pub fn observe(
        &mut self,
        rig: &RigState,
        contacts: &[SupportContact],
        surface_velocity: [f32; 3],
        gravity: f32,
        height: f32,
        response_scale: f32,
        dt: f32,
        mut pose: impl FnMut(u64) -> Option<BodyPose>,
    ) {
        if ![gravity, height, response_scale, dt]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
        {
            return;
        }
        let omega = (gravity / height).sqrt();
        let decay = (-dt * omega / response_scale).exp();
        for (index, target) in &mut self.targets {
            target.position = math::add(target.position, surface_velocity.map(|v| v * dt));
            target.confidence = if contacts.iter().any(|c| c.part == *index) {
                1.0
            } else {
                target.confidence * decay
            };
        }
        for c in contacts {
            if self.targets.contains_key(&c.part) {
                continue;
            }
            if let Some(part) = rig.parts.get(c.part) {
                if let Some(p) = pose(part.entity) {
                    self.targets.entry(c.part).or_insert(FootPlant {
                        position: p.position,
                        confidence: 1.0,
                    });
                }
            }
        }
        self.targets.retain(|_, p| p.confidence > f32::EPSILON);
    }
    pub fn correct(
        &mut self,
        rig: &RigState,
        contacts: &[SupportContact],
        surface_velocity: [f32; 3],
        gravity: f32,
        height: f32,
        uprightness: f32,
        brace: f32,
        gains: BalanceGains,
        response_scale: f32,
        dt: f32,
        mut pose: impl FnMut(u64) -> Option<BodyPose>,
    ) -> Vec<[f32; 3]> {
        let mut result = vec![[0.0; 3]; rig.joints.len()];
        if gravity <= 0.0 || height <= 0.0 || dt <= 0.0 {
            return result;
        }
        self.observe(
            rig,
            contacts,
            surface_velocity,
            gravity,
            height,
            response_scale,
            dt,
            &mut pose,
        );
        let omega = (gravity / height).sqrt();
        let total = self.targets.values().map(|p| p.confidence).sum::<f32>();
        if total <= f32::EPSILON {
            return result;
        }
        let mass = rig.parts.iter().map(|p| p.mass_kg).sum::<f32>();
        for (index, target) in &self.targets {
            let Some(foot) = rig.parts.get(*index).and_then(|p| pose(p.entity)) else {
                continue;
            };
            let load = mass * target.confidence / total.max(1.0);
            let error = math::sub(foot.position, target.position);
            let velocity = math::sub(foot.linear_velocity, surface_velocity);
            let authority = uprightness.powi(2) * (1.0 - brace);
            let force = std::array::from_fn(|i| {
                load * (-omega * omega * gains.plant_position * error[i]
                    - 2.0 * omega * gains.damping * velocity[i])
                    * authority
            });
            let mut child = *index;
            while let Some((j, joint)) = rig
                .joints
                .iter()
                .enumerate()
                .find(|(_, j)| j.child_part == child)
            {
                let Some(p) = pose(joint.child) else {
                    break;
                };
                let anchor = math::add(p.position, math::rotate(p.rotation, joint.child_anchor));
                result[j] = math::add(
                    result[j],
                    math::cross(math::sub(foot.position, anchor), force),
                );
                child = joint.parent_part;
            }
        }
        result
    }
}
