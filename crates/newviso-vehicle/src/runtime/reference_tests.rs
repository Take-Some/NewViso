use super::*;

fn body_map(entity: u64) -> BTreeMap<u64, VehicleBodyState> {
    BTreeMap::from([(
        entity,
        VehicleBodyState {
            rotation: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        },
    )])
}

#[test]
fn infinite_fuel_starts_and_never_emits_exhaustion_for_any_motor_class() {
    for class in [
        VehicleClass::Automobile,
        VehicleClass::Bike,
        VehicleClass::Boat,
        VehicleClass::Plane,
        VehicleClass::Helicopter,
        VehicleClass::Submarine,
    ] {
        let mut rt = VehicleRuntime::new();
        let mut def = VehicleDefinition::automobile();
        def.class = class;
        def.handling.petrol_tank_volume = 0.0;
        if !class.uses_wheel_probes() {
            def.wheels.clear();
        }
        rt.upsert(1, def).unwrap();
        rt.set_engine_running(1, true).unwrap();
        rt.set_consume_petrol(true);
        assert!(rt.telemetry(1).unwrap().engine_running, "{class:?}");
        rt.drain_events();
        for _ in 0..50 {
            rt.prepare_frame(0.05, 9.81, &body_map(1));
        }
        assert!(rt.telemetry(1).unwrap().engine_running, "{class:?}");
        assert!(
            !rt.drain_events()
                .iter()
                .any(|e| e.kind == VehicleEventKind::FuelExhausted),
            "{class:?}"
        );
    }
}

#[test]
fn airborne_and_watercraft_powertrains_burn_and_repair_through_same_runtime() {
    for class in [
        VehicleClass::Boat,
        VehicleClass::Plane,
        VehicleClass::Helicopter,
        VehicleClass::Submarine,
    ] {
        let mut rt = VehicleRuntime::new();
        let mut def = VehicleDefinition::automobile();
        def.class = class;
        def.wheels.clear();
        rt.upsert(5, def).unwrap();
        rt.set_health(
            5,
            VehicleHealthUpdate {
                engine_health: Some(-100.0),
                petrol_tank_health: Some(-990.0),
                ..Default::default()
            },
        )
        .unwrap();
        for _ in 0..10 {
            rt.prepare_frame(0.05, 9.81, &body_map(5));
        }
        let events = rt.drain_events();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.kind == VehicleEventKind::VehicleExploded)
                .count(),
            1,
            "{class:?}"
        );
        assert!(rt.damage_state(5).unwrap().exploded, "{class:?}");
        rt.repair_damage(5).unwrap();
        assert!(!rt.damage_state(5).unwrap().exploded);
        assert_eq!(rt.damage_state(5).unwrap().engine_health, ENGINE_HEALTH_MAX);
    }
}

#[test]
fn alarm_is_triggered_once_expires_once_and_can_be_rearmed() {
    let mut rt = VehicleRuntime::new();
    rt.upsert(1, VehicleDefinition::automobile()).unwrap();
    rt.arm_alarm(1, true).unwrap();
    assert!(rt.trigger_alarm(1).unwrap());
    let duration = rt.telemetry(1).unwrap().alarm.remaining_seconds;
    assert!((10.0..20.0).contains(&duration));
    assert!(!rt.trigger_alarm(1).unwrap());
    for _ in 0..401 {
        rt.prepare_frame(0.05, 9.81, &body_map(1));
    }
    let events = rt.drain_events();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == VehicleEventKind::AlarmStarted)
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == VehicleEventKind::AlarmStopped)
            .count(),
        1
    );
    assert!(!rt.telemetry(1).unwrap().alarm.armed);
    rt.arm_alarm(1, true).unwrap();
    assert!(rt.trigger_alarm(1).unwrap());
}

