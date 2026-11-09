use super::*;
fn profile() -> ProtectionProfile {
    ProtectionProfile {
        head_joint: "crown".into(),
        forward_axis_local: [0.0, 0.0, 1.0],
        arms: ["a", "b"]
            .map(|s| ProtectionArm {
                upper_joint: format!("{s}.upper"),
                lower_joint: format!("{s}.lower"),
                hand_joint: format!("{s}.hand"),
                contact_joint: format!("{s}.palm"),
                palm_normal_local: [0.0, 0.0, 1.0],
                palm_up_local: [0.0, 1.0, 0.0],
                trauma_parts: vec![],
                open_hand: vec![],
            })
            .to_vec(),
        tuning: ProtectionTuning {
            horizon_scale: 3.0,
            raise_time_scale: 0.3,
            release_time_scale: 2.0,
            impact_speed_fraction: 0.12,
            reach_fraction: 0.95,
            hand_radius_fraction: 0.06,
            sensor_radius_fraction: 0.5,
            elbow_outward_fraction: 0.6,
            elbow_drop_fraction: 0.2,
            forward_clearance_fraction: 0.08,
        },
    }
}
fn body(scale: f32) -> ProtectionObservation {
    ProtectionObservation {
        head: [0.0, 1.7 * scale, 0.0],
        up: [0.0, 1.0, 0.0],
        forward: [0.0, 0.0, 1.0],
        gravity: 9.81,
        arms: [-0.2, 0.2]
            .map(|x| ProtectiveArmObservation {
                shoulder: [x * scale, 1.4 * scale, 0.0],
                elbow: [x * scale, 1.1 * scale, 0.02 * scale],
                wrist: [x * scale, 0.85 * scale, 0.0],
                palm_offset_local: [0.0, 0.1 * scale, 0.0],
                capacity: 1.0,
            })
            .to_vec(),
    }
}
fn threat(point: [f32; 3], normal: [f32; 3]) -> ImpactThreat {
    ImpactThreat {
        entity: Some(7),
        point,
        normal,
        surface_velocity: [0.0; 3],
        closing_speed: 3.0,
        time_to_impact: 0.02,
    }
}
#[test]
fn protective_targets_fit_measured_limbs_and_face_the_impact_plane_at_all_body_scales() {
    let p = profile();
    p.validate().unwrap();
    for scale in [0.1, 1.0, 3.0] {
        let b = body(scale);
        let mut s = ProtectionState::default();
        for _ in 0..20 {
            s.observe(
                &p,
                &b,
                Some(threat([0.0, 1.4 * scale, 0.45 * scale], [0.0, 0.0, -1.0])),
                0.01,
            );
        }
        assert!(s.active());
        assert_eq!(s.arms.len(), 2);
        for g in &s.arms {
            let a = &b.arms[g.arm];
            assert!(
                math::length(math::sub(g.wrist, a.shoulder))
                    <= a.reach() * p.tuning.reach_fraction + 1e-6
            );
            assert!(
                math::dot(
                    math::rotate(g.hand_rotation, p.arms[g.arm].palm_normal_local),
                    [0.0, 0.0, 1.0]
                ) > 0.999
            );
            assert!(g.palm[2] < 0.45 * scale);
            assert!(g.weight > 0.1);
        }
    }
}
#[test]
fn rear_and_ceiling_impacts_protect_without_putting_wrists_behind_the_body() {
    for t in [
        threat([0.0, 1.4, -0.2], [0.0, 0.0, 1.0]),
        threat([0.0, 1.8, 0.0], [0.0, -1.0, 0.0]),
    ] {
        let p = profile();
        let b = body(1.0);
        let mut s = ProtectionState::default();
        s.observe(&p, &b, Some(t), 0.1);
        for g in &s.arms {
            assert!(g.wrist[2] >= b.arms[g.arm].shoulder[2]);
            assert!(g
                .hand_rotation
                .iter()
                .chain(&g.wrist)
                .all(|v| v.is_finite()));
        }
    }
}
#[test]
fn damage_weakens_only_its_arm_and_release_finishes_at_every_frame_rate() {
    for hz in [30.0, 60.0, 144.0, 240.0] {
        let p = profile();
        let mut b = body(1.0);
        b.arms[0].capacity = 0.25;
        let mut s = ProtectionState::default();
        for _ in 0..(hz as usize) {
            s.observe(
                &p,
                &b,
                Some(threat([0.0, 1.4, 0.45], [0.0, 0.0, -1.0])),
                1.0 / hz,
            );
        }
        assert!((s.arms[0].weight / s.arms[1].weight - 0.5).abs() < 1e-5);
        for _ in 0..(hz as usize * 3) {
            s.observe(&p, &b, None, 1.0 / hz);
        }
        assert!(!s.active() && s.arms.is_empty() && s.activation == 0.0);
    }
}
#[test]
fn nonfinite_bindings_and_observations_do_not_create_reaching_goals() {
    let mut p = profile();
    p.arms[0].palm_normal_local = [0.0; 3];
    assert!(p.validate().is_err());
    let p = profile();
    let mut b = body(1.0);
    b.arms[0].wrist[0] = f32::NAN;
    let mut s = ProtectionState::default();
    s.observe(&p, &b, Some(threat([0.0, 1.4, 0.4], [0.0, 0.0, -1.0])), 0.1);
    assert!(!s.active());
    let mut t = threat([0.0; 3], [0.0, 0.0, -1.0]);
    t.closing_speed = 0.0;
    s.observe(&p, &body(1.0), Some(t), 0.1);
    assert!(!s.active());
}

