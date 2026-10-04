use super::*;

fn body() -> VehicleBodyState {
    VehicleBodyState {
        position: [0.0, 1.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        linear_velocity: [0.0, 0.0, -10.0],
        angular_velocity: [0.0; 3],
    }
}

#[test]
fn vehicle_spawns_engine_off_and_forward_throttle_auto_starts() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(42, VehicleDefinition::automobile()).unwrap();

    let initial = runtime.telemetry(42).unwrap();
    assert!(!initial.engine_running);
    assert!(!initial.engine_starting);

    runtime
        .set_input(
            42,
            VehicleInput {
                throttle: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
    let cranking = runtime.telemetry(42).unwrap();
    assert!(!cranking.engine_running);
    assert!(cranking.engine_starting);
    assert!(cranking.engine_start_remaining > 0.0);

    let mut stationary = body();
    stationary.linear_velocity = [0.0; 3];
    let bodies = BTreeMap::from([(42, stationary)]);
    for _ in 0..120 {
        runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
        if runtime.telemetry(42).unwrap().engine_running {
            break;
        }
    }

    let started = runtime.telemetry(42).unwrap();
    assert!(started.engine_running);
    assert!(!started.engine_starting);
}

#[test]
fn failed_engine_start_retries_automatically_and_fourth_attempt_succeeds() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
    {
        // A severely damaged but startable engine can fail three attempts.
        // Wheel and body damage do not participate in this probability.
        let vehicle = runtime.vehicles.get_mut(&42).unwrap();
        vehicle.damage.engine_health = ENGINE_HEALTH_MAX * f32::EPSILON;
        vehicle.damage.driveable = true;
        vehicle.damage.driveable_player = true;
        vehicle.damage.driveable_ai = true;
        vehicle.damage.engine_dead = false;
    }
    runtime
        .set_input(
            42,
            VehicleInput {
                throttle: 1.0,
                ..Default::default()
            },
        )
        .unwrap();

    let mut stationary = body();
    stationary.linear_velocity = [0.0; 3];
    let bodies = BTreeMap::from([(42, stationary)]);
    for _ in 0..900 {
        runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
        if runtime.telemetry(42).unwrap().engine_running {
            break;
        }
    }

    let state = runtime.telemetry(42).unwrap();
    assert!(state.engine_running);
    assert_eq!(
        state.failed_engine_start_attempts,
        MAX_ENGINE_START_ATTEMPTS
    );
    let failed_events = runtime
        .drain_events()
        .into_iter()
        .filter(|event| event.kind == VehicleEventKind::EngineStartFailed)
        .count();
    assert_eq!(failed_events, usize::from(MAX_ENGINE_START_ATTEMPTS));
}

#[test]
fn reverse_input_does_not_trigger_forward_throttle_auto_start() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
    runtime
        .set_input(
            42,
            VehicleInput {
                throttle: -1.0,
                ..Default::default()
            },
        )
        .unwrap();

    let state = runtime.telemetry(42).unwrap();
    assert!(!state.engine_running);
    assert!(!state.engine_starting);
}

