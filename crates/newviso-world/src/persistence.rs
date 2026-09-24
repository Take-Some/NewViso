use super::*;
use serde::{Deserialize, Serialize};

const CHECKPOINT_SCHEMA: &str = "newviso.world.checkpoint.v1";
pub const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;

impl LivingWorldRuntime {
    /// A frame-boundary checkpoint. Already-consumed frame deliveries are excluded;
    /// mutations waiting for the next script snapshot are retained.
    pub fn checkpoint(&self) -> Result<Value, String> {
        validate_world(self)?;
        let value = json!({ "schema": CHECKPOINT_SCHEMA, "state": self });
        if serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() > MAX_CHECKPOINT_BYTES {
            return Err("world checkpoint exceeds size limit".into());
        }
        Ok(value)
    }

    pub fn from_checkpoint(value: Value) -> Result<Self, String> {
        if serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() > MAX_CHECKPOINT_BYTES {
            return Err("world checkpoint exceeds size limit".into());
        }
        if value.get("schema").and_then(Value::as_str) != Some(CHECKPOINT_SCHEMA) {
            return Err("unsupported world checkpoint schema".into());
        }
        let state = value
            .get("state")
            .ok_or("world checkpoint is missing state")?;
        let world: Self = serde_json::from_value(state.clone())
            .map_err(|e| format!("invalid world checkpoint: {e}"))?;
        validate_world(&world)?;
        Ok(world)
    }

    /// Validate into a separate instance before replacing the current world.
    pub fn restore_checkpoint(&mut self, value: Value) -> Result<(), String> {
        *self = Self::from_checkpoint(value)?;
        Ok(())
    }
}

fn time(value: f64) -> Result<(), String> {
    if value.is_finite() && value.abs() <= 1.0e15 {
        Ok(())
    } else {
        Err("invalid checkpoint world time".into())
    }
}

fn keyed(key: &str, id: &str) -> Result<(), String> {
    if key == id {
        Ok(())
    } else {
        Err("checkpoint map key differs from record ID".into())
    }
}

