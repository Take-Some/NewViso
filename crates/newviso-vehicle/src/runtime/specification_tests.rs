use super::*;

fn body(entity: u64) -> BTreeMap<u64, VehicleBodyState> {
    BTreeMap::from([(
        entity,
        VehicleBodyState {
            rotation: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        },
    )])
}

fn vehicle(definition: VehicleDefinition) -> VehicleRuntime {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(1, definition).unwrap();
    runtime.drain_events();
    runtime
}

fn damage(
    kind: VehicleDamageType,
    component: VehicleDamageComponent,
    raw: f32,
) -> VehicleDamageRequest {
    VehicleDamageRequest {
        damage_type: kind,
        component,
        raw_damage: raw,
        ..Default::default()
    }
}

#[test]
fn weapon_multiplier_changes_committed_damage_and_event_snapshots_once() {
    let mut definition = VehicleDefinition::automobile();
    definition.handling.weapon_damage_multiplier = 0.25;
    let mut runtime = vehicle(definition);
    let outcome = runtime
        .apply_damage(
            1,
            damage(
                VehicleDamageType::Bullet,
                VehicleDamageComponent::Body,
                80.0,
            ),
        )
        .unwrap();
    assert_eq!(outcome.effective_damage, 20.0);
    assert_eq!(runtime.damage_state(1).unwrap().overall_health, 980.0);
    let events = runtime.drain_events();
    let applied = events
        .iter()
        .filter(|e| e.kind == VehicleEventKind::DamageApplied)
        .collect::<Vec<_>>();
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].details["before"]["overall_health"], 1000.0);
    assert_eq!(applied[0].details["after"]["overall_health"], 980.0);
    assert!(events
        .windows(2)
        .all(|pair| pair[0].sequence < pair[1].sequence));
}

#[test]
fn protected_tyres_and_wheels_survive_damage_and_subsequent_solver_steps() {
    let mut definition = VehicleDefinition::automobile();
    definition.specification.damage.tyres_can_burst = false;
    definition.specification.damage.wheels_can_break = false;
    let mut runtime = vehicle(definition);
    runtime
        .apply_damage(
            1,
            damage(
                VehicleDamageType::Bullet,
                VehicleDamageComponent::Wheel(0),
                2000.0,
            ),
        )
        .unwrap();
    runtime.apply_suspension_damage(1, 0, 2000.0).unwrap();
    for _ in 0..20 {
        runtime.prepare_frame(0.05, 9.81, &body(1));
    }
    let wheel = runtime.damage_state(1).unwrap().wheels[0];
    assert_eq!(wheel.tyre_condition, TireCondition::Intact);
    assert_eq!(wheel.tyre_health, TYRE_HEALTH_MAX);
    assert_eq!(wheel.suspension_health, 0.0);
    assert!(!runtime.drain_events().iter().any(|e| matches!(
        e.kind,
        VehicleEventKind::TyrePunctured
            | VehicleEventKind::TyreBurst
            | VehicleEventKind::WheelDetached
    )));
}

#[test]
fn disabled_leaks_do_not_consume_fluids_at_damaged_health() {
    let mut definition = VehicleDefinition::automobile();
    definition.specification.damage.oil_leaks = VehicleLeakPolicy::Disabled;
    definition.specification.damage.petrol_leaks = VehicleLeakPolicy::Disabled;
    let mut runtime = vehicle(definition);
    runtime.set_player_driver(1, true).unwrap();
    runtime
        .set_health(
            1,
            VehicleHealthUpdate {
                engine_health: Some(100.0),
                petrol_tank_health: Some(400.0),
                ..Default::default()
            },
        )
        .unwrap();
    for _ in 0..20 {
        runtime.prepare_frame(0.05, 9.81, &body(1));
    }
    let state = runtime.damage_state(1).unwrap();
    assert_eq!(state.oil_level, state.oil_capacity);
    assert_eq!(state.petrol_tank_level, state.petrol_tank_capacity);
    assert!(!state.oil_leaking && !state.petrol_leaking);
}