#[test]
fn ignition_off_preserves_probes_and_inertia_but_removes_motor_torque() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
    runtime
        .set_input(
            42,
            VehicleInput {
                throttle: -1.0,
                ..Default::default()
            },
        )
        .unwrap();
    runtime.set_engine_running(42, false).unwrap();
    let bodies = BTreeMap::from([(42, body())]);
    let plan = runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
    assert_eq!(plan.probes.len(), 4);
    assert!(!plan.impulses.is_empty());
    let state = runtime.telemetry(42).unwrap();
    assert!(!state.engine_running);
    assert_eq!(state.engine_speed, 0.0);
    assert!(state.speed_mps > 0.0);
    assert!(state.wheels.iter().all(|w| w.angular_velocity == 0.0));
    runtime.set_engine_running(42, true).unwrap();
    runtime
        .set_input(
            42,
            VehicleInput {
                throttle: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
    runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
    assert!(
        runtime
            .telemetry(42)
            .unwrap()
            .wheels
            .iter()
            .any(|w| w.angular_velocity != 0.0)
    );
}

#[test]
fn manual_neutral_removes_drive_and_rejects_invalid_gears() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
    assert!(runtime.set_manual_gear(42, Some(100)).is_err());
    runtime.set_manual_gear(42, Some(0)).unwrap();
    runtime
        .set_input(
            42,
            VehicleInput {
                throttle: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
    runtime.prepare_frame(1.0 / 120.0, 9.81, &BTreeMap::from([(42, body())]));
    let state = runtime.telemetry(42).unwrap();
    assert_eq!(state.gear, 0);
    assert!(state.wheels.iter().all(|w| w.angular_velocity == 0.0));
    runtime.set_manual_gear(42, None).unwrap();
    assert_eq!(runtime.telemetry(42).unwrap().manual_gear, None);
}

#[test]
fn ignition_off_retains_buoyancy_for_boats_and_submarines() {
    for class in [VehicleClass::Boat, VehicleClass::Submarine] {
        let mut definition = VehicleDefinition::automobile();
        definition.class = class;
        let mut runtime = VehicleRuntime::new();
        runtime.upsert(42, definition).unwrap();
        runtime
            .set_input(
                42,
                VehicleInput {
                    throttle: 1.0,
                    ..Default::default()
                },
            )
            .unwrap();
        runtime.set_engine_running(42, false).unwrap();
        let mut body = body();
        body.linear_velocity = [0.0; 3];
        let frame = runtime.prepare_frame(1.0 / 120.0, 9.81, &BTreeMap::from([(42, body)]));
        assert!(frame.impulses.iter().map(|x| x.impulse[1]).sum::<f32>() > 0.0);
        assert!(frame.impulses.iter().all(|x| x.impulse[2] == 0.0));
    }
}

#[test]
fn parked_vehicle_has_zero_wheel_spin_and_emits_no_forces() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
    {
        let car = runtime.vehicles.get_mut(&42).unwrap();
        car.speed_mps = 4.0;
        car.speed_forward_mps = 4.0;
        for wheel in &mut car.wheels {
            wheel.angular_velocity = 12.0;
            wheel.telemetry.angular_velocity = 12.0;
        }
    }
    runtime.set_enabled(42, false).unwrap();
    let frame = runtime.prepare_frame(1.0 / 120.0, 9.81, &BTreeMap::from([(42, body())]));
    assert!(frame.impulses.is_empty());
    assert!(frame.probes.is_empty());
    let car = runtime.telemetry(42).unwrap();
    assert_eq!(car.speed_mps, 0.0);
    assert!(car.wheels.iter().all(|wheel| wheel.angular_velocity == 0.0));
}

#[test]
fn downward_triangle_normal_cannot_pull_suspension_into_ground() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
    let mut state = body();
    state.linear_velocity = [0.0; 3];
    let bodies = BTreeMap::from([(42, state)]);
    let first = runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
    let hits = first
        .probes
        .iter()
        .map(|p| VehicleProbeHit {
            seq: p.seq,
            position: add(p.origin, mul(p.direction, 0.48)),
            normal: [0.0, -1.0, 0.0],
            distance: 0.48,
            surface_entity: Some(99),
            surface_id: None,
        })
        .collect::<Vec<_>>();
    runtime.accept_probe_hits(&hits);
    let frame = runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
    assert!(frame.impulses.iter().all(|p| p.impulse[1] >= 0.0));
    assert!(frame.impulses.iter().map(|p| p.impulse[1]).sum::<f32>() > 0.0);
    assert!(
        runtime
            .telemetry(42)
            .unwrap()
            .wheels
            .iter()
            .all(|w| w.contact_normal.unwrap()[1] > 0.0)
    );
}

#[test]
fn brakes_do_not_add_energy_or_reverse_wheel_spin_at_low_speed() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
    runtime
        .set_input(
            42,
            VehicleInput {
                brake: 1.0,
                handbrake: 1.0,
                ..VehicleInput::default()
            },
        )
        .unwrap();
    let mut state = body();
    state.linear_velocity = [0.0, 0.0, -0.02];
    let bodies = BTreeMap::from([(42, state)]);
    let first = runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
    let hits = first
        .probes
        .iter()
        .map(|probe| VehicleProbeHit {
            seq: probe.seq,
            position: add(probe.origin, mul(probe.direction, 0.48)),
            normal: WORLD_UP,
            distance: 0.48,
            surface_entity: Some(99),
            surface_id: None,
        })
        .collect::<Vec<_>>();
    runtime.accept_probe_hits(&hits);
    for wheel in &mut runtime.vehicles.get_mut(&42).unwrap().wheels {
        wheel.angular_velocity = 0.01;
    }
    let frame = runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
    let horizontal_impulse: f32 = frame
        .impulses
        .iter()
        .map(|impulse| impulse.impulse[2])
        .sum();
    assert!(horizontal_impulse >= 0.0);
    assert!(horizontal_impulse <= 1500.0 * 0.02 + 0.001);
    assert!(
        runtime
            .telemetry(42)
            .unwrap()
            .wheels
            .iter()
            .all(|wheel| wheel.angular_velocity >= 0.0)
    );
}

