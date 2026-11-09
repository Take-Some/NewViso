use super::*;
use std::collections::BTreeMap;

fn model() -> EndorphinModel {
    EndorphinModel {
        schema: "newviso.endorphin.v1".into(),
        anatomy: None,
        joint_map: BTreeMap::new(),
        weapon_rig: None,
        protection: None,
        recovery_match: None,
        rig: rig().0,
        ragdoll: RagdollMaterial {
            friction: 0.75,
            restitution: 0.0,
            linear_damping: 0.08,
            angular_damping: 0.45,
            joint_max_force: 1e6,
            joint_angular_stiffness: 12.0,
            joint_angular_damping: 0.04,
        },
        recovery: RecoveryModel {
            settling_energy_fraction: 0.025,
            minimum_support_cosine: 0.65,
            response_time_scale: 0.8,
            ready_confidence: 0.9,
            balance_reach: 2.0,
            airborne_clearance_fraction: 0.25,
            landing_absorption_height_fraction: 0.65,
            capacity_floor: 0.15,
        },
        hit: HitModel {
            injury_gain: 3.0,
            minimum_weight: 0.45,
            minimum_interrupt_fraction: 0.2,
            maximum_interrupt_fraction: 0.65,
        },
    }
}
fn observation() -> BodyObservation {
    BodyObservation {
        rotation: [0.0, 0.0, 0.0, 1.0],
        linear_velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
        radius: 0.3,
        half_height: 0.6,
        gravity: 9.81,
        capacity: 1.0,
        motion_energy_per_mass: None,
        support: Some(Support {
            normal: [0.0, 1.0, 0.0],
            velocity: [0.0; 3],
        }),
        space_clear: true,
        down: true,
    }
}
fn observe_for(state: &mut RecoveryState, body: BodyObservation, dt: f32, count: usize) {
    for _ in 0..count {
        state.observe(&model().recovery, body, dt);
    }
}
#[test]
fn recovery_evidence_is_frame_rate_independent() {
    let mut a = RecoveryState::default();
    let mut b = a.clone();
    observe_for(&mut a, observation(), 1.0 / 30.0, 60);
    observe_for(&mut b, observation(), 1.0 / 144.0, 288);
    assert!((a.confidence - b.confidence).abs() < 1e-5);
    assert!(a.ready && b.ready);
}
#[test]
fn sliding_and_spinning_do_not_recover_from_a_finished_clip() {
    for (linear, angular) in [([4.0, 0.0, 0.0], [0.0; 3]), ([0.0; 3], [0.0, 5.0, 0.0])] {
        let mut b = observation();
        b.linear_velocity = linear;
        b.angular_velocity = angular;
        let mut s = RecoveryState::default();
        observe_for(&mut s, b, 0.1, 100);
        assert!(!s.ready && s.relative_energy_ratio > 1.0);
    }
}
#[test]
fn recovery_uses_support_point_velocity_on_a_moving_platform() {
    let mut b = observation();
    b.linear_velocity = [5.0, 0.0, 0.0];
    b.support.as_mut().unwrap().velocity = b.linear_velocity;
    let mut s = RecoveryState::default();
    observe_for(&mut s, b, 0.02, 100);
    assert!(s.ready);
    assert!(s.relative_energy_ratio < 1e-6);
}
#[test]
fn losing_contact_or_clearance_invalidates_accumulated_recovery() {
    let mut s = RecoveryState::default();
    observe_for(&mut s, observation(), 0.1, 30);
    assert!(s.ready);
    let mut b = observation();
    b.space_clear = false;
    s.observe(&model().recovery, b, 0.01);
    assert!(!s.ready && s.confidence == 0.0);
    b.space_clear = true;
    b.support = None;
    observe_for(&mut s, b, 0.1, 30);
    assert!(!s.ready && s.confidence == 0.0);
}
#[test]
fn landing_absorption_adapts_to_capacity_body_scale_and_gravity() {
    fn land(mut b: BodyObservation) -> bool {
        let mut s = RecoveryState::default();
        b.support = None;
        b.linear_velocity[1] = -4.0;
        s.observe(&model().recovery, b, 0.1);
        b.support = observation().support;
        b.linear_velocity = [0.0; 3];
        s.observe(&model().recovery, b, 0.01);
        s.hard_landing
    }
    let b = observation();
    assert!(!land(b));
    let mut weak = b;
    weak.capacity = 0.25;
    assert!(land(weak));
    let mut small = b;
    small.half_height = 0.1;
    assert!(land(small));
    let mut low_gravity = b;
    low_gravity.gravity = 2.0;
    assert!(land(low_gravity));
}
#[test]
fn shallow_contact_gaps_do_not_trigger_a_fall_and_bad_input_cannot_recover() {
    let mut b = observation();
    b.support = None;
    b.linear_velocity[1] = -0.1;
    let mut s = RecoveryState::default();
    s.observe(&model().recovery, b, 0.01);
    assert!(!s.falling);
    b.linear_velocity[1] = -3.0;
    s.observe(&model().recovery, b, 0.1);
    assert!(s.falling);
    b.linear_velocity[1] = f32::NAN;
    s.observe(&model().recovery, b, 0.1);
    assert!(!s.ready);
}
fn rig() -> (RigDefinition, BTreeMap<String, [f32; 16]>) {
    let part = |name: &str, joint: &str, parent: Option<&str>| RigPartDefinition {
        name: name.into(),
        joint: joint.into(),
        end_joint: None,
        parent: parent.map(str::to_owned),
        anchor_joint: None,
        radius: 0.15,
        mass_weight: 1.0,
        cone_limit_degrees: 60.0,
        sole: None,
    };
    (
        RigDefinition {
            mass_kg: 70.0,
            parts: vec![
                part("root", "root", None),
                part("limb", "limb", Some("root")),
            ],
        },
        BTreeMap::from([
            (
                "root".into(),
                math::matrix([0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
            ),
            (
                "limb".into(),
                math::matrix([0.3, 1.5, 0.0], [0.0, 0.0, 0.0, 1.0]),
            ),
        ]),
    )
}
fn build(source: BodyPose) -> RigPlan {
    let (def, matrices) = rig();
    def.build(
        &model().ragdoll,
        RigInput {
            matrices: &matrices,
            reference_matrices: None,
            anatomy: None,
            entities: &[1, 2],
            source,
            source_radius: 0.3,
            source_half_height: 0.6,
            source_mass_kg: 70.0,
            visual_position: [0.0; 3],
            visual_heading: 0.0,
        },
    )
    .unwrap()
}
fn pose() -> BodyPose {
    BodyPose {
        position: [0.0, 0.9, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        linear_velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
    }
}
#[test]
fn ownership_transfer_preserves_total_linear_and_angular_momentum() {
    let mut p = pose();
    p.linear_velocity = [3.0, -2.0, 1.0];
    p.angular_velocity = [0.5, 1.2, -0.7];
    let plan = build(p);
    let mut linear = [0.0; 3];
    let mut angular = [0.0; 3];
    for b in &plan.bodies {
        linear = math::add(linear, b.pose.linear_velocity.map(|v| v * b.mass_kg));
        let spin = math::tensor_vector(
            math::tensor(b.pose.rotation, b.inertia_diagonal),
            b.pose.angular_velocity,
        );
        let orbit = math::cross(
            math::sub(b.pose.position, plan.center_of_mass),
            b.pose.linear_velocity.map(|v| v * b.mass_kg),
        );
        angular = math::add(angular, math::add(spin, orbit));
    }
    assert!(math::length(math::sub(linear, plan.linear_momentum)) < 1e-3);
    assert!(math::length(math::sub(angular, plan.angular_momentum)) < 1e-3);
}
#[test]
fn stationary_transfer_does_not_invent_spin_or_translation() {
    let plan = build(pose());
    assert!(plan
        .bodies
        .iter()
        .all(|b| math::length(b.pose.linear_velocity) < 1e-6
            && math::length(b.pose.angular_velocity) < 1e-6));
}
#[test]
fn unchanged_physical_pose_reuses_the_palette_and_motion_wakes_it() {
    let mut plan = build(pose());
    let sample = |entity| {
        plan.bodies
            .iter()
            .find(|b| b.entity == entity)
            .map(|b| b.pose)
    };
    assert!(plan.state.refresh_palette(sample));
    let ptr = plan.state.palette().as_ptr();
    for _ in 0..100 {
        assert!(!plan.state.refresh_palette(sample));
    }
    assert_eq!(ptr, plan.state.palette().as_ptr());
    assert_eq!(plan.state.pose_updates, 1);
    assert_eq!(plan.state.pose_skips, 100);
    assert!(plan.state.refresh_palette(|id| sample(id).map(|mut p| {
        if id == 2 {
            p.position[0] += 0.01;
        }
        p
    })));
}
#[test]
fn missing_bones_duplicate_entities_and_invalid_topology_fail_before_ownership_transfer() {
    let (mut def, mut matrices) = rig();
    matrices.remove("limb");
    let input = |matrices, entities| RigInput {
        matrices,
        reference_matrices: None,
        anatomy: None,
        entities,
        source: pose(),
        source_radius: 0.3,
        source_half_height: 0.6,
        source_mass_kg: 70.0,
        visual_position: [0.0; 3],
        visual_heading: 0.0,
    };
    assert!(def
        .build(&model().ragdoll, input(&matrices, &[1, 2]))
        .is_err());
    let matrices = rig().1;
    assert!(def
        .build(&model().ragdoll, input(&matrices, &[1, 1]))
        .is_err());
    def.parts[1].parent = Some("missing".into());
    assert!(def.validate().is_err());
}
#[test]
fn model_validation_and_hit_response_use_authored_dimensionless_parameters() {
    let mut m = model();
    m.validate().unwrap();
    let a = m.hit_response(20.0 / 100.0, 0.4);
    let b = m.hit_response(40.0 / 200.0, 0.4);
    assert_eq!(a, b);
    assert!(m.hit_response(0.6, 0.4).0 > a.0);
    m.recovery.ready_confidence = 1.0;
    assert!(m.validate().is_err());
    m = model();
    m.ragdoll.angular_damping = f32::NAN;
    assert!(m.validate().is_err());
}

#[test]
fn moving_limb_prevents_a_still_root_from_hiding_articulated_energy() {
    let plan = build(pose());
    let energy = plan
        .state
        .kinetic_energy_per_mass([0.0; 3], |id| {
            plan.bodies.iter().find(|b| b.entity == id).map(|b| {
                let mut p = b.pose;
                if id == 2 {
                    p.linear_velocity = [6.0, 0.0, 0.0];
                }
                p
            })
        })
        .unwrap();
    let mut body = observation();
    body.motion_energy_per_mass = Some(energy);
    let mut state = RecoveryState::default();
    observe_for(&mut state, body, 0.02, 300);
    assert!(!state.ready && state.relative_energy_ratio > 1.0);
}

#[test]
fn supported_recovery_survives_render_frames_without_physics_contacts() {
    let mut state = RecoveryState::default();
    let mut reference = RecoveryState::default();
    let mut clock = ObservationClock::default();
    for tick in 1..=120 {
        let dt = clock.sample(tick, 1.0 / 60.0).unwrap();
        state.observe(&model().recovery, observation(), dt);
        reference.observe(&model().recovery, observation(), 1.0 / 60.0);
        // The host has no new contact events on these rendering frames.
        // They are not observations of lost support.
        for _ in 0..4 {
            if let Some(dt) = clock.sample(tick, 1.0 / 60.0) {
                let mut missing = observation();
                missing.support = None;
                state.observe(&model().recovery, missing, dt);
            }
        }
    }
    assert!(state.ready);
    assert!((state.confidence - reference.confidence).abs() < 1e-6);
}

#[test]
fn authored_sole_has_box_volume_inertia_and_preserves_inherited_momentum() {
    let (mut def, mut matrices) = rig();
    def.parts[1].end_joint = Some("toe".into());
    def.parts[1].sole = Some(SoleGeometry {
        width_radius_scale: 1.1,
        thickness_radius_scale: 0.5,
        heel_radius_scale: 1.0,
        vertical_offset_radius_scale: -0.8,
    });
    matrices.insert(
        "toe".into(),
        math::matrix([0.3, 1.5, 0.3], [0.0, 0.0, 0.0, 1.0]),
    );
    let mut source = pose();
    source.linear_velocity = [2.0, -1.0, 0.4];
    source.angular_velocity = [0.1, 0.3, -0.2];
    let plan = def
        .build(
            &model().ragdoll,
            RigInput {
                matrices: &matrices,
                reference_matrices: Some(&matrices),
                anatomy: None,
                entities: &[1, 2],
                source,
                source_radius: 0.3,
                source_half_height: 0.6,
                source_mass_kg: 70.0,
                visual_position: [0.0; 3],
                visual_heading: 0.0,
            },
        )
        .unwrap();
    let foot = &plan.bodies[1];
    let h = foot.box_half_extents.expect("authored sole");
    assert!(h.iter().all(|v| *v > 0.0));
    assert!((foot.volume() - 8.0 * h[0] * h[1] * h[2]).abs() < 1e-7);
    assert!(
        (foot.inertia_diagonal[1] - foot.mass_kg * (h[0] * h[0] + h[2] * h[2]) / 3.0).abs() < 1e-6
    );
    let mut p = [0.0; 3];
    let mut l = [0.0; 3];
    for b in &plan.bodies {
        p = math::add(p, b.pose.linear_velocity.map(|v| v * b.mass_kg));
        l = math::add(
            l,
            math::add(
                math::tensor_vector(
                    math::tensor(b.pose.rotation, b.inertia_diagonal),
                    b.pose.angular_velocity,
                ),
                math::cross(
                    math::sub(b.pose.position, plan.center_of_mass),
                    b.pose.linear_velocity.map(|v| v * b.mass_kg),
                ),
            ),
        );
    }
    assert!(math::length(math::sub(p, plan.linear_momentum)) < 1e-3);
    assert!(math::length(math::sub(l, plan.angular_momentum)) < 1e-3);
}
#[test]
fn anatomical_zero_comes_from_neutral_skeleton_instead_of_activation_pose() {
    let (mut def, mut reference) = rig();
    def.parts[0].end_joint = Some("upper".into());
    reference.insert(
        "upper".into(),
        math::matrix([0.0, 2.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
    );
    let part = AnatomicalPart {
        region: "torso".into(),
        aliases: vec![],
        damage_multiplier: 1.0,
        trauma_gain: 4.0,
        limits: None,
        muscle: None,
    };
    let anatomy = AnatomyModel {
        orientation: BodyOrientation {
            lower: "root".into(),
            upper: "upper".into(),
            left: "root".into(),
            right: "limb".into(),
            forward: None,
        },
        parts: BTreeMap::from([("root".into(), part.clone()), ("limb".into(), part)]),
        functions: BTreeMap::new(),
        resting_activation: 0.1,
        brace_energy_fraction: 0.25,
        response_time_scale: 0.25,
        balance: BalanceModel {
            stepping: None,
            support_parts: vec!["limb".into()],
            joint_weights: BTreeMap::new(),
            footprint_padding: 0.04,
            max_lean_angle: 0.35,
            recovery_upright_cosine: 0.95,
            gains: Default::default(),
        },
    };
    for yaw in [0.0, 0.8] {
        let world = math::matrix([12.0, 0.0, -20.0], math::axis_angle([0.0, yaw, 0.0]));
        let refs: BTreeMap<_, _> = reference
            .iter()
            .map(|(n, m)| (n.clone(), math::product(world, *m)))
            .collect();
        let mut animated = refs.clone();
        animated.insert(
            "limb".into(),
            math::product(
                refs["limb"],
                math::matrix([0.0; 3], math::axis_angle([0.5, 0.0, 0.0])),
            ),
        );
        let plan = def
            .build(
                &model().ragdoll,
                RigInput {
                    matrices: &animated,
                    reference_matrices: Some(&refs),
                    anatomy: Some(&anatomy),
                    entities: &[1, 2],
                    source: pose(),
                    source_radius: 0.3,
                    source_half_height: 0.6,
                    source_mass_kg: 70.0,
                    visual_position: [0.0; 3],
                    visual_heading: 0.0,
                },
            )
            .unwrap();
        let q = plan.state.joints[0].initial_rotation;
        assert!(
            (q[0].abs() - (0.25f32).sin()).abs() < 1e-5,
            "anatomical zero drifted with activation pose: {q:?}"
        );
    }
}

mod support;

#[test]
fn semantic_joint_mapping_retargets_shared_controller_without_asset_names() {
    let mut template = model();
    let names: std::collections::BTreeSet<_> = template
        .rig
        .parts
        .iter()
        .flat_map(|p| {
            [
                Some(p.joint.clone()),
                p.end_joint.clone(),
                p.anchor_joint.clone(),
            ]
            .into_iter()
            .flatten()
        })
        .collect();
    for (i, name) in names.into_iter().enumerate() {
        template
            .joint_map
            .insert(name, format!("foreign_namespace::bone_{i}"));
    }
    let bound = template.bind_joints().unwrap();
    for (original, actual) in template.rig.parts.iter().zip(&bound.rig.parts) {
        assert_eq!(actual.joint, template.joint_map[&original.joint]);
        assert_eq!(actual.name, original.name);
        assert_eq!(actual.mass_weight, original.mass_weight);
        if let Some(end) = &original.end_joint {
            assert_eq!(actual.end_joint.as_ref().unwrap(), &template.joint_map[end]);
        }
    }
    assert!(bound.joint_map.is_empty());
    assert_eq!(bound.bind_joints().unwrap(), bound);
}
#[test]
fn ambiguous_joint_mapping_cannot_merge_physical_parts() {
    let mut template = model();
    for p in &template.rig.parts {
        template
            .joint_map
            .insert(p.joint.clone(), "one_bone".into());
    }
    assert!(template.bind_joints().is_err());
}
