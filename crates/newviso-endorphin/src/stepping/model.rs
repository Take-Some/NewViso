//! Authored leg bindings and stepping parameters.
use crate::math;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepLeg {
    pub thigh: String,
    pub calf: String,
    pub foot: String,
    pub knee_direction: [f32; 3],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SteppingModel {
    pub legs: Vec<StepLeg>,
    pub stride_leg_fraction: f32,
    pub clearance_leg_fraction: f32,
    pub duration_scale: f32,
    pub capture_margin_foot_fraction: f32,
    pub reach_fraction: f32,
    pub position_response: f32,
    pub minimum_uprightness: f32,
    pub landing_radius_fraction: f32,
}
impl SteppingModel {
    pub fn validate(&self, rig: &crate::RigDefinition) -> Result<(), String> {
        if !(2..=8).contains(&self.legs.len())
            || ![
                self.stride_leg_fraction,
                self.clearance_leg_fraction,
                self.duration_scale,
                self.capture_margin_foot_fraction,
                self.reach_fraction,
                self.position_response,
                self.minimum_uprightness,
                self.landing_radius_fraction,
            ]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
            || self.reach_fraction >= 1.0
            || self.minimum_uprightness >= 1.0
        {
            return Err("invalid stepping model".into());
        }
        let mut feet = std::collections::BTreeSet::new();
        for leg in &self.legs {
            let a = rig
                .parts
                .iter()
                .find(|p| p.name == leg.thigh)
                .ok_or("missing step thigh")?;
            let b = rig
                .parts
                .iter()
                .find(|p| p.name == leg.calf)
                .ok_or("missing step calf")?;
            let c = rig
                .parts
                .iter()
                .find(|p| p.name == leg.foot)
                .ok_or("missing step foot")?;
            if b.parent.as_deref() != Some(a.name.as_str())
                || c.parent.as_deref() != Some(b.name.as_str())
                || c.sole.is_none()
                || !feet.insert(&leg.foot)
                || math::unit(leg.knee_direction).is_err()
            {
                return Err("stepping needs distinct contiguous thigh/calf/sole chains".into());
            }
        }
        Ok(())
    }
}
