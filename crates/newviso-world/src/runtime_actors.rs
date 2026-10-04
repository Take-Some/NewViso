use super::*;

impl LivingWorldRuntime {
    pub fn actor_runtime_views(&self) -> Vec<WorldActorRuntimeView> {
        self.actors
            .values()
            .map(|actor| {
                let travel = self.travel.get(&actor.desc.id);
                let travel_target_position = travel.and_then(|travel| {
                    if let Some(position) = travel.target_position { return Some(position); }
                    travel.route[travel.next_waypoint_index..]
                        .iter()
                        .filter_map(|waypoint| {
                            self.nav_nodes.get(waypoint).map(|node| node.position)
                        })
                        .find(|target| {
                            let dx = target[0] - actor.desc.position[0];
                            let dy = target[1] - actor.desc.position[1];
                            let dz = target[2] - actor.desc.position[2];
                            dx * dx + dy * dy + dz * dz > 1.0e-10
                        })
                });
                let velocity = self
                    .external_motion_velocities
                    .get(&actor.desc.id)
                    .copied()
                    .or_else(|| {
                        let travel = travel?;
                        let target = travel_target_position?;
                        let dx = target[0] - actor.desc.position[0];
                        let dy = target[1] - actor.desc.position[1];
                        let dz = target[2] - actor.desc.position[2];
                        let length = (dx * dx + dy * dy + dz * dz).sqrt();
                        (length > 1.0e-5).then_some([
                            dx / length * travel.speed,
                            dy / length * travel.speed,
                            dz / length * travel.speed,
                        ])
                    })
                    .unwrap_or([0.0; 3]);
                WorldActorRuntimeView {
                    id: actor.desc.id.clone(),
                    position: actor.desc.position,
                    velocity,
                    travel_mode: travel.map(|travel| travel.mode.clone()),
                    travel_destination: travel.map(|travel| travel.destination_node.clone()),
                    travel_target_position,
                    travel_speed: travel.map(|travel| travel.speed),
                    external_motion_authority: self
                        .external_motion_velocities
                        .contains_key(&actor.desc.id),
                    simulation_tier: actor.last_tier.as_str(),
                    representation: representation_for_tier(actor.last_tier),
                    enabled: actor.desc.enabled,
                }
            })
            .collect()
    }

    pub fn set_actor_external_motion(
        &mut self,
        actor_id: &str,
        position: [f32; 3],
        velocity: [f32; 3],
    ) -> Result<(), String> {
        if position
            .iter()
            .chain(velocity.iter())
            .any(|value| !value.is_finite())
        {
            return Err("external actor motion contains non-finite state".to_owned());
        }
        let actor_id = actor_id.trim();
        let previous_position = self
            .actors
            .get(actor_id)
            .ok_or_else(|| format!("world actor '{}' does not exist", actor_id))?
            .desc
            .position;
        let dx = position[0] - previous_position[0];
        let dy = position[1] - previous_position[1];
        let dz = position[2] - previous_position[2];
        let travelled = (dx * dx + dy * dy + dz * dz).sqrt();
        if let Some(travel) = self.travel.get_mut(actor_id) {
            travel.distance_travelled += f64::from(travelled);
        }
        self.actors
            .get_mut(actor_id)
            .expect("actor checked above")
            .desc
            .position = position;
        self.external_motion_velocities
            .insert(actor_id.to_owned(), velocity);
        Ok(())
    }

    pub fn release_actor_external_motion(&mut self, actor_id: &str) -> bool {
        self.external_motion_velocities
            .remove(actor_id.trim())
            .is_some()
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
        self.external_motion_velocities.remove(id.trim());
        self.scenario_reservations
            .retain(|_, record| record.desc.actor_id != id.trim());
    }

    pub(super) fn refresh_actor_tiers(
        &mut self,
        step: WorldStep,
        transient_observers: &[[f32; 3]],
    ) {
        let persistent_observers = &self.observers;
        let policy = &self.simulation_policy;

        for (id, record) in &mut self.actors {
            if !record.desc.enabled {
                continue;
            }
            let tier = actor_tier(
                record.desc.position,
                persistent_observers.values(),
                transient_observers,
                record.last_tier,
                policy,
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

    pub(super) fn tick_actors(&mut self, step: WorldStep) {
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
                    id.as_str(),
                    record.last_tier,
                    record.next_update_seconds,
                    step.world_seconds - record.next_update_seconds,
                ))
            })
            .collect::<Vec<_>>();

        let budget = policy.max_actor_updates_per_step as usize;
        if due.len() > budget {
            due.select_nth_unstable_by(budget, compare_actor_update_candidates);
            due.truncate(budget);
        }
        due.sort_by(compare_actor_update_candidates);

        // Only selected actors need owned IDs across the following mutations.
        let due = due
            .into_iter()
            .map(|(id, tier, _, _)| (id.to_owned(), tier))
            .collect::<Vec<_>>();
        for (id, tier) in due {
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
}
