use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct SceneArmIkConstraint {
    pub upper_joint: String,
    pub lower_joint: String,
    pub hand_joint: String,
    pub tip_joint: String,
    pub target: [f32; 3],
    pub pole: [f32; 3],
    pub rotation_degrees: [f32; 3],
    pub weight: f32,
    /// Finger grip remains active independently of the raised-arm aim blend.
    pub hand_pose_weight: f32,
    /// Local rotation offsets from the authored bind pose, restricted to the hand subtree.
    pub joint_rotations: Vec<(String, [f32; 3])>,
}

impl Scene3dRuntime {
    pub fn set_entity_arm_ik(
        &mut self,
        id: u64,
        constraints: Vec<SceneArmIkConstraint>,
    ) -> Result<(), String> {
        if self.world.entity(SceneEntityId(id)).is_none() {
            return Err(format!("arm IK entity {id} does not exist"));
        }
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
        for c in &constraints {
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
        if constraints.is_empty() {
            self.arm_ik_constraints.remove(&id);
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
            self.arm_ik_constraints.insert(id, constraints);
        }
        Ok(())
    }

    pub(super) fn model_arm_ik(&self, id: u64) -> Result<Vec<SceneArmIkConstraint>, String> {
        let Some(constraints) = self.arm_ik_constraints.get(&id) else {
            return Ok(Vec::new());
        };
        let e = self
            .world
            .entity(SceneEntityId(id))
            .ok_or("arm IK entity disappeared")?;
        let model = geometry::instance_model_matrix(
            e.transform.position,
            e.transform.rotation_degrees,
            e.transform.scale,
        );
        let inverse = inverse_affine(model).ok_or("arm IK entity has singular transform")?;
        constraints
            .iter()
            .map(|c| {
                let rotation = geometry::instance_model_matrix(
                    Vec3::ZERO,
                    Vec3::new(
                        c.rotation_degrees[0],
                        c.rotation_degrees[1],
                        c.rotation_degrees[2],
                    ),
                    Vec3::new(1.0, 1.0, 1.0),
                );
                let target =
                    transform_point_mat4(inverse, Vec3::new(c.target[0], c.target[1], c.target[2]));
                let pole =
                    transform_point_mat4(inverse, Vec3::new(c.pole[0], c.pole[1], c.pole[2]));
                Ok(SceneArmIkConstraint {
                    target: [target.x, target.y, target.z],
                    pole: [pole.x, pole.y, pole.z],
                    rotation_degrees: matrix_rotation_degrees(mul_mat4(inverse, rotation))?,
                    ..c.clone()
                })
            })
            .collect()
    }

    pub fn entity_arm_ik_snapshot(&self) -> serde_json::Value {
        serde_json::Value::Array(self.arm_ik_constraints.iter().map(|(id, constraints)| {
            serde_json::json!({"entity": id, "arms": constraints.iter().map(|c| {
                let tip_result = self.entity_joint_world_position(*id, &c.tip_joint);
                let tip = tip_result.as_ref().ok().copied();
                let shoulder = self.entity_joint_world_position(*id, &c.upper_joint).ok();
                serde_json::json!({"hand":c.hand_joint,"tip":c.tip_joint,"target":c.target,
                    "position":tip,"shoulder":shoulder,"weight":c.weight,"hand_pose_weight":c.hand_pose_weight,
                    "pose_error":tip_result.err(),
                    "error":tip.map(|p| ((p[0]-c.target[0]).powi(2)+(p[1]-c.target[1]).powi(2)+(p[2]-c.target[2]).powi(2)).sqrt())})
            }).collect::<Vec<_>>()})
        }).collect())
    }
}

/// The actual deformed hands can extend beyond the immutable bind-pose AABB.
pub(crate) fn extend_bounds(
    mut bounds: SceneBounds,
    constraints: &[SceneArmIkConstraint],
) -> SceneBounds {
    for c in constraints.iter().filter(|c| c.weight > 0.0001) {
        let p = Vec3::new(c.target[0], c.target[1], c.target[2]);
        let margin = Vec3::new(0.12, 0.12, 0.12);
        let min = p.sub(margin);
        let max = p.add(margin);
        bounds.min = Vec3::new(
            bounds.min.x.min(min.x),
            bounds.min.y.min(min.y),
            bounds.min.z.min(min.z),
        );
        bounds.max = Vec3::new(
            bounds.max.x.max(max.x),
            bounds.max.y.max(max.y),
            bounds.max.z.max(max.z),
        );
    }
    bounds
}

pub(super) fn apply_to_palette(
    palette: &mut [[f32; 16]],
    skeleton: &ModelSkeleton,
    drivers: &[Option<DerivedJointDriver>],
    source_to_model: [f32; 16],
    constraints: &[SceneArmIkConstraint],
) -> Result<(), String> {
    if constraints.is_empty() {
        return Ok(());
    }
    let mut globals = Vec::with_capacity(palette.len());
    let mut inverse_binds = Vec::with_capacity(palette.len());
    for (p, joint) in palette.iter().zip(&skeleton.joints) {
        let bind =
            inverse_affine(joint.inverse_bind_matrix).ok_or("arm IK singular inverse bind")?;
        let bind_model = mul_mat4(source_to_model, bind);
        globals.push(mul_mat4(*p, bind_model));
        inverse_binds.push(inverse_affine(bind_model).ok_or("arm IK singular model bind")?);
    }
    apply_to_globals(&mut globals, skeleton, drivers, constraints)?;
    for (i, p) in palette.iter_mut().enumerate() {
        *p = mul_mat4(globals[i], inverse_binds[i]);
    }
    Ok(())
}

pub(super) fn apply_to_globals(
    globals: &mut [[f32; 16]],
    skeleton: &ModelSkeleton,
    drivers: &[Option<DerivedJointDriver>],
    constraints: &[SceneArmIkConstraint],
) -> Result<(), String> {
    for c in constraints {
        if c.weight <= 0.0001 && c.hand_pose_weight <= 0.0001 {
            continue;
        }
        let index = |name: &str| {
            skeleton
                .joints
                .iter()
                .position(|j| j.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("arm IK skeleton '{}' has no joint '{name}'", skeleton.name))
        };
        let upper = index(&c.upper_joint)?;
        let lower = index(&c.lower_joint)?;
        let hand = index(&c.hand_joint)?;
        let tip = index(&c.tip_joint)?;
        if skeleton.joints[lower].parent != Some(upper as u16)
            || skeleton.joints[hand].parent != Some(lower as u16)
        {
            return Err("arm IK requires an upper-arm -> forearm -> hand chain".into());
        }
        if c.weight > 0.0001 {
            let desired_socket = geometry::instance_model_matrix(
                Vec3::ZERO,
                Vec3::new(
                    c.rotation_degrees[0],
                    c.rotation_degrees[1],
                    c.rotation_degrees[2],
                ),
                Vec3::new(1.0, 1.0, 1.0),
            );
            let hand_inverse = inverse_affine(globals[hand]).ok_or("arm IK singular hand")?;
            let socket_local = mul_mat4(hand_inverse, globals[tip]);
            let socket_q = rotation_quat(socket_local);
            let desired = mul_mat4(
                desired_socket,
                trs_matrix(
                    [0.0; 3],
                    [-socket_q[0], -socket_q[1], -socket_q[2], socket_q[3]],
                    [1.0; 3],
                ),
            );
            let offset = transform_point_mat4(hand_inverse, pos(globals[tip]));
            let offset = transform_vector_mat4(desired, offset);
            let target = Vec3::new(c.target[0], c.target[1], c.target[2]).sub(offset);
            let shoulder = pos(globals[upper]);
            let elbow = pos(globals[lower]);
            let wrist = pos(globals[hand]);
            let l1 = elbow.sub(shoulder).length();
            let l2 = wrist.sub(elbow).length();
            if l1 < 1.0e-5 || l2 < 1.0e-5 {
                return Err("arm IK contains zero-length bones".into());
            }
            let raw = target.sub(shoulder);
            let direction = if raw.length() > 1.0e-6 {
                raw.normalized()
            } else {
                elbow.sub(shoulder).normalized()
            };
            let distance = raw.length().clamp((l1 - l2).abs() + 0.001, l1 + l2 - 0.001);
            let reachable = shoulder.add(direction.mul(distance));
            let along = (l1 * l1 - l2 * l2 + distance * distance) / (2.0 * distance);
            let height = (l1 * l1 - along * along).max(0.0).sqrt();
            let pole = Vec3::new(c.pole[0], c.pole[1], c.pole[2]).sub(shoulder);
            let mut bend = pole.sub(direction.mul(pole.dot(direction)));
            if bend.length() < 1.0e-5 {
                let axis = if direction.y.abs() < 0.9 {
                    Vec3::new(0.0, 1.0, 0.0)
                } else {
                    Vec3::new(1.0, 0.0, 0.0)
                };
                bend = axis.sub(direction.mul(axis.dot(direction)));
            }
            let desired_elbow = shoulder
                .add(direction.mul(along))
                .add(bend.normalized().mul(height));
            let q = from_to(elbow.sub(shoulder), desired_elbow.sub(shoulder));
            rotate_branch(
                globals,
                skeleton,
                drivers,
                upper,
                shoulder,
                slerp_quat([0.0, 0.0, 0.0, 1.0], q, c.weight),
            );
            let q = from_to(
                pos(globals[hand]).sub(pos(globals[lower])),
                reachable.sub(pos(globals[lower])),
            );
            rotate_branch(
                globals,
                skeleton,
                drivers,
                lower,
                pos(globals[lower]),
                slerp_quat([0.0, 0.0, 0.0, 1.0], q, c.weight),
            );
            let current = rotation_quat(globals[hand]);
            let wanted = rotation_quat(desired);
            let delta = qmul(wanted, [-current[0], -current[1], -current[2], current[3]]);
            rotate_branch(
                globals,
                skeleton,
                drivers,
                hand,
                pos(globals[hand]),
                slerp_quat([0.0, 0.0, 0.0, 1.0], delta, c.weight),
            );
        }
        for (name, offset) in &c.joint_rotations {
            let finger = index(name)?;
            let mut ancestor = skeleton.joints[finger].parent;
            while ancestor.is_some_and(|p| p as usize != hand) {
                ancestor = skeleton.joints[ancestor.unwrap() as usize].parent;
            }
            if ancestor.is_none() || finger == hand {
                return Err("arm IK hand pose may only rotate hand descendants".into());
            }
            let joint = &skeleton.joints[finger];
            let parent = joint.parent.unwrap() as usize;
            let offset = geometry::instance_model_matrix(
                Vec3::ZERO,
                Vec3::new(offset[0], offset[1], offset[2]),
                Vec3::new(1.0, 1.0, 1.0),
            );
            let rotation = qmul(joint.bind_rotation, rotation_quat(offset));
            let wanted = mul_mat4(
                globals[parent],
                trs_matrix(joint.bind_translation, rotation, joint.bind_scale),
            );
            let current = rotation_quat(globals[finger]);
            let delta = qmul(
                rotation_quat(wanted),
                [-current[0], -current[1], -current[2], current[3]],
            );
            rotate_branch(
                globals,
                skeleton,
                drivers,
                finger,
                pos(globals[finger]),
                slerp_quat([0.0, 0.0, 0.0, 1.0], delta, c.hand_pose_weight),
            );
        }
    }
    Ok(())
}

fn pos(m: [f32; 16]) -> Vec3 {
    Vec3::new(m[12], m[13], m[14])
}
fn from_to(a: Vec3, b: Vec3) -> [f32; 4] {
    let a = a.normalized();
    let b = b.normalized();
    let dot = a.dot(b).clamp(-1.0, 1.0);
    if dot < -0.9999 {
        let axis = if a.x.abs() < 0.8 {
            a.cross(Vec3::new(1.0, 0.0, 0.0))
        } else {
            a.cross(Vec3::new(0.0, 1.0, 0.0))
        }
        .normalized();
        return [axis.x, axis.y, axis.z, 0.0];
    }
    let cross = a.cross(b);
    normalize_quat([cross.x, cross.y, cross.z, 1.0 + dot])
}
fn qmul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    normalize_quat([
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ])
}
fn rotation_quat(m: [f32; 16]) -> [f32; 4] {
    let sx = Vec3::new(m[0], m[1], m[2]).length().max(1.0e-8);
    let sy = Vec3::new(m[4], m[5], m[6]).length().max(1.0e-8);
    let sz = Vec3::new(m[8], m[9], m[10]).length().max(1.0e-8);
    let (a, b, c, d, e, f, g, h, i) = (
        m[0] / sx,
        m[4] / sy,
        m[8] / sz,
        m[1] / sx,
        m[5] / sy,
        m[9] / sz,
        m[2] / sx,
        m[6] / sy,
        m[10] / sz,
    );
    let q = if a + e + i > 0.0 {
        let s = (a + e + i + 1.0).sqrt() * 2.0;
        [(h - f) / s, (c - g) / s, (d - b) / s, s * 0.25]
    } else if a > e && a > i {
        let s = (1.0 + a - e - i).max(0.0).sqrt() * 2.0;
        [s * 0.25, (b + d) / s, (c + g) / s, (h - f) / s]
    } else if e > i {
        let s = (1.0 + e - a - i).max(0.0).sqrt() * 2.0;
        [(b + d) / s, s * 0.25, (f + h) / s, (c - g) / s]
    } else {
        let s = (1.0 + i - a - e).max(0.0).sqrt() * 2.0;
        [(c + g) / s, (f + h) / s, s * 0.25, (d - b) / s]
    };
    normalize_quat(q)
}
fn rotate_branch(
    globals: &mut [[f32; 16]],
    skeleton: &ModelSkeleton,
    drivers: &[Option<DerivedJointDriver>],
    root: usize,
    pivot: Vec3,
    rotation: [f32; 4],
) {
    let mut delta = trs_matrix([0.0; 3], rotation, [1.0; 3]);
    let p = transform_vector_mat4(delta, pivot);
    delta[12] = pivot.x - p.x;
    delta[13] = pivot.y - p.y;
    delta[14] = pivot.z - p.z;
    let mut affected = vec![false; skeleton.joints.len()];
    for i in 0..skeleton.joints.len() {
        affected[i] = i == root
            || skeleton.joints[i]
                .parent
                .is_some_and(|p| affected[p as usize])
            || drivers[i].is_some_and(|d| affected[d.joint as usize]);
        if affected[i] {
            globals[i] = mul_mat4(delta, globals[i]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use newviso_model::ModelJoint;
    #[test]
    fn arm_reaches_tip_without_stretching_and_preserves_other_bones() {
        let mut joints = Vec::new();
        let translations = [
            [0.0, 0.0, 0.0],
            [0.4, 0.0, 0.0],
            [0.35, 0.0, 0.0],
            [0.04, 0.0, 0.0],
            [0.0, -1.0, 0.0],
        ];
        let mut globals = Vec::new();
        for (i, t) in translations.into_iter().enumerate() {
            let parent = match i {
                0 | 4 => None,
                _ => Some((i - 1) as u16),
            };
            let rotation = if i == 3 {
                [
                    0.0,
                    std::f32::consts::FRAC_1_SQRT_2,
                    0.0,
                    std::f32::consts::FRAC_1_SQRT_2,
                ]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            };
            let local = trs_matrix(t, rotation, [1.0; 3]);
            let global = parent.map_or(local, |p| mul_mat4(globals[p as usize], local));
            globals.push(global);
            joints.push(ModelJoint {
                name: format!("j{i}"),
                tag: i as u32,
                parent,
                inverse_bind_matrix: inverse_affine(global).unwrap(),
                bind_translation: t,
                bind_rotation: rotation,
                bind_scale: [1.0; 3],
            });
        }
        let skeleton = ModelSkeleton {
            name: "test".into(),
            joints,
        };
        let other = globals[4];
        let c = SceneArmIkConstraint {
            upper_joint: "j0".into(),
            lower_joint: "j1".into(),
            hand_joint: "j2".into(),
            tip_joint: "j3".into(),
            target: [0.3, 0.4, -0.3],
            pole: [0.0, -1.0, 0.0],
            rotation_degrees: [0.0; 3],
            weight: 1.0,
            hand_pose_weight: 1.0,
            joint_rotations: Vec::new(),
        };
        apply_to_globals(&mut globals, &skeleton, &vec![None; 5], &[c]).unwrap();
        assert!(pos(globals[3]).sub(Vec3::new(0.3, 0.4, -0.3)).length() < 1.0e-4);
        assert!((pos(globals[1]).sub(pos(globals[0])).length() - 0.4).abs() < 1.0e-4);
        assert!((pos(globals[2]).sub(pos(globals[1])).length() - 0.35).abs() < 1.0e-4);
        assert_eq!(globals[4], other);
        assert!(
            Vec3::new(globals[3][8], globals[3][9], globals[3][10])
                .sub(Vec3::new(0.0, 0.0, 1.0))
                .length()
                < 1.0e-4
        );
    }
    #[test]
    fn equipped_fingers_hold_grip_after_arms_return_to_locomotion() {
        let translations = [
            [0.0; 3],
            [0.3, 0.0, 0.0],
            [0.25, 0.0, 0.0],
            [0.04, 0.0, 0.0],
            [0.08, 0.0, 0.0],
            [0.04, 0.0, 0.0],
        ];
        let parents = [None, Some(0), Some(1), Some(2), Some(2), Some(4)];
        let mut globals = Vec::new();
        let mut joints = Vec::new();
        for (i, t) in translations.into_iter().enumerate() {
            let local = trs_matrix(t, [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
            let global = parents[i].map_or(local, |p| mul_mat4(globals[p as usize], local));
            globals.push(global);
            joints.push(ModelJoint {
                name: format!("j{i}"),
                tag: i as u32,
                parent: parents[i],
                inverse_bind_matrix: inverse_affine(global).unwrap(),
                bind_translation: t,
                bind_rotation: [0.0, 0.0, 0.0, 1.0],
                bind_scale: [1.0; 3],
            });
        }
        let before = globals.clone();
        let skeleton = ModelSkeleton {
            name: "grip".into(),
            joints,
        };
        let c = SceneArmIkConstraint {
            upper_joint: "j0".into(),
            lower_joint: "j1".into(),
            hand_joint: "j2".into(),
            tip_joint: "j3".into(),
            target: [10.0; 3],
            pole: [0.0, -1.0, 0.0],
            rotation_degrees: [90.0; 3],
            weight: 0.0,
            hand_pose_weight: 1.0,
            joint_rotations: vec![("j4".into(), [0.0, 60.0, 0.0])],
        };
        apply_to_globals(&mut globals, &skeleton, &vec![None; 6], &[c]).unwrap();
        assert_eq!(
            &globals[..4],
            &before[..4],
            "released arms and weapon socket keep their locomotion pose"
        );
        assert!(pos(globals[4]).sub(pos(before[4])).length() < 1e-6);
        assert!((pos(globals[5]).sub(pos(globals[4])).length() - 0.04).abs() < 1e-6);
        assert!(
            pos(globals[5]).z < -0.03,
            "finger curls around grip independently of arm IK"
        );
    }
}
