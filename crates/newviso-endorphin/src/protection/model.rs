//! Authored semantics and dimensionless response parameters for protective reaching.
use crate::{math, JointRotation};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectionProfile {
    pub head_joint: String,
    pub forward_axis_local: [f32; 3],
    pub arms: Vec<ProtectionArm>,
    pub tuning: ProtectionTuning,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectionArm {
    pub upper_joint: String,
    pub lower_joint: String,
    pub hand_joint: String,
    pub contact_joint: String,
    pub palm_normal_local: [f32; 3],
    pub palm_up_local: [f32; 3],
    #[serde(default)]
    pub trauma_parts: Vec<String>,
    #[serde(default)]
    pub open_hand: Vec<JointRotation>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectionTuning {
    pub horizon_scale: f32,
    pub raise_time_scale: f32,
    pub release_time_scale: f32,
    pub impact_speed_fraction: f32,
    pub reach_fraction: f32,
    pub hand_radius_fraction: f32,
    pub sensor_radius_fraction: f32,
    pub elbow_outward_fraction: f32,
    pub elbow_drop_fraction: f32,
    pub forward_clearance_fraction: f32,
}
impl ProtectionProfile {
    pub fn validate(&self) -> Result<(), String> {
        let t = self.tuning;
        if !self.forward_axis_local.iter().all(|v| v.is_finite())
            || (math::length(self.forward_axis_local) - 1.0).abs() > 0.001
            || self.head_joint.trim().is_empty()
            || self.arms.is_empty()
            || self.arms.len() > 2
            || ![
                t.horizon_scale,
                t.raise_time_scale,
                t.release_time_scale,
                t.impact_speed_fraction,
                t.reach_fraction,
                t.hand_radius_fraction,
                t.sensor_radius_fraction,
                t.elbow_outward_fraction,
                t.elbow_drop_fraction,
                t.forward_clearance_fraction,
            ]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
            || t.reach_fraction >= 1.0
            || t.hand_radius_fraction >= 1.0
            || t.sensor_radius_fraction > 1.0
            || t.elbow_outward_fraction > 1.0
            || t.elbow_drop_fraction > 1.0
            || t.forward_clearance_fraction > 1.0
        {
            return Err("invalid Endorphin protection profile or response parameters".into());
        }
        let mut hands = std::collections::BTreeSet::new();
        for a in &self.arms {
            if [
                &a.upper_joint,
                &a.lower_joint,
                &a.hand_joint,
                &a.contact_joint,
            ]
            .iter()
            .any(|s| s.trim().is_empty())
                || a.upper_joint == a.lower_joint
                || a.lower_joint == a.hand_joint
                || a.upper_joint == a.hand_joint
                || !hands.insert(&a.hand_joint)
                || (math::length(a.palm_normal_local) - 1.0).abs() > 0.001
                || (math::length(a.palm_up_local) - 1.0).abs() > 0.001
                || !a
                    .palm_normal_local
                    .iter()
                    .chain(&a.palm_up_local)
                    .all(|v| v.is_finite())
                || math::dot(a.palm_normal_local, a.palm_up_local).abs() > 0.001
                || a.trauma_parts.iter().any(|s| s.trim().is_empty())
                || a.open_hand.len() > 32
                || a.open_hand.iter().any(|j| {
                    j.joint.trim().is_empty() || !j.rotation_degrees.iter().all(|v| v.is_finite())
                })
            {
                return Err("invalid Endorphin protective arm binding".into());
            }
        }
        Ok(())
    }
    pub fn joints(&self) -> Vec<&str> {
        let mut names = vec![self.head_joint.as_str()];
        for a in &self.arms {
            names.extend([
                a.upper_joint.as_str(),
                a.lower_joint.as_str(),
                a.hand_joint.as_str(),
                a.contact_joint.as_str(),
            ]);
        }
        names
    }
}
impl ProtectionTuning {
    pub fn horizon(self, reach: f32, gravity: f32, speed: f32) -> f32 {
        reach * self.horizon_scale
            / ((gravity.max(0.0) * reach).sqrt() + speed.max(0.0)).max(f32::EPSILON)
    }
}
