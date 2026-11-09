use super::*;
use crate::{RagdollMaterial, RigDefinition, RigInput, RigPartDefinition, SoleGeometry};
fn fixture() -> (RigState, BTreeMap<u64, BodyPose>, SteppingModel) {
    let mut parts = vec![RigPartDefinition {
        name: "trunk".into(),
        joint: "root".into(),
        end_joint: None,
        parent: None,
        anchor_joint: None,
        radius: 0.1,
        mass_weight: 10.0,
        cone_limit_degrees: 90.0,
        sole: None,
    }];
    let mut matrices = BTreeMap::from([(
        "root".into(),
        math::matrix([0.0, 0.9, 0.0], [0.0, 0.0, 0.0, 1.0]),
    )]);
    let mut legs = Vec::new();
    for (side, x) in [("a", 0.15), ("b", -0.15)] {
        let names: [String; 4] = std::array::from_fn(|i| format!("{side}.{i}"));
        for (i, point) in [
            [x, 0.85, 0.0],
            [x, 0.45, 0.08],
            [x, 0.05, 0.0],
            [x, 0.05, 0.2],
        ]
        .into_iter()
        .enumerate()
        {
            matrices.insert(names[i].clone(), math::matrix(point, [0.0, 0.0, 0.0, 1.0]));
        }
        for i in 0..3 {
            parts.push(RigPartDefinition {
                name: names[i].clone(),
                joint: names[i].clone(),
                end_joint: Some(names[i + 1].clone()),
                parent: Some(if i == 0 {
                    "trunk".into()
                } else {
                    names[i - 1].clone()
                }),
                anchor_joint: None,
                radius: 0.05,
                mass_weight: 1.0,
                cone_limit_degrees: 90.0,
                sole: (i == 2).then_some(SoleGeometry {
                    width_radius_scale: 1.0,
                    thickness_radius_scale: 0.5,
                    heel_radius_scale: 1.0,
                    vertical_offset_radius_scale: 0.0,
                }),
            });
        }
        legs.push(StepLeg {
            thigh: names[0].clone(),
            calf: names[1].clone(),
            foot: names[2].clone(),
            knee_direction: [0.0, 0.0, 1.0],
        });
    }
    let reference = BodyPose {
        position: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        linear_velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
    };
    let plan = RigDefinition {
        mass_kg: 70.0,
        parts,
    }
    .build(
        &RagdollMaterial {
            friction: 0.8,
            restitution: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            joint_max_force: 1e6,
            joint_angular_stiffness: 10.0,
            joint_angular_damping: 0.1,
        },
        RigInput {
            matrices: &matrices,
            reference_matrices: Some(&matrices),
            anatomy: None,
            entities: &[1, 2, 3, 4, 5, 6, 7],
            source: reference,
            source_radius: 0.3,
            source_half_height: 0.6,
            source_mass_kg: 70.0,
            visual_position: [0.0; 3],
            visual_heading: 0.0,
        },
    )
    .unwrap();
    let poses = plan.bodies.iter().map(|b| (b.entity, b.pose)).collect();
    (
        plan.state,
        poses,
        SteppingModel {
            legs,
            stride_leg_fraction: 0.65,
            clearance_leg_fraction: 0.14,
            duration_scale: 1.15,
            capture_margin_foot_fraction: 0.2,
            reach_fraction: 0.999,
            position_response: 1.0,
            minimum_uprightness: 0.8,
            landing_radius_fraction: 2.0,
        },
    )
}
fn surface() -> BodyPose {
    BodyPose {
        position: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        linear_velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
    }
}
fn transport(p: [f32; 3], frame: BodyPose) -> [f32; 3] {
    math::add(frame.position, math::rotate(frame.rotation, p))
}
fn plan(
    state: &mut SteppingState,
    rig: &RigState,
    poses: &BTreeMap<u64, BodyPose>,
    model: &SteppingModel,
    frame: BodyPose,
    dt: f32,
    retired: bool,
) -> Vec<Option<[f32; 4]>> {
    plan_with_contacts(state, rig, poses, model, frame, dt, retired, &[], [0.0; 3])
}
fn plan_with_contacts(
    state: &mut SteppingState,
    rig: &RigState,
    poses: &BTreeMap<u64, BodyPose>,
    model: &SteppingModel,
    frame: BodyPose,
    dt: f32,
    retired: bool,
    excluded: &[usize],
    relative_velocity: [f32; 3],
) -> Vec<Option<[f32; 4]>> {
    let sample = |id| {
        if id == 99 {
            (!retired).then_some(frame)
        } else {
            poses.get(&id).map(|p| BodyPose {
                position: transport(p.position, frame),
                rotation: math::multiply(frame.rotation, p.rotation),
                linear_velocity: math::add(frame.linear_velocity, relative_velocity),
                angular_velocity: frame.angular_velocity,
            })
        }
    };
    let mut contacts = Vec::new();
    for (i, part) in rig
        .parts
        .iter()
        .enumerate()
        .filter(|(_, p)| p.support_clearance.is_some())
    {
        if excluded.contains(&i) {
            continue;
        }
        let p = sample(part.entity).unwrap();
        let h = part.support_half_extents.unwrap();
        for x in [-h[0], h[0]] {
            for y in [-h[1], h[1]] {
                contacts.push(SupportContact {
                    part: i,
                    position: math::add(p.position, math::rotate(p.rotation, [x, y, -h[2]])),
                });
            }
        }
    }
    let com = transport([0.0, 0.7, 0.0], frame);
    let mut balance = crate::measure_balance(
        &contacts.iter().map(|c| c.position).collect::<Vec<_>>(),
        com,
        math::add(frame.linear_velocity, relative_velocity),
        frame.linear_velocity,
        [0.0, 1.0, 0.0],
        9.81,
        0.0,
        0.35,
    );
    if retired {
        balance.supported = false;
        contacts.clear();
    }
    state.plan(
        model,
        rig,
        &balance,
        &contacts,
        com,
        math::add(frame.linear_velocity, relative_velocity),
        9.81,
        1.0,
        dt,
        sample,
        |p, r, _| {
            if retired {
                return None;
            }
            let local = math::rotate(math::inverse(frame.rotation), math::sub(p, frame.position));
            let normal = math::rotate(frame.rotation, [0.0, 1.0, 0.0]);
            let point = transport([local[0], 0.0, local[2]], frame);
            let ground = StepGround {
                position: math::add(point, normal.map(|v| v * r)),
                normal,
                velocity: frame.linear_velocity,
                support: None,
            };
            ground.bind(99, frame, r)
        },
        |_, _, _| true,
    )
}
#[test]
fn planted_feet_and_balance_intent_follow_a_common_rotating_support_without_changing_the_rig() {
    let (rig, poses, model) = fixture();
    for hz in [30.0, 60.0, 144.0, 240.0] {
        let mut state = SteppingState::default();
        plan(&mut state, &rig, &poses, &model, surface(), 1.0 / hz, false);
        assert_eq!(state.phase, "stance");
        let initial = state.foot_goals.clone();
        let goal = state.support_goal.unwrap();
        for tick in 1..=10 {
            let mut p = surface();
            p.position = [tick as f32 * 0.03, 0.1, 0.0];
            p.rotation = math::axis_angle([0.0, tick as f32 * 0.04, 0.0]);
            plan(&mut state, &rig, &poses, &model, p, 1.0 / hz, false);
            assert_eq!(state.phase, "stance");
            for (foot, start) in &initial {
                assert!(
                    math::length(math::sub(state.foot_goals[foot], transport(*start, p))) < 1e-5
                );
            }
            assert!(
                math::length(math::sub(state.support_goal.unwrap(), transport(goal, p))) < 1e-5
            );
        }
    }
}
#[test]
fn an_inflight_landing_target_tracks_the_platform_and_is_cancelled_when_it_retires() {
    let (rig, poses, model) = fixture();
    let mut state = SteppingState::default();
    plan(&mut state, &rig, &poses, &model, surface(), 0.01, false);
    let foot = *state.planted.keys().next().unwrap();
    let landing = state.planted[&foot].clone();
    let original = landing.position;
    state.swing = Some(SwingStep {
        foot,
        origin: original,
        landing,
        elapsed: 0.0,
        duration: 0.4,
        clearance: 0.1,
        progress: 0.0,
        left_ground: true,
        released: true,
    });
    let mut frame = surface();
    frame.position = [0.2, 0.1, -0.1];
    frame.rotation = math::axis_angle([0.0, 0.4, 0.0]);
    plan(&mut state, &rig, &poses, &model, frame, 0.01, false);
    assert!(
        math::length(math::sub(
            state.swing.as_ref().unwrap().landing.position,
            transport(original, frame)
        )) < 1e-5
    );
    let goals = plan(&mut state, &rig, &poses, &model, frame, 0.01, true);
    assert_eq!(state.phase, "airborne");
    assert!(
        state.planted.is_empty()
            && state.foot_goals.is_empty()
            && state.support_goal.is_none()
            && state.swing.is_none()
    );
    assert!(goals.iter().all(Option::is_none));
    assert_eq!(state.completed, 0);
}

