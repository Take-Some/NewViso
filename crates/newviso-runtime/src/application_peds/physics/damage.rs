use super::*;

impl EngineApplication {
    pub(crate) fn emit_ped_gunshot(
        &mut self,
        command: &Value,
        origin: [f32; 3],
    ) -> Result<(), String> {
        let entity = command.get("source").and_then(Value::as_u64).unwrap_or(0);
        let source = command
            .get("source_actor")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                self.ped_physics
                    .bodies
                    .iter()
                    .find(|(_, b)| b.entity == entity)
                    .map(|(id, _)| id.clone())
            })
            .unwrap_or_else(|| format!("physics.{entity}"));
        self.ped_physics
            .weapon_sources
            .insert(entity, source.clone());
        self.living_world.emit_stimulus(WorldStimulusDesc {
            id: String::new(),
            kind: "gunshot".into(),
            source,
            position: origin,
            radius: 60.0,
            intensity: 1.0,
            lifetime_seconds: 0.4,
            tags: vec!["weapon".into()],
            payload: Value::Null,
        })?;
        Ok(())
    }

    pub(crate) fn apply_ped_physics_damage(
        &mut self,
        contact: PhysicsDamageContact,
    ) -> Result<bool, String> {
        let external = self
            .peds
            .states()
            .find(|p| p.profile.damage_entity == Some(contact.target))
            .map(|p| p.profile.actor_id.clone());
        if let Some(id) = external {
            return self.apply_external_combat_damage(&id, contact);
        }
        let Some(id) = self
            .ped_physics
            .bodies
            .iter()
            .find(|(_, body)| {
                body.entity == contact.target
                    || body
                        .endorphin
                        .ragdoll
                        .as_ref()
                        .is_some_and(|r| r.contains(contact.target))
            })
            .map(|(id, _)| id.clone())
        else {
            return Ok(false);
        };
        if self.peds.is_dead(&id) {
            // Corpses retain query/contact participation. A later projectile
            // applies force to the actual limb it struck without changing vitals.
            if let Some(physics) = self.physics.as_mut() {
                if physics.is_ballistic_damage_contact(&contact) && contact.contact_impulse > 0.0 {
                    physics.apply_impulse_from_script(&json!({"entity":contact.target,"impulse":contact.impulse_direction.map(|v|v*contact.contact_impulse),"point":contact.point}),0)?;
                }
            }
            return Ok(true);
        }
        let vehicle = self.vehicles.contains(contact.source);
        let mut vehicle_kind = None;
        let amount = if matches!(contact.damage_kind, PhysicsDamageKind::Collision) {
            if vehicle {
                let closing = self.physics.as_ref().map_or(0.0, |p| {
                    p.contact_closing_speed(contact.source, contact.target)
                });
                let speed = closing.max(contact.contact_impulse / PED_MASS_KG);
                let physics = self.physics.as_ref().expect("vehicle contact has physics");
                let pose = physics
                    .vehicle_body_state(contact.target)
                    .expect("ped contact has body");
                let ped_speed = pose.linear_velocity[0].hypot(pose.linear_velocity[2]);
                let vehicle_speed = physics.vehicle_body_state(contact.source).map_or(0.0, |v| {
                    vehicle_approach_speed(v.linear_velocity, contact.impulse_direction)
                });
                // Original Metadata.psc: stationary AI 0.2 m/s, moving AI 0.5 m/s,
                // followed by the archetype activation-force gate. Capsule mass
                // supplies the force gate here; resting support cannot activate it.
                if !vehicle_can_knock_down(contact.contact_impulse, vehicle_speed, ped_speed) {
                    // A native contact plus incoming speed is a real bump.
                    // The final substep's impulse can already be zero after
                    // the solver has stopped a walker against a parked car.
                    if speed > 0.1 && self.ped_physics.bodies[&id].vehicle_response.is_none() {
                        self.begin_ped_vehicle_response(
                            &id,
                            contact.source,
                            VehiclePedHit::Bump,
                            contact.impulse_direction,
                        )?;
                        host::publish_event_json(
                            "ped.vehicle.hit",
                            "newviso.peds",
                            json!({"actor_id": id,
                            "response": "bump", "damage": 0.0, "source_entity_key": contact.source.to_string()}),
                        )?;
                    }
                    return Ok(true);
                }
                let local = physics
                    .body_local_point(contact.source, pose.position)
                    .unwrap_or([0.0; 3]);
                let (min, max) = physics
                    .body_local_bounds(contact.source)
                    .unwrap_or(([-1.0, -0.5, -2.0], [1.0, 0.5, 2.0]));
                let fallen = self.ped_physics.bodies[&id]
                    .vehicle_response
                    .as_ref()
                    .is_some_and(|r| r.kind != VehiclePedHit::Bump);
                vehicle_kind = Some(classify_vehicle_hit(local, min, max, fallen));
                vehicle_impact_damage(contact.contact_impulse, closing).max(30.0)
            } else {
                0.0
            }
        } else {
            contact.direct_damage
        };
        if amount <= 0.0 {
            return Ok(true);
        }
        if vehicle {
            let now = self.living_world.world_seconds();
            let key = (id.clone(), contact.source);
            if self
                .ped_physics
                .last_collision_damage
                .get(&key)
                .is_some_and(|last| now - last < VEHICLE_DAMAGE_INTERVAL)
            {
                return Ok(true);
            }
            self.ped_physics.last_collision_damage.insert(key, now);
        }
        if let Some(kind) = vehicle_kind {
            self.begin_ped_vehicle_response(&id, contact.source, kind, contact.impulse_direction)?;
        }
        let source = self
            .ped_physics
            .weapon_sources
            .get(&contact.source)
            .cloned()
            .or_else(|| {
                self.ped_physics
                    .bodies
                    .iter()
                    .find(|(_, body)| body.entity == contact.source)
                    .map(|(id, _)| id.clone())
            })
            .unwrap_or_else(|| {
                format!(
                    "{}.{}",
                    if vehicle { "vehicle" } else { "physics" },
                    contact.source
                )
            });
        let origin = self
            .physics
            .as_ref()
            .and_then(|p| p.vehicle_body_state(contact.source))
            .map_or(contact.point, |body| body.position);
        let body_hit = if !vehicle {
            self.ped_body_contact(&id, contact.target, contact.point)
        } else {
            None
        };
        let anatomical = if !vehicle {
            self.ped_grip_contact_region(&id, contact.point)
        } else {
            None
        };
        let zone = body_hit
            .as_ref()
            .and_then(|(_, region, _)| super::super::hit_zones::HitZone::parse(region))
            .or_else(|| anatomical.map(|v| v.0))
            .or_else(|| {
                if !vehicle {
                    self.living_world
                        .actor_runtime_views()
                        .into_iter()
                        .find(|actor| actor.id == id)
                        .and_then(|actor| {
                            self.peds.state(&id).map(|ped| {
                                super::super::hit_zones::zone_from_contact(
                                    contact.point,
                                    actor.position,
                                    ped.profile.capsule_height,
                                    ped.profile.capsule_radius,
                                )
                            })
                        })
                } else {
                    None
                }
            });
        let amount = amount
            * body_hit
                .as_ref()
                .map_or_else(|| zone.map_or(1.0, |region| region.multiplier()), |v| v.2);
        let before_health = self.peds.state(&id).map_or(0.0, |p| p.health);
        let before_vitals = self.peds.state(&id).map_or(0.0, |p| p.health + p.armour);
        let impact = if vehicle_kind.is_some_and(|kind| kind != VehiclePedHit::Bump) {
            newviso_agent::peds::grip::GripImpact::Knockdown
        } else if let Some((part, _, _)) = &body_hit {
            self.ped_body_grip_impact(&id, part)
        } else if let Some((_, impact)) = anatomical {
            impact
        } else if zone == Some(super::super::hit_zones::HitZone::Arm) {
            newviso_agent::peds::grip::GripImpact::Arm
        } else {
            newviso_agent::peds::grip::GripImpact::Body
        };
        if let Some(cancel) = self.peds.damage_with_impact(
            &id,
            source.clone(),
            amount,
            origin,
            impact,
            &mut self.agents,
        )? {
            application_agents::apply_agent_commands(&mut self.living_world, vec![cancel])?;
        }
        self.record_ped_body_damage(&id, body_hit.as_ref().map(|v| v.0.as_str()), before_health)?;
        self.save_ped_vitals(&id)?;
        let dead = self.peds.is_dead(&id);
        let applied = self
            .peds
            .state(&id)
            .is_some_and(|p| p.health + p.armour < before_vitals);
        let ballistic = self
            .physics
            .as_ref()
            .is_some_and(|p| p.is_ballistic_damage_contact(&contact));
        let mut ragdoll_started = false;
        if dead {
            if let Some(body) = self.ped_physics.bodies.get_mut(&id) {
                body.death_heading = self
                    .world_presentation_states
                    .get(&id)
                    .map_or(180.0, |s| s.rotation_degrees[1]);
            }
            ragdoll_started = self.activate_ped_ragdoll(
                &id,
                contact.impulse_direction,
                contact.point,
                if ballistic {
                    contact.contact_impulse
                } else {
                    0.0
                },
            )?;
            if !ragdoll_started {
                if let Some(body) = self.ped_physics.bodies.get_mut(&id) {
                    if let Some(physics) = self.physics.as_mut() {
                        physics.release_character_body(
                            body.entity,
                            body.endorphin.model.ragdoll.angular_damping,
                        )?;
                    }
                    body.was_dead = true;
                }
                self.start_ped_fall_animation(&id, contact.impulse_direction, "death");
            }
        } else if applied {
            if !vehicle {
                self.start_ped_hit_reaction(
                    &id,
                    contact.impulse_direction,
                    (before_health - self.peds.state(&id).map_or(before_health, |p| p.health))
                        .max(0.0),
                )?;
            }
        }
        if !ragdoll_started && ballistic && contact.contact_impulse > 0.0 {
            if let Some(physics) = self.physics.as_mut() {
                physics.apply_impulse_from_script(&json!({"entity":contact.target,
                    "impulse":contact.impulse_direction.map(|v|v*contact.contact_impulse),"point":contact.point}),0)?;
            }
        }
        host::publish_event_json(
            if vehicle {
                "ped.vehicle.hit"
            } else {
                "ped.bullet.hit"
            },
            "newviso.peds",
            json!({"actor_id": id, "entity": contact.target, "source": source, "source_entity": contact.source,
                "entity_key": contact.target.to_string(), "source_entity_key": contact.source.to_string(),
                "response": vehicle_kind.map(VehiclePedHit::as_str), "weapon_type": vehicle_kind.and_then(VehiclePedHit::weapon_type),
                "position": contact.point, "hit_zone": zone.map(|z| z.name()), "hit_part":body_hit.as_ref().map(|v|&v.0), "damage": amount, "impulse": contact.contact_impulse, "dead": dead}),
        )?;
        Ok(true)
    }
}
