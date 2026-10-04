use super::*;

impl EngineApplication {
    pub(crate) fn vehicle_presentation_runtime_state(&self) -> Value {
        Value::Array(
            self.vehicle_presentations
                .iter()
                .map(|(&entity, binding)| {
                    json!({
                        "entity": entity,
                        "model_id": binding.model_id,
                        "model_name": binding.model_name,
                        "wreck_fire_until_seconds": binding.wreck_fire_until_seconds,
                        "debris": self.vehicle_debris.values().filter(|d| d.source_entity == entity).map(|d| d.runtime_state()).collect::<Vec<_>>(),
                        "body_health": binding.body_health,
                        "engine_health": binding.engine_health,
                        "deformation_count": binding.dents.len(),
                        "inferred_wheel_count": binding.inferred_wheels.len(),
                        "auto_wheel_layout": self.vehicle_auto_wheel_layout.contains(&entity),
                        "cabin": {
                            "horn": binding.cabin.horn, "high_beam": binding.cabin.high_beam,
                            "interior_light": binding.cabin.interior_light, "siren_muted": binding.cabin.siren_muted,
                            "targets": binding.cabin.targets,
                            "doors_locked": binding.parts.iter().any(|p| p.role == ModelFragmentPartRole::Door && p.locked)
                        },
                        "capabilities": {
                            "doors": binding.parts.iter().filter(|p| p.role == ModelFragmentPartRole::Door && p.visible && p.detached_entity.is_none()).map(|p| p.name.clone()).collect::<Vec<_>>(),
                            "windows": binding.parts.iter().filter(|p| rollable_window(&p.name) && p.visible && p.damage < 1.0 && p.detached_entity.is_none()).map(|p| p.name.clone()).collect::<Vec<_>>(),
                            "bonnet": binding.parts.iter().any(|p| p.role == ModelFragmentPartRole::Bonnet && p.visible && p.detached_entity.is_none()),
                            "boot": binding.parts.iter().any(|p| p.role == ModelFragmentPartRole::Boot && p.visible && p.detached_entity.is_none()),
                            "siren": binding.parts.iter().any(|p| p.role == ModelFragmentPartRole::Siren),
                            "interior_light": binding.parts.iter().any(|p| p.role == ModelFragmentPartRole::Seat)
                        },
                        "lights": {
                            "headlights": binding.lights.headlights,
                            "left_indicator": binding.lights.left_indicator,
                            "right_indicator": binding.lights.right_indicator,
                            "hazard": binding.lights.hazard,
                            "siren": binding.lights.siren
                        },
                        "audio_fx": vehicle_audio_fx_runtime_state(&binding.audio_fx),
                        "dashboard": {
                            "speed_mph":binding.dashboard.display.speed_mph,
                            "speed_kmh":binding.dashboard.display.speed_mph*1.609344,
                            "revs":binding.dashboard.display.revs,"fuel":binding.dashboard.display.fuel,
                            "engine_temperature":binding.dashboard.display.engine_temperature,
                            "oil_pressure":binding.dashboard.display.oil_pressure,
                            "oil_temperature":binding.dashboard.display.oil_temperature,
                            "vacuum":binding.dashboard.display.vacuum,"boost":binding.dashboard.display.boost,
                            "gear":binding.dashboard.display.gear,"lamps":binding.dashboard.display.lamps,
                            "odometer_miles":binding.dashboard.display.odometer_miles,
                            "render":self.scene.vehicle_dashboard_state(entity)
                        },
                        "occupants": binding.occupants.iter().map(|(seat, occupant)| json!({
                            "seat": seat,
                            "id": self.scene.entity_state(occupant.entity).and_then(|v| v.get("name").cloned()),
                            "entity": occupant.entity,
                            "position_offset": occupant.position_offset,
                            "rotation_offset_degrees": occupant.rotation_offset_degrees
                        })).collect::<Vec<_>>(),
                        "access_layout": {
                            "name": binding.access_layout.name,
                            "driver_seat": binding.access_layout.driver_seat,
                            "seats": binding.access_layout.seats.iter().map(|(seat, links)| json!({
                                "seat": seat, "shuffle": links.shuffle, "rear": links.rear
                            })).collect::<Vec<_>>()
                        },
                        "seat_reservations": binding.seat_reservations.iter().map(|(seat, actor)| json!({
                            "seat": seat, "actor_entity": actor
                        })).collect::<Vec<_>>(),
                        "door_reservations": binding.door_reservations.iter().map(|(door, actor)| json!({
                            "door": door, "actor_entity": actor
                        })).collect::<Vec<_>>(),
                        "parts": binding.parts.iter().map(|part| json!({
                            "index": part.index,
                            "pivot": part.pivot,
                            "name": part.name,
                            "role": format!("{:?}", part.role),
                            "wheel_slot": part.wheel_slot.map(|slot| format!("{:?}", slot)),
                            "open": part.open,
                            "door_motion": part.door_motion.map(|motion| json!({
                                "target_ratio": motion.target_ratio,
                                "current_speed": motion.current_speed,
                                "latched": motion.latched,
                                "driven": motion.driven,
                                "swinging": motion.swinging,
                                "auto_reset": motion.auto_reset,
                                "over_limit_seconds": motion.over_limit_seconds,
                                "break_stress": motion.break_stress
                            })),
                            "locked": part.locked,
                            "visible": part.visible,
                            "damage": part.damage,
                            "glass_hit_uv": (part.role == ModelFragmentPartRole::Glass).then_some(part.glass_hit_uv),
                            "mesh_bounds": self.scene.entity_fragment_mesh_bounds(entity, &part.mesh_names)
                                .map(|(min,max)| json!({"min":min,"max":max})),
                            "loose": part.loose,
                            "detach_velocity_boost": part.detach_velocity_boost,
                            "fire_intensity": part.fire_intensity,
                            "fire_remaining_seconds": part.fire_remaining_seconds,
                            "detached_entity": part.detached_entity,
                            "light_active": if part.role==ModelFragmentPartRole::Light {
                                Some(self.scene.runtime_entity_state(&format!("vehicle.{entity}.light.{}",part.index)).is_some())
                            } else {None},
                            "presentation_override": part.presentation_override,
                            "mesh_names": part.mesh_names
                        })).collect::<Vec<_>>()
                    })
                })
                .collect(),
        )
    }

