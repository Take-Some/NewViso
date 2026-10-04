use super::*;

impl EngineApplication {
    pub(super) fn sync_vehicle_lights(
        &mut self,
        entity: u64,
        telemetry: &newviso_vehicle::VehicleTelemetry,
    ) -> Result<(), String> {
        let Some(binding) = self.vehicle_presentations.get(&entity) else {
            return Ok(());
        };
        let Some((position, rotation_degrees, scale)) = self.scene.entity_transform_values(entity)
        else {
            return Ok(());
        };
        let blink_on = ((self.elapsed_seconds * 1.8).floor() as i64 & 1) == 0;
        let brake_on = telemetry.input.brake > 0.05 || telemetry.input.handbrake > 0.2;
        let reverse_on = telemetry.gear < 0;

        let interior_key = format!("vehicle.{entity}.interior");
        let mut seat_count = 0usize;
        let mut center = [0.0f32; 3];
        for seat in binding
            .parts
            .iter()
            .filter(|part| part.role == ModelFragmentPartRole::Seat && part.visible)
        {
            seat_count += 1;
            for axis in 0..3 {
                center[axis] += seat.pivot[axis];
            }
        }
        if binding.cabin.interior_light && seat_count > 0 {
            for value in &mut center {
                *value /= seat_count as f32;
            }
            center[1] += 0.65;
            self.scene.upsert_runtime_light(
                &interior_key,
                SceneLightDesc {
                    light_type: SceneLightType::Point,
                    color: [1.0, 0.86, 0.64],
                    intensity: 1.6,
                    range: 2.3,
                    casts_shadows: false,
                    ..SceneLightDesc::default()
                },
            )?;
            self.scene.set_runtime_entity_transform(
                &interior_key,
                Some(vehicle_local_point(
                    position,
                    rotation_degrees,
                    scale,
                    center,
                )),
                Some([0.0; 3]),
                Some([1.0; 3]),
            )?;
        } else if self.scene.runtime_entity_state(&interior_key).is_some() {
            self.scene.remove_runtime_entity(&interior_key)?;
        }

        for part in binding.parts.iter().filter(|part| {
            matches!(
                part.role,
                ModelFragmentPartRole::Light | ModelFragmentPartRole::Siren
            )
        }) {
            let lower = part.name_lower.as_str();
            let left = lower.contains("_l") || lower.contains("left") || lower.contains("dside");
            let right = lower.contains("_r") || lower.contains("right") || lower.contains("pside");
            let (active, color, intensity, range, light_type) = if lower.contains("headlight") {
                (
                    binding.lights.headlights || (telemetry.alarm.active() && blink_on),
                    [0.92, 0.96, 1.0],
                    if binding.cabin.high_beam { 30.0 } else { 18.0 },
                    if binding.cabin.high_beam { 80.0 } else { 46.0 },
                    SceneLightType::Spot,
                )
            } else if lower.contains("brakelight") {
                (brake_on, [1.0, 0.02, 0.01], 5.0, 8.0, SceneLightType::Point)
            } else if lower.contains("revers") {
                (
                    reverse_on,
                    [0.95, 0.98, 1.0],
                    3.0,
                    6.0,
                    SceneLightType::Point,
                )
            } else if lower.contains("indicator") {
                let side_enabled = telemetry.alarm.active()
                    || binding.lights.hazard
                    || (left && binding.lights.left_indicator)
                    || (right && binding.lights.right_indicator);
                (
                    side_enabled && blink_on,
                    [1.0, 0.28, 0.015],
                    4.0,
                    7.0,
                    SceneLightType::Point,
                )
            } else if part.role == ModelFragmentPartRole::Siren || lower.contains("siren") {
                let phase = ((self.elapsed_seconds * 4.0).floor() as i64 + part.index as i64) & 1;
                (
                    binding.lights.siren && phase == 0,
                    if part.index & 1 == 0 {
                        [1.0, 0.01, 0.01]
                    } else {
                        [0.02, 0.08, 1.0]
                    },
                    11.0,
                    18.0,
                    SceneLightType::Point,
                )
            } else {
                (false, [1.0; 3], 0.0, 1.0, SceneLightType::Point)
            };

            let key = format!("vehicle.{entity}.light.{}", part.index);
            if !active || part.damage >= 1.0 || !part.visible || part.detached_entity.is_some() {
                if self.scene.runtime_entity_state(&key).is_some() {
                    self.scene.remove_runtime_entity(&key)?;
                }
                continue;
            }
            self.scene.upsert_runtime_light(
                &key,
                SceneLightDesc {
                    light_type,
                    color,
                    intensity,
                    range,
                    cone_inner_degrees: 18.0,
                    cone_outer_degrees: 30.0,
                    casts_shadows: false,
                    ..SceneLightDesc::default()
                },
            )?;
            let local = [
                part.pivot[0] * scale[0],
                part.pivot[1] * scale[1],
                part.pivot[2] * scale[2],
            ];
            let rotated = rotate_euler_xyz(local, rotation_degrees);
            let world = [
                position[0] + rotated[0],
                position[1] + rotated[1],
                position[2] + rotated[2],
            ];
            self.scene.set_runtime_entity_transform(
                &key,
                Some(world),
                Some(rotation_degrees),
                None,
            )?;
        }
        Ok(())
    }

