use super::*;

impl VehicleAudioFxControls {
    pub(super) fn from_runtime_config(config: Option<&Value>) -> Self {
        let mut controls = Self::default();
        let Some(config) = config else {
            return controls;
        };
        macro_rules! reference {
            ($field:ident) => {
                controls.$field = config
                    .get(stringify!($field))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned);
            };
        }
        reference!(engine_clip);
        reference!(engine_start_clip);
        reference!(engine_shutdown_clip);
        reference!(engine_breakdown_clip);
        reference!(engine_load_clip);
        reference!(engine_high_clip);
        reference!(exhaust_clip);
        reference!(gear_shift_clip);
        reference!(road_clip);
        reference!(brake_disc_clip);
        reference!(brake_release_clip);
        reference!(handbrake_clip);
        reference!(tyre_puncture_clip);
        reference!(tyre_burst_clip);
        reference!(flat_tyre_clip);
        reference!(wheel_rim_clip);
        reference!(suspension_impact_clip);
        reference!(door_open_clip);
        reference!(door_close_clip);
        reference!(door_locked_attempt_clip);
        reference!(damage_oneshot_clip);
        reference!(tyre_skid_clip);
        reference!(steering_scrub_clip);
        reference!(brake_chirp_clip);
        reference!(suspension_clatter_clip);
        reference!(impact_clip);
        reference!(impact_heavy_clip);
        reference!(glass_break_clip);
        reference!(horn_clip);
        reference!(siren_clip);
        reference!(exhaust_effect);
        reference!(tyre_effect);
        reference!(tyre_puncture_effect);
        reference!(tyre_burst_effect);
        reference!(engine_smoke_effect);
        reference!(engine_fire_effect);
        reference!(oil_leak_effect);
        reference!(petrol_leak_effect);
        reference!(petrol_fire_effect);
        reference!(misfire_effect);
        reference!(part_fire_effect);
        reference!(explosion_effect);
        reference!(impact_effect);
        reference!(glass_effect);
        for name in [
            "debris_effect",
            "part_break_effect",
            "scrape_effect",
            "bullet_impact_effect",
            "side_window_effect",
            "windscreen_effect",
            "engine_open_smoke_effect",
            "wreck_fire_effect",
            "explosion_secondary_effect",
            "explosion_post_effect",
        ] {
            if let Some(reference) = config
                .get(name)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                controls
                    .damage_effects
                    .insert(name.to_owned(), reference.to_owned());
            }
        }
        controls.debris_lifetime_seconds = config
            .get("debris_lifetime_seconds")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0.0, 600.0) as f32)
            .unwrap_or(0.0);
        controls.max_debris = config
            .get("max_debris")
            .and_then(Value::as_u64)
            .map(|v| v.clamp(1, 256) as usize)
            .unwrap_or(48);
        controls
    }

    pub(super) fn stop_all_voices(&mut self, audio: &AudioClient) {
        for voice in [
            self.engine_voice.take(),
            self.engine_load_voice.take(),
            self.engine_high_voice.take(),
            self.exhaust_voice.take(),
            self.road_voice.take(),
            self.brake_disc_voice.take(),
            self.flat_tyre_voice.take(),
            self.wheel_rim_voice.take(),
            self.tyre_voice.take(),
            self.steering_voice.take(),
            self.siren_voice.take(),
            self.horn_voice.take(),
            self.radio_voice.take(),
        ]
        .into_iter()
        .flatten()
        {
            let _ = audio.stop(voice);
        }
    }
}

