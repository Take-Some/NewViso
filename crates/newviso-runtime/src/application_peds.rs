use super::*;
use newviso_agent::peds::PedProfile;

pub(super) mod physics;

impl EngineApplication {
    pub(super) fn apply_ped_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "ped.upsert" => {
                let profile: PedProfile =
                    serde_json::from_value(command.get("profile").cloned().ok_or_else(|| {
                        format!("script command[{index}] ped.upsert requires profile")
                    })?)
                    .map_err(|error| format!("invalid ped profile: {error}"))?;
                if !self
                    .living_world
                    .actor_runtime_views()
                    .iter()
                    .any(|a| a.id == profile.actor_id)
                {
                    return Err(format!("ped actor '{}' does not exist", profile.actor_id));
                }
                if !self.agents.is_bound(&profile.agent_id, &profile.actor_id) {
                    return Err(
                        "ped profile requires an agent bound to the same world actor".into(),
                    );
                }
                let actor_id = profile.actor_id.clone();
                if self.physical_characters.binding(&actor_id).is_none() {
                    self.physical_characters.bind(
                        serde_json::from_value(json!({
                            "actor_id": actor_id, "controller": {"radius": profile.capsule_radius,
                            "half_height": profile.capsule_height * 0.5 - profile.capsule_radius}
                        }))
                        .map_err(|e| format!("invalid ped character binding: {e}"))?,
                    )?;
                }
                let reset = command
                    .get("reset")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.peds.upsert(profile, reset)?;
                if reset {
                    self.request_ped_physics_reset(&actor_id);
                    self.save_ped_vitals(&actor_id)?;
                }
                if !reset {
                    if let Some(vitals) = self
                        .living_world
                        .fact_value(&format!("ped.{:016x}.vitals", actor_hash(&actor_id)))
                    {
                        if vitals.get("actor_id").and_then(Value::as_str) == Some(actor_id.as_str())
                        {
                            if let (Some(health), Some(armour)) = (
                                vitals.get("health").and_then(Value::as_f64),
                                vitals.get("armour").and_then(Value::as_f64),
                            ) {
                                self.peds.restore_vitals(
                                    &actor_id,
                                    health as f32,
                                    armour as f32,
                                )?;
                            }
                        }
                    }
                }
            }
            "ped.remove" => {
                let actor_id =
                    command
                        .get("actor_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!("script command[{index}] ped.remove requires actor_id")
                        })?;
                self.remove_ped_physics_body(actor_id)?;
                self.peds.remove(actor_id);
                if command.get("preserve_vitals").and_then(Value::as_bool) != Some(true) {
                    self.living_world
                        .remove_fact(&format!("ped.{:016x}.vitals", actor_hash(actor_id)));
                }
            }
            "ped.damage" => {
                let actor_id =
                    command
                        .get("actor_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!("script command[{index}] ped.damage requires actor_id")
                        })?;
                let source = command
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or("world");
                let origin = if command.get("origin").is_some() {
                    command_vec3(command, "origin", index)?
                } else {
                    self.living_world
                        .actor_runtime_views()
                        .iter()
                        .find(|a| a.id == actor_id)
                        .map(|a| a.position)
                        .unwrap_or([0.0; 3])
                };
                let amount = command_number(command, "amount", index)?;
                let before_vitals = self
                    .peds
                    .state(actor_id)
                    .map_or(0.0, |p| p.health + p.armour);
                if let Some(cancel) = self.peds.damage(
                    actor_id,
                    source.to_owned(),
                    amount,
                    origin,
                    &mut self.agents,
                )? {
                    application_agents::apply_agent_commands(&mut self.living_world, vec![cancel])?;
                }
                self.save_ped_vitals(actor_id)?;
                if self
                    .peds
                    .state(actor_id)
                    .is_some_and(|p| p.health + p.armour < before_vitals)
                {
                    let position = self
                        .living_world
                        .actor_runtime_views()
                        .iter()
                        .find(|a| a.id == actor_id)
                        .map_or(origin, |a| a.position);
                    let delta: [f32; 3] = std::array::from_fn(|i| position[i] - origin[i]);
                    let length = delta.iter().map(|v| v * v).sum::<f32>().sqrt();
                    let push = if length > 0.01 {
                        delta.map(|v| v / length)
                    } else {
                        [0.0, 0.0, -1.0]
                    };
                    if self.peds.is_dead(actor_id) {
                        let _ = self.activate_ped_ragdoll(actor_id, push, position, 0.0)?;
                    } else {
                        self.start_ped_hit_reaction(actor_id, push)?;
                    }
                }
            }
            "ped.relationship.set" => {
                let from = command
                    .get("source_group")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "ped relationship requires source_group".to_owned())?;
                let to = command
                    .get("target_group")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "ped relationship requires target_group".to_owned())?;
                let relation = serde_json::from_value(
                    command
                        .get("relation")
                        .cloned()
                        .ok_or_else(|| "ped relationship requires relation".to_owned())?,
                )
                .map_err(|error| format!("invalid ped relationship: {error}"))?;
                self.peds
                    .set_relationship(from.to_owned(), to.to_owned(), relation)?;
            }
            _ => return Err(format!("unsupported ped command[{index}] '{op}'")),
        }
        Ok(())
    }

    fn save_ped_vitals(&mut self, actor_id: &str) -> Result<(), String> {
        if let Some(ped) = self.peds.state(actor_id) {
            self.living_world.set_fact(&format!("ped.{:016x}.vitals", actor_hash(actor_id)),
                json!({"actor_id": actor_id, "health": ped.health, "armour": ped.armour, "dead": ped.dead}))?;
        }
        Ok(())
    }

    pub(super) fn ped_attack(
        &mut self,
        actor_id: &str,
        target_actor: &str,
        damage: f32,
        range: f32,
    ) -> Result<(), String> {
        if self.peds.is_dead(actor_id)
            || self.peds.is_dead(target_actor)
            || self.ped_vehicle_falling(actor_id)
        {
            return Ok(());
        }
        let views = self.living_world.actor_runtime_views();
        let Some(actor) = views.iter().find(|a| a.id == actor_id && a.enabled) else {
            return Ok(());
        };
        let Some(target) = views.iter().find(|a| a.id == target_actor && a.enabled) else {
            return Ok(());
        };
        // Background combat must not issue renderer/physics work against unloaded geometry.
        if actor.simulation_tier != "full" || target.simulation_tier != "full" {
            return Ok(());
        }
        let origin = [
            actor.position[0],
            actor.position[1] + 1.35,
            actor.position[2],
        ];
        let goal = [
            target.position[0],
            target.position[1] + 1.1,
            target.position[2],
        ];
        let direction: [f32; 3] = std::array::from_fn(|i| goal[i] - origin[i]);
        let source = self
            .world_actor_presentations
            .get(actor_id)
            .and_then(|binding| self.scene.runtime_entity_state(&binding.scene_key))
            .and_then(|value| value.get("id").and_then(Value::as_u64))
            .unwrap_or_else(|| actor_hash(actor_id) & ((1u64 << 53) - 1));
        let command = json!({"op": "physics.ballistic.fire", "source": source, "source_actor": actor_id,
            "origin": origin, "direction": direction, "damage": damage, "impulse": 2.0,
            "max_distance": range, "ignore_entity": source, "max_hits": 1});
        self.apply_physics_command(&command, 0, "physics.ballistic.fire")?;
        host::publish_event_json(
            "ped.weapon.fire",
            "newviso.peds",
            json!({"actor_id": actor_id,
            "target_actor": target_actor, "origin": origin, "direction": direction}),
        )?;
        Ok(())
    }
}

fn actor_hash(id: &str) -> u64 {
    id.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
