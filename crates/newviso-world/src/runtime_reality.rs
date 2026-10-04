use super::*;

impl LivingWorldRuntime {
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

    pub(super) fn push_reality_event(
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

    pub(super) fn allocate_reality_id(&mut self) -> String {
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

    pub fn fact_value(&self, key: &str) -> Option<&Value> { self.facts.get(key).map(|fact| &fact.value) }

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
}