impl EngineApplication {
    pub(super) fn sync_vehicle_reference_events(&mut self) -> Result<(), String> {
        let events = self.vehicles.drain_events();
        if events.is_empty() {
            return Ok(());
        }
        let audio = AudioClient::new();

        for event in events {
            self.publish_vehicle_script_event(&event)?;
            if matches!(
                event.kind,
                VehicleEventKind::EngineFireStarted | VehicleEventKind::PetrolFireStarted
            ) {
                self.apply_vehicle_fire_event_presentation(event.entity, event.kind);
            }
            if event.kind == VehicleEventKind::VehicleExploded {
                self.apply_vehicle_explosion_presentation(event.entity);
            }
            let Some(binding) = self.vehicle_presentations.get(&event.entity) else {
                continue;
            };
            let controls = binding.audio_fx.clone();
            let transform = self.scene.entity_transform_values(event.entity);
            let mut position = transform
                .map(|(position, _, _)| position)
                .unwrap_or([0.0; 3]);
            if let (Some(part_name), Some((base, rotation, scale))) =
                (event.part.as_deref(), transform)
            {
                if let Some(part) = binding
                    .parts
                    .iter()
                    .find(|part| part.name.eq_ignore_ascii_case(part_name))
                {
                    let center = self
                        .scene
                        .entity_fragment_mesh_bounds(event.entity, &part.mesh_names)
                        .map(|(min, max)| std::array::from_fn(|i| (min[i] + max[i]) * 0.5))
                        .unwrap_or(part.pivot);
                    position = vehicle_local_point(base, rotation, scale, center);
                }
            }
            if let Some(part_name) = event.part.as_deref() {
                if let Some(debris) = self
                    .vehicle_debris
                    .values()
                    .filter(|debris| {
                        debris.source_entity == event.entity
                            && debris.part.eq_ignore_ascii_case(part_name)
                    })
                    .max_by(|a, b| {
                        a.created_seconds
                            .total_cmp(&b.created_seconds)
                            .then_with(|| a.entity.cmp(&b.entity))
                    })
                {
                    if let Some(body) = self
                        .physics
                        .as_ref()
                        .and_then(|p| p.vehicle_body_state(debris.entity))
                    {
                        position = body.position;
                    }
                }
            }
            if let Some(wheel_index) = event.wheel_index {
                if let Some(contact) = self
                    .vehicles
                    .wheel_telemetry(event.entity, wheel_index)
                    .and_then(|wheel| wheel.contact_position)
                {
                    position = contact;
                }
            }

            if event.wheel_index.is_none() {
                if let Some((base, rotation, scale)) = transform {
                    let anchor = match event.kind {
                        VehicleEventKind::EngineDamaged
                        | VehicleEventKind::EngineFireStarted
                        | VehicleEventKind::OilLeakStarted => binding
                            .parts
                            .iter()
                            .find(|part| {
                                part.name_lower.contains("engine")
                                    || part.name_lower.contains("overheat")
                            })
                            .or_else(|| {
                                binding
                                    .parts
                                    .iter()
                                    .find(|part| part.role == ModelFragmentPartRole::Bonnet)
                            }),
                        VehicleEventKind::PetrolLeakStarted
                        | VehicleEventKind::PetrolFireStarted => binding
                            .parts
                            .iter()
                            .filter(|part| part.visible && part.detached_entity.is_none())
                            .max_by(|a, b| a.pivot[2].total_cmp(&b.pivot[2])),
                        VehicleEventKind::EngineMisfireStarted => binding
                            .parts
                            .iter()
                            .find(|part| part.role == ModelFragmentPartRole::Exhaust),
                        _ => None,
                    };
                    if let Some(part) = anchor {
                        position = vehicle_local_point(base, rotation, scale, part.pivot);
                    }
                }
            }

            if let Some(hit) = event.position {
                position = if event.local_space {
                    transform
                        .map(|(base, rotation, scale)| {
                            vehicle_local_point(base, rotation, scale, hit)
                        })
                        .unwrap_or(position)
                } else {
                    hit
                };
            } else if let (Some(index), Some((base, rotation, scale))) =
                (event.part_index, transform)
            {
                if let Some(part) = binding.parts.iter().find(|p| p.index == index) {
                    let center = self
                        .scene
                        .entity_fragment_mesh_bounds(event.entity, &part.mesh_names)
                        .map(|(min, max)| std::array::from_fn(|i| (min[i] + max[i]) * 0.5))
                        .unwrap_or(part.pivot);
                    position = vehicle_local_point(base, rotation, scale, center);
                }
            }
            let impact_normal = event
                .normal
                .map(|n| {
                    if event.local_space {
                        transform
                            .map(|(_, rotation, _)| rotate_euler_xyz(n, rotation))
                            .unwrap_or(n)
                    } else {
                        n
                    }
                })
                .filter(|n| n.iter().map(|x| x * x).sum::<f32>() > 0.001)
                .unwrap_or([0.0, 1.0, 0.0]);

            let glass_laminated = binding
                .parts
                .iter()
                .find(|p| {
                    event.part_index == Some(p.index)
                        || event
                            .part
                            .as_deref()
                            .is_some_and(|name| name.eq_ignore_ascii_case(&p.name))
                })
                .is_some_and(|p| {
                    p.name_lower.contains("windscreen") || p.name_lower.contains("windshield")
                });
            let (clip, gain, pitch) = match event.kind {
                VehicleEventKind::EngineStarted => {
                    (controls.engine_start_clip.as_deref(), 1.0, 1.0)
                }
                VehicleEventKind::EngineStartFailed => {
                    (controls.engine_breakdown_clip.as_deref(), 0.95, 1.0)
                }
                VehicleEventKind::EngineStopped => {
                    (controls.engine_shutdown_clip.as_deref(), 0.9, 1.0)
                }
                VehicleEventKind::GearShifted => (
                    controls.gear_shift_clip.as_deref(),
                    (0.48 + event.speed_mps / 70.0).clamp(0.48, 1.0),
                    (0.93 + event.speed_mps / 180.0).clamp(0.9, 1.12),
                ),
                VehicleEventKind::BrakeReleased => (
                    controls.brake_release_clip.as_deref(),
                    (0.55 + event.hold_seconds * 0.35).clamp(0.55, 1.0),
                    1.0,
                ),
                VehicleEventKind::HandbrakeApplied | VehicleEventKind::HandbrakeReleased => (
                    controls.handbrake_clip.as_deref(),
                    0.82,
                    if matches!(event.kind, VehicleEventKind::HandbrakeReleased) {
                        1.03
                    } else {
                        0.98
                    },
                ),
                VehicleEventKind::TyrePunctured => {
                    (controls.tyre_puncture_clip.as_deref(), 0.95, 1.0)
                }
                VehicleEventKind::TyreBurst | VehicleEventKind::WheelDetached => {
                    (controls.tyre_burst_clip.as_deref(), 1.0, 1.0)
                }
                VehicleEventKind::SuspensionImpact | VehicleEventKind::JumpLanded => (
                    controls.suspension_impact_clip.as_deref(),
                    (0.35 + event.magnitude * 0.18).clamp(0.35, 1.2),
                    (0.96 + event.magnitude * 0.015).clamp(0.9, 1.12),
                ),
                VehicleEventKind::DoorOpened => (controls.door_open_clip.as_deref(), 0.9, 1.0),
                VehicleEventKind::DoorClosed => (
                    controls.door_close_clip.as_deref(),
                    (0.72 + event.magnitude * 0.18).clamp(0.72, 1.08),
                    (0.97 + event.magnitude * 0.025).clamp(0.97, 1.06),
                ),
                VehicleEventKind::DoorLockedAttempt => {
                    (controls.door_locked_attempt_clip.as_deref(), 0.92, 1.0)
                }
                VehicleEventKind::EngineDamaged
                | VehicleEventKind::Deformation
                | VehicleEventKind::LightSmashed => (
                    controls.damage_oneshot_clip.as_deref(),
                    (0.35 + event.magnitude / 120.0).clamp(0.35, 1.0),
                    1.0,
                ),
                VehicleEventKind::OilLeakStarted
                | VehicleEventKind::EngineFireStarted
                | VehicleEventKind::PetrolLeakStarted
                | VehicleEventKind::PetrolFireStarted
                | VehicleEventKind::VehicleDisabled => {
                    (controls.engine_breakdown_clip.as_deref(), 0.95, 1.0)
                }
                // The source archive presently has no confirmed final cue
                // binding for these transitions in Shared. Fail closed rather
                // than substituting an unrelated impact/backfire sample.
                VehicleEventKind::EngineMisfireStarted
                | VehicleEventKind::EngineMisfireStopped
                | VehicleEventKind::FuelExhausted
                | VehicleEventKind::VehicleExploded => (None, 0.0, 1.0),
                // Continuous state events are represented by their dedicated
                // loops. CollisionImpact is already played at the exact physics
                // contact point by apply_vehicle_contact_damage.
                VehicleEventKind::SkidStarted
                | VehicleEventKind::SkidStopped
                | VehicleEventKind::WheelSpinStarted
                | VehicleEventKind::WheelSpinStopped
                | VehicleEventKind::CollisionImpact
                | VehicleEventKind::CollisionScrape
                | VehicleEventKind::GlassCracked
                | VehicleEventKind::GlassBroken
                | VehicleEventKind::DoorLatchLoosened
                | VehicleEventKind::DoorBrokenOff
                | VehicleEventKind::BonnetBrokenOff
                | VehicleEventKind::BootBrokenOff
                | VehicleEventKind::PartLoose
                | VehicleEventKind::PartBrokenOff
                | VehicleEventKind::HornStarted
                | VehicleEventKind::HornStopped
                | VehicleEventKind::SirenStarted
                | VehicleEventKind::SirenStopped
                | VehicleEventKind::VehicleRepaired
                | VehicleEventKind::VehicleCreated
                | VehicleEventKind::VehicleRemoved
                | VehicleEventKind::OccupantEntered
                | VehicleEventKind::OccupantLeft
                | VehicleEventKind::SeatChanged
                | VehicleEventKind::AlarmStarted
                | VehicleEventKind::AlarmStopped
                | VehicleEventKind::CoolingFanStarted
                | VehicleEventKind::CoolingFanStopped
                | VehicleEventKind::LockChanged
                | VehicleEventKind::LightsChanged => (None, 0.0, 1.0),
                VehicleEventKind::DamageApplied
                | VehicleEventKind::SpecificationChanged
                | VehicleEventKind::DriveabilityChanged
                | VehicleEventKind::VehicleRestored
                | VehicleEventKind::OilLeakStopped
                | VehicleEventKind::PetrolLeakStopped
                | VehicleEventKind::EngineFireStopped
                | VehicleEventKind::PetrolFireStopped => (None, 0.0, 1.0),
            };

            if let Some(clip) = clip {
                match play_vehicle_one_shot(&audio, clip, gain, pitch, event.entity, position) {
                    Ok(()) => {
                        if let Some(binding) = self.vehicle_presentations.get_mut(&event.entity) {
                            binding.audio_fx.audio_events_emitted += 1;
                        }
                    }
                    Err(error) => host::warn(
                        "newviso.vehicle.audio",
                        format!(
                            "vehicle event {:?} entity={} cue skipped: {error}",
                            event.kind, event.entity
                        ),
                    ),
                }
            }

            let effect = match event.kind {
                VehicleEventKind::TyrePunctured => controls.tyre_puncture_effect.as_deref(),
                VehicleEventKind::TyreBurst | VehicleEventKind::WheelDetached => {
                    controls.tyre_burst_effect.as_deref()
                }
                VehicleEventKind::EngineFireStarted => controls.engine_fire_effect.as_deref(),
                VehicleEventKind::OilLeakStarted => controls.oil_leak_effect.as_deref(),
                VehicleEventKind::PetrolLeakStarted => controls.petrol_leak_effect.as_deref(),
                VehicleEventKind::PetrolFireStarted => controls.petrol_fire_effect.as_deref(),
                VehicleEventKind::EngineMisfireStarted => controls.misfire_effect.as_deref(),
                VehicleEventKind::VehicleExploded => controls.explosion_effect.as_deref(),
                VehicleEventKind::GlassBroken => controls
                    .damage_effects
                    .get(if glass_laminated {
                        "windscreen_effect"
                    } else {
                        "side_window_effect"
                    })
                    .map(String::as_str)
                    .or(controls.glass_effect.as_deref()),
                VehicleEventKind::LightSmashed => controls.glass_effect.as_deref(),
                VehicleEventKind::DoorBrokenOff
                | VehicleEventKind::BonnetBrokenOff
                | VehicleEventKind::BootBrokenOff
                | VehicleEventKind::PartBrokenOff => controls
                    .damage_effects
                    .get("part_break_effect")
                    .map(String::as_str),
                _ => None,
            };
            if let Some(effect) = effect {
                let mut request = json!({
                    "asset_ref": effect,
                    "position": position,
                    "direction": if matches!(
                        event.kind,
                        VehicleEventKind::OilLeakStarted | VehicleEventKind::PetrolLeakStarted
                    ) { [0.0, -1.0, 0.0] } else { impact_normal },
                    "scale": 1.0,
                    "count_scale": 1.0,
                    "inherited_velocity": self.physics
                        .as_ref()
                        .map(|physics| physics.body_linear_velocity(event.entity))
                        .unwrap_or([0.0; 3]),
                    "seed": (event.entity as u32)
                        .wrapping_add(event.sequence as u32)
                        .wrapping_mul(31)
                });
                if let Some(fallback) = controls.fallback_catalog.as_deref() {
                    request["fallback_catalog"] = Value::String(fallback.to_owned());
                }
                match application_particle_effects::spawn_particle_effect(
                    &mut self.scene,
                    &request,
                    0,
                ) {
                    Ok(report) => {
                        if let Some(binding) = self.vehicle_presentations.get_mut(&event.entity) {
                            binding.audio_fx.damage_emitted += report.emitted as u64;
                        }
                    }
                    Err(error) => host::warn(
                        "newviso.vehicle.fx",
                        format!(
                            "vehicle entity={} semantic damage effect skipped: {error}",
                            event.entity
                        ),
                    ),
                }
            }
            if event.kind == VehicleEventKind::VehicleExploded {
                for name in ["explosion_secondary_effect", "explosion_post_effect"] {
                    if let Some(effect) = controls.damage_effects.get(name) {
                        let request = json!({"asset_ref": effect, "position": position, "direction": [0.0,1.0,0.0], "scale": 1.0, "seed": event.sequence as u32 ^ name.len() as u32});
                        match application_particle_effects::spawn_particle_effect(
                            &mut self.scene,
                            &request,
                            0,
                        ) {
                            Ok(report) => {
                                if let Some(binding) =
                                    self.vehicle_presentations.get_mut(&event.entity)
                                {
                                    binding.audio_fx.damage_emitted += report.emitted as u64;
                                }
                            }
                            Err(error) => host::warn(
                                "newviso.vehicle.fx",
                                format!("secondary explosion effect skipped: {error}"),
                            ),
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn sync_vehicle_audio_fx(
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
        let tyre_active = telemetry.speed_mps > 2.0 && rubber_contact_wheels > 0 && max_slip > 0.82;
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
                    {
                        continue;
                    }
                    let local = [
                        part.pivot[0],
                        part.pivot[1] - wheel.compression,
                        part.pivot[2],
                    ];
                    let world = vehicle_local_point(position, rotation_degrees, scale, local);
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

    pub(crate) fn set_vehicle_audio_fx_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.audio_fx.configure requires resolved entity"
                )
            })?;

        let binding = self
            .vehicle_presentations
            .get_mut(&entity)
            .ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.audio_fx.configure entity {entity} has no fragment presentation"
                )
            })?;

        let audio = AudioClient::new();
        if command.get("engine_clip").is_some() {
            set_optional_string_and_stop_voice(
                command,
                "engine_clip",
                &mut binding.audio_fx.engine_clip,
                &mut binding.audio_fx.engine_voice,
                &audio,
                index,
            )?;
        }
        for key in [
            "engine_load_clip",
            "engine_high_clip",
            "exhaust_clip",
            "road_clip",
            "brake_disc_clip",
            "flat_tyre_clip",
            "wheel_rim_clip",
            "steering_scrub_clip",
        ] {
            if command.get(key).is_some() {
                let (clip, voice) = match key {
                    "engine_load_clip" => (
                        &mut binding.audio_fx.engine_load_clip,
                        &mut binding.audio_fx.engine_load_voice,
                    ),
                    "engine_high_clip" => (
                        &mut binding.audio_fx.engine_high_clip,
                        &mut binding.audio_fx.engine_high_voice,
                    ),
                    "exhaust_clip" => (
                        &mut binding.audio_fx.exhaust_clip,
                        &mut binding.audio_fx.exhaust_voice,
                    ),
                    "road_clip" => (
                        &mut binding.audio_fx.road_clip,
                        &mut binding.audio_fx.road_voice,
                    ),
                    "brake_disc_clip" => (
                        &mut binding.audio_fx.brake_disc_clip,
                        &mut binding.audio_fx.brake_disc_voice,
                    ),
                    "flat_tyre_clip" => (
                        &mut binding.audio_fx.flat_tyre_clip,
                        &mut binding.audio_fx.flat_tyre_voice,
                    ),
                    "wheel_rim_clip" => (
                        &mut binding.audio_fx.wheel_rim_clip,
                        &mut binding.audio_fx.wheel_rim_voice,
                    ),
                    _ => (
                        &mut binding.audio_fx.steering_scrub_clip,
                        &mut binding.audio_fx.steering_voice,
                    ),
                };
                set_optional_string_and_stop_voice(command, key, clip, voice, &audio, index)?;
            }
        }
        if command.get("tyre_skid_clip").is_some() {
            set_optional_string_and_stop_voice(
                command,
                "tyre_skid_clip",
                &mut binding.audio_fx.tyre_skid_clip,
                &mut binding.audio_fx.tyre_voice,
                &audio,
                index,
            )?;
        }
        if command.get("siren_clip").is_some() {
            set_optional_string_and_stop_voice(
                command,
                "siren_clip",
                &mut binding.audio_fx.siren_clip,
                &mut binding.audio_fx.siren_voice,
                &audio,
                index,
            )?;
        }
        if command.get("engine_gain").is_some() {
            binding.audio_fx.engine_gain =
                optional_number(command, "engine_gain", binding.audio_fx.engine_gain, index)?
                    .clamp(0.0, 4.0);
        }
        if command.get("tyre_gain").is_some() {
            binding.audio_fx.tyre_gain =
                optional_number(command, "tyre_gain", binding.audio_fx.tyre_gain, index)?
                    .clamp(0.0, 4.0);
        }
        for key in ["horn_clip", "radio_clip"] {
            if command.get(key).is_some() {
                let (clip, voice) = if key == "horn_clip" {
                    (
                        &mut binding.audio_fx.horn_clip,
                        &mut binding.audio_fx.horn_voice,
                    )
                } else {
                    (
                        &mut binding.audio_fx.radio_clip,
                        &mut binding.audio_fx.radio_voice,
                    )
                };
                set_optional_string_and_stop_voice(command, key, clip, voice, &audio, index)?;
            }
        }
        if command.get("siren_gain").is_some() {
            binding.audio_fx.siren_gain =
                optional_number(command, "siren_gain", binding.audio_fx.siren_gain, index)?
                    .clamp(0.0, 4.0);
        }
        set_optional_string(
            command,
            "exhaust_effect",
            &mut binding.audio_fx.exhaust_effect,
            index,
        )?;
        set_optional_string(
            command,
            "tyre_effect",
            &mut binding.audio_fx.tyre_effect,
            index,
        )?;
        for key in [
            "tyre_puncture_effect",
            "tyre_burst_effect",
            "engine_smoke_effect",
            "engine_fire_effect",
            "oil_leak_effect",
            "petrol_leak_effect",
            "petrol_fire_effect",
            "misfire_effect",
            "part_fire_effect",
            "explosion_effect",
        ] {
            let target = match key {
                "tyre_puncture_effect" => &mut binding.audio_fx.tyre_puncture_effect,
                "tyre_burst_effect" => &mut binding.audio_fx.tyre_burst_effect,
                "engine_smoke_effect" => &mut binding.audio_fx.engine_smoke_effect,
                "engine_fire_effect" => &mut binding.audio_fx.engine_fire_effect,
                "oil_leak_effect" => &mut binding.audio_fx.oil_leak_effect,
                "petrol_leak_effect" => &mut binding.audio_fx.petrol_leak_effect,
                "petrol_fire_effect" => &mut binding.audio_fx.petrol_fire_effect,
                "misfire_effect" => &mut binding.audio_fx.misfire_effect,
                "part_fire_effect" => &mut binding.audio_fx.part_fire_effect,
                _ => &mut binding.audio_fx.explosion_effect,
            };
            set_optional_string(command, key, target, index)?;
        }
        for key in [
            "engine_start_clip",
            "engine_shutdown_clip",
            "engine_breakdown_clip",
            "gear_shift_clip",
            "brake_release_clip",
            "handbrake_clip",
            "tyre_puncture_clip",
            "tyre_burst_clip",
            "suspension_impact_clip",
            "door_open_clip",
            "door_close_clip",
            "door_locked_attempt_clip",
            "damage_oneshot_clip",
            "brake_chirp_clip",
            "suspension_clatter_clip",
            "impact_clip",
            "impact_heavy_clip",
            "glass_break_clip",
        ] {
            let target = match key {
                "engine_start_clip" => &mut binding.audio_fx.engine_start_clip,
                "engine_shutdown_clip" => &mut binding.audio_fx.engine_shutdown_clip,
                "engine_breakdown_clip" => &mut binding.audio_fx.engine_breakdown_clip,
                "gear_shift_clip" => &mut binding.audio_fx.gear_shift_clip,
                "brake_release_clip" => &mut binding.audio_fx.brake_release_clip,
                "handbrake_clip" => &mut binding.audio_fx.handbrake_clip,
                "tyre_puncture_clip" => &mut binding.audio_fx.tyre_puncture_clip,
                "tyre_burst_clip" => &mut binding.audio_fx.tyre_burst_clip,
                "suspension_impact_clip" => &mut binding.audio_fx.suspension_impact_clip,
                "door_open_clip" => &mut binding.audio_fx.door_open_clip,
                "door_close_clip" => &mut binding.audio_fx.door_close_clip,
                "door_locked_attempt_clip" => &mut binding.audio_fx.door_locked_attempt_clip,
                "damage_oneshot_clip" => &mut binding.audio_fx.damage_oneshot_clip,
                "brake_chirp_clip" => &mut binding.audio_fx.brake_chirp_clip,
                "suspension_clatter_clip" => &mut binding.audio_fx.suspension_clatter_clip,
                "impact_clip" => &mut binding.audio_fx.impact_clip,
                "impact_heavy_clip" => &mut binding.audio_fx.impact_heavy_clip,
                _ => &mut binding.audio_fx.glass_break_clip,
            };
            set_optional_string(command, key, target, index)?;
        }
        set_optional_string(
            command,
            "impact_effect",
            &mut binding.audio_fx.impact_effect,
            index,
        )?;
        set_optional_string(
            command,
            "glass_effect",
            &mut binding.audio_fx.glass_effect,
            index,
        )?;
        set_optional_string(
            command,
            "fallback_catalog",
            &mut binding.audio_fx.fallback_catalog,
            index,
        )?;
        for name in [
            "debris_effect",
            "part_break_effect",
            "scrape_effect",
            "bullet_impact_effect",
            "side_window_effect",
            "windscreen_effect",
            "engine_open_smoke_effect",
            "wreck_fire_effect",
            "explosion_secondary_effect",
            "explosion_post_effect",
        ] {
            if command.get(name).is_some() {
                let mut value = binding.audio_fx.damage_effects.get(name).cloned();
                set_optional_string(command, name, &mut value, index)?;
                if let Some(value) = value {
                    binding
                        .audio_fx
                        .damage_effects
                        .insert(name.to_owned(), value);
                } else {
                    binding.audio_fx.damage_effects.remove(name);
                }
            }
        }
        if command.get("debris_lifetime_seconds").is_some() {
            binding.audio_fx.debris_lifetime_seconds = optional_number(
                command,
                "debris_lifetime_seconds",
                binding.audio_fx.debris_lifetime_seconds,
                index,
            )?
            .clamp(0.0, 600.0);
        }
        binding.audio_fx.audio_retry_after_seconds = 0.0;
        Ok(())
    }
}

