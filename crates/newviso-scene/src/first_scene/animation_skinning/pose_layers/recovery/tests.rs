use super::*;
fn identity() -> [f32; 16] {
    trs_matrix([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3])
}
fn skeleton() -> ModelSkeleton {
    let joint = |name: &str, parent, translation| newviso_model::ModelJoint {
        name: name.into(),
        tag: 1,
        parent,
        bind_translation: translation,
        bind_rotation: [0.0, 0.0, 0.0, 1.0],
        bind_scale: [1.0; 3],
        inverse_bind_matrix: identity(),
    };
    ModelSkeleton {
        name: "recovery".into(),
        joints: vec![
            joint("root", None, [0.0; 3]),
            joint("limb", Some(0), [0.0, 1.0, 0.0]),
            joint("roll_helper", Some(0), [0.0, 2.0, 0.0]),
        ],
    }
}
fn blend(
    s: &ModelSkeleton,
    drivers: &[Option<DerivedJointDriver>],
    from: &[[f32; 16]],
) -> RecoveryBlend {
    RecoveryBlend {
        locals: local_poses(s, drivers, from).unwrap().into(),
        clip_name: "getup".into(),
        duration: 1.0,
        elapsed: 0.0,
        started: false,
        rebase_error: 0.0,
        initial_pose_committed: false,
    }
}
fn close(a: [f32; 16], b: [f32; 16]) {
    for (a, b) in a.iter().zip(b) {
        assert!((a - b).abs() < 0.0001, "{a} != {b}");
    }
}
#[test]
fn trs_roundtrip_preserves_half_turns_reflections_and_authored_scale() {
    for q in [
        [0.0, 0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.2, 0.3, 0.4, 0.5],
    ] {
        for scale in [[1.0; 3], [0.2, 1.5, -2.0], [-1.0, -2.0, 3.0]] {
            let m = trs_matrix([2.0, -0.4, 7.0], q, scale);
            let p = decompose(m).unwrap();
            close(m, trs_matrix(p.translation, p.rotation, p.scale));
        }
    }
}
#[test]
fn captured_world_pose_survives_model_origin_heading_and_scale_rebase() {
    let s = skeleton();
    let drivers = [None; 3];
    let source = vec![
        identity(),
        trs_matrix(
            [0.0, 1.0, 0.0],
            [0.70710677, 0.0, 0.0, 0.70710677],
            [1.0; 3],
        ),
        trs_matrix([0.0, 2.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
    ];
    for scale in [0.1, 1.0, 3.0] {
        let old = trs_matrix(
            [20.0, 0.4, -11.0],
            [0.0, 0.38268343, 0.0, 0.9238795],
            [scale; 3],
        );
        let new = trs_matrix(
            [19.7, 1.1, -10.8],
            [0.0, -0.70710677, 0.0, 0.70710677],
            [scale; 3],
        );
        let relative = mul_mat4(inverse_affine(new).unwrap(), old);
        let rebased: Vec<_> = source.iter().map(|m| mul_mat4(relative, *m)).collect();
        let r = blend(&s, &drivers, &rebased);
        let actual = r
            .apply(vec![identity(); 3], &s, &drivers, identity(), identity())
            .unwrap();
        for (before, after) in source.iter().zip(actual) {
            close(mul_mat4(old, *before), mul_mat4(new, after));
        }
    }
}
#[test]
fn opposing_local_offsets_never_collapse_a_connected_bone() {
    for end in [[0.0, -1.0, 0.0], [0.001, -0.9999995, 0.0]] {
        for step in 0..=100 {
            let t = step as f32 / 100.0;
            assert!((length(offset([0.0, 1.0, 0.0], end, t)) - 1.0).abs() < 0.00001);
        }
        let near = offset([0.0, 1.0, 0.0], end, 0.99999);
        assert!(length(std::array::from_fn(|i| near[i] - end[i])) < 0.0001);
    }
}
#[test]
fn rotating_blend_keeps_connected_lengths_and_rigid_bases() {
    let s = skeleton();
    let d = [None; 3];
    let from = vec![
        identity(),
        trs_matrix([0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
        trs_matrix([0.0, 2.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
    ];
    let target = vec![
        trs_matrix([1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [1.0; 3]),
        trs_matrix([1.0, -1.0, 0.0], [0.0, 0.0, 1.0, 0.0], [1.0; 3]),
        trs_matrix([1.0, -2.0, 0.0], [0.0, 0.0, 1.0, 0.0], [1.0; 3]),
    ];
    let mut r = blend(&s, &d, &from);
    r.start_for_clip("getup");
    for step in 0..=20 {
        r.elapsed = step as f32 / 20.0;
        let p = r
            .apply(target.clone(), &s, &d, identity(), identity())
            .unwrap();
        let local = local_poses(&s, &d, &p).unwrap();
        for i in 1..3 {
            assert!((length(local[i].translation) - i as f32).abs() < 0.00001);
        }
        for m in p {
            for i in 0..3 {
                assert!((length([m[i * 4], m[i * 4 + 1], m[i * 4 + 2]]) - 1.0).abs() < 0.00001);
            }
            assert!(dot([m[0], m[1], m[2]], [m[4], m[5], m[6]]).abs() < 0.00001);
        }
    }
}
#[test]
fn derived_helpers_follow_the_blended_driver_at_both_endpoints_and_midway() {
    let s = skeleton();
    let d = [
        None,
        None,
        Some(DerivedJointDriver {
            joint: 1,
            bind_offset: trs_matrix([0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
        }),
    ];
    let from = vec![
        identity(),
        trs_matrix([0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
        trs_matrix([0.0, 2.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
    ];
    let target = vec![
        identity(),
        trs_matrix(
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.70710677, 0.70710677],
            [1.0; 3],
        ),
        trs_matrix(
            [-1.0, 1.0, 0.0],
            [0.0, 0.0, 0.70710677, 0.70710677],
            [1.0; 3],
        ),
    ];
    let mut r = blend(&s, &d, &from);
    r.elapsed = 0.5;
    let p = r
        .apply(target.clone(), &s, &d, identity(), identity())
        .unwrap();
    close(p[2], mul_mat4(p[1], d[2].unwrap().bind_offset));
    r.elapsed = 0.0;
    for (a, b) in r
        .apply(target.clone(), &s, &d, identity(), identity())
        .unwrap()
        .iter()
        .zip(from)
    {
        close(*a, b);
    }
    r.elapsed = 1.0;
    assert_eq!(
        r.apply(target.clone(), &s, &d, identity(), identity())
            .unwrap(),
        target
    );
}
#[test]
fn destination_animation_keeps_advancing_during_the_pose_blend() {
    let s = skeleton();
    let d = [None; 3];
    let from = vec![identity(); 3];
    let mut r = blend(&s, &d, &from);
    r.elapsed = 0.5;
    let early = vec![trs_matrix([2.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]); 3];
    let late = vec![trs_matrix([4.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]); 3];
    let a = r.apply(early, &s, &d, identity(), identity()).unwrap();
    let b = r.apply(late, &s, &d, identity(), identity()).unwrap();
    assert!((a[0][12] - 1.0).abs() < 0.00001);
    assert!((b[0][12] - 2.0).abs() < 0.00001);
}
#[test]
fn blend_waits_for_matching_playback_and_uses_unclamped_simulation_time() {
    let s = skeleton();
    let d = [None; 3];
    let from = vec![identity(); 3];
    let mut a = blend(&s, &d, &from);
    a.advance(10.0);
    assert_eq!(a.alpha(), 0.0);
    assert!(!a.start_for_clip("knockdown"));
    a.advance(1.0);
    assert_eq!(a.alpha(), 0.0);
    assert!(a.start_for_clip("getup"));
    let mut b = a.clone();
    for _ in 0..100 {
        a.advance(0.004);
    }
    b.advance(0.4);
    assert!((a.alpha() - b.alpha()).abs() < 0.00001);
    b.advance(f32::NAN);
    b.advance(-1.0);
    assert!((a.alpha() - b.alpha()).abs() < 0.00001);
    b.advance(30.0);
    assert_eq!(b.alpha(), 1.0);
}
#[test]
fn incompatible_transform_or_scale_is_rejected_without_a_deformed_palette() {
    let mut shear = identity();
    shear[4] = 0.3;
    assert!(decompose(shear).is_err());
    let mut nan = identity();
    nan[12] = f32::NAN;
    assert!(decompose(nan).is_err());
    let s = skeleton();
    let d = [None; 3];
    let from = vec![identity(); 3];
    let r = blend(&s, &d, &from);
    let reflected = vec![trs_matrix([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, -1.0]); 3];
    assert!(r.apply(reflected, &s, &d, identity(), identity()).is_err());
}
#[test]
fn clip_switch_and_stop_retire_the_captured_pose() {
    let s = skeleton();
    let d = [None; 3];
    let from = vec![identity(); 3];
    let mut layers = PoseLayers {
        recovery: Some(blend(&s, &d, &from)),
        ..Default::default()
    };
    assert!(layers.bind_recovery_clip("getup"));
    layers.advance(0.3);
    assert!(!layers.is_physical());
    assert!(layers.sample().requires_rigid_presentation());
    assert!(!layers.bind_recovery_clip("idle"));
    assert!(!layers.sample().requires_rigid_presentation());
    layers.recovery = Some(blend(&s, &d, &from));
    layers.cancel_recovery();
    assert!(!layers.sample().requires_rigid_presentation());
}

fn physical_state() -> SkinnedEntityAnimationState {
    let s = Arc::new(skeleton());
    let mut state = SkinnedEntityAnimationState::new(AssetId(1), identity(), s.clone()).unwrap();
    let g = vec![
        identity(),
        trs_matrix([0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
        trs_matrix([0.0, 2.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
    ];
    state.pose_layers.physical = Some(PhysicalPose {
        frozen_locals: locals_from_globals(&s, &g).unwrap().into(),
        globals: g.clone().into(),
        palette: g.into(),
        integrity: PhysicalSkeletonIntegrity::default(),
    });
    state
}
fn recovery_clip() -> ModelAnimationClip {
    ModelAnimationClip {
        name: "getup".into(),
        duration_seconds: 1.0,
        looping: false,
        mover: None,
        tracks: vec![],
    }
}
#[test]
fn invalid_target_clip_is_rejected_while_the_physical_pose_stays_owned() {
    let state = physical_state();
    let original = state.pose_layers.physical.as_ref().unwrap().globals.clone();
    let mut clip = recovery_clip();
    clip.tracks.push(newviso_model::JointAnimationTrack {
        joint: 0,
        translation_interpolation: AnimationInterpolation::Linear,
        rotation_interpolation: AnimationInterpolation::Linear,
        scale_interpolation: AnimationInterpolation::Linear,
        translations: vec![],
        rotations: vec![],
        scales: vec![AnimationVec3Key {
            time_seconds: 0.0,
            value: [-1.0, 1.0, 1.0],
        }],
    });
    assert!(prepare_recovery(&state, identity(), identity(), &clip, 0.0, 1.0).is_err());
    clip.tracks[0].joint = 100;
    assert!(prepare_recovery(&state, identity(), identity(), &clip, 0.0, 1.0).is_err());
    assert!(prepare_recovery(
        &state,
        identity(),
        identity(),
        &recovery_clip(),
        0.0,
        f32::NAN
    )
    .is_err());
    assert_eq!(
        state
            .pose_layers
            .physical
            .as_ref()
            .unwrap()
            .globals
            .as_ref(),
        original.as_ref()
    );
    assert!(state.pose_layers.recovery.is_none());
}
#[test]
fn prepared_capture_is_bound_to_the_installed_skeleton_and_preserves_world_pose() {
    let state = physical_state();
    let new = trs_matrix(
        [2.0, 1.1, -4.0],
        [0.0, 0.70710677, 0.0, 0.70710677],
        [1.0; 3],
    );
    let capture = prepare_recovery(&state, identity(), new, &recovery_clip(), 0.2, 0.35).unwrap();
    assert!(Arc::ptr_eq(&capture.skeleton, &state.skeleton));
    assert_eq!(capture.blend.alpha(), 0.0);
    assert!(!capture.blend.started);
    let palette = capture
        .blend
        .apply(
            vec![identity(); 3],
            &state.skeleton,
            &state.derived_joint_drivers,
            identity(),
            identity(),
        )
        .unwrap();
    for (a, b) in palette
        .iter()
        .zip(state.pose_layers.physical.as_ref().unwrap().globals.iter())
    {
        close(mul_mat4(new, *a), *b);
    }
}

#[test]
fn synchronous_seed_palette_starts_from_the_rebased_physical_hierarchy() {
    let state = physical_state();
    let new = trs_matrix(
        [3.0, 1.1, -5.0],
        [0.0, 0.70710677, 0.0, 0.70710677],
        [1.0; 3],
    );
    let capture = prepare_recovery(&state, identity(), new, &recovery_clip(), 0.2, 0.35).unwrap();
    assert_ne!(capture.initial_palette[0], identity());
    for (seed, physical) in capture
        .initial_palette
        .iter()
        .zip(state.pose_layers.physical.as_ref().unwrap().globals.iter())
    {
        close(mul_mat4(new, *seed), *physical);
    }
    assert!(!capture.blend.initial_pose_committed);
}

#[test]
fn length_metrology_keeps_absolute_checks_below_relative_f32_resolution() {
    let (absolute, relative) = measured_length_error(0.0001502, 0.00015, 4.0);
    assert!(absolute < 0.00001);
    assert!(relative.is_none());
    let (absolute, relative) = measured_length_error(0.00017, 0.00015, 4.0);
    assert!(absolute > 0.00001);
    assert!(
        relative.is_none(),
        "tiny stretched edges remain visible to the absolute gate"
    );
    let (_, relative) = measured_length_error(1.0015, 1.0, 4.0);
    assert!(
        relative.unwrap() > 0.001,
        "a resolved 0.15% stretch still fails"
    );
}
