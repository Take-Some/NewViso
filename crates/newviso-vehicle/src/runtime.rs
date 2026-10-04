use crate::damage::*;
use crate::events::*;
use crate::math::*;
use crate::specification::*;
use crate::systems::*;
use crate::types::*;
use std::collections::BTreeMap;

mod ignition;
mod powertrain;
mod simulation;
mod systems;

use self::systems::advance_vehicle_systems;
use ignition::*;
use powertrain::*;
use simulation::*;

fn deterministic_unit(mut value: u64) -> f32 {
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51afd7ed558ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ceb9fe1a85ec53);
    value ^= value >> 33;
    ((value >> 40) as u32 as f32) / ((1u32 << 24) as f32)
}

#[derive(Clone, Copy, Debug)]
struct WheelContact {
    position: Vec3,
    distance: f32,
    normal: Vec3,
    surface_entity: Option<u64>,
    surface_id: Option<u32>,
    surface_class: VehicleSurfaceClass,
    grip_multiplier: f32,
}

#[derive(Clone, Debug, Default)]
struct WheelState {
    tire_condition: TireCondition,
    contact: Option<WheelContact>,
    compression: f32,
    old_compression: f32,
    angular_velocity: f32,
    rotation_angle: f32,
    telemetry: WheelTelemetry,
}

#[derive(Clone, Copy, Debug)]
struct TransmissionState {
    gear: i8,
    engine_speed: f32,
    clutch: f32,
    shift_timer: f32,
}

