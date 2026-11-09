use super::*;
impl Scene3dRuntime {
    /// A separate channel prevents gameplay weapon updates from erasing a reflex.
    pub fn set_entity_protective_arm_ik(
        &mut self,
        id: u64,
        constraints: Vec<SceneArmIkConstraint>,
    ) -> Result<(), String> {
        self.set_arm_ik_channel(id, constraints, true)
    }
    pub(super) fn set_arm_ik_channel(
        &mut self,
        id: u64,
        constraints: Vec<SceneArmIkConstraint>,
        protective: bool,
    ) -> Result<(), String> {
        if self.world.entity(SceneEntityId(id)).is_none() {
            return Err(format!("arm IK entity {id} does not exist"));
        }
        validate_arm_constraints(&constraints)?;
        if constraints.is_empty() {
            if protective {
                self.protective_arm_ik_constraints.remove(&id);
            } else {
                self.arm_ik_constraints.remove(&id);
            }
        } else {
            let entity = self.world.entity(SceneEntityId(id)).unwrap();
            let transform = entity.transform;
            let bounds = extend_bounds(entity.bounds, &constraints);
            self.world.update_spatial_from(
                SceneEntityId(id),
                transform,
                bounds,
                SceneMutationSource::Animation,
            )?;
            if protective {
                self.protective_arm_ik_constraints.insert(id, constraints);
            } else {
                self.arm_ik_constraints.insert(id, constraints);
            }
        }
        Ok(())
    }
    pub(crate) fn combined_arm_ik(&self, id: u64) -> Vec<SceneArmIkConstraint> {
        let mut out = merge_arm_channels(
            self.arm_ik_constraints
                .get(&id)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            self.protective_arm_ik_constraints
                .get(&id)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        );
        out.extend(
            self.leg_ik_constraints
                .get(&id)
                .into_iter()
                .flatten()
                .map(super::super::leg_ik::SceneLegIkConstraint::solver_constraint),
        );
        out
    }
}