pub(super) fn vehicle_audio_fx_runtime_state(audio: &VehicleAudioFxControls) -> Value {
    let mut out = serde_json::Map::new();
    macro_rules! field {
        ($name:literal, $value:expr) => {
            out.insert(
                $name.to_owned(),
                serde_json::to_value($value).expect("vehicle audio diagnostics must serialize"),
            );
        };
    }

    // Preserve the original flat diagnostic API used by project scripts while
    // exposing every newly ported reference event/loop cue.
    field!("engine_clip", &audio.engine_clip);
    field!("engine_start_clip", &audio.engine_start_clip);
    field!("engine_shutdown_clip", &audio.engine_shutdown_clip);
    field!("engine_breakdown_clip", &audio.engine_breakdown_clip);
    field!("engine_load_clip", &audio.engine_load_clip);
    field!("engine_high_clip", &audio.engine_high_clip);
    field!("exhaust_clip", &audio.exhaust_clip);
    field!("gear_shift_clip", &audio.gear_shift_clip);
    field!("road_clip", &audio.road_clip);
    field!("brake_disc_clip", &audio.brake_disc_clip);
    field!("brake_release_clip", &audio.brake_release_clip);
    field!("handbrake_clip", &audio.handbrake_clip);
    field!("tyre_puncture_clip", &audio.tyre_puncture_clip);
    field!("tyre_burst_clip", &audio.tyre_burst_clip);
    field!("flat_tyre_clip", &audio.flat_tyre_clip);
    field!("wheel_rim_clip", &audio.wheel_rim_clip);
    field!("tyre_skid_clip", &audio.tyre_skid_clip);
    field!("steering_scrub_clip", &audio.steering_scrub_clip);
    field!("suspension_impact_clip", &audio.suspension_impact_clip);
    field!("door_open_clip", &audio.door_open_clip);
    field!("door_close_clip", &audio.door_close_clip);
    field!("door_locked_attempt_clip", &audio.door_locked_attempt_clip);
    field!("damage_oneshot_clip", &audio.damage_oneshot_clip);
    field!("brake_chirp_clip", &audio.brake_chirp_clip);
    field!("suspension_clatter_clip", &audio.suspension_clatter_clip);
    field!("impact_clip", &audio.impact_clip);
    field!("impact_heavy_clip", &audio.impact_heavy_clip);
    field!("glass_break_clip", &audio.glass_break_clip);
    field!("siren_clip", &audio.siren_clip);
    field!("horn_clip", &audio.horn_clip);
    field!("radio_clip", &audio.radio_clip);

    field!("engine_voice", audio.engine_voice);
    field!("engine_load_voice", audio.engine_load_voice);
    field!("engine_high_voice", audio.engine_high_voice);
    field!("exhaust_voice", audio.exhaust_voice);
    field!("road_voice", audio.road_voice);
    field!("brake_disc_voice", audio.brake_disc_voice);
    field!("flat_tyre_voice", audio.flat_tyre_voice);
    field!("wheel_rim_voice", audio.wheel_rim_voice);
    field!("tyre_voice", audio.tyre_voice);
    field!("steering_voice", audio.steering_voice);
    field!("siren_voice", audio.siren_voice);
    field!("horn_voice", audio.horn_voice);
    field!("radio_voice", audio.radio_voice);

    field!("audio_events_emitted", audio.audio_events_emitted);
    field!("exhaust_effect", &audio.exhaust_effect);
    field!("impact_effect", &audio.impact_effect);
    field!("tyre_effect", &audio.tyre_effect);
    field!("tyre_puncture_effect", &audio.tyre_puncture_effect);
    field!("tyre_burst_effect", &audio.tyre_burst_effect);
    field!("engine_smoke_effect", &audio.engine_smoke_effect);
    field!("engine_fire_effect", &audio.engine_fire_effect);
    field!("oil_leak_effect", &audio.oil_leak_effect);
    field!("petrol_leak_effect", &audio.petrol_leak_effect);
    field!("petrol_fire_effect", &audio.petrol_fire_effect);
    field!("misfire_effect", &audio.misfire_effect);
    field!("part_fire_effect", &audio.part_fire_effect);
    field!("explosion_effect", &audio.explosion_effect);
    field!("exhaust_emitted", audio.exhaust_emitted);
    field!("impact_emitted", audio.impact_emitted);
    field!("damage_emitted", audio.damage_emitted);
    field!("damage_effects", &audio.damage_effects);
    field!("debris_lifetime_seconds", audio.debris_lifetime_seconds);
    field!("max_debris", audio.max_debris);
    field!("physical_debris_persistent", true);

    Value::Object(out)
}