#[test]
fn contact_points_follow_hits_and_clear_when_contact_is_lost() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(7, VehicleDefinition::automobile()).unwrap();
    let bodies = BTreeMap::from([(7, body())]);
    let first = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
    let position = [1.0, 0.0, 2.0];
    runtime.accept_probe_hits(&[VehicleProbeHit {
        seq: first.probes[0].seq,
        position,
        normal: WORLD_UP,
        distance: 0.48,
        surface_entity: Some(99),
        surface_id: None,
    }]);
    let wheel = runtime.telemetry(7).unwrap().wheels[0];
    assert_eq!(wheel.contact_position, Some(position));
    assert_eq!(wheel.contact_normal, Some(WORLD_UP));
    runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
    runtime.accept_probe_hits(&[]);
    let wheel = runtime.telemetry(7).unwrap().wheels[0];
    assert!(!wheel.contact);
    assert_eq!(wheel.contact_position, None);
    assert_eq!(wheel.contact_normal, None);
}

#[test]
fn reference_handling_converts_authoring_units() {
    let handling = HandlingData::from_reference_units(ReferenceHandlingData {
        initial_drive_max_flat_vel: 180.0,
        steering_lock: 30.0,
        initial_drag_coeff: 10.0,
        suspension_comp_damp: 2.0,
        ..ReferenceHandlingData::default()
    });
    assert!((handling.max_flat_velocity_mps - 50.0).abs() < 1.0e-5);
    assert!((handling.max_gearing_velocity_mps - 60.0).abs() < 1.0e-5);
    assert!((handling.steering_lock_rad - 30.0_f32.to_radians()).abs() < 1.0e-6);
    assert!((handling.drag_coefficient - 0.001).abs() < 1.0e-7);
    assert!((handling.suspension_comp_damp - 0.2).abs() < 1.0e-6);
}

#[test]
fn automobile_emits_four_suspension_queries() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
    let bodies = BTreeMap::from([(42, body())]);
    let frame = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
    assert_eq!(frame.probes.len(), 4);
}

#[test]
fn steering_input_turns_grounded_car_in_requested_direction() {
    for (steer, requested_side) in [(-0.5, -1.0), (0.5, 1.0)] {
        for speed in [-10.0, 10.0] {
            let mut runtime = VehicleRuntime::new();
            runtime.upsert(7, VehicleDefinition::automobile()).unwrap();
            runtime
                .set_input(
                    7,
                    VehicleInput {
                        steer,
                        ..Default::default()
                    },
                )
                .unwrap();
            let mut moving = body();
            moving.linear_velocity = [0.0, 0.0, -speed];
            let bodies = BTreeMap::from([(7, moving)]);
            let frame = runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
            runtime.accept_probe_hits(
                &frame
                    .probes
                    .iter()
                    .map(|probe| VehicleProbeHit {
                        seq: probe.seq,
                        position: add(probe.origin, mul(probe.direction, 0.45)),
                        normal: WORLD_UP,
                        distance: 0.45,
                        surface_entity: None,
                        surface_id: None,
                    })
                    .collect::<Vec<_>>(),
            );
            let frame = runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
            let state = runtime.telemetry(7).unwrap();
            for wheel in &state.wheels[..2] {
                let forward = rotate_around_axis([0.0, 0.0, -1.0], WORLD_UP, wheel.steer_angle);
                assert!(
                    forward[0] * requested_side > 0.0,
                    "steer={steer} speed={speed} forward={forward:?}"
                );
            }
            // Integrate actual contact impulses: forward right steering must
            // yaw toward +X. Reverse travel naturally reverses this yaw.
            let yaw_impulse: f32 = frame
                .impulses
                .iter()
                .map(|impulse| cross(sub(impulse.point, moving.position), impulse.impulse)[1])
                .sum();
            assert!(
                yaw_impulse * requested_side * speed < -0.001,
                "steer={steer} speed={speed} yaw_impulse={yaw_impulse}"
            );
        }
    }
}