#[test]
fn player_only_leaks_start_on_driver_assignment_and_stop_on_departure() {
    let mut definition = VehicleDefinition::automobile();
    definition.specification.damage.oil_leaks = VehicleLeakPolicy::PlayerOnly;
    definition.specification.damage.petrol_leaks = VehicleLeakPolicy::PlayerOnly;
    let mut runtime = vehicle(definition);
    runtime
        .set_health(
            1,
            VehicleHealthUpdate {
                engine_health: Some(100.0),
                petrol_tank_health: Some(500.0),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!runtime.damage_state(1).unwrap().oil_leaking);
    runtime.set_player_driver(1, true).unwrap();
    runtime.set_player_driver(1, true).unwrap();
    runtime.set_player_driver(1, false).unwrap();
    let events = runtime.drain_events();
    for kind in [
        VehicleEventKind::OilLeakStarted,
        VehicleEventKind::OilLeakStopped,
        VehicleEventKind::PetrolLeakStarted,
        VehicleEventKind::PetrolLeakStopped,
    ] {
        assert_eq!(events.iter().filter(|e| e.kind == kind).count(), 1);
    }
}

#[test]
fn burning_player_engine_keeps_torque_and_ai_driveability_remains_separate() {
    let mut runtime = vehicle(VehicleDefinition::automobile());
    runtime.set_player_driver(1, true).unwrap();
    runtime.set_engine_running(1, true).unwrap();
    runtime
        .set_health(
            1,
            VehicleHealthUpdate {
                engine_health: Some(-100.0),
                ..Default::default()
            },
        )
        .unwrap();
    let telemetry = runtime.telemetry(1).unwrap();
    assert!(telemetry.engine_running && telemetry.driveable_player && !telemetry.driveable_ai);
    assert!(telemetry.engine_output_multiplier > 0.0);
    runtime
        .set_health(
            1,
            VehicleHealthUpdate {
                engine_health: Some(ENGINE_DAMAGE_FIRE_FINISH),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!runtime.telemetry(1).unwrap().engine_running);
    let health = runtime.damage_state(1).unwrap().engine_health;
    runtime.prepare_frame(0.05, 9.81, &body(1));
    assert!(
        runtime.damage_state(1).unwrap().engine_health < health,
        "fire continues burning after ignition shuts down"
    );
}

#[test]
fn repair_emits_end_transitions_and_restores_both_driveability_states() {
    let mut runtime = vehicle(VehicleDefinition::automobile());
    runtime
        .set_health(
            1,
            VehicleHealthUpdate {
                engine_health: Some(-100.0),
                petrol_tank_health: Some(-10.0),
                ..Default::default()
            },
        )
        .unwrap();
    runtime.drain_events();
    runtime.repair_damage(1).unwrap();
    runtime.repair_damage(1).unwrap();
    let events = runtime.drain_events();
    for kind in [
        VehicleEventKind::EngineFireStopped,
        VehicleEventKind::PetrolFireStopped,
        VehicleEventKind::VehicleRestored,
        VehicleEventKind::DriveabilityChanged,
    ] {
        assert_eq!(
            events.iter().filter(|e| e.kind == kind).count(),
            1,
            "{kind:?}"
        );
    }
}

#[test]
fn electric_powertrain_never_consumes_petrol_oil_or_starts_combustion_effects() {
    let mut definition = VehicleDefinition::automobile();
    definition.specification.powertrain = VehiclePowertrainKind::Electric;
    definition.handling.petrol_consumption_rate = 10.0;
    let mut runtime = vehicle(definition);
    runtime.set_engine_running(1, true).unwrap();
    runtime.set_consume_petrol(true);
    for _ in 0..30 {
        runtime.prepare_frame(0.05, 9.81, &body(1));
    }
    let telemetry = runtime.telemetry(1).unwrap();
    assert!(telemetry.engine_running);
    assert_eq!(telemetry.petrol_tank_capacity, 0.0);
    assert_eq!(telemetry.oil_capacity, 0.0);
    assert_eq!(telemetry.powertrain_state, VehiclePowertrainState::Running);
    runtime
        .set_health(
            1,
            VehicleHealthUpdate {
                engine_health: Some(-100.0),
                petrol_tank_health: Some(-999.0),
                ..Default::default()
            },
        )
        .unwrap();
    runtime.prepare_frame(0.05, 9.81, &body(1));
    let state = runtime.damage_state(1).unwrap();
    assert!(!state.engine_on_fire && !state.petrol_tank_on_fire && !state.exploded);
}

#[test]
fn tanker_trailer_has_an_explosive_tank_without_an_engine() {
    let mut definition = VehicleDefinition::automobile();
    definition.class = VehicleClass::Trailer;
    definition.specification.carries_petrol = true;
    let mut runtime = vehicle(definition);
    runtime.set_engine_running(1, true).unwrap();
    assert_eq!(
        runtime.telemetry(1).unwrap().powertrain_state,
        VehiclePowertrainState::Unpowered
    );
    runtime
        .apply_damage(
            1,
            damage(
                VehicleDamageType::Bullet,
                VehicleDamageComponent::PetrolTank,
                2000.0,
            ),
        )
        .unwrap();
    assert_eq!(
        runtime.damage_state(1).unwrap().engine_health,
        ENGINE_HEALTH_MAX
    );
    assert!(!runtime.damage_state(1).unwrap().exploded);
    runtime.prepare_frame(0.05, 9.81, &body(1));
    assert!(runtime.damage_state(1).unwrap().exploded);
    assert_eq!(
        runtime
            .drain_events()
            .iter()
            .filter(|e| e.kind == VehicleEventKind::VehicleExploded)
            .count(),
        1
    );
}

#[test]
fn aircraft_health_triggers_misfire_with_full_fuel_and_torque_loss() {
    for class in [VehicleClass::Plane, VehicleClass::Helicopter] {
        let mut definition = VehicleDefinition::automobile();
        definition.class = class;
        definition.wheels.clear();
        let mut runtime = vehicle(definition);
        runtime
            .set_health(
                1,
                VehicleHealthUpdate {
                    engine_health: Some(400.0),
                    ..Default::default()
                },
            )
            .unwrap();
        runtime.set_engine_running(1, true).unwrap();
        runtime.prepare_frame(0.05, 9.81, &body(1));
        let telemetry = runtime.telemetry(1).unwrap();
        assert!(telemetry.engine_misfiring);
        assert!(telemetry.engine_output_multiplier < telemetry.engine_condition);
        let mut specification = runtime.definition(1).unwrap().specification.clone();
        specification.damage.engine_misfires = false;
        runtime.set_specification(1, specification).unwrap();
        assert!(!runtime.telemetry(1).unwrap().engine_misfiring);
        let events = runtime.drain_events();
        assert_eq!(
            events
                .iter()
                .filter(|e| e.kind == VehicleEventKind::EngineMisfireStarted)
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.kind == VehicleEventKind::EngineMisfireStopped)
                .count(),
            1
        );
    }
}

#[test]
fn engine_and_tank_regions_follow_authored_geometry_instead_of_front_rear_guess() {
    let mut definition = VehicleDefinition::automobile();
    definition.specification.damage.engine_region = Some(VehicleDamageRegion {
        center: [0.0, 0.0, 2.0],
        radius: 0.4,
    });
    definition.specification.damage.petrol_tank_region = Some(VehicleDamageRegion {
        center: [0.0, 0.0, -2.0],
        radius: 0.3,
    });
    let mut runtime = vehicle(definition);
    let mut hit = damage(
        VehicleDamageType::Bullet,
        VehicleDamageComponent::Body,
        25.0,
    );
    hit.local_position = [0.0, 0.0, -2.0];
    runtime.apply_damage(1, hit).unwrap();
    let state = runtime.damage_state(1).unwrap();
    assert_eq!(state.engine_health, ENGINE_HEALTH_MAX);
    assert!(state.petrol_tank_health < PETROL_TANK_HEALTH_MAX);
    hit.local_position = [0.0, 0.0, 2.0];
    runtime.apply_damage(1, hit).unwrap();
    assert!(runtime.damage_state(1).unwrap().engine_health < ENGINE_HEALTH_MAX);
}

#[test]
fn reverse_and_handbrake_flags_constrain_controls_and_manual_gear() {
    let mut definition = VehicleDefinition::automobile();
    definition.specification.controls.reverse = false;
    definition.specification.controls.handbrake = false;
    let mut runtime = vehicle(definition);
    runtime
        .set_input(
            1,
            VehicleInput {
                throttle: -1.0,
                handbrake: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
    runtime.prepare_frame(0.05, 9.81, &body(1));
    let telemetry = runtime.telemetry(1).unwrap();
    assert_eq!(telemetry.input.throttle, 0.0);
    assert_eq!(telemetry.input.handbrake, 0.0);
    assert!(runtime.set_manual_gear(1, Some(-1)).is_err());
    assert!(!runtime
        .drain_events()
        .iter()
        .any(|e| e.kind == VehicleEventKind::HandbrakeApplied));
}

#[test]
fn specification_update_is_idempotent_and_keeps_damage_and_fuel() {
    let mut runtime = vehicle(VehicleDefinition::automobile());
    runtime.set_petrol_tank_level(1, 15.0).unwrap();
    runtime
        .set_tire_condition(1, 1, TireCondition::Punctured)
        .unwrap();
    let mut specification = VehicleSpecification::default();
    specification.model_name = "example".into();
    runtime.set_specification(1, specification.clone()).unwrap();
    runtime.set_specification(1, specification).unwrap();
    assert_eq!(runtime.damage_state(1).unwrap().petrol_tank_level, 15.0);
    assert_eq!(
        runtime.damage_state(1).unwrap().wheels[1].tyre_condition,
        TireCondition::Punctured
    );
    assert_eq!(
        runtime
            .drain_events()
            .iter()
            .filter(|e| e.kind == VehicleEventKind::SpecificationChanged)
            .count(),
        1
    );
}

#[test]
fn fire_and_explosion_proofs_also_gate_autonomous_progression() {
    let mut runtime = vehicle(VehicleDefinition::automobile());
    runtime
        .set_damage_policy(
            1,
            VehicleDamagePolicy {
                fire_proof: true,
                explosion_proof: true,
                ..Default::default()
            },
        )
        .unwrap();
    runtime
        .set_health(
            1,
            VehicleHealthUpdate {
                engine_health: Some(-10.0),
                petrol_tank_health: Some(-999.99),
                ..Default::default()
            },
        )
        .unwrap();
    for _ in 0..20 {
        runtime.prepare_frame(0.05, 9.81, &body(1));
    }
    assert_eq!(runtime.damage_state(1).unwrap().engine_health, -10.0);
    assert!(!runtime.damage_state(1).unwrap().exploded);
}

#[test]
fn invalid_specification_is_rejected_without_mutation() {
    let mut runtime = vehicle(VehicleDefinition::automobile());
    let mut specification = VehicleSpecification::default();
    specification.damage.engine_region = Some(VehicleDamageRegion {
        center: [0.0; 3],
        radius: f32::NAN,
    });
    assert!(runtime.set_specification(1, specification).is_err());
    assert_eq!(
        runtime.definition(1).unwrap().specification,
        VehicleSpecification::default()
    );
    assert!(runtime.drain_events().is_empty());
}

#[test]
fn forced_explosion_clears_active_effects_and_emits_committed_end_events_once() {
    for (engine_health, tank_health) in [(150.0, 500.0), (-100.0, -10.0), (400.0, 1000.0)] {
        let mut definition = VehicleDefinition::automobile();
        definition.class = VehicleClass::Plane;
        definition.wheels.clear();
        let mut runtime = vehicle(definition);
        runtime.set_player_driver(1, true).unwrap();
        runtime.set_engine_running(1, true).unwrap();
        runtime
            .set_health(
                1,
                VehicleHealthUpdate {
                    engine_health: Some(engine_health),
                    petrol_tank_health: Some(tank_health),
                    ..Default::default()
                },
            )
            .unwrap();
        runtime.prepare_frame(0.05, 9.81, &body(1));
        let before = runtime.damage_state(1).unwrap().status();
        runtime.drain_events();
        assert!(runtime.explode(1).unwrap());
        assert!(!runtime.explode(1).unwrap());
        let state = runtime.damage_state(1).unwrap();
        assert!(state.exploded);
        assert!(
            !state.oil_leaking
                && !state.petrol_leaking
                && !state.engine_on_fire
                && !state.petrol_tank_on_fire
                && !state.engine_misfiring()
        );
        let events = runtime.drain_events();
        for (active, kind) in [
            (before.oil_leaking, VehicleEventKind::OilLeakStopped),
            (before.petrol_leaking, VehicleEventKind::PetrolLeakStopped),
            (before.engine_on_fire, VehicleEventKind::EngineFireStopped),
            (
                before.petrol_tank_on_fire,
                VehicleEventKind::PetrolFireStopped,
            ),
            (
                before.engine_misfiring,
                VehicleEventKind::EngineMisfireStopped,
            ),
        ] {
            assert_eq!(
                events.iter().filter(|e| e.kind == kind).count(),
                usize::from(active),
                "{kind:?}"
            );
        }
        let exploded = events
            .iter()
            .find(|e| e.kind == VehicleEventKind::VehicleExploded)
            .unwrap();
        assert_eq!(exploded.details["after"]["overall_health"], 0.0);
        assert_eq!(exploded.details["after"]["engine_on_fire"], false);
    }
}

#[test]
fn terminal_tank_explodes_once_after_explosion_proof_is_removed() {
    let mut runtime = vehicle(VehicleDefinition::automobile());
    runtime
        .set_damage_policy(
            1,
            VehicleDamagePolicy {
                explosion_proof: true,
                ..Default::default()
            },
        )
        .unwrap();
    runtime
        .set_health(
            1,
            VehicleHealthUpdate {
                petrol_tank_health: Some(-999.99),
                ..Default::default()
            },
        )
        .unwrap();
    runtime.prepare_frame(0.05, 9.81, &body(1));
    assert_eq!(
        runtime.damage_state(1).unwrap().petrol_tank_health,
        PETROL_TANK_FINISHED
    );
    assert!(!runtime.damage_state(1).unwrap().exploded);
    runtime.drain_events();
    runtime
        .set_damage_policy(1, VehicleDamagePolicy::default())
        .unwrap();
    runtime.prepare_frame(0.05, 9.81, &body(1));
    runtime.prepare_frame(0.05, 9.81, &body(1));
    assert!(runtime.damage_state(1).unwrap().exploded);
    let events = runtime.drain_events();
    let explosions = events
        .iter()
        .filter(|e| e.kind == VehicleEventKind::VehicleExploded)
        .collect::<Vec<_>>();
    assert_eq!(explosions.len(), 1);
    assert_eq!(explosions[0].details["after"]["body_health"], 0.0);
    assert_eq!(
        explosions[0].details["after"]["engine_health"],
        ENGINE_DAMAGE_FINISHED
    );
}