fn validate_world(w: &LivingWorldRuntime) -> Result<(), String> {
    let mut checked = LivingWorldRuntime::default();
    checked.configure_clock(w.clock.policy.clone())?;
    checked.set_world_seconds(w.clock.world_seconds)?;
    time(w.clock.accumulator_seconds)?;
    if w.clock.accumulator_seconds < 0.0 {
        return Err("negative checkpoint backlog".into());
    }
    checked.configure_simulation(w.simulation_policy.clone())?;
    checked.set_population_streaming_policy(w.population_streaming.clone())?;
    for (id, observer) in &w.observers {
        keyed(id, &observer.id)?;
        observer.validate()?;
    }
    for (id, actor) in &w.actors {
        keyed(id, &actor.desc.id)?;
        actor.desc.validate()?;
        time(actor.last_update_seconds)?;
        time(actor.next_update_seconds)?;
    }
    for (id, process) in &w.processes {
        keyed(id, &process.desc.id)?;
        process.desc.validate()?;
        time(process.next_due_seconds)?;
    }
    for (id, event) in &w.scheduled_events {
        keyed(id, &event.desc.id)?;
        event.desc.validate()?;
        time(event.due_world_seconds)?;
        time(event.expires_world_seconds)?;
        if event.expires_world_seconds <= event.due_world_seconds {
            return Err("checkpoint event expiration precedes delivery".into());
        }
    }
    for (key, fact) in &w.facts {
        validate_id("world fact", key)?;
        time(fact.updated_world_seconds)?;
        if fact.revision == 0 || !valid_json_payload(&fact.value) {
            return Err("invalid checkpoint fact".into());
        }
    }
    for (id, desc) in &w.population_channels {
        keyed(id, &desc.id)?;
        checked.upsert_population_channel(desc.clone())?;
    }
    for (id, desc) in &w.zones {
        keyed(id, &desc.id)?;
        checked.upsert_zone(desc.clone())?;
    }
    for (id, desc) in &w.model_sets {
        keyed(id, &desc.id)?;
        checked.upsert_model_set(desc.clone())?;
    }
    for (id, desc) in &w.scenario_points {
        keyed(id, &desc.id)?;
        checked.upsert_scenario_point(desc.clone())?;
    }
    for ((source, target), desc) in &w.relationships {
        keyed(source, &desc.source_group)?;
        keyed(target, &desc.target_group)?;
        checked.upsert_relationship(desc.clone())?;
    }
    for (id, stimulus) in &w.stimuli {
        keyed(id, &stimulus.desc.id)?;
        time(stimulus.expires_world_seconds)?;
        checked.emit_stimulus(stimulus.desc.clone())?;
    }
    for (id, desc) in &w.nav_nodes {
        keyed(id, &desc.id)?;
        checked.upsert_nav_node(desc.clone())?;
    }
    for (id, desc) in &w.nav_edges {
        keyed(id, &desc.id)?;
        checked.upsert_nav_edge(desc.clone())?;
    }
    for (id, travel) in &w.travel {
        keyed(id, &travel.actor_id)?;
        WorldTravelRequestDesc {
            actor_id: id.clone(),
            start_node: None,
            destination_node: travel.destination_node.clone(),
            speed: travel.speed,
            mode: travel.mode.clone(),
            payload: travel.payload.clone(),
        }
        .validate()?;
        time(travel.started_world_seconds)?;
        time(travel.last_update_world_seconds)?;
        if !w.actors.contains_key(id)
            || travel.route.is_empty()
            || travel.next_waypoint_index >= travel.route.len()
            || travel.route.last() != Some(&travel.destination_node)
            || travel
                .route
                .iter()
                .any(|node| !w.nav_nodes.contains_key(node))
            || !travel.distance_travelled.is_finite()
            || travel.distance_travelled < 0.0
        {
            return Err("invalid checkpoint actor route".into());
        }
    }
    let mut reservations: BTreeMap<&str, Vec<&WorldScenarioReservationRecord>> = BTreeMap::new();
    for (id, record) in &w.scenario_reservations {
        keyed(id, &record.desc.id)?;
        record.desc.validate()?;
        time(record.starts_world_seconds)?;
        time(record.ends_world_seconds)?;
        if !w.actors.contains_key(&record.desc.actor_id)
            || !w
                .scenario_points
                .contains_key(&record.desc.scenario_point_id)
            || record.ends_world_seconds <= record.starts_world_seconds
        {
            return Err("invalid checkpoint scenario reservation".into());
        }
        reservations
            .entry(&record.desc.scenario_point_id)
            .or_default()
            .push(record);
    }
    for records in reservations.values_mut() {
        records.sort_by(|a, b| a.starts_world_seconds.total_cmp(&b.starts_world_seconds));
        let (mut all_end, mut exclusive_end) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for record in records {
            if record.starts_world_seconds < exclusive_end
                || (record.desc.exclusive && record.starts_world_seconds < all_end)
            {
                return Err("overlapping exclusive checkpoint reservations".into());
            }
            all_end = all_end.max(record.ends_world_seconds);
            if record.desc.exclusive {
                exclusive_end = exclusive_end.max(record.ends_world_seconds);
            }
        }
    }
    if w.reality_events.len() > MAX_REALITY_HISTORY
        || w.next_stimulus_id == 0
        || w.next_event_id == 0
        || w.next_reality_event_id == 0
        || w.next_reality_sequence == 0
    {
        return Err("invalid checkpoint counters or history size".into());
    }
    for records in [&w.reality_events, &w.pending_reality_events] {
        let mut previous = 0;
        for record in records {
            record.desc.validate()?;
            time(record.occurred_world_seconds)?;
            if record.sequence <= previous || record.sequence >= w.next_reality_sequence {
                return Err("invalid checkpoint event sequence".into());
            }
            previous = record.sequence;
        }
    }
    Ok(())
}

pub(super) mod relationships {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        map: &BTreeMap<(String, String), RelationshipRuleDesc>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        map.values().collect::<Vec<_>>().serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<(String, String), RelationshipRuleDesc>, D::Error> {
        let records = Vec::<RelationshipRuleDesc>::deserialize(deserializer)?;
        let mut map = BTreeMap::new();
        for record in records {
            let key = (record.source_group.clone(), record.target_group.clone());
            if map.insert(key, record).is_some() {
                return Err(serde::de::Error::custom(
                    "duplicate checkpoint relationship",
                ));
            }
        }
        Ok(map)
    }
}