    pub(super) fn sync_vehicle_occupants(&mut self, vehicle_entity: u64) -> Result<(), String> {
        let Some(binding) = self.vehicle_presentations.get(&vehicle_entity) else {
            return Ok(());
        };
        if binding.occupants.is_empty() {
            return Ok(());
        }
        let Some((vehicle_position, vehicle_rotation, vehicle_scale)) =
            self.scene.entity_transform_values(vehicle_entity)
        else {
            return Ok(());
        };
        for (seat_name, occupant) in &binding.occupants {
            let Some(seat) = binding.parts.iter().find(|part| {
                part.role == ModelFragmentPartRole::Seat
                    && part.name.eq_ignore_ascii_case(seat_name)
            }) else {
                continue;
            };
            if self.scene.entity_state(occupant.entity).is_none() {
                continue;
            }
            let local = [
                seat.pivot[0] + occupant.position_offset[0],
                seat.pivot[1] + occupant.position_offset[1],
                seat.pivot[2] + occupant.position_offset[2],
            ];
            let world =
                vehicle_local_point(vehicle_position, vehicle_rotation, vehicle_scale, local);
            let rotation = std::array::from_fn(|axis| {
                vehicle_rotation[axis] + occupant.rotation_offset_degrees[axis]
            });
            let scale = self
                .scene
                .entity_transform_values(occupant.entity)
                .map(|(_, _, scale)| scale)
                .unwrap_or([1.0; 3]);
            self.scene
                .set_entity_transform(occupant.entity, world, rotation, scale)?;
        }
        Ok(())
    }

