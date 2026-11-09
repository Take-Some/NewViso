use super::*;
fn obs(angle: f32, seconds: f64) -> Observation {
    Observation {
        angle,
        seconds,
        edge_radius: 1.0,
        ..Default::default()
    }
}
#[test]
fn lock_requires_alignment_and_rejects_unknown_keys() {
    let mut m = Mechanism::new(MechanismConfig::default()).unwrap();
    assert_eq!(
        m.operate(Operation::Lock, obs(0.4, 0.0), true),
        DoorResult::NotClosed
    );
    assert_eq!(
        m.operate(Operation::Lock, obs(0.0, 0.0), false),
        DoorResult::Denied
    );
    assert_eq!(
        m.operate(Operation::Lock, obs(0.0, 0.0), true),
        DoorResult::Accepted
    );
    assert_eq!(
        m.operate(Operation::Open { target_angle: 1.5 }, obs(0.0, 0.0), false),
        DoorResult::Denied
    );
    assert!(m.tick(obs(0.0, 0.1)).retention_nm > 1000.0);
    assert!(m.tick(obs(0.0, 0.2)).target.is_none());
}
#[test]
fn retracting_a_handle_releases_retention_but_never_sets_a_pose() {
    let mut m = Mechanism::new(MechanismConfig::default()).unwrap();
    assert!(m.tick(obs(0.0, 0.0)).retention_nm > 0.0);
    m.operate(Operation::Open { target_angle: 1.5 }, obs(0.0, 0.0), false);
    let a = m.tick(obs(0.1, 0.1));
    assert_eq!(a.retention_nm, 0.0);
    assert_eq!(a.target, Some(1.5));
    assert!(a.motor_nm <= m.config.operating_torque_nm);
}
#[test]
fn a_returning_door_latches_only_after_alignment_and_speed_allow_it() {
    let mut m = Mechanism::new(MechanismConfig::default()).unwrap();
    m.operate(Operation::Open { target_angle: 1.5 }, obs(0.0, 0.0), true);
    m.operate(Operation::Release, obs(0.4, 0.5), true);
    m.tick(obs(0.4, 3.0));
    assert!(!m.state.latch_engaged);
    let mut o = obs(0.0, 4.0);
    o.angular_speed = 3.0;
    m.tick(o);
    assert!(!m.state.latch_engaged);
    m.tick(obs(0.0, 5.0));
    assert!(m.state.latch_engaged);
}
#[test]
fn roller_hardware_yields_to_push_while_a_locked_bolt_does_not() {
    let c = MechanismConfig {
        latch: LatchKind::Roller,
        ..Default::default()
    };
    let mut m = Mechanism::new(c).unwrap();
    let o = Observation {
        load_nm: 20.0,
        ..obs(0.0, 1.0)
    };
    m.tick(o);
    assert!(!m.state.latch_engaged);
    m.operate(Operation::Lock, obs(0.0, 2.0), true);
    assert!(m.tick(o).retention_nm > 0.0);
}
#[test]
fn egress_is_directional_and_keys_are_not_consumed() {
    let c = MechanismConfig {
        key_item: Some("key.office".into()),
        free_egress_side: Some(-1),
        ..Default::default()
    };
    assert!(c.access(-1.0, false));
    assert!(!c.access(1.0, false));
    assert!(c.access(1.0, true));
}
#[test]
fn obstruction_expires_effort_and_occupied_portals_suppress_closing() {
    let mut m = Mechanism::new(MechanismConfig::default()).unwrap();
    m.operate(Operation::Open { target_angle: 1.5 }, obs(0.0, 0.0), true);
    m.tick(obs(0.0, 7.0));
    assert_eq!(m.state.last_result, DoorResult::Obstructed);
    assert!(m.state.goal.is_none());
    let o = Observation {
        occupied: true,
        ..obs(1.5, 8.0)
    };
    assert_eq!(m.operate(Operation::Close, o, true), DoorResult::Busy);
    assert!(m.tick(o).target.is_none());
}
#[test]
fn damage_and_saved_states_preserve_broken_hardware() {
    let mut m = Mechanism::new(MechanismConfig::default()).unwrap();
    m.operate(Operation::Lock, obs(0.0, 0.0), true);
    m.tick(Observation {
        impact_joules: 500.0,
        ..obs(0.0, 0.1)
    });
    assert!(m.state.bolt_broken);
    assert!(!m.state.locked);
    let saved: MechanismState =
        serde_json::from_str(&serde_json::to_string(&m.state).unwrap()).unwrap();
    let mut restored = Mechanism::new(m.config.clone()).unwrap();
    restored.restore(saved, obs(0.0, 10.0)).unwrap();
    assert_eq!(
        restored.operate(Operation::Lock, obs(0.0, 11.0), true),
        DoorResult::Broken
    );
    assert_eq!(
        restored.operate(Operation::Repair, obs(0.0, 12.0), true),
        DoorResult::Accepted
    );
}
#[test]
fn passage_geometry_keeps_finite_edges_and_mirrored_rotations() {
    assert_eq!(
        segment_distance(
            [0.0, 0.0, -1.0],
            [0.0, 0.0, 1.0],
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0]
        ),
        0.0
    );
    assert!(
        (segment_distance(
            [2.0, 0.0, -1.0],
            [2.0, 0.0, 1.0],
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0]
        ) - 1.0)
            .abs()
            < 1e-6
    );
    let q = |a: f32| [0.0, (a / 2.0).sin(), 0.0, (a / 2.0).cos()];
    assert!((twist(q(2.8), q(-2.9), q(0.0)) - (std::f32::consts::TAU - 5.7)).abs() < 1e-5);
}

