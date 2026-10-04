use super::*;

impl EngineApplication {
    pub(crate) fn set_vehicle_fuel_policy_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let enabled = command
            .get("consume")
            .or_else(|| command.get("enabled"))
            .and_then(Value::as_bool)
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.fuel_policy.set requires boolean consume")
            })?;
        self.vehicles.set_consume_petrol(enabled);
        Ok(())
    }

    pub(crate) fn set_vehicle_fuel_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.fuel.set requires resolved entity")
            })?;
        if let Some(raw) = command.get("consumption_rate") {
            let rate = raw
                .as_f64()
                .map(|value| value as f32)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] vehicle.fuel.set consumption_rate must be finite and non-negative"
                    )
                })?;
            self.vehicles.set_petrol_consumption_rate(entity, rate)?;
        }
        if let Some(raw) = command.get("level") {
            let level = raw
                .as_f64()
                .map(|value| value as f32)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] vehicle.fuel.set level must be finite and non-negative"
                    )
                })?;
            self.vehicles.set_petrol_tank_level(entity, level)?;
        }
        if let Some(raw) = command.get("tank_health") {
            let health = raw
                .as_f64()
                .map(|value| value as f32)
                .filter(|value| value.is_finite())
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] vehicle.fuel.set tank_health must be finite numeric"
                    )
                })?;
            self.vehicles.set_petrol_tank_health(entity, health)?;
        }
        if command.get("consumption_rate").is_none()
            && command.get("level").is_none()
            && command.get("tank_health").is_none()
        {
            return Err(format!(
                "script command[{index}] vehicle.fuel.set requires consumption_rate, level or tank_health"
            ));
        }
        Ok(())
    }

    pub(crate) fn set_vehicle_tire_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("script command[{index}] vehicle.tire.set requires entity"))?;
        let definition = self
            .vehicles
            .definition(entity)
            .ok_or_else(|| format!("unknown vehicle {entity}"))?;
        let wheel = if let Some(name) = command.get("wheel").and_then(Value::as_str) {
            definition
                .wheels
                .iter()
                .position(|wheel| wheel.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("unknown wheel '{name}'"))?
        } else {
            command
                .get("wheel_index")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .filter(|index| *index < definition.wheels.len())
                .ok_or_else(|| "vehicle.tire.set requires wheel or wheel_index".to_owned())?
        };
        let condition: TireCondition = serde_json::from_value(
            command
                .get("condition")
                .cloned()
                .ok_or_else(|| "vehicle.tire.set requires condition".to_owned())?,
        )
        .map_err(|error| format!("invalid tyre condition: {error}"))?;
        let name = definition.wheels[wheel].name.clone();
        if let Some(binding) = self.vehicle_presentations.get_mut(&entity) {
            if let Some(part) = binding.parts.iter_mut().find(|part| {
                part.role == ModelFragmentPartRole::Wheel
                    && (resolve_wheel_index(definition, part.wheel_slot) == Some(wheel)
                        || part.name.eq_ignore_ascii_case(&name))
            }) {
                if part.detached_entity.is_some() {
                    return Err("detached wheel requires vehicle repair".to_owned());
                }
                part.damage = match condition {
                    TireCondition::Intact => 0.0,
                    TireCondition::Punctured => 0.3,
                    TireCondition::Rim => 0.7,
                    TireCondition::Missing => 1.0,
                };
                part.presentation_override = true;
            }
        }
        self.vehicles.set_tire_condition(entity, wheel, condition)
    }

    pub(crate) fn set_vehicle_surface_policy_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        if command
            .get("clear")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            self.vehicles.clear_surface_profiles();
        }

        if let Some(default) = command.get("default") {
            let profile = parse_surface_profile(default, index, "default")?;
            self.vehicles.set_default_surface_profile(profile)?;
        }

        if let Some(raw_surfaces) = command.get("surfaces") {
            let surfaces = raw_surfaces.as_array().ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.surface_policy.set surfaces must be an array"
                )
            })?;
            for (surface_index, surface) in surfaces.iter().enumerate() {
                let surface_id = surface
                    .get("surface_id")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        format!(
                            "script command[{index}] vehicle.surface_policy.set surfaces[{surface_index}] requires u32 surface_id"
                        )
                    })?;
                if surface
                    .get("remove")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    self.vehicles.remove_surface_profile(surface_id);
                    continue;
                }
                let profile =
                    parse_surface_profile(surface, index, &format!("surfaces[{surface_index}]"))?;
                self.vehicles.set_surface_profile(surface_id, profile)?;
            }
        }
        Ok(())
    }

    pub(crate) fn set_vehicle_part_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.part.set requires resolved entity")
            })?;
        let part_name = command
            .get("part")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.part.set requires string part")
            })?;
        let elapsed = self.elapsed_seconds;
        let binding = self
            .vehicle_presentations
            .get_mut(&entity)
            .ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.part.set entity {entity} has no fragment presentation"
                )
            })?;
        if command.get("open").is_some() {
            binding
                .cabin
                .targets
                .remove(&part_name.to_ascii_lowercase());
        }
        let requested_detach = command
            .get("detach")
            .map(|raw| {
                raw.as_bool()
                    .ok_or_else(|| format!("script command[{index}] detach must be boolean"))
            })
            .transpose()?
            .unwrap_or(false);
        let definition = self.vehicles.definition(entity).cloned();
        let mut semantic_events = Vec::<VehicleEvent>::new();
        let mut matched = 0usize;
        for part in &mut binding.parts {
            if !part.name.eq_ignore_ascii_case(part_name) {
                continue;
            }
            if part.detached_entity.is_some() {
                return Err(
                    "detached vehicle part requires vehicle repair before editing".to_owned(),
                );
            }
            if requested_detach && !vehicle_part_detachable(part.role) {
                return Err(format!("part '{}' cannot detach", part.name));
            }
            matched += 1;
            let previous_damage = part.damage;
            part.presentation_override = true;
            if command.get("open").is_some() {
                let requested = optional_number(command, "open", part.open, index)?.clamp(0.0, 1.0);
                let opening = requested > part.open + 0.001;
                if part.role == ModelFragmentPartRole::Door && part.locked && opening {
                    let mut event =
                        VehicleEvent::new(0, entity, VehicleEventKind::DoorLockedAttempt);
                    event.part = Some(part.name.clone());
                    event.magnitude = (requested - part.open).abs();
                    semantic_events.push(event);
                } else {
                    let previous = part.open;
                    part.open = requested;
                    let transition = if matches!(
                        part.role,
                        ModelFragmentPartRole::Door
                            | ModelFragmentPartRole::Bonnet
                            | ModelFragmentPartRole::Boot
                    ) {
                        vehicle_door_audio_transition(previous, requested)
                    } else {
                        None
                    };
                    let mut allow_audio = true;
                    if let Some(motion) = part.door_motion.as_mut() {
                        motion.target_ratio = requested;
                        motion.current_speed = 0.0;
                        motion.driven = true;
                        motion.swinging = false;
                        motion.latched = requested <= DOOR_AUDIO_OPEN_RATIO;
                        motion.just_latched = false;
                        if transition.is_some() {
                            allow_audio = motion.last_audio_seconds < 0.0
                                || elapsed - motion.last_audio_seconds
                                    >= DOOR_AUDIO_RETRIGGER_SECONDS;
                            if allow_audio {
                                motion.last_audio_seconds = elapsed;
                            }
                        }
                    }
                    if allow_audio {
                        if let Some(kind) = transition {
                            let mut event = VehicleEvent::new(0, entity, kind);
                            event.part = Some(part.name.clone());
                            event.magnitude = (requested - previous).abs();
                            semantic_events.push(event);
                        }
                    }
                }
            }
            if let Some(swinging) = command.get("swinging") {
                let Some(motion) = part.door_motion.as_mut() else {
                    return Err("swinging applies only to door/bonnet/boot parts".to_owned());
                };
                let swinging = swinging
                    .as_bool()
                    .ok_or_else(|| "swinging must be boolean".to_owned())?;
                motion.swinging = swinging;
                motion.driven = !swinging;
                if swinging {
                    motion.latched = false;
                }
            }
            if let Some(auto_reset) = command.get("auto_reset") {
                let Some(motion) = part.door_motion.as_mut() else {
                    return Err("auto_reset applies only to door/bonnet/boot parts".to_owned());
                };
                motion.auto_reset = auto_reset
                    .as_bool()
                    .ok_or_else(|| "auto_reset must be boolean".to_owned())?;
            }
            if let Some(latched) = command.get("latched") {
                let Some(motion) = part.door_motion.as_mut() else {
                    return Err("latched applies only to door/bonnet/boot parts".to_owned());
                };
                let latched = latched
                    .as_bool()
                    .ok_or_else(|| "latched must be boolean".to_owned())?;
                motion.latched = latched;
                motion.swinging = !latched;
                if latched {
                    motion.target_ratio = 0.0;
                    motion.current_speed = 0.0;
                    part.open = 0.0;
                }
            }
            if let Some(locked) = command.get("locked") {
                if part.role != ModelFragmentPartRole::Door {
                    return Err("only doors can be locked".to_owned());
                }
                let locked = locked
                    .as_bool()
                    .ok_or_else(|| "locked must be boolean".to_owned())?;
                if part.locked != locked {
                    let mut event = VehicleEvent::new(0, entity, VehicleEventKind::LockChanged);
                    event.part = Some(part.name.clone());
                    event.details = json!({"locked": locked});
                    semantic_events.push(event);
                }
                part.locked = locked;
            }
            if let Some(visible) = command.get("visible") {
                part.visible = visible.as_bool().ok_or_else(|| {
                    format!("script command[{index}] vehicle.part.set visible must be boolean")
                })?;
            }
            if command.get("damage").is_some() {
                part.damage =
                    optional_number(command, "damage", part.damage, index)?.clamp(0.0, 1.0);
            }
            if requested_detach {
                part.damage = 1.0;
            }
            if previous_damage < 1.0 && part.damage >= 1.0 {
                let kind = match part.role {
                    ModelFragmentPartRole::Glass => Some(VehicleEventKind::GlassBroken),
                    ModelFragmentPartRole::Light => Some(VehicleEventKind::LightSmashed),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let mut event = VehicleEvent::new(0, entity, kind);
                    event.part = Some(part.name.clone());
                    event.part_index = Some(part.index);
                    event.position = self
                        .scene
                        .entity_fragment_mesh_bounds(entity, &part.mesh_names)
                        .map(|(min, max)| std::array::from_fn(|i| (min[i] + max[i]) * 0.5));
                    event.local_space = true;
                    event.magnitude = 1.0;
                    semantic_events.push(event);
                }
            }
            if part.role == ModelFragmentPartRole::Glass && command.get("damage").is_some() {
                self.vehicles
                    .set_glass_damage(entity, part.index, part.damage)?;
            }
            if let Some(definition) = definition.as_ref() {
                if part.role == ModelFragmentPartRole::Wheel {
                    if let Some(wheel) = resolve_wheel_index(definition, part.wheel_slot) {
                        self.vehicles.set_tire_condition(
                            entity,
                            wheel,
                            tire_condition_from_damage(part.damage),
                        )?;
                    }
                }
            }
        }
        if matched == 0 {
            return Err(format!(
                "script command[{index}] vehicle.part.set part '{part_name}' not found on entity {entity}"
            ));
        }
        for event in semantic_events {
            self.vehicles.emit_event(event);
        }
        Ok(())
    }
}
