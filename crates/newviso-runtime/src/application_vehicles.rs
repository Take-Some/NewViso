use super::*;
use newviso_audio_client::{AudioClient, AudioClipRef, AudioPlayRequest, AudioVoiceUpdateRequest};
use newviso_model::{ModelFragmentPartRole, ModelResource, ModelWheelSlot};
use newviso_scene::{SceneLightDesc, SceneLightType, SceneModelPartPose};
use newviso_vehicle::{
    AeroHandling, HandlingData, ReferenceHandlingData, VehicleClass, VehicleSurfaceClass,
    VehicleSurfaceProfile, WaterHandling, WheelConfig,
};

#[derive(Clone, Debug)]
pub(super) struct VehiclePresentationBinding {
    model_id: u64,
    model_name: String,
    parts: Vec<VehiclePresentationPart>,
    lights: VehicleLightControls,
    audio_fx: VehicleAudioFxControls,
    occupants: BTreeMap<String, VehicleOccupantBinding>,
    inferred_wheels: Vec<WheelConfig>,
}

#[derive(Clone, Debug)]
struct VehiclePresentationPart {
    index: u32,
    name: String,
    role: ModelFragmentPartRole,
    mesh_names: Vec<String>,
    pivot: [f32; 3],
    wheel_slot: Option<ModelWheelSlot>,
    open: f32,
    visible: bool,
    damage: f32,
    presentation_override: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct VehicleLightControls {
    headlights: bool,
    left_indicator: bool,
    right_indicator: bool,
    hazard: bool,
    siren: bool,
}

#[derive(Clone, Debug)]
struct VehicleAudioFxControls {
    engine_clip: Option<String>,
    tyre_skid_clip: Option<String>,
    siren_clip: Option<String>,
    engine_voice: Option<u64>,
    tyre_voice: Option<u64>,
    siren_voice: Option<u64>,
    engine_gain: f32,
    tyre_gain: f32,
    siren_gain: f32,
    exhaust_effect: Option<String>,
    tyre_effect: Option<String>,
    fallback_catalog: Option<String>,
    next_exhaust_seconds: f64,
    next_tyre_seconds: f64,
    audio_retry_after_seconds: f64,
}

impl Default for VehicleAudioFxControls {
    fn default() -> Self {
        Self {
            engine_clip: None,
            tyre_skid_clip: None,
            siren_clip: None,
            engine_voice: None,
            tyre_voice: None,
            siren_voice: None,
            engine_gain: 0.85,
            tyre_gain: 0.7,
            siren_gain: 0.9,
            exhaust_effect: None,
            tyre_effect: None,
            fallback_catalog: None,
            next_exhaust_seconds: 0.0,
            next_tyre_seconds: 0.0,
            audio_retry_after_seconds: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
struct VehicleOccupantBinding {
    entity: u64,
    position_offset: [f32; 3],
    rotation_offset_degrees: [f32; 3],
}

impl EngineApplication {
    pub(super) fn bind_vehicle_model_presentation(
        &mut self,
        entity: u64,
        model: &ModelResource,
    ) -> Result<bool, String> {
        let previous = self.vehicle_presentations.remove(&entity);
        let Some(fragment) = model.fragment.as_ref() else {
            if let Some(previous) = previous {
                let audio = AudioClient::new();
                for voice in [
                    previous.audio_fx.engine_voice,
                    previous.audio_fx.tyre_voice,
                    previous.audio_fx.siren_voice,
                ]
                .into_iter()
                .flatten()
                {
                    let _ = audio.stop(voice);
                }
                for part in previous.parts {
                    let key = format!("vehicle.{entity}.light.{}", part.index);
                    if self.scene.runtime_entity_state(&key).is_some() {
                        self.scene.remove_runtime_entity(&key)?;
                    }
                }
            }
            return Ok(false);
        };
        let parts = fragment
            .parts
            .iter()
            .map(|part| {
                let old = previous.as_ref().and_then(|binding| {
                    binding
                        .parts
                        .iter()
                        .find(|candidate| candidate.name.eq_ignore_ascii_case(&part.name))
                });
                VehiclePresentationPart {
                    index: part.index,
                    name: part.name.clone(),
                    role: part.role,
                    mesh_names: part.mesh_names.clone(),
                    pivot: part.rest_position,
                    wheel_slot: part.wheel_slot,
                    open: old.map_or(0.0, |old| old.open),
                    visible: old.is_none_or(|old| old.visible),
                    damage: old.map_or(0.0, |old| old.damage),
                    presentation_override: old.is_some_and(|old| old.presentation_override),
                }
            })
            .collect::<Vec<_>>();
        let handling_for_layout = self
            .vehicles
            .definition(entity)
            .map(|definition| definition.handling.clone())
            .unwrap_or_default();
        let inferred_wheels = infer_wheel_layout_from_model(model, &handling_for_layout);
        let wheel_parts = parts
            .iter()
            .filter(|part| part.role == ModelFragmentPartRole::Wheel)
            .count();
        let articulated_parts = parts
            .iter()
            .filter(|part| {
                matches!(
                    part.role,
                    ModelFragmentPartRole::Wheel
                        | ModelFragmentPartRole::Suspension
                        | ModelFragmentPartRole::WheelHub
                        | ModelFragmentPartRole::Door
                        | ModelFragmentPartRole::Bonnet
                        | ModelFragmentPartRole::Boot
                        | ModelFragmentPartRole::Glass
                        | ModelFragmentPartRole::Breakable
                        | ModelFragmentPartRole::Steering
                )
            })
            .count();
        self.vehicle_presentations.insert(
            entity,
            VehiclePresentationBinding {
                model_id: model.id.0,
                model_name: model.name.clone(),
                parts,
                lights: previous
                    .as_ref()
                    .map_or_else(VehicleLightControls::default, |binding| binding.lights),
                audio_fx: previous
                    .as_ref()
                    .map_or_else(VehicleAudioFxControls::default, |binding| {
                        binding.audio_fx.clone()
                    }),
                occupants: previous
                    .as_ref()
                    .map_or_else(BTreeMap::new, |binding| binding.occupants.clone()),
                inferred_wheels: inferred_wheels.clone(),
            },
        );
        if self.vehicle_auto_wheel_layout.contains(&entity)
            && !inferred_wheels.is_empty()
            && self.vehicles.contains(entity)
        {
            if let Some(current) = self.vehicles.definition(entity).cloned() {
                let mut updated = current;
                updated.wheels = inferred_wheels.clone();
                self.vehicles.upsert(entity, updated)?;
            }
        }
        host::info(
            "newviso.vehicle.presentation",
            format!(
                "bound fragment model entity={entity} model='{}' parts={} articulated={} wheel_parts={} inferred_wheels={}",
                model.name,
                fragment.parts.len(),
                articulated_parts,
                wheel_parts,
                inferred_wheels.len()
            ),
        );
        Ok(true)
    }

    pub(super) fn sync_vehicle_presentations(&mut self) -> Result<(), String> {
        let entities = self
            .vehicle_presentations
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for entity in entities {
            if !self.vehicles.contains(entity) || !self.scene.entity_model_installed(entity) {
                continue;
            }
            let Some(telemetry) = self.vehicles.telemetry(entity) else {
                continue;
            };
            let Some(definition) = self.vehicles.definition(entity) else {
                continue;
            };
            let Some(binding) = self.vehicle_presentations.get(&entity).cloned() else {
                continue;
            };

            let mut poses = Vec::<SceneModelPartPose>::new();
            for part in &binding.parts {
                let mut pose = SceneModelPartPose {
                    mesh_names: part.mesh_names.clone(),
                    pivot: part.pivot,
                    ..SceneModelPartPose::default()
                };
                match part.role {
                    ModelFragmentPartRole::Wheel => {
                        if let Some(wheel_index) = resolve_wheel_index(definition, part.wheel_slot)
                        {
                            if let Some(wheel) = telemetry.wheels.get(wheel_index) {
                                pose.translation[1] = wheel.compression;
                                pose.rotation_degrees[0] = wheel.rotation_angle.to_degrees();
                                pose.rotation_degrees[1] = wheel.steer_angle.to_degrees();
                            }
                        }
                    }
                    ModelFragmentPartRole::Suspension | ModelFragmentPartRole::WheelHub => {
                        if let Some(slot) = part
                            .wheel_slot
                            .or_else(|| infer_wheel_slot_from_name(&part.name))
                        {
                            if let Some(wheel_index) = resolve_wheel_index(definition, Some(slot)) {
                                if let Some(wheel) = telemetry.wheels.get(wheel_index) {
                                    pose.translation[1] = wheel.compression;
                                    if part.role == ModelFragmentPartRole::WheelHub {
                                        pose.rotation_degrees[1] = wheel.steer_angle.to_degrees();
                                    }
                                }
                            }
                        }
                    }
                    ModelFragmentPartRole::Door => {
                        let direction = if part.name.to_ascii_lowercase().contains("pside")
                            || part.name.to_ascii_lowercase().contains("_rf")
                            || part.name.to_ascii_lowercase().contains("_rr")
                        {
                            -1.0
                        } else {
                            1.0
                        };
                        let damage = part.damage.clamp(0.0, 1.0);
                        pose.rotation_degrees[1] = direction * 72.0 * part.open;
                        pose.rotation_degrees[2] += direction * damage * 4.5;
                        pose.translation[0] += direction * damage * 0.035;
                        pose.translation[1] -= damage * 0.018;
                    }
                    ModelFragmentPartRole::Bonnet => {
                        let damage = part.damage.clamp(0.0, 1.0);
                        pose.rotation_degrees[0] = -58.0 * part.open - damage * 5.0;
                        pose.translation[1] -= damage * 0.025;
                    }
                    ModelFragmentPartRole::Boot => {
                        let damage = part.damage.clamp(0.0, 1.0);
                        pose.rotation_degrees[0] = 68.0 * part.open + damage * 4.0;
                        pose.translation[1] -= damage * 0.018;
                    }
                    ModelFragmentPartRole::Steering => {
                        pose.rotation_degrees[2] = -telemetry.input.steer * 420.0;
                    }
                    ModelFragmentPartRole::BodyPanel => {
                        let damage = part.damage.clamp(0.0, 1.0);
                        let side = if part.pivot[0] < 0.0 { -1.0 } else { 1.0 };
                        pose.scale = [
                            1.0 - damage * 0.045,
                            1.0 - damage * 0.09,
                            1.0 - damage * 0.025,
                        ];
                        pose.translation[0] -= side * damage * 0.035;
                        pose.translation[1] -= damage * 0.025;
                        pose.rotation_degrees[2] += side * damage * 2.5;
                    }
                    _ => {}
                }
                pose.visible = part.visible
                    && !(matches!(
                        part.role,
                        ModelFragmentPartRole::Glass | ModelFragmentPartRole::Breakable
                    ) && part.damage >= 1.0);
                let continuously_articulated = matches!(
                    part.role,
                    ModelFragmentPartRole::Wheel
                        | ModelFragmentPartRole::Suspension
                        | ModelFragmentPartRole::WheelHub
                        | ModelFragmentPartRole::Door
                        | ModelFragmentPartRole::Bonnet
                        | ModelFragmentPartRole::Boot
                        | ModelFragmentPartRole::Steering
                        | ModelFragmentPartRole::BodyPanel
                        | ModelFragmentPartRole::Glass
                        | ModelFragmentPartRole::Breakable
                );
                if continuously_articulated || part.presentation_override {
                    poses.push(pose);
                }
            }
            if !poses.is_empty() {
                self.scene.set_entity_model_part_poses(entity, &poses)?;
            }
            self.sync_vehicle_lights(entity, &binding, &telemetry)?;
            self.sync_vehicle_audio_fx(entity, &telemetry)?;
            self.sync_vehicle_occupants(entity, &binding)?;
        }
        Ok(())
    }

    fn sync_vehicle_lights(
        &mut self,
        entity: u64,
        binding: &VehiclePresentationBinding,
        telemetry: &newviso_vehicle::VehicleTelemetry,
    ) -> Result<(), String> {
        let Some((position, rotation_degrees, scale)) = self.scene.entity_transform_values(entity)
        else {
            return Ok(());
        };
        let blink_on = ((self.elapsed_seconds * 1.8).floor() as i64 & 1) == 0;
        let brake_on = telemetry.input.brake > 0.05 || telemetry.input.handbrake > 0.2;
        let reverse_on = telemetry.gear < 0;

        for part in binding.parts.iter().filter(|part| {
            matches!(
                part.role,
                ModelFragmentPartRole::Light | ModelFragmentPartRole::Siren
            )
        }) {
            let lower = part.name.to_ascii_lowercase();
            let left = lower.contains("_l") || lower.contains("left") || lower.contains("dside");
            let right = lower.contains("_r") || lower.contains("right") || lower.contains("pside");
            let (active, color, intensity, range, light_type) = if lower.contains("headlight") {
                (
                    binding.lights.headlights,
                    [0.92, 0.96, 1.0],
                    18.0,
                    46.0,
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
                let side_enabled = binding.lights.hazard
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
            if !active {
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

    fn sync_vehicle_audio_fx(
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
        let engine_speed = telemetry.engine_speed.clamp(0.0, 1.25);
        let max_slip = telemetry
            .wheels
            .iter()
            .filter(|wheel| wheel.contact)
            .map(|wheel| {
                wheel
                    .longitudinal_slip
                    .abs()
                    .max(wheel.lateral_slip_angle.abs() * 2.5)
            })
            .fold(0.0f32, f32::max);
        let tyre_active = telemetry.speed_mps > 2.0 && max_slip > 0.12;

        let mut fx_requests = Vec::<Value>::new();
        let mut audio_error = None::<String>;
        {
            let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
                return Ok(());
            };
            let controls = &mut binding.audio_fx;

            if elapsed >= controls.audio_retry_after_seconds {
                let audio = AudioClient::new();
                let engine_gain =
                    controls.engine_gain * (0.24 + 0.76 * throttle_load.max(engine_speed * 0.55));
                let engine_pitch = 0.62 + engine_speed * 1.55;
                if let Err(error) = sync_loop_voice(
                    &audio,
                    controls.engine_clip.as_deref(),
                    &mut controls.engine_voice,
                    controls.engine_clip.is_some(),
                    engine_gain,
                    engine_pitch,
                ) {
                    audio_error = Some(error);
                }

                if audio_error.is_none() {
                    let tyre_gain = controls.tyre_gain
                        * ((max_slip - 0.12) / 0.88).clamp(0.0, 1.0)
                        * telemetry
                            .wheels
                            .iter()
                            .filter(|wheel| wheel.contact)
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
                    ) {
                        audio_error = Some(error);
                    }
                }

                if audio_error.is_none() {
                    if let Err(error) = sync_loop_voice(
                        &audio,
                        controls.siren_clip.as_deref(),
                        &mut controls.siren_voice,
                        binding.lights.siren && controls.siren_clip.is_some(),
                        controls.siren_gain,
                        1.0,
                    ) {
                        audio_error = Some(error);
                    }
                }

                if audio_error.is_some() {
                    controls.audio_retry_after_seconds = elapsed + 2.0;
                }
            }

            if controls.exhaust_effect.is_some()
                && throttle_load > 0.04
                && elapsed >= controls.next_exhaust_seconds
            {
                let exhaust_parts = binding
                    .parts
                    .iter()
                    .filter(|part| part.role == ModelFragmentPartRole::Exhaust)
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
                            "count_scale": 0.25 + throttle_load * 0.75,
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
                        elapsed + (0.14 - throttle_load as f64 * 0.07).clamp(0.055, 0.14);
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
                    let wheel_slip = wheel
                        .longitudinal_slip
                        .abs()
                        .max(wheel.lateral_slip_angle.abs() * 2.5);
                    if !wheel.contact || wheel_slip <= 0.12 {
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
                        "scale": (0.35 + wheel_slip * 0.7).clamp(0.35, 1.6),
                        "count_scale": ((wheel_slip - 0.1) * 1.5).clamp(0.1, 1.5),
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
                controls.next_tyre_seconds = elapsed + 0.09;
            }
        }

        if let Some(error) = audio_error {
            host::warn(
                "newviso.vehicle.audio",
                format!("vehicle entity={entity} audio update deferred: {error}"),
            );
        }
        for request in fx_requests {
            if let Err(error) =
                application_particle_effects::spawn_particle_effect(&mut self.scene, &request, 0)
            {
                host::warn(
                    "newviso.vehicle.fx",
                    format!("vehicle entity={entity} particle effect skipped: {error}"),
                );
            }
        }
        Ok(())
    }

    fn sync_vehicle_occupants(
        &mut self,
        vehicle_entity: u64,
        binding: &VehiclePresentationBinding,
    ) -> Result<(), String> {
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

    pub(super) fn set_vehicle_audio_fx_from_script(
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
        set_optional_string(
            command,
            "fallback_catalog",
            &mut binding.audio_fx.fallback_catalog,
            index,
        )?;
        binding.audio_fx.audio_retry_after_seconds = 0.0;
        Ok(())
    }

    pub(super) fn set_vehicle_occupant_from_script(
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
        let seat = command
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
                binding.occupants.remove(&seat);
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
        if !binding.parts.iter().any(|part| {
            part.role == ModelFragmentPartRole::Seat && part.name.eq_ignore_ascii_case(&seat)
        }) {
            return Err(format!(
                "script command[{index}] vehicle.occupant.set seat '{seat}' was not imported from the vehicle fragment"
            ));
        }
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
            seat,
            VehicleOccupantBinding {
                entity: occupant,
                position_offset,
                rotation_offset_degrees,
            },
        );
        Ok(())
    }

    pub(super) fn apply_vehicle_contact_damage(
        &mut self,
        contact: application_physics::PhysicsDamageContact,
    ) -> Result<bool, String> {
        if !self.vehicles.contains(contact.target) {
            return Ok(false);
        }
        let Some(binding) = self.vehicle_presentations.get_mut(&contact.target) else {
            return Ok(false);
        };
        let Some(definition) = self.vehicles.definition(contact.target) else {
            return Ok(false);
        };
        let Some((position, rotation_degrees, scale)) =
            self.scene.entity_transform_values(contact.target)
        else {
            return Ok(false);
        };

        let mass = definition.handling.mass.max(1.0);
        let impact = (contact.contact_impulse / (mass * 0.30))
            .max(contact.direct_damage / 90.0)
            .clamp(0.0, 1.0);
        if impact < 0.015 {
            return Ok(false);
        }

        let relative = [
            contact.point[0] - position[0],
            contact.point[1] - position[1],
            contact.point[2] - position[2],
        ];
        let unrotated = inverse_rotate_euler_xyz(relative, rotation_degrees);
        let local_point: [f32; 3] = std::array::from_fn(|axis| {
            let denominator = scale[axis].abs().max(1.0e-4);
            unrotated[axis] / denominator
        });

        let mut nearest = None::<(usize, f32)>;
        for (part_index, part) in binding.parts.iter().enumerate() {
            if !matches!(
                part.role,
                ModelFragmentPartRole::Door
                    | ModelFragmentPartRole::Bonnet
                    | ModelFragmentPartRole::Boot
                    | ModelFragmentPartRole::Glass
                    | ModelFragmentPartRole::BodyPanel
                    | ModelFragmentPartRole::Breakable
                    | ModelFragmentPartRole::Light
                    | ModelFragmentPartRole::Extra
            ) {
                continue;
            }
            let dx = local_point[0] - part.pivot[0];
            let dy = local_point[1] - part.pivot[1];
            let dz = local_point[2] - part.pivot[2];
            let distance_sq = dx * dx + dy * dy + dz * dz;
            if nearest.is_none_or(|(_, best)| distance_sq < best) {
                nearest = Some((part_index, distance_sq));
            }
        }

        let Some((part_index, distance_sq)) = nearest else {
            return Ok(false);
        };
        // Do not punch a remote panel when the collision point is clearly
        // outside the imported vehicle fragment.
        if distance_sq > 12.25 {
            return Ok(false);
        }

        let part = &mut binding.parts[part_index];
        let fragility = match part.role {
            ModelFragmentPartRole::Glass => 2.6,
            ModelFragmentPartRole::Breakable | ModelFragmentPartRole::Light => 2.0,
            ModelFragmentPartRole::Door
            | ModelFragmentPartRole::Bonnet
            | ModelFragmentPartRole::Boot => 1.25,
            _ => 1.0,
        };
        let before = part.damage;
        part.damage = (part.damage + impact * fragility).clamp(0.0, 1.0);
        part.presentation_override = true;

        if part.damage > before + 1.0e-4 {
            host::debug(
                "newviso.vehicle.damage",
                format!(
                    "vehicle={} part='{}' role={:?} impulse={:.2} direct_damage={:.2} damage={:.3}->{:.3}",
                    contact.target,
                    part.name,
                    part.role,
                    contact.contact_impulse,
                    contact.direct_damage,
                    before,
                    part.damage
                ),
            );
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn set_vehicle_surface_policy_from_script(
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

    pub(super) fn set_vehicle_part_from_script(
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
        let binding = self
            .vehicle_presentations
            .get_mut(&entity)
            .ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.part.set entity {entity} has no fragment presentation"
                )
            })?;
        let mut matched = 0usize;
        for part in &mut binding.parts {
            if !part.name.eq_ignore_ascii_case(part_name) {
                continue;
            }
            matched += 1;
            part.presentation_override = true;
            if command.get("open").is_some() {
                part.open = optional_number(command, "open", part.open, index)?.clamp(0.0, 1.0);
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
        }
        if matched == 0 {
            return Err(format!(
                "script command[{index}] vehicle.part.set part '{part_name}' not found on entity {entity}"
            ));
        }
        Ok(())
    }

    pub(super) fn set_vehicle_lights_from_script(
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
        let binding = self
            .vehicle_presentations
            .get_mut(&entity)
            .ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.lights.set entity {entity} has no fragment presentation"
                )
            })?;
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
        Ok(())
    }

    pub(super) fn vehicle_presentation_runtime_state(&self) -> Value {
        Value::Array(
            self.vehicle_presentations
                .iter()
                .map(|(&entity, binding)| {
                    json!({
                        "entity": entity,
                        "model_id": binding.model_id,
                        "model_name": binding.model_name,
                        "inferred_wheel_count": binding.inferred_wheels.len(),
                        "auto_wheel_layout": self.vehicle_auto_wheel_layout.contains(&entity),
                        "lights": {
                            "headlights": binding.lights.headlights,
                            "left_indicator": binding.lights.left_indicator,
                            "right_indicator": binding.lights.right_indicator,
                            "hazard": binding.lights.hazard,
                            "siren": binding.lights.siren
                        },
                        "audio_fx": {
                            "engine_clip": binding.audio_fx.engine_clip,
                            "tyre_skid_clip": binding.audio_fx.tyre_skid_clip,
                            "siren_clip": binding.audio_fx.siren_clip,
                            "engine_voice": binding.audio_fx.engine_voice,
                            "tyre_voice": binding.audio_fx.tyre_voice,
                            "siren_voice": binding.audio_fx.siren_voice,
                            "exhaust_effect": binding.audio_fx.exhaust_effect,
                            "tyre_effect": binding.audio_fx.tyre_effect
                        },
                        "occupants": binding.occupants.iter().map(|(seat, occupant)| json!({
                            "seat": seat,
                            "entity": occupant.entity,
                            "position_offset": occupant.position_offset,
                            "rotation_offset_degrees": occupant.rotation_offset_degrees
                        })).collect::<Vec<_>>(),
                        "parts": binding.parts.iter().map(|part| json!({
                            "index": part.index,
                            "name": part.name,
                            "role": format!("{:?}", part.role),
                            "wheel_slot": part.wheel_slot.map(|slot| format!("{:?}", slot)),
                            "open": part.open,
                            "visible": part.visible,
                            "damage": part.damage,
                            "presentation_override": part.presentation_override,
                            "mesh_names": part.mesh_names
                        })).collect::<Vec<_>>()
                    })
                })
                .collect(),
        )
    }

    pub(super) fn upsert_vehicle_from_script(
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
        } else {
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
                .unwrap_or(VehicleClass::Automobile);
            let mut definition = match class {
                VehicleClass::Bike => VehicleDefinition::bike(),
                _ => VehicleDefinition::automobile(),
            };
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

        self.vehicles.upsert(entity, definition)?;
        if self.scene.entity_state(entity).is_some() {
            self.scene.set_physics_process_active(entity, true)?;
        }
        host::info("newviso.vehicle", format!("vehicle upsert entity={entity}"));
        Ok(entity)
    }

    pub(super) fn remove_vehicle_from_script(
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
        self.vehicles.remove(entity);
        self.vehicle_auto_wheel_layout.remove(&entity);
        if let Some(binding) = self.vehicle_presentations.remove(&entity) {
            let audio = AudioClient::new();
            for voice in [
                binding.audio_fx.engine_voice,
                binding.audio_fx.tyre_voice,
                binding.audio_fx.siren_voice,
            ]
            .into_iter()
            .flatten()
            {
                let _ = audio.stop(voice);
            }
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

    pub(super) fn set_vehicle_input_from_script(
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

    pub(super) fn set_vehicle_enabled_from_script(
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

fn sync_loop_voice(
    audio: &AudioClient,
    clip: Option<&str>,
    voice_id: &mut Option<u64>,
    active: bool,
    gain: f32,
    speed: f32,
) -> Result<(), String> {
    let Some(clip) = clip.map(str::trim).filter(|value| !value.is_empty()) else {
        if let Some(existing) = voice_id.take() {
            let _ = audio.stop(existing);
        }
        return Ok(());
    };

    if let Some(existing) = *voice_id {
        match audio.update_voice(&AudioVoiceUpdateRequest {
            voice_id: existing,
            gain: Some(gain.clamp(0.0, 4.0)),
            speed: Some(speed.clamp(0.05, 4.0)),
            paused: Some(!active),
        }) {
            Ok(ack) if ack.accepted => return Ok(()),
            Ok(_) => {
                *voice_id = None;
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
    let ack = audio.play(&AudioPlayRequest {
        version: 1,
        clip: AudioClipRef::new(clip),
        gain: gain.clamp(0.0, 4.0),
        speed: speed.clamp(0.05, 4.0),
        looping: true,
        paused: false,
    })?;
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

fn set_optional_string(
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

fn set_optional_string_and_stop_voice(
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

fn inverse_rotate_euler_xyz(value: [f32; 3], rotation_degrees: [f32; 3]) -> [f32; 3] {
    let [rx, ry, rz] = rotation_degrees.map(f32::to_radians);
    let (sx, cx) = (-rx).sin_cos();
    let (sy, cy) = (-ry).sin_cos();
    let (sz, cz) = (-rz).sin_cos();

    // Forward transform is X -> Y -> Z, therefore the inverse is -Z -> -Y -> -X.
    let after_z = [
        value[0] * cz - value[1] * sz,
        value[0] * sz + value[1] * cz,
        value[2],
    ];
    let after_y = [
        after_z[0] * cy + after_z[2] * sy,
        after_z[1],
        -after_z[0] * sy + after_z[2] * cy,
    ];
    [
        after_y[0],
        after_y[1] * cx - after_y[2] * sx,
        after_y[1] * sx + after_y[2] * cx,
    ]
}

fn vehicle_local_point(
    position: [f32; 3],
    rotation_degrees: [f32; 3],
    scale: [f32; 3],
    local: [f32; 3],
) -> [f32; 3] {
    let scaled = [
        local[0] * scale[0],
        local[1] * scale[1],
        local[2] * scale[2],
    ];
    let rotated = rotate_euler_xyz(scaled, rotation_degrees);
    [
        position[0] + rotated[0],
        position[1] + rotated[1],
        position[2] + rotated[2],
    ]
}

fn infer_wheel_layout_from_model(
    model: &ModelResource,
    handling: &HandlingData,
) -> Vec<WheelConfig> {
    let Some(fragment) = model.fragment.as_ref() else {
        return Vec::new();
    };
    let ordered_slots = [
        ModelWheelSlot::FrontLeft,
        ModelWheelSlot::FrontRight,
        ModelWheelSlot::RearLeft,
        ModelWheelSlot::RearRight,
    ];
    let mut wheels = Vec::<WheelConfig>::new();
    for slot in ordered_slots {
        let Some(part) = fragment.parts.iter().find(|part| {
            part.role == ModelFragmentPartRole::Wheel && part.wheel_slot == Some(slot)
        }) else {
            continue;
        };

        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut found_bounds = false;
        for mesh_name in &part.mesh_names {
            let Some(mesh) = model.meshes.iter().find(|mesh| mesh.name == *mesh_name) else {
                continue;
            };
            for axis in 0..3 {
                min[axis] = min[axis].min(mesh.bounds.min[axis]);
                max[axis] = max[axis].max(mesh.bounds.max[axis]);
            }
            found_bounds = true;
        }
        let radius = if found_bounds {
            (((max[1] - min[1]).abs().max((max[2] - min[2]).abs())) * 0.5).clamp(0.15, 1.5)
        } else {
            0.34
        };
        let width = if found_bounds {
            (max[0] - min[0]).abs().clamp(0.06, 0.9)
        } else {
            0.24
        };
        let front = matches!(slot, ModelWheelSlot::FrontLeft | ModelWheelSlot::FrontRight);
        let left = matches!(slot, ModelWheelSlot::FrontLeft | ModelWheelSlot::RearLeft);
        let front_drive = handling.front_drive_weight();
        let driven = if front {
            front_drive > 0.001
        } else {
            (1.0 - front_drive) > 0.001
        };
        let rest_length = (radius * 0.82).clamp(0.18, 0.48);
        wheels.push(WheelConfig {
            name: part.name.clone(),
            mount_local: [
                part.rest_position[0],
                part.rest_position[1] + rest_length,
                part.rest_position[2],
            ],
            radius,
            width,
            rest_length,
            travel_up: handling.suspension_upper_limit.max(0.08).min(0.35),
            travel_down: handling.suspension_lower_limit.max(0.08).min(0.40),
            steered: front,
            driven,
            handbrake: !front,
            front,
            left,
            opposite_index: None,
            grip_multiplier: 1.0,
        });
    }

    for index in 0..wheels.len() {
        let target_front = wheels[index].front;
        let target_left = !wheels[index].left;
        wheels[index].opposite_index = wheels
            .iter()
            .position(|candidate| candidate.front == target_front && candidate.left == target_left);
    }
    wheels
}

fn parse_surface_profile(
    value: &Value,
    index: usize,
    label: &str,
) -> Result<VehicleSurfaceProfile, String> {
    let class = value
        .get("class")
        .cloned()
        .map(serde_json::from_value::<VehicleSurfaceClass>)
        .transpose()
        .map_err(|error| {
            format!(
                "script command[{index}] vehicle.surface_policy.set {label} invalid class: {error}"
            )
        })?
        .unwrap_or(VehicleSurfaceClass::Default);
    let mut profile = VehicleSurfaceProfile::for_class(class);
    macro_rules! coefficient {
        ($field:ident, $key:literal) => {
            if let Some(raw) = value.get($key) {
                profile.$field = raw
                    .as_f64()
                    .map(|value| value as f32)
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| {
                        format!(
                            "script command[{index}] vehicle.surface_policy.set {label}.{} must be finite numeric",
                            $key
                        )
                    })?;
            }
        };
    }
    coefficient!(dry_grip, "dry_grip");
    coefficient!(wet_grip, "wet_grip");
    coefficient!(snow_grip, "snow_grip");
    coefficient!(rolling_resistance, "rolling_resistance");
    coefficient!(fx_response, "fx_response");
    profile.validate().map_err(|error| {
        format!("script command[{index}] vehicle.surface_policy.set {label}: {error}")
    })
}

fn resolve_wheel_index(
    definition: &VehicleDefinition,
    slot: Option<ModelWheelSlot>,
) -> Option<usize> {
    let slot = slot?;
    definition.wheels.iter().position(|wheel| match slot {
        ModelWheelSlot::FrontLeft => wheel.front && wheel.left,
        ModelWheelSlot::FrontRight => wheel.front && !wheel.left,
        ModelWheelSlot::RearLeft => !wheel.front && wheel.left,
        ModelWheelSlot::RearRight => !wheel.front && !wheel.left,
    })
}

fn infer_wheel_slot_from_name(name: &str) -> Option<ModelWheelSlot> {
    let name = name.to_ascii_lowercase();
    if name.contains("_lf") || name.ends_with("lf") {
        Some(ModelWheelSlot::FrontLeft)
    } else if name.contains("_rf") || name.ends_with("rf") {
        Some(ModelWheelSlot::FrontRight)
    } else if name.contains("_lr") || name.ends_with("lr") {
        Some(ModelWheelSlot::RearLeft)
    } else if name.contains("_rr") || name.ends_with("rr") {
        Some(ModelWheelSlot::RearRight)
    } else {
        None
    }
}

fn rotate_euler_xyz(value: [f32; 3], rotation_degrees: [f32; 3]) -> [f32; 3] {
    let [rx, ry, rz] = rotation_degrees.map(f32::to_radians);
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    let after_x = [
        value[0],
        value[1] * cx - value[2] * sx,
        value[1] * sx + value[2] * cx,
    ];
    let after_y = [
        after_x[0] * cy + after_x[2] * sy,
        after_x[1],
        -after_x[0] * sy + after_x[2] * cy,
    ];
    [
        after_y[0] * cz - after_y[1] * sz,
        after_y[0] * sz + after_y[1] * cz,
        after_y[2],
    ]
}

fn parse_vec3(value: &Value, index: usize, label: &str) -> Result<[f32; 3], String> {
    serde_json::from_value::<[f32; 3]>(value.clone())
        .map_err(|error| format!("script command[{index}] {label} must be [x,y,z]: {error}"))
}

fn parse_vec4(value: &Value, index: usize, label: &str) -> Result<[f32; 4], String> {
    serde_json::from_value::<[f32; 4]>(value.clone())
        .map_err(|error| format!("script command[{index}] {label} must be [x,y,z,w]: {error}"))
}

fn optional_number(value: &Value, key: &str, default: f32, index: usize) -> Result<f32, String> {
    match value.get(key) {
        Some(raw) => raw
            .as_f64()
            .map(|number| number as f32)
            .filter(|number| number.is_finite())
            .ok_or_else(|| {
                format!("script command[{index}] vehicle field {key} must be finite numeric")
            }),
        None => Ok(default),
    }
}

#[cfg(test)]
mod vehicle_presentation_tests {
    use super::*;

    #[test]
    fn wheel_slot_name_inference_covers_vehicle_corner_names() {
        assert_eq!(
            infer_wheel_slot_from_name("suspension_lf"),
            Some(ModelWheelSlot::FrontLeft)
        );
        assert_eq!(
            infer_wheel_slot_from_name("hub_rf"),
            Some(ModelWheelSlot::FrontRight)
        );
        assert_eq!(
            infer_wheel_slot_from_name("spring_lr"),
            Some(ModelWheelSlot::RearLeft)
        );
        assert_eq!(
            infer_wheel_slot_from_name("hub_rr"),
            Some(ModelWheelSlot::RearRight)
        );
        assert_eq!(infer_wheel_slot_from_name("engine"), None);
    }

    #[test]
    fn vehicle_local_rotation_round_trips() {
        let value = [1.25, -0.5, 3.0];
        let rotation = [17.0, -43.0, 29.0];
        let rotated = rotate_euler_xyz(value, rotation);
        let restored = inverse_rotate_euler_xyz(rotated, rotation);
        for axis in 0..3 {
            assert!(
                (restored[axis] - value[axis]).abs() < 1.0e-4,
                "axis={axis} restored={restored:?} source={value:?}"
            );
        }
    }
}
