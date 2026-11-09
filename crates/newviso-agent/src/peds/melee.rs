//! Timed unarmed attacks and directional guard; the host owns spatial/contact validation.
use super::*;
pub mod action;
pub use action::{MeleeAction, MeleeEquipment, MeleeModel};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct MeleeProfile {
    pub model_asset: String,
    pub model: Option<MeleeModel>,
    pub inventory_owner: Option<String>,
    pub interrupt_recovery_seconds: f32,
    pub guard_break_recovery_seconds: f32,
    pub guard_half_angle_radians: f32,
    pub guard_regen_scale: f32,
    pub joint_map: BTreeMap<String, String>,
    pub range: f32,
    pub damage: f32,
    pub windup_seconds: f32,
    pub recovery_seconds: f32,
    pub max_stamina: f32,
    pub stamina_regen: f32,
    pub attack_cost: f32,
    pub block_cost: f32,
    pub block_damage_scale: f32,
    pub combo_window_seconds: f32,
    pub combo_damage_bonus: f32,
    pub combo_max_hits: u8,
}
impl Default for MeleeProfile {
    fn default() -> Self {
        Self {
            model_asset: "combat/melee/unarmed.json".into(),
            model: None,
            inventory_owner: None,
            interrupt_recovery_seconds: 0.12,
            guard_break_recovery_seconds: 0.55,
            guard_half_angle_radians: 0.35f32.acos(),
            guard_regen_scale: 0.25,
            joint_map: BTreeMap::new(),
            range: 1.55,
            damage: 18.0,
            windup_seconds: 0.17,
            recovery_seconds: 0.38,
            max_stamina: 100.0,
            stamina_regen: 20.0,
            attack_cost: 18.0,
            block_cost: 16.0,
            block_damage_scale: 0.18,
            combo_window_seconds: 0.5,
            combo_damage_bonus: 0.12,
            combo_max_hits: 3,
        }
    }
}
impl MeleeProfile {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.model_asset.trim().is_empty() {
            return Err("missing melee model asset".into());
        }
        if let Some(model) = &self.model {
            model.validate()?;
        }
        if let Some(owner) = &self.inventory_owner {
            validate_id("melee inventory owner", owner)?;
        }
        for (name, n, min, max) in [
            (
                "interrupt recovery",
                self.interrupt_recovery_seconds,
                0.0,
                3.0,
            ),
            (
                "guard break recovery",
                self.guard_break_recovery_seconds,
                0.0,
                3.0,
            ),
            (
                "guard half angle",
                self.guard_half_angle_radians,
                0.0,
                std::f32::consts::PI,
            ),
            ("guard regen scale", self.guard_regen_scale, 0.0, 1.0),
            ("melee range", self.range, 0.5, 3.0),
            ("melee damage", self.damage, 0.0, 1000.0),
            ("melee windup", self.windup_seconds, 0.05, 2.0),
            ("melee recovery", self.recovery_seconds, 0.1, 3.0),
            ("melee stamina", self.max_stamina, 1.0, 10000.0),
            ("stamina regen", self.stamina_regen, 0.0, 1000.0),
            ("attack cost", self.attack_cost, 0.0, self.max_stamina),
            ("block cost", self.block_cost, 0.0, self.max_stamina),
            ("block scale", self.block_damage_scale, 0.0, 1.0),
            ("combo window", self.combo_window_seconds, 0.0, 2.0),
            ("combo bonus", self.combo_damage_bonus, 0.0, 1.0),
        ] {
            tasks::finite_range(name, n, min, max)?;
        }
        if !(1..=8).contains(&self.combo_max_hits) {
            return Err("combo_max_hits must be in 1..=8".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct MeleeState {
    pub action: Option<MeleeAction>,
    pub equipment: Option<MeleeEquipment>,
    pub contact_consumed: bool,
    pub active_until: f64,
    pub wrist_offset: Option<[f32; 3]>,
    #[serde(skip)]
    last_advanced: Option<f64>,
    #[serde(skip)]
    regen_carry: f64,
    #[serde(skip)]
    cancelled_pose: Option<[f32; 3]>,
    pub phase: String,
    pub progress: f32,
    pub stamina: f32,
    pub blocking: bool,
    pub attacks: u64,
    pub hits: u64,
    pub blocks: u64,
    pub hand: u8,
    pub combo_step: u8,
    pub last_impact: f64,
    pub guard_broken: bool,
    pub facing: [f32; 3],
    #[serde(skip)]
    pub(super) guard_requested: bool,
    #[serde(skip)]
    started_at: f64,
    #[serde(skip)]
    impact_at: f64,
    #[serde(skip)]
    finish_at: f64,
    #[serde(skip)]
    combo_expires_at: f64,
    #[serde(skip)]
    strike_damage: f32,
    pub target: Option<String>,
    #[serde(skip)]
    direction: [f32; 3],
}
#[derive(Clone, Debug)]
pub struct ActiveMeleeStrike {
    pub actor_id: String,
    pub sequence: u64,
    pub damage: f32,
    pub direction: [f32; 3],
    pub action: MeleeAction,
    pub equipment: Option<MeleeEquipment>,
    pub active_start: f64,
    pub active_end: f64,
}
impl MeleeState {
    pub(super) fn new(p: &MeleeProfile) -> Self {
        Self {
            action: None,
            equipment: None,
            contact_consumed: true,
            active_until: 0.0,
            wrist_offset: p
                .model
                .as_ref()
                .and_then(|m| m.actions.first())
                .map(|a| a.pose.ready),
            cancelled_pose: None,
            last_advanced: None,
            regen_carry: 0.0,
            phase: "idle".into(),
            progress: 0.0,
            stamina: p.max_stamina,
            blocking: false,
            attacks: 0,
            hits: 0,
            blocks: 0,
            hand: 0,
            combo_step: 0,
            last_impact: -1000.0,
            guard_broken: false,
            facing: [0.0, 0.0, -1.0],
            guard_requested: false,
            started_at: 0.0,
            impact_at: 0.0,
            finish_at: 0.0,
            combo_expires_at: 0.0,
            strike_damage: p.damage,
            target: None,
            direction: [0.0, 0.0, -1.0],
        }
    }
    pub(super) fn cancel(&mut self, now: f64, until: f64) {
        self.cancelled_pose = self.wrist_offset;
        self.contact_consumed = true;
        self.combo_step = 0;
        self.combo_expires_at = 0.0;
        self.blocking = false;
        self.guard_requested = false;
        self.phase = "recovery".into();
        self.progress = 0.0;
        self.started_at = now;
        self.impact_at = now;
        self.active_until = now;
        self.finish_at = until;
    }
}
impl PedRuntime {
    pub fn melee_guard(
        &mut self,
        id: &str,
        requested: bool,
        facing: [f32; 3],
    ) -> Result<(), String> {
        tasks::validate_position(facing)?;
        let Some(ped) = self.records.get_mut(id) else {
            return Err(format!("unknown melee actor '{id}'"));
        };
        let Some(state) = ped.melee.as_mut() else {
            return Ok(());
        };
        let n = facing[0].hypot(facing[2]);
        if n < 0.001 {
            return Err("melee facing must have a horizontal direction".into());
        }
        state.facing = [facing[0] / n, 0.0, facing[2] / n];
        state.guard_requested = requested && !ped.dead;
        state.blocking = state.guard_requested
            && state.phase == "idle"
            && !state.guard_broken
            && state.stamina > 0.0;
        Ok(())
    }
    pub fn begin_melee(
        &mut self,
        id: &str,
        target: Option<String>,
        direction: [f32; 3],
        now: f64,
    ) -> Result<bool, String> {
        self.begin_melee_scaled(id, target, direction, now, 1.0)
    }
    fn begin_melee_scaled(
        &mut self,
        id: &str,
        target: Option<String>,
        direction: [f32; 3],
        now: f64,
        cost_scale: f32,
    ) -> Result<bool, String> {
        tasks::validate_position(direction)?;
        let n = direction[0].hypot(direction[2]);
        if n < 0.001 {
            return Ok(false);
        }
        let Some(ped) = self
            .records
            .get_mut(id)
            .filter(|p| !p.dead && p.stunned_until <= now)
        else {
            return Ok(false);
        };
        if ped.combat.as_ref().is_some_and(|c| {
            c.mode != "engage"
                || !c.visible
                || now < c.ready_at
                || ped
                    .profile
                    .combat
                    .as_ref()
                    .is_some_and(|p| p.director.is_some())
                    && (!c.tactics.attack_allowed || now > c.tactics.permit_until)
        }) {
            return Ok(false);
        }
        let Some((profile, state)) = ped.profile.melee.as_ref().zip(ped.melee.as_mut()) else {
            return Ok(false);
        };
        if now < state.finish_at || state.stamina < profile.attack_cost * cost_scale {
            return Ok(false);
        }
        state.stamina -= profile.attack_cost * cost_scale;
        state.contact_consumed = false;
        state.action = None;
        state.equipment = None;
        state.attacks += 1;
        state.combo_step =
            if now <= state.combo_expires_at && state.combo_step < profile.combo_max_hits {
                state.combo_step + 1
            } else {
                1
            };
        state.hand = (state.combo_step - 1) % 2;
        state.strike_damage =
            profile.damage * (1.0 + profile.combo_damage_bonus * f32::from(state.combo_step - 1));
        state.direction = [direction[0] / n, 0.0, direction[2] / n];
        state.facing = state.direction;
        state.target = target;
        state.started_at = now;
        state.impact_at = now + f64::from(profile.windup_seconds);
        state.active_until = state.impact_at;
        state.finish_at = state.impact_at + f64::from(profile.recovery_seconds);
        state.combo_expires_at = state.finish_at + f64::from(profile.combo_window_seconds);
        state.cancelled_pose = None;
        state.wrist_offset = None;
        state.phase = "windup".into();
        state.progress = 0.0;
        state.blocking = false;
        state.guard_broken = false;
        Ok(true)
    }
    /// Start an authored action while retaining the common stamina/combo/stun rules.
    pub fn begin_melee_action(
        &mut self,
        id: &str,
        target: Option<String>,
        direction: [f32; 3],
        now: f64,
        action: MeleeAction,
        base_damage: f32,
        equipment: Option<MeleeEquipment>,
    ) -> Result<bool, String> {
        action.validate()?;
        if !base_damage.is_finite() || base_damage < 0.0 {
            return Err("invalid melee damage".into());
        }
        if !self.begin_melee_scaled(id, target, direction, now, action.stamina_scale)? {
            return Ok(false);
        }
        let p = self.records.get_mut(id).expect("melee owner");
        let (profile, state) = p
            .profile
            .melee
            .as_ref()
            .zip(p.melee.as_mut())
            .expect("melee profile");
        state.strike_damage = base_damage
            * action.damage_scale
            * (1.0 + profile.combo_damage_bonus * f32::from(state.combo_step - 1));
        state.hand = action.hand;
        state.impact_at = now + f64::from(profile.windup_seconds * action.startup_scale);
        state.active_until = state.impact_at + f64::from(action.active_seconds);
        state.finish_at =
            state.active_until + f64::from(profile.recovery_seconds * action.recovery_scale);
        state.combo_expires_at = state.finish_at + f64::from(profile.combo_window_seconds);
        state.equipment = equipment;
        state.wrist_offset = Some(action.pose.ready);
        state.action = Some(action);
        Ok(true)
    }
    /// The host samples these after animation. A hit is consumed only by actual contact.
    pub fn active_melee_strikes(&self, now: f64) -> Vec<ActiveMeleeStrike> {
        self.records
            .values()
            .filter_map(|p| {
                let s = p.melee.as_ref()?;
                if p.dead
                    || p.stunned_until > now
                    || s.contact_consumed
                    || s.action.is_none()
                    || now > s.finish_at
                {
                    return None;
                }
                Some(ActiveMeleeStrike {
                    actor_id: p.profile.actor_id.clone(),
                    sequence: s.attacks,
                    damage: s.strike_damage,
                    direction: s.direction,
                    action: s.action.clone()?,
                    equipment: s.equipment.clone(),
                    active_start: s.impact_at,
                    active_end: s.active_until,
                })
            })
            .collect()
    }
    pub fn consume_melee_contact(&mut self, id: &str, sequence: u64) -> bool {
        let Some(s) = self.records.get_mut(id).and_then(|p| p.melee.as_mut()) else {
            return false;
        };
        if s.attacks != sequence || s.contact_consumed {
            return false;
        }
        s.contact_consumed = true;
        true
    }
    /// Revoke voluntary action once while a host-owned physical response is active.
    /// Repeated observations must not keep extending the recovery deadline.
    pub fn suspend_melee(&mut self, id: &str, now: f64) -> bool {
        let active = self
            .records
            .get(id)
            .and_then(|p| p.melee.as_ref())
            .is_some_and(|s| {
                s.guard_requested
                    || s.blocking
                    || !s.contact_consumed
                    || matches!(s.phase.as_str(), "windup" | "active")
            });
        if active {
            self.cancel_melee(id, now);
        }
        active
    }

    pub fn cancel_melee(&mut self, id: &str, now: f64) {
        if let Some(p) = self.records.get_mut(id) {
            let duration = p
                .profile
                .melee
                .as_ref()
                .map_or(0.0, |m| m.interrupt_recovery_seconds);
            if let Some(s) = p.melee.as_mut() {
                s.cancel(now, now + f64::from(duration));
            }
        }
    }
    pub fn obstruct_melee(&mut self, id: &str, sequence: u64, contact_time: f64, now: f64) {
        let Some(p) = self.records.get_mut(id) else {
            return;
        };
        let Some(s) = p.melee.as_mut().filter(|s| s.attacks == sequence) else {
            return;
        };
        if let Some(action) = s.action.as_ref() {
            let (phase, progress) = if contact_time < s.impact_at {
                (
                    "windup",
                    ((contact_time - s.started_at) / (s.impact_at - s.started_at).max(f64::EPSILON))
                        as f32,
                )
            } else if contact_time < s.active_until {
                (
                    "active",
                    ((contact_time - s.impact_at)
                        / (s.active_until - s.impact_at).max(f64::EPSILON))
                        as f32,
                )
            } else {
                (
                    "recovery",
                    ((contact_time - s.active_until)
                        / (s.finish_at - s.active_until).max(f64::EPSILON))
                        as f32,
                )
            };
            s.wrist_offset = Some(action.wrist_goal(phase, progress));
        }
        let duration = p.profile.melee.as_ref().map_or(0.0, |m| m.recovery_seconds);
        s.cancel(now, now + f64::from(duration));
    }
    pub fn interrupt_combat(&mut self, id: &str, now: f64, duration: f32) {
        let Some(ped) = self.records.get_mut(id).filter(|p| !p.dead) else {
            return;
        };
        ped.stunned_until = ped.stunned_until.max(now + f64::from(duration));
        if let Some(melee) = &mut ped.melee {
            melee.cancel(now, ped.stunned_until);
        }
        if let Some(combat) = &mut ped.combat {
            combat.ready_at = combat.ready_at.max(ped.stunned_until);
            if combat.reload_until > now {
                combat.reload_until += f64::from(duration.min(0.3));
            }
        }
    }
    pub fn advance_melee(&mut self, now: f64, dt: f32) {
        for ped in self.records.values_mut() {
            let Some((profile, state)) = ped.profile.melee.as_ref().zip(ped.melee.as_mut()) else {
                continue;
            };
            let elapsed = state
                .last_advanced
                .map_or(dt.max(0.0) as f64, |last| (now - last).max(0.0));
            state.last_advanced = Some(now);
            if ped.dead {
                state.blocking = false;
                state.contact_consumed = true;
                state.wrist_offset = None;
                state.phase = "dead".into();
                continue;
            }
            if now > state.combo_expires_at {
                state.combo_step = 0;
            }
            if now >= state.finish_at {
                state.phase = "idle".into();
                state.progress = 0.0;
            } else if now < state.impact_at {
                state.phase = "windup".into();
                state.progress = ((now - state.started_at)
                    / (state.impact_at - state.started_at).max(0.001))
                .clamp(0.0, 1.0) as f32;
            } else if now < state.active_until {
                state.phase = "active".into();
                state.progress = ((now - state.impact_at)
                    / (state.active_until - state.impact_at).max(0.001))
                .clamp(0.0, 1.0) as f32;
            } else {
                state.phase = "recovery".into();
                state.progress = ((now - state.active_until)
                    / (state.finish_at - state.active_until).max(0.001))
                .clamp(0.0, 1.0) as f32;
            }
            if state.phase == "idle" && state.stamina >= profile.block_cost {
                state.guard_broken = false;
            }
            state.blocking = state.guard_requested
                && !state.guard_broken
                && state.phase == "idle"
                && ped.stunned_until <= now
                && state.stamina > 0.0;
            if state.phase == "idle" {
                let guard_scale = if state.blocking {
                    profile.guard_regen_scale
                } else {
                    1.0
                };
                let idle = (now - (now - elapsed).max(state.finish_at)).max(0.0);
                let next = (f64::from(state.stamina)
                    + state.regen_carry
                    + f64::from(profile.stamina_regen * guard_scale) * idle)
                    .min(f64::from(profile.max_stamina));
                state.stamina = next as f32;
                state.regen_carry = if next < f64::from(profile.max_stamina) {
                    next - f64::from(state.stamina)
                } else {
                    0.0
                };
            }
            if let Some(action) = state
                .action
                .as_ref()
                .or_else(|| profile.model.as_ref().and_then(|m| m.actions.first()))
            {
                state.wrist_offset = Some(if state.blocking {
                    action.pose.guard
                } else if let Some(seed) = state.cancelled_pose {
                    action::recover_goal(seed, action.pose.ready, state.progress)
                } else {
                    action.wrist_goal(&state.phase, state.progress)
                });
            }
        }
    }
    /// Return the received damage and whether guard absorbed the strike. Back attacks bypass guard.
    pub fn defend_melee(
        &mut self,
        id: &str,
        damage: f32,
        incoming: [f32; 3],
        now: f64,
    ) -> (f32, bool) {
        let Some(ped) = self.records.get_mut(id).filter(|p| !p.dead) else {
            return (0.0, false);
        };
        let Some((profile, state)) = ped.profile.melee.as_ref().zip(ped.melee.as_mut()) else {
            return (damage, false);
        };
        let n = incoming[0].hypot(incoming[2]).max(0.001);
        if !state.blocking
            || (state.facing[0] * incoming[0] + state.facing[2] * incoming[2]) / n
                < profile.guard_half_angle_radians.cos()
        {
            return (damage, false);
        }
        state.last_impact = now;
        if state.stamina < profile.block_cost {
            state.stamina = 0.0;
            state.guard_broken = true;
            state.cancel(now, now + f64::from(profile.guard_break_recovery_seconds));
            return (damage, false);
        }
        state.stamina -= profile.block_cost;
        state.blocks += 1;
        (damage * profile.block_damage_scale, true)
    }
    pub fn melee_hit(&mut self, id: &str, now: f64) {
        if let Some(state) = self.records.get_mut(id).and_then(|p| p.melee.as_mut()) {
            state.hits += 1;
            state.last_impact = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use action::{MeleePose, StrikeBinding};
    fn action() -> MeleeAction {
        MeleeAction {
            id: "jab".into(),
            hand: 0,
            contact_fraction: 0.6,
            damage_type: "blunt".into(),
            damage_scale: 1.0,
            impulse: 12.0,
            startup_scale: 1.0,
            active_seconds: 0.1,
            recovery_scale: 1.0,
            stamina_scale: 1.0,
            guard_penetration: 0.0,
            strikes: vec![StrikeBinding {
                joint: Some("hand".into()),
                end_joint: None,
                start: [0.0; 3],
                end: [0.0, 0.1, 0.0],
                radius: 0.05,
                material: "flesh".into(),
            }],
            pose: MeleePose {
                ready: [0.0, 0.1, 0.2],
                chamber: [0.0, 0.1, 0.1],
                contact: [0.0, 0.1, 0.5],
                follow_through: [0.0, 0.1, 0.6],
                guard: [0.0, 0.2, 0.2],
                pole: [0.2, -0.3, 0.0],
            },
        }
    }
    fn runtime() -> PedRuntime {
        let mut r = PedRuntime::default();
        r.upsert(
            PedProfile {
                actor_id: "player".into(),
                damage_entity: Some(1),
                melee: Some(MeleeProfile {
                    model: Some(MeleeModel {
                        schema: "newviso.melee.v1".into(),
                        approach_range: 0.7,
                        actions: vec![action()],
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            true,
        )
        .unwrap();
        r
    }
    fn start(r: &mut PedRuntime, now: f64) -> bool {
        r.begin_melee_action("player", None, [0.0, 0.0, -1.0], now, action(), 18.0, None)
            .unwrap()
    }
    #[test]
    fn only_contact_consumption_resolves_a_strike_and_old_sequences_are_rejected() {
        let mut r = runtime();
        assert!(start(&mut r, 1.0));
        assert!(!start(&mut r, 1.01));
        r.advance_melee(1.1, 0.1);
        assert_eq!(
            r.state("player").unwrap().melee.as_ref().unwrap().phase,
            "windup"
        );
        let strike = r.active_melee_strikes(1.1).pop().unwrap();
        assert!((strike.active_start - 1.17).abs() < 0.00001);
        r.advance_melee(1.2, 0.1);
        assert_eq!(
            r.state("player").unwrap().melee.as_ref().unwrap().phase,
            "active"
        );
        assert_eq!(
            r.state("player").unwrap().melee.as_ref().unwrap().hits,
            0,
            "clock never applies a hit"
        );
        assert!(r.consume_melee_contact("player", strike.sequence));
        assert!(!r.consume_melee_contact("player", strike.sequence));
        assert!(r.active_melee_strikes(1.21).is_empty());
        r.advance_melee(1.7, 0.5);
        assert!(start(&mut r, 1.7));
        assert!(!r.consume_melee_contact("player", strike.sequence));
    }
    #[test]
    fn cancellation_retracts_from_the_observed_pose_and_never_reenters_active() {
        let mut r = runtime();
        assert!(start(&mut r, 1.0));
        r.advance_melee(1.22, 0.02);
        let before = r
            .state("player")
            .unwrap()
            .melee
            .as_ref()
            .unwrap()
            .wrist_offset
            .unwrap();
        r.interrupt_combat("player", 1.22, 0.4);
        assert!(r.active_melee_strikes(1.23).is_empty());
        assert!(!start(&mut r, 1.3));
        r.advance_melee(1.3, 0.08);
        let s = r.state("player").unwrap().melee.as_ref().unwrap();
        assert_eq!(s.phase, "recovery");
        assert!(s.wrist_offset.unwrap()[2] <= before[2]);
        r.advance_melee(1.7, 0.4);
        assert!(start(&mut r, 1.7));
    }
    #[test]
    fn wall_contact_uses_contact_time_and_resets_combo_without_delayed_damage() {
        let mut r = runtime();
        assert!(start(&mut r, 1.0));
        r.advance_melee(1.2, 0.2);
        r.obstruct_melee("player", 1, 1.15, 1.2);
        let s = r.state("player").unwrap().melee.as_ref().unwrap();
        assert!(s.contact_consumed);
        assert_eq!(s.combo_step, 0);
        assert!(r.active_melee_strikes(1.21).is_empty());
        r.advance_melee(1.3, 0.1);
        assert_eq!(
            r.state("player").unwrap().melee.as_ref().unwrap().phase,
            "recovery"
        );
    }
    #[test]
    fn authored_pose_is_continuous_at_all_phase_boundaries() {
        let a = action();
        for (left, lp, right, rp) in [
            ("windup", 1.0, "active", 0.0),
            ("active", 1.0, "recovery", 0.0),
            ("recovery", 1.0, "idle", 0.0),
        ] {
            assert_eq!(a.wrist_goal(left, lp), a.wrist_goal(right, rp));
        }
        let at = a.wrist_goal("active", a.contact_fraction);
        assert_eq!(at, a.pose.contact);
    }
    #[test]
    fn combo_damage_advances_then_expires_and_stun_clears_it() {
        let mut r = runtime();
        assert!(start(&mut r, 1.0));
        let first = r.active_melee_strikes(1.2)[0].damage;
        r.advance_melee(1.7, 0.7);
        assert!(start(&mut r, 1.7));
        assert!(r.active_melee_strikes(1.9)[0].damage > first);
        r.advance_melee(3.0, 1.3);
        assert!(start(&mut r, 3.0));
        assert_eq!(
            r.state("player")
                .unwrap()
                .melee
                .as_ref()
                .unwrap()
                .combo_step,
            1
        );
        r.interrupt_combat("player", 3.1, 0.3);
        assert_eq!(
            r.state("player")
                .unwrap()
                .melee
                .as_ref()
                .unwrap()
                .combo_step,
            0
        );
    }
    #[test]
    fn guard_has_directional_stamina_cost_and_recovers_after_exhaustion() {
        let mut r = runtime();
        r.melee_guard("player", true, [0.0, 0.0, -1.0]).unwrap();
        let (damage, blocked) = r.defend_melee("player", 20.0, [0.0, 0.0, -1.0], 1.0);
        assert!(blocked && damage < 4.0);
        assert_eq!(
            r.defend_melee("player", 20.0, [0.0, 0.0, 1.0], 1.1),
            (20.0, false)
        );
        for _ in 0..5 {
            assert!(r.defend_melee("player", 20.0, [0.0, 0.0, -1.0], 1.2).1);
        }
        assert!(!r.defend_melee("player", 20.0, [0.0, 0.0, -1.0], 1.3).1);
        r.advance_melee(2.0, 0.7);
        assert!(!r.state("player").unwrap().melee.as_ref().unwrap().blocking);
        for i in 0..12 {
            r.melee_guard("player", true, [0.0, 0.0, -1.0]).unwrap();
            r.advance_melee(2.1 + i as f64 * 0.1, 0.1);
        }
        assert!(r.state("player").unwrap().melee.as_ref().unwrap().blocking);
    }
    #[test]
    fn stamina_and_pose_do_not_depend_on_render_cadence() {
        let mut outputs = Vec::new();
        for rate in [30, 60, 144] {
            let mut r = runtime();
            assert!(start(&mut r, 1.0));
            for i in 1..=rate {
                r.advance_melee(1.0 + i as f64 / rate as f64, 1.0 / rate as f32);
            }
            let s = r.state("player").unwrap().melee.as_ref().unwrap();
            outputs.push((s.stamina, s.wrist_offset));
        }
        for (stamina, pose) in &outputs {
            assert!(
                (stamina - outputs[0].0).abs() < 0.0001,
                "cadence outputs {outputs:?}"
            );
            assert_eq!(*pose, outputs[0].1);
        }
    }
}

#[cfg(test)]
mod authority_tests {
    use super::*;
    fn runtime() -> PedRuntime {
        let mut p = PedRuntime::default();
        p.upsert(
            PedProfile {
                actor_id: "actor".into(),
                damage_entity: Some(1),
                melee: Some(MeleeProfile::default()),
                ..Default::default()
            },
            true,
        )
        .unwrap();
        p
    }
    #[test]
    fn suspension_clears_guard_once_without_an_ever_extending_recovery_timer() {
        let mut p = runtime();
        p.melee_guard("actor", true, [0.0, 0.0, 1.0]).unwrap();
        assert!(p.state("actor").unwrap().melee.as_ref().unwrap().blocking);
        assert!(p.suspend_melee("actor", 1.0));
        let deadline = p.state("actor").unwrap().melee.as_ref().unwrap().finish_at;
        for i in 1..=240 {
            let now = 1.0 + f64::from(i) / 120.0;
            p.advance_melee(now, 1.0 / 120.0);
            assert!(!p.suspend_melee("actor", now));
            let s = p.state("actor").unwrap().melee.as_ref().unwrap();
            assert!(!s.blocking && !s.guard_requested);
            assert_eq!(s.finish_at, deadline);
        }
        p.melee_guard("actor", true, [0.0, 0.0, 1.0]).unwrap();
        assert!(p.state("actor").unwrap().melee.as_ref().unwrap().blocking);
    }
    #[test]
    fn suspension_cancels_consumed_active_motion_and_rejects_its_late_sequence() {
        let mut p = runtime();
        assert!(p.begin_melee("actor", None, [0.0, 0.0, 1.0], 1.0).unwrap());
        // Contact may already have been consumed while its follow-through is active.
        let s = p.records.get_mut("actor").unwrap().melee.as_mut().unwrap();
        s.phase = "active".into();
        s.contact_consumed = true;
        let sequence = s.attacks;
        assert!(p.suspend_melee("actor", 1.2));
        assert!(!p.suspend_melee("actor", 1.21));
        assert!(!p.consume_melee_contact("actor", sequence));
        p.advance_melee(2.0, 0.8);
        assert!(p.begin_melee("actor", None, [0.0, 0.0, 1.0], 2.0).unwrap());
        assert!(!p.consume_melee_contact("actor", sequence));
    }
}
