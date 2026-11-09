//! The same protective hand goals become bounded articulated joint targets.
use super::*;
use crate::{BodyPose, RigState};
impl RigState {
    pub fn protection_targets(
        &self,
        profile: &ProtectionProfile,
        state: &ProtectionState,
        mut pose: impl FnMut(u64) -> Option<BodyPose>,
    ) -> Vec<Option<([f32; 4], f32)>> {
        let mut result = vec![None; self.joints.len()];
        for goal in &state.arms {
            let Some(binding) = profile.arms.get(goal.arm) else {
                continue;
            };
            let part = |name: &str| self.parts.iter().position(|p| p.joint == name);
            let Some(((upper, lower), hand)) = part(&binding.upper_joint)
                .zip(part(&binding.lower_joint))
                .zip(part(&binding.hand_joint))
            else {
                continue;
            };
            let joint = |part| self.joints.iter().position(|j| j.child_part == part);
            let Some(((uj, lj), hj)) = joint(upper).zip(joint(lower)).zip(joint(hand)) else {
                continue;
            };
            let positions = |part: usize, p: BodyPose| {
                math::position(math::product(
                    math::matrix(p.position, p.rotation),
                    self.parts[part].body_to_joint,
                ))
            };
            let Some(((u, l), h)) = pose(self.parts[upper].entity)
                .zip(pose(self.parts[lower].entity))
                .zip(pose(self.parts[hand].entity))
            else {
                continue;
            };
            let shoulder = positions(upper, u);
            let elbow = positions(lower, l);
            let wrist = positions(hand, h);
            let bend = math::sub(goal.pole, shoulder);
            let Some((desired_elbow, desired_wrist)) = crate::solve_two_link(
                shoulder,
                goal.wrist,
                bend,
                math::length(math::sub(elbow, shoulder)),
                math::length(math::sub(wrist, elbow)),
                profile.tuning.reach_fraction,
            ) else {
                continue;
            };
            let upper_rotation = math::multiply(
                math::align_axis(
                    math::sub(elbow, shoulder),
                    math::sub(desired_elbow, shoulder),
                ),
                u.rotation,
            );
            let lower_rotation = math::multiply(
                math::align_axis(
                    math::sub(wrist, elbow),
                    math::sub(desired_wrist, desired_elbow),
                ),
                l.rotation,
            );
            let hand_rotation = math::multiply(
                goal.hand_rotation,
                math::inverse(math::rotation(self.parts[hand].body_to_joint)),
            );
            let Some(root) = pose(self.joints[uj].parent) else {
                continue;
            };
            for (index, parent, child) in [
                (uj, root.rotation, upper_rotation),
                (lj, upper_rotation, lower_rotation),
                (hj, lower_rotation, hand_rotation),
            ] {
                let j = self.joints[index];
                let target = math::multiply(
                    math::inverse(math::multiply(parent, j.parent_frame_rotation)),
                    math::multiply(child, j.child_frame_rotation),
                );
                result[index] = Some((target, goal.weight));
            }
        }
        result
    }
}
