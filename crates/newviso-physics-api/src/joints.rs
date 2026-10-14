use super::*;
use std::collections::BTreeSet;

/// A backend-neutral ball socket. Frames are local to each rigid body's visual
/// origin. Equal frame rotations define zero relative angular displacement.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsJointSnapshot {
    pub id: u64,
    pub parent: PhysicsEntityKey,
    pub child: PhysicsEntityKey,
    pub parent_anchor: PhysicsVec3,
    pub child_anchor: PhysicsVec3,
    pub parent_frame_rotation: PhysicsQuat,
    pub child_frame_rotation: PhysicsQuat,
    pub max_force: f32,
    /// Maximum relative angular displacement in radians, in any direction.
    pub cone_limit: f32,
    pub angular_stiffness: f32,
    pub angular_damping: f32,
    /// Bodies in the same nonzero family do not collide with each other.
    /// They still collide with the world, vehicles and other families.
    #[serde(default)]
    pub collision_family: u64,
}

pub(super) fn validate_joints(input: &PhysicsFrameInput) -> Result<(), String> {
    let bodies: BTreeSet<_> = input
        .bodies
        .iter()
        .map(|b| b.entity)
        .chain(input.colliders.iter().map(|c| c.entity))
        .collect();
    let mut ids = BTreeSet::new();
    for joint in &input.joints {
        let valid_quat = |q: PhysicsQuat| {
            let length = q.iter().map(|v| v * v).sum::<f32>();
            q.iter().all(|v| v.is_finite()) && (0.99..=1.01).contains(&length)
        };
        if !ids.insert(joint.id)
            || joint.parent == joint.child
            || !bodies.contains(&joint.parent)
            || !bodies.contains(&joint.child)
            || joint
                .parent_anchor
                .iter()
                .chain(joint.child_anchor.iter())
                .any(|v| !v.is_finite())
            || !valid_quat(joint.parent_frame_rotation)
            || !valid_quat(joint.child_frame_rotation)
            || !joint.max_force.is_finite()
            || joint.max_force <= 0.0
            || !joint.cone_limit.is_finite()
            || !(0.0..=std::f32::consts::PI).contains(&joint.cone_limit)
            || !joint.angular_stiffness.is_finite()
            || joint.angular_stiffness < 0.0
            || !joint.angular_damping.is_finite()
            || joint.angular_damping < 0.0
        {
            return Err(format!("invalid physics joint {}", joint.id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_frames_default_to_no_joints() {
        let value = serde_json::to_value(PhysicsFrameInput::empty(1, 1, 1.0 / 60.0)).unwrap();
        let mut legacy = value;
        legacy.as_object_mut().unwrap().remove("joints");
        let restored: PhysicsFrameInput = serde_json::from_value(legacy).unwrap();
        assert!(restored.joints.is_empty());
    }
    #[test]
    fn dangling_and_nonfinite_joints_are_rejected() {
        let mut input = PhysicsFrameInput::empty(1, 1, 1.0 / 60.0);
        input.joints.push(PhysicsJointSnapshot {
            id: 3,
            parent: 1,
            child: 2,
            parent_anchor: [0.0; 3],
            child_anchor: [0.0; 3],
            parent_frame_rotation: [0.0, 0.0, 0.0, 1.0],
            child_frame_rotation: [0.0, 0.0, 0.0, 1.0],
            max_force: 1000.0,
            cone_limit: 1.0,
            angular_stiffness: 10.0,
            angular_damping: 1.0,
            collision_family: 1,
        });
        assert!(validate_joints(&input).is_err());
        input.joints[0].max_force = f32::NAN;
        assert!(validate_joints(&input).is_err());
    }
}
