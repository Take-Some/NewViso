//! Ped state and event decisions; locomotion stays in the generic agent task stack.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PedRelationship { Respect, Like, Ignore, Dislike, Wanted, Hate, Dead }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PedProfile {
    pub actor_id: String,
    pub agent_id: String,
    pub group: String,
    pub max_health: f32,
    pub armour: f32,
    pub armed: bool,
    pub invulnerable: bool,
    pub block_events: bool,
    pub hearing_radius: f32,
    pub flee_distance: f32,
    pub flee_seconds: f32,
    pub move_speed: f32,
    pub combat_range: f32,
    pub combat_cooldown_seconds: f32,
    pub combat_damage: f32,
    pub capsule_radius: f32,
    pub capsule_height: f32,
    pub death_clip: Option<String>,
}

impl Default for PedProfile {
    fn default() -> Self {
        Self { actor_id: String::new(), agent_id: String::new(), group: "civilian".into(),
            max_health: 200.0, armour: 0.0, armed: false, invulnerable: false,
            block_events: false, hearing_radius: 45.0, flee_distance: 30.0, flee_seconds: 12.0,
            move_speed: 3.5, combat_range: 18.0, combat_cooldown_seconds: 0.6,
            combat_damage: 20.0, capsule_radius: 0.3, capsule_height: 1.8, death_clip: None }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PedState {
    pub profile: PedProfile,
    pub health: f32,
    pub armour: f32,
    pub dead: bool,
    pub last_damage_source: Option<String>,
    #[serde(skip)] handled_events: BTreeMap<String, f64>,
    #[serde(skip)] pending_threat: Option<(String, [f32; 3])>,
}

#[derive(Clone, Debug, Default)]
pub struct PedRuntime {
    records: BTreeMap<String, PedState>,
    relationships: BTreeMap<(String, String), PedRelationship>,
    events: Vec<Value>,
}

impl PedRuntime {
    pub fn upsert(&mut self, profile: PedProfile, reset: bool) -> Result<(), String> {
        validate_id("ped actor", &profile.actor_id)?; validate_id("ped agent", &profile.agent_id)?;
        validate_id("ped relationship group", &profile.group)?;
        for (name, n, min, max) in [
            ("health", profile.max_health, 1.0, 1.0e6), ("armour", profile.armour, 0.0, 1.0e6),
            ("hearing", profile.hearing_radius, 0.0, 100_000.0),
            ("flee distance", profile.flee_distance, 0.1, 100_000.0),
            ("flee duration", profile.flee_seconds, 0.1, 86_400.0),
            ("move speed", profile.move_speed, 0.001, 1000.0),
            ("combat range", profile.combat_range, 0.25, 10_000.0),
            ("combat cooldown", profile.combat_cooldown_seconds, 0.05, 3600.0),
            ("combat damage", profile.combat_damage, 0.0, 100_000.0),
            ("capsule radius", profile.capsule_radius, 0.05, 10.0),
            ("capsule height", profile.capsule_height, profile.capsule_radius * 2.0, 30.0),
        ] { tasks::finite_range(name, n, min, max)?; }
        if profile.death_clip.as_ref().is_some_and(|p| p.trim().is_empty() || p.len() > 512) {
            return Err("invalid ped death clip".into());
        }
        if let Some(state) = self.records.get_mut(&profile.actor_id).filter(|_| !reset) {
            state.health = state.health.min(profile.max_health);
            state.profile = profile;
        } else {
            self.records.insert(profile.actor_id.clone(), PedState { health: profile.max_health,
                armour: profile.armour, profile, dead: false, last_damage_source: None,
                handled_events: BTreeMap::new(), pending_threat: None });
        }
        Ok(())
    }

    pub fn restore_vitals(&mut self, actor_id: &str, health: f32, armour: f32) -> Result<(), String> {
        let ped = self.records.get_mut(actor_id).ok_or_else(|| format!("ped '{actor_id}' does not exist"))?;
        tasks::finite_range("saved ped health", health, 0.0, ped.profile.max_health)?;
        tasks::finite_range("saved ped armour", armour, 0.0, 1.0e6)?;
        ped.health = health; ped.armour = armour; ped.dead = health == 0.0;
        Ok(())
    }

    pub fn remove(&mut self, actor_id: &str) { self.records.remove(actor_id); }
    pub fn state(&self, actor_id: &str) -> Option<&PedState> { self.records.get(actor_id) }
    pub fn states(&self) -> impl Iterator<Item = &PedState> { self.records.values() }
    pub fn is_dead(&self, actor_id: &str) -> bool { self.state(actor_id).is_some_and(|p| p.dead) }

    pub fn set_relationship(&mut self, from: String, to: String, relation: PedRelationship) -> Result<(), String> {
        validate_id("relationship source", &from)?; validate_id("relationship target", &to)?;
        self.relationships.insert((from, to), relation); Ok(())
    }

    pub fn damage(&mut self, actor_id: &str, source: String, amount: f32,
        origin: [f32; 3], agents: &mut AgentRuntime) -> Result<Option<AgentCommand>, String> {
        tasks::finite_range("ped damage", amount, 0.0, 1.0e6)?;
        tasks::validate_position(origin)?; validate_id("damage source", &source)?;
        let state = self.records.get_mut(actor_id).ok_or_else(|| format!("ped '{actor_id}' does not exist"))?;
        if state.dead || state.profile.invulnerable || amount == 0.0 { return Ok(None); }
        let absorbed = amount.min(state.armour);
        state.armour -= absorbed;
        state.health = (state.health - (amount - absorbed)).max(0.0);
        state.last_damage_source = Some(source.clone());
        state.pending_threat = Some((source.clone(), origin));
        state.dead = state.health <= 0.0;
        self.events.push(json!({"kind": "ped.damage", "actor_id": actor_id, "source": source,
            "amount": amount, "absorbed": absorbed, "health": state.health, "armour": state.armour}));
        if state.dead {
            self.events.push(json!({"kind": "ped.death", "actor_id": actor_id, "source": source}));
            return agents.clear_tasks(&state.profile.agent_id);
        }
        Ok(None)
    }

    pub fn tick(&mut self, snapshot: &AgentWorldSnapshot, agents: &mut AgentRuntime) -> Result<(), String> {
        let live = snapshot.actors.iter().filter(|a| a.enabled).map(|a| (a.id.as_str(), a))
            .collect::<BTreeMap<_, _>>();
        let groups = self.records.iter().map(|(id, p)| (id.clone(), p.profile.group.clone()))
            .collect::<BTreeMap<_, _>>();
        let dead = self.records.iter().filter(|(_, p)| p.dead).map(|(id, _)| id.clone()).collect::<BTreeSet<_>>();
        for state in self.records.values_mut().filter(|p| !p.dead) {
            let Some(actor) = live.get(state.profile.actor_id.as_str()) else { continue; };
            state.handled_events.retain(|_, until| *until > snapshot.world_seconds);
            let mut threat = state.pending_threat.take().map(|(source, position)| (100, source, position));
            if !state.profile.block_events {
                for stimulus in &snapshot.stimuli {
                    if stimulus.source == state.profile.actor_id || stimulus.remaining_seconds <= 0.0
                        || state.handled_events.contains_key(&stimulus.id) { continue; }
                    let distance = distance_between(actor.position, stimulus.position);
                    if distance > state.profile.hearing_radius || distance > stimulus.radius { continue; }
                    let priority = match stimulus.kind.as_str() {
                        "explosion" => 90, "gunshot" | "weapon.fire" => 80,
                        "danger" | "vehicle.collision" => 70, "threat" => 60, _ => continue,
                    };
                    if threat.as_ref().is_none_or(|(p, _, _)| priority > *p) {
                        threat = Some((priority, stimulus.source.clone(), stimulus.position));
                    }
                    // A sustained sound is handled once; renewed shots carry new ids.
                    state.handled_events.insert(stimulus.id.clone(), snapshot.world_seconds + 30.0);
                    while state.handled_events.len() > 128 {
                        let oldest = state.handled_events.iter().min_by(|a, b| a.1.total_cmp(b.1)).unwrap().0.clone();
                        state.handled_events.remove(&oldest);
                    }
                }
            }
            let Some((priority, source, position)) = threat else { continue; };
            let relation = groups.get(&source).and_then(|group| self.relationships.get(&(state.profile.group.clone(), group.clone())))
                .copied().unwrap_or(PedRelationship::Dislike);
            if state.profile.block_events || matches!(relation, PedRelationship::Ignore | PedRelationship::Like | PedRelationship::Respect) {
                continue;
            }
            // Refresh a running reaction in place: repeated shots must not reset its action cooldown.
            let active = agents.agents.get(&state.profile.agent_id)
                .and_then(|r| r.tasks.get("ped.reaction"))
                .is_some_and(|t| t.status == AgentTaskStatus::Running);
            if active { continue; }
            let combat = state.profile.armed && live.contains_key(source.as_str()) && !dead.contains(&source);
            let task = if combat {
                AgentTaskKind::Combat { target_actor: source.clone(), speed: state.profile.move_speed,
                    range: state.profile.combat_range, cooldown_seconds: state.profile.combat_cooldown_seconds,
                    damage: state.profile.combat_damage, duration_seconds: state.profile.flee_seconds * 2.0 }
            } else {
                AgentTaskKind::Flee { threat_position: position, threat_actor: live.contains_key(source.as_str()).then_some(source.clone()),
                    speed: state.profile.move_speed, safe_distance: state.profile.flee_distance,
                    duration_seconds: state.profile.flee_seconds }
            };
            agents.set_task(&state.profile.agent_id, AgentTaskDesc { id: "ped.reaction".into(),
                lane: AgentTaskLane::Reaction, priority, task })?;
            self.events.push(json!({"kind": "ped.reaction", "actor_id": state.profile.actor_id,
                "source": source, "reaction": if combat { "combat" } else { "flee" }, "priority": priority}));
        }
        Ok(())
    }

    pub fn drain_events(&mut self) -> Vec<Value> { std::mem::take(&mut self.events) }
    pub fn runtime_state(&self) -> Value { json!({"schema": "newviso.peds.v1", "peds": self.records.values().collect::<Vec<_>>()}) }
}