pub(super) fn sync_loop_voice(
    audio: &AudioClient,
    clip: Option<&str>,
    voice_id: &mut Option<u64>,
    active: bool,
    gain: f32,
    speed: f32,
    entity: u64,
    position: [f32; 3],
) -> Result<(), String> {
    let Some(clip) = clip.map(str::trim).filter(|value| !value.is_empty()) else {
        if let Some(existing) = voice_id.take() {
            let _ = audio.stop(existing);
        }
        return Ok(());
    };

    if !active {
        if let Some(existing) = voice_id.take() {
            let _ = audio.stop(existing);
        }
        return Ok(());
    }

    if let Some(existing) = *voice_id {
        match audio.update_voice(&AudioVoiceUpdateRequest {
            voice_id: existing,
            gain: Some(gain.clamp(0.0, 4.0)),
            speed: Some(speed.clamp(0.05, 4.0)),
            paused: Some(!active),
        }) {
            Ok(ack) if ack.accepted => return Ok(()),
            Ok(ack) => {
                *voice_id = None;
                return Err(format!(
                    "looping vehicle voice {existing} update rejected: {}",
                    ack.message
                ));
            }
            Err(error) => {
                *voice_id = None;
                return Err(error);
            }
        }
    }

    if !active {
        return Ok(());
    }
    // Select the semantic playback contract from AssetManager metadata. The
    // engine never parses or guesses the dictionary's source/container format.
    let address = AssetAddress::parse(clip)?;
    let is_cue = newviso_assets_client::AssetClient::new()
        .resolve_type(address.logical_path())
        .ok()
        .and_then(|info| {
            info.get("descriptor")
                .and_then(|d| d.get("asset_kind"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|kind| kind == "sound_cue_dictionary");
    if !is_cue {
        if let Some(existing) = voice_id.take() {
            let _ = audio.stop(existing);
        }
        return Err(format!(
            "vehicle audio requires a final YSCD cue; raw/fallback clip '{clip}' is rejected"
        ));
    }
    let request = serde_json::to_vec(&json!({"version":1,"cue":{"logical_path":clip},
        "gain":gain.clamp(0.0,4.0),"pitch":speed.clamp(0.05,4.0),
        "position":position,"scope_id":entity}))
    .map_err(|e| e.to_string())?;
    let reply = host::call_service(
        newviso_audio_client::ENGINE_AUDIO_SERVICE_ID,
        "play_cue_json_v1",
        &request,
    )?;
    let ack: newviso_audio_client::AudioPlayAck = serde_json::from_slice(&reply)
        .map_err(|e| format!("audio cue play returned invalid acknowledgement: {e}"))?;
    if !ack.accepted {
        return Err(format!(
            "audio provider rejected looping vehicle clip '{clip}': {}",
            ack.message
        ));
    }
    *voice_id = ack.voice_id;
    if voice_id.is_none() {
        return Err(format!(
            "audio provider accepted looping vehicle clip '{clip}' without a voice id"
        ));
    }
    Ok(())
}

pub(super) fn play_vehicle_one_shot(
    _audio: &AudioClient,
    clip: &str,
    gain: f32,
    pitch: f32,
    entity: u64,
    position: [f32; 3],
) -> Result<(), String> {
    let clip = clip.trim();
    if clip.is_empty() {
        return Ok(());
    }
    let address = AssetAddress::parse(clip)?;
    let is_cue = newviso_assets_client::AssetClient::new()
        .resolve_type(address.logical_path())
        .ok()
        .and_then(|info| {
            info.get("descriptor")
                .and_then(|d| d.get("asset_kind"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|kind| kind == "sound_cue_dictionary");
    if !is_cue {
        return Err(format!(
            "vehicle one-shot requires a final YSCD cue; raw/fallback clip '{clip}' is rejected"
        ));
    }
    let request = serde_json::to_vec(&json!({
        "version":1,
        "cue":{"logical_path":clip},
        "gain":gain.clamp(0.0,4.0),
        "pitch":pitch.clamp(0.05,4.0),
        "position":position,
        "scope_id":entity,
        "seed":entity.wrapping_mul(0x9E3779B185EBCA87)
            ^ u64::from(position[0].to_bits()).rotate_left(7)
            ^ u64::from(position[1].to_bits()).rotate_left(23)
            ^ u64::from(position[2].to_bits()).rotate_left(41)
            ^ u64::from(gain.to_bits())
            ^ u64::from(pitch.to_bits()).rotate_left(17)
    }))
    .map_err(|e| e.to_string())?;
    let reply = host::call_service(
        newviso_audio_client::ENGINE_AUDIO_SERVICE_ID,
        "play_cue_json_v1",
        &request,
    )?;
    let ack: newviso_audio_client::AudioPlayAck = serde_json::from_slice(&reply)
        .map_err(|e| format!("vehicle one-shot returned invalid acknowledgement: {e}"))?;
    if !ack.accepted {
        return Err(format!(
            "audio provider rejected vehicle cue '{clip}': {}",
            ack.message
        ));
    }
    Ok(())
}

pub(super) fn set_optional_string(
    command: &Value,
    key: &str,
    target: &mut Option<String>,
    index: usize,
) -> Result<(), String> {
    let Some(raw) = command.get(key) else {
        return Ok(());
    };
    if raw.is_null() {
        *target = None;
        return Ok(());
    }
    let value = raw
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "script command[{index}] vehicle presentation field '{key}' must be non-empty string or null"
            )
        })?;
    *target = Some(value.to_owned());
    Ok(())
}

pub(super) fn set_optional_string_and_stop_voice(
    command: &Value,
    key: &str,
    target: &mut Option<String>,
    voice_id: &mut Option<u64>,
    audio: &AudioClient,
    index: usize,
) -> Result<(), String> {
    let previous = target.clone();
    set_optional_string(command, key, target, index)?;
    if previous != *target {
        if let Some(existing) = voice_id.take() {
            let _ = audio.stop(existing);
        }
    }
    Ok(())
}
