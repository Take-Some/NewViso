use super::*;
fn spec() -> Spec {
    serde_json::from_value(
        json!({"entity":1,"anchor_entity":20,"joint_ids":[30,31],"local_pivot":[0,0,0],
        "frame_rotation":[0,0,0,1],"mass":25,"friction":0.6,"restitution":0.02,
        "linear_damping":0.01,"angular_damping":0.08,"span_fraction":0.75,"max_force":50000,
        "angle_limits":[-1.7,1.7],"stop_stiffness":1600,"stop_damping":12,"stop_max_torque":800,
        "collision_inset":[0,0,0]}),
    )
    .unwrap()
}
fn source() -> newviso_scene::ScenePhysicsSource {
    newviso_scene::ScenePhysicsSource {
        entity: 1,
        name: "authored.leaf".into(),
        asset_ref: Some("models/leaf".into()),
        position: [4.0, 1.5, -3.0],
        rotation_degrees: [0.0, 37.0, 0.0],
        scale: [1.0; 3],
        local_bounds: Some(([0.0, -1.0, -0.06], [1.4, 1.0, 0.06])),
        dynamic: false,
        solid: false,
        physics_claimed: false,
    }
}
#[test]
fn an_existing_hinge_pivot_and_noncentered_model_keep_the_same_closed_visual_pose() {
    for scale in [[1.0; 3], [-1.0, 2.0, 0.5]] {
        let mut s = source();
        s.scale = scale;
        let p = spec();
        let g = Geometry::from_source(&s, &p).unwrap();
        let offset = rotate_camera_vector(g.visual_offset, g.rotation);
        let anchor = rotate_camera_vector(g.child_pivot, g.rotation);
        for i in 0..3 {
            assert!((g.center[i] + offset[i] - s.position[i]).abs() < 1e-5);
            assert!((g.center[i] + anchor[i] - g.hinge_position[i]).abs() < 1e-5);
        }
        assert!((g.half_extents[0] - 0.7).abs() < 1e-6);
        assert!((g.half_extents[1] - scale[1].abs()).abs() < 1e-6);
    }
}
#[test]
fn bottom_pivots_use_the_same_hinge_line_and_invalid_bounds_do_not_activate() {
    let mut s = source();
    s.local_bounds = Some(([0.0, 0.0, -0.05], [1.0, 2.4, 0.05]));
    let mut p = spec();
    let g = Geometry::from_source(&s, &p).unwrap();
    assert!((g.hinge_position[1] - 2.7).abs() < 1e-5);
    p.collision_inset = [0.0, 2.0, 0.0];
    assert!(Geometry::from_source(&s, &p).is_err());
    p = spec();
    p.frame_rotation = [0.0; 4];
    assert!(p.validate().is_err());
}
#[test]
fn pending_bindings_reserve_their_body_and_joint_ids_and_are_idempotent() {
    let mut p = PhysicsRuntime::empty_for_tests();
    p.supports_joints = true;
    p.supports_articulation = true;
    let s = spec();
    let v = serde_json::to_value(&s).unwrap();
    p.bind_scene_hinge(&v).unwrap();
    p.bind_scene_hinge(&v).unwrap();
    assert_eq!(p.scene_hinges.entries.len(), 1);
    let mut other = s.clone();
    other.entity = 2;
    other.anchor_entity = 21;
    assert!(p
        .bind_scene_hinge(&serde_json::to_value(other).unwrap())
        .is_err());
    let mut other = s;
    other.entity = 3;
    other.joint_ids = [32, 33];
    assert!(p
        .bind_scene_hinge(&serde_json::to_value(other).unwrap())
        .is_err());
}
