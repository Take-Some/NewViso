//! Adaptive hand support, injury and breakable grip. Gameplay coefficients are asset data.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WeaponGripProfile {
    pub enabled: bool,
    pub model_asset: String,
    pub model: Option<WeaponGripModel>,
}
impl Default for WeaponGripProfile {
    fn default() -> Self {
        Self {
            enabled: true,
            model_asset: "physics/peds/humanoid_grip.json".into(),
            model: None,
        }
    }
}
impl WeaponGripProfile {
    pub(super) fn validate(&self) -> Result<(), String> {
        if let Some(model) = &self.model {
            model.validate()?;
        }
        if self.model_asset.split('/').any(|s| s == "..") {
            return Err("grip model path escapes content".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DisarmedDecisionModel {
    pub fight: f32,
    pub escape: f32,
    pub injury_aversion: f32,
    pub distance_aversion: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GripHitReceiver {
    pub region: String,
    pub radius_scale: f32,
    pub segments: Vec<[String; 2]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeaponGripModel {
    pub schema: String,
    pub hand_force: [f32; 2],
    pub wrist_torque: [f32; 2],
    pub slip_travel: f32,
    pub slip_stiffness: f32,
    pub slip_damping: f32,
    pub vitality_exponent: f32,
    pub stamina_exponent: f32,
    pub wound_exponent: f32,
    pub pain_gain: f32,
    pub pain_decay: f32,
    pub motion_response_seconds: f32,
    pub collection_reach_fraction: f32,
    pub holding_joint: String,
    pub injury_coupling: BTreeMap<String, [f32; 2]>,
    pub reaction_support: BTreeMap<String, [f32; 2]>,
    pub disarmed_decision: DisarmedDecisionModel,
    #[serde(default)]
    pub hit_receivers: Vec<GripHitReceiver>,
}
impl WeaponGripModel {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != "newviso.ped-grip-model.v1" {
            return Err("unsupported weapon grip model".into());
        }
        for value in self.hand_force.iter().chain(&self.wrist_torque).chain([
            &self.slip_travel,
            &self.slip_stiffness,
            &self.slip_damping,
            &self.motion_response_seconds,
            &self.collection_reach_fraction,
        ]) {
            if !value.is_finite() || *value <= 0.0 {
                return Err("grip physical properties must be finite and positive".into());
            }
        }
        for value in [
            &self.vitality_exponent,
            &self.stamina_exponent,
            &self.wound_exponent,
            &self.pain_gain,
            &self.pain_decay,
            &self.disarmed_decision.fight,
            &self.disarmed_decision.escape,
            &self.disarmed_decision.injury_aversion,
            &self.disarmed_decision.distance_aversion,
        ] {
            if !value.is_finite() || *value < 0.0 {
                return Err("grip response parameters must be finite and nonnegative".into());
            }
        }
        if self.holding_joint.trim().is_empty()
            || !self.injury_coupling.contains_key("body")
            || !self.reaction_support.contains_key("animated")
        {
            return Err(
                "grip model requires holding joint, body coupling and animated support".into(),
            );
        }
        for receiver in &self.hit_receivers {
            if receiver.region.trim().is_empty()
                || !receiver.radius_scale.is_finite()
                || receiver.radius_scale <= 0.0
                || receiver.segments.is_empty()
                || receiver
                    .segments
                    .iter()
                    .flatten()
                    .any(|joint| joint.trim().is_empty())
            {
                return Err("invalid grip anatomical receiver".into());
            }
        }
        for weights in self
            .injury_coupling
            .values()
            .chain(self.reaction_support.values())
        {
            if weights
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            {
                return Err("grip coupling and support weights must be in [0,1]".into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GripImpact {
    Body,
    Arm,
    LeftArm,
    RightArm,
    Knockdown,
    Suppressed,
}
impl GripImpact {
    fn coupling(self) -> Option<&'static str> {
        match self {
            Self::Body => Some("body"),
            Self::Arm => Some("arm"),
            Self::LeftArm => Some("left_arm"),
            Self::RightArm => Some("right_arm"),
            Self::Knockdown => Some("fall"),
            Self::Suppressed => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GripLossReason {
    #[serde(
        alias = "arm_injury",
        alias = "heavy_hit",
        alias = "critical_injury",
        alias = "knockdown"
    )]
    SupportFailure,
    Action,
    Death,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WeaponGripState {
    pub lost: bool,
    pub reason: Option<GripLossReason>,
    pub held_item: Option<String>,
    pub held_generation: u64,
    pub release_sequence: u64,
    pub wound: [f32; 2],
    pub pain: [f32; 2],
    pub slip: f32,
    pub force_demand: f32,
    pub force_capacity: f32,
    pub torque_demand: f32,
    pub torque_capacity: f32,
    pub utilization: f32,
    pub support_quality: f32,
}
impl Default for WeaponGripState {
    fn default() -> Self {
        Self {
            lost: false,
            reason: None,
            held_item: None,
            held_generation: 0,
            release_sequence: 0,
            wound: [0.0; 2],
            pain: [0.0; 2],
            slip: 0.0,
            force_demand: 0.0,
            force_capacity: 0.0,
            torque_demand: 0.0,
            torque_capacity: 0.0,
            utilization: 0.0,
            support_quality: 1.0,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct WeaponGripLoad {
    pub force: f32,
    pub torque: f32,
    pub support_span: f32,
    pub support: [f32; 2],
}
impl WeaponGripLoad {
    pub fn validate(self) -> Result<(), String> {
        if [self.force, self.torque, self.support_span]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
            || self
                .support
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("invalid weapon support load".into());
        }
        Ok(())
    }
}
impl WeaponGripState {
    pub(super) fn damage(
        &mut self,
        model: &WeaponGripModel,
        max_health: f32,
        health_damage: f32,
        impact: GripImpact,
    ) {
        let Some(region) = impact.coupling() else {
            return;
        };
        let coupling = model
            .injury_coupling
            .get(region)
            .unwrap_or(&model.injury_coupling["body"]);
        for (i, weight) in coupling.iter().enumerate() {
            let injury = health_damage / max_health * weight;
            self.wound[i] += injury;
            self.pain[i] += injury;
        }
    }
    fn bind(&mut self, item: &str, generation: u64) {
        if self.held_item.as_deref() != Some(item) || self.held_generation != generation {
            self.held_item = Some(item.into());
            self.held_generation = generation;
            self.lost = false;
            self.reason = None;
            self.slip = 0.0;
        }
    }
    pub fn release(&mut self, reason: GripLossReason) -> bool {
        if self.lost {
            return false;
        }
        self.lost = true;
        self.reason = Some(reason);
        self.release_sequence += 1;
        true
    }
    /// Analytic viscoelastic slip: a physical travel limit, never an HP threshold.
    fn advance(
        &mut self,
        model: &WeaponGripModel,
        load: WeaponGripLoad,
        vitality: f32,
        stamina: f32,
        dt: f32,
    ) -> bool {
        let decay = (-model.pain_decay * dt).exp();
        let mid_decay = decay.sqrt();
        let condition = vitality.clamp(0.0, 1.0).powf(model.vitality_exponent)
            * stamina.clamp(0.0, 1.0).powf(model.stamina_exponent);
        let efficiency: [f32; 2] = std::array::from_fn(|i| {
            let value = condition
                * (-model.wound_exponent * self.wound[i]
                    - model.pain_gain * self.pain[i] * mid_decay)
                    .exp();
            self.pain[i] *= decay;
            value * load.support[i]
        });
        self.support_quality = efficiency[0].max(efficiency[1]);
        self.force_demand = load.force;
        self.torque_demand = load.torque;
        self.force_capacity = model
            .hand_force
            .iter()
            .zip(efficiency)
            .map(|(f, e)| f * e)
            .sum();
        self.torque_capacity = model
            .wrist_torque
            .iter()
            .zip(efficiency)
            .map(|(f, e)| f * e)
            .sum::<f32>()
            + model.hand_force[1] * efficiency[1] * load.support_span;
        self.utilization = (load.force / self.force_capacity.max(f32::MIN_POSITIVE))
            .hypot(load.torque / self.torque_capacity.max(f32::MIN_POSITIVE))
            .min(f32::MAX.sqrt());
        let excess_force = ((self.utilization - 1.0).max(0.0)
            * (load.force + load.torque / load.support_span.max(model.slip_travel)))
        .min(f32::MAX.sqrt());
        let equilibrium = excess_force / model.slip_stiffness;
        let memory = (-model.slip_stiffness * dt / model.slip_damping).exp();
        self.slip =
            (self.slip * memory + equilibrium * (1.0 - memory)).clamp(0.0, model.slip_travel);
        self.slip >= model.slip_travel
    }
}
impl PedRuntime {
    /// Tissue and motor recovery continue when the hand is empty or the body is streamed out.
    pub fn advance_unheld_weapon_grip(
        &mut self,
        id: &str,
        support: [f32; 2],
        dt: f32,
    ) -> Result<(), String> {
        let load = WeaponGripLoad {
            force: 0.0,
            torque: 0.0,
            support_span: 0.0,
            support,
        };
        load.validate()?;
        tasks::finite_range("unheld grip timestep", dt, 0.0, 1.0)?;
        let ped = self
            .records
            .get_mut(id)
            .ok_or_else(|| format!("unknown grip actor '{id}'"))?;
        if ped.dead {
            return Ok(());
        }
        let Some(model) = &ped.profile.weapon_grip.model else {
            return Ok(());
        };
        let stamina = ped
            .melee
            .as_ref()
            .zip(ped.profile.melee.as_ref())
            .map_or(1.0, |(s, p)| s.stamina / p.max_stamina);
        ped.weapon_grip.advance(
            model,
            load,
            ped.health / ped.profile.max_health,
            stamina,
            dt,
        );
        Ok(())
    }

    pub fn recover_weapon_grip_elapsed(&mut self, id: &str, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err("invalid saved grip elapsed time".into());
        }
        let ped = self
            .records
            .get_mut(id)
            .ok_or_else(|| format!("unknown grip actor '{id}'"))?;
        if let Some(model) = &ped.profile.weapon_grip.model {
            let decay = (-(model.pain_decay as f64) * seconds).exp() as f32;
            ped.weapon_grip.pain = ped.weapon_grip.pain.map(|v| v * decay);
        }
        self.advance_unheld_weapon_grip(id, [1.0; 2], 0.0)
    }
    pub fn advance_weapon_grip(
        &mut self,
        actor_id: &str,
        item: &str,
        generation: u64,
        load: WeaponGripLoad,
        dt: f32,
    ) -> Result<bool, String> {
        load.validate()?;
        tasks::finite_range("weapon grip timestep", dt, 0.0, 1.0)?;
        let ped = self
            .records
            .get_mut(actor_id)
            .ok_or_else(|| format!("ped '{actor_id}' does not exist"))?;
        let Some(model) = &ped.profile.weapon_grip.model else {
            return Ok(false);
        };
        ped.weapon_grip.bind(item, generation);
        if !ped.profile.weapon_grip.enabled || ped.dead || ped.weapon_grip.lost {
            return Ok(false);
        }
        let stamina = ped
            .melee
            .as_ref()
            .zip(ped.profile.melee.as_ref())
            .map_or(1.0, |(s, p)| s.stamina / p.max_stamina);
        let failed = ped.weapon_grip.advance(
            model,
            load,
            ped.health / ped.profile.max_health,
            stamina,
            dt,
        );
        if failed && ped.weapon_grip.release(GripLossReason::SupportFailure) {
            if let Some(combat) = ped.combat.as_mut() {
                combat.lose_weapon();
            }
            self.events.push(json!({"kind":"ped.weapon.grip_lost","actor_id":actor_id,
                "reason":ped.weapon_grip.reason,"item_id":item,"equipment_generation":generation,
                "force_demand":ped.weapon_grip.force_demand,"force_capacity":ped.weapon_grip.force_capacity,
                "torque_demand":ped.weapon_grip.torque_demand,"torque_capacity":ped.weapon_grip.torque_capacity}));
            return Ok(true);
        }
        Ok(false)
    }
    pub fn release_weapon_grip(&mut self, actor_id: &str, reason: GripLossReason) -> bool {
        let Some(ped) = self.records.get_mut(actor_id) else {
            return false;
        };
        if let Some(combat) = ped.combat.as_mut() {
            combat.lose_weapon();
        }
        ped.weapon_grip.release(reason)
    }
    pub fn restore_weapon_grip(
        &mut self,
        actor_id: &str,
        grip: WeaponGripState,
    ) -> Result<(), String> {
        if grip
            .wound
            .iter()
            .chain(&grip.pain)
            .chain([
                &grip.slip,
                &grip.force_demand,
                &grip.force_capacity,
                &grip.torque_demand,
                &grip.torque_capacity,
                &grip.utilization,
                &grip.support_quality,
            ])
            .any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err("invalid saved weapon grip state".into());
        }
        let ped = self
            .records
            .get_mut(actor_id)
            .ok_or_else(|| format!("ped '{actor_id}' does not exist"))?;
        if grip.lost {
            if let Some(combat) = ped.combat.as_mut() {
                combat.lose_weapon();
            }
        }
        ped.weapon_grip = grip;
        Ok(())
    }
}

/// Choose among available actions by current reach, vitality, stamina and hand support.
pub(super) fn fight_disarmed(
    profile: &PedProfile,
    grip: &WeaponGripState,
    melee: Option<&melee::MeleeState>,
    distance: f32,
) -> bool {
    let Some(fists) = profile.melee.as_ref() else {
        return false;
    };
    let Some(model) = profile.weapon_grip.model.as_ref() else {
        return true;
    };
    let energy = melee
        .map_or(1.0, |s| s.stamina / fists.max_stamina)
        .clamp(0.0, 1.0);
    let health_condition = (-model.wound_exponent * grip.wound.iter().copied().sum::<f32>()).exp();
    let readiness = grip.support_quality * energy * health_condition;
    let reach =
        (1.0 + distance.max(0.0) / fists.range).powf(-model.disarmed_decision.distance_aversion);
    let fight = model.disarmed_decision.fight * readiness * reach;
    let escape = model.disarmed_decision.escape
        * (1.0 - readiness)
        * (1.0 + model.disarmed_decision.injury_aversion * (1.0 - health_condition));
    fight > escape
}

#[cfg(test)]
mod tests;
