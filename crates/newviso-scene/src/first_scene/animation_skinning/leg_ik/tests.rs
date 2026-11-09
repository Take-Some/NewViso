use super::*;
fn identity() -> [f32; 16] {
    trs_matrix([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3])
}
fn fixture(count: usize) -> (ModelSkeleton, Vec<[f32; 16]>, Vec<SceneLegIkConstraint>) {
    let mut skeleton = ModelSkeleton {
        name: "foreign.multi_support".into(),
        joints: Vec::new(),
    };
    let mut globals = Vec::new();
    let mut legs = Vec::new();
    for i in 0..count {
        let start = skeleton.joints.len();
        let x = i as f32 * 0.25;
        let positions = [[x, 0.9, 0.0], [x, 0.5, 0.05], [x, 0.1, 0.0]];
        for (k, name) in ["upper", "lower", "end"].iter().enumerate() {
            let parent = if k == 0 {
                None
            } else {
                Some((start + k - 1) as u16)
            };
            let local = if k == 0 {
                positions[k]
            } else {
                std::array::from_fn(|j| positions[k][j] - positions[k - 1][j])
            };
            skeleton.joints.push(newviso_model::ModelJoint {
                name: format!("foreign_{i}_{name}"),
                tag: 0,
                parent,
                bind_translation: local,
                bind_rotation: [0.0, 0.0, 0.0, 1.0],
                bind_scale: [1.0; 3],
                inverse_bind_matrix: identity(),
            });
            globals.push(trs_matrix(positions[k], [0.0, 0.0, 0.0, 1.0], [1.0; 3]));
        }
        legs.push(SceneLegIkConstraint {
            hip_joint: format!("foreign_{i}_upper"),
            knee_joint: format!("foreign_{i}_lower"),
            foot_joint: format!("foreign_{i}_end"),
            target: [x, 0.15, -0.1],
            pole: [x, 0.5, 0.4],
            rotation_degrees: [0.0; 3],
            weight: 1.0,
            support: None,
        });
    }
    (skeleton, globals, legs)
}
fn pos(m: [f32; 16]) -> [f32; 3] {
    [m[12], m[13], m[14]]
}
fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
}
#[test]
fn one_four_and_six_support_limbs_use_the_same_length_preserving_palette_path() {
    for count in [1, 4, 6] {
        let (skeleton, original, legs) = fixture(count);
        let constraints = legs
            .iter()
            .map(SceneLegIkConstraint::solver_constraint)
            .collect::<Vec<_>>();
        validate_leg_constraints(&constraints).unwrap();
        let drivers = vec![None; skeleton.joints.len()];
        let mut globals = original.clone();
        arm_ik::apply_to_globals(&mut globals, &skeleton, &drivers, &constraints).unwrap();
        let mut palette = original.clone();
        arm_ik::apply_to_palette(&mut palette, &skeleton, &drivers, identity(), &constraints)
            .unwrap();
        for (i, leg) in legs.iter().enumerate() {
            let base = i * 3;
            assert!(dist(pos(globals[base + 2]), leg.target) < 1e-5);
            for k in 0..2 {
                assert!(
                    (dist(pos(globals[base + k]), pos(globals[base + k + 1]))
                        - dist(pos(original[base + k]), pos(original[base + k + 1])))
                    .abs()
                        < 1e-5
                );
            }
        }
        for (a, b) in globals.iter().zip(palette) {
            assert!((0..16).all(|i| (a[i] - b[i]).abs() < 1e-5));
        }
    }
}
#[test]
fn supporting_limbs_do_not_relax_weapon_arm_count_or_allow_shared_drivers() {
    let (_, _, legs) = fixture(4);
    let mut c = legs
        .iter()
        .map(SceneLegIkConstraint::solver_constraint)
        .collect::<Vec<_>>();
    assert!(arm_ik::layers::validate_arm_constraints(&c).is_err());
    assert!(validate_leg_constraints(&c).is_ok());
    c[1].hand_joint = c[0].hand_joint.clone();
    assert!(validate_leg_constraints(&c).is_err());
}
