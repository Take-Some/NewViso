use super::*;

impl VehicleRuntime {
    pub fn set_player_driver(&mut self, entity: VehicleEntity, player: bool) -> Result<(), String> {
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle {entity}"))?;
        let before = vehicle.damage.status();
        vehicle.player_driver = player;
        reconcile_damage_features(
            &mut vehicle.damage,
            vehicle.definition.class,
            &vehicle.definition.specification,
            player,
        );
        let after = vehicle.damage.status();
        let stopped = vehicle.engine_running && !vehicle.engine_operational();
        if stopped {
            vehicle.engine_running = false;
            vehicle.engine_starting = false;
            vehicle.transmission.engine_speed = 0.0;
        }
        for signal in damage_transition_signals(before, after) {
            let mut event = self.next_event(entity, signal.kind);
            event.details = serde_json::json!({"before": before, "after": after});
            self.events.push(event);
        }
        if stopped {
            let event = self.next_event(entity, VehicleEventKind::EngineStopped);
            self.events.push(event);
        }
        Ok(())
    }

    pub fn set_specification(
        &mut self,
        entity: VehicleEntity,
        specification: VehicleSpecification,
    ) -> Result<(), String> {
        specification.validate()?;
        let mut definition = self
            .definition(entity)
            .ok_or_else(|| format!("unknown vehicle {entity}"))?
            .clone();
        definition.specification = specification;
        self.upsert(entity, definition)
    }

    pub fn set_damage_policy(
        &mut self,
        entity: VehicleEntity,
        policy: VehicleDamagePolicy,
    ) -> Result<(), String> {
        self.vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle {entity}"))?
            .damage_policy = policy;
        Ok(())
    }

    pub fn set_ambient_temperature(&mut self, temperature: f32) -> Result<(), String> {
        if !temperature.is_finite() {
            return Err("ambient temperature must be finite".into());
        }
        self.ambient_temperature = temperature;
        Ok(())
    }

    pub fn set_thermal_state(
        &mut self,
        entity: VehicleEntity,
        state: VehicleThermalState,
    ) -> Result<(), String> {
        if !state.temperature_celsius.is_finite() {
            return Err("engine temperature must be finite".into());
        }
        self.vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle {entity}"))?
            .thermal = state;
        Ok(())
    }

    pub fn arm_alarm(&mut self, entity: VehicleEntity, armed: bool) -> Result<(), String> {
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle {entity}"))?;
        if armed && vehicle.definition.class != VehicleClass::Automobile {
            return Err("only automobiles support a car alarm".into());
        }
        let stopped = vehicle.alarm.active();
        vehicle.alarm = VehicleAlarmState {
            armed: armed && !vehicle.damage.exploded,
            remaining_seconds: 0.0,
        };
        if stopped {
            let event = self.next_event(entity, VehicleEventKind::AlarmStopped);
            self.events.push(event);
        }
        Ok(())
    }

    pub fn trigger_alarm(&mut self, entity: VehicleEntity) -> Result<bool, String> {
        let sample = deterministic_unit(self.next_event_seq ^ entity.rotate_left(23));
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle {entity}"))?;
        if !vehicle.alarm.armed
            || vehicle.damage.exploded
            || vehicle.definition.class != VehicleClass::Automobile
        {
            return Ok(false);
        }
        // Reference CAR_ALARM_DURATION + random(CAR_ALARM_RANDOM_DURATION).
        let duration = 10.0 + sample * 10.0;
        vehicle.alarm = VehicleAlarmState {
            armed: false,
            remaining_seconds: duration,
        };
        let mut event = self.next_event(entity, VehicleEventKind::AlarmStarted);
        event.hold_seconds = duration;
        self.events.push(event);
        Ok(true)
    }

