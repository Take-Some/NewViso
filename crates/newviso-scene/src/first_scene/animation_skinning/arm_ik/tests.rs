use super::*;
use newviso_model::ModelJoint;
#[test]
fn arm_reaches_tip_without_stretching_and_preserves_other_bones() {
    let mut joints = Vec::new();
    let translations = [
        [0.0, 0.0, 0.0],
        [0.4, 0.0, 0.0],
        [0.35, 0.0, 0.0],
        [0.04, 0.0, 0.0],
        [0.0, -1.0, 0.0],
    ];
    let mut globals = Vec::new();
    for (i, t) in translations.into_iter().enumerate() {
        let parent = match i {
            0 | 4 => None,
            _ => Some((i - 1) as u16),
        };
        let rotation = if i == 3 {
            [
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
            ]
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };
        let local = trs_matrix(t, rotation, [1.0; 3]);
        let global = parent.map_or(local, |p| mul_mat4(globals[p as usize], local));
        globals.push(global);
        joints.push(ModelJoint {
            name: format!("j{i}"),
            tag: i as u32,
            parent,
            inverse_bind_matrix: inverse_affine(global).unwrap(),
            bind_translation: t,
            bind_rotation: rotation,
            bind_scale: [1.0; 3],
        });
    }
    let skeleton = ModelSkeleton {
        name: "test".into(),
        joints,
    };
    let other = globals[4];
    let c = SceneArmIkConstraint {
        upper_joint: "j0".into(),
        lower_joint: "j1".into(),
        hand_joint: "j2".into(),
        tip_joint: "j3".into(),
        target: [0.3, 0.4, -0.3],
        pole: [0.0, -1.0, 0.0],
        rotation_degrees: [0.0; 3],
        weight: 1.0,
        hand_pose_weight: 1.0,
        joint_rotations: Vec::new(),
    };
    apply_to_globals(&mut globals, &skeleton, &vec![None; 5], &[c]).unwrap();
    assert!(pos(globals[3]).sub(Vec3::new(0.3, 0.4, -0.3)).length() < 1.0e-4);
    assert!((pos(globals[1]).sub(pos(globals[0])).length() - 0.4).abs() < 1.0e-4);
    assert!((pos(globals[2]).sub(pos(globals[1])).length() - 0.35).abs() < 1.0e-4);
    assert_eq!(globals[4], other);
    assert!(
        Vec3::new(globals[3][8], globals[3][9], globals[3][10])
            .sub(Vec3::new(0.0, 0.0, 1.0))
            .length()
            < 1.0e-4
    );
}
#[test]
fn equipped_fingers_hold_grip_after_arms_return_to_locomotion() {
    let translations = [
        [0.0; 3],
        [0.3, 0.0, 0.0],
        [0.25, 0.0, 0.0],
        [0.04, 0.0, 0.0],
        [0.08, 0.0, 0.0],
        [0.04, 0.0, 0.0],
    ];
    let parents = [None, Some(0), Some(1), Some(2), Some(2), Some(4)];
    let mut globals = Vec::new();
    let mut joints = Vec::new();
    for (i, t) in translations.into_iter().enumerate() {
        let local = trs_matrix(t, [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
        let global = parents[i].map_or(local, |p| mul_mat4(globals[p as usize], local));
        globals.push(global);
        joints.push(ModelJoint {
            name: format!("j{i}"),
            tag: i as u32,
            parent: parents[i],
            inverse_bind_matrix: inverse_affine(global).unwrap(),
            bind_translation: t,
            bind_rotation: [0.0, 0.0, 0.0, 1.0],
            bind_scale: [1.0; 3],
        });
    }
    let before = globals.clone();
    let skeleton = ModelSkeleton {
        name: "grip".into(),
        joints,
    };
    let c = SceneArmIkConstraint {
        upper_joint: "j0".into(),
        lower_joint: "j1".into(),
        hand_joint: "j2".into(),
        tip_joint: "j3".into(),
        target: [10.0; 3],
        pole: [0.0, -1.0, 0.0],
        rotation_degrees: [90.0; 3],
        weight: 0.0,
        hand_pose_weight: 1.0,
        joint_rotations: vec![("j4".into(), [0.0, 60.0, 0.0])],
    };
    apply_to_globals(&mut globals, &skeleton, &vec![None; 6], &[c]).unwrap();
    assert_eq!(
        &globals[..4],
        &before[..4],
        "released arms and weapon socket keep their locomotion pose"
    );
    assert!(pos(globals[4]).sub(pos(before[4])).length() < 1e-6);
    assert!((pos(globals[5]).sub(pos(globals[4])).length() - 0.04).abs() < 1e-6);
    assert!(
        pos(globals[5]).z < -0.03,
        "finger curls around grip independently of arm IK"
    );
}

#[test]
fn renamed_scaled_rotated_rigs_with_twist_helpers_preserve_every_segment() {
    for scale in [0.35_f32, 1.0, 2.4] {
        let translations = [
            [0.0; 3],
            [0.07, 0.0, 0.0],
            [0.23, 0.0, 0.0],
            [0.06, 0.0, 0.0],
            [0.19, 0.0, 0.0],
            [0.035, 0.0, 0.0],
            [0.0, -0.8, 0.0],
        ];
        let parents = [None, Some(0), Some(1), Some(2), Some(3), Some(4), None];
        let mut globals = Vec::new();
        let mut joints = Vec::new();
        for (i, t) in translations.into_iter().enumerate() {
            let t = t.map(|v| v * scale);
            let rotation = if parents[i].is_none() {
                [0.0, 0.38268343, 0.0, 0.9238795]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            };
            let local = trs_matrix(t, rotation, [1.0; 3]);
            let global = parents[i].map_or(local, |p| mul_mat4(globals[p as usize], local));
            globals.push(global);
            joints.push(ModelJoint {
                name: format!("foreign:{i}"),
                tag: i as u32,
                parent: parents[i],
                inverse_bind_matrix: inverse_affine(global).unwrap(),
                bind_translation: t,
                bind_rotation: rotation,
                bind_scale: [1.0; 3],
            });
        }
        let before = globals.clone();
        let skeleton = ModelSkeleton {
            name: "helper_chain".into(),
            joints,
        };
        let target = [0.18 * scale, 0.2 * scale, -0.18 * scale];
        let c = SceneArmIkConstraint {
            upper_joint: "foreign:0".into(),
            lower_joint: "foreign:2".into(),
            hand_joint: "foreign:4".into(),
            tip_joint: "foreign:5".into(),
            target,
            pole: [0.0, -scale, 0.0],
            rotation_degrees: [0.0; 3],
            weight: 1.0,
            hand_pose_weight: 0.0,
            joint_rotations: Vec::new(),
        };
        apply_to_globals(&mut globals, &skeleton, &vec![None; 7], &[c]).unwrap();
        assert!(
            pos(globals[5])
                .sub(Vec3::new(target[0], target[1], target[2]))
                .length()
                < 1e-4
        );
        for i in 1..6 {
            let p = parents[i].unwrap() as usize;
            assert!(
                (pos(globals[i]).sub(pos(globals[p])).length()
                    - pos(before[i]).sub(pos(before[p])).length())
                .abs()
                    < 1e-5
            );
        }
        assert_eq!(globals[6], before[6]);
    }
}
