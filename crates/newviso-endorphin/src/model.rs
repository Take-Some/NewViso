use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndorphinModel {
    pub schema: String,
    pub rig: crate::RigDefinition,
    #[serde(default)]
    pub joint_map: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub weapon_rig: Option<crate::WeaponRig>,
    #[serde(default)]
    pub protection: Option<crate::ProtectionProfile>,
    pub ragdoll: RagdollMaterial,
    pub recovery: RecoveryModel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_match: Option<crate::RecoveryMatchProfile>,
    pub hit: HitModel,
    #[serde(default)]
    pub anatomy: Option<crate::AnatomyModel>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RagdollMaterial {
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub joint_max_force: f32,
    pub joint_angular_stiffness: f32,
    pub joint_angular_damping: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryModel {
    /// Kinetic energy budget as a fraction of m*g*support_radius.
    pub settling_energy_fraction: f32,
    pub minimum_support_cosine: f32,
    /// Evidence response time in units of sqrt(body_height/gravity).
    pub response_time_scale: f32,
    pub ready_confidence: f32,
    pub balance_reach: f32,
    pub airborne_clearance_fraction: f32,
    /// Absorbable drop height as a fraction of full body height and capacity.
    pub landing_absorption_height_fraction: f32,
    pub capacity_floor: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitModel {
    pub injury_gain: f32,
    pub minimum_weight: f32,
    pub minimum_interrupt_fraction: f32,
    pub maximum_interrupt_fraction: f32,
}
impl EndorphinModel {
    pub fn validate(&self) -> Result<(), String> {
        let nonnegative = |v: f32| v.is_finite() && v >= 0.0;
        let positive = |v: f32| v.is_finite() && v > 0.0;
        let unit = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
        let r = &self.ragdoll;
        let c = &self.recovery;
        let h = &self.hit;
        if self.schema != "newviso.endorphin.v1"
            || !nonnegative(r.friction)
            || !unit(r.restitution)
            || !unit(r.linear_damping)
            || !unit(r.angular_damping)
            || !positive(r.joint_max_force)
            || !nonnegative(r.joint_angular_stiffness)
            || !nonnegative(r.joint_angular_damping)
            || !positive(c.settling_energy_fraction)
            || !positive(c.response_time_scale)
            || !unit(c.minimum_support_cosine)
            || c.minimum_support_cosine >= 1.0
            || !unit(c.ready_confidence)
            || c.ready_confidence <= 0.0
            || c.ready_confidence >= 1.0
            || !positive(c.balance_reach)
            || !positive(c.airborne_clearance_fraction)
            || !positive(c.landing_absorption_height_fraction)
            || !unit(c.capacity_floor)
            || c.capacity_floor <= 0.0
            || !positive(h.injury_gain)
            || !unit(h.minimum_weight)
            || !unit(h.minimum_interrupt_fraction)
            || !unit(h.maximum_interrupt_fraction)
            || h.minimum_interrupt_fraction > h.maximum_interrupt_fraction
        {
            return Err("invalid Endorphin model: schema, material or response parameters".into());
        }
        if self
            .joint_map
            .iter()
            .any(|(a, b)| a.trim().is_empty() || b.trim().is_empty())
        {
            return Err("empty semantic joint mapping".into());
        }
        if let Some(rig) = &self.weapon_rig {
            rig.validate()?;
        }
        if let Some(p) = &self.protection {
            p.validate()?;
        }
        self.rig.validate()?;
        if let Some(profile) = &self.recovery_match {
            profile.validate()?;
        }
        if let Some(a) = &self.anatomy {
            a.validate(&self.rig)?;
        }
        Ok(())
    }

    /// Damage is normalized by the actor's authored maximum vitality.
    /// Armour absorption is handled by the caller, before this calculation.
    pub fn hit_response(&self, lost_vitality_fraction: f32, clip_seconds: f32) -> (f32, f32) {
        let severity = 1.0 - (-lost_vitality_fraction.max(0.0) * self.hit.injury_gain).exp();
        let weight = self.hit.minimum_weight + (1.0 - self.hit.minimum_weight) * severity;
        let interrupt = self.hit.minimum_interrupt_fraction
            + (self.hit.maximum_interrupt_fraction - self.hit.minimum_interrupt_fraction)
                * severity;
        (weight, clip_seconds * interrupt)
    }
}
