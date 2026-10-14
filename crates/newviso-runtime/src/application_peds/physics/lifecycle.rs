use super::*;

impl EngineApplication {
    pub(crate) fn sync_ped_physics_bodies(&mut self, dt: f32) -> Result<(), String> {
        let views = self.living_world.actor_runtime_views();
        let mut eligible = BTreeMap::new();
        for ped in self.peds.states() {
            let id = &ped.profile.actor_id;
            let Some(actor) = views
                .iter()
                .find(|a| &a.id == id && a.enabled && a.simulation_tier == "full")
            else {
                continue;
            };
            let Some(character) = self.physical_characters.binding(id).filter(|c| c.enabled) else {
                continue;
            };
            let Some(entity) = self
                .world_actor_presentations
                .get(id)
                .and_then(|binding| self.scene.runtime_entity_state(&binding.scene_key))
                .and_then(|state| state.get("id").and_then(Value::as_u64))
            else {
                continue;
            };
            eligible.insert(
                id.clone(),
                (
                    entity,
                    character.clone(),
                    actor.position,
                    actor.velocity,
                    ped.dead,
                ),
            );
        }
        let retired = self
            .ped_physics
            .bodies
            .iter()
            .filter_map(|(id, body)| {
                let retain = eligible.get(id).is_some_and(|(entity, binding, _, _, _)| {
                    *entity == body.entity
                        && binding.controller.radius == body.radius
                        && binding.controller.half_height == body.half_height
                });
                (!retain).then_some(id.clone())
            })
            .collect::<Vec<_>>();
        for id in retired {
            self.remove_ped_physics_body(&id)?;
        }

        let mut native = BTreeMap::new();
        for (id, (entity, binding, position, velocity, dead)) in eligible {
            let center_offset = binding.center_offset_y();
            if !self.ped_physics.bodies.contains_key(&id) {
                let center = [position[0], position[1] + center_offset, position[2]];
                let command = capsule_body_command(
                    entity,
                    center,
                    velocity,
                    binding.controller.radius,
                    binding.controller.half_height,
                );
                let Some(physics) = self.physics.as_mut() else {
                    continue;
                };
                physics.upsert_body_from_script(&command, 0)?;
                self.scene.set_physics_process_active(entity, true)?;
                let heading = self
                    .world_presentation_states
                    .get(&id)
                    .map_or(180.0, |s| s.rotation_degrees[1]);
                self.ped_physics.bodies.insert(
                    id.clone(),
                    PedBody {
                        entity,
                        center_offset,
                        radius: binding.controller.radius,
                        half_height: binding.controller.half_height,
                        recovery_seconds: 0.0,
                        death_heading: heading,
                        was_dead: false,
                        reset_requested: false,
                        vehicle_response: None,
                        reaction: None,
                        animation_generation: 0,
                        ragdoll: None,
                        hit_reaction: None,
                    },
                );
            }
            self.advance_ped_hit_reaction(&id, dt);
            self.sync_ped_body_reaction(&id, dt, dead)?;
            let body = &self.ped_physics.bodies[&id];
            native.insert(
                id,
                NativeCharacterBody {
                    entity,
                    controlled: !dead
                        && body.recovery_seconds <= 0.0
                        && body.vehicle_response.is_none()
                        && body.reaction.is_none(),
                },
            );
        }
        self.physical_characters.set_native_bodies(native);
        Ok(())
    }

    pub(crate) fn request_ped_physics_reset(&mut self, id: &str) {
        if let Some(body) = self.ped_physics.bodies.get_mut(id) {
            body.reset_requested = true;
        }
        self.ped_physics
            .last_collision_damage
            .retain(|(actor, _), _| actor != id);
    }

    pub(crate) fn remove_ped_physics_body(&mut self, id: &str) -> Result<(), String> {
        self.clear_ped_ragdoll(id)?;
        if let Some(body) = self.ped_physics.bodies.remove(id) {
            if let Some(physics) = self.physics.as_mut() {
                physics.destroy_body_from_script(&json!({"entity": body.entity}), 0)?;
            }
            self.scene.set_physics_process_active(body.entity, false)?;
            self.ped_physics.weapon_sources.remove(&body.entity);
        }
        self.ped_physics
            .last_collision_damage
            .retain(|(actor, _), _| actor != id);
        Ok(())
    }

    pub(crate) fn is_ped_physics_entity(&self, entity: u64) -> bool {
        self.ped_physics.bodies.values().any(|body| {
            body.entity == entity || body.ragdoll.as_ref().is_some_and(|r| r.contains(entity))
        })
    }

    pub(crate) fn ped_vehicle_falling(&self, id: &str) -> bool {
        self.ped_physics.bodies.get(id).is_some_and(|b| {
            b.reaction.is_some()
                || b.vehicle_response
                    .as_ref()
                    .is_some_and(|r| r.kind != VehiclePedHit::Bump)
        })
    }
}
