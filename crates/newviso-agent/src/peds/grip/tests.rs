use super::*;
fn model() -> WeaponGripModel {
    serde_json::from_value(json!({
        "schema":"newviso.ped-grip-model.v1","hand_force":[100,100],"wrist_torque":[8,8],
        "slip_travel":0.02,"slip_stiffness":100,"slip_damping":10,
        "vitality_exponent":1.5,"stamina_exponent":0.7,"wound_exponent":6,
        "pain_gain":12,"pain_decay":1.8,"motion_response_seconds":0.08,
        "collection_reach_fraction":1,"holding_joint":"hand",
        "injury_coupling":{"body":[0.45,0.45],"arm":[1,1],"right_arm":[1,0],"left_arm":[0,1]},
        "reaction_support":{"animated":[1,1]},
        "disarmed_decision":{"fight":1,"escape":1,"injury_aversion":2,"distance_aversion":1}
    }))
    .unwrap()
}
fn ped() -> PedRuntime {
    let mut p = PedRuntime::default();
    p.upsert(
        PedProfile {
            actor_id: "npc".into(),
            agent_id: "npc.ai".into(),
            armed: true,
            max_health: 100.,
            weapon_grip: WeaponGripProfile {
                model: Some(model()),
                ..Default::default()
            },
            combat: Some(combat::CombatProfile {
                target_actor: "player".into(),
                ..Default::default()
            }),
            ..Default::default()
        },
        true,
    )
    .unwrap();
    p
}
fn load(force: f32) -> WeaponGripLoad {
    WeaponGripLoad {
        force,
        torque: 0.0,
        support_span: 0.3,
        support: [1., 1.],
    }
}
#[test]
fn equal_injury_does_not_prescribe_equal_outcomes_for_different_weapons() {
    let mut light = ped();
    let mut heavy = ped();
    for p in [&mut light, &mut heavy] {
        p.damage_with_impact(
            "npc",
            "attacker".into(),
            20.,
            [0.; 3],
            GripImpact::Body,
            &mut AgentRuntime::default(),
        )
        .unwrap();
        assert!(
            !p.state("npc").unwrap().weapon_grip.lost,
            "damage itself must not decide release"
        );
    }
    for _ in 0..30 {
        light
            .advance_weapon_grip("npc", "light", 1, load(9.81), 0.01)
            .unwrap();
        heavy
            .advance_weapon_grip("npc", "heavy", 1, load(120.), 0.01)
            .unwrap();
    }
    assert!(!light.state("npc").unwrap().weapon_grip.lost);
    assert!(heavy.state("npc").unwrap().weapon_grip.lost);
    assert_eq!(
        light.state("npc").unwrap().health,
        heavy.state("npc").unwrap().health
    );
}
#[test]
fn directional_wounds_and_guard_are_data_driven_and_armour_does_not_invent_injury() {
    let mut p = ped();
    p.damage_with_impact(
        "npc",
        "a".into(),
        10.,
        [0.; 3],
        GripImpact::LeftArm,
        &mut AgentRuntime::default(),
    )
    .unwrap();
    assert_eq!(p.state("npc").unwrap().weapon_grip.wound, [0., 0.1]);
    p.damage_with_impact(
        "npc",
        "a".into(),
        20.,
        [0.; 3],
        GripImpact::Suppressed,
        &mut AgentRuntime::default(),
    )
    .unwrap();
    assert_eq!(p.state("npc").unwrap().weapon_grip.wound, [0., 0.1]);
    let mut p = ped();
    p.records.get_mut("npc").unwrap().armour = 50.;
    p.damage_with_impact(
        "npc",
        "a".into(),
        40.,
        [0.; 3],
        GripImpact::Arm,
        &mut AgentRuntime::default(),
    )
    .unwrap();
    assert_eq!(p.state("npc").unwrap().weapon_grip.wound, [0.; 2]);
}
#[test]
fn hold_failure_depends_on_force_torque_stamina_and_actual_hand_support() {
    let m = model();
    let mut healthy = WeaponGripState::default();
    assert!(!healthy.advance(&m, load(40.), 1., 1., 0.2));
    let mut tired = WeaponGripState::default();
    assert!(tired.advance(&m, load(40.), 1., 0.001, 0.2));
    let mut unsupported = WeaponGripState::default();
    let mut no_hands = load(40.);
    no_hands.support = [0.; 2];
    assert!(unsupported.advance(&m, no_hands, 1., 1., 0.2));
    let mut rotating = WeaponGripState::default();
    let mut torque = load(1.);
    torque.torque = 400.;
    assert!(rotating.advance(&m, torque, 1., 1., 0.2));
}
#[test]
fn constant_load_slip_is_invariant_to_frame_partition_and_pain_recovers() {
    let m = model();
    let run = |steps: u32| {
        let mut state = WeaponGripState::default();
        for _ in 0..steps {
            state.advance(&m, load(300.), 1., 1., 0.005 / steps as f32);
        }
        state
    };
    let a = run(6);
    let b = run(48);
    assert!((a.slip - b.slip).abs() < 1e-5);
    let mut state = WeaponGripState::default();
    state.pain = [0.5; 2];
    let pain = state.pain;
    for _ in 0..60 {
        state.advance(&m, load(0.), 1., 1., 1. / 60.);
    }
    assert!(state.pain[0] < pain[0]);
    assert!(state.slip == 0.);
}
#[test]
fn reequipping_a_new_generation_retains_wounds_and_allows_another_release() {
    let mut p = ped();
    p.advance_weapon_grip("npc", "gun", 10, load(1.), 0.01)
        .unwrap();
    assert!(p.release_weapon_grip("npc", GripLossReason::Action));
    assert!(!p.release_weapon_grip("npc", GripLossReason::Action));
    p.records.get_mut("npc").unwrap().weapon_grip.wound = [0.05, 0.0];
    p.advance_weapon_grip("npc", "gun", 11, load(1.), 0.01)
        .unwrap();
    let s = &p.state("npc").unwrap().weapon_grip;
    assert!(!s.lost);
    assert_eq!(s.wound, [0.05, 0.]);
    assert!(p.release_weapon_grip("npc", GripLossReason::Action));
    assert_eq!(p.state("npc").unwrap().weapon_grip.release_sequence, 2);
}
#[test]
fn release_blocks_an_active_shooter_and_survives_save_without_global_life_latches() {
    let mut p = ped();
    let c = p.records.get_mut("npc").unwrap().combat.as_mut().unwrap();
    c.mode = "engage".into();
    c.visible = true;
    assert!(p.combat_shot("npc", 1.));
    p.release_weapon_grip("npc", GripLossReason::Action);
    assert!(!p.combat_shot("npc", 100.));
    let state = p.state("npc").unwrap().weapon_grip.clone();
    let profile = p.state("npc").unwrap().profile.clone();
    p.upsert(profile.clone(), false).unwrap();
    assert!(p.state("npc").unwrap().weapon_grip.lost);
    p.upsert(profile, true).unwrap();
    assert!(!p.state("npc").unwrap().weapon_grip.lost);
    p.restore_weapon_grip("npc", state).unwrap();
    assert!(p.state("npc").unwrap().weapon_grip.lost);
}
#[test]
fn invalid_models_and_nonfinite_loads_fail_before_changing_state() {
    let mut m = model();
    m.hand_force[0] = f32::NAN;
    assert!(m.validate().is_err());
    let mut p = ped();
    let before = p.runtime_state();
    let mut input = load(1.);
    input.torque = f32::INFINITY;
    assert!(p.advance_weapon_grip("npc", "gun", 1, input, 0.1).is_err());
    assert_eq!(p.runtime_state(), before);
}