#[test]
fn retained_hardware_uses_incident_energy_even_when_ccd_reports_zero_impulse() {
    assert!((incident_energy(36.0, 2.0, 0.75) - 40.5).abs() < 0.001);
    assert!(incident_energy(80.0, 7.0, 0.95 / 1.2) > 450.0);
    assert_eq!(incident_energy(80.0, -7.0, 0.8), 0.0);
    assert_eq!(incident_energy(80.0, 0.0, 0.8), 0.0);
    assert_eq!(incident_energy(80.0, 7.0, 0.0), 0.0);
    assert!(incident_energy(f32::MAX, f32::MAX, 1.0).is_finite());
}

#[test]
fn excess_impact_work_breaks_both_retained_parts_and_is_released_once() {
    let mut m = Mechanism::new(MechanismConfig {
        initially_locked: true,
        ..Default::default()
    })
    .unwrap();
    let a = m.tick(Observation {
        impact_joules: 1200.0,
        ..obs(0.0, 1.0)
    });
    assert!(m.state.bolt_broken && m.state.latch_broken);
    assert!(!m.state.locked && !m.state.latch_engaged);
    assert!((a.released_joules - 630.0).abs() < 0.001);
    assert_eq!(m.tick(obs(0.0, 2.0)).released_joules, 0.0);
    let impulse = fracture_impulse(a.released_joules, 12.0, 0.95, 560.0);
    assert!((impulse * 0.95).powi(2) / (2.0 * 12.0) <= a.released_joules + 0.01);
    assert!(impulse <= 560.0);
}

#[test]
fn elastic_energy_is_spent_once_across_bolt_and_latch() {
    let mut m = Mechanism::new(MechanismConfig {
        initially_locked: true,
        ..Default::default()
    })
    .unwrap();
    let a = m.tick(Observation {
        impact_joules: 100.0,
        ..obs(0.1, 1.0)
    });
    assert!(m.state.bolt_broken && !m.state.latch_broken);
    assert!((m.state.latch_damage_joules - 50.0).abs() < 0.01);
    assert_eq!(a.released_joules, 0.0);
}
