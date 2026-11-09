use super::*;
fn profile() -> FootMotionProfile {
    serde_json::from_value(
        serde_json::json!({"contact_height_ratio":0.2,"lift_off_speed_ratio":0.4,"stance_speed_ratio":0.5,
        "transition_seconds":0.09,"correction_seconds":0.08,"anticipation_seconds":0.22,"pelvis_drop_ratio":0.7,"speed_ratio":1.5,
        "acceleration_ratio":8.0,"plant_tolerance_ratio":0.02,"anchor_engage_weight":0.8,
        "anchor_release_weight":0.2,"anchor_reach_ratio":0.97,"anchor_turn_degrees":65.0,"roll_compliance_degrees":8.0}),
    )
    .unwrap()
}
fn sample(dt: f32) -> FootMotionInput {
    FootMotionInput {
        authored: [0.0, 0.1, 0.0],
        relative_height: -0.8,
        root: [0.0, 0.9, 0.0],
        root_velocity: [0.0; 3],

        sole_length: 0.28,
        leg_length: 0.8,
        skin: 0.015,
        gravity: 9.81,
        dt,
    }
}
#[test]
fn swinging_foot_keeps_authored_horizontal_path_and_releases_contact() {
    let p = profile();
    p.validate().unwrap();
    let mut s = FootMotionState::default();
    let mut i = sample(1.0 / 120.0);
    s.update(i, p, Some(i.authored), false, [0.0; 3]);
    for frame in 1..120 {
        i.authored[0] = frame as f32 * 0.01;
        i.relative_height = -0.64;
        let o = s.update(
            i,
            p,
            Some([i.authored[0] + 0.25, 0.3, 0.0]),
            false,
            [0.0; 3],
        );
        if frame > 90 {
            assert!((o.target[0] - i.authored[0]).abs() < 1e-4);
            assert!(o.contact < 1e-5);
        }
    }
}
#[test]
fn correction_to_a_new_riser_has_bounded_speed_and_acceleration() {
    let p = profile();
    let mut s = FootMotionState::default();
    let i = sample(1.0 / 120.0);
    s.update(i, p, Some(i.authored), false, [0.0; 3]);
    let mut previous = i.authored;
    let mut velocity = [0.0; 3];
    for _ in 0..120 {
        let o = s.update(i, p, Some([0.1, 0.28, 0.0]), false, [0.0; 3]);
        assert!(
            length(std::array::from_fn(|k| o.target[k] - previous[k]))
                <= p.speed_ratio * (i.gravity * i.leg_length).sqrt() * i.dt + 1e-6
        );
        assert!(
            length(std::array::from_fn(
                |k| o.correction_velocity[k] - velocity[k]
            )) <= p.acceleration_ratio * i.gravity * i.dt + 1e-4
        );
        previous = o.target;
        velocity = o.correction_velocity;
    }
    assert!(length([previous[0] - 0.1, previous[1] - 0.28, previous[2]]) < 1e-5);
}
#[test]
fn loaded_contact_resists_authored_sliding_and_follows_its_support() {
    let p = profile();
    let mut s = FootMotionState::default();
    let mut i = sample(1.0 / 120.0);
    let mut goal = i.authored;
    s.update(i, p, Some(goal), false, [0.0; 3]);
    for _ in 0..120 {
        i.authored[2] += 0.001;
        let o = s.update(i, p, Some(goal), true, [0.0; 3]);
        assert!((o.target[2] - goal[2]).abs() < 1e-5);
    }
    for _ in 0..120 {
        goal[0] += 0.001;
        i.authored[0] += 0.001;
        let o = s.update(i, p, Some(goal), true, [0.001, 0.0, 0.0]);
        assert!(length(std::array::from_fn(|k| o.target[k] - goal[k])) < 1e-5);
    }
}
#[test]
fn disappearing_support_releases_offset_and_contact() {
    let p = profile();
    let mut s = FootMotionState::default();
    let i = sample(1.0 / 120.0);
    s.update(i, p, Some([0.1, 0.28, 0.0]), false, [0.0; 3]);
    for _ in 0..120 {
        s.update(i, p, None, false, [0.0; 3]);
    }
    let o = s.update(i, p, None, false, [0.0; 3]);
    assert!(length(std::array::from_fn(|k| o.target[k] - i.authored[k])) < 1e-5);
    assert!(o.contact < 1e-5);
}
#[test]
fn invalid_motion_profiles_are_rejected() {
    let mut p = profile();
    p.correction_seconds = 0.0;
    assert!(p.validate().is_err());
    let mut p = profile();
    p.anchor_engage_weight = p.anchor_release_weight;
    assert!(p.validate().is_err());
    let mut p = profile();
    p.acceleration_ratio = f32::NAN;
    assert!(p.validate().is_err());
}