#[test]
fn invalid_observations_cannot_submit_nonfinite_leg_targets_or_retain_ghost_goals() {
    let (rig, poses, model) = fixture();
    for dt in [0.0, -0.01, f32::NAN, f32::INFINITY] {
        let mut state = SteppingState::default();
        plan(&mut state, &rig, &poses, &model, surface(), 0.01, false);
        let goals = plan(&mut state, &rig, &poses, &model, surface(), dt, false);
        assert!(goals.iter().all(Option::is_none));
        assert!(
            state.foot_goals.is_empty() && state.planted.is_empty() && state.support_goal.is_none()
        );
    }
}

fn pending_step(state: &mut SteppingState, foot: usize) {
    let landing = state.planted[&foot].clone();
    state.swing = Some(SwingStep {
        foot,
        origin: landing.position,
        landing,
        elapsed: 0.0,
        duration: 0.4,
        clearance: 0.1,
        progress: 0.0,
        left_ground: false,
        released: false,
    });
}
#[test]
fn unloaded_leg_reaches_for_capture_support_while_a_loaded_leg_waits_for_safe_transfer() {
    let (rig, poses, model) = fixture();
    for grounded in [false, true] {
        let mut state = SteppingState::default();
        plan(&mut state, &rig, &poses, &model, surface(), 0.01, false);
        let foot = *state.planted.keys().next().unwrap();
        pending_step(&mut state, foot);
        let excluded = if grounded { vec![] } else { vec![foot] };
        let targets = plan_with_contacts(
            &mut state,
            &rig,
            &poses,
            &model,
            surface(),
            0.01,
            false,
            &excluded,
            [0.0; 3],
        );
        let step = state.swing.as_ref().unwrap();
        assert_eq!(
            step.released, !grounded,
            "an already unloaded foot must not wait for impossible unloading"
        );
        assert_eq!(step.left_ground, !grounded);
        assert_eq!(step.progress > 0.0, !grounded);
        assert!(targets.iter().flatten().flatten().all(|v| v.is_finite()));
        if grounded {
            assert_eq!(state.phase, "load_transfer");
        }
    }
}
#[test]
fn loss_of_the_planned_stance_replans_the_free_leg_instead_of_lifting_the_only_supported_foot() {
    let (rig, poses, model) = fixture();
    let mut state = SteppingState::default();
    plan(&mut state, &rig, &poses, &model, surface(), 0.01, false);
    let feet: Vec<_> = state.planted.keys().copied().collect();
    pending_step(&mut state, feet[0]);
    plan_with_contacts(
        &mut state,
        &rig,
        &poses,
        &model,
        surface(),
        0.01,
        false,
        &[feet[1]],
        [1.0, 0.0, 0.0],
    );
    let step = state
        .swing
        .as_ref()
        .expect("free leg can seek capture support");
    assert_eq!(
        step.foot, feet[1],
        "the only supported foot must remain the stance foot"
    );
    assert!(step.released && step.left_ground && step.progress > 0.0);
    assert_eq!(
        state.completed, 0,
        "a planned reach is not a completed physical step"
    );
}

