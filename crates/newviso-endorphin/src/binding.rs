//! Data-driven semantic joints and weapon posture. No asset or provider identities.
use crate::{math, EndorphinModel};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponRig {
    pub eye_joint: String,
    pub max_yaw_degrees: f32,
    pub max_pitch_degrees: f32,
    pub max_fire_error_degrees: f32,
    pub right: ArmBinding,
    pub left: ArmBinding,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmBinding {
    pub upper_joint: String,
    pub root_joint: String,
    pub tip_joint: String,
    pub weapon_joint: String,
    #[serde(default)]
    pub grips: BTreeMap<String, GripPose>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GripPose {
    pub hand: [[f32; 3]; 3],
    pub socket: [[f32; 3]; 3],
    pub wrist_offset: [f32; 3],
    pub socket_offset: [f32; 3],
    pub attachment_offset: [f32; 3],
    pub attachment_rotation: [f32; 3],
    pub joint_rotations: Vec<JointRotation>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointRotation {
    pub joint: String,
    pub rotation_degrees: [f32; 3],
}
impl WeaponRig {
    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..90.0).contains(&self.max_yaw_degrees)
            || !(0.0..90.0).contains(&self.max_pitch_degrees)
            || !(0.0..=15.0).contains(&self.max_fire_error_degrees)
            || self.max_fire_error_degrees == 0.0
            || self.max_yaw_degrees == 0.0
            || self.max_pitch_degrees == 0.0
            || self.joints().iter().any(|s| s.trim().is_empty())
        {
            return Err("invalid weapon rig joints or front-facing aim envelope".into());
        }
        for arm in [&self.right, &self.left] {
            for grip in arm.grips.values() {
                let vectors = grip.hand.iter().chain(grip.socket.iter()).chain([
                    &grip.wrist_offset,
                    &grip.socket_offset,
                    &grip.attachment_offset,
                    &grip.attachment_rotation,
                ]);
                if vectors.flatten().any(|v| !v.is_finite())
                    || grip.joint_rotations.iter().any(|r| {
                        r.joint.trim().is_empty()
                            || r.rotation_degrees.iter().any(|v| !v.is_finite())
                    })
                {
                    return Err("invalid weapon grip data".into());
                }
                for basis in [grip.hand, grip.socket] {
                    if basis.iter().any(|v| (math::length(*v) - 1.0).abs() > 0.01)
                        || math::dot(math::cross(basis[0], basis[1]), basis[2]) < 0.99
                    {
                        return Err("weapon grip basis must be a proper rotation".into());
                    }
                }
            }
        }
        Ok(())
    }
    pub fn joints(&self) -> Vec<&str> {
        let mut names = vec![self.eye_joint.as_str()];
        for a in [&self.right, &self.left] {
            names.extend([
                a.upper_joint.as_str(),
                a.root_joint.as_str(),
                a.tip_joint.as_str(),
                a.weapon_joint.as_str(),
            ]);
        }
        names
    }
    /// Heading uses the actor's logical +Z forward, independent of model bind yaw.
    pub fn can_aim(&self, forward: [f32; 3], direction: [f32; 3]) -> bool {
        if forward
            .iter()
            .chain(direction.iter())
            .any(|v| !v.is_finite())
        {
            return false;
        }
        let horizontal = direction[0].hypot(direction[2]);
        let heading = forward[0].hypot(forward[2]);
        horizontal > f32::EPSILON
            && heading > f32::EPSILON
            && (forward[0] * direction[0] + forward[2] * direction[2]) / (horizontal * heading)
                >= self.max_yaw_degrees.to_radians().cos()
            && direction[1].abs().atan2(horizontal) <= self.max_pitch_degrees.to_radians()
    }
}
impl EndorphinModel {
    /// Resolve once at asset load. Mapping is simultaneous, so A->B, B->C does not
    /// accidentally remap A twice. Logical parts/functions/trauma remain unchanged.
    pub fn bind_joints(&self) -> Result<Self, String> {
        let mut bound = self.clone();
        let resolve = |name: &mut String| {
            if let Some(actual) = self.joint_map.get(name) {
                *name = actual.clone();
            }
        };
        for p in &mut bound.rig.parts {
            resolve(&mut p.joint);
            if let Some(s) = p.end_joint.as_mut() {
                resolve(s);
            }
            if let Some(s) = p.anchor_joint.as_mut() {
                resolve(s);
            }
        }
        if let Some(a) = bound.anatomy.as_mut() {
            let o = &mut a.orientation;
            for s in [&mut o.lower, &mut o.upper, &mut o.left, &mut o.right] {
                resolve(s);
            }
            if let Some(f) = o.forward.as_mut() {
                for s in f {
                    resolve(s);
                }
            }
        }
        if let Some(r) = bound.weapon_rig.as_mut() {
            resolve(&mut r.eye_joint);
            for a in [&mut r.right, &mut r.left] {
                for s in [
                    &mut a.upper_joint,
                    &mut a.root_joint,
                    &mut a.tip_joint,
                    &mut a.weapon_joint,
                ] {
                    resolve(s);
                }
                for g in a.grips.values_mut() {
                    for j in &mut g.joint_rotations {
                        resolve(&mut j.joint);
                    }
                }
            }
        }
        if let Some(profile) = bound.protection.as_mut() {
            resolve(&mut profile.head_joint);
            for arm in &mut profile.arms {
                for name in [
                    &mut arm.upper_joint,
                    &mut arm.lower_joint,
                    &mut arm.hand_joint,
                    &mut arm.contact_joint,
                ] {
                    resolve(name);
                }
                for joint in &mut arm.open_hand {
                    resolve(&mut joint.joint);
                }
            }
        }
        if let Some(profile) = &mut bound.recovery_match {
            for marker in &mut profile.joints {
                resolve(&mut marker.joint);
            }
        }
        bound.joint_map.clear();
        bound.validate()?;
        Ok(bound)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aim_envelope_is_rotation_and_scale_independent_and_rejects_rear() {
        let arm = ArmBinding {
            upper_joint: "u".into(),
            root_joint: "e".into(),
            tip_joint: "h".into(),
            weapon_joint: "s".into(),
            grips: BTreeMap::new(),
        };
        let r = WeaponRig {
            eye_joint: "eye".into(),
            max_yaw_degrees: 65.0,
            max_pitch_degrees: 60.0,
            max_fire_error_degrees: 6.0,
            right: arm.clone(),
            left: arm,
        };
        for yaw in [-170.0_f32, -30.0, 0.0, 90.0, 175.0] {
            let (s, c) = yaw.to_radians().sin_cos();
            let f = [s, 0.0, c];
            assert!(r.can_aim(f, f.map(|v| v * 13.0)));
            assert!(!r.can_aim(f, f.map(|v| -v)));
            assert!(!r.can_aim(f, [0.0, 1.0, 0.0]));
        }
        assert!(!r.can_aim([0.0; 3], [1.0, 0.0, 0.0]));
        assert!(!r.can_aim([0.0, 0.0, 1.0], [f32::NAN, 0.0, 1.0]));
    }
}
