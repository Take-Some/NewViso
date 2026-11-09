use super::*;
fn physical_test_skeleton() -> ModelSkeleton {
    let identity = trs_matrix([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
    let joint = |name: &str, parent, translation| newviso_model::ModelJoint {
        name: name.into(),
        tag: 1,
        parent,
        bind_translation: translation,
        bind_rotation: [0.0, 0.0, 0.0, 1.0],
        bind_scale: [1.0; 3],
        inverse_bind_matrix: identity,
    };
    ModelSkeleton {
        name: "physical-hierarchy".into(),
        joints: vec![
            joint("root", None, [0.0; 3]),
            joint("pelvis", Some(0), [0.0, 1.0, 0.0]),
            joint("lower", Some(1), [0.0, 1.0, 0.0]),
            joint("middle", Some(2), [0.0, 1.0, 0.0]),
            joint("tip", Some(3), [0.0, 1.0, 0.0]),
            joint("helper", Some(1), [0.5, 0.0, 0.0]),
        ],
    }
}
#[test]
fn physical_hierarchy_keeps_lengths_and_scale_under_independent_body_drift() {
    let skeleton = physical_test_skeleton();
    let frozen: Vec<_> = skeleton
        .joints
        .iter()
        .map(|j| trs_matrix(j.bind_translation, j.bind_rotation, j.bind_scale))
        .collect();
    let mut drivers = vec![None; 6];
    drivers[5] = Some(DerivedJointDriver {
        joint: 2,
        bind_offset: trs_matrix([0.25, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
    });
    let q = [0.0, 0.0, 0.70710677, 0.70710677];
    let requested = BTreeMap::from([
        (1, trs_matrix([2.0, 1.0, 0.0], q, [3.0; 3])),
        (
            2,
            trs_matrix([50.0, 10.0, 0.0], [0.0, 0.0, 0.0, 1.0], [7.0, 9.0, 2.0]),
        ),
        (4, trs_matrix([200.0, 300.0, 400.0], q, [8.0, 9.0, 10.0])),
    ]);
    let (pose, integrity) = physical_globals(&skeleton, &frozen, &requested, &drivers).unwrap();
    for (index, expected) in [
        (1, [2.0, 1.0, 0.0]),
        (2, [1.0, 1.0, 0.0]),
        (3, [1.0, 2.0, 0.0]),
        (4, [1.0, 3.0, 0.0]),
        (5, [1.25, 1.0, 0.0]),
    ] {
        for axis in 0..3 {
            assert!((pose[index][12 + axis] - expected[axis]).abs() < 1e-5);
        }
    }
    assert_eq!(integrity.constrained_bones, 3);
    assert!(integrity.max_bone_length_error < 1e-5);
    assert!(integrity.max_relative_scale_error < 1e-5);
    assert!(integrity.max_basis_shear < 1e-5);
}
#[test]
fn physical_rotation_preserves_authored_reflection_and_removes_shear() {
    let rest = trs_matrix([0.0, 0.7, 0.0], [0.0, 0.0, 0.0, 1.0], [-1.0, 2.0, 3.0]);
    let mut candidate = trs_matrix([4.0, 8.0, 9.0], [0.0, 0.0, 0.0, 1.0], [-9.0, 8.0, 7.0]);
    candidate[4] = 0.6;
    let local = physical_local(candidate, rest, true).unwrap();
    assert_eq!(&local[12..15], &rest[12..15]);
    for (column, scale) in [1.0, 2.0, 3.0].into_iter().enumerate() {
        assert!((physical_length(physical_axis(local, column)) - scale).abs() < 1e-5);
    }
    assert!(physical_dot(physical_axis(local, 0), physical_axis(local, 1)).abs() < 1e-5);
    assert!(
        physical_dot(
            physical_cross(physical_axis(local, 0), physical_axis(local, 1)),
            physical_axis(local, 2)
        ) < 0.0
    );
    assert!(physical_local([0.0; 16], rest, true).is_err());
}

#[test]
fn coincident_helper_offsets_do_not_amplify_floating_point_roundoff() {
    let skeleton = physical_test_skeleton();
    let mut frozen: Vec<_> = skeleton
        .joints
        .iter()
        .map(|j| trs_matrix(j.bind_translation, j.bind_rotation, j.bind_scale))
        .collect();
    frozen[3] = trs_matrix([1e-7, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
    let requested = BTreeMap::from([(
        1,
        trs_matrix([2.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
    )]);
    let (_, integrity) =
        physical_globals(&skeleton, &frozen, &requested, &vec![None; frozen.len()]).unwrap();
    assert_eq!(integrity.subresolution_bones, 1);
    assert!(integrity.max_bone_length_error < 1e-6);
    assert!(integrity.max_relative_bone_length_error < 1e-6);
}
#[test]
fn micron_helpers_report_global_quantization_separately_from_skeleton_deformation() {
    let skeleton = physical_test_skeleton();
    let mut frozen: Vec<_> = skeleton
        .joints
        .iter()
        .map(|j| trs_matrix(j.bind_translation, j.bind_rotation, j.bind_scale))
        .collect();
    frozen[3] = trs_matrix([1.4e-6, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
    let requested = BTreeMap::from([(
        1,
        trs_matrix([0.5, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
    )]);
    let (_, integrity) =
        physical_globals(&skeleton, &frozen, &requested, &vec![None; frozen.len()]).unwrap();
    assert!(
        integrity.max_relative_bone_length_error > 0.001,
        "retain raw global roundoff evidence"
    );
    assert!(integrity.max_relative_local_bone_length_error < 1e-6);
    assert!(integrity.max_bone_length_error < 1e-7);
}
