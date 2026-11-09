//! Opt-in combat decisions. Geometry visibility is supplied by the host; health stays in PedRuntime.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CombatProfile {
    pub target_actor: String,
    pub director_asset: String,
    pub director: Option<super::director::DirectorModel>,
    pub sight_range: f32,
    pub memory_seconds: f32,
    pub reaction_seconds: f32,
    pub magazine_capacity: u16,
    pub reserve: u16,
    pub reload_seconds: f32,
    pub spread: f32,
    pub melee_only: bool,
}
impl Default for CombatProfile {
    fn default() -> Self {
        Self {
            target_actor: String::new(),
            director_asset: "combat/director/humanoid.json".into(),
            director: None,
            sight_range: 32.0,
            memory_seconds: 7.0,
            reaction_seconds: 0.8,
            magazine_capacity: 8,
            reserve: 80,
            reload_seconds: 2.4,
            spread: 0.035,
            melee_only: false,
        }
    }
}
impl CombatProfile {
    pub(super) fn validate(&self) -> Result<(), String> {
        validate_id("combat target", &self.target_actor)?;
        if self.director_asset.trim().is_empty() {
            return Err("missing combat director asset".into());
        }
        if let Some(director) = &self.director {
            director.validate()?;
        }
        for (name, n, min, max) in [
            ("sight range", self.sight_range, 1.0, 500.0),
            ("combat memory", self.memory_seconds, 0.1, 120.0),
            ("combat reaction", self.reaction_seconds, 0.0, 10.0),
            ("combat reload", self.reload_seconds, 0.1, 30.0),
            ("combat spread", self.spread, 0.0, 0.5),
        ] {
            tasks::finite_range(name, n, min, max)?;
        }
        if self.magazine_capacity == 0 || self.magazine_capacity > 500 {
            return Err("invalid combat magazine capacity".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatState {
    pub target_actor: String,
    pub melee_weapon: bool,
    pub melee_reach: Option<f32>,
    pub tactics: super::director::TacticalState,
    pub mode: String,
    pub style: String,
    pub magazine: u16,
    pub reserve: u16,
    pub shots: u64,
    pub visible: bool,
    pub reload_until: f64,
    pub last_seen_position: Option<[f32; 3]>,
    next_sense: f64,
    remembered_until: f64,
    pub(super) ready_at: f64,
}
impl CombatState {
    pub(super) fn new(p: &CombatProfile) -> Self {
        Self {
            target_actor: p.target_actor.clone(),
            melee_weapon: false,
            melee_reach: None,
            tactics: Default::default(),
            mode: "patrol".into(),
            style: if p.melee_only { "fists" } else { "ranged" }.into(),
            magazine: p.magazine_capacity,
            reserve: p.reserve,
            shots: 0,
            visible: false,
            reload_until: 0.0,
            last_seen_position: None,
            next_sense: 0.0,
            remembered_until: 0.0,
            ready_at: 0.0,
        }
    }
    pub(super) fn lose_weapon(&mut self) {
        self.reload_until = 0.0;
        self.next_sense = 0.0;
    }
    fn reload(&mut self, now: f64, p: &CombatProfile) {
        if self.reload_until > 0.0 && now >= self.reload_until {
            let count = p
                .magazine_capacity
                .saturating_sub(self.magazine)
                .min(self.reserve);
            self.magazine += count;
            self.reserve -= count;
            self.reload_until = 0.0;
        }
        if self.magazine == 0 && self.reserve > 0 && self.reload_until == 0.0 {
            self.reload_until = now + f64::from(p.reload_seconds);
        }
    }
    pub fn consume_shot(&mut self, now: f64, p: &CombatProfile) -> bool {
        self.reload(now, p);
        if self.mode != "engage"
            || !self.visible
            || now < self.ready_at
            || self.reload_until > 0.0
            || self.magazine == 0
        {
            return false;
        }
        self.magazine -= 1;
        self.shots += 1;
        self.reload(now, p);
        true
    }
}

impl PedRuntime {
    /// Only due, resident combatants need a geometry probe (at most 6.7 Hz per actor).
    pub fn combat_probes(&self, now: f64) -> Vec<(String, String, f32)> {
        self.records
            .values()
            .filter(|p| !p.dead)
            .filter_map(|p| {
                let c = p.profile.combat.as_ref()?;
                let s = p.combat.as_ref()?;
                (now >= s.next_sense).then(|| {
                    (
                        p.profile.actor_id.clone(),
                        s.target_actor.clone(),
                        c.sight_range,
                    )
                })
            })
            .collect()
    }
    pub fn tick_combat(
        &mut self,
        snapshot: &AgentWorldSnapshot,
        visibility: &BTreeMap<String, bool>,
        agents: &mut AgentRuntime,
    ) -> Result<Vec<AgentCommand>, String> {
        let now = snapshot.world_seconds;
        let mut commands = Vec::new();
        for ped in self.records.values_mut().filter(|p| !p.dead) {
            let Some(profile) = ped.profile.combat.as_ref() else {
                continue;
            };
            let state = ped.combat.as_mut().expect("combat profile has state");
            if !profile.melee_only && !ped.weapon_grip.lost && !state.melee_weapon {
                state.reload(now, profile);
            }
            if now < state.next_sense {
                continue;
            }
            state.next_sense =
                now + f64::from(profile.director.as_ref().map_or(0.15, |m| m.sense_interval));
            let actor = snapshot
                .actors
                .iter()
                .find(|a| a.id == ped.profile.actor_id && a.enabled && a.simulation_tier == "full");
            let target = snapshot
                .actors
                .iter()
                .find(|a| a.id == state.target_actor && a.enabled && a.simulation_tier == "full");
            let old = state.mode.clone();
            let old_style = state.style.clone();
            state.visible = actor.is_some()
                && target.is_some()
                && visibility.get(&ped.profile.actor_id) == Some(&true);
            let observed = if state.visible {
                target.map(|t| t.position)
            } else {
                state.last_seen_position
            };
            let distance = actor
                .zip(observed)
                .map_or(f32::INFINITY, |(a, b)| distance_between(a.position, b));
            let close = ped.profile.melee.as_ref().is_some_and(|m| {
                distance < m.range + if old_style == "fists" { 1.0 } else { 0.15 }
            });
            let fight_disarmed = super::grip::fight_disarmed(
                &ped.profile,
                &ped.weapon_grip,
                ped.melee.as_ref(),
                distance,
            );
            state.style = if ped.weapon_grip.lost && !fight_disarmed {
                "unarmed"
            } else if state.melee_weapon {
                "melee"
            } else if profile.melee_only
                || ped.weapon_grip.lost
                || ped.profile.melee.is_some()
                    && (close || state.magazine == 0 && state.reserve == 0)
            {
                "fists"
            } else {
                "ranged"
            }
            .into();
            if let Some(melee) = ped.melee.as_mut() {
                let facing = actor
                    .zip(observed)
                    .map(|(a, b)| [b[0] - a.position[0], 0.0, b[2] - a.position[2]]);
                if let Some(v) = facing {
                    let n = v[0].hypot(v[2]).max(0.001);
                    melee.facing = v.map(|x| x / n);
                }
            }
            let mut heard = None;
            if let (Some(actor), Some(_)) = (actor, target) {
                heard = snapshot
                    .stimuli
                    .iter()
                    .filter(|s| {
                        s.source == state.target_actor
                            && s.remaining_seconds > 0.0
                            && matches!(s.kind.as_str(), "gunshot" | "weapon.fire")
                            && distance_between(actor.position, s.position)
                                <= ped.profile.hearing_radius.min(s.radius)
                    })
                    .last()
                    .map(|s| s.position);
            }
            state.visible = actor.is_some()
                && target.is_some()
                && visibility.get(&ped.profile.actor_id) == Some(&true);
            if state.visible {
                state.last_seen_position = target.map(|t| t.position);
                state.remembered_until = now + f64::from(profile.memory_seconds);
                state.mode = "engage".into();
                if old != "engage" {
                    state.ready_at = now + f64::from(profile.reaction_seconds);
                }
            } else if actor.is_some() && target.is_some() {
                if let Some(position) = heard {
                    state.last_seen_position = Some(position);
                    state.remembered_until = now + f64::from(profile.memory_seconds);
                }
                state.mode = if now < state.remembered_until {
                    "search"
                } else {
                    "patrol"
                }
                .into();
            } else {
                state.mode = "patrol".into();
                state.visible = false;
            }
            if state.style == "unarmed" && state.mode != "patrol" {
                state.mode = "flee".into();
            }
            if state.mode == "patrol" {
                state.last_seen_position = None;
            }
            if old == state.mode && old_style == state.style {
                continue;
            }
            self.events.push(
                json!({"kind":"ped.combat.state","actor_id":ped.profile.actor_id,
                "state":state.mode,"target_actor":state.target_actor}),
            );
            if profile.director.is_some() {
                continue;
            }
            if let Some(cancel) = agents.clear_task(&ped.profile.agent_id, "combat.engagement")? {
                commands.push(cancel);
            }
            if state.mode != "patrol" {
                let task = if state.mode == "flee" {
                    AgentTaskKind::Flee {
                        threat_position: state
                            .last_seen_position
                            .unwrap_or(actor.unwrap().position),
                        threat_actor: Some(state.target_actor.clone()),
                        speed: ped.profile.move_speed,
                        safe_distance: ped.profile.flee_distance,
                        duration_seconds: ped.profile.flee_seconds,
                    }
                } else if state.visible {
                    AgentTaskKind::Combat {
                        target_actor: state.target_actor.clone(),
                        speed: ped.profile.move_speed,
                        range: if matches!(state.style.as_str(), "fists" | "melee") {
                            ped.profile.melee.as_ref().map_or(1.55, |m| m.range)
                        } else {
                            ped.profile.combat_range
                        },
                        cooldown_seconds: if state.style == "fists" {
                            0.65
                        } else {
                            ped.profile.combat_cooldown_seconds
                        },
                        damage: ped.profile.combat_damage,
                        duration_seconds: 3600.0,
                    }
                } else {
                    AgentTaskKind::GoToPosition {
                        position: state.last_seen_position.unwrap(),
                        speed: ped.profile.move_speed,
                        stop_distance: 1.0,
                    }
                };
                agents.set_task(
                    &ped.profile.agent_id,
                    AgentTaskDesc {
                        id: "combat.engagement".into(),
                        lane: AgentTaskLane::Reaction,
                        priority: 90,
                        task,
                    },
                )?;
            }
        }
        Ok(commands)
    }
    pub fn restore_combat_ammo(
        &mut self,
        id: &str,
        magazine: u64,
        reserve: u64,
    ) -> Result<(), String> {
        if magazine > u16::MAX as u64 || reserve > u16::MAX as u64 {
            return Err("invalid saved weapon ammunition".into());
        }
        let Some(ped) = self.records.get_mut(id) else {
            return Err(format!("unknown ammunition owner '{id}'"));
        };
        if let (Some(state), Some(profile)) = (&mut ped.combat, &ped.profile.combat) {
            state.magazine = (magazine as u16).min(profile.magazine_capacity);
            state.reserve = reserve as u16;
            state.reload_until = 0.0;
        }
        Ok(())
    }

    pub fn combat_shot(&mut self, actor_id: &str, now: f64) -> bool {
        let Some(ped) = self
            .records
            .get_mut(actor_id)
            .filter(|p| !p.dead && !p.weapon_grip.lost && p.stunned_until <= now)
        else {
            return false;
        };
        match (&ped.profile.combat, &mut ped.combat) {
            (Some(p), Some(s)) => {
                !p.melee_only
                    && !matches!(s.style.as_str(), "fists" | "melee")
                    && (p.director.is_none()
                        || s.tactics.attack_allowed && now <= s.tactics.permit_until)
                    && s.consume_shot(now, p)
            }
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn magazine_reload_respects_reaction_and_line_of_sight() {
        let profile = CombatProfile {
            magazine_capacity: 2,
            reserve: 1,
            reload_seconds: 2.0,
            ..Default::default()
        };
        let mut s = CombatState::new(&profile);
        s.mode = "engage".into();
        s.visible = true;
        s.ready_at = 1.0;
        assert!(!s.consume_shot(0.9, &profile));
        assert!(s.consume_shot(1.0, &profile));
        s.visible = false;
        assert!(!s.consume_shot(1.1, &profile));
        s.visible = true;
        assert!(s.consume_shot(1.2, &profile));
        assert!(!s.consume_shot(3.1, &profile));
        assert!(s.consume_shot(3.21, &profile));
        assert_eq!(s.reserve, 0);
        assert!(!s.consume_shot(20.0, &profile));
        assert_eq!(s.shots, 3);
        s.mode = "search".into();
        assert!(!s.consume_shot(30.0, &profile));
    }
    #[test]
    fn hit_stun_delays_shots_and_fist_style_does_not_consume_ammo() {
        let mut r = PedRuntime::default();
        r.upsert(
            PedProfile {
                actor_id: "enemy".into(),
                agent_id: "enemy.ai".into(),
                combat: Some(CombatProfile {
                    target_actor: "player".into(),
                    ..Default::default()
                }),
                melee: Some(melee::MeleeProfile::default()),
                ..Default::default()
            },
            true,
        )
        .unwrap();
        let c = r.records.get_mut("enemy").unwrap().combat.as_mut().unwrap();
        c.mode = "engage".into();
        c.visible = true;
        assert!(r.combat_shot("enemy", 1.0));
        r.interrupt_combat("enemy", 1.01, 0.3);
        assert!(!r.combat_shot("enemy", 1.1));
        assert!(r.combat_shot("enemy", 1.4));
        r.records
            .get_mut("enemy")
            .unwrap()
            .combat
            .as_mut()
            .unwrap()
            .style = "fists".into();
        let magazine = r.state("enemy").unwrap().combat.as_ref().unwrap().magazine;
        assert!(!r.combat_shot("enemy", 2.0));
        assert!(r
            .begin_melee("enemy", Some("player".into()), [0.0, 0.0, -1.0], 2.0)
            .unwrap());
        assert_eq!(
            r.state("enemy").unwrap().combat.as_ref().unwrap().magazine,
            magazine
        );
    }
}