#[test]
fn anatomical_front_survives_yaw_pitch_roll_and_different_body_sizes() {
    for scale in [0.1, 1.0, 3.0] {
        for q in [
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 0.70710677, 0.0, 0.70710677],
            [0.70710677, 0.0, 0.0, 0.70710677],
            [0.0, 0.0, 0.70710677, 0.70710677],
        ] {
            let body = body(scale);
            let up = math::rotate(q, body.up);
            let right = math::rotate(q, body.arms[0].shoulder);
            let left = math::rotate(q, body.arms[1].shoulder);
            let forward = protection_forward(up, right, left).unwrap();
            assert!(math::dot(forward, math::rotate(q, body.forward)) > 0.999);
        }
    }
    assert!(protection_forward([0.0, 1.0, 0.0], [0.0; 3], [0.0; 3]).is_none());
}
#[test]
fn rear_head_guards_keep_their_palms_outside_the_head() {
    let p = profile();
    let b = body(1.0);
    let mut s = ProtectionState::default();
    s.observe(&p, &b, Some(threat([0.0, 1.4, -0.2], [0.0, 0.0, 1.0])), 0.1);
    for g in &s.arms {
        let side = math::unit([b.arms[g.arm].shoulder[0], 0.0, 0.0]).unwrap();
        assert!(math::dot(g.palm_normal, side) > 0.999);
        assert!(g.wrist[2] >= 0.0);
        assert!(g.palm[0] * side[0] > 0.1);
    }
}

#[test]
fn moving_surface_release_tracks_the_contact_plane_at_every_frame_rate() {
    let p = profile();
    let b = body(1.0);
    for hz in [30.0_f32, 60.0, 144.0, 240.0] {
        let mut s = ProtectionState::default();
        let mut t = threat([0.0, 1.4, 0.3], [0.0, 0.0, -1.0]);
        t.surface_velocity = [0.0, 0.0, 0.1];
        t.time_to_impact = 0.2;
        for _ in 0..20 {
            s.observe(&p, &b, Some(t.clone()), 0.01);
        }
        let initial_wrist = s.arms[0].wrist;
        let initial_activation = s.activation;
        let steps = (hz / 10.0).round() as usize;
        for _ in 0..steps {
            s.observe(&p, &b, None, 1.0 / hz);
        }
        let elapsed = steps as f32 / hz;
        let retained = s.threat.as_ref().unwrap();
        assert_eq!(s.phase, "release");
        assert!(s.activation > 0.0 && s.activation < initial_activation);
        assert!((retained.point[2] - (0.3 + 0.1 * elapsed)).abs() < 1e-5);
        assert!((retained.time_to_impact - (0.2 - elapsed)).abs() < 1e-5);
        assert!((s.arms[0].wrist[2] - initial_wrist[2] - 0.1 * elapsed).abs() < 1e-5);
        for _ in 0..(hz as usize * 3) {
            s.observe(&p, &b, None, 1.0 / hz);
        }
        assert!(!s.active() && s.threat.is_none() && s.arms.is_empty());
    }
}

#[test]
fn fresh_contact_observations_replace_prediction_without_double_advection() {
    let p = profile();
    let b = body(1.0);
    let mut s = ProtectionState::default();
    let mut first = threat([0.0, 1.4, 0.3], [0.0, 0.0, -1.0]);
    first.surface_velocity = [0.0, 0.0, 0.1];
    s.observe(&p, &b, Some(first), 0.1);
    s.observe(&p, &b, None, 0.01);
    let mut current = threat([0.0, 1.4, 0.35], [0.0, 0.0, -1.0]);
    current.entity = Some(8);
    current.surface_velocity = [0.0, 0.0, -0.2];
    current.time_to_impact = 0.05;
    s.observe(&p, &b, Some(current.clone()), 0.01);
    let observed = s.threat.as_ref().unwrap();
    assert_eq!(observed.entity, current.entity);
    assert_eq!(observed.point, current.point);
    assert_eq!(observed.surface_velocity, current.surface_velocity);
    assert_eq!(observed.time_to_impact, current.time_to_impact);
}

#[test]
fn reused_protection_frame_follows_the_current_body_observation() {
    let p = profile();
    let mut b = body(1.0);
    let mut s = ProtectionState::default();
    s.observe(&p, &b, Some(threat([0.0, 1.4, 0.3], [0.0, 0.0, -1.0])), 0.1);
    let q = [0.0, 0.70710677, 0.0, 0.70710677];
    let offset = [2.0, 0.5, -3.0];
    b.head = math::add(math::rotate(q, b.head), offset);
    b.up = math::rotate(q, b.up);
    b.forward = math::rotate(q, b.forward);
    for a in &mut b.arms {
        a.shoulder = math::add(math::rotate(q, a.shoulder), offset);
        a.elbow = math::add(math::rotate(q, a.elbow), offset);
        a.wrist = math::add(math::rotate(q, a.wrist), offset);
    }
    s.observe(&p, &b, None, 0.01);
    let frame = s.frame.as_ref().unwrap();
    assert!(math::dot(frame.up, b.up) > 0.999);
    assert!(math::dot(frame.forward, b.forward) > 0.999);
    assert_eq!(
        frame.shoulders,
        b.arms.iter().map(|a| a.shoulder).collect::<Vec<_>>()
    );
}
