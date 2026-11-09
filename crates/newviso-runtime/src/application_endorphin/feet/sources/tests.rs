use super::*;
use newviso_model::{ModelJoint, ModelSkeleton};
fn anatomy(n: usize) -> (ModelSkeleton, Vec<LegBinding>) {
    let mut s = ModelSkeleton {
        name: "unknown_support_rig".into(),
        joints: Vec::new(),
    };
    let mut legs = Vec::new();
    for i in 0..n {
        let base = s.joints.len();
        let names = ["socket", "hinge", "sole", "front"].map(|name| format!("branch.{i}.{name}"));
        for (j, name) in names.iter().enumerate() {
            s.joints.push(ModelJoint {
                name: name.clone(),
                tag: 0,
                parent: (j > 0).then(|| (base + j - 1) as u16),
                inverse_bind_matrix: [0.0; 16],
                bind_translation: [0.0; 3],
                bind_rotation: [0.0, 0.0, 0.0, 1.0],
                bind_scale: [1.0; 3],
            });
        }
        legs.push(LegBinding {
            hip: names[0].clone(),
            knee: names[1].clone(),
            foot: names[2].clone(),
            toe: names[3].clone(),
            sole_height: 0.05,
            sole_half_width: Some(0.04),
            heel_length: Some(0.03),
        });
    }
    (s, legs)
}
#[test]
fn anatomy_validation_accepts_foreign_names_and_multiple_independent_branches() {
    for n in [1, 2, 4, 6, 32] {
        let (s, legs) = anatomy(n);
        validate_leg_skeleton(&s, &legs).unwrap();
    }
}
#[test]
fn missing_joints_wrong_ancestry_and_overlapping_branches_are_rejected() {
    let (mut s, legs) = anatomy(2);
    s.joints[4].parent = Some(3);
    assert!(validate_leg_skeleton(&s, &legs).is_err());
    let (mut s, legs) = anatomy(1);
    s.joints[2].parent = Some(0);
    assert!(validate_leg_skeleton(&s, &legs).is_err());
    let (s, mut legs) = anatomy(1);
    legs[0].toe = "absent".into();
    assert!(validate_leg_skeleton(&s, &legs).is_err());
    let (mut s, legs) = anatomy(1);
    s.joints[2].parent = Some(999);
    assert!(validate_leg_skeleton(&s, &legs).is_err());
}