#[test]
fn probe_hits_feed_suspension_forces() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(7, VehicleDefinition::automobile()).unwrap();
    let bodies = BTreeMap::from([(7, body())]);
    let first = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
    let hits = first
        .probes
        .iter()
        .map(|probe| VehicleProbeHit {
            seq: probe.seq,
            position: add(probe.origin, mul(probe.direction, 0.48)),
            normal: WORLD_UP,
            distance: 0.48,
            surface_entity: Some(99),
            surface_id: None,
        })
        .collect::<Vec<_>>();
    runtime.accept_probe_hits(&hits);
    let second = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
    assert!(
        second
            .impulses
            .iter()
            .any(|impulse| impulse.impulse[1] > 0.0)
    );
}

#[test]
fn surface_policy_and_weather_reduce_wheel_grip() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(7, VehicleDefinition::automobile()).unwrap();
    runtime
        .set_surface_profile(
            55,
            VehicleSurfaceProfile::for_class(VehicleSurfaceClass::Ice),
        )
        .unwrap();
    runtime.set_surface_weather(1.0, 0.0).unwrap();

    let bodies = BTreeMap::from([(7, body())]);
    let first = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
    runtime.accept_probe_hits(
        &first
            .probes
            .iter()
            .map(|probe| VehicleProbeHit {
                seq: probe.seq,
                position: add(probe.origin, mul(probe.direction, 0.45)),
                normal: WORLD_UP,
                distance: 0.45,
                surface_entity: Some(99),
                surface_id: Some(55),
            })
            .collect::<Vec<_>>(),
    );

    let telemetry = runtime.telemetry(7).expect("vehicle telemetry");
    assert!(telemetry.wheels.iter().all(|wheel| {
        wheel.contact
            && wheel.surface_id == Some(55)
            && wheel.surface_class == VehicleSurfaceClass::Ice
            && wheel.surface_grip_multiplier < 0.3
    }));
}

#[test]
fn missing_next_hit_clears_contact() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(7, VehicleDefinition::automobile()).unwrap();
    let bodies = BTreeMap::from([(7, body())]);
    let first = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
    runtime.accept_probe_hits(
        &first
            .probes
            .iter()
            .map(|probe| VehicleProbeHit {
                seq: probe.seq,
                position: probe.origin,
                normal: WORLD_UP,
                distance: 0.45,
                surface_entity: Some(9),
                surface_id: None,
            })
            .collect::<Vec<_>>(),
    );
    let _second = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
    runtime.accept_probe_hits(&[]);
    let state = runtime.runtime_state();
    let contacts = state["vehicles"][0]["wheels"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|wheel| wheel["contact"].as_bool() == Some(true))
        .count();
    assert_eq!(contacts, 0);
}
#[test]
fn puncture_is_local_and_changes_radius_grip_and_offcentre_drag() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(7, VehicleDefinition::automobile()).unwrap();
    let mut moving = body();
    moving.linear_velocity = [0.0, 0.0, -12.0];
    let bodies = BTreeMap::from([(7, moving)]);
    let first = runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
    let hits: Vec<_> = first
        .probes
        .iter()
        .map(|p| VehicleProbeHit {
            seq: p.seq,
            position: add(p.origin, mul(p.direction, 0.65)),
            normal: WORLD_UP,
            distance: 0.65,
            surface_entity: None,
            surface_id: None,
        })
        .collect();
    runtime.accept_probe_hits(&hits);
    let mut intact = runtime.clone();
    let baseline = intact.prepare_frame(1.0 / 120.0, 9.81, &bodies);
    runtime
        .set_tire_condition(7, 0, TireCondition::Punctured)
        .unwrap();
    let damaged = runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
    let t = runtime.telemetry(7).unwrap();
    assert_eq!(t.wheels[0].tire_condition, TireCondition::Punctured);
    assert_eq!(t.wheels[1].tire_condition, TireCondition::Intact);
    assert!(t.wheels[0].effective_radius < t.wheels[1].effective_radius);
    assert!(t.wheels[0].tire_grip_multiplier < t.wheels[1].tire_grip_multiplier);
    assert_ne!(damaged.impulses[0].impulse, baseline.impulses[0].impulse);
    assert!(damaged.impulses[0].point[0] < 0.0);
}

