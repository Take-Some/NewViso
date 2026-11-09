use super::*;
use newviso_agent::peds::{
    grip::{GripImpact, WeaponGripState},
    PedProfile,
};

mod combat;
mod grip;
mod hit_zones;
mod melee;
pub(super) mod physics;

impl EngineApplication {
    pub(super) fn apply_ped_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "ped.weapon.release" => return self.release_weapon_action(command, index),
            "ped.melee.attack" | "ped.melee.guard" | "ped.melee.cancel" => {
                return self.apply_melee_command(command, index, op)
            }
            "ped.upsert" => {
                let mut profile: PedProfile =
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
                if profile.damage_entity.is_none()
                    && !self.agents.is_bound(&profile.agent_id, &profile.actor_id)
                {
                    return Err(
                        "ped profile requires an agent bound to the same world actor".into(),
                    );
                }
                self.resolve_ped_grip_model(&mut profile)?;
                self.resolve_ped_melee_model(&mut profile)?;
                self.resolve_ped_combat_model(&mut profile)?;
                let actor_id = profile.actor_id.clone();
                if profile.damage_entity.is_none()
                    && self.physical_characters.binding(&actor_id).is_none()
                {
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
                    self.ped_physics.melee.invalidate(&actor_id);
                    self.request_ped_physics_reset(&actor_id);
                    self.living_world
                        .remove_fact(&format!("ped.{:016x}.vitals", actor_hash(&actor_id)));
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
                                if let Some(grip) = vitals.get("weapon_grip") {
                                    let grip: WeaponGripState =
                                        serde_json::from_value(grip.clone()).map_err(|e| {
                                            format!("invalid saved weapon grip: {e}")
                                        })?;
                                    self.peds.restore_weapon_grip(&actor_id, grip)?;
                                    if let Some(saved_at) =
                                        vitals.get("weapon_grip_saved_at").and_then(Value::as_f64)
                                    {
                                        self.peds.recover_weapon_grip_elapsed(
                                            &actor_id,
                                            (self.living_world.world_seconds() - saved_at).max(0.0),
                                        )?;
                                    }
                                }
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
                self.ped_physics.melee.invalidate(actor_id);
                self.remove_ped_physics_body(actor_id)?;
                self.peds.remove(actor_id);
                if command.get("preserve_vitals").and_then(Value::as_bool) != Some(true) {
                    self.living_world
                        .remove_fact(&format!("ped.{:016x}.vitals", actor_hash(actor_id)));
                }
            }
            "ped.endorphin.activate" => {
                let id = command
                    .get("actor_id")
                    .and_then(Value::as_str)
                    .ok_or("Endorphin activation requires actor_id")?;
                let impulse = if command.get("impulse").is_some() {
                    command_number(command, "impulse", index)?
                } else {
                    0.0
                };
                if impulse < 0.0 {
                    return Err("Endorphin activation impulse must be nonnegative".into());
                }
                let direction = if command.get("direction").is_some() {
                    command_vec3(command, "direction", index)?
                } else {
                    [0.0; 3]
                };
                let n = direction.iter().map(|v| v * v).sum::<f32>().sqrt();
                if impulse > 0.0 && n <= f32::EPSILON {
                    return Err("Endorphin impulse requires a direction".into());
                }
                let direction = if n > f32::EPSILON {
                    direction.map(|v| v / n)
                } else {
                    direction
                };
                let body = self
                    .ped_physics
                    .bodies
                    .get(id)
                    .ok_or("Endorphin activation requires a resident body")?;
                let point = if command.get("point").is_some() {
                    command_vec3(command, "point", index)?
                } else {
                    self.physics
                        .as_ref()
                        .and_then(|p| p.vehicle_body_state(body.entity))
                        .ok_or("Endorphin activation requires native state")?
                        .position
                };
                if !self.activate_ped_ragdoll(id, direction, point, impulse)? {
                    return Err("Endorphin activation requires an installed skeleton and joint-capable provider".into());
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
                let raw_amount = command_number(command, "amount", index)?;
                // Explicit zone is authoritative (bone/socket hit). A plain ped.damage
                // remains unchanged for backwards-compatible environmental damage.
                let zone = command
                    .get("hit_zone")
                    .and_then(Value::as_str)
                    .map(|name| {
                        hit_zones::HitZone::parse(name)
                            .ok_or_else(|| format!("invalid ped hit_zone '{name}'"))
                    })
                    .transpose()?;
                let part = if let Some(name) = command.get("hit_part").and_then(Value::as_str) {
                    Some(
                        self.ped_body_part_alias(actor_id, name)
                            .ok_or_else(|| format!("unknown anatomical part {name}"))?,
                    )
                } else if let Some(point) = command.get("hit_point") {
                    let point: [f32; 3] = serde_json::from_value(point.clone())
                        .map_err(|e| format!("invalid anatomical hit point: {e}"))?;
                    if point.iter().any(|v| !v.is_finite()) {
                        return Err("nonfinite anatomical hit point".into());
                    }
                    self.ped_body_contact(actor_id, 0, point).map(|v| v.0)
                } else {
                    command
                        .get("hit_zone")
                        .and_then(Value::as_str)
                        .and_then(|s| self.ped_body_part_alias(actor_id, s))
                };
                let multiplier = part
                    .as_ref()
                    .and_then(|s| self.ped_body_damage_multiplier(actor_id, s))
                    .or_else(|| {
                        command
                            .get("hit_zone")
                            .and_then(Value::as_str)
                            .and_then(|s| self.ped_body_damage_multiplier(actor_id, s))
                    })
                    .unwrap_or_else(|| zone.map_or(1.0, |z| z.multiplier()));
                let amount = raw_amount * multiplier;
                if !amount.is_finite() || amount > 1.0e6 {
                    return Err("scaled ped damage is out of range".into());
                }
                let before_health = self.peds.state(actor_id).map_or(0.0, |p| p.health);
                let before_vitals = self
                    .peds
                    .state(actor_id)
                    .map_or(0.0, |p| p.health + p.armour);
                let impact =
                    if command.get("suppress_reaction").and_then(Value::as_bool) == Some(true) {
                        GripImpact::Suppressed
                    } else if let Some(part) = part.as_deref() {
                        self.ped_body_grip_impact(actor_id, part)
                    } else if command
                        .get("hit_zone")
                        .and_then(Value::as_str)
                        .is_some_and(|z| z == "left_arm" || z == "left_hand")
                    {
                        GripImpact::LeftArm
                    } else if command
                        .get("hit_zone")
                        .and_then(Value::as_str)
                        .is_some_and(|z| z == "right_arm" || z == "right_hand")
                    {
                        GripImpact::RightArm
                    } else if zone == Some(hit_zones::HitZone::Arm) {
                        GripImpact::Arm
                    } else {
                        GripImpact::Body
                    };
                if let Some(cancel) = self.peds.damage_with_impact(
                    actor_id,
                    source.to_owned(),
                    amount,
                    origin,
                    impact,
                    &mut self.agents,
                )? {
                    application_agents::apply_agent_commands(&mut self.living_world, vec![cancel])?;
                }
                self.record_ped_body_damage(actor_id, part.as_deref(), before_health)?;
                self.save_ped_vitals(actor_id)?;
                if self.peds.state(actor_id).is_some_and(|p| {
                    p.profile.damage_entity.is_some() && p.health + p.armour < before_vitals
                }) && command.get("suppress_reaction").and_then(Value::as_bool) != Some(true)
                {
                    self.peds
                        .interrupt_combat(actor_id, self.living_world.world_seconds(), 0.22);
                }
                if self.peds.state(actor_id).is_some_and(|p| {
                    p.profile.damage_entity.is_none()
                        && p.health + p.armour < before_vitals
                        && command.get("suppress_reaction").and_then(Value::as_bool) != Some(true)
                }) {
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
                        self.start_ped_hit_reaction(
                            actor_id,
                            push,
                            (before_health
                                - self
                                    .peds
                                    .state(actor_id)
                                    .map_or(before_health, |p| p.health))
                            .max(0.0),
                        )?;
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
                json!({"actor_id": actor_id, "health": ped.health, "armour": ped.armour, "dead": ped.dead, "weapon_grip": ped.weapon_grip, "weapon_grip_saved_at":self.living_world.world_seconds(),
                    "body_damage":self.ped_physics.bodies.get(actor_id).map(|b|json!(b.endorphin.damage))
                        .or_else(||self.living_world.fact_value(&format!("ped.{:016x}.vitals",actor_hash(actor_id))).and_then(|v|v.get("body_damage")).cloned())}))?;
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
            || !self.ped_combat_authority(actor_id)
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
        let melee = self.peds.state(actor_id).is_some_and(|p| {
            p.combat
                .as_ref()
                .is_some_and(|c| matches!(c.style.as_str(), "fists" | "melee"))
        });
        if melee {
            let direction = std::array::from_fn(|i| target.position[i] - actor.position[i]);
            return self.begin_ped_melee(actor_id, Some(target_actor.to_owned()), direction);
        }
        let height = self
            .peds
            .state(target_actor)
            .map_or(0.0, |p| p.profile.capsule_height);
        let goal = [
            target.position[0],
            target.position[1] + height * 0.6,
            target.position[2],
        ];
        let Some((origin, barrel)) = self.ped_weapon_ray(actor_id) else {
            return Ok(());
        };
        let desired = newviso_endorphin::math::sub(goal, origin);
        let n = newviso_endorphin::math::length(desired);
        let tolerance = self
            .ped_physics
            .bodies
            .get(actor_id)
            .and_then(|b| b.endorphin.model.weapon_rig.as_ref())
            .map(|r| r.max_fire_error_degrees);
        if !self.ped_can_aim_weapon(actor_id, desired)
            || !tolerance.is_some_and(|angle| {
                n > f32::EPSILON
                    && newviso_endorphin::math::dot(barrel, desired) / n >= angle.to_radians().cos()
            })
        {
            return Ok(());
        }
        if !self
            .peds
            .combat_shot(actor_id, self.living_world.world_seconds())
        {
            return Ok(());
        }
        let direction = combat::shot_direction(barrel, self.peds.state(actor_id));
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

pub(crate) fn actor_hash(id: &str) -> u64 {
    id.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
