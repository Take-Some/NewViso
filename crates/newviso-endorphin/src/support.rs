//! Inverse dynamics from body gravity, desired translation and physical foot loads.
use crate::{math, BalanceGains, BalanceState, BodyPose, RigState};
#[derive(Clone, Copy, Debug)]
pub struct SupportContact {
    pub part: usize,
    pub position: [f32; 3],
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub enum AngularDriveBody {
    Parent,
    Child,
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct WorldAngularDrive {
    pub body: AngularDriveBody,
    pub target_rotation: [f32; 4],
    pub stiffness: f32,
    pub damping: f32,
}
pub struct SupportPlan {
    pub world_drives: Vec<Option<WorldAngularDrive>>,
    pub joint_torques: Vec<[f32; 3]>,
    pub center_of_pressure: [f32; 3],
    pub root_moment: [f32; 3],
    pub stance_hips: Vec<usize>,
}
/// Project the desired pressure point onto the measured contact patch. Nonnegative
/// pressure fractions sum to one; no contact can pull on the supporting surface.
fn pressure_weights(contacts: &[SupportContact], target: [f32; 3]) -> Vec<f32> {
    let n = contacts.len();
    if n == 0 {
        return vec![];
    }
    let center: [f32; 3] =
        std::array::from_fn(|i| contacts.iter().map(|c| c.position[i]).sum::<f32>() / n as f32);
    let points: Vec<[f32; 2]> = contacts
        .iter()
        .map(|c| [c.position[0] - center[0], c.position[2] - center[2]])
        .collect();
    let target = [target[0] - center[0], target[2] - center[2]];
    let lipschitz = points
        .iter()
        .map(|p| p[0] * p[0] + p[1] * p[1])
        .sum::<f32>();
    let mut weights = vec![1.0 / n as f32; n];
    if lipschitz <= f32::EPSILON {
        return weights;
    }
    let mut order = vec![0.0; n];
    let mut next = vec![0.0; n];
    for _ in 0..64 {
        let cp: [f32; 2] =
            std::array::from_fn(|i| points.iter().zip(&weights).map(|(p, w)| p[i] * w).sum());
        for (value, (p, w)) in next.iter_mut().zip(points.iter().zip(&weights)) {
            *value = w - (p[0] * (cp[0] - target[0]) + p[1] * (cp[1] - target[1])) / lipschitz;
        }
        order.copy_from_slice(&next);
        order.sort_by(|a, b| b.total_cmp(a));
        let mut sum = 0.0;
        let mut threshold = 0.0;
        for (i, v) in order.iter().enumerate() {
            sum += v;
            let t = (sum - 1.0) / (i + 1) as f32;
            if *v > t {
                threshold = t;
            }
        }
        let mut change = 0.0f32;
        for (v, w) in next.iter_mut().zip(&weights) {
            *v = (*v - threshold).max(0.0);
            change = change.max((*v - *w).abs());
        }
        std::mem::swap(&mut weights, &mut next);
        if change < 1e-6 {
            break;
        }
    }
    weights
}
impl RigState {
    pub fn support_plan(
        &self,
        contacts: &[SupportContact],
        balance: &BalanceState,
        com: [f32; 3],
        velocity: [f32; 3],
        surface_velocity: [f32; 3],
        gravity: f32,
        gains: BalanceGains,
        pose: impl FnMut(u64) -> Option<BodyPose>,
    ) -> Option<SupportPlan> {
        self.support_plan_with_torso(
            contacts,
            balance,
            com,
            velocity,
            surface_velocity,
            gravity,
            gains,
            None,
            None,
            None,
            pose,
        )
    }
    pub fn support_plan_with_torso(
        &self,
        contacts: &[SupportContact],
        balance: &BalanceState,
        com: [f32; 3],
        velocity: [f32; 3],
        surface_velocity: [f32; 3],
        gravity: f32,
        gains: BalanceGains,
        torso_target: Option<[f32; 4]>,
        height_target: Option<f32>,
        support_target: Option<[f32; 3]>,
        mut pose: impl FnMut(u64) -> Option<BodyPose>,
    ) -> Option<SupportPlan> {
        if !balance.supported || contacts.is_empty() || gravity <= 0.0 {
            return None;
        }
        let height = com[1] - balance.support_center[1];
        if height <= f32::EPSILON {
            return None;
        }
        let omega = (gravity / height).sqrt();
        let relative = math::sub(velocity, surface_velocity);
        let poses: Vec<_> = self
            .parts
            .iter()
            .map(|p| pose(p.entity))
            .collect::<Option<_>>()?;
        let mass = self.parts.iter().map(|p| p.mass_kg).sum::<f32>();
        let sample = |entity| {
            self.parts
                .iter()
                .position(|p| p.entity == entity)
                .map(|i| poses[i])
        };
        let supported_inertia = self.inertia_about(balance.support_center, sample)?;
        let supported_spin = self.angular_velocity_about(
            balance.support_center,
            surface_velocity,
            supported_inertia,
            sample,
        )?;
        let offset = math::sub(com, balance.support_center);
        let lean = math::cross(
            math::sub(com, support_target.unwrap_or(balance.support_center)),
            [0.0, 1.0, 0.0],
        )
        .map(|v| v / height);
        let alpha: [f32; 3] = std::array::from_fn(|i| {
            if i == 1 {
                0.0
            } else {
                omega * omega * gains.capture * lean[i]
                    - 2.0 * omega * gains.damping * supported_spin[i]
            }
        });
        let ay = height_target.map_or(0.0, |target| {
            (omega * omega * gains.height * (target - height)
                - 2.0 * omega * gains.height.sqrt() * gains.damping * relative[1])
                .max(-gravity)
        });
        let normal = mass * (gravity + ay).max(f32::EPSILON);
        let gravity_moment = math::cross(offset, [0.0, -mass * gravity, 0.0]);
        let vertical_momentum = math::cross(offset, [0.0, mass * ay, 0.0]);
        let required = math::sub(
            math::add(
                math::tensor_vector(supported_inertia, alpha),
                vertical_momentum,
            ),
            gravity_moment,
        );
        let balanced = !self.stance_entities.is_empty();
        let target = if balanced {
            [
                balance.support_center[0] + required[2] / normal,
                balance.support_center[1],
                balance.support_center[2] - required[0] / normal,
            ]
        } else {
            [com[0], balance.support_center[1], com[2]]
        };
        let weights = pressure_weights(contacts, target);
        let pressure: [f32; 3] = std::array::from_fn(|i| {
            contacts
                .iter()
                .zip(&weights)
                .map(|(c, w)| c.position[i] * w)
                .sum()
        });
        // Pressure projection also limits the achievable angular acceleration.
        // This prevents asking an ankle for a support moment outside its sole.
        let feasible_moment = math::add(
            gravity_moment,
            math::cross(
                math::sub(pressure, balance.support_center),
                [0.0, normal, 0.0],
            ),
        );
        let feasible = math::solve_symmetric(
            supported_inertia,
            math::sub(feasible_moment, vertical_momentum),
        )?;
        let tangential = math::cross(feasible, offset);
        let acceleration = if balanced {
            [tangential[0], ay + tangential[1], tangential[2]]
        } else {
            [0.0, ay, 0.0]
        };
        let mut force = Vec::with_capacity(self.parts.len());
        let mut moment = Vec::with_capacity(self.parts.len());
        for (part, p) in self.parts.iter().zip(&poses) {
            let relative_position = math::sub(p.position, com);
            let angular_acceleration = if balanced { feasible } else { [0.0; 3] };
            let body_acceleration = math::add(
                acceleration,
                math::cross(angular_acceleration, relative_position),
            );
            let f = std::array::from_fn(|i| {
                part.mass_kg * ((if i == 1 { -gravity } else { 0.0 }) - body_acceleration[i])
            });
            force.push(f);
            moment.push(math::sub(
                math::cross(relative_position, f),
                math::tensor_vector(
                    math::tensor(p.rotation, part.inertia_diagonal),
                    angular_acceleration,
                ),
            ));
        }
        let mut center_of_pressure = [0.0; 3];
        for (contact, weight) in contacts.iter().zip(weights) {
            if contact.part >= self.parts.len() {
                return None;
            }
            let f = std::array::from_fn(|i| {
                mass * weight * ((if i == 1 { gravity } else { 0.0 }) + acceleration[i])
            });
            force[contact.part] = math::add(force[contact.part], f);
            moment[contact.part] = math::add(
                moment[contact.part],
                math::cross(math::sub(contact.position, com), f),
            );
            center_of_pressure =
                math::add(center_of_pressure, contact.position.map(|v| v * weight));
        }
        let mut torques = vec![[0.0; 3]; self.joints.len()];
        for (index, joint) in self.joints.iter().enumerate().rev() {
            let p = poses[joint.child_part];
            let anchor = math::add(p.position, math::rotate(p.rotation, joint.child_anchor));
            torques[index] = math::sub(
                moment[joint.child_part],
                math::cross(math::sub(anchor, com), force[joint.child_part]),
            )
            .map(|v| -v * gains.gravity_compensation * balance.uprightness.powi(2));
            force[joint.parent_part] = math::add(force[joint.parent_part], force[joint.child_part]);
            moment[joint.parent_part] =
                math::add(moment[joint.parent_part], moment[joint.child_part]);
        }
        // A torso virtual torque is realized as bounded internal hip couples.
        // Pose impedance and gravity compensation continue carrying leg load.
        let mut stance_hips = Vec::new();
        let mut hip_loads = Vec::new();
        let pressure_fractions = pressure_weights(contacts, pressure);
        for (contact, weight) in contacts.iter().zip(&pressure_fractions) {
            if !self
                .stance_entities
                .contains(&self.parts[contact.part].entity)
            {
                continue;
            }
            let mut child = contact.part;
            while let Some((index, joint)) = self
                .joints
                .iter()
                .enumerate()
                .find(|(_, j)| j.child_part == child)
            {
                if joint.parent_part == 0 {
                    if let Some(slot) = stance_hips.iter().position(|j| *j == index) {
                        hip_loads[slot] += *weight;
                    } else {
                        stance_hips.push(index);
                        hip_loads.push(*weight);
                    }
                    break;
                }
                child = joint.parent_part;
            }
        }
        let mut world_drives = vec![None; self.joints.len()];
        if !stance_hips.is_empty() {
            let sampled = |entity| {
                self.parts
                    .iter()
                    .position(|p| p.entity == entity)
                    .map(|i| poses[i])
            };
            let inertia = self.inertia_about(com, sampled)?;
            let target_rotation = torso_target.unwrap_or_else(|| {
                math::multiply(
                    math::align_axis(balance.body_up, [0.0, 1.0, 0.0]),
                    poses[0].rotation,
                )
            });
            let stiffness = mass * gravity * height * gains.posture;
            let damping = 2.0
                * (stiffness * (inertia[0][0] + inertia[1][1] + inertia[2][2]) / 3.0).sqrt()
                * gains.damping;
            let total = hip_loads.iter().sum::<f32>();
            if total > f32::EPSILON {
                for (index, load) in stance_hips.iter().zip(hip_loads) {
                    world_drives[*index] = (stiffness > 0.0).then_some(WorldAngularDrive {
                        body: AngularDriveBody::Parent,
                        target_rotation,
                        stiffness: stiffness * load / total,
                        damping: damping * load / total,
                    });
                }
            }
        }
        // A loaded sole is regulated in world space through the ankle couple.
        // This survives pelvis motion while sharing the same muscle effort cap.
        if gains.plant_position > 0.0 {
            for contact in contacts {
                let part = &self.parts[contact.part];
                let Some(axis) = part.support_axis else {
                    continue;
                };
                let Some((index, joint)) = self
                    .joints
                    .iter()
                    .enumerate()
                    .find(|(_, j)| j.child_part == contact.part)
                else {
                    continue;
                };
                let Some(muscle) = joint.muscle else {
                    continue;
                };
                let p = poses[contact.part];
                world_drives[index] = Some(WorldAngularDrive {
                    body: AngularDriveBody::Child,
                    target_rotation: math::multiply(
                        math::align_axis(math::rotate(p.rotation, axis), balance.support_normal),
                        p.rotation,
                    ),
                    // Ground orientation authority follows the available support
                    // moment. In weak gravity a touching sole cannot act as a weld.
                    stiffness: {
                        let foot_load: f32 = contacts
                            .iter()
                            .zip(&pressure_fractions)
                            .filter(|(c, _)| c.part == contact.part)
                            .map(|(_, w)| *w)
                            .sum();
                        let lever = part.support_half_extents.map_or(0.0, |h| h[0].max(h[1]));
                        muscle.stiffness.min(normal * foot_load * lever)
                            * gains.plant_position
                            * balance.uprightness.powi(2)
                    },
                    damping: 0.0,
                });
            }
        }
        if !torques.iter().flatten().all(|v| v.is_finite()) {
            return None;
        }
        Some(SupportPlan {
            world_drives,
            joint_torques: torques,
            center_of_pressure,
            root_moment: moment[0],
            stance_hips,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pressure_is_nonnegative_conserved_and_projects_to_support() {
        let c = vec![
            SupportContact {
                part: 0,
                position: [-0.2, 0.0, -0.1],
            },
            SupportContact {
                part: 0,
                position: [0.2, 0.0, -0.1],
            },
            SupportContact {
                part: 1,
                position: [0.2, 0.0, 0.1],
            },
            SupportContact {
                part: 1,
                position: [-0.2, 0.0, 0.1],
            },
        ];
        for (target, expected) in [
            ([0.08, 0.0, 0.02], [0.08, 0.0, 0.02]),
            ([1.0, 0.0, 0.5], [0.2, 0.0, 0.1]),
        ] {
            let w = pressure_weights(&c, target);
            assert!(w.iter().all(|w| *w >= 0.0));
            assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5);
            let cp =
                std::array::from_fn(|i| c.iter().zip(&w).map(|(c, w)| c.position[i] * w).sum());
            assert!(math::length(math::sub(cp, expected)) < 0.0001, "cp={cp:?}");
        }
    }
}
