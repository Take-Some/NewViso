use super::*;

#[test]
fn support_inverse_dynamics_transmits_the_measured_foot_load_through_the_joint() {
    let plan = build(pose());
    let contacts = [SupportContact {
        part: 1,
        position: [0.0; 3],
    }];
    let state = BalanceState {
        supported: true,
        uprightness: 1.0,
        body_up: [0.0, 1.0, 0.0],
        support_center: [0.0; 3],
        ..Default::default()
    };
    let result = plan
        .state
        .support_plan(
            &contacts,
            &state,
            plan.center_of_mass,
            [0.0; 3],
            [0.0; 3],
            9.81,
            BalanceGains::default(),
            |id| plan.bodies.iter().find(|b| b.entity == id).map(|b| b.pose),
        )
        .unwrap();
    let ground = [0.0, 70.0 * 9.81, 0.0];
    let j = plan.state.joints[0];
    let child = &plan.bodies[j.child_part];
    let anchor = math::add(
        child.pose.position,
        math::rotate(child.pose.rotation, j.child_anchor),
    );
    let expected = -math::add(
        math::cross(
            math::sub(child.pose.position, anchor),
            [0.0, -child.mass_kg * 9.81, 0.0],
        ),
        math::cross(math::sub(contacts[0].position, anchor), ground),
    )[2];
    let residual = math::cross(math::sub(contacts[0].position, plan.center_of_mass), ground);
    assert!(math::length(math::sub(result.root_moment, residual)) < 0.001);
    assert!((result.joint_torques[0][2] - expected).abs() < 0.001);
    assert_eq!(result.center_of_pressure, [0.0; 3]);
    let double = plan
        .state
        .support_plan(
            &contacts,
            &state,
            plan.center_of_mass,
            [0.0; 3],
            [0.0; 3],
            19.62,
            BalanceGains::default(),
            |id| plan.bodies.iter().find(|b| b.entity == id).map(|b| b.pose),
        )
        .unwrap();
    assert!((double.joint_torques[0][2] - expected * 2.0).abs() < 0.002);
    assert!(plan
        .state
        .support_plan(
            &[],
            &state,
            plan.center_of_mass,
            [0.0; 3],
            [0.0; 3],
            9.81,
            BalanceGains::default(),
            |_| None
        )
        .is_none());
}

#[test]
fn stance_hip_couples_control_torso_without_inventing_a_root_torque() {
    let mut plan = build(pose());
    plan.state.stance_entities = vec![plan.state.parts[1].entity];
    let com = plan.center_of_mass;
    let contacts: Vec<_> = [[-0.4, -0.3], [0.4, -0.3], [0.4, 0.3], [-0.4, 0.3]]
        .iter()
        .map(|p| SupportContact {
            part: 1,
            position: [com[0] + p[0], 0.0, com[2] + p[1]],
        })
        .collect();
    let state = BalanceState {
        supported: true,
        uprightness: 1.0,
        body_up: [0.05, 0.9987492, 0.0],
        support_center: [com[0], 0.0, com[2]],
        ..Default::default()
    };
    let run = |spin: f32| {
        plan.state
            .support_plan(
                &contacts,
                &state,
                com,
                [0.0; 3],
                [0.0; 3],
                9.81,
                BalanceGains::default(),
                |id| {
                    plan.bodies.iter().find(|b| b.entity == id).map(|b| {
                        let mut p = b.pose;
                        p.angular_velocity = [0.0, 0.0, spin];
                        p
                    })
                },
            )
            .unwrap()
    };
    let static_plan = run(0.0);
    assert_eq!(static_plan.stance_hips, vec![0]);
    assert!((static_plan.center_of_pressure[0] - com[0]).abs() < 0.0001);
    assert!(math::length(static_plan.root_moment) < 0.001);
    // The core exports an implicit parent goal; it does not add an explicit
    // outer-loop root torque to the gravity compensation.
    let drive = static_plan.world_drives[0].unwrap();
    assert!(drive.stiffness > 0.0 && drive.damping > 0.0);
    let root = plan
        .bodies
        .iter()
        .find(|b| b.entity == plan.state.parts[0].entity)
        .unwrap();
    let correction = math::multiply(drive.target_rotation, math::inverse(root.pose.rotation));
    assert!(correction[2] > 0.0);
    let up = math::rotate(correction, state.body_up);
    assert!((up[1] - 1.0).abs() < 0.0001);
    assert!(static_plan.world_drives.iter().skip(1).all(Option::is_none));
}
#[test]
fn planted_foot_goals_follow_support_motion_and_fade_continuously_after_contact_loss() {
    let mut plan = build(pose());
    plan.state.joints[0].child_anchor = [0.0, 0.2, 0.0];
    let mut plants = FootPlantState::default();
    let contacts = [SupportContact {
        part: 1,
        position: [0.3, 1.35, 0.0],
    }];
    let sample = |id| plan.bodies.iter().find(|b| b.entity == id).map(|b| b.pose);
    let still = plants.correct(
        &plan.state,
        &contacts,
        [0.0; 3],
        9.81,
        1.8,
        1.0,
        0.0,
        BalanceGains::default(),
        0.8,
        0.01,
        sample,
    );
    assert!(still.iter().all(|v| math::length(*v) < 1e-6));
    let original = plants.targets[&1].position;
    let displaced = |id| {
        sample(id).map(|mut p| {
            if id == 2 {
                p.position[0] += 0.1;
            }
            p
        })
    };
    let correction = plants.correct(
        &plan.state,
        &contacts,
        [0.0; 3],
        9.81,
        1.8,
        1.0,
        0.0,
        BalanceGains::default(),
        0.8,
        0.01,
        displaced,
    );
    assert!(
        correction[0][2] < 0.0,
        "the leg must oppose forward foot slip"
    );
    assert_eq!(plants.targets[&1].position, original);
    plants.correct(
        &plan.state,
        &[],
        [1.0, 0.0, 0.0],
        9.81,
        1.8,
        1.0,
        0.0,
        BalanceGains::default(),
        0.8,
        0.05,
        sample,
    );
    assert!((plants.targets[&1].position[0] - original[0] - 0.05).abs() < 1e-6);
    assert!(plants.targets[&1].confidence > 0.0 && plants.targets[&1].confidence < 1.0);
    let release = plants.correct(
        &plan.state,
        &[],
        [0.0; 3],
        9.81,
        1.8,
        1.0,
        1.0,
        BalanceGains::default(),
        0.8,
        0.01,
        displaced,
    );
    assert!(release.iter().all(|v| math::length(*v) < 1e-6));
}

