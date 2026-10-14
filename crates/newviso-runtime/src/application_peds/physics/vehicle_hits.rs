use super::*;

impl EngineApplication {
    pub(super) fn begin_ped_vehicle_response(
        &mut self,
        id: &str,
        source: u64,
        kind: VehiclePedHit,
        direction: [f32; 3],
        launch: bool,
    ) -> Result<(), String> {
        let body = self
            .ped_physics
            .bodies
            .get_mut(id)
            .expect("vehicle hit targets ped body");
        let getting_up = body
            .reaction
            .as_ref()
            .is_some_and(|r| r.phase == animation::ReactionPhase::GetUp);
        let already_fallen = !getting_up
            && body
                .vehicle_response
                .as_ref()
                .is_some_and(|r| r.kind != VehiclePedHit::Bump);
        if !already_fallen {
            body.death_heading = self
                .world_presentation_states
                .get(id)
                .map_or(180.0, |s| s.rotation_degrees[1]);
        }
        let kind = if body
            .vehicle_response
            .as_ref()
            .is_some_and(|r| r.kind == VehiclePedHit::RunOver)
        {
            VehiclePedHit::RunOver
        } else {
            kind
        };
        body.recovery_seconds = body.recovery_seconds.max(if kind == VehiclePedHit::Bump {
            0.2
        } else {
            VEHICLE_FALL_SECONDS
        });
        let elapsed = body.vehicle_response.as_ref().map_or(0.0, |r| r.elapsed);
        body.vehicle_response = Some(VehiclePedResponse {
            source,
            kind,
            direction,
            elapsed,
            wheel_load: 0.0,
        });
        if kind != VehiclePedHit::Bump && !already_fallen {
            let physics = self.physics.as_mut().expect("native ped has physics");
            physics.release_character_body_towards(body.entity, direction)?;
            if launch {
                if let Some(vehicle) = physics.vehicle_body_state(source) {
                    let current = physics.body_linear_velocity(body.entity);
                    let mut velocity = vehicle.linear_velocity.map(|v| {
                        v * if kind == VehiclePedHit::RunOver {
                            0.9
                        } else {
                            0.65
                        }
                    });
                    velocity[1] = current[1];
                    if kind == VehiclePedHit::Rammed {
                        let speed = vehicle.linear_velocity[0].hypot(vehicle.linear_velocity[2]);
                        let local = physics
                            .body_local_point(
                                source,
                                physics
                                    .vehicle_body_state(body.entity)
                                    .expect("ped has body")
                                    .position,
                            )
                            .unwrap_or([0.0; 3]);
                        let (_, max) = physics
                            .body_local_bounds(source)
                            .unwrap_or(([0.0; 3], [1.0, 0.5, 2.0]));
                        let rise_time = max[2].abs().max(0.5) / speed.max(0.5);
                        velocity[1] = velocity[1]
                            .max(((max[1] - local[1]).max(0.1) * 1.5 / rise_time).clamp(1.0, 6.0));
                    }
                    physics.set_body_velocity_from_script(
                        &json!({"entity": body.entity, "velocity": velocity}),
                        0,
                    )?;
                }
            }
        }
        if kind != VehiclePedHit::Bump {
            self.start_ped_fall_animation(
                id,
                direction,
                if kind == VehiclePedHit::RunOver {
                    "vehicle_runover"
                } else {
                    "vehicle_ram"
                },
            );
        }
        Ok(())
    }

    pub(crate) fn apply_ped_wheel_runover(&mut self) -> Result<(), String> {
        let Some(physics) = self.physics.as_ref() else {
            return Ok(());
        };
        let (tick, fixed_dt) = physics.fixed_clock();
        let dt =
            (tick.saturating_sub(self.ped_physics.last_wheel_tick) as f32 * fixed_dt).min(0.25);
        self.ped_physics.last_wheel_tick = tick;
        if dt <= 0.0 {
            return Ok(());
        }
        for body in self.ped_physics.bodies.values_mut() {
            if let Some(response) = body.vehicle_response.as_mut() {
                response.wheel_load = 0.0;
            }
        }
        let mut loads = BTreeMap::<(String, u64, u64), (f32, [f32; 3], [f32; 3])>::new();
        for vehicle in self.vehicles.entity_ids() {
            let Some(telemetry) = self.vehicles.telemetry(vehicle) else {
                continue;
            };
            for wheel in telemetry.wheels {
                let Some(entity) = wheel
                    .surface_entity
                    .filter(|_| wheel.contact && wheel.normal_force > 1.0)
                else {
                    continue;
                };
                let Some((id, _)) = self.ped_physics.bodies.iter().find(|(_, b)| {
                    b.entity == entity || b.ragdoll.as_ref().is_some_and(|r| r.contains(entity))
                }) else {
                    continue;
                };
                let Some(point) = wheel.contact_position else {
                    continue;
                };
                let normal = wheel.contact_normal.unwrap_or([0.0, 1.0, 0.0]);
                let entry = loads
                    .entry((id.clone(), vehicle, entity))
                    .or_insert((0.0, point, normal));
                entry.0 += wheel.normal_force;
            }
        }
        for ((id, source, entity), (load, point, normal)) in loads {
            self.physics
                .as_mut()
                .expect("wheel has physics")
                .apply_impulse_from_script(
                    &json!({"entity": entity,
                "impulse": normal.map(|v| -v * load * dt), "point": point}),
                    0,
                )?;
            if self.peds.is_dead(&id) {
                continue;
            }
            self.begin_ped_vehicle_response(
                &id,
                source,
                VehiclePedHit::RunOver,
                normal.map(|v| -v),
                false,
            )?;
            self.ped_physics
                .bodies
                .get_mut(&id)
                .expect("ped exists")
                .vehicle_response
                .as_mut()
                .expect("runover exists")
                .wheel_load = load;
            let amount = wheel_crush_damage(load, dt);
            if amount <= 0.0 {
                continue;
            }
            let source_actor = format!("vehicle.{source}");
            if let Some(cancel) =
                self.peds
                    .damage(&id, source_actor.clone(), amount, point, &mut self.agents)?
            {
                application_agents::apply_agent_commands(&mut self.living_world, vec![cancel])?;
            }
            self.save_ped_vitals(&id)?;
            if self.peds.is_dead(&id) {
                self.ped_physics
                    .bodies
                    .get_mut(&id)
                    .expect("ped exists")
                    .was_dead = true;
            }
            host::publish_event_json(
                "ped.vehicle.hit",
                "newviso.peds",
                json!({"actor_id": id,
                "entity_key": entity.to_string(), "source": source_actor, "source_entity_key": source.to_string(),
                "response": "run_over", "weapon_type": "RUNOVERBYVEHICLE", "wheel_load": load,
                "damage": amount, "position": point, "dead": self.peds.is_dead(&id)}),
            )?;
        }
        Ok(())
    }
}
