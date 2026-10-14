#[test]
    fn palette_blending_preserves_weighted_positions_through_interrupted_transitions() {
        let a=vec![axis_rotation(0,0.1),axis_rotation(1,0.2)];
        let mut b=vec![axis_rotation(1,0.7),axis_rotation(2,-0.6)];
        b[0][12]=2.;b[1][13]=3.;
        let c=vec![axis_rotation(2,1.2),axis_rotation(0,-0.2)];
        let vertex=AssetTriangleVertex {
            position:Vec3::new(0.2,1.1,-0.3),normal:Vec3::new(0.,1.,0.),
            tangent:[1.,0.,0.,1.],color:[1.;4],uv:[0.1,0.2],
            joints:[0,1,0,1,0,1,0,1],weights:[0.125;8],skin_influences:8,
        };
        let va=skin_vertices_to_packed(&[vertex],&a).unwrap();
        let vb=skin_vertices_to_packed(&[vertex],&b).unwrap();
        let vc=skin_vertices_to_packed(&[vertex],&c).unwrap();
        let mut current=a.clone();
        interpolate_skin_palette(&mut current,&a,&b,0.37);
        let intermediate=skin_vertices_to_packed(&[vertex],&current).unwrap();
        for axis in 0..3 {
            assert!((intermediate[axis]-(va[axis]+(vb[axis]-va[axis])*0.37)).abs()<1e-5);
        }
        let interrupted=current.clone();
        interpolate_skin_palette(&mut current,&interrupted,&c,0.61);
        let displayed=skin_vertices_to_packed(&[vertex],&current).unwrap();
        for axis in 0..3 {
            assert!((displayed[axis]-(intermediate[axis]+(vc[axis]-intermediate[axis])*0.61)).abs()<1e-5);
        }
        assert_eq!(&displayed[7..13],&va[7..13]);
    }

    #[test]
    fn presented_foot_markers_follow_exact_vertex_blend_alpha() {
        let from = Some([[1.0, 0.2, 2.0], [3.0, 0.5, 4.0]]);
        let to = Some([[5.0, 0.0, 6.0], [7.0, 0.1, 8.0]]);
        assert_eq!(interpolate_presented_feet(from, to, 0.0), from);
        assert_eq!(interpolate_presented_feet(from, to, 1.0), to);
        let middle = interpolate_presented_feet(from, to, 0.5).unwrap();
        assert!((middle[0][0] - 3.0).abs() < 1e-6);
        assert!((middle[0][1] - 0.1).abs() < 1e-6);
        assert!((middle[1][1] - 0.3).abs() < 1e-6);
        assert_eq!(interpolate_presented_feet(None, to, 0.25), to);
        assert_eq!(interpolate_presented_feet(from, None, 0.25), None);
    }

    #[test]
    fn worker_foot_markers_reuse_skinning_palette_bind_space() {
        let identity = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        let skeleton = ModelSkeleton {
            name: "foot-test".into(),
            joints: ["SKEL_L_Foot", "SKEL_R_Foot"].into_iter()
                .enumerate()
                .map(|(index, name)| newviso_model::ModelJoint {
                    name: name.into(), tag: (index + 1) as u32, parent: None,
                    inverse_bind_matrix: identity,
                    bind_translation: [0.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                }).collect(),
        };
        let palette = [
            trs_matrix([0.0, 0.14, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
            trs_matrix([0.1, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
        ];
        let indices = find_foot_joint_indices(&skeleton).unwrap();
        assert_eq!(indices, [0, 1]);
        let feet = skin_palette_feet_positions(&skeleton, &palette, identity, indices).unwrap();
        assert!((feet[0][1] - 0.14).abs() < 1e-6);
        assert!((feet[1][0] - 0.1).abs() < 1e-6);
        assert_eq!(feet[1][1], 0.0);
    }

    use super::*;

    fn axis_rotation(axis: usize, angle: f32) -> [f32; 16] {
        let mut q = [0.0, 0.0, 0.0, (angle * 0.5).cos()];
        q[axis] = (angle * 0.5).sin();
        trs_matrix([0.0; 3], q, [1.0; 3])
    }

    #[test]
    fn camera_head_bob_removes_imported_root_half_turn() {
        let root = axis_rotation(1, std::f32::consts::PI);
        let neutral_root = axis_rotation(1, 0.0);
        let bone = axis_rotation(2, std::f32::consts::FRAC_PI_2);
        // GTA locomotion replaces the skeleton's half-turn bind root. Only a
        // seven degree head nod should reach the view, never +/-180 pitch/roll.
        let head = mul_mat4(axis_rotation(0, 7.0_f32.to_radians()), bone);
        let angles = joint_camera_animation_angles(mul_mat4(root, bone), head, root, neutral_root).unwrap();
        for (actual, expected) in angles.into_iter().zip([7.0, 0.0, 0.0]) {
            assert!((actual - expected).abs() < 1.0e-4, "{angles:?}");
        }
    }

    #[test]
    fn camera_head_angles_preserve_pitch_heading_roll_signs() {
        let root = axis_rotation(0, -std::f32::consts::FRAC_PI_2);
        let bone = axis_rotation(2, 0.3);
        let desired = mul_mat4(axis_rotation(1, -20.0_f32.to_radians()),
            mul_mat4(axis_rotation(0, 7.0_f32.to_radians()), axis_rotation(2, -3.0_f32.to_radians())));
        let bind = mul_mat4(root, bone);
        let animated = mul_mat4(desired, bind);
        let angles = joint_camera_animation_angles(bind, animated, root, root).unwrap();
        for (actual, expected) in angles.into_iter().zip([7.0, 20.0, -3.0]) {
            assert!((actual - expected).abs() < 1.0e-4, "{angles:?}");
        }
    }

    #[test]
    fn camera_head_angles_do_not_flip_pitch_at_heading_half_turn() {
        let identity = axis_rotation(0, 0.0);
        for heading in [179.0_f32, 180.0, 181.0] {
            let pose = mul_mat4(axis_rotation(1, -heading.to_radians()), axis_rotation(0, 5.0_f32.to_radians()));
            let angles = joint_camera_animation_angles(identity, pose, identity, identity).unwrap();
            assert!((angles[0] - 5.0).abs() < 1.0e-4, "{angles:?}");
            assert!(angles[2].abs() < 1.0e-4, "{angles:?}");
        }
    }

    #[test]
    fn looping_vec3_sampling_blends_across_clip_seam() {
        let keys = [
            AnimationVec3Key {
                time_seconds: 0.0,
                value: [0.0, 0.0, 0.0],
            },
            AnimationVec3Key {
                time_seconds: 0.75,
                value: [4.0, 0.0, 0.0],
            },
        ];
        let sampled = sample_vec3_clip(
            &keys,
            AnimationInterpolation::Linear,
            0.875,
            true,
            1.0,
            [0.0; 3],
        );
        assert!((sampled[0] - 2.0).abs() < 1.0e-5, "{sampled:?}");
    }

    #[test]
    fn looping_quaternion_sampling_has_no_end_to_start_snap() {
        let keys = [
            AnimationQuatKey {
                time_seconds: 0.0,
                value: [0.0, 0.0, 0.0, 1.0],
            },
            AnimationQuatKey {
                time_seconds: 0.75,
                value: [0.0, 1.0, 0.0, 0.0],
            },
        ];
        let sampled = sample_quat_clip(
            &keys,
            AnimationInterpolation::Linear,
            0.875,
            true,
            1.0,
            [0.0, 0.0, 0.0, 1.0],
        );
        let expected = 0.70710677_f32;
        assert!((sampled[1].abs() - expected).abs() < 1.0e-4, "{sampled:?}");
        assert!((sampled[3].abs() - expected).abs() < 1.0e-4, "{sampled:?}");
    }

    #[test]
    fn blended_palette_crossfades_local_translation() {
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let skeleton = ModelSkeleton {
            name: "blend-test".to_owned(),
            joints: vec![newviso_model::ModelJoint {
                name: "root".to_owned(),
                tag: 1,
                parent: None,
                inverse_bind_matrix: identity,
                bind_translation: [0.0, 0.0, 0.0],
                bind_rotation: [0.0, 0.0, 0.0, 1.0],
                bind_scale: [1.0, 1.0, 1.0],
            }],
        };
        let drivers = build_derived_joint_drivers(&skeleton).unwrap();
        let make_clip = |name: &str, x: f32| ModelAnimationClip {
            name: name.to_owned(),
            duration_seconds: 1.0,
            looping: true,
            tracks: vec![newviso_model::JointAnimationTrack {
                joint: 0,
                translation_interpolation: AnimationInterpolation::Linear,
                rotation_interpolation: AnimationInterpolation::Linear,
                scale_interpolation: AnimationInterpolation::Linear,
                translations: vec![AnimationVec3Key {
                    time_seconds: 0.0,
                    value: [x, 0.0, 0.0],
                }],
                rotations: Vec::new(),
                scales: Vec::new(),
            }],
            mover: None,
        };
        let from = make_clip("from", 0.0);
        let to = make_clip("to", 10.0);
        let palette = build_blended_skin_palette(
            &skeleton, &drivers, &from, 0.0, &to, 0.0, 0.5, identity, identity,
        )
        .unwrap();
        let moved = transform_point_mat4(palette[0], Vec3::ZERO);
        assert!((moved.x - 5.0).abs() < 1.0e-5, "x={}", moved.x);
    }

    #[test]
    fn continuous_clip_clock_wraps_without_quantizing_to_sample_rate() {
        let clip = ModelAnimationClip {
            name: "clock".to_owned(),
            duration_seconds: 1.0,
            looping: true,
            tracks: Vec::new(),
            mover: None,
        };
        let t = advance_clip_time(&clip, 0.99, 1.0 / 120.0);
        assert!(t > 0.998 && t < 1.0, "t={t}");
        let wrapped = advance_clip_time(&clip, t, 1.0 / 120.0);
        assert!(wrapped > 0.006 && wrapped < 0.008, "wrapped={wrapped}");
    }

    #[test]
    fn quaternion_slerp_preserves_unit_length() {
        let q = slerp_quat([0.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 0.0], 0.5);
        let length = q.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((length - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn untracked_helper_chain_follows_animated_core_joint() {
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let child_inverse_bind = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 1.0,
        ];
        let skeleton = ModelSkeleton {
            name: "helper-test".to_owned(),
            joints: vec![
                newviso_model::ModelJoint {
                    name: "root".to_owned(),
                    tag: 1,
                    parent: None,
                    inverse_bind_matrix: identity,
                    bind_translation: [0.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                },
                newviso_model::ModelJoint {
                    name: "root_helper".to_owned(),
                    tag: 2,
                    parent: None,
                    inverse_bind_matrix: identity,
                    bind_translation: [0.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                },
                newviso_model::ModelJoint {
                    name: "roll".to_owned(),
                    tag: 3,
                    parent: Some(1),
                    inverse_bind_matrix: child_inverse_bind,
                    bind_translation: [1.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                },
            ],
        };
        let drivers = build_derived_joint_drivers(&skeleton).unwrap();
        assert_eq!(drivers[1].map(|driver| driver.joint), Some(0));

        let clip = ModelAnimationClip {
            name: "move-root".to_owned(),
            duration_seconds: 1.0,
            looping: true,
            tracks: vec![newviso_model::JointAnimationTrack {
                joint: 0,
                translation_interpolation: AnimationInterpolation::Linear,
                rotation_interpolation: AnimationInterpolation::Linear,
                scale_interpolation: AnimationInterpolation::Linear,
                translations: vec![AnimationVec3Key {
                    time_seconds: 0.0,
                    value: [2.0, 0.0, 0.0],
                }],
                rotations: Vec::new(),
                scales: Vec::new(),
            }],
            mover: None,
        };
        let palette =
            build_skin_palette(&skeleton, &drivers, &clip, 0.0, identity, identity).unwrap();

        for joint in 0..3 {
            let moved = transform_point_mat4(palette[joint], Vec3::ZERO);
            assert!(
                (moved.x - 2.0).abs() < 1.0e-5,
                "joint {joint} did not inherit core motion: x={}",
                moved.x
            );
        }
    }

    #[test]
    fn sibling_thigh_roll_follows_animated_thigh() {
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let inverse_x = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 1.0,
        ];
        let skeleton = ModelSkeleton {
            name: "rsc7-roll-test".to_owned(),
            joints: vec![
                newviso_model::ModelJoint {
                    name: "SKEL_ROOT".to_owned(),
                    tag: 0,
                    parent: None,
                    inverse_bind_matrix: identity,
                    bind_translation: [0.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                },
                newviso_model::ModelJoint {
                    name: "SKEL_L_Thigh".to_owned(),
                    tag: 58271,
                    parent: Some(0),
                    inverse_bind_matrix: inverse_x,
                    bind_translation: [1.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                },
                newviso_model::ModelJoint {
                    name: "RB_L_ThighRoll".to_owned(),
                    tag: 23639,
                    parent: Some(0),
                    inverse_bind_matrix: inverse_x,
                    bind_translation: [1.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                },
            ],
        };
        let drivers = build_derived_joint_drivers(&skeleton).unwrap();
        assert_eq!(drivers[2].map(|driver| driver.joint), Some(1));

        let clip = ModelAnimationClip {
            name: "move-thigh".to_owned(),
            duration_seconds: 1.0,
            looping: true,
            tracks: vec![newviso_model::JointAnimationTrack {
                joint: 1,
                translation_interpolation: AnimationInterpolation::Linear,
                rotation_interpolation: AnimationInterpolation::Linear,
                scale_interpolation: AnimationInterpolation::Linear,
                translations: vec![AnimationVec3Key {
                    time_seconds: 0.0,
                    value: [2.0, 0.0, 0.0],
                }],
                rotations: Vec::new(),
                scales: Vec::new(),
            }],
            mover: None,
        };

        let palette =
            build_skin_palette(&skeleton, &drivers, &clip, 0.0, identity, identity).unwrap();
        let moved = transform_point_mat4(palette[2], Vec3::ZERO);
        assert!((moved.x - 1.0).abs() < 1.0e-5, "roll x={}", moved.x);
    }

    #[test]
    fn affine_inverse_roundtrip_is_identity() {
        let m = trs_matrix(
            [1.0, 2.0, -3.0],
            [0.0, 0.38268343, 0.0, 0.9238795],
            [1.2, 0.8, 1.1],
        );
        let inv = inverse_affine(m).unwrap();
        let product = mul_mat4(m, inv);
        for (index, value) in product.iter().enumerate() {
            let expected = if index % 5 == 0 { 1.0 } else { 0.0 };
            assert!((value - expected).abs() < 1.0e-4);
        }
    }