#[test]
fn missing_wheel_has_no_probe_or_force_and_repair_restores_it() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(7, VehicleDefinition::automobile()).unwrap();
    let bodies = BTreeMap::from([(7, body())]);
    let first = runtime.prepare_frame(0.01, 9.81, &bodies);
    runtime
        .set_tire_condition(7, 0, TireCondition::Missing)
        .unwrap();
    runtime.accept_probe_hits(&[VehicleProbeHit {
        seq: first.probes[0].seq,
        position: first.probes[0].origin,
        normal: WORLD_UP,
        distance: 0.0,
        surface_entity: None,
        surface_id: None,
    }]);
    let frame = runtime.prepare_frame(0.01, 9.81, &bodies);
    assert_eq!(frame.probes.len(), 3);
    assert_eq!(runtime.telemetry(7).unwrap().wheels[0].normal_force, 0.0);
    runtime.repair_tires(7).unwrap();
    assert_eq!(runtime.prepare_frame(0.01, 9.81, &bodies).probes.len(), 4);
}

#[test]
fn reordering_wheels_preserves_damage_by_name() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(7, VehicleDefinition::automobile()).unwrap();
    runtime
        .set_tire_condition(7, 0, TireCondition::Punctured)
        .unwrap();
    let mut definition = VehicleDefinition::automobile();
    definition.wheels.swap(0, 1);
    definition.wheels[0].opposite_index = Some(1);
    definition.wheels[1].opposite_index = Some(0);
    runtime.upsert(7, definition).unwrap();
    assert_eq!(
        runtime.telemetry(7).unwrap().wheels[1].tire_condition,
        TireCondition::Punctured
    );
}

#[test]
fn engine_wheel_damage_does_not_prevent_start_for_player_or_ai() {
    for player in [false, true] {
        for condition in [
            TireCondition::Intact,
            TireCondition::Punctured,
            TireCondition::Rim,
            TireCondition::Missing,
        ] {
            let mut runtime = VehicleRuntime::new();
            runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
            runtime.set_player_driver(42, player).unwrap();
            for index in 0..4 {
                runtime.set_tire_condition(42, index, condition).unwrap();
            }
            runtime
                .set_health(
                    42,
                    VehicleHealthUpdate {
                        overall_health: Some(0.0),
                        body_health: Some(0.0),
                        ..Default::default()
                    },
                )
                .unwrap();
            for wheel in &mut runtime.vehicles.get_mut(&42).unwrap().damage.wheels {
                wheel.friction_damage = 2.0;
            }
            runtime
                .set_input(
                    42,
                    VehicleInput {
                        throttle: 1.0,
                        ..Default::default()
                    },
                )
                .unwrap();
            let mut stationary = body();
            stationary.linear_velocity = [0.0; 3];
            let bodies = BTreeMap::from([(42, stationary)]);
            for _ in 0..120 {
                runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
            }
            let state = runtime.telemetry(42).unwrap();
            assert!(state.engine_running, "player={player}, tyre={condition:?}");
            assert_eq!(state.failed_engine_start_attempts, 0);
            assert!(
                runtime
                    .drain_events()
                    .iter()
                    .all(|e| e.kind != VehicleEventKind::EngineStartFailed)
            );
            runtime.set_engine_running(42, false).unwrap();
            runtime.set_engine_running(42, true).unwrap();
            assert!(runtime.telemetry(42).unwrap().engine_running);
            runtime.set_player_driver(42, !player).unwrap();
            runtime
                .set_health(
                    42,
                    VehicleHealthUpdate {
                        body_health: Some(25.0),
                        ..Default::default()
                    },
                )
                .unwrap();
            runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
            assert!(runtime.telemetry(42).unwrap().engine_running);
        }
    }
}