    pub(super) fn advance_vehicle_cabin(&mut self, entity: u64) {
        let engine_running = self
            .vehicles
            .telemetry(entity)
            .is_some_and(|t| t.engine_running);
        let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
            return;
        };
        let dt = (self.elapsed_seconds - binding.cabin.last_update_seconds).clamp(0.0, 0.1) as f32;
        binding.cabin.last_update_seconds = self.elapsed_seconds;
        for part in &mut binding.parts {
            if part.door_motion.is_some() {
                continue;
            }
            if let Some(&target) = binding.cabin.targets.get(&part.name_lower) {
                if part.detached_entity.is_some()
                    || !part.visible
                    || (part.rollable_window && part.damage >= 1.0)
                {
                    continue;
                }
                let speed = if part.rollable_window { 0.7 } else { 1.6 };
                part.open += (target - part.open).clamp(-dt * speed, dt * speed);
                part.presentation_override = true;
            }
        }
        if binding.occupants.is_empty() {
            binding.cabin.horn = false;
        }
        if !engine_running {
            binding.lights.headlights = false;
            binding.cabin.high_beam = false;
        }
    }

    pub(crate) fn set_vehicle_occupant_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let vehicle_entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.occupant.set requires resolved vehicle entity"
                )
            })?;
        let mut seat = command
            .get("seat")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.occupant.set requires string seat")
            })?
            .to_owned();

        if command
            .get("clear")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            if let Some(binding) = self.vehicle_presentations.get_mut(&vehicle_entity) {
                if let Some(actual) = binding
                    .occupants
                    .keys()
                    .find(|name| name.eq_ignore_ascii_case(&seat))
                    .cloned()
                {
                    if let Some(old) = binding.occupants.remove(&actual) {
                        let mut event =
                            VehicleEvent::new(0, vehicle_entity, VehicleEventKind::OccupantLeft);
                        event.actor_entity = Some(old.entity);
                        event.seat = Some(actual.clone());
                        self.vehicles.emit_event(event);
                        if binding.access_layout.driver_seat.as_deref() == Some(actual.as_str()) {
                            self.vehicles.set_player_driver(vehicle_entity, false)?;
                        }
                    }
                }
            }
            return Ok(());
        }

        let occupant = command
            .get("occupant_entity")
            .and_then(Value::as_u64)
            .or_else(|| {
                command
                    .get("occupant_id")
                    .and_then(Value::as_str)
                    .and_then(|id| self.scene.runtime_entity_stable_id(id))
            })
            .ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.occupant.set requires occupant_entity or occupant_id"
                )
            })?;
        if self.scene.entity_state(occupant).is_none() {
            return Err(format!(
                "script command[{index}] vehicle.occupant.set occupant entity {occupant} does not exist"
            ));
        }
        let binding = self
            .vehicle_presentations
            .get_mut(&vehicle_entity)
            .ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.occupant.set entity {vehicle_entity} has no fragment presentation"
                )
            })?;
        if let Some(part) = binding.parts.iter().find(|part| {
            part.role == ModelFragmentPartRole::Seat && part.name.eq_ignore_ascii_case(&seat)
        }) {
            seat = part.name.clone();
        }
        if !binding.parts.iter().any(|part| {
            part.role == ModelFragmentPartRole::Seat
                && part.name.eq_ignore_ascii_case(&seat)
                && part.visible
                && part.detached_entity.is_none()
        }) {
            return Err(format!(
                "script command[{index}] vehicle.occupant.set seat '{seat}' was not imported from the vehicle fragment"
            ));
        }
        if binding
            .occupants
            .get(&seat)
            .is_some_and(|binding| binding.entity != occupant)
        {
            return Err(format!("vehicle seat '{seat}' is already occupied"));
        }
        if binding
            .seat_reservations
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&seat))
            .is_some_and(|(_, owner)| *owner != occupant)
        {
            return Err(format!(
                "vehicle seat '{seat}' is reserved by another actor"
            ));
        }
        let previous_seat = binding
            .occupants
            .iter()
            .find(|(_, item)| item.entity == occupant)
            .map(|(name, _)| name.clone());
        // An actor may occupy one seat at a time, including after a seat shuffle.
        binding
            .occupants
            .retain(|name, binding| name == &seat || binding.entity != occupant);
        let position_offset = command
            .get("position_offset")
            .map(|value| parse_vec3(value, index, "vehicle.occupant.set position_offset"))
            .transpose()?
            .unwrap_or([0.0; 3]);
        let rotation_offset_degrees = command
            .get("rotation_offset_degrees")
            .map(|value| parse_vec3(value, index, "vehicle.occupant.set rotation_offset_degrees"))
            .transpose()?
            .unwrap_or([0.0; 3]);
        binding.occupants.insert(
            seat.clone(),
            VehicleOccupantBinding {
                entity: occupant,
                position_offset,
                rotation_offset_degrees,
            },
        );
        binding
            .seat_reservations
            .retain(|_, owner| *owner != occupant);
        binding
            .door_reservations
            .retain(|_, owner| *owner != occupant);
        let is_driver = binding.access_layout.driver_seat.as_deref() == Some(seat.as_str());
        if is_driver {
            self.vehicles.set_player_driver(
                vehicle_entity,
                command
                    .get("actor_is_player")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )?;
        } else if previous_seat.as_deref() == binding.access_layout.driver_seat.as_deref()
            && previous_seat.is_some()
        {
            self.vehicles.set_player_driver(vehicle_entity, false)?;
        }
        // Commit the target seat before releasing the previous vehicle: all
        // validation above must succeed before a transfer mutates either car.
        let mut transfers = Vec::new();
        for (&other_vehicle, other_binding) in &mut self.vehicle_presentations {
            if other_vehicle == vehicle_entity {
                continue;
            }
            let old_seats: Vec<_> = other_binding
                .occupants
                .iter()
                .filter(|(_, item)| item.entity == occupant)
                .map(|(name, _)| name.clone())
                .collect();
            for old_seat in old_seats {
                other_binding.occupants.remove(&old_seat);
                let was_driver =
                    other_binding.access_layout.driver_seat.as_deref() == Some(old_seat.as_str());
                transfers.push((other_vehicle, old_seat, was_driver));
            }
            other_binding
                .seat_reservations
                .retain(|_, owner| *owner != occupant);
            other_binding
                .door_reservations
                .retain(|_, owner| *owner != occupant);
        }
        for (other_vehicle, old_seat, was_driver) in transfers {
            if was_driver {
                self.vehicles.set_player_driver(other_vehicle, false)?;
            }
            let mut event = VehicleEvent::new(0, other_vehicle, VehicleEventKind::OccupantLeft);
            event.actor_entity = Some(occupant);
            event.seat = Some(old_seat);
            event.details = json!({"reason": "vehicle_transfer", "destination": vehicle_entity});
            self.vehicles.emit_event(event);
        }
        if previous_seat.as_deref() != Some(seat.as_str()) {
            let mut event = VehicleEvent::new(
                0,
                vehicle_entity,
                if previous_seat.is_some() {
                    VehicleEventKind::SeatChanged
                } else {
                    VehicleEventKind::OccupantEntered
                },
            );
            event.actor_entity = Some(occupant);
            event.seat = Some(seat);
            event.previous_seat = previous_seat.clone();
            self.vehicles.emit_event(event);
        }
        if previous_seat.is_none() {
            self.vehicles.trigger_alarm(vehicle_entity)?;
        }
        Ok(())
    }

    pub(crate) fn set_vehicle_cabin_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or("vehicle.cabin.set requires a resolved entity")?;
        let definition = self
            .vehicles
            .definition(entity)
            .ok_or("vehicle.cabin.set requires a registered vehicle")?;
        let engine = command
            .get("engine_running")
            .map(|v| v.as_bool().ok_or("engine_running must be boolean"))
            .transpose()?;
        let gear = command
            .get("gear")
            .map(|v| {
                if v.is_null() {
                    return Ok(None);
                }
                let n = v
                    .as_i64()
                    .ok_or("gear must be an integer or null for automatic")?;
                if !definition.class.uses_wheel_probes()
                    || n < -1
                    || n > i64::from(definition.handling.initial_drive_gears)
                {
                    return Err("gear is unsupported or outside the authored range");
                }
                Ok(Some(n as i8))
            })
            .transpose()?;
        let actor = command
            .get("actor_id")
            .and_then(Value::as_str)
            .map(|id| {
                self.scene
                    .runtime_entity_stable_id(id)
                    .ok_or("cabin actor does not exist")
            })
            .transpose()?;
        if !self.vehicle_presentations.contains_key(&entity) && actor.is_none() {
            if command.as_object().is_some_and(|o| {
                o.keys().all(|k| {
                    matches!(
                        k.as_str(),
                        "op" | "id" | "entity" | "engine_running" | "gear"
                    )
                })
            }) {
                if let Some(running) = engine {
                    self.vehicles.set_engine_running(entity, running)?;
                }
                if let Some(gear) = gear {
                    self.vehicles.set_manual_gear(entity, gear)?;
                }
                return Ok(());
            }
        }

        let mut binding = self
            .vehicle_presentations
            .get(&entity)
            .cloned()
            .ok_or("vehicle has no imported cabin presentation")?;
        if let Some(actor) = actor {
            let seat = binding
                .occupants
                .iter()
                .find(|(_, o)| o.entity == actor)
                .map(|(name, _)| name.as_str())
                .ok_or("cabin actor is not seated in this vehicle")?;
            let driving = [
                "engine_running",
                "gear",
                "horn",
                "headlights",
                "high_beam",
                "left_indicator",
                "right_indicator",
                "hazard",
                "siren",
                "siren_muted",
            ]
            .iter()
            .any(|key| command.get(*key).is_some());
            let configured_driver = binding
                .access_layout
                .driver_seat
                .as_deref()
                .unwrap_or("seat_dside_f");
            if driving
                && !seat.eq_ignore_ascii_case(configured_driver)
                && !seat.contains("driver")
                && !seat.contains("pilot")
            {
                return Err("only the configured driver seat can operate driving controls".into());
            }
        }

        let old_horn = binding.cabin.horn;
        let old_siren = binding.lights.siren;
        let old_lights = (
            binding.lights.headlights,
            binding.lights.left_indicator,
            binding.lights.right_indicator,
            binding.lights.hazard,
            binding.cabin.high_beam,
            binding.cabin.interior_light,
        );
        macro_rules! boolean {
            ($target:expr, $key:literal) => {
                if let Some(v) = command.get($key) {
                    $target = v.as_bool().ok_or(concat!($key, " must be boolean"))?;
                }
            };
        }
        boolean!(binding.cabin.horn, "horn");
        boolean!(binding.cabin.high_beam, "high_beam");
        boolean!(binding.cabin.interior_light, "interior_light");
        boolean!(binding.cabin.siren_muted, "siren_muted");
        boolean!(binding.lights.headlights, "headlights");
        boolean!(binding.lights.left_indicator, "left_indicator");
        boolean!(binding.lights.right_indicator, "right_indicator");
        boolean!(binding.lights.hazard, "hazard");
        if command.get("siren").is_some()
            && !binding
                .parts
                .iter()
                .any(|p| p.role == ModelFragmentPartRole::Siren)
        {
            return Err("this vehicle has no siren".into());
        }
        boolean!(binding.lights.siren, "siren");

        let mut semantic_events = Vec::<VehicleEvent>::new();
        if old_horn != binding.cabin.horn {
            semantic_events.push(VehicleEvent::new(
                0,
                entity,
                if binding.cabin.horn {
                    VehicleEventKind::HornStarted
                } else {
                    VehicleEventKind::HornStopped
                },
            ));
        }
        if old_siren != binding.lights.siren {
            semantic_events.push(VehicleEvent::new(
                0,
                entity,
                if binding.lights.siren {
                    VehicleEventKind::SirenStarted
                } else {
                    VehicleEventKind::SirenStopped
                },
            ));
        }

        if let Some(locked) = command.get("locks") {
            let locked = locked.as_bool().ok_or("locks must be boolean")?;
            for part in &mut binding.parts {
                if part.role == ModelFragmentPartRole::Door {
                    if part.locked != locked {
                        let mut event = VehicleEvent::new(0, entity, VehicleEventKind::LockChanged);
                        event.part = Some(part.name.clone());
                        event.details = json!({"locked": locked});
                        semantic_events.push(event);
                    }
                    part.locked = locked;
                }
            }
        }

        for key in ["windows", "doors", "driver_door", "bonnet", "boot"] {
            if command.get(key).is_none() {
                continue;
            }
            let value = optional_number(command, key, 0.0, index)?;
            if !(0.0..=1.0).contains(&value) {
                return Err(format!("{key} open ratio must lie between 0 and 1"));
            }
            let names = binding
                .parts
                .iter()
                .filter(|p| {
                    p.visible
                        && p.detached_entity.is_none()
                        && match key {
                            "windows" => rollable_window(&p.name) && p.damage < 1.0,
                            "doors" => p.role == ModelFragmentPartRole::Door,
                            "driver_door" => p.name == "door_dside_f",
                            "bonnet" => p.role == ModelFragmentPartRole::Bonnet,
                            "boot" => p.role == ModelFragmentPartRole::Boot,
                            _ => false,
                        }
                })
                .map(|p| p.name_lower.clone())
                .collect::<Vec<_>>();
            if names.is_empty() {
                return Err(format!("vehicle has no operable {key}"));
            }
            for name in names {
                let current = binding
                    .cabin
                    .targets
                    .get(&name)
                    .copied()
                    .or_else(|| {
                        binding
                            .parts
                            .iter()
                            .find(|part| part.name.eq_ignore_ascii_case(&name))
                            .map(|part| part.open)
                    })
                    .unwrap_or(0.0);
                let part_locked = binding
                    .parts
                    .iter()
                    .find(|part| part.name.eq_ignore_ascii_case(&name))
                    .is_some_and(|part| part.role == ModelFragmentPartRole::Door && part.locked);
                if part_locked && value > current + 0.05 {
                    let mut event =
                        VehicleEvent::new(0, entity, VehicleEventKind::DoorLockedAttempt);
                    event.part = Some(name.clone());
                    event.magnitude = (value - current).abs();
                    semantic_events.push(event);
                    continue;
                }
                binding.cabin.targets.insert(name, value);
            }
        }

        if let Some(running) = engine {
            self.vehicles.set_engine_running(entity, running)?;
        }
        if let Some(gear) = gear {
            self.vehicles.set_manual_gear(entity, gear)?;
        }
        if !self
            .vehicles
            .telemetry(entity)
            .is_some_and(|t| t.engine_running)
        {
            binding.lights.headlights = false;
            binding.cabin.high_beam = false;
        }
        let new_lights = (
            binding.lights.headlights,
            binding.lights.left_indicator,
            binding.lights.right_indicator,
            binding.lights.hazard,
            binding.cabin.high_beam,
            binding.cabin.interior_light,
        );
        if old_lights != new_lights {
            let mut event = VehicleEvent::new(0, entity, VehicleEventKind::LightsChanged);
            event.details = json!({"headlights": new_lights.0,
                "left_indicator": new_lights.1, "right_indicator": new_lights.2,
                "hazard": new_lights.3, "high_beam": new_lights.4,
                "interior_light": new_lights.5, "siren": binding.lights.siren});
            semantic_events.push(event);
        }
        self.vehicle_presentations.insert(entity, binding);
        for event in semantic_events {
            self.vehicles.emit_event(event);
        }
        Ok(())
    }

    pub(crate) fn set_vehicle_lights_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("script command[{index}] vehicle.lights.set requires resolved entity")
            })?;
        let engine_running = self
            .vehicles
            .telemetry(entity)
            .is_some_and(|t| t.engine_running);
        let binding = self
            .vehicle_presentations
            .get_mut(&entity)
            .ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.lights.set entity {entity} has no fragment presentation"
                )
            })?;
        let old_lights = (
            binding.lights.headlights,
            binding.lights.left_indicator,
            binding.lights.right_indicator,
            binding.lights.hazard,
            binding.lights.siren,
        );
        macro_rules! set_bool {
            ($field:ident, $key:literal) => {
                if let Some(raw) = command.get($key) {
                    binding.lights.$field = raw.as_bool().ok_or_else(|| {
                        format!(
                            "script command[{index}] vehicle.lights.set '{}' must be boolean",
                            $key
                        )
                    })?;
                }
            };
        }
        set_bool!(headlights, "headlights");
        set_bool!(left_indicator, "left_indicator");
        set_bool!(right_indicator, "right_indicator");
        set_bool!(hazard, "hazard");
        set_bool!(siren, "siren");
        if !engine_running {
            binding.lights.headlights = false;
            binding.cabin.high_beam = false;
        }
        let new_lights = (
            binding.lights.headlights,
            binding.lights.left_indicator,
            binding.lights.right_indicator,
            binding.lights.hazard,
            binding.lights.siren,
        );
        if old_lights != new_lights {
            let mut event = VehicleEvent::new(0, entity, VehicleEventKind::LightsChanged);
            event.details = json!({"headlights":new_lights.0,"left_indicator":new_lights.1,"right_indicator":new_lights.2,"hazard":new_lights.3,"siren":new_lights.4});
            self.vehicles.emit_event(event);
            if old_lights.4 != new_lights.4 {
                self.vehicles.emit_event(VehicleEvent::new(
                    0,
                    entity,
                    if new_lights.4 {
                        VehicleEventKind::SirenStarted
                    } else {
                        VehicleEventKind::SirenStopped
                    },
                ));
            }
        }
        Ok(())
    }
}