#[test]
fn support_transfer_keeps_motor_authority_without_faking_native_foot_contacts() {
    let mut plan = build(pose());
    plan.state.stance_entities = vec![2];
    let sample = |id| plan.bodies.iter().find(|b| b.entity == id).map(|b| b.pose);
    let mut control = BodyControlState::default();
    control.inherit_support(&plan.state, None, [0.0, 1.0, 0.0], 1.0, sample);
    assert!(control.plants.targets.is_empty());
    assert_eq!(control.activation, 0.0);
    control.inherit_support(
        &plan.state,
        Some(Support {
            normal: [0.0, 1.0, 0.0],
            velocity: [0.0; 3],
        }),
        [0.0, 1.0, 0.0],
        0.8,
        sample,
    );
    assert!((control.activation - 0.8).abs() < 1e-6);
    assert_eq!(control.plants.targets.len(), 1);
    assert!(
        !control.balance.supported,
        "the new native feet have not contacted yet"
    );
    assert_eq!(
        control.plants.targets[&1].position,
        plan.bodies[1].pose.position
    );
}

#[test]
fn support_memory_matches_the_full_plant_task_without_sampling_joint_ancestry() {
    let plan = build(pose());
    let contacts = [SupportContact {
        part: 1,
        position: [0.0; 3],
    }];
    let mut observed = FootPlantState::default();
    let mut full = FootPlantState::default();
    for (contacts, velocity) in [
        (&contacts[..], [2.0, 0.0, 0.0]),
        (&[][..], [0.0; 3]),
        (&[][..], [-1.0, 0.0, 0.0]),
    ] {
        let mut memory_reads = 0;
        let mut full_reads = 0;
        observed.observe(
            &plan.state,
            contacts,
            velocity,
            9.81,
            1.8,
            0.8,
            0.01,
            |id| {
                memory_reads += 1;
                plan.bodies.iter().find(|b| b.entity == id).map(|b| b.pose)
            },
        );
        full.correct(
            &plan.state,
            contacts,
            velocity,
            9.81,
            1.8,
            1.0,
            0.0,
            BalanceGains::default(),
            0.8,
            0.01,
            |id| {
                full_reads += 1;
                plan.bodies.iter().find(|b| b.entity == id).map(|b| b.pose)
            },
        );
        assert_eq!(
            serde_json::to_value(&observed).unwrap(),
            serde_json::to_value(&full).unwrap()
        );
        assert!(
            memory_reads < full_reads,
            "unused leg task still samples its ancestry"
        );
    }
}
