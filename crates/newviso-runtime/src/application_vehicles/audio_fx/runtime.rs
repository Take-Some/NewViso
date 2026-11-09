use super::*;

impl EngineApplication {
    pub(crate) fn sync_vehicle_audio_fx(
        &mut self,
        entity: u64,
        telemetry: &newviso_vehicle::VehicleTelemetry,
    ) -> Result<(), String> {
        let Some((position, rotation_degrees, scale)) = self.scene.entity_transform_values(entity)
        else {
            return Ok(());
        };
        let elapsed = self.elapsed_seconds;
        let throttle_load = telemetry.input.throttle.abs().clamp(0.0, 1.0);
        let brake_load = telemetry
            .input
            .brake
            .max(telemetry.input.handbrake)
            .clamp(0.0, 1.0);
        let steer_load = telemetry.input.steer.abs().clamp(0.0, 1.0);
        let engine_speed = telemetry.engine_speed.clamp(0.0, 1.25);
        let clutch = telemetry.clutch.clamp(0.0, 1.0);
        let authored_top_speed = self
            .vehicles
            .definition(entity)
            .map(|definition| definition.handling.max_flat_velocity_mps.max(15.0))
            .unwrap_or(45.0);
        let speed_ratio = (telemetry.speed_mps / authored_top_speed).clamp(0.0, 1.25);
        let forward_speed_ratio =
            (telemetry.speed_forward_mps.abs() / authored_top_speed).clamp(0.0, 1.25);
        let contact_wheels = telemetry
            .wheels
            .iter()
            .filter(|wheel| wheel.contact)
            .count();
        let contact_factor = if telemetry.wheels.is_empty() {
            0.0
        } else {
            contact_wheels as f32 / telemetry.wheels.len() as f32
        };
        let rubber_contact = |wheel: &&newviso_vehicle::WheelTelemetry| {
            wheel.contact
                && !matches!(
                    wheel.tire_condition,
                    TireCondition::Rim | TireCondition::Missing
                )
        };
        let rubber_contact_wheels = telemetry.wheels.iter().filter(rubber_contact).count();
        let rubber_contact_factor = if telemetry.wheels.is_empty() {
            0.0
        } else {
            rubber_contact_wheels as f32 / telemetry.wheels.len() as f32
        };
        let max_long_slip = telemetry
            .wheels
            .iter()
            .filter(rubber_contact)
            .map(|wheel| wheel.longitudinal_slip.abs() / 0.12)
            .fold(0.0f32, f32::max);
        let max_lat_slip = telemetry
            .wheels
            .iter()
            .filter(rubber_contact)
            .map(|wheel| wheel.lateral_slip_angle.abs())
            .fold(0.0f32, f32::max);
        let max_slip = telemetry
            .wheels
            .iter()
            .filter(rubber_contact)
            .map(|wheel| wheel.slip_intensity)
            .fold(0.0f32, f32::max);
        let rim_contact_count = telemetry
            .wheels
            .iter()
            .filter(|wheel| wheel.contact && wheel.tire_condition == TireCondition::Rim)
            .count();
        let max_rim_slip = telemetry
            .wheels
            .iter()
            .filter(|wheel| wheel.contact && wheel.tire_condition == TireCondition::Rim)
            .map(|wheel| wheel.slip_intensity)
            .fold(0.0f32, f32::max);
        let max_scrub_speed = telemetry
            .wheels
            .iter()
            .filter(rubber_contact)
            .map(|wheel| wheel.sliding_speed_mps)
            .fold(0.0f32, f32::max);
        let tyre_active = rubber_contact_wheels > 0 && max_scrub_speed > 1.0 && max_slip > 0.82;
        let rubber_road_active = telemetry.speed_mps > 1.0 && rubber_contact_wheels > 0;
        // Controlled cornering gets its own rubber side scrub. Bare rims are a
        // different material/contact path and never feed tyre scrub/skid audio.
        let steering_scrub = ((steer_load * (telemetry.speed_mps / 18.0).clamp(0.0, 1.0))
            .max((max_lat_slip * 2.2).clamp(0.0, 1.0))
            * (1.0 - ((max_slip - 0.55) / 0.75).clamp(0.0, 1.0)))
        .clamp(0.0, 1.0);

        let mut fx_requests = Vec::<Value>::new();
        let mut audio_error = None::<String>;
        {
            let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
                return Ok(());
            };
            let controls = &mut binding.audio_fx;
            let audio = AudioClient::new();

            if elapsed >= controls.audio_retry_after_seconds {
                // GTA vehicle audio is layered: a low/idle bed remains present,
                // load adds intake/combustion, high RPM adds a separate bright
                // layer, and exhaust follows load with a slightly different curve.
                let running = telemetry.engine_running;
                let misfire_gain = if telemetry.engine_misfiring {
                    (0.42 + (((elapsed * 27.0).sin() * 0.5 + 0.5) as f32) * 0.58).clamp(0.42, 1.0)
                } else {
                    1.0
                };
                let rpm = engine_speed.clamp(0.0, 1.0);
                let high_ratio = ((rpm - 0.48) / 0.47).clamp(0.0, 1.0);
                let low_presence = 1.0 - high_ratio * 0.62;
                let base_gain = controls.engine_gain
                    * (0.17 + 0.40 * low_presence + 0.10 * throttle_load)
                    * misfire_gain;
                let base_pitch =
                    ((0.72 + rpm * 0.58) * (0.94 + 0.06 * misfire_gain)).clamp(0.55, 1.55);
                if let Err(error) = sync_loop_voice(
                    &audio,
                    controls.engine_clip.as_deref(),
                    &mut controls.engine_voice,
                    running && controls.engine_clip.is_some(),
                    base_gain,
                    base_pitch,
                    entity,
                    position,
                ) {
                    audio_error = Some(error);
                }

                if audio_error.is_none() {
                    let load_gain = controls.engine_gain
                        * 0.62
                        * throttle_load
                        * (0.24 + 0.76 * rpm)
                        * (0.30 + 0.70 * clutch)
                        * misfire_gain;
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.engine_load_clip.as_deref(),
                        &mut controls.engine_load_voice,
                        running && load_gain > 0.015 && controls.engine_load_clip.is_some(),
                        load_gain,
                        (0.78 + rpm * 0.70).clamp(0.62, 1.62),
                        entity,
                        position,
                    ) {
                        audio_error = Some(error);
                    }
                }

                if audio_error.is_none() {
                    let high_gain = controls.engine_gain
                        * 0.54
                        * high_ratio
                        * (0.22 + throttle_load * 0.78)
                        * (0.45 + 0.55 * clutch)
                        * misfire_gain;
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.engine_high_clip.as_deref(),
                        &mut controls.engine_high_voice,
                        running && high_gain > 0.015 && controls.engine_high_clip.is_some(),
                        high_gain,
                        (0.91 + rpm * 0.82).clamp(0.72, 1.78),
                        entity,
                        position,
                    ) {
                        audio_error = Some(error);
                    }
                }