#[test]
fn damage_proofs_block_state_mutation_and_events_but_authoritative_health_still_works() {
    let mut rt = VehicleRuntime::new();
    rt.upsert(1, VehicleDefinition::automobile()).unwrap();
    rt.set_damage_policy(
        1,
        VehicleDamagePolicy {
            bullet_proof: true,
            ..Default::default()
        },
    )
    .unwrap();
    rt.drain_events();
    let out = rt
        .apply_damage(
            1,
            VehicleDamageRequest {
                damage_type: VehicleDamageType::Bullet,
                component: VehicleDamageComponent::Engine,
                raw_damage: 900.0,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(out.effective_damage, 0.0);
    assert!(rt.drain_events().is_empty());
    assert_eq!(rt.damage_state(1).unwrap().engine_health, 1000.0);
    rt.set_health(
        1,
        VehicleHealthUpdate {
            engine_health: Some(200.0),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(rt.damage_state(1).unwrap().engine_health, 200.0);
}

#[test]
fn player_driver_uses_reference_petrol_fire_burn_rate() {
    let mut rt = VehicleRuntime::new();
    rt.upsert(1, VehicleDefinition::automobile()).unwrap();
    rt.set_player_driver(1, true).unwrap();
    rt.set_health(
        1,
        VehicleHealthUpdate {
            petrol_tank_health: Some(-100.0),
            ..Default::default()
        },
    )
    .unwrap();
    rt.prepare_frame(0.1, 9.81, &body_map(1));
    assert!((rt.damage_state(1).unwrap().petrol_tank_health + 106.0).abs() < 1.0e-4);
}

#[test]
fn fan_hysteresis_and_aircraft_airflow_are_reference_consistent() {
    let mut thermal = VehicleThermalState {
        temperature_celsius: 107.1,
        has_cooling_fan: true,
        cooling_fan_on: false,
    };
    thermal.advance(
        VehicleClass::Automobile,
        false,
        1000.0,
        false,
        0.0,
        107.1,
        0.1,
    );
    assert!(thermal.cooling_fan_on);
    thermal.temperature_celsius = 90.0;
    thermal.advance(
        VehicleClass::Automobile,
        false,
        1000.0,
        false,
        0.0,
        90.0,
        0.1,
    );
    assert!(thermal.cooling_fan_on);
    thermal.temperature_celsius = 80.0;
    thermal.advance(
        VehicleClass::Automobile,
        false,
        1000.0,
        false,
        0.0,
        80.0,
        0.1,
    );
    assert!(!thermal.cooling_fan_on);
    let mut car = VehicleThermalState::default();
    let mut plane = car;
    car.advance(
        VehicleClass::Automobile,
        true,
        1000.0,
        false,
        1.0,
        20.0,
        0.1,
    );
    plane.advance(VehicleClass::Plane, true, 1000.0, false, 1.0, 20.0, 0.1);
    assert!(plane.temperature_celsius > car.temperature_celsius);
}

#[test]
fn explosions_are_idempotent_and_repair_restores_actual_powertrain_health() {
    let mut rt = VehicleRuntime::new();
    rt.upsert(1, VehicleDefinition::automobile()).unwrap();
    rt.set_engine_running(1, true).unwrap();
    rt.arm_alarm(1, true).unwrap();
    rt.trigger_alarm(1).unwrap();
    rt.drain_events();
    assert!(rt.explode(1).unwrap());
    assert!(!rt.explode(1).unwrap());
    let events = rt.drain_events();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == VehicleEventKind::VehicleExploded)
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == VehicleEventKind::EngineStopped)
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == VehicleEventKind::AlarmStopped)
            .count(),
        1
    );
    rt.repair_damage(1).unwrap();
    rt.set_engine_running(1, true).unwrap();
    assert!(rt.telemetry(1).unwrap().engine_running);
    assert_eq!(rt.damage_state(1).unwrap().body_health, BODY_HEALTH_MAX);
}

#[test]
fn damage_events_keep_cause_part_and_vehicle_local_coordinates() {
    let mut rt = VehicleRuntime::new();
    rt.upsert(1, VehicleDefinition::automobile()).unwrap();
    rt.drain_events();
    rt.apply_damage(
        1,
        VehicleDamageRequest {
            source_entity: Some(9),
            part_index: Some(3),
            damage_type: VehicleDamageType::Bullet,
            component: VehicleDamageComponent::Glass,
            raw_damage: 10.0,
            local_position: [1.0, 2.0, 3.0],
            ..Default::default()
        },
    )
    .unwrap();
    let e = rt
        .drain_events()
        .into_iter()
        .find(|e| e.kind == VehicleEventKind::GlassBroken)
        .unwrap();
    assert_eq!(e.other_entity, Some(9));
    assert_eq!(e.part_index, Some(3));
    assert!(e.local_space);
    assert_eq!(e.position, Some([1.0, 2.0, 3.0]));
    assert_eq!(e.damage_type, Some(VehicleDamageType::Bullet));
}

