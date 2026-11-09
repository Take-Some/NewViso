//! A revolute joint assembled from two native positional sockets.
//! It uses the existing JointConstraints/AngularLimits protocol, with no backend ABI extension.
use crate::*;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsHinge {
    pub joint_ids: [u64; 2],
    pub parent: u64,
    pub child: u64,
    pub parent_pivot: PhysicsVec3,
    pub child_pivot: PhysicsVec3,
    pub parent_frame_rotation: PhysicsQuat,
    pub child_frame_rotation: PhysicsQuat,
    pub span: f32,
    pub max_force: f32,
    pub collision_family: u64,
    pub angle_limits: [f32; 2],
    pub stop_stiffness: f32,
    pub stop_damping: f32,
    pub stop_max_torque: f32,
    #[serde(default)]
    pub motor: Option<PhysicsJointMotor>,
}
fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let t = cross([q[0], q[1], q[2]], v).map(|v| v * 2.0);
    let c = cross([q[0], q[1], q[2]], t);
    std::array::from_fn(|i| v[i] + q[3] * t[i] + c[i])
}
impl PhysicsHinge {
    pub fn expand(&self) -> Result<[PhysicsJointSnapshot; 2], String> {
        let quat = |q: PhysicsQuat| {
            q.iter().all(|v| v.is_finite())
                && (q.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() < 0.01
        };
        let limits = PhysicsAngularLimits {
            swing_x: [0.0; 2],
            swing_z: [0.0; 2],
            twist: self.angle_limits,
            stiffness: self.stop_stiffness,
            damping: self.stop_damping,
            max_torque: self.stop_max_torque,
        };
        if self.joint_ids[0] == self.joint_ids[1]
            || self.parent == self.child
            || !self
                .parent_pivot
                .iter()
                .chain(&self.child_pivot)
                .all(|v| v.is_finite())
            || !quat(self.parent_frame_rotation)
            || !quat(self.child_frame_rotation)
            || !self.span.is_finite()
            || self.span <= 0.0
            || !self.max_force.is_finite()
            || self.max_force <= 0.0
            || !limits.valid()
            || self.motor.is_some_and(|m| !m.valid())
        {
            return Err("invalid hinge anchors, frames, IDs, span or effort limits".into());
        }
        Ok(std::array::from_fn(|i| {
            let offset = [
                0.0,
                if i == 0 {
                    -self.span * 0.5
                } else {
                    self.span * 0.5
                },
                0.0,
            ];
            let a = rotate(self.parent_frame_rotation, offset);
            let b = rotate(self.child_frame_rotation, offset);
            PhysicsJointSnapshot {
                id: self.joint_ids[i],
                parent: self.parent,
                child: self.child,
                parent_anchor: std::array::from_fn(|i| self.parent_pivot[i] + a[i]),
                child_anchor: std::array::from_fn(|i| self.child_pivot[i] + b[i]),
                parent_frame_rotation: self.parent_frame_rotation,
                child_frame_rotation: self.child_frame_rotation,
                max_force: self.max_force,
                cone_limit: std::f32::consts::PI,
                angular_stiffness: 0.0,
                angular_damping: 0.0,
                collision_family: self.collision_family,
                angular_limits: (i == 0).then_some(limits),
                motor: if i == 0 { self.motor } else { None },
            }
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn hinge() -> PhysicsHinge {
        PhysicsHinge {
            joint_ids: [30, 31],
            parent: 1,
            child: 2,
            parent_pivot: [0.0; 3],
            child_pivot: [-0.45, 0.0, 0.0],
            parent_frame_rotation: [0.0, 0.0, 0.0, 1.0],
            child_frame_rotation: [0.0, 0.0, 0.0, 1.0],
            span: 1.6,
            max_force: 30000.0,
            collision_family: 21,
            angle_limits: [-1.6, 1.6],
            stop_stiffness: 1000.0,
            stop_damping: 10.0,
            stop_max_torque: 500.0,
            motor: None,
        }
    }
    #[test]
    fn sockets_retain_separation_and_leave_only_the_authored_twist_axis() {
        let h = hinge();
        let j = h.expand().unwrap();
        assert_eq!(j[0].parent_anchor, [0.0, -0.8, 0.0]);
        assert_eq!(j[1].child_anchor, [-0.45, 0.8, 0.0]);
        assert_eq!(j[0].angular_limits.unwrap().twist, h.angle_limits);
        assert!(j[1].angular_limits.is_none() && j[1].motor.is_none());
    }
    #[test]
    fn rotated_hinges_and_invalid_updates_are_not_silently_substituted() {
        let mut h = hinge();
        h.parent_frame_rotation = [
            0.0,
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        ];
        let j = h.expand().unwrap();
        assert!((j[0].parent_anchor[0] - 0.8).abs() < 1e-6);
        h.span = 0.0;
        assert!(h.expand().is_err());
        h = hinge();
        h.joint_ids = [30, 30];
        assert!(h.expand().is_err());
        h = hinge();
        h.angle_limits = [0.1, 1.0];
        assert!(h.expand().is_err());
    }
}