#[test]
fn empty_hands_recover_support_without_restoring_ownership_or_healing_wounds() {
    let mut p = ped();
    p.damage_with_impact(
        "npc",
        "a".into(),
        20.,
        [0.; 3],
        GripImpact::Arm,
        &mut AgentRuntime::default(),
    )
    .unwrap();
    p.advance_weapon_grip("npc", "gun", 1, load(9.81), 0.01)
        .unwrap();
    p.release_weapon_grip("npc", GripLossReason::Action);
    let old = p.state("npc").unwrap().weapon_grip.clone();
    for _ in 0..60 {
        p.advance_unheld_weapon_grip("npc", [1.; 2], 1. / 60.)
            .unwrap();
    }
    let new = &p.state("npc").unwrap().weapon_grip;
    assert!(new.pain[0] < old.pain[0]);
    assert!(new.support_quality > old.support_quality);
    assert_eq!(new.wound, old.wound);
    assert!(new.lost);
    assert_eq!(new.release_sequence, old.release_sequence);
    assert!(!p.combat_shot("npc", 100.));
}
#[test]
fn restored_transient_pain_uses_elapsed_world_time_and_rejects_invalid_time() {
    let mut p = ped();
    p.records.get_mut("npc").unwrap().weapon_grip.pain = [0.5; 2];
    p.records.get_mut("npc").unwrap().weapon_grip.wound = [0.1, 0.2];
    p.recover_weapon_grip_elapsed("npc", 10.).unwrap();
    let state = &p.state("npc").unwrap().weapon_grip;
    assert!(state.pain[0] < 1e-6);
    assert_eq!(state.wound, [0.1, 0.2]);
    assert!(p.recover_weapon_grip_elapsed("npc", f64::NAN).is_err());
}