#[test]
fn engine_wheel_hits_do_not_stall_an_undamaged_engine() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
    runtime.set_engine_running(42, true).unwrap();
    for index in 0..4 {
        runtime
            .apply_damage(
                42,
                VehicleDamageRequest {
                    damage_type: VehicleDamageType::Bullet,
                    component: VehicleDamageComponent::Wheel(index),
                    raw_damage: 10000.0,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(runtime.telemetry(42).unwrap().engine_running);
    }
    assert_eq!(
        runtime.damage_state(42).unwrap().engine_health,
        ENGINE_HEALTH_MAX
    );
    assert_eq!(runtime.damage_state(42).unwrap().overall_health, 0.0);
    assert!(
        runtime
            .telemetry(42)
            .unwrap()
            .wheels
            .iter()
            .all(|w| w.tire_condition == TireCondition::Missing)
    );
    let mut stationary = body();
    stationary.linear_velocity = [0.0; 3];
    let bodies = BTreeMap::from([(42, stationary)]);
    for _ in 0..120 {
        runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
    }
    assert!(runtime.telemetry(42).unwrap().engine_running);
    assert!(
        runtime
            .drain_events()
            .iter()
            .all(|e| e.kind != VehicleEventKind::EngineStopped)
    );
}

#[test]
fn engine_wheel_absence_allows_revs_without_traction() {
    let mut runtime = VehicleRuntime::new();
    runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
    for index in 0..4 {
        runtime
            .set_tire_condition(42, index, TireCondition::Missing)
            .unwrap();
    }
    runtime.set_engine_running(42, true).unwrap();
    runtime
        .set_input(
            42,
            VehicleInput {
                throttle: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
    let mut stationary = body();
    stationary.linear_velocity = [0.0; 3];
    let bodies = BTreeMap::from([(42, stationary)]);
    for _ in 0..240 {
        let plan = runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
        assert!(plan.probes.is_empty());
        assert!(plan.impulses.iter().all(|i| length(i.impulse) < 0.0001));
    }
    let state = runtime.telemetry(42).unwrap();
    assert!(state.engine_running);
    assert_eq!(state.powertrain_state, VehiclePowertrainState::Running);
    assert!(state.engine_speed > 0.95);
    assert_eq!(state.speed_mps, 0.0);
}

#[test]
fn engine_wheel_healthy_engine_never_fails_because_of_body_damage() {
    for entity in 1..=16 {
        let mut runtime = VehicleRuntime::new();
        runtime
            .upsert(entity, VehicleDefinition::automobile())
            .unwrap();
        runtime
            .set_health(
                entity,
                VehicleHealthUpdate {
                    overall_health: Some(1.0),
                    body_health: Some(50.0),
                    ..Default::default()
                },
            )
            .unwrap();
        let mut stationary = body();
        stationary.linear_velocity = [0.0; 3];
        let bodies = BTreeMap::from([(entity, stationary)]);
        for _ in 0..4 {
            runtime.set_engine_running(entity, false).unwrap();
            runtime
                .set_input(
                    entity,
                    VehicleInput {
                        throttle: 1.0,
                        ..Default::default()
                    },
                )
                .unwrap();
            for _ in 0..90 {
                runtime.prepare_frame(1.0 / 120.0, 9.81, &bodies);
            }
            assert!(runtime.telemetry(entity).unwrap().engine_running);
            assert_eq!(
                runtime
                    .telemetry(entity)
                    .unwrap()
                    .failed_engine_start_attempts,
                0
            );
            assert!(
                runtime
                    .drain_events()
                    .iter()
                    .all(|e| e.kind != VehicleEventKind::EngineStartFailed)
            );
        }
    }
}

#[test]
fn engine_wheel_absence_does_not_bypass_real_engine_failure_or_empty_fuel() {
    for empty_fuel in [false, true] {
        let mut runtime = VehicleRuntime::new();
        runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
        for index in 0..4 {
            runtime
                .set_tire_condition(42, index, TireCondition::Missing)
                .unwrap();
        }
        if empty_fuel {
            runtime.set_petrol_tank_level(42, 0.0).unwrap();
        } else {
            runtime
                .set_health(
                    42,
                    VehicleHealthUpdate {
                        engine_health: Some(0.0),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        runtime.set_engine_running(42, true).unwrap();
        assert!(!runtime.telemetry(42).unwrap().engine_running);
    }
}
