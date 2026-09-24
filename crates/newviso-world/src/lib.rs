mod clock;
mod data;
mod navigation;
mod persistence;
pub use persistence::MAX_CHECKPOINT_BYTES;
mod reality;
mod scheduler;
mod simulation;

pub use clock::WorldClockPolicyDesc;
pub use data::{
    AmbientModelSetDesc, LivingWorldZoneDesc, PopulationChannelDesc, PopulationStreamingPolicyDesc,
    RelationshipRuleDesc, ScenarioPointDesc, WorldStimulusDesc,
};
pub use navigation::{WorldNavEdgeDesc, WorldNavNodeDesc, WorldTravelRequestDesc};
pub use reality::{WorldRealityEventDesc, WorldScenarioReservationDesc};
pub use scheduler::{WorldProcessDesc, WorldScheduledEventDesc};
pub use simulation::{WorldActorDesc, WorldObserverDesc, WorldSimulationPolicyDesc};

use clock::{WorldClockRuntime, WorldStep};
use data::{
    point_in_aabb, valid_json_payload, valid_label, validate_id, zone_volume, ActiveStimulus,
};
use navigation::{
    advance_toward, nearest_node, shortest_route, WorldTravelCompletion, WorldTravelRecord,
};
use reality::{
    WorldRealityEventRecord, WorldRepresentationChange, WorldScenarioReservationRecord,
    MAX_REALITY_HISTORY,
};
use scheduler::{
    first_due_at_or_after, ScheduledWorldEvent, WorldEventDelivery, WorldFactRecord,
    WorldProcessActivation, WorldProcessRecord,
};
use serde_json::{json, Value};
use simulation::{actor_tier, ActorRecord, ActorUpdateTicket, SimulationTier};
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct WorldActorRuntimeView {
    pub id: String,
    pub position: [f32; 3],
    pub representation: &'static str,
    pub enabled: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LivingWorldRuntime {
    clock: WorldClockRuntime,
    simulation_policy: WorldSimulationPolicyDesc,
    observers: BTreeMap<String, WorldObserverDesc>,
    actors: BTreeMap<String, ActorRecord>,
    processes: BTreeMap<String, WorldProcessRecord>,
    scheduled_events: BTreeMap<String, ScheduledWorldEvent>,
    facts: BTreeMap<String, WorldFactRecord>,
    population_channels: BTreeMap<String, PopulationChannelDesc>,
    population_streaming: PopulationStreamingPolicyDesc,
    zones: BTreeMap<String, LivingWorldZoneDesc>,
    model_sets: BTreeMap<String, AmbientModelSetDesc>,
    scenario_points: BTreeMap<String, ScenarioPointDesc>,
    #[serde(with = "persistence::relationships")]
    relationships: BTreeMap<(String, String), RelationshipRuleDesc>,
    stimuli: BTreeMap<String, ActiveStimulus>,
    #[serde(skip)]
    frame_actor_updates: Vec<ActorUpdateTicket>,
    #[serde(skip)]
    frame_process_activations: Vec<WorldProcessActivation>,
    #[serde(skip)]
    frame_events: Vec<WorldEventDelivery>,
    next_stimulus_id: u64,
    next_event_id: u64,
    dropped_expired_events: u64,
    reality_events: Vec<WorldRealityEventRecord>,
    #[serde(skip)]
    frame_reality_events: Vec<WorldRealityEventRecord>,
    pending_reality_events: Vec<WorldRealityEventRecord>,
    scenario_reservations: BTreeMap<String, WorldScenarioReservationRecord>,
    nav_nodes: BTreeMap<String, WorldNavNodeDesc>,
    nav_edges: BTreeMap<String, WorldNavEdgeDesc>,
    travel: BTreeMap<String, WorldTravelRecord>,
    #[serde(skip)]
    frame_travel_completions: Vec<WorldTravelCompletion>,
    #[serde(skip)]
    frame_representation_changes: Vec<WorldRepresentationChange>,
    next_reality_event_id: u64,
    next_reality_sequence: u64,
}

impl Default for LivingWorldRuntime {
    fn default() -> Self {
        Self {
            clock: WorldClockRuntime::default(),
            simulation_policy: WorldSimulationPolicyDesc::default(),
            observers: BTreeMap::new(),
            actors: BTreeMap::new(),
            processes: BTreeMap::new(),
            scheduled_events: BTreeMap::new(),
            facts: BTreeMap::new(),
            population_channels: BTreeMap::new(),
            population_streaming: PopulationStreamingPolicyDesc::default(),
            zones: BTreeMap::new(),
            model_sets: BTreeMap::new(),
            scenario_points: BTreeMap::new(),
            relationships: BTreeMap::new(),
            stimuli: BTreeMap::new(),
            frame_actor_updates: Vec::new(),
            frame_process_activations: Vec::new(),
            frame_events: Vec::new(),
            next_stimulus_id: 1,
            next_event_id: 1,
            dropped_expired_events: 0,
            reality_events: Vec::new(),
            frame_reality_events: Vec::new(),
            pending_reality_events: Vec::new(),
            scenario_reservations: BTreeMap::new(),
            nav_nodes: BTreeMap::new(),
            nav_edges: BTreeMap::new(),
            travel: BTreeMap::new(),
            frame_travel_completions: Vec::new(),
            frame_representation_changes: Vec::new(),
            next_reality_event_id: 1,
            next_reality_sequence: 1,
        }
    }
}

impl LivingWorldRuntime {
    pub fn actor_runtime_views(&self) -> Vec<WorldActorRuntimeView> {
        self.actors
            .values()
            .map(|actor| WorldActorRuntimeView {
                id: actor.desc.id.clone(),
                position: actor.desc.position,
                representation: representation_for_tier(actor.last_tier),
                enabled: actor.desc.enabled,
            })
            .collect()
    }

    pub fn configure_clock(&mut self, policy: WorldClockPolicyDesc) -> Result<(), String> {
        self.clock.configure(policy)
    }

    pub fn set_world_seconds(&mut self, world_seconds: f64) -> Result<(), String> {
        self.clock.set_world_seconds(world_seconds)?;
        for process in self.processes.values_mut() {
            process.next_due_seconds = first_due_at_or_after(
                self.clock.world_seconds,
                process.desc.interval_seconds,
                process.desc.phase_seconds,
            );
        }
        Ok(())
    }

    pub fn configure_simulation(
        &mut self,
        policy: WorldSimulationPolicyDesc,
    ) -> Result<(), String> {
        policy.validate()?;
        self.simulation_policy = policy;
        Ok(())
    }

    pub fn upsert_observer(&mut self, desc: WorldObserverDesc) -> Result<(), String> {
        desc.validate()?;
        self.observers.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_observer(&mut self, id: &str) {
        self.observers.remove(id.trim());
    }

    pub fn upsert_actor(&mut self, desc: WorldActorDesc) -> Result<(), String> {
        desc.validate()?;
        if let Some(existing) = self.actors.get_mut(&desc.id) {
            existing.desc = desc;
        } else {
            self.actors.insert(
                desc.id.clone(),
                ActorRecord {
                    desc,
                    last_update_seconds: self.clock.world_seconds,
                    next_update_seconds: self.clock.world_seconds,
                    last_tier: SimulationTier::Background,
                },
            );
        }
        Ok(())
    }

    pub fn remove_actor(&mut self, id: &str) {
        self.actors.remove(id.trim());
        self.travel.remove(id.trim());
        self.scenario_reservations
            .retain(|_, record| record.desc.actor_id != id.trim());
    }

    pub fn upsert_process(&mut self, mut desc: WorldProcessDesc) -> Result<(), String> {
        desc.validate()?;
        desc.phase_seconds = desc.phase_seconds.rem_euclid(desc.interval_seconds);

        if let Some(existing) = self.processes.get_mut(&desc.id) {
            let schedule_changed = existing.desc.interval_seconds != desc.interval_seconds
                || existing.desc.phase_seconds != desc.phase_seconds;
            existing.desc = desc;
            if schedule_changed {
                existing.next_due_seconds = first_due_at_or_after(
                    self.clock.world_seconds,
                    existing.desc.interval_seconds,
                    existing.desc.phase_seconds,
                );
            }
        } else {
            let next_due_seconds = first_due_at_or_after(
                self.clock.world_seconds,
                desc.interval_seconds,
                desc.phase_seconds,
            );
            self.processes.insert(
                desc.id.clone(),
                WorldProcessRecord {
                    desc,
                    next_due_seconds,
                },
            );
        }
        Ok(())
    }

    pub fn remove_process(&mut self, id: &str) {
        self.processes.remove(id.trim());
    }

    pub fn schedule_event(&mut self, mut desc: WorldScheduledEventDesc) -> Result<String, String> {
        if desc.id.trim().is_empty() {
            desc.id = format!("world.event.{:016x}", self.next_event_id);
            self.next_event_id = self.next_event_id.wrapping_add(1).max(1);
        }
        desc.validate()?;

        let id = desc.id.clone();
        let due_world_seconds = self.clock.world_seconds + desc.delay_seconds;
        self.scheduled_events.insert(
            id.clone(),
            ScheduledWorldEvent {
                expires_world_seconds: due_world_seconds + desc.ttl_seconds,
                due_world_seconds,
                desc,
            },
        );
        Ok(id)
    }

    pub fn cancel_event(&mut self, id: &str) {
        self.scheduled_events.remove(id.trim());
    }

    pub fn record_reality_event(
        &mut self,
        mut desc: WorldRealityEventDesc,
    ) -> Result<String, String> {
        if desc.id.trim().is_empty() {
            desc.id = self.allocate_reality_id();
        }
        desc.validate()?;
        let id = desc.id.clone();
        self.push_reality_event(desc, self.clock.world_seconds);
        Ok(id)
    }

    fn push_reality_event(
        &mut self,
        mut desc: WorldRealityEventDesc,
        occurred_world_seconds: f64,
    ) -> String {
        if desc.id.trim().is_empty() {
            desc.id = self.allocate_reality_id();
        }
        let id = desc.id.clone();
        let record = WorldRealityEventRecord {
            sequence: self.next_reality_sequence,
            occurred_world_seconds,
            desc,
        };
        self.next_reality_sequence = self.next_reality_sequence.wrapping_add(1).max(1);
        // Mutations issued after the script snapshot must survive until the next frame.
        self.pending_reality_events.push(record.clone());
        self.reality_events.push(record);
        if self.reality_events.len() > MAX_REALITY_HISTORY {
            let overflow = self.reality_events.len() - MAX_REALITY_HISTORY;
            self.reality_events.drain(0..overflow);
        }
        id
    }

    fn allocate_reality_id(&mut self) -> String {
        loop {
            let id = format!("reality.event.{:016x}", self.next_reality_event_id);
            self.next_reality_event_id = self.next_reality_event_id.wrapping_add(1).max(1);
            if !self
                .reality_events
                .iter()
                .chain(&self.pending_reality_events)
                .chain(&self.frame_reality_events)
                .any(|event| event.desc.id == id)
            {
                return id;
            }
        }
    }

    pub fn reserve_scenario(
        &mut self,
        mut desc: WorldScenarioReservationDesc,
    ) -> Result<String, String> {
        if desc.id.trim().is_empty() {
            desc.id = format!("scenario.reservation.{:016x}", self.next_reality_event_id);
            self.next_reality_event_id = self.next_reality_event_id.wrapping_add(1).max(1);
        }
        desc.validate()?;
        if !self.scenario_points.contains_key(&desc.scenario_point_id) {
            return Err(format!(
                "world scenario reservation references unknown scenario point '{}'",
                desc.scenario_point_id
            ));
        }
        if !self.actors.contains_key(&desc.actor_id) {
            return Err(format!(
                "world scenario reservation references unknown actor '{}'",
                desc.actor_id
            ));
        }

        let starts = self.clock.world_seconds + desc.delay_seconds;
        let ends = starts + desc.duration_seconds;
        {
            let conflict = self.scenario_reservations.values().any(|existing| {
                existing.desc.id != desc.id
                    && (existing.desc.exclusive || desc.exclusive)
                    && existing.desc.scenario_point_id == desc.scenario_point_id
                    && intervals_overlap(
                        starts,
                        ends,
                        existing.starts_world_seconds,
                        existing.ends_world_seconds,
                    )
            });
            if conflict {
                return Err(format!(
                    "exclusive scenario point '{}' is already reserved for the requested world-time interval",
                    desc.scenario_point_id
                ));
            }
        }

        let id = desc.id.clone();
        self.scenario_reservations.insert(
            id.clone(),
            WorldScenarioReservationRecord {
                desc,
                starts_world_seconds: starts,
                ends_world_seconds: ends,
            },
        );
        Ok(id)
    }

    pub fn release_scenario_reservation(&mut self, id: &str) {
        self.scenario_reservations.remove(id.trim());
    }

    pub fn upsert_nav_node(&mut self, desc: WorldNavNodeDesc) -> Result<(), String> {
        desc.validate()?;
        self.nav_nodes.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_nav_node(&mut self, id: &str) -> Result<(), String> {
        let id = id.trim();
        if self
            .nav_edges
            .values()
            .any(|edge| edge.from == id || edge.to == id)
        {
            return Err(format!(
                "world navigation node '{id}' is still referenced by an edge"
            ));
        }
        if self
            .travel
            .values()
            .any(|travel| travel.route.iter().any(|node| node == id))
        {
            return Err(format!(
                "world navigation node '{id}' is still referenced by active travel"
            ));
        }
        self.nav_nodes.remove(id);
        Ok(())
    }

    pub fn upsert_nav_edge(&mut self, desc: WorldNavEdgeDesc) -> Result<(), String> {
        desc.validate()?;
        if !self.nav_nodes.contains_key(&desc.from) {
            return Err(format!(
                "world navigation edge '{}' references missing from node '{}'",
                desc.id, desc.from
            ));
        }
        if !self.nav_nodes.contains_key(&desc.to) {
            return Err(format!(
                "world navigation edge '{}' references missing to node '{}'",
                desc.id, desc.to
            ));
        }
        self.nav_edges.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_nav_edge(&mut self, id: &str) {
        self.nav_edges.remove(id.trim());
    }

    pub fn start_travel(&mut self, desc: WorldTravelRequestDesc) -> Result<(), String> {
        desc.validate()?;
        let actor_position = self
            .actors
            .get(&desc.actor_id)
            .ok_or_else(|| format!("world travel actor '{}' does not exist", desc.actor_id))?
            .desc
            .position;
        let start_node = match desc.start_node.as_deref() {
            Some(id) => {
                if !self.nav_nodes.contains_key(id) {
                    return Err(format!("world travel start node '{id}' does not exist"));
                }
                id.to_owned()
            }
            None => nearest_node(&self.nav_nodes, actor_position)
                .ok_or_else(|| "world navigation graph has no nodes".to_owned())?,
        };
        let route = shortest_route(
            &self.nav_nodes,
            &self.nav_edges,
            &start_node,
            &desc.destination_node,
        )?;
        let actor_id = desc.actor_id.clone();
        self.travel.insert(
            actor_id.clone(),
            WorldTravelRecord {
                actor_id: actor_id.clone(),
                route,
                next_waypoint_index: 0,
                destination_node: desc.destination_node.clone(),
                speed: desc.speed,
                mode: desc.mode.clone(),
                payload: desc.payload.clone(),
                started_world_seconds: self.clock.world_seconds,
                last_update_world_seconds: self.clock.world_seconds,
                distance_travelled: 0.0,
            },
        );
        self.push_reality_event(
            WorldRealityEventDesc {
                id: String::new(),
                kind: "world.travel.started".to_owned(),
                source: "world.navigation".to_owned(),
                cause: None,
                participants: vec![actor_id],
                position: Some(actor_position),
                importance: 1.0,
                tags: vec!["navigation".to_owned(), "travel".to_owned()],
                payload: json!({
                    "start_node": start_node,
                    "destination_node": desc.destination_node,
                    "mode": desc.mode,
                    "payload": desc.payload,
                }),
            },
            self.clock.world_seconds,
        );
        Ok(())
    }

    pub fn cancel_travel(&mut self, actor_id: &str) {
        self.travel.remove(actor_id.trim());
    }

    pub fn set_fact(&mut self, key: &str, value: Value) -> Result<(), String> {
        self.set_fact_with_cause(key, value, None)
    }

    pub fn set_fact_with_cause(
        &mut self,
        key: &str,
        value: Value,
        cause: Option<String>,
    ) -> Result<(), String> {
        validate_id("world fact", key)?;
        if let Some(cause) = cause.as_deref() {
            validate_id("world fact cause", cause)?;
        }
        if !valid_json_payload(&value) {
            return Err("world fact payload exceeds generic backend limits".to_owned());
        }
        let revision = self
            .facts
            .get(key)
            .map(|record| record.revision.wrapping_add(1).max(1))
            .unwrap_or(1);
        let previous_value = self.facts.get(key).map(|record| record.value.clone());
        let fact_value = value.clone();
        self.facts.insert(
            key.to_owned(),
            WorldFactRecord {
                value,
                revision,
                updated_world_seconds: self.clock.world_seconds,
            },
        );
        self.push_reality_event(
            WorldRealityEventDesc {
                id: String::new(),
                kind: "world.fact.changed".to_owned(),
                source: "world.facts".to_owned(),
                cause,
                participants: Vec::new(),
                position: None,
                importance: 1.0,
                tags: vec!["fact".to_owned()],
                payload: json!({
                    "key": key,
                    "revision": revision,
                    "previous_value": previous_value,
                    "value": fact_value,
                }),
            },
            self.clock.world_seconds,
        );
        Ok(())
    }

    pub fn remove_fact(&mut self, key: &str) {
        if let Some(record) = self.facts.remove(key.trim()) {
            self.push_reality_event(
                WorldRealityEventDesc {
                    id: String::new(),
                    kind: "world.fact.removed".to_owned(),
                    source: "world.facts".to_owned(),
                    cause: None,
                    participants: Vec::new(),
                    position: None,
                    importance: 1.0,
                    tags: vec!["fact".to_owned()],
                    payload: json!({
                        "key": key.trim(),
                        "revision": record.revision,
                        "previous_value": record.value,
                    }),
                },
                self.clock.world_seconds,
            );
        }
    }

    pub fn upsert_population_channel(&mut self, desc: PopulationChannelDesc) -> Result<(), String> {
        validate_id("population channel", &desc.id)?;
        if !desc.density.is_finite()
            || !(0.0..=64.0).contains(&desc.density)
            || desc.max_active > 1_000_000
            || !desc.spawn_radius.is_finite()
            || desc.spawn_radius < 0.0
            || !desc.despawn_radius.is_finite()
            || desc.despawn_radius < desc.spawn_radius
            || desc.creation_budget_per_tick > 1_000_000
            || desc.removal_budget_per_tick > 1_000_000
            || !desc.update_interval_seconds.is_finite()
            || !(0.0..=3600.0).contains(&desc.update_interval_seconds)
            || desc.model_set.as_deref().is_some_and(|id| !valid_label(id))
            || desc.tags.iter().any(|tag| !valid_label(tag))
            || data::invalid_float_map(&desc.parameters)
        {
            return Err("invalid generic PopulationChannelDesc parameters".to_owned());
        }
        self.population_channels.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_population_channel(&mut self, id: &str) {
        self.population_channels.remove(id.trim());
    }

    pub fn set_population_streaming_policy(
        &mut self,
        desc: PopulationStreamingPolicyDesc,
    ) -> Result<(), String> {
        if desc.max_resident_sets == 0
            || desc.max_resident_sets > 65_536
            || desc.request_budget_per_tick > 65_536
            || desc.eviction_budget_per_tick > 65_536
            || desc
                .fallback_set
                .as_deref()
                .is_some_and(|id| !valid_label(id))
        {
            return Err("invalid generic PopulationStreamingPolicyDesc parameters".to_owned());
        }
        self.population_streaming = desc;
        Ok(())
    }

    pub fn upsert_zone(&mut self, desc: LivingWorldZoneDesc) -> Result<(), String> {
        validate_id("living-world zone", &desc.id)?;
        if desc
            .min
            .iter()
            .chain(desc.max.iter())
            .any(|value| !value.is_finite())
            || desc
                .min
                .iter()
                .zip(desc.max.iter())
                .any(|(min, max)| min > max)
            || desc.tags.iter().any(|tag| !valid_label(tag))
            || data::invalid_float_map(&desc.parameters)
        {
            return Err("invalid generic LivingWorldZoneDesc parameters".to_owned());
        }
        self.zones.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_zone(&mut self, id: &str) {
        self.zones.remove(id.trim());
    }

    pub fn upsert_model_set(&mut self, desc: AmbientModelSetDesc) -> Result<(), String> {
        validate_id("ambient model set", &desc.id)?;
        if !valid_label(&desc.category)
            || desc.assets.is_empty()
            || desc.assets.len() > 4096
            || desc
                .assets
                .iter()
                .any(|asset| asset.trim().is_empty() || asset.len() > 1024)
            || (!desc.weights.is_empty() && desc.weights.len() != desc.assets.len())
            || desc
                .weights
                .iter()
                .any(|weight| !weight.is_finite() || *weight < 0.0)
            || desc.tags.iter().any(|tag| !valid_label(tag))
        {
            return Err("invalid generic AmbientModelSetDesc parameters".to_owned());
        }
        self.model_sets.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_model_set(&mut self, id: &str) {
        self.model_sets.remove(id.trim());
    }

    pub fn upsert_scenario_point(&mut self, desc: ScenarioPointDesc) -> Result<(), String> {
        validate_id("scenario point", &desc.id)?;
        if !valid_label(&desc.kind)
            || desc
                .group
                .as_deref()
                .is_some_and(|group| !valid_label(group))
            || desc.position.iter().any(|value| !value.is_finite())
            || !desc.heading_degrees.is_finite()
            || desc.heading_degrees.abs() > 1.0e6
            || !desc.radius.is_finite()
            || !(0.001..=1_000_000.0).contains(&desc.radius)
            || !desc.probability.is_finite()
            || !(0.0..=1.0).contains(&desc.probability)
            || desc
                .model_set
                .as_deref()
                .is_some_and(|model_set| !valid_label(model_set))
            || desc.tags.iter().any(|tag| !valid_label(tag))
            || data::invalid_float_map(&desc.parameters)
        {
            return Err("invalid generic ScenarioPointDesc parameters".to_owned());
        }
        self.scenario_points.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_scenario_point(&mut self, id: &str) {
        self.scenario_points.remove(id.trim());
        self.scenario_reservations
            .retain(|_, record| record.desc.scenario_point_id != id.trim());
    }

    pub fn upsert_relationship(&mut self, desc: RelationshipRuleDesc) -> Result<(), String> {
        if !valid_label(&desc.source_group)
            || !valid_label(&desc.target_group)
            || !valid_label(&desc.relation)
            || !desc.weight.is_finite()
            || desc.weight.abs() > 1.0e6
            || desc.tags.iter().any(|tag| !valid_label(tag))
        {
            return Err("invalid generic RelationshipRuleDesc parameters".to_owned());
        }
        self.relationships
            .insert((desc.source_group.clone(), desc.target_group.clone()), desc);
        Ok(())
    }

    pub fn remove_relationship(&mut self, source_group: &str, target_group: &str) {
        self.relationships.remove(&(
            source_group.trim().to_owned(),
            target_group.trim().to_owned(),
        ));
    }

    pub fn emit_stimulus(&mut self, mut desc: WorldStimulusDesc) -> Result<String, String> {
        if desc.id.trim().is_empty() {
            desc.id = format!("stimulus.{:016x}", self.next_stimulus_id);
            self.next_stimulus_id = self.next_stimulus_id.wrapping_add(1).max(1);
        }
        validate_id("world stimulus", &desc.id)?;
        if !valid_label(&desc.kind)
            || !valid_label(&desc.source)
            || desc.position.iter().any(|value| !value.is_finite())
            || !desc.radius.is_finite()
            || !(0.001..=1_000_000.0).contains(&desc.radius)
            || !desc.intensity.is_finite()
            || !(0.0..=1.0e9).contains(&desc.intensity)
            || !desc.lifetime_seconds.is_finite()
            || !(0.001..=86_400.0).contains(&desc.lifetime_seconds)
            || desc.tags.iter().any(|tag| !valid_label(tag))
            || !valid_json_payload(&desc.payload)
        {
            return Err("invalid generic WorldStimulusDesc parameters".to_owned());
        }

        let id = desc.id.clone();
        self.stimuli.insert(
            id.clone(),
            ActiveStimulus {
                expires_world_seconds: self.clock.world_seconds + f64::from(desc.lifetime_seconds),
                desc,
            },
        );
        Ok(id)
    }

    pub fn clear_stimulus(&mut self, id: &str) {
        self.stimuli.remove(id.trim());
    }

    pub fn tick_frame(&mut self, dt: f32, transient_observers: &[[f32; 3]]) {
        self.frame_actor_updates.clear();
        self.frame_process_activations.clear();
        self.frame_events.clear();
        self.frame_reality_events.clear();
        self.frame_travel_completions.clear();
        self.frame_representation_changes.clear();

        for step in self.clock.advance_frame(dt) {
            self.tick_step(step, transient_observers);
        }

        self.frame_reality_events = std::mem::take(&mut self.pending_reality_events);

        self.frame_process_activations.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| {
                    a.due_world_seconds
                        .partial_cmp(&b.due_world_seconds)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| a.id.cmp(&b.id))
        });
        self.frame_events.sort_by(|a, b| {
            b.desc
                .priority
                .cmp(&a.desc.priority)
                .then_with(|| {
                    a.due_world_seconds
                        .partial_cmp(&b.due_world_seconds)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| a.desc.id.cmp(&b.desc.id))
        });
    }

    fn tick_step(&mut self, step: WorldStep, transient_observers: &[[f32; 3]]) {
        self.stimuli
            .retain(|_, active| active.expires_world_seconds > step.world_seconds);
        self.scenario_reservations
            .retain(|_, reservation| reservation.ends_world_seconds > step.world_seconds);
        self.refresh_actor_tiers(step, transient_observers);
        self.tick_processes(step);
        self.tick_scheduled_events(step);
        self.tick_actors(step);
        self.refresh_actor_tiers(step, transient_observers);
    }

    fn refresh_actor_tiers(&mut self, step: WorldStep, transient_observers: &[[f32; 3]]) {
        let persistent_observers = self.observers.values().cloned().collect::<Vec<_>>();
        let policy = self.simulation_policy.clone();

        for (id, record) in &mut self.actors {
            if !record.desc.enabled {
                continue;
            }
            let tier = actor_tier(
                record.desc.position,
                &persistent_observers,
                transient_observers,
                &policy,
            );
            let previous_tier = record.last_tier;
            if previous_tier == tier {
                continue;
            }

            // Moving toward a more detailed tier must not wait for the old
            // background cadence. Make the actor eligible immediately.
            if tier.rank() < previous_tier.rank() {
                record.next_update_seconds = record.next_update_seconds.min(step.world_seconds);
            }
            record.last_tier = tier;
            self.frame_representation_changes
                .push(WorldRepresentationChange {
                    actor_id: id.clone(),
                    previous: representation_for_tier(previous_tier),
                    current: representation_for_tier(tier),
                    world_seconds: step.world_seconds,
                    fixed_tick: step.fixed_tick,
                });
        }
    }

    fn tick_processes(&mut self, step: WorldStep) {
        for record in self.processes.values_mut() {
            if !record.desc.enabled || step.world_seconds + f64::EPSILON < record.next_due_seconds {
                continue;
            }
            let due = record.next_due_seconds;
            let elapsed = (step.world_seconds - due).max(0.0);
            let occurrences = 1 + (elapsed / record.desc.interval_seconds).floor() as u64;
            record.next_due_seconds += record.desc.interval_seconds * occurrences as f64;
            self.frame_process_activations.push(WorldProcessActivation {
                id: record.desc.id.clone(),
                priority: record.desc.priority,
                due_world_seconds: due,
                delivered_world_seconds: step.world_seconds,
                occurrences,
                tags: record.desc.tags.clone(),
                payload: record.desc.payload.clone(),
            });
        }
    }

    fn tick_scheduled_events(&mut self, step: WorldStep) {
        let due_ids = self
            .scheduled_events
            .iter()
            .filter_map(|(id, event)| {
                (event.due_world_seconds <= step.world_seconds + f64::EPSILON).then(|| id.clone())
            })
            .collect::<Vec<_>>();

        for id in due_ids {
            let Some(event) = self.scheduled_events.remove(&id) else {
                continue;
            };
            if step.world_seconds > event.expires_world_seconds {
                self.dropped_expired_events = self.dropped_expired_events.wrapping_add(1);
                continue;
            }
            let delivered_desc = event.desc.clone();
            let reality_event_id = self.push_reality_event(
                WorldRealityEventDesc {
                    id: String::new(),
                    kind: delivered_desc.kind,
                    source: delivered_desc.source,
                    cause: delivered_desc.cause.or(Some(delivered_desc.id)),
                    participants: Vec::new(),
                    position: delivered_desc.position,
                    importance: (delivered_desc.priority.max(0) as f32) + 1.0,
                    tags: delivered_desc.tags,
                    payload: delivered_desc.payload,
                },
                step.world_seconds,
            );
            self.frame_events.push(WorldEventDelivery {
                due_world_seconds: event.due_world_seconds,
                delivered_world_seconds: step.world_seconds,
                reality_event_id,
                desc: event.desc,
            });
        }
    }

    fn tick_actors(&mut self, step: WorldStep) {
        let policy = self.simulation_policy.clone();

        let mut due = self
            .actors
            .iter()
            .filter_map(|(id, record)| {
                if !record.desc.enabled
                    || record.next_update_seconds > step.world_seconds + f64::EPSILON
                {
                    return None;
                }
                Some((
                    id.clone(),
                    record.last_tier,
                    record.next_update_seconds,
                    step.world_seconds - record.next_update_seconds,
                ))
            })
            .collect::<Vec<_>>();

        due.sort_by(|a, b| {
            b.3.partial_cmp(&a.3)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.1.rank().cmp(&b.1.rank()))
                .then_with(|| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal))
                .then_with(|| a.0.cmp(&b.0))
        });
        due.truncate(policy.max_actor_updates_per_step as usize);

        for (id, tier, _, _) in due {
            let Some(record) = self.actors.get(&id) else {
                continue;
            };
            let interval = match tier {
                SimulationTier::Full => policy.full_interval_seconds,
                SimulationTier::Reduced => policy.reduced_interval_seconds,
                SimulationTier::Background => policy.background_interval_seconds,
            };
            let delta_seconds = (step.world_seconds - record.last_update_seconds).max(0.0);
            self.advance_actor_travel(&id, step);
            let Some(record) = self.actors.get_mut(&id) else {
                continue;
            };
            record.last_update_seconds = step.world_seconds;
            record.next_update_seconds = step.world_seconds + f64::from(interval);
            self.frame_actor_updates.push(ActorUpdateTicket {
                actor_id: id,
                tier,
                delta_seconds,
                world_seconds: step.world_seconds,
                fixed_tick: step.fixed_tick,
            });
        }
    }

    fn advance_actor_travel(&mut self, actor_id: &str, step: WorldStep) {
        let Some(mut travel) = self.travel.remove(actor_id) else {
            return;
        };
        let Some(actor) = self.actors.get_mut(actor_id) else {
            return;
        };

        // A new route must not consume time from before its own start.
        let delta_seconds = (step.world_seconds - travel.last_update_world_seconds).max(0.0);
        travel.last_update_world_seconds = step.world_seconds;
        let mut budget = (delta_seconds * f64::from(travel.speed)) as f32;
        let mut completed = false;

        while budget > 0.0 && travel.next_waypoint_index < travel.route.len() {
            let node_id = &travel.route[travel.next_waypoint_index];
            let Some(node) = self.nav_nodes.get(node_id) else {
                break;
            };
            let (next_position, consumed, reached) =
                advance_toward(actor.desc.position, node.position, budget);
            actor.desc.position = next_position;
            travel.distance_travelled += f64::from(consumed);
            budget = (budget - consumed).max(0.0);
            if reached {
                travel.next_waypoint_index += 1;
            } else {
                break;
            }
        }

        if travel.next_waypoint_index >= travel.route.len() {
            completed = true;
        }

        if completed {
            let completion = WorldTravelCompletion {
                actor_id: actor_id.to_owned(),
                destination_node: travel.destination_node.clone(),
                mode: travel.mode.clone(),
                payload: travel.payload.clone(),
                world_seconds: step.world_seconds,
                distance_travelled: travel.distance_travelled,
            };
            let position = actor.desc.position;
            self.frame_travel_completions.push(completion.clone());
            self.push_reality_event(
                WorldRealityEventDesc {
                    id: String::new(),
                    kind: "world.travel.completed".to_owned(),
                    source: "world.navigation".to_owned(),
                    cause: None,
                    participants: vec![actor_id.to_owned()],
                    position: Some(position),
                    importance: 1.0,
                    tags: vec!["navigation".to_owned(), "travel".to_owned()],
                    payload: json!({
                        "destination_node": completion.destination_node,
                        "mode": completion.mode,
                        "distance_travelled": completion.distance_travelled,
                        "started_world_seconds": travel.started_world_seconds,
                        "payload": completion.payload,
                    }),
                },
                step.world_seconds,
            );
        } else {
            self.travel.insert(actor_id.to_owned(), travel);
        }
    }

    fn zone_memberships(&self, point: [f32; 3]) -> Vec<String> {
        let mut zones = self
            .zones
            .values()
            .filter(|zone| point_in_aabb(point, zone.min, zone.max))
            .collect::<Vec<_>>();
        zones.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| zone_volume(a).total_cmp(&zone_volume(b)))
                .then_with(|| a.id.cmp(&b.id))
        });
        zones.into_iter().map(|zone| zone.id.clone()).collect()
    }

    pub fn runtime_state(&self) -> Value {
        let population_channels = self
            .population_channels
            .values()
            .map(|channel| {
                let active_count = self
                    .actors
                    .values()
                    .filter(|actor| {
                        actor.desc.enabled
                            && actor.desc.channel.as_deref() == Some(channel.id.as_str())
                    })
                    .count();
                json!({
                    "id": channel.id,
                    "density": channel.density,
                    "max_active": channel.max_active,
                    "active_count": active_count,
                    "spawn_radius": channel.spawn_radius,
                    "despawn_radius": channel.despawn_radius,
                    "creation_budget_per_tick": channel.creation_budget_per_tick,
                    "removal_budget_per_tick": channel.removal_budget_per_tick,
                    "update_interval_seconds": channel.update_interval_seconds,
                    "model_set": channel.model_set,
                    "tags": channel.tags,
                    "parameters": channel.parameters,
                })
            })
            .collect::<Vec<_>>();

        let zones = self
            .zones
            .values()
            .map(|zone| {
                let actor_count = self
                    .actors
                    .values()
                    .filter(|actor| {
                        actor.desc.enabled && point_in_aabb(actor.desc.position, zone.min, zone.max)
                    })
                    .count();
                json!({
                    "id": zone.id,
                    "min": zone.min,
                    "max": zone.max,
                    "priority": zone.priority,
                    "actor_count": actor_count,
                    "tags": zone.tags,
                    "parameters": zone.parameters,
                })
            })
            .collect::<Vec<_>>();

        let observers = self
            .observers
            .values()
            .map(|observer| {
                json!({
                    "id": observer.id,
                    "position": observer.position,
                    "full_radius": observer.full_radius,
                    "reduced_radius": observer.reduced_radius,
                    "importance": observer.importance,
                    "tags": observer.tags,
                    "zones": self.zone_memberships(observer.position),
                })
            })
            .collect::<Vec<_>>();

        let actors = self
            .actors
            .values()
            .map(|actor| {
                json!({
                    "id": actor.desc.id,
                    "kind": actor.desc.kind,
                    "position": actor.desc.position,
                    "group": actor.desc.group,
                    "channel": actor.desc.channel,
                    "enabled": actor.desc.enabled,
                    "tags": actor.desc.tags,
                    "parameters": actor.desc.parameters,
                    "state": actor.desc.state,
                    "simulation_tier": actor.last_tier.as_str(),
                    "representation": representation_for_tier(actor.last_tier),
                    "last_update_world_seconds": actor.last_update_seconds,
                    "next_update_world_seconds": actor.next_update_seconds,
                    "zones": self.zone_memberships(actor.desc.position),
                })
            })
            .collect::<Vec<_>>();

        let processes = self
            .processes
            .values()
            .map(|record| {
                json!({
                    "id": record.desc.id,
                    "interval_seconds": record.desc.interval_seconds,
                    "phase_seconds": record.desc.phase_seconds,
                    "enabled": record.desc.enabled,
                    "priority": record.desc.priority,
                    "next_due_world_seconds": record.next_due_seconds,
                    "tags": record.desc.tags,
                    "payload": record.desc.payload,
                })
            })
            .collect::<Vec<_>>();

        let scheduled_events = self
            .scheduled_events
            .values()
            .map(|event| {
                json!({
                    "id": event.desc.id,
                    "kind": event.desc.kind,
                    "source": event.desc.source,
                    "priority": event.desc.priority,
                    "position": event.desc.position,
                    "due_world_seconds": event.due_world_seconds,
                    "expires_world_seconds": event.expires_world_seconds,
                    "cause": event.desc.cause,
                    "tags": event.desc.tags,
                    "payload": event.desc.payload,
                })
            })
            .collect::<Vec<_>>();

        let facts = self
            .facts
            .iter()
            .map(|(key, record)| {
                json!({
                    "key": key,
                    "value": record.value,
                    "revision": record.revision,
                    "updated_world_seconds": record.updated_world_seconds,
                })
            })
            .collect::<Vec<_>>();

        let actor_updates = self
            .frame_actor_updates
            .iter()
            .map(|ticket| {
                json!({
                    "actor_id": ticket.actor_id,
                    "simulation_tier": ticket.tier.as_str(),
                    "delta_seconds": ticket.delta_seconds,
                    "world_seconds": ticket.world_seconds,
                    "fixed_tick": ticket.fixed_tick,
                })
            })
            .collect::<Vec<_>>();

        let process_activations = self
            .frame_process_activations
            .iter()
            .map(|activation| {
                json!({
                    "id": activation.id,
                    "priority": activation.priority,
                    "due_world_seconds": activation.due_world_seconds,
                    "delivered_world_seconds": activation.delivered_world_seconds,
                    "lateness_seconds": (activation.delivered_world_seconds - activation.due_world_seconds).max(0.0),
                    "occurrences": activation.occurrences,
                    "tags": activation.tags,
                    "payload": activation.payload,
                })
            })
            .collect::<Vec<_>>();

        let due_events = self
            .frame_events
            .iter()
            .map(|event| {
                json!({
                    "id": event.desc.id,
                    "kind": event.desc.kind,
                    "source": event.desc.source,
                    "priority": event.desc.priority,
                    "position": event.desc.position,
                    "due_world_seconds": event.due_world_seconds,
                    "delivered_world_seconds": event.delivered_world_seconds,
                    "lateness_seconds": (event.delivered_world_seconds - event.due_world_seconds).max(0.0),
                    "reality_event_id": event.reality_event_id,
                    "cause": event.desc.cause,
                    "tags": event.desc.tags,
                    "payload": event.desc.payload,
                })
            })
            .collect::<Vec<_>>();

        let reality_history = self
            .reality_events
            .iter()
            .map(|record| {
                json!({
                    "sequence": record.sequence,
                    "id": record.desc.id,
                    "kind": record.desc.kind,
                    "source": record.desc.source,
                    "cause": record.desc.cause,
                    "participants": record.desc.participants,
                    "position": record.desc.position,
                    "importance": record.desc.importance,
                    "tags": record.desc.tags,
                    "payload": record.desc.payload,
                    "occurred_world_seconds": record.occurred_world_seconds,
                })
            })
            .collect::<Vec<_>>();

        let frame_reality = self
            .frame_reality_events
            .iter()
            .map(|record| {
                json!({
                    "sequence": record.sequence,
                    "id": record.desc.id,
                    "kind": record.desc.kind,
                    "source": record.desc.source,
                    "cause": record.desc.cause,
                    "participants": record.desc.participants,
                    "position": record.desc.position,
                    "importance": record.desc.importance,
                    "tags": record.desc.tags,
                    "payload": record.desc.payload,
                    "occurred_world_seconds": record.occurred_world_seconds,
                })
            })
            .collect::<Vec<_>>();

        let scenario_reservations = self
            .scenario_reservations
            .values()
            .map(|reservation| {
                json!({
                    "id": reservation.desc.id,
                    "scenario_point_id": reservation.desc.scenario_point_id,
                    "actor_id": reservation.desc.actor_id,
                    "starts_world_seconds": reservation.starts_world_seconds,
                    "ends_world_seconds": reservation.ends_world_seconds,
                    "priority": reservation.desc.priority,
                    "exclusive": reservation.desc.exclusive,
                    "payload": reservation.desc.payload,
                })
            })
            .collect::<Vec<_>>();

        let representation_changes = self
            .frame_representation_changes
            .iter()
            .map(|change| {
                json!({
                    "actor_id": change.actor_id,
                    "previous": change.previous,
                    "current": change.current,
                    "world_seconds": change.world_seconds,
                    "fixed_tick": change.fixed_tick,
                })
            })
            .collect::<Vec<_>>();

        let nav_nodes = self
            .nav_nodes
            .values()
            .map(|node| {
                json!({
                    "id": node.id,
                    "position": node.position,
                    "tags": node.tags,
                    "parameters": node.parameters,
                })
            })
            .collect::<Vec<_>>();

        let nav_edges = self
            .nav_edges
            .values()
            .map(|edge| {
                json!({
                    "id": edge.id,
                    "from": edge.from,
                    "to": edge.to,
                    "bidirectional": edge.bidirectional,
                    "distance": edge.distance,
                    "cost_scale": edge.cost_scale,
                    "enabled": edge.enabled,
                    "tags": edge.tags,
                    "parameters": edge.parameters,
                })
            })
            .collect::<Vec<_>>();

        let active_travel = self
            .travel
            .values()
            .map(|travel| {
                json!({
                    "actor_id": travel.actor_id,
                    "route": travel.route,
                    "next_waypoint_index": travel.next_waypoint_index,
                    "destination_node": travel.destination_node,
                    "speed": travel.speed,
                    "mode": travel.mode,
                    "started_world_seconds": travel.started_world_seconds,
                    "distance_travelled": travel.distance_travelled,
                    "payload": travel.payload,
                })
            })
            .collect::<Vec<_>>();

        let travel_completions = self
            .frame_travel_completions
            .iter()
            .map(|completion| {
                json!({
                    "actor_id": completion.actor_id,
                    "destination_node": completion.destination_node,
                    "mode": completion.mode,
                    "world_seconds": completion.world_seconds,
                    "distance_travelled": completion.distance_travelled,
                    "payload": completion.payload,
                })
            })
            .collect::<Vec<_>>();

        let model_sets = self
            .model_sets
            .values()
            .map(|set| {
                json!({
                    "id": set.id,
                    "category": set.category,
                    "asset_count": set.assets.len(),
                    "assets": set.assets,
                    "weights": set.weights,
                    "tags": set.tags,
                })
            })
            .collect::<Vec<_>>();

        let scenario_points = self
            .scenario_points
            .values()
            .map(|point| {
                json!({
                    "id": point.id,
                    "kind": point.kind,
                    "group": point.group,
                    "position": point.position,
                    "heading_degrees": point.heading_degrees,
                    "radius": point.radius,
                    "probability": point.probability,
                    "model_set": point.model_set,
                    "enabled": point.enabled,
                    "tags": point.tags,
                    "parameters": point.parameters,
                })
            })
            .collect::<Vec<_>>();

        let relationships = self
            .relationships
            .values()
            .map(|rule| {
                json!({
                    "source_group": rule.source_group,
                    "target_group": rule.target_group,
                    "relation": rule.relation,
                    "weight": rule.weight,
                    "tags": rule.tags,
                })
            })
            .collect::<Vec<_>>();

        let stimuli = self
            .stimuli
            .values()
            .map(|active| {
                json!({
                    "id": active.desc.id,
                    "kind": active.desc.kind,
                    "source": active.desc.source,
                    "position": active.desc.position,
                    "radius": active.desc.radius,
                    "intensity": active.desc.intensity,
                    "remaining_seconds": (active.expires_world_seconds - self.clock.world_seconds).max(0.0),
                    "tags": active.desc.tags,
                    "payload": active.desc.payload,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "clock": {
                "world_seconds": self.clock.world_seconds,
                "fixed_tick": self.clock.fixed_tick,
                "fixed_hz": self.clock.policy.fixed_hz,
                "time_scale": self.clock.policy.time_scale,
                "max_steps_per_frame": self.clock.policy.max_steps_per_frame,
                "last_frame_steps": self.clock.last_frame_steps,
                "backlog_seconds": self.clock.accumulator_seconds,
            },
            "simulation": {
                "policy": {
                    "transient_full_radius": self.simulation_policy.transient_full_radius,
                    "transient_reduced_radius": self.simulation_policy.transient_reduced_radius,
                    "full_interval_seconds": self.simulation_policy.full_interval_seconds,
                    "reduced_interval_seconds": self.simulation_policy.reduced_interval_seconds,
                    "background_interval_seconds": self.simulation_policy.background_interval_seconds,
                    "max_actor_updates_per_step": self.simulation_policy.max_actor_updates_per_step,
                },
                "observers": observers,
                "actors": actors,
                "frame_actor_updates": actor_updates,
                "frame_representation_changes": representation_changes,
            },
            "processes": {
                "registered": processes,
                "frame_due": process_activations,
            },
            "events": {
                "scheduled": scheduled_events,
                "frame_due": due_events,
                "dropped_expired": self.dropped_expired_events,
            },
            "facts": facts,
            "reality": {
                "history": reality_history,
                "frame_events": frame_reality,
                "max_history": MAX_REALITY_HISTORY,
            },
            "scenario_reservations": scenario_reservations,
            "navigation": {
                "nodes": nav_nodes,
                "edges": nav_edges,
                "active_travel": active_travel,
                "frame_completions": travel_completions,
            },
            "population": {
                "channels": population_channels,
                "streaming": {
                    "max_resident_sets": self.population_streaming.max_resident_sets,
                    "request_budget_per_tick": self.population_streaming.request_budget_per_tick,
                    "eviction_budget_per_tick": self.population_streaming.eviction_budget_per_tick,
                    "fallback_set": self.population_streaming.fallback_set,
                }
            },
            "zones": {
                "items": zones,
            },
            "model_sets": model_sets,
            "scenario_points": scenario_points,
            "relationships": relationships,
            "stimuli": stimuli,
        })
    }
}

fn representation_for_tier(tier: SimulationTier) -> &'static str {
    match tier {
        SimulationTier::Full => "physical",
        SimulationTier::Reduced => "proxy",
        SimulationTier::Background => "abstract",
    }
}

fn intervals_overlap(a_start: f64, a_end: f64, b_start: f64, b_end: f64) -> bool {
    a_start < b_end && b_start < a_end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(id: &str, position: [f32; 3]) -> WorldActorDesc {
        WorldActorDesc {
            id: id.to_owned(),
            kind: "generic".to_owned(),
            position,
            group: None,
            channel: None,
            enabled: true,
            tags: Vec::new(),
            parameters: BTreeMap::new(),
            state: Value::Null,
        }
    }

    #[test]
    fn background_world_advances_without_observer() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .upsert_actor(actor("far", [10_000.0, 0.0, 0.0]))
            .unwrap();
        runtime
            .upsert_process(WorldProcessDesc {
                id: "economy".to_owned(),
                interval_seconds: 1.0,
                phase_seconds: 0.0,
                enabled: true,
                priority: 0,
                tags: Vec::new(),
                payload: json!({"system": "generic"}),
            })
            .unwrap();

        for _ in 0..50 {
            runtime.tick_frame(0.1, &[]);
        }

        let state = runtime.runtime_state();
        assert!(state["clock"]["world_seconds"].as_f64().unwrap() >= 4.9);
        assert!(
            state["simulation"]["actors"][0]["last_update_world_seconds"]
                .as_f64()
                .unwrap()
                > 0.0
        );
        assert!(
            state["processes"]["registered"][0]["next_due_world_seconds"]
                .as_f64()
                .unwrap()
                > 4.0
        );
    }

    #[test]
    fn observer_changes_fidelity_not_world_existence() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .upsert_actor(actor("near", [0.0, 0.0, 0.0]))
            .unwrap();
        runtime
            .upsert_actor(actor("far", [5000.0, 0.0, 0.0]))
            .unwrap();

        runtime.tick_frame(0.1, &[[0.0, 0.0, 0.0]]);
        let state = runtime.runtime_state();
        let actors = state["simulation"]["actors"].as_array().unwrap();
        let near = actors.iter().find(|item| item["id"] == "near").unwrap();
        let far = actors.iter().find(|item| item["id"] == "far").unwrap();
        assert_eq!(near["simulation_tier"].as_str(), Some("full"));
        assert_eq!(far["simulation_tier"].as_str(), Some("background"));
        assert!(far["enabled"].as_bool().unwrap());
    }

    #[test]
    fn fixed_clock_preserves_backlog_instead_of_dropping_world_time() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .configure_clock(WorldClockPolicyDesc {
                fixed_hz: 20.0,
                time_scale: 1.0,
                max_steps_per_frame: 2,
            })
            .unwrap();
        runtime.tick_frame(1.0, &[]);
        let state = runtime.runtime_state();
        assert_eq!(state["clock"]["last_frame_steps"].as_u64(), Some(2));
        assert!(state["clock"]["backlog_seconds"].as_f64().unwrap() > 0.8);
    }

    #[test]
    fn scheduled_events_fire_without_player_input() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .schedule_event(WorldScheduledEventDesc {
                id: "event.1".to_owned(),
                kind: "generic.change".to_owned(),
                source: "world.process".to_owned(),
                cause: None,
                delay_seconds: 0.2,
                ttl_seconds: 2.0,
                priority: 3,
                position: None,
                tags: Vec::new(),
                payload: json!({"value": 7}),
            })
            .unwrap();

        runtime.tick_frame(0.25, &[]);
        let state = runtime.runtime_state();
        assert_eq!(
            state["events"]["frame_due"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(state["events"]["frame_due"][0]["kind"], "generic.change");
    }

    #[test]
    fn world_facts_keep_revision_and_time() {
        let mut runtime = LivingWorldRuntime::default();
        runtime.set_fact("economy.index", json!(12.0)).unwrap();
        runtime.tick_frame(0.1, &[]);
        runtime.set_fact("economy.index", json!(13.0)).unwrap();
        let state = runtime.runtime_state();
        assert_eq!(state["facts"][0]["revision"].as_u64(), Some(2));
        assert_eq!(state["facts"][0]["value"], json!(13.0));
    }

    #[test]
    fn zones_are_world_memberships_not_single_player_zone() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .upsert_zone(LivingWorldZoneDesc {
                id: "outer".to_owned(),
                min: [-10.0, -10.0, -10.0],
                max: [10.0, 10.0, 10.0],
                priority: 1,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .upsert_zone(LivingWorldZoneDesc {
                id: "inner".to_owned(),
                min: [-2.0, -2.0, -2.0],
                max: [2.0, 2.0, 2.0],
                priority: 2,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .upsert_actor(actor("actor.1", [0.0, 0.0, 0.0]))
            .unwrap();

        let state = runtime.runtime_state();
        assert_eq!(
            state["simulation"]["actors"][0]["zones"],
            json!(["inner", "outer"])
        );
    }

    #[test]
    fn expired_stimuli_follow_world_clock() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .emit_stimulus(WorldStimulusDesc {
                id: "test".to_owned(),
                kind: "noise".to_owned(),
                source: "test".to_owned(),
                position: [0.0; 3],
                radius: 1.0,
                intensity: 1.0,
                lifetime_seconds: 0.25,
                tags: Vec::new(),
                payload: Value::Null,
            })
            .unwrap();
        runtime.tick_frame(0.3, &[]);
        assert_eq!(
            runtime.runtime_state()["stimuli"].as_array().map(Vec::len),
            Some(0)
        );
    }
    #[test]
    fn scheduled_event_is_persisted_as_reality_history() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .schedule_event(WorldScheduledEventDesc {
                id: "delivery.1".to_owned(),
                kind: "factory.delivery.arrival".to_owned(),
                source: "factory.17".to_owned(),
                cause: None,
                delay_seconds: 0.1,
                ttl_seconds: 5.0,
                priority: 2,
                position: Some([100.0, 20.0, 0.0]),
                tags: vec!["logistics".to_owned()],
                payload: json!({"cargo": 3}),
            })
            .unwrap();

        runtime.tick_frame(0.2, &[]);
        let state = runtime.runtime_state();
        assert_eq!(
            state["reality"]["history"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(
            state["reality"]["history"][0]["kind"],
            "factory.delivery.arrival"
        );
        assert_eq!(state["reality"]["history"][0]["cause"], "delivery.1");
    }

    #[test]
    fn exclusive_scenario_reservation_prevents_double_booking() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .upsert_scenario_point(ScenarioPointDesc {
                id: "bench.1".to_owned(),
                kind: "sit".to_owned(),
                group: None,
                position: [0.0; 3],
                heading_degrees: 0.0,
                radius: 1.0,
                probability: 1.0,
                model_set: None,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime.upsert_actor(actor("a", [0.0; 3])).unwrap();
        runtime.upsert_actor(actor("b", [0.0; 3])).unwrap();

        runtime
            .reserve_scenario(WorldScenarioReservationDesc {
                id: "r1".to_owned(),
                scenario_point_id: "bench.1".to_owned(),
                actor_id: "a".to_owned(),
                delay_seconds: 0.0,
                duration_seconds: 10.0,
                priority: 0,
                exclusive: true,
                payload: Value::Null,
            })
            .unwrap();

        let conflict = runtime.reserve_scenario(WorldScenarioReservationDesc {
            id: "r2".to_owned(),
            scenario_point_id: "bench.1".to_owned(),
            actor_id: "b".to_owned(),
            delay_seconds: 5.0,
            duration_seconds: 10.0,
            priority: 0,
            exclusive: true,
            payload: Value::Null,
        });
        assert!(conflict.is_err());
    }

    #[test]
    fn observer_transition_requests_physical_representation_without_creating_world() {
        let mut runtime = LivingWorldRuntime::default();
        runtime.upsert_actor(actor("actor.1", [0.0; 3])).unwrap();
        runtime.tick_frame(0.1, &[[0.0, 0.0, 0.0]]);
        let state = runtime.runtime_state();
        assert_eq!(
            state["simulation"]["actors"][0]["representation"],
            "physical"
        );
        assert_eq!(
            state["simulation"]["frame_representation_changes"][0]["previous"],
            "abstract"
        );
        assert_eq!(
            state["simulation"]["frame_representation_changes"][0]["current"],
            "physical"
        );
    }
    #[test]
    fn background_actor_moves_over_route_without_observer() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .configure_simulation(WorldSimulationPolicyDesc {
                reduced_interval_seconds: 0.1,
                background_interval_seconds: 0.1,
                ..WorldSimulationPolicyDesc::default()
            })
            .unwrap();
        runtime
            .upsert_actor(actor("walker", [0.0, 0.0, 0.0]))
            .unwrap();
        runtime
            .upsert_nav_node(WorldNavNodeDesc {
                id: "a".to_owned(),
                position: [0.0, 0.0, 0.0],
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .upsert_nav_node(WorldNavNodeDesc {
                id: "b".to_owned(),
                position: [10.0, 0.0, 0.0],
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .upsert_nav_edge(WorldNavEdgeDesc {
                id: "ab".to_owned(),
                from: "a".to_owned(),
                to: "b".to_owned(),
                bidirectional: true,
                distance: None,
                cost_scale: 1.0,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .start_travel(WorldTravelRequestDesc {
                actor_id: "walker".to_owned(),
                start_node: Some("a".to_owned()),
                destination_node: "b".to_owned(),
                speed: 5.0,
                mode: "generic".to_owned(),
                payload: Value::Null,
            })
            .unwrap();

        for _ in 0..30 {
            runtime.tick_frame(0.1, &[]);
        }

        let state = runtime.runtime_state();
        let walker = state["simulation"]["actors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == "walker")
            .unwrap();
        assert!((walker["position"][0].as_f64().unwrap() - 10.0).abs() < 0.01);
        assert!(state["navigation"]["active_travel"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(state["reality"]["history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "world.travel.completed"));
    }
    #[test]
    fn observer_promotion_does_not_wait_for_background_actor_cadence() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .configure_simulation(WorldSimulationPolicyDesc {
                background_interval_seconds: 30.0,
                ..WorldSimulationPolicyDesc::default()
            })
            .unwrap();
        runtime
            .upsert_actor(actor("actor.fast-promote", [1000.0, 0.0, 0.0]))
            .unwrap();

        runtime.tick_frame(0.1, &[]);
        let before = runtime.runtime_state();
        let next_background_update = before["simulation"]["actors"][0]["next_update_world_seconds"]
            .as_f64()
            .unwrap();
        assert!(next_background_update > 20.0);

        runtime.tick_frame(0.05, &[[1000.0, 0.0, 0.0]]);
        let after = runtime.runtime_state();
        let actor = &after["simulation"]["actors"][0];
        assert_eq!(actor["representation"], "physical");
        assert!(after["simulation"]["frame_actor_updates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|update| update["actor_id"] == "actor.fast-promote"));
    }
}

#[cfg(test)]
mod reality_tests;