                if audio_error.is_none() {
                    let exhaust_gain = controls.engine_gain
                        * (0.08
                            + throttle_load * (0.44 + rpm * 0.20)
                            + rpm * 0.16
                            + forward_speed_ratio.min(1.0) * 0.06)
                            .clamp(0.0, 1.0)
                        * misfire_gain;
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.exhaust_clip.as_deref(),
                        &mut controls.exhaust_voice,
                        running
                            && telemetry.specification.exhaust
                            && telemetry.specification.uses_combustion(telemetry.class)
                            && controls.exhaust_clip.is_some(),
                        exhaust_gain,
                        (0.70 + rpm * 0.74).clamp(0.56, 1.55),
                        entity,
                        position,
                    ) {
                        audio_error = Some(error);
                    }
                }

                if audio_error.is_none() {
                    // Generic road/tyre roll belongs to rubber contact only.
                    // A bare rim uses WHEEL_RIM instead of layering tyre-road
                    // noise underneath the metal scrape/friction loop.
                    let road_gain = controls.tyre_gain
                        * speed_ratio.clamp(0.0, 1.0).sqrt()
                        * rubber_contact_factor
                        * telemetry
                            .wheels
                            .iter()
                            .filter(rubber_contact)
                            .map(|wheel| wheel.surface_grip_multiplier)
                            .fold(1.0f32, f32::min)
                            .clamp(0.25, 1.25);
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.road_clip.as_deref(),
                        &mut controls.road_voice,
                        rubber_road_active && road_gain > 0.01 && controls.road_clip.is_some(),
                        road_gain,
                        (0.72 + speed_ratio.min(1.25) * 0.48).clamp(0.68, 1.35),
                        entity,
                        position,
                    ) {
                        audio_error = Some(error);
                    }
                }

                if audio_error.is_none() {
                    let brake_disc_gain = controls.tyre_gain
                        * brake_load
                        * (telemetry.speed_mps / 16.0).clamp(0.0, 1.0)
                        * contact_factor;
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.brake_disc_clip.as_deref(),
                        &mut controls.brake_disc_voice,
                        brake_disc_gain > 0.02 && controls.brake_disc_clip.is_some(),
                        brake_disc_gain,
                        (0.92 + telemetry.speed_mps / 90.0).clamp(0.85, 1.35),
                        entity,
                        position,
                    ) {
                        audio_error = Some(error);
                    }
                }

                if audio_error.is_none() {
                    let flat_count = telemetry
                        .wheels
                        .iter()
                        .filter(|wheel| wheel.tire_condition == TireCondition::Punctured)
                        .count();
                    let flat_gain = controls.tyre_gain
                        * (flat_count as f32 / telemetry.wheels.len().max(1) as f32)
                        * (telemetry.speed_mps / 18.0).clamp(0.0, 1.0);
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.flat_tyre_clip.as_deref(),
                        &mut controls.flat_tyre_voice,
                        flat_count > 0
                            && telemetry.speed_mps > 1.0
                            && controls.flat_tyre_clip.is_some(),
                        flat_gain,
                        (0.82 + telemetry.speed_mps / 70.0).clamp(0.75, 1.55),
                        entity,
                        position,
                    ) {
                        audio_error = Some(error);
                    }
                }

                if audio_error.is_none() {
                    let rim_gain = controls.tyre_gain
                        * (rim_contact_count as f32 / telemetry.wheels.len().max(1) as f32)
                        * (0.35 + (telemetry.speed_mps / 12.0).clamp(0.0, 1.0) * 0.65)
                        * (0.55 + max_rim_slip.clamp(0.0, 2.0) * 0.45);
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.wheel_rim_clip.as_deref(),
                        &mut controls.wheel_rim_voice,
                        rim_contact_count > 0
                            && telemetry.speed_mps > 0.45
                            && controls.wheel_rim_clip.is_some(),
                        rim_gain,
                        (0.82 + telemetry.speed_mps / 70.0 + max_rim_slip * 0.08).clamp(0.72, 1.65),
                        entity,
                        position,
                    ) {
                        audio_error = Some(error);
                    }
                }

                if audio_error.is_none() {
                    let tyre_gain = controls.tyre_gain
                        * ((max_slip - 0.12) / 0.88).clamp(0.0, 1.0)
                        * rubber_contact_factor
                        * telemetry
                            .wheels
                            .iter()
                            .filter(rubber_contact)
                            .map(|wheel| wheel.surface_grip_multiplier)
                            .fold(1.0f32, f32::min)
                            .clamp(0.2, 1.25);
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.tyre_skid_clip.as_deref(),
                        &mut controls.tyre_voice,
                        tyre_active && controls.tyre_skid_clip.is_some(),
                        tyre_gain,
                        (0.82 + telemetry.speed_mps / 85.0).clamp(0.65, 1.8),
                        entity,
                        position,
                    ) {
                        audio_error = Some(error);
                    }
                }

                if audio_error.is_none() {
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.steering_scrub_clip.as_deref(),
                        &mut controls.steering_voice,
                        rubber_road_active
                            && steering_scrub > 0.06
                            && controls.steering_scrub_clip.is_some(),
                        controls.tyre_gain * steering_scrub * 0.72,
                        (0.88 + telemetry.speed_mps / 110.0).clamp(0.75, 1.45),
                        entity,
                        position,
                    ) {
                        audio_error = Some(error);
                    }
                }

                if audio_error.is_none() {
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.siren_clip.as_deref(),
                        &mut controls.siren_voice,
                        binding.lights.siren
                            && !binding.cabin.siren_muted
                            && controls.siren_clip.is_some(),
                        controls.siren_gain,
                        1.0,
                        entity,
                        position,
                    ) {
                        audio_error = Some(error);
                    }
                }
                if audio_error.is_none() {
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.horn_clip.as_deref(),
                        &mut controls.horn_voice,
                        binding.cabin.horn
                            || (telemetry.alarm.active()
                                && ((elapsed * 2.0).floor() as u64 & 1) == 0),
                        0.9,
                        1.0,
                        entity,
                        position,
                    ) {
                        audio_error = Some(error);
                    }
                }
                if audio_error.is_none() {
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.radio_clip.as_deref(),
                        &mut controls.radio_voice,
                        controls.radio_clip.is_some(),
                        0.45,
                        1.0,
                        entity,
                        position,
                    ) {
                        audio_error = Some(error);
                    }
                }
                if audio_error.is_some() {
                    controls.audio_retry_after_seconds = elapsed + 2.0;
                }
            }

            // The original vehicle sound set treats hard braking as a discrete
            // chirp, not merely a louder continuous skid. Latch it until the
            // brake/slip condition is released so one braking event => one cue.
            let brake_chirp = telemetry.speed_mps > 3.5
                && brake_load >= 0.85
                && contact_wheels == telemetry.wheels.len()
                && max_long_slip > 0.62;
            if brake_chirp && !controls.brake_chirp_latched {
                if let Some(clip) = controls.brake_chirp_clip.as_deref() {
                    match play_vehicle_one_shot(
                        &audio,
                        clip,
                        (0.30 + speed_ratio * 0.70) * controls.tyre_gain,
                        (0.92 + speed_ratio * 0.18).clamp(0.85, 1.2),
                        entity,
                        position,
                    ) {
                        Ok(()) => controls.audio_events_emitted += 1,
                        Err(error) => {
                            audio_error.get_or_insert(error);
                        }
                    };
                }
                controls.brake_chirp_latched = true;
            } else if brake_load <= 0.25 || max_long_slip < 0.32 {
                controls.brake_chirp_latched = false;
            }

            // Chassis clatter follows suspension acceleration rather than steering
            // input. Damage raises its audibility, matching the source model.
            let suspension_velocity = telemetry
                .wheels
                .iter()
                .filter(|wheel| wheel.contact)
                .map(|wheel| wheel.suspension_velocity.abs())
                .fold(0.0f32, f32::max);
            if controls.last_audio_seconds > 0.0 {
                let dt = (elapsed - controls.last_audio_seconds).clamp(0.001, 0.25) as f32;
                let suspension_accel =
                    ((suspension_velocity - controls.last_suspension_velocity).abs() / dt)
                        .clamp(0.0, 30.0);
                let damage_ratio = (1.0 - binding.body_health / 1000.0).clamp(0.0, 1.0);
                let clatter = ((suspension_accel - 1.8) / 10.0).clamp(0.0, 1.0)
                    * (telemetry.speed_mps / 12.0).clamp(0.15, 1.0)
                    * (0.35 + damage_ratio * 0.65);
                if clatter > 0.12 && elapsed >= controls.next_clatter_seconds {
                    if let Some(clip) = controls.suspension_clatter_clip.as_deref() {
                        match play_vehicle_one_shot(
                            &audio,
                            clip,
                            clatter,
                            (0.92 + clatter * 0.14).clamp(0.85, 1.12),
                            entity,
                            position,
                        ) {
                            Ok(()) => controls.audio_events_emitted += 1,
                            Err(error) => {
                                audio_error.get_or_insert(error);
                            }
                        };
                    }
                    controls.next_clatter_seconds = elapsed + 0.12;
                }
            }
            controls.last_suspension_velocity = suspension_velocity;
            controls.last_audio_seconds = elapsed;

            if telemetry.engine_running
                && telemetry.specification.exhaust
                && telemetry.specification.uses_combustion(telemetry.class)
                && controls.exhaust_effect.is_some()
                && elapsed >= controls.next_exhaust_seconds
            {
                let exhaust_parts = binding
                    .parts
                    .iter()
                    .filter(|part| {
                        part.role == ModelFragmentPartRole::Exhaust
                            && part.visible
                            && part.detached_entity.is_none()
                            && part.damage < 1.0
                    })
                    .collect::<Vec<_>>();
                if !exhaust_parts.is_empty() {
                    let direction = rotate_euler_xyz([0.0, 0.05, 1.0], rotation_degrees);
                    for part in exhaust_parts {
                        let world =
                            vehicle_local_point(position, rotation_degrees, scale, part.pivot);
                        let mut request = json!({
                            "asset_ref": controls.exhaust_effect.as_deref().unwrap_or_default(),
                            "position": world,
                            "direction": direction,
                            "scale": 0.65 + throttle_load * 0.65,
                            "count_scale": 0.65 + throttle_load * 1.1,
                            "inherited_velocity": self.physics.as_ref()
                                .map(|p|p.body_linear_velocity(entity)).unwrap_or([0.0;3]),
                            "seed": (entity as u32)
                                .wrapping_add(part.index)
                                .wrapping_add((elapsed * 60.0) as u32)
                        });
                        if let Some(fallback) = controls.fallback_catalog.as_deref() {
                            request["fallback_catalog"] = Value::String(fallback.to_owned());
                        }
                        fx_requests.push(request);
                    }
                    controls.next_exhaust_seconds =
                        elapsed + (0.22 - throttle_load as f64 * 0.11).clamp(0.10, 0.22);
                }
            }

            if controls.tyre_effect.is_some()
                && tyre_active
                && elapsed >= controls.next_tyre_seconds
            {
                let up = rotate_euler_xyz([0.0, 1.0, 0.0], rotation_degrees);
                for part in binding
                    .parts
                    .iter()
                    .filter(|part| part.role == ModelFragmentPartRole::Wheel)
                {
                    let Some(wheel_index) = resolve_wheel_index(
                        self.vehicles.definition(entity).expect("vehicle checked"),
                        part.wheel_slot,
                    ) else {
                        continue;
                    };
                    let Some(wheel) = telemetry.wheels.get(wheel_index) else {
                        continue;
                    };
                    let wheel_slip = wheel.slip_intensity;
                    if !wheel.contact
                        || matches!(
                            wheel.tire_condition,
                            TireCondition::Rim | TireCondition::Missing
                        )
                        || wheel_slip <= 0.82
                        || wheel.sliding_speed_mps <= 1.0
                    {
                        continue;
                    }
                    let local = [
                        part.pivot[0],
                        part.pivot[1] - wheel.compression,
                        part.pivot[2],
                    ];
                    let world = wheel.contact_position.unwrap_or_else(|| {
                        vehicle_local_point(position, rotation_degrees, scale, local)
                    });
                    let mut request = json!({
                        "asset_ref": controls.tyre_effect.as_deref().unwrap_or_default(),
                        "position": world,
                        "direction": up,
                        "scale": (0.30 + (wheel_slip - 0.65).max(0.0) * 0.42).clamp(0.30, 1.7),
                        "count_scale": ((wheel_slip - 0.72) * 0.9).clamp(0.08, 1.7),
                        "seed": (entity as u32)
                            .wrapping_mul(31)
                            .wrapping_add(part.index)
                            .wrapping_add((elapsed * 75.0) as u32)
                    });
                    if let Some(fallback) = controls.fallback_catalog.as_deref() {
                        request["fallback_catalog"] = Value::String(fallback.to_owned());
                    }
                    fx_requests.push(request);
                }
                let smoke_rate = ((max_slip - 0.8) / 1.7).clamp(0.0, 1.0) as f64;
                controls.next_tyre_seconds =
                    elapsed + (0.11 - smoke_rate * 0.055).clamp(0.05, 0.11);
            }

            if rim_contact_count > 0
                && telemetry.speed_mps > 0.8
                && elapsed >= controls.next_rim_scrape_seconds
            {
                if let Some(effect) = controls.damage_effects.get("scrape_effect") {
                    for (wheel_index, wheel) in telemetry.wheels.iter().enumerate() {
                        if !wheel.contact || wheel.tire_condition != TireCondition::Rim {
                            continue;
                        }
                        let Some(contact) = wheel.contact_position else {
                            continue;
                        };
                        let direction = wheel.contact_normal.unwrap_or([0.0, 1.0, 0.0]);
                        let scrape = wheel
                            .slip_intensity
                            .max((telemetry.speed_mps / 22.0).clamp(0.12, 1.0));
                        let mut request = json!({
                            "asset_ref": effect,
                            "position": contact,
                            "direction": direction,
                            "scale": (0.25 + scrape * 0.30).clamp(0.25, 0.85),
                            "count_scale": (0.18 + scrape * 0.60).clamp(0.18, 1.2),
                            "inherited_velocity": self.physics.as_ref()
                                .map(|physics| physics.body_linear_velocity(entity))
                                .unwrap_or([0.0; 3]),
                            "seed": (entity as u32)
                                .wrapping_mul(131)
                                .wrapping_add(wheel_index as u32)
                                .wrapping_add((elapsed * 91.0) as u32)
                        });
                        if let Some(fallback) = controls.fallback_catalog.as_deref() {
                            request["fallback_catalog"] = Value::String(fallback.to_owned());
                        }
                        fx_requests.push(request);
                    }
                    controls.next_rim_scrape_seconds =
                        elapsed + (0.10 - max_rim_slip.min(1.5) as f64 * 0.025).clamp(0.055, 0.10);
                }
            }

            if elapsed >= controls.next_damage_fx_seconds {
                let inherited_velocity = self
                    .physics
                    .as_ref()
                    .map(|physics| physics.body_linear_velocity(entity))
                    .unwrap_or([0.0; 3]);
                let up = rotate_euler_xyz([0.0, 1.0, 0.0], rotation_degrees);
                let down = up.map(|value| -value);

                let engine_part = binding
                    .parts
                    .iter()
                    .find(|part| {
                        part.visible
                            && part.detached_entity.is_none()
                            && (part.name_lower.contains("engine")
                                || part.name_lower.contains("overheat"))
                    })
                    .or_else(|| {
                        binding.parts.iter().find(|part| {
                            part.role == ModelFragmentPartRole::Bonnet
                                && part.visible
                                && part.detached_entity.is_none()
                        })
                    });
                let engine_position = engine_part
                    .map(|part| vehicle_local_point(position, rotation_degrees, scale, part.pivot))
                    .unwrap_or(position);

                let rear_part = binding
                    .parts
                    .iter()
                    .filter(|part| part.visible && part.detached_entity.is_none())
                    .max_by(|a, b| a.pivot[2].total_cmp(&b.pivot[2]));
                let rear_position = rear_part
                    .map(|part| vehicle_local_point(position, rotation_degrees, scale, part.pivot))
                    .unwrap_or(position);

                if telemetry.engine_smoke_level > 0.01 {
                    let open = binding.parts.iter().any(|part| {
                        part.role == ModelFragmentPartRole::Bonnet
                            && (part.open > 0.08 || part.detached_entity.is_some())
                    });
                    let effect = if open {
                        controls
                            .damage_effects
                            .get("engine_open_smoke_effect")
                            .map(String::as_str)
                            .or(controls.engine_smoke_effect.as_deref())
                    } else {
                        controls.engine_smoke_effect.as_deref()
                    };
                    if let Some(effect) = effect {
                        fx_requests.push(json!({
                            "asset_ref": effect,
                            "position": engine_position,
                            "direction": up,
                            "scale": 0.45 + telemetry.engine_smoke_level * 1.25,
                            "count_scale": 0.35 + telemetry.engine_smoke_level * 1.9,
                            "inherited_velocity": inherited_velocity,
                            "seed": (entity as u32).wrapping_add((elapsed * 83.0) as u32)
                        }));
                    }
                }
                if telemetry.engine_fire_level > 0.01 {
                    if let Some(effect) = controls.engine_fire_effect.as_deref() {
                        fx_requests.push(json!({
                            "asset_ref": effect,
                            "position": engine_position,
                            "direction": up,
                            "scale": 0.55 + telemetry.engine_fire_level * 1.25,
                            "count_scale": 0.55 + telemetry.engine_fire_level * 1.8,
                            "inherited_velocity": inherited_velocity,
                            "seed": (entity as u32).wrapping_add((elapsed * 97.0) as u32)
                        }));
                    }
                }
                let oil_leak_level = ((telemetry.engine_smoke_level - 0.5) * 2.0).clamp(0.0, 1.0);
                if oil_leak_level > 0.01 {
                    if let Some(effect) = controls.oil_leak_effect.as_deref() {
                        fx_requests.push(json!({
                            "asset_ref": effect,
                            "position": engine_position,
                            "direction": down,
                            "scale": 0.45 + oil_leak_level * 0.55,
                            "count_scale": 0.35 + oil_leak_level * 1.1,
                            "inherited_velocity": inherited_velocity,
                            "seed": (entity as u32).wrapping_add((elapsed * 71.0) as u32)
                        }));
                    }
                }
                if telemetry.petrol_leak_level > 0.01 {
                    if let Some(effect) = controls.petrol_leak_effect.as_deref() {
                        fx_requests.push(json!({
                            "asset_ref": effect,
                            "position": rear_position,
                            "direction": down,
                            "scale": 0.45 + telemetry.petrol_leak_level * 0.65,
                            "count_scale": 0.45 + telemetry.petrol_leak_level * 1.25,
                            "inherited_velocity": inherited_velocity,
                            "seed": (entity as u32).wrapping_add((elapsed * 67.0) as u32)
                        }));
                    }
                }
                if telemetry.petrol_fire_level > 0.01 {
                    if let Some(effect) = controls.petrol_fire_effect.as_deref() {
                        for side in [-0.42f32, 0.42f32] {
                            let local = rear_part
                                .map(|part| [part.pivot[0] + side, part.pivot[1], part.pivot[2]])
                                .unwrap_or([side, 0.0, 1.0]);
                            fx_requests.push(json!({
                                "asset_ref": effect,
                                "position": vehicle_local_point(position, rotation_degrees, scale, local),
                                "direction": up,
                                "scale": 0.55 + telemetry.petrol_fire_level * 1.15,
                                "count_scale": 0.55 + telemetry.petrol_fire_level * 1.55,
                                "inherited_velocity": inherited_velocity,
                                "seed": (entity as u32)
                                    .wrapping_add((elapsed * 101.0) as u32)
                                    .wrapping_add((side.to_bits()).rotate_left(7))
                            }));
                        }
                    }
                }

                if telemetry.engine_misfiring {
                    if let Some(effect) = controls.misfire_effect.as_deref() {
                        for part in binding.parts.iter().filter(|part| {
                            part.role == ModelFragmentPartRole::Exhaust
                                && part.visible
                                && part.detached_entity.is_none()
                        }) {
                            fx_requests.push(json!({
                                "asset_ref": effect,
                                "position": vehicle_local_point(position, rotation_degrees, scale, part.pivot),
                                "direction": rotate_euler_xyz([0.0, 0.05, 1.0], rotation_degrees),
                                "scale": 0.75,
                                "count_scale": 0.9,
                                "inherited_velocity": inherited_velocity,
                                "seed": (entity as u32)
                                    .wrapping_add(part.index)
                                    .wrapping_add((elapsed * 119.0) as u32)
                            }));
                        }
                    }
                }

                if elapsed < binding.wreck_fire_until_seconds {
                    if let Some(effect) = controls.damage_effects.get("wreck_fire_effect") {
                        // GTA attaches the main wreck fire to the cabin and
                        // additional fires to the engine and rear body. Use the
                        // surviving mesh surfaces when those bones are absent:
                        // a vehicle's root can be below the opaque floor panel.
                        let meshes = binding
                            .parts
                            .iter()
                            .filter(|part| {
                                part.visible
                                    && part.detached_entity.is_none()
                                    && matches!(
                                        part.role,
                                        ModelFragmentPartRole::Body
                                            | ModelFragmentPartRole::Bonnet
                                            | ModelFragmentPartRole::Boot
                                            | ModelFragmentPartRole::Roof
                                    )
                            })
                            .flat_map(|part| part.mesh_names.iter().cloned())
                            .collect::<Vec<_>>();
                        let (anchors, fire_scale) = self
                            .scene
                            .entity_fragment_mesh_bounds(entity, &meshes)
                            .map(|(min, max)| {
                                let x = (min[0] + max[0]) * 0.5;
                                let z = (min[2] + max[2]) * 0.5;
                                let height = max[1] - min[1];
                                let length = max[2] - min[2];
                                let shoulder = min[1] + height * 0.65;
                                let local = [
                                    [x, max[1] + 0.04, z],
                                    [x, shoulder, min[2] + length * 0.18],
                                    [x, shoulder, max[2] - length * 0.18],
                                ];
                                (
                                    local.map(|point| {
                                        vehicle_local_point(
                                            position,
                                            rotation_degrees,
                                            scale,
                                            point,
                                        )
                                    }),
                                    ((max[0] - min[0]).abs() * scale[0].abs() * 0.8)
                                        .clamp(0.75, 2.0),
                                )
                            })
                            .unwrap_or(([position, engine_position, rear_position], 1.0));
                        for (index, anchor) in anchors.into_iter().enumerate() {
                            fx_requests.push(json!({
                                "asset_ref": effect,
                                "position": anchor,
                                "direction": [0.0, 1.0, 0.0],
                                "scale": fire_scale,
                                "count_scale": 1.0,
                                "inherited_velocity": inherited_velocity,
                                "emission_seconds": 0.085,
                                "seed": (entity as u32).wrapping_add((elapsed * 61.0) as u32)
                                    .wrapping_add((index as u32).wrapping_mul(7919))
                            }));
                        }
                    }
                }
                if let Some(effect) = controls.part_fire_effect.as_deref() {
                    for part in binding.parts.iter().filter(|part| {
                        part.fire_intensity > 0.01 && part.visible && part.detached_entity.is_none()
                    }) {
                        let intensity = part.fire_intensity.clamp(0.0, 1.0);
                        fx_requests.push(json!({
                            "asset_ref": effect,
                            "position": vehicle_local_point(position, rotation_degrees, scale, part.pivot),
                            "direction": up,
                            "scale": 0.35 + intensity * 0.95,
                            "count_scale": 0.35 + intensity * 1.35,
                            "inherited_velocity": inherited_velocity,
                            "seed": (entity as u32)
                                .wrapping_mul(37)
                                .wrapping_add(part.index)
                                .wrapping_add((elapsed * 89.0) as u32)
                        }));
                    }
                }

                if let Some(fallback) = controls.fallback_catalog.as_deref() {
                    for request in &mut fx_requests {
                        if request.get("fallback_catalog").is_none() {
                            request["fallback_catalog"] = Value::String(fallback.to_owned());
                        }
                    }
                }
                controls.next_damage_fx_seconds = elapsed + 0.085;
            }
        }

        if let Some(error) = audio_error {
            host::warn(
                "newviso.vehicle.audio",
                format!("vehicle entity={entity} audio update deferred: {error}"),
            );
        }
        for mut request in fx_requests {
            request["emitter_id"] = json!(entity);
            request["emission_seconds"] = json!(0.085);
            let report =
                application_particle_effects::spawn_particle_effect(&mut self.scene, &request, 0);
            if let Ok(report) = &report {
                if let Some(binding) = self.vehicle_presentations.get_mut(&entity) {
                    binding.audio_fx.damage_emitted += report.emitted as u64;
                }
                if request
                    .get("asset_ref")
                    .and_then(Value::as_str)
                    .is_some_and(|r| {
                        self.vehicle_presentations
                            .get(&entity)
                            .is_some_and(|b| b.audio_fx.exhaust_effect.as_deref() == Some(r))
                    })
                {
                    if let Some(binding) = self.vehicle_presentations.get_mut(&entity) {
                        binding.audio_fx.exhaust_emitted += report.emitted as u64;
                    }
                }
            }
            if let Err(error) = report {
                host::warn(
                    "newviso.vehicle.fx",
                    format!("vehicle entity={entity} particle effect skipped: {error}"),
                );
            }
        }
        Ok(())
    }
}
