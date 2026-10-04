use super::*;
use newviso_agent::peds::PedProfile;

impl EngineApplication {
    pub(super) fn apply_ped_command(&mut self, command: &Value, index: usize, op: &str) -> Result<(), String> {
        match op {
            "ped.upsert" => {
                let profile: PedProfile = serde_json::from_value(command.get("profile").cloned()
                    .ok_or_else(|| format!("script command[{index}] ped.upsert requires profile"))?)
                    .map_err(|error| format!("invalid ped profile: {error}"))?;
                if !self.living_world.actor_runtime_views().iter().any(|a| a.id == profile.actor_id) {
                    return Err(format!("ped actor '{}' does not exist", profile.actor_id));
                }
                if !self.agents.is_bound(&profile.agent_id, &profile.actor_id) {
                    return Err("ped profile requires an agent bound to the same world actor".into());
                }
                let actor_id = profile.actor_id.clone();
                let reset = command.get("reset").and_then(Value::as_bool).unwrap_or(false);
                self.peds.upsert(profile, reset)?;
                if !reset {
                    if let Some(vitals) = self.living_world.fact_value(&format!("ped.{:016x}.vitals", actor_hash(&actor_id))) {
                        if vitals.get("actor_id").and_then(Value::as_str) == Some(actor_id.as_str()) {
                            if let (Some(health), Some(armour)) = (vitals.get("health").and_then(Value::as_f64), vitals.get("armour").and_then(Value::as_f64)) {
                                self.peds.restore_vitals(&actor_id, health as f32, armour as f32)?;
                            }
                        }
                    }
                }
            }
            "ped.remove" => {
                let actor_id = command.get("actor_id").and_then(Value::as_str)
                    .ok_or_else(|| format!("script command[{index}] ped.remove requires actor_id"))?;
                self.peds.remove(actor_id);
                if command.get("preserve_vitals").and_then(Value::as_bool) != Some(true) {
                    self.living_world.remove_fact(&format!("ped.{:016x}.vitals", actor_hash(actor_id)));
                }
            }
            "ped.damage" => {
                let actor_id = command.get("actor_id").and_then(Value::as_str)
                    .ok_or_else(|| format!("script command[{index}] ped.damage requires actor_id"))?;
                let source = command.get("source").and_then(Value::as_str).unwrap_or("world");
                let origin = if command.get("origin").is_some() { command_vec3(command, "origin", index)? }
                    else { self.living_world.actor_runtime_views().iter().find(|a| a.id == actor_id)
                        .map(|a| a.position).unwrap_or([0.0; 3]) };
                let amount = command_number(command, "amount", index)?;
                if let Some(cancel) = self.peds.damage(actor_id, source.to_owned(), amount, origin, &mut self.agents)? {
                    application_agents::apply_agent_commands(&mut self.living_world, vec![cancel])?;
                }
                self.save_ped_vitals(actor_id)?;
            }
            "ped.relationship.set" => {
                let from = command.get("source_group").and_then(Value::as_str)
                    .ok_or_else(|| "ped relationship requires source_group".to_owned())?;
                let to = command.get("target_group").and_then(Value::as_str)
                    .ok_or_else(|| "ped relationship requires target_group".to_owned())?;
                let relation = serde_json::from_value(command.get("relation").cloned()
                    .ok_or_else(|| "ped relationship requires relation".to_owned())?)
                    .map_err(|error| format!("invalid ped relationship: {error}"))?;
                self.peds.set_relationship(from.to_owned(), to.to_owned(), relation)?;
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

    pub(super) fn ped_attack(&mut self, actor_id: &str, target_actor: &str,
        damage: f32, range: f32) -> Result<(), String> {
        if self.peds.is_dead(actor_id) || self.peds.is_dead(target_actor) { return Ok(()); }
        let views = self.living_world.actor_runtime_views();
        let Some(actor) = views.iter().find(|a| a.id == actor_id && a.enabled) else { return Ok(()); };
        let Some(target) = views.iter().find(|a| a.id == target_actor && a.enabled) else { return Ok(()); };
        // Background combat must not issue renderer/physics work against unloaded geometry.
        if actor.simulation_tier != "full" || target.simulation_tier != "full" { return Ok(()); }
        let origin = [actor.position[0], actor.position[1] + 1.35, actor.position[2]];
        let goal = [target.position[0], target.position[1] + 1.1, target.position[2]];
        let direction: [f32; 3] = std::array::from_fn(|i| goal[i] - origin[i]);
        let source = self.world_actor_presentations.get(actor_id)
            .and_then(|binding| self.scene.runtime_entity_state(&binding.scene_key))
            .and_then(|value| value.get("id").and_then(Value::as_u64))
            .unwrap_or_else(|| actor_hash(actor_id) & ((1u64 << 53) - 1));
        let command = json!({"op": "physics.ballistic.fire", "source": source, "source_actor": actor_id,
            "origin": origin, "direction": direction, "damage": damage, "impulse": 2.0,
            "max_distance": range, "ignore_entity": source, "max_hits": 1});
        self.apply_physics_command(&command, 0, "physics.ballistic.fire")?;
        host::publish_event_json("ped.weapon.fire", "newviso.peds", json!({"actor_id": actor_id,
            "target_actor": target_actor, "origin": origin, "direction": direction}))?;
        Ok(())
    }

    /// Character controllers are not rigid bodies. Test their capsules alongside
    /// the physical scene, and let the nearest opaque surface occlude the shot.
    pub(super) fn try_ped_ballistic(&mut self, command: &Value, origin: [f32; 3],
        direction: [f32; 3], max_distance: f32, damage: f32, ignore: Option<u64>,
        falloff_min: f32, falloff_max: f32, modifier: f32) -> Result<bool, String> {
        let length = direction.iter().map(|n| n * n).sum::<f32>().sqrt();
        let direction = direction.map(|n| n / length);
        let delta = direction.map(|n| n * max_distance);
        let end: [f32; 3] = std::array::from_fn(|i| origin[i] + delta[i]);
        let min = std::array::from_fn(|i| origin[i].min(end[i]) - 0.01);
        let max = std::array::from_fn(|i| origin[i].max(end[i]) + 0.01);
        let solids = self.scene.physics_static_solid_aabbs_near(&[(min, max)]);
        let blocker = self.physics.as_ref().and_then(|physics|
            physics.character_sweep_sphere(origin, delta, 0.001, ignore, &solids))
            .map(|hit| hit.fraction * max_distance).unwrap_or(max_distance);
        let source = command.get("source_actor").and_then(Value::as_str)
            .map(str::to_owned).unwrap_or_else(|| command.get("source").map(Value::to_string).unwrap_or_else(|| "world".into()));
        self.living_world.emit_stimulus(WorldStimulusDesc { id: String::new(), kind: "gunshot".into(),
            source: source.clone(), position: origin, radius: 60.0, intensity: 1.0, lifetime_seconds: 0.4,
            tags: vec!["weapon".into()], payload: Value::Null })?;
        let views = self.living_world.actor_runtime_views();
        let mut nearest: Option<(String, f32)> = None;
        for ped in self.peds.states() {
            if ped.dead || ped.profile.actor_id == source { continue; }
            let Some(actor) = views.iter().find(|a| a.id == ped.profile.actor_id && a.enabled && a.simulation_tier == "full") else { continue; };
            if let Some(t) = ray_capsule(origin, direction, actor.position,
                ped.profile.capsule_radius, ped.profile.capsule_height) {
                if t < blocker && nearest.as_ref().is_none_or(|(_, old)| t < *old) {
                    nearest = Some((ped.profile.actor_id.clone(), t));
                }
            }
        }
        let Some((actor_id, distance)) = nearest else { return Ok(false); };
        let falloff = if falloff_max > falloff_min {
            let t = ((distance - falloff_min) / (falloff_max - falloff_min)).clamp(0.0, 1.0);
            1.0 + t * (modifier - 1.0)
        } else { 1.0 };
        if let Some(cancel) = self.peds.damage(&actor_id, source.clone(), damage * falloff, origin, &mut self.agents)? {
            application_agents::apply_agent_commands(&mut self.living_world, vec![cancel])?;
        }
        self.save_ped_vitals(&actor_id)?;
        let position: [f32; 3] = std::array::from_fn(|i| origin[i] + direction[i] * distance);
        host::publish_event_json("ped.bullet.hit", "newviso.peds", json!({"actor_id": actor_id,
            "source": source, "position": position, "damage": damage * falloff, "distance": distance}))?;
        Ok(true)
    }
}

fn actor_hash(id: &str) -> u64 {
    id.bytes().fold(0xcbf29ce484222325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3))
}

pub(super) fn ray_capsule(origin: [f32; 3], direction: [f32; 3], feet: [f32; 3], radius: f32, height: f32) -> Option<f32> {
    let x = origin[0] - feet[0]; let z = origin[2] - feet[2];
    let a = direction[0] * direction[0] + direction[2] * direction[2];
    let b = x * direction[0] + z * direction[2]; let c = x * x + z * z - radius * radius;
    let mut best: Option<f32> = None;
    let mut consider = |t: f32| { if t >= 0.0 && best.is_none_or(|old| t < old) { best = Some(t); } };
    if a > 1.0e-8 {
        let discriminant = b * b - a * c;
        if discriminant >= 0.0 {
            for t in [(-b - discriminant.sqrt()) / a, (-b + discriminant.sqrt()) / a] {
                let y = origin[1] + direction[1] * t;
                if (feet[1] + radius..=feet[1] + height - radius).contains(&y) { consider(t); }
            }
        }
    }
    for y in [feet[1] + radius, feet[1] + height - radius] {
        let offset = [x, origin[1] - y, z];
        let b = offset.iter().zip(direction).map(|(a, b)| a * b).sum::<f32>();
        let c = offset.iter().map(|n| n * n).sum::<f32>() - radius * radius;
        let discriminant = b * b - c;
        if discriminant >= 0.0 { consider(-b - discriminant.sqrt()); consider(-b + discriminant.sqrt()); }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capsule_hits_body_head_and_vertical_shots_but_misses_beside_ped() {
        for y in [0.5, 1.1, 1.75] {
            assert!(ray_capsule([0.0, y, -5.0], [0.0, 0.0, 1.0], [0.0; 3], 0.3, 1.8).is_some());
        }
        assert!(ray_capsule([0.0, 5.0, 0.0], [0.0, -1.0, 0.0], [0.0; 3], 0.3, 1.8).is_some());
        assert!(ray_capsule([0.4, 1.1, -5.0], [0.0, 0.0, 1.0], [0.0; 3], 0.3, 1.8).is_none());
    }
}