impl Default for TransmissionState {
    fn default() -> Self {
        Self {
            gear: 1,
            engine_speed: 0.0,
            clutch: 1.0,
            shift_timer: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct VehicleEventState {
    brake_hold_seconds: f32,
    handbrake_active: bool,
    skid_active: bool,
    wheel_spin_active: bool,
    suspension_impact_active: bool,
    grounded: bool,
    grounded_initialized: bool,
}

#[derive(Clone, Debug)]
struct VehicleInstance {
    definition: VehicleDefinition,
    input: VehicleInput,
    enabled: bool,
    engine_condition: f32,
    engine_running: bool,
    engine_starting: bool,
    engine_start_will_fail: bool,
    engine_start_remaining: f32,
    failed_engine_start_attempts: u8,
    engine_start_attempt_sequence: u32,
    manual_gear: Option<i8>,
    player_driver: bool,
    alarm: VehicleAlarmState,
    thermal: VehicleThermalState,
    damage_policy: VehicleDamagePolicy,
    transmission: TransmissionState,
    wheels: Vec<WheelState>,
    damage: VehicleDamageState,
    event_state: VehicleEventState,
    speed_mps: f32,
    speed_forward_mps: f32,
}

impl VehicleInstance {
    // Engine operation depends on the powertrain and fuel system. Chassis
    // driveability also includes wheels and body health and is not an ignition
    // or stall condition.
    fn engine_operational(&self) -> bool {
        if self.damage.exploded || self.damage.engine_dead {
            return false;
        }
        if self.definition.class == VehicleClass::Submarine {
            return self.damage.engine_health >= 0.0;
        }
        let engine_limit = if self.player_driver {
            ENGINE_DAMAGE_FINISHED
        } else {
            ENGINE_DAMAGE_ON_FIRE
        };
        let petrol_limit = if self.player_driver {
            PETROL_TANK_FINISHED
        } else {
            PETROL_TANK_ON_FIRE
        };
        self.damage.engine_health > engine_limit && self.damage.petrol_tank_health >= petrol_limit
    }

    fn motor_output(&self) -> f32 {
        if !self.engine_running
            || !self.enabled
            || self.damage.engine_dead
            || !self.engine_operational()
        {
            return 0.0;
        }
        // Negative burning health is a fire progression clock, not zero motor
        // torque. The already-running player engine shuts down at FIRE_FINISH.
        let condition = if self.damage.engine_on_fire {
            1.0
        } else {
            self.engine_condition
        };
        condition * self.damage.engine_output_multiplier()
    }

    fn powertrain_state(&self) -> VehiclePowertrainState {
        if self.damage.exploded {
            VehiclePowertrainState::Wrecked
        } else if !self
            .definition
            .specification
            .has_engine(self.definition.class)
        {
            VehiclePowertrainState::Unpowered
        } else if !self.enabled || self.damage.engine_dead || !self.engine_operational() {
            VehiclePowertrainState::Disabled
        } else if !self.damage.has_fuel() {
            VehiclePowertrainState::FuelStarved
        } else if self.engine_starting {
            VehiclePowertrainState::Starting
        } else if self.engine_running {
            VehiclePowertrainState::Running
        } else {
            VehiclePowertrainState::Off
        }
    }
}

fn wheel_telemetry_from_state(
    wheel: &WheelState,
    damage: Option<&WheelDamageState>,
) -> WheelTelemetry {
    let mut telemetry = wheel.telemetry;
    let rubber = damage
        .map(|damage| damage.tyre_rubber_remaining)
        .unwrap_or_else(|| {
            if wheel.tire_condition == TireCondition::Intact {
                1.0
            } else {
                0.0
            }
        });
    telemetry.tire_condition = wheel.tire_condition;
    telemetry.tire_rubber_remaining = rubber;
    telemetry.tire_grip_multiplier = wheel.tire_condition.grip_multiplier_with_rubber(rubber);
    telemetry
}

fn build_vehicle_telemetry(entity: VehicleEntity, vehicle: &VehicleInstance) -> VehicleTelemetry {
    VehicleTelemetry {
        entity,
        specification: vehicle.definition.specification.clone(),
        powertrain_state: vehicle.powertrain_state(),
        driveable_player: vehicle.damage.driveable_player,
        driveable_ai: vehicle.damage.driveable_ai,
        engine_output_multiplier: vehicle.motor_output(),
        class: vehicle.definition.class,
        speed_mps: vehicle.speed_mps,
        speed_forward_mps: vehicle.speed_forward_mps,
        gear: vehicle.transmission.gear,
        engine_speed: vehicle.transmission.engine_speed,
        engine_running: vehicle.engine_running,
        engine_starting: vehicle.engine_starting,
        engine_start_remaining: vehicle.engine_start_remaining,
        failed_engine_start_attempts: vehicle.failed_engine_start_attempts,
        engine_condition: vehicle.engine_condition,
        engine_smoke_level: vehicle.damage.engine_damage_evolution(),
        engine_fire_level: vehicle.damage.engine_fire_evolution(),
        engine_misfiring: vehicle.damage.engine_misfiring(),
        petrol_leak_level: vehicle.damage.petrol_leak_evolution(),
        petrol_fire_level: vehicle.damage.petrol_fire_evolution(),
        petrol_tank_level: vehicle.damage.petrol_tank_level,
        petrol_tank_capacity: vehicle.damage.petrol_tank_capacity,
        fuel_fraction: if vehicle.damage.petrol_tank_capacity > 0.0 {
            (vehicle.damage.petrol_tank_level / vehicle.damage.petrol_tank_capacity).clamp(0.0, 1.0)
        } else {
            1.0
        },
        oil_level: vehicle.damage.oil_level,
        oil_capacity: vehicle.damage.oil_capacity,
        exploded: vehicle.damage.exploded,
        manual_gear: vehicle.manual_gear,
        enabled: vehicle.enabled,
        player_driver: vehicle.player_driver,
        alarm: vehicle.alarm,
        thermal: vehicle.thermal,
        damage_policy: vehicle.damage_policy,
        clutch: vehicle.transmission.clutch,
        input: vehicle.input,
        wheels: vehicle
            .wheels
            .iter()
            .enumerate()
            .map(|(index, wheel)| {
                wheel_telemetry_from_state(wheel, vehicle.damage.wheels.get(index))
            })
            .collect(),
    }
}

#[derive(Clone, Copy, Debug)]
struct ProbeRoute {
    vehicle: VehicleEntity,
    wheel_index: usize,
    direction: Vec3,
}

#[derive(Clone, Debug)]
pub struct VehicleRuntime {
    vehicles: BTreeMap<VehicleEntity, VehicleInstance>,
    pending_probe_routes: BTreeMap<u64, ProbeRoute>,
    next_probe_seq: u64,
    surface_profiles: BTreeMap<u32, VehicleSurfaceProfile>,
    default_surface_profile: VehicleSurfaceProfile,
    surface_wetness: f32,
    surface_snow: f32,
    consume_petrol: bool,
    ambient_temperature: f32,
    events: Vec<VehicleEvent>,
    next_event_seq: u64,
}

impl Default for VehicleRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl VehicleRuntime {
    pub fn new() -> Self {
        Self {
            vehicles: BTreeMap::new(),
            pending_probe_routes: BTreeMap::new(),
            next_probe_seq: 0x5645_4800_0000_0001,
            surface_profiles: BTreeMap::new(),
            default_surface_profile: VehicleSurfaceProfile::default(),
            surface_wetness: 0.0,
            surface_snow: 0.0,
            consume_petrol: false,
            ambient_temperature: 20.0,
            events: Vec::new(),
            next_event_seq: 0x5645_5645_0000_0001,
        }
    }

    pub fn upsert(
        &mut self,
        entity: VehicleEntity,
        definition: VehicleDefinition,
    ) -> Result<(), String> {
        definition.validate()?;
        let previous_specification = self
            .vehicles
            .get(&entity)
            .map(|v| v.definition.specification.clone());
        let previous_status = self.vehicles.get(&entity).map(|v| v.damage.status());
        let wheel_count = definition.wheels.len();
        let petrol_tank_volume = if definition.specification.has_petrol_tank(definition.class) {
            definition.handling.petrol_tank_volume
        } else {
            0.0
        };
        let oil_volume = if definition.specification.uses_combustion(definition.class) {
            definition.handling.oil_volume
        } else {
            0.0
        };
        let has_cooling_fan = matches!(definition.class, VehicleClass::Automobile);
        let created = !self.vehicles.contains_key(&entity);
        match self.vehicles.get_mut(&entity) {
            Some(vehicle) => {
                // Preserve damage by wheel identity, including when a layout is reordered.
                let old: BTreeMap<_, _> = vehicle
                    .definition
                    .wheels
                    .iter()
                    .zip(&vehicle.wheels)
                    .map(|(config, state)| (config.name.clone(), state.clone()))
                    .collect();
                let old_damage: BTreeMap<_, _> = vehicle
                    .definition
                    .wheels
                    .iter()
                    .zip(&vehicle.damage.wheels)
                    .map(|(config, state)| (config.name.clone(), *state))
                    .collect();
                vehicle.wheels = definition
                    .wheels
                    .iter()
                    .map(|config| old.get(&config.name).cloned().unwrap_or_default())
                    .collect();
                vehicle.damage.wheels = definition
                    .wheels
                    .iter()
                    .map(|config| old_damage.get(&config.name).copied().unwrap_or_default())
                    .collect();
                let fuel_fraction = if vehicle.damage.petrol_tank_capacity > 0.0 {
                    vehicle.damage.petrol_tank_level / vehicle.damage.petrol_tank_capacity
                } else {
                    0.0
                };
                let oil_fraction = if vehicle.damage.oil_capacity > 0.0 {
                    vehicle.damage.oil_level / vehicle.damage.oil_capacity
                } else {
                    0.0
                };
                vehicle.damage.petrol_tank_capacity = petrol_tank_volume;
                vehicle.damage.petrol_tank_level =
                    petrol_tank_volume * fuel_fraction.clamp(0.0, 1.0);
                vehicle.damage.oil_capacity = oil_volume;
                vehicle.damage.oil_level = oil_volume * oil_fraction.clamp(0.0, 1.0);
                vehicle.definition = definition;
            }
            None => {
                self.vehicles.insert(
                    entity,
                    VehicleInstance {
                        definition,
                        input: VehicleInput::default(),
                        enabled: true,
                        engine_condition: 1.0,
                        engine_running: false,
                        engine_starting: false,
                        engine_start_will_fail: false,
                        engine_start_remaining: 0.0,
                        failed_engine_start_attempts: 0,
                        engine_start_attempt_sequence: 0,
                        manual_gear: None,
                        player_driver: false,
                        alarm: VehicleAlarmState::default(),
                        thermal: VehicleThermalState {
                            has_cooling_fan: has_cooling_fan,
                            ..VehicleThermalState::default()
                        },
                        damage_policy: VehicleDamagePolicy::default(),
                        transmission: TransmissionState::default(),
                        wheels: vec![WheelState::default(); wheel_count],
                        damage: VehicleDamageState::new_with_volumes(
                            wheel_count,
                            petrol_tank_volume,
                            oil_volume,
                        ),
                        event_state: VehicleEventState::default(),
                        speed_mps: 0.0,
                        speed_forward_mps: 0.0,
                    },
                );
            }
        }
        let vehicle = self.vehicles.get_mut(&entity).expect("upserted vehicle");
        vehicle.input = vehicle
            .definition
            .specification
            .controls
            .constrain(vehicle.input);
        if !vehicle.definition.specification.controls.reverse && vehicle.manual_gear == Some(-1) {
            vehicle.manual_gear = None;
        }
        reconcile_damage_features(
            &mut vehicle.damage,
            vehicle.definition.class,
            &vehicle.definition.specification,
            vehicle.player_driver,
        );
        let stopped = vehicle.engine_running
            && (!vehicle
                .definition
                .specification
                .has_engine(vehicle.definition.class)
                || !vehicle.engine_operational());
        if stopped {
            vehicle.engine_running = false;
            vehicle.engine_starting = false;
            vehicle.engine_start_remaining = 0.0;
            vehicle.transmission.engine_speed = 0.0;
        }
        let status = vehicle.damage.status();
        let specification = vehicle.definition.specification.clone();
        if let Some(before) = previous_status {
            for signal in damage_transition_signals(before, status) {
                let mut event = self.next_event(entity, signal.kind);
                event.details = serde_json::json!({"before": before, "after": status});
                self.events.push(event);
            }
        }
        if previous_specification
            .as_ref()
            .is_some_and(|old| old != &specification)
        {
            let mut event = self.next_event(entity, VehicleEventKind::SpecificationChanged);
            event.details =
                serde_json::json!({"before": previous_specification, "after": specification});
            self.events.push(event);
        }
        if stopped {
            let event = self.next_event(entity, VehicleEventKind::EngineStopped);
            self.events.push(event);
        }
        if created {
            let event = self.next_event(entity, VehicleEventKind::VehicleCreated);
            self.events.push(event);
        }
        Ok(())
    }

    pub fn remove(&mut self, entity: VehicleEntity) -> bool {
        self.pending_probe_routes
            .retain(|_, route| route.vehicle != entity);
        let removed = self.vehicles.remove(&entity).is_some();
        if removed {
            let event = self.next_event(entity, VehicleEventKind::VehicleRemoved);
            self.events.push(event);
        }
        removed
    }

    pub fn contains(&self, entity: VehicleEntity) -> bool {
        self.vehicles.contains_key(&entity)
    }

    fn next_event(&mut self, entity: VehicleEntity, kind: VehicleEventKind) -> VehicleEvent {
        let sequence = self.next_event_seq;
        self.next_event_seq = self.next_event_seq.wrapping_add(1).max(1);
        VehicleEvent::new(sequence, entity, kind)
    }

    pub fn emit_event(&mut self, mut event: VehicleEvent) {
        if event.sequence == 0 {
            event.sequence = self.next_event_seq;
            self.next_event_seq = self.next_event_seq.wrapping_add(1).max(1);
        }
        self.events.push(event);
    }

    pub fn drain_events(&mut self) -> Vec<VehicleEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn damage_state(&self, entity: VehicleEntity) -> Option<&VehicleDamageState> {
        self.vehicles.get(&entity).map(|vehicle| &vehicle.damage)
    }

    pub fn set_glass_damage(
        &mut self,
        entity: VehicleEntity,
        part_index: u32,
        damage: f32,
    ) -> Result<(), String> {
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
        let glass = vehicle.damage.glass.entry(part_index).or_default();
        glass.damage = damage.clamp(0.0, 1.0);
        glass.broken = glass.damage >= 1.0;
        if glass.damage == 0.0 {
            glass.crack_points.clear();
        }
        Ok(())
    }

    /// Damage nearby panes without applying the same collision twice to body health.
    pub fn damage_glass(
        &mut self,
        entity: VehicleEntity,
        part_index: u32,
        damage: f32,
        kind: VehicleDamageType,
        laminated: bool,
        point: Vec3,
        normal: Vec3,
    ) -> Result<Option<VehicleEventKind>, String> {
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
        if vehicle.damage_policy.protects(kind) {
            return Ok(None);
        }
        let damage_type = kind;
        let glass = vehicle.damage.glass.entry(part_index).or_default();
        let transition = glass.apply_hit(damage, kind, laminated, point);
        if let Some(kind) = transition {
            let mut event = self.next_event(entity, kind);
            event.part_index = Some(part_index);
            event.position = Some(point);
            event.normal = Some(normal);
            event.local_space = true;
            event.damage_type = Some(damage_type);
            event.magnitude = damage;
            self.emit_event(event);
        }
        Ok(transition)
    }

    pub fn apply_damage(
        &mut self,
        entity: VehicleEntity,
        request: VehicleDamageRequest,
    ) -> Result<VehicleDamageOutcome, String> {
        if request
            .local_position
            .iter()
            .chain(&request.local_normal)
            .chain(&request.local_direction)
            .any(|v| !v.is_finite())
            || !request.speed_mps.is_finite()
            || !request.contact_impulse.is_finite()
        {
            return Err("vehicle damage vectors and impulse must be finite".into());
        }
        let sample = deterministic_unit(self.next_event_seq ^ entity.rotate_left(17));
        let (outcome, committed) = {
            let vehicle = self
                .vehicles
                .get_mut(&entity)
                .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
            if vehicle.damage_policy.protects(request.damage_type) || vehicle.damage.exploded {
                return Ok(VehicleDamageOutcome::default());
            }
            let was_running = vehicle.engine_running;
            let before = vehicle.damage.status();
            let mut outcome = apply_damage_with_specification(
                &mut vehicle.damage,
                &vehicle.definition.handling,
                vehicle.definition.class,
                &vehicle.definition.specification,
                vehicle.player_driver,
                request,
                sample,
            );
            vehicle.engine_condition = vehicle.damage.engine_condition();
            if !vehicle.engine_operational() || vehicle.damage.engine_dead {
                vehicle.engine_running = false;
                vehicle.engine_starting = false;
                vehicle.engine_start_will_fail = false;
                vehicle.engine_start_remaining = 0.0;
                vehicle.transmission.engine_speed = 0.0;
            }
            if was_running && !vehicle.engine_running {
                outcome.signals.push(VehicleDamageSignal::new(
                    VehicleEventKind::EngineStopped,
                    1.0,
                ));
            }
            for (index, wheel_damage) in vehicle.damage.wheels.iter().enumerate() {
                if let Some(wheel) = vehicle.wheels.get_mut(index) {
                    wheel.tire_condition = wheel_damage.tyre_condition;
                    wheel.telemetry.tire_condition = wheel_damage.tyre_condition;
                    wheel.telemetry.tire_rubber_remaining = wheel_damage.tyre_rubber_remaining;
                    if let Some(config) = vehicle.definition.wheels.get(index) {
                        wheel.telemetry.effective_radius = config.radius
                            * wheel_damage
                                .tyre_condition
                                .radius_multiplier_with_rubber(wheel_damage.tyre_rubber_remaining);
                        wheel.telemetry.tire_grip_multiplier = wheel_damage
                            .tyre_condition
                            .grip_multiplier_with_rubber(wheel_damage.tyre_rubber_remaining);
                    }
                    if wheel_damage.tyre_condition == TireCondition::Missing {
                        wheel.contact = None;
                        wheel.compression = 0.0;
                        wheel.angular_velocity = 0.0;
                    }
                }
            }
            let committed = if outcome.effective_damage > 0.0 {
                let mut event = VehicleEvent::new(0, entity, VehicleEventKind::DamageApplied);
                event.other_entity = request.source_entity;
                event.damage_type = Some(request.damage_type);
                event.part_index = request.part_index;
                event.position = Some(request.local_position);
                event.normal = Some(request.local_normal);
                event.local_space = true;
                event.magnitude = outcome.effective_damage;
                event.details = serde_json::json!({"before": before, "after": vehicle.damage.status(), "raw_damage": request.raw_damage});
                Some(event)
            } else {
                None
            };
            (outcome, committed)
        };

        for signal in &outcome.signals {
            let mut event = self.next_event(entity, signal.kind);
            event.wheel_index = signal.wheel_index;
            event.other_entity = request.source_entity;
            event.part_index = request.part_index;
            event.position = Some(request.local_position);
            event.normal = Some(request.local_normal);
            event.local_space = true;
            event.damage_type = Some(request.damage_type);
            event.magnitude = signal.magnitude;
            event.speed_mps = request.speed_mps;
            if let Some(ref committed) = committed {
                event.details = committed.details.clone();
            }
            self.events.push(event);
        }
        if let Some(event) = committed {
            self.emit_event(event);
        }
        if outcome.effective_damage > 0.0 {
            self.trigger_alarm(entity)?;
        }
        Ok(outcome)
    }

    pub fn repair_damage(&mut self, entity: VehicleEntity) -> Result<(), String> {
        let transitions = {
            let vehicle = self
                .vehicles
                .get_mut(&entity)
                .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
            let before = vehicle.damage.status();
            vehicle.damage.repair();
            reconcile_damage_features(
                &mut vehicle.damage,
                vehicle.definition.class,
                &vehicle.definition.specification,
                vehicle.player_driver,
            );
            vehicle.engine_starting = false;
            vehicle.engine_start_will_fail = false;
            vehicle.engine_start_remaining = 0.0;
            vehicle.failed_engine_start_attempts = 0;
            vehicle.engine_condition = 1.0;
            for (index, wheel) in vehicle.wheels.iter_mut().enumerate() {
                wheel.tire_condition = TireCondition::Intact;
                wheel.telemetry.tire_condition = TireCondition::Intact;
                wheel.telemetry.tire_rubber_remaining = 1.0;
                wheel.telemetry.tire_grip_multiplier = 1.0;
                wheel.telemetry.effective_radius = vehicle.definition.wheels[index].radius;
            }
            damage_transition_signals(before, vehicle.damage.status())
        };
        for signal in transitions {
            let event = self.next_event(entity, signal.kind);
            self.events.push(event);
        }
        let event = self.next_event(entity, VehicleEventKind::VehicleRepaired);
        self.events.push(event);
        Ok(())
    }

    pub fn set_enabled(&mut self, entity: VehicleEntity, enabled: bool) -> Result<(), String> {
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
        vehicle.enabled = enabled;
        if !enabled {
            vehicle.speed_mps = 0.0;
            vehicle.speed_forward_mps = 0.0;
            vehicle.engine_running = false;
            vehicle.engine_starting = false;
            vehicle.engine_start_will_fail = false;
            vehicle.engine_start_remaining = 0.0;
            vehicle.transmission.engine_speed = 0.0;
            for wheel in &mut vehicle.wheels {
                wheel.angular_velocity = 0.0;
                wheel.telemetry.angular_velocity = 0.0;
            }
        }
        Ok(())
    }

    /// Ignition never disables suspension, tyre contacts or passive motion.
    pub fn set_engine_running(
        &mut self,
        entity: VehicleEntity,
        running: bool,
    ) -> Result<(), String> {
        let (changed, now_running) = {
            let vehicle = self
                .vehicles
                .get_mut(&entity)
                .ok_or_else(|| format!("unknown vehicle {entity}"))?;
            let old_running = vehicle.engine_running;
            let now_running = running
                && vehicle
                    .definition
                    .specification
                    .has_engine(vehicle.definition.class)
                && vehicle.engine_operational()
                && !vehicle.damage.engine_dead
                && vehicle.damage.engine_health > ENGINE_DAMAGE_ON_FIRE
                && vehicle.damage.has_fuel();
            vehicle.engine_running = now_running;
            vehicle.engine_starting = false;
            vehicle.engine_start_will_fail = false;
            vehicle.engine_start_remaining = 0.0;
            vehicle.failed_engine_start_attempts = 0;
            if !now_running {
                vehicle.transmission.engine_speed = 0.0;
            }
            (old_running != now_running, now_running)
        };
        if changed {
            let event = self.next_event(
                entity,
                if now_running {
                    VehicleEventKind::EngineStarted
                } else {
                    VehicleEventKind::EngineStopped
                },
            );
            self.events.push(event);
        }
        Ok(())
    }

    pub fn set_manual_gear(
        &mut self,
        entity: VehicleEntity,
        gear: Option<i8>,
    ) -> Result<(), String> {
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle {entity}"))?;
        if !vehicle.definition.class.uses_wheel_probes() {
            return Err("this vehicle has no wheel gearbox".into());
        }
        if gear.is_some_and(|g| g < -1 || g > vehicle.definition.handling.initial_drive_gears as i8)
        {
            return Err("gear exceeds the authored gearbox range".into());
        }
        if gear == Some(-1) && !vehicle.definition.specification.controls.reverse {
            return Err("vehicle specification disables reverse gear".into());
        }
        vehicle.manual_gear = gear;
        Ok(())
    }

    pub fn set_engine_condition(
        &mut self,
        entity: VehicleEntity,
        condition: f32,
    ) -> Result<(), String> {
        if !condition.is_finite() {
            return Err("engine condition must be finite".to_owned());
        }
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
        vehicle.engine_condition = condition.clamp(0.0, 1.0);
        Ok(())
    }

    pub fn apply_suspension_damage(
        &mut self,
        entity: VehicleEntity,
        index: usize,
        damage: f32,
    ) -> Result<bool, String> {
        if !damage.is_finite() || damage <= 0.0 {
            return Ok(false);
        }
        let detached = {
            let vehicle = self
                .vehicles
                .get_mut(&entity)
                .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
            if vehicle.definition.class == VehicleClass::Bike
                || vehicle.definition.specification.damage.indestructible
            {
                return Ok(false);
            }
            let wheel_damage = vehicle
                .damage
                .wheels
                .get_mut(index)
                .ok_or_else(|| format!("unknown wheel index {index}"))?;
            if wheel_damage.tyre_condition == TireCondition::Missing {
                return Ok(false);
            }
            wheel_damage.suspension_health = (wheel_damage.suspension_health - damage).max(0.0);
            if wheel_damage.suspension_health <= 0.0
                && vehicle.definition.specification.damage.wheels_can_break
            {
                wheel_damage.tyre_condition = TireCondition::Missing;
                wheel_damage.tyre_health = 0.0;
                wheel_damage.tyre_rubber_remaining = 0.0;
                if let Some(wheel) = vehicle.wheels.get_mut(index) {
                    wheel.tire_condition = TireCondition::Missing;
                    wheel.contact = None;
                    wheel.compression = 0.0;
                    wheel.angular_velocity = 0.0;
                    wheel.telemetry = WheelTelemetry {
                        tire_condition: TireCondition::Missing,
                        ..WheelTelemetry::default()
                    };
                }
                true
            } else {
                false
            }
        };
        if detached {
            let mut event = self.next_event(entity, VehicleEventKind::WheelDetached);
            event.wheel_index = Some(index);
            event.magnitude = damage;
            self.events.push(event);
        }
        Ok(detached)
    }

    pub fn set_tire_condition(
        &mut self,
        entity: VehicleEntity,
        index: usize,
        condition: TireCondition,
    ) -> Result<(), String> {
        let previous = {
            let vehicle = self
                .vehicles
                .get_mut(&entity)
                .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
            let config = vehicle
                .definition
                .wheels
                .get(index)
                .ok_or_else(|| format!("unknown wheel index {index}"))?;
            let wheel = &mut vehicle.wheels[index];
            let previous = wheel.tire_condition;
            wheel.tire_condition = condition;
            let rubber_remaining = match condition {
                TireCondition::Intact | TireCondition::Punctured => 1.0,
                TireCondition::Rim | TireCondition::Missing => 0.0,
            };
            wheel.telemetry.tire_condition = condition;
            wheel.telemetry.tire_rubber_remaining = rubber_remaining;
            wheel.telemetry.effective_radius =
                config.radius * condition.radius_multiplier_with_rubber(rubber_remaining);
            wheel.telemetry.tire_grip_multiplier =
                condition.grip_multiplier_with_rubber(rubber_remaining);
            if let Some(damage) = vehicle.damage.wheels.get_mut(index) {
                damage.tyre_condition = condition;
                damage.tyre_rubber_remaining = rubber_remaining;
                damage.tyre_health = match condition {
                    TireCondition::Intact => TYRE_HEALTH_MAX,
                    TireCondition::Punctured => TYRE_HEALTH_FLAT + TYRE_HEALTH_FLAT_ADD,
                    TireCondition::Rim | TireCondition::Missing => 0.0,
                };
            }
            if condition == TireCondition::Missing {
                wheel.contact = None;
                wheel.compression = 0.0;
                wheel.angular_velocity = 0.0;
                wheel.telemetry = WheelTelemetry {
                    tire_condition: condition,
                    ..WheelTelemetry::default()
                };
                self.pending_probe_routes
                    .retain(|_, route| route.vehicle != entity || route.wheel_index != index);
            }
            previous
        };

        if previous != condition {
            let kind = match condition {
                TireCondition::Punctured => Some(VehicleEventKind::TyrePunctured),
                TireCondition::Rim => Some(VehicleEventKind::TyreBurst),
                TireCondition::Missing => Some(VehicleEventKind::WheelDetached),
                TireCondition::Intact => Some(VehicleEventKind::VehicleRepaired),
            };
            if let Some(kind) = kind {
                let mut event = self.next_event(entity, kind);
                event.wheel_index = Some(index);
                self.events.push(event);
            }
        }
        Ok(())
    }

    pub fn repair_tires(&mut self, entity: VehicleEntity) -> Result<(), String> {
        let count = self
            .definition(entity)
            .ok_or_else(|| format!("unknown vehicle entity {entity}"))?
            .wheels
            .len();
        for index in 0..count {
            self.set_tire_condition(entity, index, TireCondition::Intact)?;
        }
        Ok(())
    }

    pub fn set_input(&mut self, entity: VehicleEntity, input: VehicleInput) -> Result<(), String> {
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
        vehicle.input = vehicle.definition.specification.controls.constrain(input);

        // GTA reads VehicleAccelerate directly here because a stopped engine can
        // leave the transmission throttle at zero. Do the same at the runtime seam.
        if vehicle.input.throttle > ENGINE_START_THROTTLE_THRESHOLD {
            let _ = begin_engine_start_attempt(entity, vehicle);
        }
        Ok(())
    }

    pub fn set_consume_petrol(&mut self, enabled: bool) {
        self.consume_petrol = enabled;
    }

    pub fn consume_petrol(&self) -> bool {
        self.consume_petrol
    }

    pub fn set_petrol_consumption_rate(
        &mut self,
        entity: VehicleEntity,
        rate: f32,
    ) -> Result<(), String> {
        if !rate.is_finite() || rate < 0.0 {
            return Err(
                "vehicle petrol consumption rate must be finite and non-negative".to_owned(),
            );
        }
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
        vehicle.definition.handling.petrol_consumption_rate = rate;
        Ok(())
    }

    pub fn set_petrol_tank_level(
        &mut self,
        entity: VehicleEntity,
        level: f32,
    ) -> Result<(), String> {
        if !level.is_finite() || level < 0.0 {
            return Err("vehicle petrol tank level must be finite and non-negative".to_owned());
        }
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
        if vehicle.damage.petrol_tank_capacity <= 0.0 {
            vehicle.damage.petrol_tank_level = 0.0;
        } else {
            vehicle.damage.petrol_tank_level =
                level.clamp(0.0, vehicle.damage.petrol_tank_capacity);
        }
        Ok(())
    }

    pub fn set_petrol_tank_health(
        &mut self,
        entity: VehicleEntity,
        health: f32,
    ) -> Result<(), String> {
        self.set_health(
            entity,
            VehicleHealthUpdate {
                petrol_tank_health: Some(health),
                ..Default::default()
            },
        )
    }

    pub fn petrol_stats(&self, entity: VehicleEntity) -> Option<(f32, f32, f32)> {
        let vehicle = self.vehicles.get(&entity)?;
        Some((
            vehicle.damage.petrol_tank_capacity,
            vehicle.damage.petrol_tank_level,
            vehicle.definition.handling.petrol_consumption_rate,
        ))
    }

    pub fn set_surface_profile(
        &mut self,
        surface_id: u32,
        profile: VehicleSurfaceProfile,
    ) -> Result<(), String> {
        self.surface_profiles
            .insert(surface_id, profile.validate()?);
        Ok(())
    }

    pub fn remove_surface_profile(&mut self, surface_id: u32) -> bool {
        self.surface_profiles.remove(&surface_id).is_some()
    }

    pub fn clear_surface_profiles(&mut self) {
        self.surface_profiles.clear();
    }

    pub fn set_default_surface_profile(
        &mut self,
        profile: VehicleSurfaceProfile,
    ) -> Result<(), String> {
        self.default_surface_profile = profile.validate()?;
        Ok(())
    }

    pub fn set_surface_weather(&mut self, wetness: f32, snow: f32) -> Result<(), String> {
        if !wetness.is_finite() || !snow.is_finite() {
            return Err("vehicle surface weather contains non-finite values".to_owned());
        }
        self.surface_wetness = wetness.clamp(0.0, 1.0);
        self.surface_snow = snow.clamp(0.0, 1.0);
        Ok(())
    }

    pub fn surface_profile(&self, surface_id: Option<u32>) -> VehicleSurfaceProfile {
        surface_id
            .and_then(|id| self.surface_profiles.get(&id).copied())
            .unwrap_or(self.default_surface_profile)
    }

    pub fn entity_ids(&self) -> impl Iterator<Item = VehicleEntity> + '_ {
        self.vehicles.keys().copied()
    }

    pub fn definition(&self, entity: VehicleEntity) -> Option<&VehicleDefinition> {
        self.vehicles
            .get(&entity)
            .map(|vehicle| &vehicle.definition)
    }

    pub fn prepare_frame(
        &mut self,
        dt: f32,
        gravity: f32,
        bodies: &BTreeMap<VehicleEntity, VehicleBodyState>,
    ) -> VehicleFramePlan {
        if !dt.is_finite() || dt <= 0.0 || !gravity.is_finite() {
            return VehicleFramePlan::default();
        }
        let dt = dt.min(0.1);
        let gravity = gravity.abs().max(0.01);
        let consume_petrol = self.consume_petrol;
        let vehicle_count = self.vehicles.len();
        let wheel_count = self
            .vehicles
            .values()
            .map(|vehicle| vehicle.wheels.len())
            .sum::<usize>();
        let mut plan = VehicleFramePlan {
            probes: Vec::with_capacity(wheel_count),
            impulses: Vec::with_capacity(wheel_count.saturating_mul(2) + vehicle_count),
            angular_velocity_deltas: Vec::with_capacity(vehicle_count),
        };
        let mut fresh_routes = std::mem::take(&mut self.pending_probe_routes);
        fresh_routes.clear();
        let mut frame_events = Vec::<VehicleEvent>::with_capacity(vehicle_count.saturating_mul(2));

        for (&entity, vehicle) in &mut self.vehicles {
            if !vehicle.enabled {
                continue;
            }
            let Some(body) = bodies.get(&entity).copied() else {
                continue;
            };

            let forward = normalize_or(
                rotate_vec(body.rotation, vehicle.definition.forward_local),
                [0.0, 0.0, -1.0],
            );
            let up = normalize_or(
                rotate_vec(body.rotation, vehicle.definition.up_local),
                WORLD_UP,
            );
            let right = normalize_or(cross(forward, up), [1.0, 0.0, 0.0]);

            vehicle.speed_mps = length(body.linear_velocity);
            vehicle.speed_forward_mps = dot(body.linear_velocity, forward);

            // Holding forward after a stall is also sufficient to request another
            // ignition cycle; no key release/re-press is required.
            if vehicle.input.throttle > ENGINE_START_THROTTLE_THRESHOLD
                && !vehicle.engine_running
                && !vehicle.engine_starting
            {
                let _ = begin_engine_start_attempt(entity, vehicle);
            }
            advance_engine_start_attempt(entity, vehicle, dt, &mut frame_events);

            let old_gear = vehicle.transmission.gear;
            let drive_wheels_loaded =
                vehicle
                    .definition
                    .wheels
                    .iter()
                    .zip(&vehicle.wheels)
                    .any(|(config, wheel)| {
                        config.driven
                            && wheel.tire_condition != TireCondition::Missing
                            && wheel.contact.is_some()
                    });
            update_transmission(
                &vehicle.definition.handling,
                vehicle.input,
                vehicle.manual_gear,
                vehicle.speed_forward_mps,
                drive_wheels_loaded,
                dt,
                &mut vehicle.transmission,
            );
            if old_gear != vehicle.transmission.gear {
                let mut event = VehicleEvent::new(0, entity, VehicleEventKind::GearShifted);
                event.old_gear = Some(old_gear);
                event.new_gear = Some(vehicle.transmission.gear);
                event.speed_mps = vehicle.speed_mps;
                frame_events.push(event);
            }

            if !vehicle.engine_running || vehicle.motor_output() <= 0.0 {
                vehicle.transmission.engine_speed = 0.0;
            }

            // Reference brake event semantics: remember how long a brake was held,
            // then emit the release event only after the authored 0.25 s threshold.
            let brake_active = vehicle.input.brake > 0.1;
            if brake_active {
                vehicle.event_state.brake_hold_seconds += dt;
            } else if vehicle.event_state.brake_hold_seconds > 0.0 {
                if vehicle.event_state.brake_hold_seconds > 0.25 && vehicle.engine_running {
                    let mut event = VehicleEvent::new(0, entity, VehicleEventKind::BrakeReleased);
                    event.hold_seconds = vehicle.event_state.brake_hold_seconds;
                    event.speed_mps = vehicle.speed_mps;
                    frame_events.push(event);
                }
                vehicle.event_state.brake_hold_seconds = 0.0;
            }

            let handbrake_active = vehicle.input.handbrake > 0.1;
            if handbrake_active != vehicle.event_state.handbrake_active {
                let mut event = VehicleEvent::new(
                    0,
                    entity,
                    if handbrake_active {
                        VehicleEventKind::HandbrakeApplied
                    } else {
                        VehicleEventKind::HandbrakeReleased
                    },
                );
                event.speed_mps = vehicle.speed_mps;
                frame_events.push(event);
                vehicle.event_state.handbrake_active = handbrake_active;
            }

            match vehicle.definition.class {
                VehicleClass::Automobile
                | VehicleClass::Bike
                | VehicleClass::Train
                | VehicleClass::Trailer => {
                    let grounded_before =
                        vehicle.wheels.iter().any(|wheel| wheel.contact.is_some());
                    if !vehicle.event_state.grounded_initialized {
                        vehicle.event_state.grounded = grounded_before;
                        vehicle.event_state.grounded_initialized = true;
                    } else if grounded_before && !vehicle.event_state.grounded {
                        let landing_speed = (-dot(body.linear_velocity, up)).max(0.0);
                        if landing_speed > 0.55 {
                            let mut event =
                                VehicleEvent::new(0, entity, VehicleEventKind::JumpLanded);
                            event.magnitude = landing_speed;
                            event.speed_mps = vehicle.speed_mps;
                            event.position = vehicle
                                .wheels
                                .iter()
                                .filter_map(|wheel| wheel.telemetry.contact_position)
                                .next();
                            frame_events.push(event);
                        }
                        vehicle.event_state.grounded = true;
                    } else if !grounded_before {
                        vehicle.event_state.grounded = false;
                    }

                    simulate_ground_vehicle(
                        entity, vehicle, body, forward, up, dt, gravity, &mut plan,
                    );

                    let mut max_slip = 0.0f32;
                    let mut driven_slip = 0.0f32;
                    let mut max_suspension_velocity = 0.0f32;
                    for (index, wheel) in vehicle.wheels.iter().enumerate() {
                        let telemetry = wheel.telemetry;
                        if !telemetry.contact {
                            continue;
                        }
                        let slip = telemetry.slip_intensity;
                        max_slip = max_slip.max(slip);
                        if vehicle
                            .definition
                            .wheels
                            .get(index)
                            .is_some_and(|config| config.driven)
                        {
                            driven_slip = driven_slip.max(telemetry.longitudinal_slip.abs() / 0.12);
                        }
                        max_suspension_velocity =
                            max_suspension_velocity.max(telemetry.suspension_velocity.abs());
                    }

                    let skid_threshold = if vehicle.event_state.skid_active {
                        0.72
                    } else {
                        1.02
                    };
                    let skid_active = vehicle.speed_mps > 2.0 && max_slip > skid_threshold;
                    if skid_active != vehicle.event_state.skid_active {
                        let mut event = VehicleEvent::new(
                            0,
                            entity,
                            if skid_active {
                                VehicleEventKind::SkidStarted
                            } else {
                                VehicleEventKind::SkidStopped
                            },
                        );
                        event.magnitude = max_slip;
                        event.speed_mps = vehicle.speed_mps;
                        frame_events.push(event);
                        vehicle.event_state.skid_active = skid_active;
                    }

                    let spin_threshold = if vehicle.event_state.wheel_spin_active {
                        1.0
                    } else {
                        1.55
                    };
                    let wheel_spin_active = vehicle.input.throttle.abs() > 0.1
                        && driven_slip > spin_threshold
                        && vehicle.wheels.iter().any(|wheel| wheel.telemetry.contact);
                    if wheel_spin_active != vehicle.event_state.wheel_spin_active {
                        let mut event = VehicleEvent::new(
                            0,
                            entity,
                            if wheel_spin_active {
                                VehicleEventKind::WheelSpinStarted
                            } else {
                                VehicleEventKind::WheelSpinStopped
                            },
                        );
                        event.magnitude = driven_slip;
                        event.speed_mps = vehicle.speed_mps;
                        frame_events.push(event);
                        vehicle.event_state.wheel_spin_active = wheel_spin_active;
                    }

                    let suspension_impact = max_suspension_velocity > 1.8;
                    if suspension_impact && !vehicle.event_state.suspension_impact_active {
                        let mut event =
                            VehicleEvent::new(0, entity, VehicleEventKind::SuspensionImpact);
                        event.magnitude = max_suspension_velocity;
                        event.speed_mps = vehicle.speed_mps;
                        frame_events.push(event);
                    }
                    vehicle.event_state.suspension_impact_active = suspension_impact;

                    // Preserve the reference wheel-damage state independently from
                    // the render presentation. Slip accumulates friction damage;
                    // deflation/burst cadence is then handled by damage.rs at 30 Hz.
                    for (index, wheel) in vehicle.wheels.iter().enumerate() {
                        let telemetry = wheel.telemetry;
                        let Some(damage) = vehicle.damage.wheels.get_mut(index) else {
                            continue;
                        };
                        if telemetry.contact {
                            let slip = telemetry.slip_intensity;
                            if slip > 1.0 && vehicle.speed_mps > 2.0 {
                                damage.friction_damage = (damage.friction_damage
                                    + (slip - 1.0)
                                        * (vehicle.speed_mps / 30.0).clamp(0.0, 2.0)
                                        * dt
                                        * 0.12)
                                    .clamp(0.0, 2.0);
                            }
                            if telemetry.suspension_velocity.abs() > 4.0 {
                                damage.suspension_health = (damage.suspension_health
                                    - (telemetry.suspension_velocity.abs() - 4.0) * dt * 18.0)
                                    .max(0.0);
                            }
                        }
                    }

                    append_wheel_probes(
                        &mut self.next_probe_seq,
                        entity,
                        vehicle,
                        body,
                        up,
                        &mut plan,
                        &mut fresh_routes,
                    );
                }
                VehicleClass::Plane => {
                    simulate_plane(
                        entity, vehicle, body, forward, up, right, dt, gravity, &mut plan,
                    );
                }
                VehicleClass::Helicopter => {
                    simulate_helicopter(
                        entity, vehicle, body, forward, up, right, dt, gravity, &mut plan,
                    );
                }
                VehicleClass::Boat => {
                    simulate_boat(
                        entity, vehicle, body, forward, up, right, dt, gravity, &mut plan,
                    );
                }
                VehicleClass::Submarine => {
                    simulate_submarine(
                        entity, vehicle, body, forward, up, right, dt, gravity, &mut plan,
                    );
                }
            }

            advance_vehicle_powertrain(
                entity,
                vehicle,
                dt,
                dot(up, WORLD_UP) < 0.0,
                consume_petrol,
                &mut frame_events,
            );
            advance_vehicle_systems(
                entity,
                vehicle,
                dt,
                self.ambient_temperature,
                &mut frame_events,
            );

            apply_drag_and_downforce(entity, vehicle, body, up, dt, gravity, &mut plan);
        }

        self.pending_probe_routes = fresh_routes;
        for event in frame_events {
            self.emit_event(event);
        }
        plan
    }

    pub fn accept_probe_hits(&mut self, hits: &[VehicleProbeHit]) {
        // Every pending query belongs to the last generated suspension batch.
        // Missing hits explicitly clear contact, so stale ground support cannot
        // survive after a wheel leaves a ledge.
        for route in self.pending_probe_routes.values().copied() {
            if let Some(wheel) = self
                .vehicles
                .get_mut(&route.vehicle)
                .and_then(|vehicle| vehicle.wheels.get_mut(route.wheel_index))
            {
                wheel.contact = None;
                wheel.telemetry.contact = false;
                wheel.telemetry.contact_position = None;
                wheel.telemetry.contact_normal = None;
                wheel.telemetry.surface_entity = None;
                wheel.telemetry.surface_id = None;
                wheel.telemetry.surface_class = VehicleSurfaceClass::Default;
                wheel.telemetry.surface_grip_multiplier = 0.0;
            }
        }

        let mut nearest = BTreeMap::<u64, VehicleProbeHit>::new();
        for &hit in hits {
            if !hit.distance.is_finite()
                || hit.distance < 0.0
                || !self.pending_probe_routes.contains_key(&hit.seq)
            {
                continue;
            }
            nearest
                .entry(hit.seq)
                .and_modify(|current| {
                    if hit.distance < current.distance {
                        *current = hit;
                    }
                })
                .or_insert(hit);
        }

        for (seq, hit) in nearest {
            let Some(route) = self.pending_probe_routes.get(&seq).copied() else {
                continue;
            };
            let profile = self.surface_profile(hit.surface_id);
            let grip_multiplier = profile.grip(self.surface_wetness, self.surface_snow);
            let Some(wheel) = self
                .vehicles
                .get_mut(&route.vehicle)
                .and_then(|vehicle| vehicle.wheels.get_mut(route.wheel_index))
            else {
                continue;
            };
            let mut normal = normalize_or(hit.normal, mul(route.direction, -1.0));
            if dot(normal, route.direction) > 0.0 {
                normal = mul(normal, -1.0);
            }
            wheel.contact = Some(WheelContact {
                position: hit.position,
                distance: hit.distance,
                normal,
                surface_entity: hit.surface_entity,
                surface_id: hit.surface_id,
                surface_class: profile.class,
                grip_multiplier,
            });
            wheel.telemetry.contact = true;
            wheel.telemetry.contact_position = Some(hit.position);
            wheel.telemetry.contact_normal = Some(normal);
            wheel.telemetry.surface_entity = hit.surface_entity;
            wheel.telemetry.surface_id = hit.surface_id;
            wheel.telemetry.surface_class = profile.class;
            wheel.telemetry.surface_grip_multiplier = grip_multiplier;
        }
    }

    pub fn telemetry(&self, entity: VehicleEntity) -> Option<VehicleTelemetry> {
        self.vehicles
            .get(&entity)
            .map(|vehicle| build_vehicle_telemetry(entity, vehicle))
    }

    pub fn wheel_telemetry(
        &self,
        entity: VehicleEntity,
        wheel_index: usize,
    ) -> Option<WheelTelemetry> {
        self.vehicles.get(&entity).and_then(|vehicle| {
            vehicle.wheels.get(wheel_index).map(|wheel| {
                wheel_telemetry_from_state(wheel, vehicle.damage.wheels.get(wheel_index))
            })
        })
    }

    pub fn runtime_state(&self) -> serde_json::Value {
        let vehicles = self
            .vehicles
            .iter()
            .map(|(&entity, vehicle)| {
                let mut value = serde_json::to_value(build_vehicle_telemetry(entity, vehicle))
                    .expect("vehicle telemetry must serialize");
                if let Some(object) = value.as_object_mut() {
                    object.insert(
                        "damage".to_owned(),
                        serde_json::to_value(&vehicle.damage)
                            .expect("vehicle damage state must serialize"),
                    );
                }
                value
            })
            .collect::<Vec<_>>();

        serde_json::json!({
            "schema": "newviso.vehicle.runtime.v1",
            "count": vehicles.len(),
            "pending_probes": self.pending_probe_routes.len(),
            "pending_events": self.events.len(),
            "consume_petrol": self.consume_petrol,
            "surface_policy": {
                "mapped_surfaces": self.surface_profiles.len(),
                "wetness": self.surface_wetness,
                "snow": self.surface_snow,
                "default": self.default_surface_profile
            },
            "vehicles": vehicles,
        })
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod reference_tests;

#[cfg(test)]
mod specification_tests;
