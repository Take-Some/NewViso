use super::*;

impl LivingWorldRuntime {
    pub fn runtime_state(&self) -> Value {
        self.runtime_state_with_history(true)
    }

    /// Bounded hot-path snapshot for per-frame script execution. Historical
    /// reality records remain authoritative in the world backend and in the
    /// full runtime/persistence snapshot, but are not recopied through the
    /// scripting ABI every rendered frame. Scripts receive frame_events for

    /// Bounded hot-path snapshot for per-frame script execution. Historical
    /// reality records remain authoritative in the world backend and in the
    /// full runtime/persistence snapshot, but are not recopied through the
    /// scripting ABI every rendered frame. Scripts receive frame_events for
    /// current causality and can use event delivery for incremental history.
    pub fn frame_state(&self) -> Value {
        let mut state = self.runtime_state_with_history(false);
        // Static navigation topology can be very large for a city and is
        // already owned authoritatively by LivingWorld. Per-frame scripts need
        // current travel/completion state, not a full copy of every authored
        // node and edge on every QuickJS call. Keep the arrays present for ABI
        // compatibility, but omit their static contents from the hot snapshot.
        if let Some(navigation) = state.get_mut("navigation").and_then(Value::as_object_mut) {
            navigation.insert("node_count".to_owned(), json!(self.nav_nodes.len()));
            navigation.insert("edge_count".to_owned(), json!(self.nav_edges.len()));
            navigation.insert("nodes".to_owned(), Value::Array(Vec::new()));
            navigation.insert("edges".to_owned(), Value::Array(Vec::new()));
        }
        state
    }

    pub(super) fn runtime_state_with_history(&self, include_history: bool) -> Value {
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

        let reality_history = if include_history {
            self.reality_events
                .iter()
                .map(reality_event_state)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let frame_reality = self
            .frame_reality_events
            .iter()
            .map(reality_event_state)
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
                    "target_position": travel.target_position,
                    "arrival_radius": travel.arrival_radius,
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
                    "tier_hysteresis_radius": self.simulation_policy.tier_hysteresis_radius,
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
                "history_available": include_history,
                "history_count": self.reality_events.len(),
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

fn reality_event_state(record: &WorldRealityEventRecord) -> Value {
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
}