    pub fn set_health(
        &mut self,
        entity: VehicleEntity,
        update: VehicleHealthUpdate,
    ) -> Result<(), String> {
        for value in [
            update.overall_health,
            update.body_health,
            update.engine_health,
            update.petrol_tank_health,
            update.oil_level,
        ]
        .into_iter()
        .flatten()
        {
            if !value.is_finite() {
                return Err("vehicle health values must be finite".into());
            }
        }
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle {entity}"))?;
        let old = vehicle.damage.clone();
        let state = &mut vehicle.damage;
        if let Some(v) = update.overall_health {
            state.overall_health = v.clamp(0.0, BODY_HEALTH_MAX);
        }
        if let Some(v) = update.body_health {
            state.body_health = v.clamp(0.0, BODY_HEALTH_MAX);
        }
        if let Some(v) = update.oil_level {
            state.oil_level = v.clamp(0.0, state.oil_capacity);
        }
        if let Some(v) = update.engine_health {
            state.engine_health = v.clamp(ENGINE_DAMAGE_FINISHED, ENGINE_HEALTH_MAX);
            state.engine_on_fire = state.engine_health < ENGINE_DAMAGE_ON_FIRE
                && state.engine_health > ENGINE_DAMAGE_FIRE_FINISH;
            state.engine_dead = state.engine_health == ENGINE_DAMAGE_ON_FIRE
                || state.engine_health <= ENGINE_DAMAGE_FIRE_FINISH;
            state.oil_leaking = state.engine_health < ENGINE_DAMAGE_OIL_LEAKING;
        }
        if let Some(v) = update.petrol_tank_health {
            state.petrol_tank_health = v.clamp(PETROL_TANK_FINISHED, PETROL_TANK_HEALTH_MAX);
            state.petrol_leaking = state.petrol_tank_health < PETROL_TANK_LEAKING;
            state.petrol_tank_on_fire = state.petrol_tank_health < PETROL_TANK_ON_FIRE;
        }
        reconcile_damage_features(
            state,
            vehicle.definition.class,
            &vehicle.definition.specification,
            vehicle.player_driver,
        );
        let after = state.status();
        vehicle.engine_condition = state.engine_condition();
        let mut signals = damage_transition_signals(old.status(), after)
            .into_iter()
            .map(|signal| (signal.kind, signal.magnitude))
            .collect::<Vec<_>>();
        if state.engine_health < old.engine_health {
            signals.push((
                VehicleEventKind::EngineDamaged,
                old.engine_health - state.engine_health,
            ));
        }
        if vehicle.engine_running && !vehicle.engine_operational() {
            vehicle.engine_running = false;
            vehicle.engine_starting = false;
            vehicle.engine_start_remaining = 0.0;
            vehicle.transmission.engine_speed = 0.0;
            signals.push((VehicleEventKind::EngineStopped, 1.0));
        }
        for (kind, magnitude) in signals {
            let mut event = self.next_event(entity, kind);
            event.magnitude = magnitude;
            event.damage_type = Some(VehicleDamageType::Script);
            event.details = serde_json::json!({"before": old.status(), "after": after});
            self.events.push(event);
        }
        Ok(())
    }

    pub fn explode(&mut self, entity: VehicleEntity) -> Result<bool, String> {
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle {entity}"))?;
        if vehicle.damage.exploded
            || vehicle.damage_policy.explosion_proof
            || vehicle.damage_policy.invincible
            || vehicle.definition.specification.damage.indestructible
        {
            return Ok(false);
        }
        let before = vehicle.damage.status();
        let was_running = vehicle.engine_running;
        vehicle.damage.exploded = true;
        vehicle.damage.overall_health = 0.0;
        vehicle.damage.body_health = 0.0;
        vehicle.damage.engine_health = ENGINE_DAMAGE_FINISHED;
        vehicle.damage.petrol_tank_health = PETROL_TANK_FINISHED;
        vehicle.damage.engine_dead = true;
        vehicle.damage.engine_on_fire = false;
        vehicle.damage.driveable = false;
        vehicle.damage.driveable_player = false;
        vehicle.damage.driveable_ai = false;
        vehicle.engine_condition = 0.0;
        vehicle.engine_running = false;
        vehicle.engine_starting = false;
        vehicle.engine_start_remaining = 0.0;
        vehicle.transmission.engine_speed = 0.0;
        reconcile_damage_features(
            &mut vehicle.damage,
            vehicle.definition.class,
            &vehicle.definition.specification,
            vehicle.player_driver,
        );
        let after = vehicle.damage.status();
        let mut signals = damage_transition_signals(before, after);
        signals.insert(
            0,
            VehicleDamageSignal::new(VehicleEventKind::VehicleExploded, 1.0),
        );
        if was_running {
            signals.push(VehicleDamageSignal::new(
                VehicleEventKind::EngineStopped,
                1.0,
            ));
        }
        for signal in signals {
            let mut event = self.next_event(entity, signal.kind);
            event.magnitude = signal.magnitude;
            event.details = serde_json::json!({"before": before, "after": after});
            self.events.push(event);
        }
        self.arm_alarm(entity, false)?;
        Ok(true)
    }
}

pub(super) fn advance_vehicle_systems(
    entity: VehicleEntity,
    vehicle: &mut VehicleInstance,
    dt: f32,
    ambient: f32,
    events: &mut Vec<VehicleEvent>,
) {
    if vehicle.alarm.active() {
        vehicle.alarm.remaining_seconds = if vehicle.damage.exploded {
            0.0
        } else {
            (vehicle.alarm.remaining_seconds - dt).max(0.0)
        };
        if !vehicle.alarm.active() {
            events.push(VehicleEvent::new(0, entity, VehicleEventKind::AlarmStopped));
        }
    }
    let was_fan_on = vehicle.thermal.cooling_fan_on;
    vehicle.thermal.advance(
        vehicle.definition.class,
        vehicle.engine_running,
        vehicle.damage.engine_health,
        vehicle.damage.engine_on_fire,
        vehicle.speed_forward_mps.abs()
            / vehicle.definition.handling.max_flat_velocity_mps.max(0.01),
        ambient,
        dt,
    );
    if was_fan_on != vehicle.thermal.cooling_fan_on {
        events.push(VehicleEvent::new(
            0,
            entity,
            if vehicle.thermal.cooling_fan_on {
                VehicleEventKind::CoolingFanStarted
            } else {
                VehicleEventKind::CoolingFanStopped
            },
        ));
    }
}