fn measured_contacts(rig: &RigState, poses: &BTreeMap<u64, BodyPose>) -> Vec<SupportContact> {
    let mut contacts = Vec::new();
    for (part, body) in rig.parts.iter().enumerate() {
        let Some(h) = body.support_half_extents else {
            continue;
        };
        let pose = poses[&body.entity];
        for x in [-h[0], h[0]] {
            for y in [-h[1], h[1]] {
                contacts.push(SupportContact {
                    part,
                    position: math::add(pose.position, math::rotate(pose.rotation, [x, y, -h[2]])),
                });
            }
        }
    }
    contacts
}

#[test]
fn separate_supports_do_not_choose_the_capture_frame_by_foot_order_or_manifold_density() {
    let (rig, poses, model) = fixture();
    let contacts = measured_contacts(&rig, &poses);
    let balance = BalanceState {
        supported: true,
        uprightness: 1.0,
        body_up: [0.0, 1.0, 0.0],
        ..Default::default()
    };
    for reverse in [false, true] {
        for dense in [false, true] {
            let mut state = SteppingState::default();
            plan(&mut state, &rig, &poses, &model, surface(), 0.01, false);
            let feet: Vec<_> = state.planted.keys().copied().collect();
            for (i, foot) in feet.iter().enumerate() {
                let mut frame = surface();
                frame.linear_velocity[0] = if (i == 0) != reverse { 2.0 } else { -2.0 };
                let target = state.planted[foot]
                    .clone()
                    .bind(
                        99 + i as u64,
                        frame,
                        rig.parts[*foot].support_clearance.unwrap(),
                    )
                    .unwrap();
                state.planted.insert(*foot, target);
            }
            let mut measured = contacts.clone();
            if dense {
                for _ in 0..20 {
                    measured.extend(contacts.iter().filter(|c| c.part == feet[0]).copied());
                }
            }
            let targets = state.plan(
                &model,
                &rig,
                &balance,
                &measured,
                [0.0, 0.7, 0.0],
                [0.0; 3],
                9.81,
                1.0,
                0.01,
                |id| {
                    if id == 99 || id == 100 {
                        let mut p = surface();
                        p.linear_velocity[0] = if (id == 99) != reverse { 2.0 } else { -2.0 };
                        Some(p)
                    } else {
                        poses.get(&id).copied()
                    }
                },
                |_, _, _| None,
                |_, _, _| true,
            );
            assert!(
                state.anticipated_support_margin > 0.0,
                "symmetric measured supports must not invent relative drift: {}",
                state.anticipated_support_margin
            );
            assert_eq!(
                state.rejected, 0,
                "a stable capture point must not request a speculative step"
            );
            assert!(targets.iter().filter(|q| q.is_some()).count() == 6);
        }
    }
}

