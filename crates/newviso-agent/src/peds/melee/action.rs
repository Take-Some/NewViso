//! Authored attack geometry and response. Assets are loaded by the host.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StrikeBinding {
    /// Bone name; None means the held item's resident attachment transform.
    pub joint: Option<String>,
    #[serde(default)]
    pub end_joint: Option<String>,
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub radius: f32,
    pub material: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MeleePose {
    /// Wrist offsets in shoulder right/up/forward coordinates.
    pub ready: [f32; 3],
    pub chamber: [f32; 3],
    pub contact: [f32; 3],
    pub follow_through: [f32; 3],
    pub guard: [f32; 3],
    pub pole: [f32; 3],
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MeleeAction {
    pub id: String,
    pub hand: u8,
    pub contact_fraction: f32,
    pub damage_type: String,
    pub damage_scale: f32,
    pub impulse: f32,
    pub startup_scale: f32,
    pub active_seconds: f32,
    pub recovery_scale: f32,
    pub stamina_scale: f32,
    pub guard_penetration: f32,
    pub strikes: Vec<StrikeBinding>,
    pub pose: MeleePose,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MeleeModel {
    pub schema: String,
    pub approach_range: f32,
    pub actions: Vec<MeleeAction>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeleeEquipment {
    pub owner_id: String,
    pub item_id: Option<String>,
    pub generation: u64,
}
impl MeleeAction {
    pub fn validate(&self) -> Result<(), String> {
        let finite = |v: f32| v.is_finite();
        if self.id.trim().is_empty()
            || self.hand > 1
            || !self.contact_fraction.is_finite()
            || self.contact_fraction <= 0.0
            || self.contact_fraction >= 1.0
            || self.damage_type.trim().is_empty()
            || ![
                self.damage_scale,
                self.impulse,
                self.startup_scale,
                self.active_seconds,
                self.recovery_scale,
                self.stamina_scale,
                self.guard_penetration,
            ]
            .into_iter()
            .all(finite)
            || self.damage_scale < 0.0
            || self.impulse < 0.0
            || self.startup_scale <= 0.0
            || self.active_seconds <= 0.0
            || self.recovery_scale <= 0.0
            || self.stamina_scale < 0.0
            || !(0.0..=1.0).contains(&self.guard_penetration)
            || self.strikes.is_empty()
            || self.strikes.len() > 16
            || [
                self.pose.ready,
                self.pose.chamber,
                self.pose.contact,
                self.pose.follow_through,
                self.pose.guard,
                self.pose.pole,
            ]
            .iter()
            .flatten()
            .any(|v| !v.is_finite())
        {
            return Err("invalid melee action response/timeline".into());
        }
        for c in &self.strikes {
            if c.joint.as_ref().is_some_and(|s| s.trim().is_empty())
                || c.material.trim().is_empty()
                || c.start.iter().chain(&c.end).any(|v| !v.is_finite())
                || !c.radius.is_finite()
                || c.radius <= 0.0
            {
                return Err("invalid strike capsule binding".into());
            }
        }
        Ok(())
    }
}
impl MeleeModel {
    pub fn validate(&self) -> Result<(), String> {
        if !self.approach_range.is_finite()
            || self.approach_range <= 0.0
            || self.schema != "newviso.melee.v1"
            || self.actions.is_empty()
            || self.actions.len() > 32
        {
            return Err("invalid melee model".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for a in &self.actions {
            a.validate()?;
            if !ids.insert(&a.id) {
                return Err("duplicate melee action".into());
            }
        }
        Ok(())
    }
}

fn blend(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    let t = t.clamp(0.0, 1.0);
    let t = t * t * (3.0 - 2.0 * t);
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
impl MeleeAction {
    pub fn wrist_goal(&self, phase: &str, progress: f32) -> [f32; 3] {
        match phase {
            "windup" => blend(self.pose.ready, self.pose.chamber, progress),
            "active" if progress < self.contact_fraction => blend(
                self.pose.chamber,
                self.pose.contact,
                progress / self.contact_fraction,
            ),
            "active" => blend(
                self.pose.contact,
                self.pose.follow_through,
                (progress - self.contact_fraction) / (1.0 - self.contact_fraction),
            ),
            "recovery" => blend(self.pose.follow_through, self.pose.ready, progress),
            _ => self.pose.ready,
        }
    }
}
pub(super) fn recover_goal(a: [f32; 3], ready: [f32; 3], progress: f32) -> [f32; 3] {
    blend(a, ready, progress)
}
