//! Contact-aware stepping and support-consistent two-link leg targets.
//! This module only produces goals. Native physics owns every position and velocity.
use crate::{BalanceState, BodyPose, RigState, SupportContact, math};
use serde::Serialize;
use std::collections::BTreeMap;
mod model;
pub use model::{StepLeg, SteppingModel};
mod ground;
mod observation;
mod support_motion;
mod trajectory;
pub use ground::{StepGround, SupportAnchor};
#[derive(Clone, Debug, Serialize)]
pub struct SwingStep {
    pub foot: usize,
    pub origin: [f32; 3],
    pub landing: StepGround,
    pub elapsed: f32,
    pub duration: f32,
    pub clearance: f32,
    pub progress: f32,
    pub left_ground: bool,
    pub released: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct SteppingState {
    pub phase: String,
    pub swing: Option<SwingStep>,
    pub planted: BTreeMap<usize, StepGround>,
    pub completed: u64,
    pub rejected: u64,
    pub desired_velocity: [f32; 3],
    pub foot_goals: BTreeMap<usize, [f32; 3]>,
    pub anticipated_support_margin: f32,
    pub support_goal: Option<[f32; 3]>,
    #[serde(skip)]
    knee_planes: BTreeMap<usize, [f32; 3]>,
    #[serde(skip)]
    references: BTreeMap<usize, ([f32; 4], [f32; 4], [f32; 3], [f32; 3])>,
    #[serde(skip)]
    root_reference: Option<[f32; 4]>,
    #[serde(skip)]
    sole_references: BTreeMap<usize, [f32; 4]>,
    #[serde(skip)]
    root_height: Option<f32>,
    #[serde(skip)]
    joint_goals: BTreeMap<usize, [f32; 4]>,
    #[serde(skip)]
    support_goal_frame: Option<(u64, [f32; 3])>,
    #[serde(skip)]
    sole_frames: BTreeMap<usize, (u64, [f32; 4])>,
    #[serde(skip)]
    pose_samples: Vec<(u64, Option<BodyPose>)>,
    #[serde(skip)]
    scratch: StepScratch,
}
#[derive(Clone, Debug, Default)]
struct StepScratch {
    legs: Vec<LegSample>,
    prospective: Vec<[f32; 3]>,
    balance: crate::balance::BalanceScratch,
}
#[derive(Clone, Debug)]
struct LegSample {
    foot: usize,
    hip_joint: usize,
    knee_joint: usize,
    ankle_joint: usize,
    sole: BodyPose,
    hip: [f32; 3],
    knee: [f32; 3],
    ankle: [f32; 3],
    length: f32,
    radius: f32,
}
impl SteppingState {
    #[allow(clippy::too_many_arguments)]
    pub fn plan(
        &mut self,
        model: &SteppingModel,
        rig: &RigState,
        balance: &BalanceState,
        contacts: &[SupportContact],
        com: [f32; 3],
        velocity: [f32; 3],
        gravity: f32,
        capacity: f32,
        dt: f32,
        sample_pose: impl FnMut(u64) -> Option<BodyPose>,
        ground: impl FnMut([f32; 3], f32, f32) -> Option<StepGround>,
        path_clear: impl FnMut([f32; 3], [f32; 3], f32) -> bool,
    ) -> Vec<Option<[f32; 4]>> {
        let mut observation =
            observation::PoseObservation::new(sample_pose, std::mem::take(&mut self.pose_samples));
        let mut scratch = std::mem::take(&mut self.scratch);
        let result = self.plan_sampled(
            model,
            rig,
            balance,
            contacts,
            com,
            velocity,
            gravity,
            capacity,
            dt,
            |entity| observation.pose(entity),
            ground,
            path_clear,
            &mut scratch,
        );
        self.pose_samples = observation.into_frames();
        self.scratch = scratch;
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn plan_sampled(
        &mut self,
        model: &SteppingModel,
        rig: &RigState,
        balance: &BalanceState,
        contacts: &[SupportContact],
        com: [f32; 3],
        velocity: [f32; 3],
        gravity: f32,
        capacity: f32,
        dt: f32,
        mut pose: impl FnMut(u64) -> Option<BodyPose>,
        mut ground: impl FnMut([f32; 3], f32, f32) -> Option<StepGround>,
        mut path_clear: impl FnMut([f32; 3], [f32; 3], f32) -> bool,
        scratch: &mut StepScratch,
    ) -> Vec<Option<[f32; 4]>> {
        let mut result = vec![None; rig.joints.len()];
        if ![gravity, dt, capacity, balance.uprightness]
            .iter()
            .all(|v| v.is_finite())
            || !com.iter().chain(&velocity).all(|v| v.is_finite())
            || gravity <= 0.0
            || dt <= 0.0
            || capacity <= 0.0
            || balance.uprightness < model.minimum_uprightness
        {
            "unavailable".clone_into(&mut self.phase);
            self.swing = None;
            self.planted.clear();
            self.references.clear();
            self.knee_planes.clear();
            self.joint_goals.clear();
            self.root_reference = None;
            self.sole_references.clear();
            self.root_height = None;
            self.support_goal = None;
            self.support_goal_frame = None;
            self.sole_frames.clear();
            self.foot_goals.clear();
            return result;
        }
        self.advance_supports(dt, &mut pose);
        if !balance.supported && self.planted.is_empty() {
            "airborne".clone_into(&mut self.phase);
            self.support_goal = None;
            self.foot_goals.clear();
            return result;
        }
        let legs = &mut scratch.legs;
        legs.clear();
        for binding in &model.legs {
            let find = |name: &str| rig.parts.iter().position(|p| p.name == name);
            let Some((upper, lower, foot)) = find(&binding.thigh)
                .zip(find(&binding.calf))
                .zip(find(&binding.foot))
                .map(|((a, b), c)| (a, b, c))
            else {
                continue;
            };
            let find_joint = |child| rig.joints.iter().position(|j| j.child_part == child);
            let Some((hj, kj, aj)) = find_joint(upper)
                .zip(find_joint(lower))
                .zip(find_joint(foot))
                .map(|((a, b), c)| (a, b, c))
            else {
                continue;
            };
            let Some((up, lo, sole)) = pose(rig.parts[upper].entity)
                .zip(pose(rig.parts[lower].entity))
                .zip(pose(rig.parts[foot].entity))
                .map(|((a, b), c)| (a, b, c))
            else {
                continue;
            };
            let anchor =
                |p: BodyPose, offset| math::add(p.position, math::rotate(p.rotation, offset));
            let hip = anchor(up, rig.joints[hj].child_anchor);
            let knee = anchor(lo, rig.joints[kj].child_anchor);
            let ankle = anchor(sole, rig.joints[aj].child_anchor);
            let length = math::length(math::sub(knee, hip)) + math::length(math::sub(ankle, knee));
            let radius = rig.parts[foot].support_clearance.unwrap_or(0.0);
            if length <= 0.0 || radius <= 0.0 {
                continue;
            }
            self.sole_references.entry(foot).or_insert(sole.rotation);
            let Some(root) = pose(rig.joints[hj].parent) else {
                continue;
            };
            self.root_reference.get_or_insert_with(|| {
                math::multiply(
                    math::align_axis(balance.body_up, [0.0, 1.0, 0.0]),
                    root.rotation,
                )
            });
            self.references.entry(foot).or_insert((
                up.rotation,
                lo.rotation,
                math::sub(knee, hip),
                math::sub(ankle, knee),
            ));
            self.knee_planes.entry(foot).or_insert_with(|| {
                let axis = math::sub(ankle, hip);
                let projection =
                    math::dot(math::sub(knee, hip), axis) / math::dot(axis, axis).max(f32::EPSILON);
                let bend = math::sub(math::sub(knee, hip), axis.map(|v| v * projection));
                if math::length(bend) > length * 0.001 {
                    math::rotate(math::inverse(root.rotation), bend)
                } else {
                    binding.knee_direction
                }
            });
            if !self.planted.contains_key(&foot) {
                if let Some(g) = ground(sole.position, radius, length * model.stride_leg_fraction) {
                    self.planted.insert(foot, g);
                }
            }
            self.follow_sole_frame(foot, &mut pose);
            legs.push(LegSample {
                foot,
                hip_joint: hj,
                knee_joint: kj,
                ankle_joint: aj,
                sole,
                hip,
                knee,
                ankle,
                length,
                radius,
            });
        }
        if legs.len() != model.legs.len() || self.planted.len() < legs.len() {
            "unavailable".clone_into(&mut self.phase);
            self.swing = None;
            self.foot_goals.clear();
            self.joint_goals.clear();
            self.support_goal = None;
            self.support_goal_frame = None;
            self.anticipated_support_margin = 0.0;
            return result;
        }
        let mean: [f32; 3] = std::array::from_fn(|i| {
            self.planted.values().map(|g| g.position[i]).sum::<f32>() / legs.len() as f32
        });
        self.support_goal.get_or_insert([com[0], mean[1], com[2]]);
        let height = (com[1] - mean[1]).max(f32::EPSILON);
        let omega = (gravity / height).sqrt();
        let surface_velocity = self.support_velocity(contacts, None);
        let relative = math::sub(velocity, surface_velocity);
        let capture = math::add(com, relative.map(|v| v / omega));
        // A pending unload cannot use a stance that has actually lost contact.
        // Replan from the current support instead of trying to lift its only foot.
        if self
            .swing
            .as_ref()
            .is_some_and(|step| !step.released && !contacts.iter().any(|c| c.part != step.foot))
        {
            self.swing = None;
            self.support_goal = Some([com[0], mean[1], com[2]]);
            self.support_goal_frame = None;
        }
        if self.swing.is_none() {
            "stance".clone_into(&mut self.phase);
            let prospective = &mut scratch.prospective;
            prospective.clear();
            prospective.extend(contacts.iter().map(|c| c.position));
            // A sole already descending a few millimetres onto verified floor
            // supplies imminent support. This only postpones a new step; it
            // never changes the measured physical support or motor load.
            for leg in legs.iter() {
                if contacts.iter().any(|c| c.part == leg.foot) {
                    continue;
                }
                let surface = &self.planted[&leg.foot];
                let gap = math::dot(
                    math::sub(leg.sole.position, surface.position),
                    surface.normal,
                );
                let up = math::rotate(leg.sole.rotation, [0.0, 0.0, 1.0]);
                if gap >= 0.0
                    && gap <= leg.radius * model.landing_radius_fraction
                    && math::dot(up, surface.normal) >= model.minimum_uprightness
                    && leg.sole.linear_velocity[1] - surface.velocity[1] <= omega * leg.radius
                {
                    if let Some(h) = rig.parts[leg.foot].support_half_extents {
                        let rotation = self.sole_references[&leg.foot];
                        for x in [-h[0], h[0]] {
                            for y in [-h[1], h[1]] {
                                prospective.push(math::sub(
                                    math::add(
                                        surface.position,
                                        math::rotate(rotation, [x, y, 0.0]),
                                    ),
                                    surface.normal.map(|v| v * leg.radius),
                                ));
                            }
                        }
                    }
                }
            }
            self.anticipated_support_margin = crate::balance::measure_balance_reusing(
                prospective,
                com,
                velocity,
                surface_velocity,
                balance.body_up,
                gravity,
                0.0,
                std::f32::consts::FRAC_PI_2,
                &mut scratch.balance,
            )
            .support_margin;
            let margin = legs.iter().map(|l| l.radius).fold(f32::INFINITY, f32::min)
                * model.capture_margin_foot_fraction;
            if self.anticipated_support_margin < margin {
                // Stable stance needs no swing-time extrapolation or exponential.
                let planar = |v: [f32; 3]| [v[0], 0.0, v[2]];
                let duration = legs
                    .iter()
                    .map(|l| (l.length / gravity).sqrt() * model.duration_scale / capacity.sqrt())
                    .sum::<f32>()
                    / legs.len() as f32;
                let prediction = (omega * duration).exp();
                let desired = std::array::from_fn(|i| {
                    if i == 1 {
                        mean[i]
                    } else {
                        mean[i] + (capture[i] - mean[i]) * prediction * model.position_response
                            - self.desired_velocity[i] / omega
                    }
                });
                let candidate = legs
                    .iter()
                    .filter(|l| {
                        legs.iter().any(|other| {
                            other.foot != l.foot && contacts.iter().any(|c| c.part == other.foot)
                        })
                    })
                    .max_by(|a, b| {
                        math::length(planar(math::sub(a.sole.position, desired)))
                            .total_cmp(&math::length(planar(math::sub(b.sole.position, desired))))
                    });
                if let Some(leg) = candidate {
                    let stride = leg.length * model.stride_leg_fraction * capacity.sqrt();
                    let delta = planar(math::sub(desired, leg.sole.position));
                    let span = math::length(delta);
                    let mut target = math::add(
                        leg.sole.position,
                        delta.map(|v| v * (stride / span.max(stride))),
                    );
                    if let Some(other) =
                        legs.iter().filter(|l| l.foot != leg.foot).min_by(|a, b| {
                            math::length(math::sub(a.hip, leg.hip))
                                .total_cmp(&math::length(math::sub(b.hip, leg.hip)))
                        })
                    {
                        let lateral = [leg.hip[0] - other.hip[0], 0.0, leg.hip[2] - other.hip[2]];
                        let width = math::length(lateral);
                        if let Ok(lateral) = math::unit(lateral) {
                            let separation =
                                math::dot(math::sub(target, other.sole.position), lateral);
                            target = math::add(
                                target,
                                lateral.map(|v| v * (width - separation).max(0.0)),
                            );
                        }
                    }
                    let landing = ground(target, leg.radius, stride);
                    if let Some(landing) = landing {
                        let clearance = leg.length * model.clearance_leg_fraction;
                        let middle = math::add(
                            math::add(leg.sole.position, landing.position).map(|v| v * 0.5),
                            [0.0, clearance, 0.0],
                        );
                        if path_clear(leg.sole.position, middle, leg.radius)
                            && path_clear(middle, landing.position, leg.radius)
                        {
                            self.swing = Some(SwingStep {
                                foot: leg.foot,
                                origin: leg.sole.position,
                                landing,
                                elapsed: 0.0,
                                duration: (leg.length / gravity).sqrt() * model.duration_scale
                                    / capacity.sqrt(),
                                clearance,
                                progress: 0.0,
                                left_ground: false,
                                released: false,
                            });
                        } else {
                            self.rejected += 1;
                        }
                    } else {
                        self.rejected += 1;
                    }
                }
            }
        }
        // Keep both feet available while shifting load. A queued foot may lift
        // only when the measured capture point is supported by the remaining
        // actual contact patch. An airborne foot is already unloaded and may
        // reach for capture support; elapsed time never proves physical support.
        let ready_to_lift = self
            .swing
            .as_ref()
            .filter(|step| !step.released)
            .is_some_and(|step| {
                let remaining: Vec<_> = contacts
                    .iter()
                    .filter(|c| c.part != step.foot)
                    .map(|c| c.position)
                    .collect();
                let surface_velocity = self.support_velocity(contacts, Some(step.foot));
                let support = crate::balance::measure_balance_reusing(
                    &remaining,
                    com,
                    velocity,
                    surface_velocity,
                    balance.body_up,
                    gravity,
                    0.0,
                    std::f32::consts::FRAC_PI_2,
                    &mut scratch.balance,
                );
                support.supported
                    && (!contacts.iter().any(|c| c.part == step.foot)
                        || support.support_margin >= 0.0)
            });
        if let Some(step) = self.swing.as_mut() {
            if !step.released {
                if let Some(leg) = legs.iter().find(|l| l.foot == step.foot) {
                    step.origin = leg.sole.position;
                }
                step.released = ready_to_lift;
            }
            if step.released {
                step.elapsed += dt;
            }
            step.progress = (step.elapsed / step.duration).clamp(0.0, 1.0);
            let phase = if !step.released {
                "load_transfer"
            } else if step.progress < 1.0 {
                "swing"
            } else {
                "landing"
            };
            phase.clone_into(&mut self.phase);
            let leg = legs
                .iter()
                .find(|l| l.foot == step.foot)
                .expect("step foot");
            let grounded = contacts.iter().any(|c| c.part == step.foot);
            if step.released {
                step.left_ground |= !grounded;
            }
            if step.progress >= 0.5
                && step.left_ground
                && grounded
                && math::length(math::sub(leg.sole.position, step.landing.position))
                    <= leg.radius * model.landing_radius_fraction
            {
                let mut landed = step.landing.clone();
                if !landed.replant(leg.sole.position, &mut pose) {
                    self.support_goal = None;
                    return result;
                }
                self.sole_frames.remove(&step.foot);
                self.planted.insert(step.foot, landed);
                self.completed += 1;
                self.support_goal = Some([com[0], mean[1], com[2]]);
                self.swing = None;
                "stance".clone_into(&mut self.phase);
            }
        }
        if let Some(step) = &self.swing {
            let supports: Vec<_> = legs.iter().filter(|l| l.foot != step.foot).collect();
            if let Some(goal) = &mut self.support_goal {
                if !supports.is_empty() {
                    for i in [0, 2] {
                        let desired = supports
                            .iter()
                            .map(|l| self.planted[&l.foot].position[i])
                            .sum::<f32>()
                            / supports.len() as f32;
                        goal[i] += (desired - goal[i])
                            * (1.0 - (-dt * omega * model.position_response).exp());
                    }
                }
            }
        }
        // Retain the stable map nodes instead of freeing and allocating them
        // on every fixed tick. Removed leg bindings still lose their goals.
        self.foot_goals
            .retain(|foot, _| legs.iter().any(|leg| leg.foot == *foot));
        for leg in legs.iter() {
            let mut target = self.planted[&leg.foot].position;
            if let Some(step) = self
                .swing
                .as_ref()
                .filter(|s| s.released && s.foot == leg.foot)
            {
                target = step.target();
            }
            self.foot_goals.insert(leg.foot, target);
            let hipj = rig.joints[leg.hip_joint];
            let kneej = rig.joints[leg.knee_joint];
            let anklej = rig.joints[leg.ankle_joint];
            let Some(root) = pose(hipj.parent) else {
                continue;
            };
            let normal = self.planted[&leg.foot].normal;
            let reference = self.sole_references[&leg.foot];
            let sole_rotation = math::multiply(
                math::align_axis(math::rotate(reference, [0.0, 0.0, 1.0]), normal),
                reference,
            );
            let swinging = self
                .swing
                .as_ref()
                .is_some_and(|step| step.released && step.foot == leg.foot);
            let root_rotation = self.root_reference.unwrap_or(root.rotation);
            let height = *self.root_height.get_or_insert(root.position[1] - mean[1]);
            let center = self.support_goal.unwrap_or(mean);
            let mut root_goal = root.position;
            for i in [0, 2] {
                root_goal[i] += (center[i] - com[i]) * model.position_response;
            }
            root_goal[1] = mean[1] + height;
            let hip = if swinging {
                leg.hip
            } else {
                math::add(root_goal, math::rotate(root_rotation, hipj.parent_anchor))
            };
            let ankle_goal = math::add(target, math::rotate(sole_rotation, anklej.child_anchor));
            let a = math::length(math::sub(leg.knee, leg.hip));
            let b = math::length(math::sub(leg.ankle, leg.knee));
            let bend = math::rotate(root_rotation, self.knee_planes[&leg.foot]);
            let Some((knee, ankle)) =
                solve_two_link(hip, ankle_goal, bend, a, b, model.reach_fraction)
            else {
                continue;
            };
            let reference = self.references[&leg.foot];
            let desired_upper = math::sub(knee, hip);
            let desired_lower = math::sub(ankle, knee);
            let hinge = math::cross(desired_upper, desired_lower);
            let reference_hinge = math::cross(reference.2, reference.3);
            let reference_hinge = if math::length(reference_hinge) > f32::EPSILON {
                reference_hinge
            } else {
                math::cross(reference.2, bend)
            };
            let upper = math::multiply(
                segment_frame(reference.2, reference_hinge, desired_upper, hinge),
                reference.0,
            );
            let lower = math::multiply(
                segment_frame(reference.3, reference_hinge, desired_lower, hinge),
                reference.1,
            );
            for (index, parent, child) in [
                (
                    leg.hip_joint,
                    if swinging {
                        root.rotation
                    } else {
                        root_rotation
                    },
                    upper,
                ),
                (leg.knee_joint, upper, lower),
                (leg.ankle_joint, lower, sole_rotation),
            ] {
                let j = rig.joints[index];
                let desired = math::multiply(
                    math::inverse(math::multiply(parent, j.parent_frame_rotation)),
                    math::multiply(child, j.child_frame_rotation),
                );
                let goal = self.joint_goals.entry(index).or_insert(j.initial_rotation);
                // A stance adjustment follows the gravitational response time.
                // It must not jump from the transferred pose to a flattened sole.
                *goal = if swinging {
                    desired
                } else {
                    math::slerp(
                        *goal,
                        desired,
                        1.0 - (-dt * omega * model.position_response).exp(),
                    )
                };
                result[index] = Some(*goal);
            }
            let _ = kneej;
        }
        self.bind_support_goal(&mut pose);
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inverse_kinematics_keeps_lengths_and_knee_side_at_all_scales() {
        for scale in [0.1, 1.0, 3.0] {
            let (knee, foot) = solve_two_link(
                [0.0, scale, 0.0],
                [0.0, 0.1 * scale, 0.1 * scale],
                [0.0, 0.0, 1.0],
                0.5 * scale,
                0.5 * scale,
                0.999,
            )
            .unwrap();
            assert!(
                (math::length(math::sub(knee, [0.0, scale, 0.0])) - 0.5 * scale).abs()
                    < scale * 0.0001
            );
            assert!((math::length(math::sub(foot, knee)) - 0.5 * scale).abs() < scale * 0.0001);
            assert!(knee[2] > foot[2]);
        }
    }
    #[test]
    fn unreachable_goal_is_clamped_and_degenerate_chain_rejected() {
        let (knee, foot) =
            solve_two_link([0.0; 3], [5.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.4, 0.5, 0.99).unwrap();
        assert!((math::length(foot) - 0.891).abs() < 0.00001);
        assert!((math::length(math::sub(foot, knee)) - 0.5).abs() < 0.00001);
        assert!(
            solve_two_link([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.0, 1.0, 0.99).is_none()
        );
    }
}

#[cfg(test)]
mod hinge_tests {
    use super::*;
    #[test]
    fn three_dimensional_leg_goals_preserve_one_common_knee_hinge() {
        let a = [0.0, -0.5, 0.04];
        let b = [0.0, -0.4, -0.04];
        let original = math::cross(a, b);
        for goal in [[0.2, 0.1, 0.3], [-0.3, 0.2, 0.1], [0.1, 0.0, -0.2]] {
            let (knee, foot) = solve_two_link(
                [0.0, 0.8, 0.0],
                goal,
                [0.0, 0.0, 1.0],
                math::length(a),
                math::length(b),
                0.999,
            )
            .unwrap();
            let u = math::sub(knee, [0.0, 0.8, 0.0]);
            let l = math::sub(foot, knee);
            let expected = math::unit(math::cross(u, l)).unwrap();
            for (from, to) in [(a, u), (b, l)] {
                let q = segment_frame(from, original, to, expected);
                assert!(
                    math::dot(math::unit(math::rotate(q, original)).unwrap(), expected) > 0.99999
                );
                assert!(
                    math::dot(
                        math::unit(math::rotate(q, from)).unwrap(),
                        math::unit(to).unwrap()
                    ) > 0.99999
                );
            }
        }
    }
}

mod ik;
use ik::segment_frame;
pub use ik::solve_two_link;

#[cfg(test)]
mod support_tests;