    pub(crate) fn upsert_vehicle_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<u64, String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.upsert requires resolved entity")
            })?;
        let explicit_wheel_layout =
            command.get("definition").is_some() || command.get("wheels").is_some();
        if explicit_wheel_layout {
            self.vehicle_auto_wheel_layout.remove(&entity);
        } else if !self.vehicles.contains(entity) {
            self.vehicle_auto_wheel_layout.insert(entity);
        }

        let mut definition = if let Some(raw) = command.get("definition") {
            serde_json::from_value::<VehicleDefinition>(raw.clone()).map_err(|error| {
                format!("script command[{index}] vehicle.upsert invalid definition: {error}")
            })?
        } else {
            let class = command
                .get("class")
                .cloned()
                .map(serde_json::from_value::<VehicleClass>)
                .transpose()
                .map_err(|error| {
                    format!("script command[{index}] vehicle.upsert invalid class: {error}")
                })?
                .unwrap_or_else(|| {
                    self.vehicles
                        .definition(entity)
                        .map_or(VehicleClass::Automobile, |d| d.class)
                });
            let mut definition = self
                .vehicles
                .definition(entity)
                .filter(|d| d.class == class)
                .cloned()
                .unwrap_or_else(|| match class {
                    VehicleClass::Bike => VehicleDefinition::bike(),
                    _ => VehicleDefinition::automobile(),
                });
            definition.class = class;
            if !class.uses_wheel_probes() {
                definition.wheels.clear();
            }

            if let Some(raw) = command.get("reference_handling") {
                let source =
                    serde_json::from_value::<ReferenceHandlingData>(raw.clone()).map_err(
                        |error| {
                            format!(
                                "script command[{index}] vehicle.upsert invalid reference_handling: {error}"
                            )
                        },
                    )?;
                definition.handling = HandlingData::from_reference_units(source);
            } else if let Some(raw) = command.get("handling") {
                definition.handling =
                    serde_json::from_value::<HandlingData>(raw.clone()).map_err(|error| {
                        format!("script command[{index}] vehicle.upsert invalid handling: {error}")
                    })?;
            }

            if let Some(raw) = command.get("chassis_half_extents") {
                definition.chassis_half_extents =
                    parse_vec3(raw, index, "vehicle.upsert chassis_half_extents")?;
            }
            if let Some(raw) = command.get("wheels") {
                definition.wheels = serde_json::from_value::<Vec<WheelConfig>>(raw.clone())
                    .map_err(|error| {
                        format!("script command[{index}] vehicle.upsert invalid wheels: {error}")
                    })?;
            }
            if let Some(raw) = command.get("aero") {
                definition.aero =
                    serde_json::from_value::<AeroHandling>(raw.clone()).map_err(|error| {
                        format!("script command[{index}] vehicle.upsert invalid aero: {error}")
                    })?;
            }
            if let Some(raw) = command.get("water") {
                definition.water =
                    serde_json::from_value::<WaterHandling>(raw.clone()).map_err(|error| {
                        format!("script command[{index}] vehicle.upsert invalid water: {error}")
                    })?;
            }
            definition
        };

        if let Some(raw) = command
            .get("specification")
            .or_else(|| command.pointer("/definition/specification"))
        {
            definition.specification = serde_json::from_value(raw.clone())
                .map_err(|error| format!("invalid vehicle specification: {error}"))?;
        } else if let Some(previous) = self.vehicles.definition(entity) {
            definition.specification = previous.specification.clone();
        }

        if !explicit_wheel_layout {
            if let Some(inferred) = self
                .vehicle_presentations
                .get(&entity)
                .map(|binding| binding.inferred_wheels.clone())
                .filter(|wheels| !wheels.is_empty())
            {
                definition.wheels = inferred;
            }
        }

        // A handling drive bias is authoritative. If a caller supplied the stock
        // four-wheel layout but changed RWD/FWD/AWD, update the default drive mask.
        if definition.class == VehicleClass::Automobile
            && definition.wheels.len() == 4
            && command.get("wheels").is_none()
            && command.pointer("/definition/wheels").is_none()
        {
            let front_weight = definition.handling.front_drive_weight();
            let rear_weight = 1.0 - front_weight;
            for wheel in &mut definition.wheels {
                wheel.driven = if wheel.front {
                    front_weight > 0.001
                } else {
                    rear_weight > 0.001
                };
            }
        }

        definition.validate()?;
        let physics_exists = self
            .physics
            .as_ref()
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.upsert requires engine.physics")
            })?
            .has_body(entity);

        let create_body = command
            .get("create_body")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let recreate_body = command
            .get("recreate_body")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        if create_body && (!physics_exists || recreate_body) {
            let position = command
                .get("position")
                .map(|value| parse_vec3(value, index, "vehicle.upsert position"))
                .transpose()?
                .unwrap_or([0.0, definition.chassis_half_extents[1] + 0.75, 0.0]);
            let rotation = command
                .get("rotation")
                .map(|value| parse_vec4(value, index, "vehicle.upsert rotation"))
                .transpose()?
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let volume = 8.0
                * definition.chassis_half_extents[0]
                * definition.chassis_half_extents[1]
                * definition.chassis_half_extents[2];
            let density = (definition.handling.mass / volume.max(0.01)).max(0.01);
            let body_command = json!({
                "entity": entity,
                "body_kind": "dynamic",
                "shape": {
                    "kind": "box",
                    "half_extents": definition.chassis_half_extents
                },
                "position": position,
                "rotation": rotation,
                "linear_velocity": command
                    .get("linear_velocity")
                    .cloned()
                    .unwrap_or_else(|| json!([0.0, 0.0, 0.0])),
                "angular_velocity": command
                    .get("angular_velocity")
                    .cloned()
                    .unwrap_or_else(|| json!([0.0, 0.0, 0.0])),
                "friction": 0.12,
                "restitution": 0.02,
                "density": density,
                "mass_properties": command.get("mass_properties").cloned().unwrap_or_else(|| json!({
                    "mass": definition.handling.mass,
                    "center_of_mass": definition.handling.center_of_mass_offset,
                    "inertia_diagonal": std::array::from_fn::<_, 3, _>(|axis| {
                        let a = (axis + 1) % 3;
                        let b = (axis + 2) % 3;
                        definition.handling.mass / 3.0 *
                            (definition.chassis_half_extents[a].powi(2) + definition.chassis_half_extents[b].powi(2)) *
                            definition.handling.inertia_multiplier[axis]
                    })
                })),
                "convex_hulls": command.get("convex_hulls").cloned().unwrap_or_else(|| json!([])),
                "linear_damping": 0.015,
                "angular_damping": 0.05,
                "participates_in_queries": true,
                "casts_contacts": true,
                "continuous_collision": true
            });
            self.physics
                .as_mut()
                .expect("physics presence checked above")
                .upsert_body_from_script(&body_command, index)?;
        }

        // Recreating the body is a reset: stale wheel spin/compression belongs
        // to the old pose and must not be carried into a freshly parked vehicle.
        if recreate_body {
            if let Some(binding) = self.vehicle_presentations.get_mut(&entity) {
                for (seat, occupant) in &binding.occupants {
                    let mut event = VehicleEvent::new(0, entity, VehicleEventKind::OccupantLeft);
                    event.actor_entity = Some(occupant.entity);
                    event.seat = Some(seat.clone());
                    event.details = json!({"reason":"vehicle_recreated"});
                    self.vehicles.emit_event(event);
                }
                binding.occupants.clear();
                binding.seat_reservations.clear();
                binding.door_reservations.clear();
            }
            self.vehicles.remove(entity);
        }
        self.vehicles.upsert(entity, definition)?;
        if command.get("specification").is_some()
            || command.pointer("/definition/specification").is_some()
        {
            self.vehicle_explicit_specifications.insert(entity);
        }
        if command
            .get("repair")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            self.vehicles.repair_damage(entity)?;
        }
        if let Some(binding) = self.vehicle_presentations.get_mut(&entity) {
            if command
                .get("repair")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                if let (Some(physics), Some(hulls)) = (
                    self.physics.as_mut(),
                    binding.original_collision_hulls.as_ref(),
                ) {
                    physics.restore_body_collision_hulls(entity, hulls.clone());
                }
                binding.dents.clear();
                binding.impact_history.clear();
                binding.body_health = 1000.0;
                binding.engine_health = 1000.0;
                binding.next_part_break_seconds = 0.0;
                binding.wreck_fire_until_seconds = 0.0;
                for part in &mut binding.parts {
                    part.damage = 0.0;
                    part.glass_hit_uv = [0.5; 2];
                    part.loose = false;
                    part.detach_velocity_boost = [0.0; 3];
                    part.fire_intensity = 0.0;
                    part.fire_remaining_seconds = 0.0;
                    part.visible = true;
                    if let Some(motion) = part.door_motion.as_mut() {
                        *motion = VehicleDoorMotionState::default();
                        part.open = 0.0;
                    }
                    part.presentation_override = true;
                    part.detached_entity = None;
                }
                self.vehicles.repair_tires(entity)?;
            }
            self.vehicles
                .set_engine_condition(entity, binding.engine_health / 1000.0)?;
        }
        if let Some(model) = self
            .vehicle_presentations
            .get(&entity)
            .map(|binding| binding.model_name.clone())
        {
            self.apply_vehicle_model_specification(entity, &model)?;
        }
        if self.scene.entity_state(entity).is_some() {
            self.scene.set_physics_process_active(entity, true)?;
        }
        host::info("newviso.vehicle", format!("vehicle upsert entity={entity}"));
        Ok(entity)
    }

    pub(crate) fn remove_vehicle_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.remove requires resolved entity")
            })?;
        if let Some(binding) = self.vehicle_presentations.get(&entity) {
            for (seat, occupant) in &binding.occupants {
                let mut event = VehicleEvent::new(0, entity, VehicleEventKind::OccupantLeft);
                event.actor_entity = Some(occupant.entity);
                event.seat = Some(seat.clone());
                event.details = json!({"reason": "vehicle_removed"});
                self.vehicles.emit_event(event);
            }
        }
        self.vehicles.remove(entity);
        let interior_key = format!("vehicle.{entity}.interior");
        if self.scene.runtime_entity_state(&interior_key).is_some() {
            self.scene.remove_runtime_entity(&interior_key)?;
        }
        self.vehicle_auto_wheel_layout.remove(&entity);
        self.vehicle_explicit_specifications.remove(&entity);
        if let Some(mut binding) = self.vehicle_presentations.remove(&entity) {
            binding.audio_fx.stop_all_voices(&AudioClient::new());
            for part in binding.parts {
                let key = format!("vehicle.{entity}.light.{}", part.index);
                if self.scene.runtime_entity_state(&key).is_some() {
                    self.scene.remove_runtime_entity(&key)?;
                }
            }
        }
        if command
            .get("destroy_body")
            .and_then(Value::as_bool)
            .unwrap_or(true)
        {
            if let Some(physics) = self.physics.as_mut() {
                physics.destroy_body_from_script(&json!({"entity": entity}), index)?;
            }
            if self.scene.entity_state(entity).is_some() {
                self.scene.set_physics_process_active(entity, false)?;
            }
        }
        Ok(())
    }

    pub(crate) fn set_vehicle_input_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.input.set requires resolved entity")
            })?;
        let input = VehicleInput {
            throttle: optional_number(command, "throttle", 0.0, index)?,
            brake: optional_number(command, "brake", 0.0, index)?,
            steer: optional_number(command, "steer", 0.0, index)?,
            handbrake: optional_number(command, "handbrake", 0.0, index)?,
            pitch: optional_number(command, "pitch", 0.0, index)?,
            roll: optional_number(command, "roll", 0.0, index)?,
            yaw: optional_number(command, "yaw", 0.0, index)?,
            collective: optional_number(command, "collective", 0.0, index)?,
        };
        self.vehicles.set_input(entity, input)
    }

    pub(crate) fn set_vehicle_enabled_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.enabled.set requires resolved entity")
            })?;
        let enabled = command
            .get("enabled")
            .and_then(Value::as_bool)
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.enabled.set requires boolean enabled")
            })?;
        self.vehicles.set_enabled(entity, enabled)
    }
}