#[test]
fn zero_and_nonfinite_frames_cannot_advance_vehicle_timers() {
    let mut rt = VehicleRuntime::new();
    rt.upsert(1, VehicleDefinition::automobile()).unwrap();
    rt.arm_alarm(1, true).unwrap();
    rt.trigger_alarm(1).unwrap();
    let before = rt.telemetry(1).unwrap().alarm.remaining_seconds;
    for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(rt.prepare_frame(dt, 9.81, &body_map(1)).impulses.is_empty());
    }
    assert_eq!(rt.telemetry(1).unwrap().alarm.remaining_seconds, before);
    let input = VehicleInput {
        throttle: f32::NAN,
        steer: f32::INFINITY,
        ..Default::default()
    }
    .sanitized();
    assert_eq!(input.throttle, 0.0);
    assert_eq!(input.steer, 0.0);
}

#[test]
fn rollover_shock_damages_engine_even_at_rear_body_impact() {
    let mut rt = VehicleRuntime::new();
    rt.upsert(1, VehicleDefinition::automobile()).unwrap();
    rt.apply_damage(
        1,
        VehicleDamageRequest {
            damage_type: VehicleDamageType::Collision,
            component: VehicleDamageComponent::Body,
            raw_damage: 15_000.0,
            local_position: [0.0, 0.5, 1.0],
            upside_down: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(rt.damage_state(1).unwrap().engine_health < ENGINE_HEALTH_MAX);
    assert!(rt
        .drain_events()
        .iter()
        .any(|e| e.kind == VehicleEventKind::EngineDamaged));
}

#[test]
fn petrol_health_setter_uses_same_transition_events_and_repair_resets_them() {
    let mut rt = VehicleRuntime::new();
    rt.upsert(1, VehicleDefinition::automobile()).unwrap();
    rt.drain_events();
    rt.set_petrol_tank_health(1, 500.0).unwrap();
    rt.set_petrol_tank_health(1, 500.0).unwrap();
    let events = rt.drain_events();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == VehicleEventKind::PetrolLeakStarted)
            .count(),
        1
    );
    rt.set_petrol_tank_health(1, -1.0).unwrap();
    assert_eq!(
        rt.drain_events()
            .iter()
            .filter(|e| e.kind == VehicleEventKind::PetrolFireStarted)
            .count(),
        1
    );
    rt.repair_damage(1).unwrap();
    assert!(!rt.damage_state(1).unwrap().petrol_tank_on_fire);
    rt.set_petrol_tank_health(1, -1.0).unwrap();
    assert_eq!(
        rt.drain_events()
            .iter()
            .filter(|e| e.kind == VehicleEventKind::PetrolFireStarted)
            .count(),
        1
    );
}

#[test]
fn collateral_glass_damage_does_not_double_charge_body_health_or_repeat_shatter() {
    let mut rt = VehicleRuntime::new();
    rt.upsert(1, VehicleDefinition::automobile()).unwrap();
    rt.drain_events();
    let health = rt.damage_state(1).unwrap().body_health;
    rt.damage_glass(
        1,
        7,
        30.0,
        VehicleDamageType::Collision,
        false,
        [0.0; 3],
        [1.0, 0.0, 0.0],
    )
    .unwrap();
    rt.damage_glass(
        1,
        7,
        30.0,
        VehicleDamageType::Collision,
        false,
        [0.0; 3],
        [1.0, 0.0, 0.0],
    )
    .unwrap();
    assert_eq!(rt.damage_state(1).unwrap().body_health, health);
    assert_eq!(
        rt.drain_events()
            .iter()
            .filter(|e| e.kind == VehicleEventKind::GlassBroken)
            .count(),
        1
    );
    rt.repair_damage(1).unwrap();
    assert!(rt.damage_state(1).unwrap().glass.is_empty());
}
