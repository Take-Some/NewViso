use super::*;

mod access;
mod audio_fx;
mod cabin;
mod collision_hull;
pub(super) use collision_hull::fragment_collision_hull as particle_fragment_collision_hull;
mod controls;
mod dashboard;
use dashboard::VehicleDashboardState;
mod damage_presentation;
mod debris;
pub(super) use debris::VehicleDebrisState;
mod doors;
mod events;
mod helpers;
mod lifecycle;
mod particle_interiors;
mod specifications;
mod systems;

use audio_fx::*;
use helpers::*;
use newviso_audio_client::{AudioClient, AudioVoiceUpdateRequest};
use newviso_model::{ModelFragmentPartRole, ModelResource, ModelWheelSlot};
use newviso_scene::{SceneLightDesc, SceneLightType, SceneModelDent, SceneModelPartPose};
use newviso_vehicle::{
    AeroHandling, HandlingData, ReferenceHandlingData, TireCondition, VehicleClass,
    VehicleDamageComponent, VehicleDamageRequest, VehicleDamageType, VehicleEvent,
    VehicleEventKind, VehicleSurfaceClass, VehicleSurfaceProfile, WaterHandling, WheelConfig,
};

#[derive(Clone, Debug)]
pub(super) struct VehiclePresentationBinding {
    model_id: u64,
    model_name: String,
    wreck_fire_until_seconds: f64,
    parts: Vec<VehiclePresentationPart>,
    lights: VehicleLightControls,
    cabin: VehicleCabinControls,
    audio_fx: VehicleAudioFxControls,
    dashboard: VehicleDashboardState,
    occupants: BTreeMap<String, VehicleOccupantBinding>,
    access_layout: VehicleAccessLayoutState,
    seat_reservations: BTreeMap<String, u64>,
    door_reservations: BTreeMap<String, u64>,
    inferred_wheels: Vec<WheelConfig>,
    dents: Vec<SceneModelDent>,
    impact_history: BTreeMap<(u64, u32), (f64, f32)>,
    body_health: f32,
    engine_health: f32,
    last_damage_update_seconds: f64,
    next_part_break_seconds: f64,
    door_motion_runtime: VehicleDoorMotionRuntime,
    original_collision_hulls: Option<Vec<Vec<[f32; 3]>>>,
}

#[derive(Clone, Copy, Debug)]
struct VehicleDoorMotionState {
    target_ratio: f32,
    current_speed: f32,
    latched: bool,
    driven: bool,
    swinging: bool,
    auto_reset: bool,
    just_latched: bool,
    over_limit_seconds: f32,
    break_stress: f32,
    last_audio_seconds: f64,
}

