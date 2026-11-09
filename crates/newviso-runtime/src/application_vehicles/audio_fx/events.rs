use super::*;

impl EngineApplication {
    pub(crate) fn sync_vehicle_reference_events(&mut self) -> Result<(), String> {
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
}