#[test]
fn rising_pelvis_does_not_drag_a_loaded_foot_up_with_the_capsule() {
    let p = profile();
    let mut s = FootMotionState::default();
    let mut i = sample(1.0 / 120.0);
    let goal = i.authored;
    s.update(i, p, Some(goal), false, [0.0; 3]);
    for _ in 0..120 {
        i.authored[1] += 0.001;
        i.root[1] += 0.001;
        let output = s.update(i, p, Some(goal), false, [0.0; 3]);
        assert!((output.target[1] - goal[1]).abs() < 1e-5);
    }
}

#[test]
fn released_anchor_cannot_keep_pulling_a_free_swing_sideways() {
    let p = profile();
    let mut state = FootMotionState::default();
    let mut i = sample(1.0 / 120.0);
    let goal = i.authored;
    state.update(i, p, Some(goal), false, [0.0; 3]);
    for _ in 0..80 {
        i.authored[0] += 0.003;
        state.update(i, p, Some(goal), true, [0.0; 3]);
    }

    i.relative_height = -0.62;
    for _ in 0..80 {
        i.authored[0] += 0.005;
        let result = state.update(i, p, Some([i.authored[0] + 0.2, 0.3, 0.0]), false, [0.0; 3]);
        if result.contact < 0.05 {
            assert!((result.target[0] - i.authored[0]).abs() < i.sole_length * 0.15);
        }
    }
}

#[test]
fn render_fixed_step_aliasing_is_not_mistaken_for_a_swing() {
    let p = profile();
    let mut s = FootMotionState::default();
    let mut i = sample(1.0 / 200.0);
    i.root_velocity = [0.0, 0.0, -1.25];
    s.update(i, p, Some(i.authored), false, [0.0; 3]);
    let mut local = 0.0;
    for frame in 1..120 {
        let root_z = -((frame as f32 / 200.0) * 120.0).floor() / 120.0 * 1.25;
        local += 1.25 / 200.0;
        i.root[2] = root_z;
        i.authored[2] = root_z + local;
        let velocity = s.authored_velocity(i);
        assert!(length(velocity) < 0.0001);
        s.update(i, p, Some(i.authored), false, [0.0; 3]);
    }
}

#[test]
fn the_lower_foot_is_still_a_swing_when_it_is_moving_fast() {
    let p = profile();
    let mut state = FootMotionState::default();
    let mut i = sample(1.0 / 120.0);
    state.update(i, p, Some(i.authored), false, [0.0; 3]);
    i.authored[2] -= 0.04;
    assert!(state.demand(i, p, true, false) < 1e-5);
}

#[test]
fn each_leg_retains_its_own_swing_height_when_it_is_the_lower_of_two_feet() {
    let p = profile();
    let mut state = FootMotionState::default();
    let mut i = sample(1.0 / 120.0);
    state.update(i, p, Some(i.authored), false, [0.0; 3]);
    i.relative_height += 0.18;
    i.authored[1] += 0.18;

    assert!((state.lift(i) - 0.18).abs() < 1e-6);
}

#[test]
fn a_small_sole_collision_correction_does_not_amplify_when_swing_weight_is_zero() {
    let p = profile();
    let mut state = FootMotionState::default();
    let mut i = sample(1.0 / 120.0);
    state.update(i, p, Some(i.authored), false, [0.0; 3]);
    i.relative_height += 0.2;
    for _ in 0..60 {
        state.update(i, p, Some(i.authored), false, [0.0; 3]);
    }
    state.project_horizontal([0.003, 0.0, 0.0]);
    assert!((state.estimate(i.authored)[0] - i.authored[0] - 0.003).abs() < 1e-6);
    for _ in 0..120 {
        state.update(i, p, Some(i.authored), false, [0.0; 3]);
    }
    assert!((state.estimate(i.authored)[0] - i.authored[0]).abs() < 1e-5);
}

#[test]
fn landing_preparation_does_not_claim_a_loaded_contact() {
    let p = profile();
    let mut state = FootMotionState::default();
    let mut i = sample(1.0 / 120.0);
    i.root_velocity = [4.0, 0.0, 0.0];
    for _ in 0..120 {
        i.root[0] += 4.0 * i.dt;
        i.authored[0] += 4.0 * i.dt;
        let output = state.update(
            i,
            p,
            Some([i.authored[0] + 0.06, i.authored[1], 0.0]),
            false,
            [0.0; 3],
        );
        assert!(output.contact < 0.001);
        assert!(output.landing_weight > 0.99);
    }
}