impl Default for VehicleDoorMotionState {
    fn default() -> Self {
        Self {
            target_ratio: 0.0,
            current_speed: 0.0,
            latched: true,
            driven: false,
            swinging: false,
            auto_reset: true,
            just_latched: false,
            over_limit_seconds: 0.0,
            break_stress: 0.0,
            last_audio_seconds: -1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct VehicleDoorMotionRuntime {
    last_update_seconds: f64,
    last_linear_velocity: [f32; 3],
    last_angular_velocity: [f32; 3],
}

impl Default for VehicleDoorMotionRuntime {
    fn default() -> Self {
        Self {
            last_update_seconds: 0.0,
            last_linear_velocity: [0.0; 3],
            last_angular_velocity: [0.0; 3],
        }
    }
}

#[derive(Clone, Debug)]
struct VehiclePresentationPart {
    index: u32,
    name: String,
    name_lower: String,
    rollable_window: bool,
    role: ModelFragmentPartRole,
    parent_part_index: Option<u32>,
    mesh_names: Vec<String>,
    pivot: [f32; 3],
    wheel_slot: Option<ModelWheelSlot>,
    open: f32,
    door_motion: Option<VehicleDoorMotionState>,
    locked: bool,
    visible: bool,
    damage: f32,
    glass_hit_uv: [f32; 2],
    loose: bool,
    detach_velocity_boost: [f32; 3],
    fire_intensity: f32,
    fire_remaining_seconds: f32,
    presentation_override: bool,
    detached_entity: Option<u64>,
}

#[derive(Clone, Debug, Default)]
struct VehicleCabinControls {
    horn: bool,
    high_beam: bool,
    interior_light: bool,
    siren_muted: bool,
    targets: BTreeMap<String, f32>,
    last_update_seconds: f64,
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
    engine_start_clip: Option<String>,
    engine_shutdown_clip: Option<String>,
    engine_breakdown_clip: Option<String>,
    engine_load_clip: Option<String>,
    engine_high_clip: Option<String>,
    exhaust_clip: Option<String>,
    gear_shift_clip: Option<String>,
    road_clip: Option<String>,
    brake_disc_clip: Option<String>,
    brake_release_clip: Option<String>,
    handbrake_clip: Option<String>,
    tyre_puncture_clip: Option<String>,
    tyre_burst_clip: Option<String>,
    flat_tyre_clip: Option<String>,
    wheel_rim_clip: Option<String>,
    suspension_impact_clip: Option<String>,
    door_open_clip: Option<String>,
    door_close_clip: Option<String>,
    door_locked_attempt_clip: Option<String>,
    damage_oneshot_clip: Option<String>,
    tyre_skid_clip: Option<String>,
    steering_scrub_clip: Option<String>,
    brake_chirp_clip: Option<String>,
    suspension_clatter_clip: Option<String>,
    impact_clip: Option<String>,
    impact_heavy_clip: Option<String>,
    glass_break_clip: Option<String>,
    siren_clip: Option<String>,
    horn_clip: Option<String>,
    radio_clip: Option<String>,
    engine_voice: Option<u64>,
    engine_load_voice: Option<u64>,
    engine_high_voice: Option<u64>,
    exhaust_voice: Option<u64>,
    road_voice: Option<u64>,
    brake_disc_voice: Option<u64>,
    flat_tyre_voice: Option<u64>,
    wheel_rim_voice: Option<u64>,
    tyre_voice: Option<u64>,
    steering_voice: Option<u64>,
    siren_voice: Option<u64>,
    horn_voice: Option<u64>,
    radio_voice: Option<u64>,
    engine_gain: f32,
    tyre_gain: f32,
    siren_gain: f32,
    exhaust_effect: Option<String>,
    tyre_effect: Option<String>,
    tyre_puncture_effect: Option<String>,
    tyre_burst_effect: Option<String>,
    engine_smoke_effect: Option<String>,
    engine_fire_effect: Option<String>,
    oil_leak_effect: Option<String>,
    petrol_leak_effect: Option<String>,
    petrol_fire_effect: Option<String>,
    misfire_effect: Option<String>,
    part_fire_effect: Option<String>,
    explosion_effect: Option<String>,
    fallback_catalog: Option<String>,
    damage_effects: BTreeMap<String, String>,
    // Accept legacy FX configuration without letting it delete world geometry.
    debris_lifetime_seconds: f32,
    max_debris: usize,
    damage_emitted: u64,
    next_exhaust_seconds: f64,
    next_tyre_seconds: f64,
    next_rim_scrape_seconds: f64,
    next_damage_fx_seconds: f64,
    audio_retry_after_seconds: f64,
    impact_effect: Option<String>,
    glass_effect: Option<String>,
    next_impact_seconds: f64,
    exhaust_emitted: u64,
    impact_emitted: u64,
    brake_chirp_latched: bool,
    last_suspension_velocity: f32,
    last_audio_seconds: f64,
    next_clatter_seconds: f64,
    next_impact_audio_seconds: f64,
    audio_events_emitted: u64,
}

impl Default for VehicleAudioFxControls {
    fn default() -> Self {
        Self {
            engine_clip: None,
            engine_start_clip: None,
            engine_shutdown_clip: None,
            engine_breakdown_clip: None,
            engine_load_clip: None,
            engine_high_clip: None,
            exhaust_clip: None,
            gear_shift_clip: None,
            road_clip: None,
            brake_disc_clip: None,
            brake_release_clip: None,
            handbrake_clip: None,
            tyre_puncture_clip: None,
            tyre_burst_clip: None,
            flat_tyre_clip: None,
            wheel_rim_clip: None,
            suspension_impact_clip: None,
            door_open_clip: None,
            door_close_clip: None,
            door_locked_attempt_clip: None,
            damage_oneshot_clip: None,
            tyre_skid_clip: None,
            steering_scrub_clip: None,
            brake_chirp_clip: None,
            suspension_clatter_clip: None,
            impact_clip: None,
            impact_heavy_clip: None,
            glass_break_clip: None,
            siren_clip: None,
            horn_clip: None,
            radio_clip: None,
            engine_voice: None,
            engine_load_voice: None,
            engine_high_voice: None,
            exhaust_voice: None,
            road_voice: None,
            brake_disc_voice: None,
            flat_tyre_voice: None,
            wheel_rim_voice: None,
            tyre_voice: None,
            steering_voice: None,
            siren_voice: None,
            horn_voice: None,
            radio_voice: None,
            engine_gain: 0.85,
            tyre_gain: 0.7,
            siren_gain: 0.9,
            exhaust_effect: None,
            tyre_effect: None,
            tyre_puncture_effect: None,
            tyre_burst_effect: None,
            engine_smoke_effect: None,
            engine_fire_effect: None,
            oil_leak_effect: None,
            petrol_leak_effect: None,
            petrol_fire_effect: None,
            misfire_effect: None,
            part_fire_effect: None,
            explosion_effect: None,
            fallback_catalog: None,
            damage_effects: BTreeMap::new(),
            debris_lifetime_seconds: 0.0,
            max_debris: 48,
            damage_emitted: 0,
            next_exhaust_seconds: 0.0,
            next_tyre_seconds: 0.0,
            next_rim_scrape_seconds: 0.0,
            next_damage_fx_seconds: 0.0,
            audio_retry_after_seconds: 0.0,
            impact_effect: None,
            glass_effect: None,
            next_impact_seconds: 0.0,
            exhaust_emitted: 0,
            impact_emitted: 0,
            brake_chirp_latched: false,
            last_suspension_velocity: 0.0,
            last_audio_seconds: 0.0,
            next_clatter_seconds: 0.0,
            next_impact_audio_seconds: 0.0,
            audio_events_emitted: 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct VehicleSeatAccessLinks {
    shuffle: Option<String>,
    rear: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct VehicleAccessLayoutState {
    name: Option<String>,
    driver_seat: Option<String>,
    seats: BTreeMap<String, VehicleSeatAccessLinks>,
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
        self.scene.clear_entity_particle_interior(entity);
        let interior_key = format!("vehicle.{entity}.interior");
        if self.scene.runtime_entity_state(&interior_key).is_some() {
            self.scene.remove_runtime_entity(&interior_key)?;
        }
        let Some(fragment) = model.fragment.as_ref() else {
            if let Some(mut previous) = previous {
                previous.audio_fx.stop_all_voices(&AudioClient::new());
                for part in previous.parts {
                    let key = format!("vehicle.{entity}.light.{}", part.index);
                    if self.scene.runtime_entity_state(&key).is_some() {
                        self.scene.remove_runtime_entity(&key)?;
                    }
                }
            }
            return Ok(false);
        };
        let mut parts = fragment
            .parts
            .iter()
            .map(|part| {
                let old = previous.as_ref().and_then(|binding| {
                    binding.parts.iter().find(|candidate| {
                        candidate.index == part.index
                            || (!part.name.is_empty()
                                && candidate.name.eq_ignore_ascii_case(&part.name))
                    })
                });
                VehiclePresentationPart {
                    index: part.index,
                    name: part.name.clone(),
                    name_lower: part.name.to_ascii_lowercase(),
                    rollable_window: rollable_window(&part.name),
                    role: part.role,
                    parent_part_index: part.parent_part_index,
                    mesh_names: part.mesh_names.clone(),
                    pivot: part.rest_position,
                    wheel_slot: part
                        .wheel_slot
                        .or_else(|| infer_wheel_slot_from_name(&part.name)),
                    open: old.map_or(0.0, |old| old.open),
                    door_motion: old.and_then(|old| old.door_motion).or_else(|| {
                        matches!(
                            part.role,
                            ModelFragmentPartRole::Door
                                | ModelFragmentPartRole::Bonnet
                                | ModelFragmentPartRole::Boot
                        )
                        .then(VehicleDoorMotionState::default)
                    }),
                    locked: old.is_some_and(|old| old.locked),
                    visible: old.is_none_or(|old| old.visible),
                    damage: old.map_or(0.0, |old| old.damage),
                    glass_hit_uv: old.map_or([0.5; 2], |old| old.glass_hit_uv),
                    loose: old.is_some_and(|old| old.loose),
                    detach_velocity_boost: old.map_or([0.0; 3], |old| old.detach_velocity_boost),
                    fire_intensity: old.map_or(0.0, |old| old.fire_intensity),
                    fire_remaining_seconds: old.map_or(0.0, |old| old.fire_remaining_seconds),
                    presentation_override: old.is_some_and(|old| old.presentation_override),
                    detached_entity: old.and_then(|old| old.detached_entity),
                }
            })
            .collect::<Vec<_>>();

        // Compatibility for already-cooked vehicle assets produced before the
        // cooker preserved tyre/rim semantics. The source mesh/material/texture
        // data is already separate; only the fragment role grouping was lost.
        let mut next_part_index = parts
            .iter()
            .map(|part| part.index)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let authored_rim_slots = parts
            .iter()
            .filter(|part| part.role == ModelFragmentPartRole::WheelHub)
            .filter_map(|part| part.wheel_slot)
            .collect::<Vec<_>>();
        let mut recovered_rims = Vec::new();
        for part in parts
            .iter_mut()
            .filter(|part| part.role == ModelFragmentPartRole::Wheel)
        {
            let Some(slot) = part.wheel_slot else {
                continue;
            };
            if authored_rim_slots.contains(&slot) {
                continue;
            }
            let Some((tyre_meshes, rim_meshes)) =
                split_runtime_wheel_meshes(model, &part.mesh_names, part.pivot)
            else {
                continue;
            };

            part.mesh_names = tyre_meshes;
            let mut rim = part.clone();
            rim.index = next_part_index;
            next_part_index = next_part_index.saturating_add(1);
            rim.name = format!("{}_rim", part.name);
            rim.name_lower = rim.name.to_ascii_lowercase();
            rim.role = ModelFragmentPartRole::WheelHub;
            rim.mesh_names = rim_meshes;
            rim.damage = 0.0;
            rim.loose = false;
            rim.presentation_override = false;
            rim.detached_entity = None;
            recovered_rims.push(rim);
        }
        parts.extend(recovered_rims);

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
                    ModelFragmentPartRole::Body
                        | ModelFragmentPartRole::Wheel
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
                wreck_fire_until_seconds: previous
                    .as_ref()
                    .map_or(0.0, |b| b.wreck_fire_until_seconds),
                parts,
                impact_history: previous
                    .as_ref()
                    .map_or_else(BTreeMap::new, |b| b.impact_history.clone()),
                cabin: previous
                    .as_ref()
                    .map_or_else(VehicleCabinControls::default, |b| b.cabin.clone()),
                lights: previous
                    .as_ref()
                    .map_or_else(VehicleLightControls::default, |binding| binding.lights),
                audio_fx: previous.as_ref().map_or_else(
                    || {
                        VehicleAudioFxControls::from_runtime_config(
                            self.settings.variables.get("engine_vehicle"),
                        )
                    },
                    |binding| binding.audio_fx.clone(),
                ),
                dashboard: previous
                    .as_ref()
                    .map_or_else(VehicleDashboardState::default, |b| b.dashboard),
                occupants: previous
                    .as_ref()
                    .map_or_else(BTreeMap::new, |binding| binding.occupants.clone()),
                access_layout: previous
                    .as_ref()
                    .map_or_else(VehicleAccessLayoutState::default, |binding| {
                        binding.access_layout.clone()
                    }),
                seat_reservations: previous
                    .as_ref()
                    .map_or_else(BTreeMap::new, |binding| binding.seat_reservations.clone()),
                door_reservations: previous
                    .as_ref()
                    .map_or_else(BTreeMap::new, |binding| binding.door_reservations.clone()),
                inferred_wheels: inferred_wheels.clone(),
                dents: previous.as_ref().map_or_else(Vec::new, |b| b.dents.clone()),
                body_health: previous.as_ref().map_or(1000.0, |b| b.body_health),
                engine_health: previous.as_ref().map_or(1000.0, |b| b.engine_health),
                last_damage_update_seconds: previous
                    .as_ref()
                    .map_or(self.elapsed_seconds, |b| b.last_damage_update_seconds),
                next_part_break_seconds: previous
                    .as_ref()
                    .map_or(0.0, |b| b.next_part_break_seconds),
                door_motion_runtime: previous.as_ref().map_or_else(
                    || VehicleDoorMotionRuntime {
                        last_update_seconds: self.elapsed_seconds,
                        ..VehicleDoorMotionRuntime::default()
                    },
                    |binding| binding.door_motion_runtime,
                ),
                original_collision_hulls: previous
                    .as_ref()
                    .and_then(|binding| binding.original_collision_hulls.clone())
                    .or_else(|| {
                        self.physics
                            .as_ref()
                            .and_then(|physics| physics.body_collision_hulls(entity))
                    }),
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
        self.apply_vehicle_model_specification(entity, &model.name)?;
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
        self.advance_vehicle_debris()?;
        let entities = self
            .vehicle_presentations
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for entity in entities {
            if !self.vehicles.contains(entity) || !self.scene.entity_model_installed(entity) {
                continue;
            }
            self.sync_vehicle_particle_interior(entity)?;
            if let (Some(state), Some(binding)) = (
                self.vehicles.damage_state(entity),
                self.vehicle_presentations.get_mut(&entity),
            ) {
                binding.body_health = state.body_health;
                binding.engine_health = state.engine_health;
            }
            self.advance_vehicle_part_fires(entity);
            loop {
                let pending = self.vehicle_presentations.get(&entity).and_then(|binding| {
                    binding
                        .parts
                        .iter()
                        .find(|part| {
                            part.damage >= 1.0
                                && part.detached_entity.is_none()
                                && vehicle_part_detachable(part.role)
                        })
                        .map(|part| part.index)
                });
                let Some(part_index) = pending else {
                    break;
                };
                self.detach_vehicle_part(entity, part_index)?;
                if self
                    .vehicle_presentations
                    .get(&entity)
                    .is_some_and(|binding| {
                        binding.parts.iter().any(|part| {
                            part.index == part_index
                                && part.damage >= 1.0
                                && part.detached_entity.is_none()
                        })
                    })
                {
                    break;
                }
            }
            self.advance_vehicle_doors(entity);
            self.advance_vehicle_cabin(entity);
            let Some(telemetry) = self.vehicles.telemetry(entity) else {
                continue;
            };

            let poses = {
                let Some(definition) = self.vehicles.definition(entity) else {
                    continue;
                };
                let Some(binding) = self.vehicle_presentations.get(&entity) else {
                    continue;
                };
                let mut poses = Vec::<SceneModelPartPose>::with_capacity(binding.parts.len());
                for part in &binding.parts {
                    let continuously_articulated = matches!(
                        part.role,
                        ModelFragmentPartRole::Body
                            | ModelFragmentPartRole::Wheel
                            | ModelFragmentPartRole::Suspension
                            | ModelFragmentPartRole::WheelHub
                            | ModelFragmentPartRole::Door
                            | ModelFragmentPartRole::Bonnet
                            | ModelFragmentPartRole::Boot
                            | ModelFragmentPartRole::Steering
                            | ModelFragmentPartRole::BodyPanel
                            | ModelFragmentPartRole::Glass
                            | ModelFragmentPartRole::Breakable
                            | ModelFragmentPartRole::Light
                            | ModelFragmentPartRole::Extra
                            | ModelFragmentPartRole::Spoiler
                            | ModelFragmentPartRole::Roof
                    );
                    if !continuously_articulated && !part.presentation_override {
                        continue;
                    }

                    let mut pose = SceneModelPartPose {
                        mesh_names: part.mesh_names.clone(),
                        pivot: part.pivot,
                        dents: if part.role != ModelFragmentPartRole::Wheel {
                            binding.dents.clone()
                        } else {
                            Vec::new()
                        },
                        glass_damage: (part.role == ModelFragmentPartRole::Glass).then_some([
                            part.glass_hit_uv[0],
                            part.glass_hit_uv[1],
                            part.damage,
                        ]),
                        ..SceneModelPartPose::default()
                    };
                    if matches!(
                        part.role,
                        ModelFragmentPartRole::Glass | ModelFragmentPartRole::Breakable
                    ) {
                        if let Some(parent_index) = part.parent_part_index {
                            if let Some(parent) = binding
                                .parts
                                .iter()
                                .find(|candidate| candidate.index == parent_index)
                            {
                                pose.pivot = parent.pivot;
                                apply_vehicle_parent_articulation(&mut pose, parent);
                            }
                        }
                    }
                    match part.role {
                        ModelFragmentPartRole::Wheel => {
                            if let Some(wheel_index) =
                                resolve_wheel_index(definition, part.wheel_slot)
                            {
                                if let Some(wheel) = telemetry.wheels.get(wheel_index) {
                                    pose.translation[1] = wheel.compression;
                                    pose.rotation_degrees = wheel_part_rotation_degrees(
                                        wheel.rotation_angle,
                                        wheel.steer_angle,
                                    );
                                    if wheel.tire_condition == TireCondition::Punctured {
                                        // The tyre mesh is distinct from the rim. Flatten the
                                        // rubber in vehicle/world vertical after wheel spin so
                                        // the flat patch does not rotate with the tread.
                                        let rubber = wheel.tire_rubber_remaining.clamp(0.0, 1.0);
                                        pose.post_rotation_scale = [1.0, 0.72 + rubber * 0.10, 1.0];
                                    }
                                }
                            }
                        }
                        ModelFragmentPartRole::Suspension | ModelFragmentPartRole::WheelHub => {
                            if let Some(slot) = part.wheel_slot {
                                if let Some(wheel_index) =
                                    resolve_wheel_index(definition, Some(slot))
                                {
                                    if let Some(wheel) = telemetry.wheels.get(wheel_index) {
                                        pose.translation[1] = wheel.compression;
                                        if part.role == ModelFragmentPartRole::WheelHub {
                                            // Rim is its own authored geometry/material and must
                                            // rotate with the wheel at full, unscaled size.
                                            pose.rotation_degrees = wheel_part_rotation_degrees(
                                                wheel.rotation_angle,
                                                wheel.steer_angle,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        ModelFragmentPartRole::Door
                        | ModelFragmentPartRole::Bonnet
                        | ModelFragmentPartRole::Boot => {
                            apply_vehicle_parent_articulation(&mut pose, part);
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
                            if part.loose {
                                pose.rotation_degrees[0] += side * (7.0 + damage * 14.0);
                                pose.translation[1] -= 0.025 + damage * 0.035;
                            }
                        }
                        ModelFragmentPartRole::Breakable
                        | ModelFragmentPartRole::Extra
                        | ModelFragmentPartRole::Spoiler
                        | ModelFragmentPartRole::Roof
                            if part.loose =>
                        {
                            let side = if part.pivot[0] < 0.0 { -1.0 } else { 1.0 };
                            let damage = part.damage.clamp(0.0, 1.0);
                            pose.rotation_degrees[2] += side * (6.0 + damage * 18.0);
                            pose.translation[1] -= 0.02 + damage * 0.04;
                        }
                        ModelFragmentPartRole::Glass if part.rollable_window => {
                            pose.translation[1] -= part.open * 0.45;
                        }
                        _ => {}
                    }
                    let wheel_component_visible = match part.role {
                        ModelFragmentPartRole::Wheel => part
                            .wheel_slot
                            .and_then(|slot| resolve_wheel_index(definition, Some(slot)))
                            .and_then(|index| telemetry.wheels.get(index))
                            .is_none_or(|wheel| {
                                !matches!(
                                    wheel.tire_condition,
                                    TireCondition::Rim | TireCondition::Missing
                                )
                            }),
                        ModelFragmentPartRole::WheelHub => part
                            .wheel_slot
                            .and_then(|slot| resolve_wheel_index(definition, Some(slot)))
                            .and_then(|index| telemetry.wheels.get(index))
                            .is_none_or(|wheel| wheel.tire_condition != TireCondition::Missing),
                        _ => true,
                    };
                    pose.visible = part.visible
                        && wheel_component_visible
                        && part.detached_entity.is_none()
                        && !part.parent_part_index.is_some_and(|index| {
                            binding.parts.iter().any(|parent| {
                                parent.index == index && parent.detached_entity.is_some()
                            })
                        })
                        && !(matches!(
                            part.role,
                            ModelFragmentPartRole::Glass
                                | ModelFragmentPartRole::Breakable
                                | ModelFragmentPartRole::Light
                        ) && part.damage >= 1.0);
                    poses.push(pose);
                }
                poses
            };
            if !poses.is_empty() {
                self.scene.set_entity_model_part_poses(entity, &poses)?;
            }
            self.sync_vehicle_lights(entity, &telemetry)?;
            self.sync_vehicle_dashboard(entity, &telemetry);
            self.sync_vehicle_audio_fx(entity, &telemetry)?;
            self.sync_vehicle_occupants(entity)?;
        }
        self.sync_vehicle_reference_events()?;
        self.sync_vehicle_tracks()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