#[test]
fn one_planning_observation_samples_each_resident_body_once() {
    let (rig, poses, model) = fixture();
    let contacts = measured_contacts(&rig, &poses);
    let balance = BalanceState {
        supported: true,
        uprightness: 1.0,
        body_up: [0.0, 1.0, 0.0],
        ..Default::default()
    };
    let mut state = SteppingState::default();
    plan(&mut state, &rig, &poses, &model, surface(), 0.01, false);
    let mut calls = BTreeMap::<u64, usize>::new();
    let goals = state.plan(
        &model,
        &rig,
        &balance,
        &contacts,
        [0.0, 0.7, 0.0],
        [0.0; 3],
        9.81,
        1.0,
        0.01,
        |id| {
            *calls.entry(id).or_default() += 1;
            if id == 99 {
                Some(surface())
            } else {
                poses.get(&id).copied()
            }
        },
        |_, _, _| None,
        |_, _, _| true,
    );
    assert_eq!(goals.iter().filter(|q| q.is_some()).count(), 6);
    assert!(
        calls.values().all(|n| *n == 1),
        "one physical observation must not repeatedly query the same body: {calls:?}"
    );
}

#[test]
fn missing_or_nonfinite_limb_observations_retire_previous_control_goals() {
    let (rig, poses, model) = fixture();
    let contacts = measured_contacts(&rig, &poses);
    let foot = rig
        .parts
        .iter()
        .find(|p| p.support_half_extents.is_some())
        .unwrap()
        .entity;
    let balance = BalanceState {
        supported: true,
        uprightness: 1.0,
        body_up: [0.0, 1.0, 0.0],
        ..Default::default()
    };
    for (damaged, nonfinite, warm) in [
        (foot, false, true),
        (foot, true, true),
        (rig.parts[0].entity, false, true),
        (rig.parts[0].entity, true, true),
        (rig.parts[0].entity, false, false),
        (rig.parts[0].entity, true, false),
    ] {
        let mut state = SteppingState::default();
        if warm {
            plan(&mut state, &rig, &poses, &model, surface(), 0.01, false);
            assert!(!state.foot_goals.is_empty() && state.support_goal.is_some());
        }
        let goals = state.plan(
            &model,
            &rig,
            &balance,
            &contacts,
            [0.0, 0.7, 0.0],
            [0.0; 3],
            9.81,
            1.0,
            0.01,
            |id| {
                if id == 99 {
                    return Some(surface());
                }
                let mut p = poses.get(&id).copied()?;
                if id == damaged {
                    if !nonfinite {
                        return None;
                    }
                    p.linear_velocity[0] = f32::NAN;
                }
                Some(p)
            },
            |_, _, _| None,
            |_, _, _| true,
        );
        assert!(
            goals.iter().all(Option::is_none),
            "incomplete physical evidence cannot drive a leg"
        );
        assert!(
            state.foot_goals.is_empty() && state.support_goal.is_none(),
            "previous observation cannot remain a live control goal"
        );
        assert_eq!(state.phase, "unavailable");
    }
}
