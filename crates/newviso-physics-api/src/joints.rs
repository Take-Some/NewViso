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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angular_limits: Option<PhysicsAngularLimits>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motor: Option<PhysicsJointMotor>,
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
            || joint.angular_limits.is_some_and(|v| !v.valid())
            || joint.motor.is_some_and(|v| !v.valid())
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
            angular_limits: None,
            motor: None,
        });
        assert!(validate_joints(&input).is_err());
        input.joints[0].max_force = f32::NAN;
        assert!(validate_joints(&input).is_err());
    }
}

/// Limits in the joint frame: Y is twist, X/Z span the swing plane.
/// Stops are compliant physical torques; they never overwrite body poses.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsAngularLimits {
    pub swing_x: [f32; 2],
    pub swing_z: [f32; 2],
    pub twist: [f32; 2],
    pub stiffness: f32,
    pub damping: f32,
    pub max_torque: f32,
}
impl PhysicsAngularLimits {
    pub fn valid(self) -> bool {
        [self.swing_x, self.swing_z, self.twist].iter().all(|r| {
            r.iter()
                .all(|v| v.is_finite() && v.abs() <= std::f32::consts::PI)
                && r[0] <= 0.0
                && r[1] >= 0.0
        }) && [self.stiffness, self.damping, self.max_torque]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0)
    }
}
/// An internal equal/opposite angular drive, in Nm/rad, Nm*s/rad and Nm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsJointMotor {
    /// World goal, actuated by this same internal couple and effort budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_drive: Option<PhysicsWorldAngularDrive>,
    /// Internal child torque in the parent joint frame; shares the motor torque budget.
    #[serde(default)]
    pub feed_forward_torque: PhysicsVec3,
    pub target_rotation: PhysicsQuat,
    pub stiffness: f32,
    pub damping: f32,
    pub max_torque: f32,
}
impl PhysicsJointMotor {
    pub fn valid(self) -> bool {
        self.world_drive.is_none_or(|g| g.valid())
            && self.feed_forward_torque.iter().all(|v| v.is_finite())
            && self.target_rotation.iter().all(|v| v.is_finite())
            && (self.target_rotation.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() <= 0.01
            && [self.stiffness, self.damping, self.max_torque]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0)
    }
}

/// The other body receives the opposite reaction, including when unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsWorldAngularDrive {
    #[serde(default)]
    pub body: PhysicsJointBody,
    pub target_rotation: PhysicsQuat,
    pub stiffness: f32,
    pub damping: f32,
}
impl PhysicsWorldAngularDrive {
    pub fn valid(self) -> bool {
        self.target_rotation.iter().all(|v| v.is_finite())
            && (self.target_rotation.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() <= 0.01
            && [self.stiffness, self.damping]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0)
    }
}

#[cfg(test)]
mod world_drive_validation_tests {
    use super::*;
    #[test]
    fn legacy_motor_and_invalid_parent_orientation_are_handled() {
        let raw = r#"{"target_rotation":[0,0,0,1],"stiffness":5,"damping":1,"max_torque":10}"#;
        let mut motor: PhysicsJointMotor = serde_json::from_str(raw).unwrap();
        assert!(motor.world_drive.is_none() && motor.valid());
        motor.world_drive = Some(PhysicsWorldAngularDrive {
            body: PhysicsJointBody::Parent,
            target_rotation: [0.0; 4],
            stiffness: 1.0,
            damping: 1.0,
        });
        assert!(!motor.valid());
        motor.world_drive.as_mut().unwrap().target_rotation = [0.0, 0.0, 0.0, 1.0];
        assert!(motor.valid());
        motor.world_drive.as_mut().unwrap().damping = f32::NAN;
        assert!(!motor.valid());
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicsJointBody {
    #[default]
    Parent,
    Child,
}