pub(crate) fn validate_arm_constraints(constraints: &[SceneArmIkConstraint]) -> Result<(), String> {
    if constraints.iter().any(|c| {
        c.joint_rotations.len() > 32
            || c.joint_rotations
                .iter()
                .any(|(name, r)| name.trim().is_empty() || r.iter().any(|v| !v.is_finite()))
    }) {
        return Err("arm IK contains invalid hand pose joints".into());
    }
    if constraints.len() > 2 {
        return Err("arm IK supports at most two arms".into());
    }
    for c in constraints {
        if [&c.upper_joint, &c.lower_joint, &c.hand_joint, &c.tip_joint]
            .iter()
            .any(|s| s.trim().is_empty())
            || c.target
                .iter()
                .chain(c.pole.iter())
                .chain(c.rotation_degrees.iter())
                .any(|v| !v.is_finite())
            || !c.hand_pose_weight.is_finite()
            || !(0.0..=1.0).contains(&c.hand_pose_weight)
            || !c.weight.is_finite()
            || !(0.0..=1.0).contains(&c.weight)
        {
            return Err(
                "arm IK requires finite targets, valid joint names and weight in 0..1".into(),
            );
        }
    }

    Ok(())
}
pub(super) fn merge_arm_channels(
    base: &[SceneArmIkConstraint],
    protective: &[SceneArmIkConstraint],
) -> Vec<SceneArmIkConstraint> {
    let mut out = base.to_vec();
    for p in protective.iter().filter(|p| p.weight > f32::EPSILON) {
        let Some(index) = out
            .iter()
            .position(|b| b.hand_joint.eq_ignore_ascii_case(&p.hand_joint))
        else {
            let mut goal = p.clone();
            goal.hand_pose_weight *= p.weight;
            out.push(goal);
            continue;
        };
        let b = &out[index];
        let mut c = p.clone();
        let normal = b.weight * (1.0 - p.weight);
        let total = normal + p.weight;
        let blend = p.weight / total.max(f32::EPSILON);
        c.weight = total;
        c.target = std::array::from_fn(|i| b.target[i] + (p.target[i] - b.target[i]) * blend);
        c.pole = std::array::from_fn(|i| b.pole[i] + (p.pole[i] - b.pole[i]) * blend);
        let quat = |r: [f32; 3]| {
            rotation_quat(geometry::instance_model_matrix(
                Vec3::ZERO,
                Vec3::new(r[0], r[1], r[2]),
                Vec3::ONE,
            ))
        };
        c.rotation_degrees = matrix_rotation_degrees(trs_matrix(
            [0.0; 3],
            slerp_quat(quat(b.rotation_degrees), quat(p.rotation_degrees), blend),
            [1.0; 3],
        ))
        .unwrap_or(p.rotation_degrees);
        c.hand_pose_weight = b.hand_pose_weight * (1.0 - p.weight) + p.hand_pose_weight * p.weight;
        let hand_blend = p.hand_pose_weight * p.weight / c.hand_pose_weight.max(f32::EPSILON);
        let mut joints = std::collections::BTreeMap::new();
        for (name, r) in &b.joint_rotations {
            joints.insert(name.clone(), (*r, [0.0; 3]));
        }
        for (name, r) in &p.joint_rotations {
            joints.entry(name.clone()).or_insert(([0.0; 3], [0.0; 3])).1 = *r;
        }
        c.joint_rotations = joints
            .into_iter()
            .map(|(n, (a, b))| {
                (
                    n,
                    std::array::from_fn(|i| {
                        let delta = (b[i] - a[i] + 180.0).rem_euclid(360.0) - 180.0;
                        a[i] + delta * hand_blend
                    }),
                )
            })
            .collect();
        if p.hand_pose_weight <= f32::EPSILON {
            c.hand_pose_weight = b.hand_pose_weight;
            c.joint_rotations = b.joint_rotations.clone();
        }
        out[index] = c;
    }
    out
}
#[cfg(test)]
mod channel_tests {
    use super::*;
    fn arm(name: &str, target: [f32; 3]) -> SceneArmIkConstraint {
        SceneArmIkConstraint {
            upper_joint: "u".into(),
            lower_joint: "e".into(),
            hand_joint: name.into(),
            tip_joint: name.into(),
            target,
            pole: [0.0, 1.0, 0.0],
            rotation_degrees: [0.0; 3],
            weight: 1.0,
            hand_pose_weight: 1.0,
            joint_rotations: vec![("finger".into(), [60.0, 0.0, 0.0])],
        }
    }
    #[test]
    fn protection_reserves_only_its_arm_and_restores_the_unchanged_weapon_channel() {
        let base = vec![arm("right", [1.0, 0.0, 0.0]), arm("left", [-1.0, 0.0, 0.0])];
        let mut protection = arm("right", [0.0, 1.0, 0.0]);
        protection.joint_rotations[0].1 = [0.0; 3];
        let out = merge_arm_channels(&base, &[protection.clone()]);
        assert_eq!(out[0].target, protection.target);
        assert_eq!(out[0].joint_rotations[0].1, [0.0; 3]);
        assert_eq!(out[1], base[1]);
        protection.weight = 0.25;
        let out = merge_arm_channels(&base, &[protection]);
        assert_eq!(out[0].target, [0.75, 0.25, 0.0]);
        assert_eq!(out[0].joint_rotations[0].1, [45.0, 0.0, 0.0]);
        assert_eq!(merge_arm_channels(&base, &[]), base);
    }
    #[test]
    fn protective_arms_preserve_the_grip_of_a_held_weapon() {
        let base = vec![arm("right", [1.0, 0.0, 0.0])];
        let mut protection = arm("right", [0.0, 1.0, 0.0]);
        protection.hand_pose_weight = 0.0;
        protection.joint_rotations.clear();
        let out = merge_arm_channels(&base, &[protection]);
        assert_eq!(out[0].target, [0.0, 1.0, 0.0]);
        assert_eq!(out[0].joint_rotations, base[0].joint_rotations);
        assert_eq!(out[0].hand_pose_weight, base[0].hand_pose_weight);
    }
}
