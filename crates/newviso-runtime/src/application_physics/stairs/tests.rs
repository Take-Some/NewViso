use super::*;
#[test]
fn foot_geometry_comes_from_each_native_body_without_enabling_stair_drive() {
    let mut p = PhysicsRuntime::empty_for_tests();
    for (id, shape, height) in [
        (
            1,
            json!({"kind":"capsule","radius":0.2,"half_height":0.5}),
            0.7,
        ),
        (2, json!({"kind":"box","half_extents":[0.3,0.8,0.4]}), 0.8),
        (3, json!({"kind":"sphere","radius":0.4}), 0.4),
        (
            4,
            json!({"kind":"cylinder","radius":0.3,"half_height":0.6}),
            0.6,
        ),
    ] {
        p.upsert_body_from_script(
            &json!({"entity":id,"body_kind":"dynamic","position":[0,2,0],"shape":shape}),
            0,
        )
        .unwrap();
        let cfg = p.character_foot_config(id).unwrap();
        assert!((cfg.radius + cfg.half_height - height).abs() < 1e-6);
    }
    assert!(p.character_stairs.is_empty());
    assert!(p.script_character_drives.is_empty());
    p.upsert_body_from_script(
        &json!({"entity":2,"body_kind":"dynamic","position":[0,2,0],
        "shape":{"kind":"box","half_extents":[0.2,0.4,0.2]}}),
        0,
    )
    .unwrap();
    let cfg = p.character_foot_config(2).unwrap();
    assert!((cfg.radius + cfg.half_height - 0.4).abs() < 1e-6);
}
#[test]
fn static_and_trigger_shapes_cannot_own_animated_feet() {
    let mut p = PhysicsRuntime::empty_for_tests();
    for (id, kind, trigger) in [(1, "static", false), (2, "dynamic", true)] {
        p.upsert_body_from_script(
            &json!({"entity":id,"body_kind":kind,"position":[0,2,0],
            "shape":{"kind":"box","half_extents":[1,1,1]},"is_trigger":trigger}),
            0,
        )
        .unwrap();
        assert!(p.character_foot_config(id).is_none());
    }
}

#[test]
fn tilted_cylinder_uses_its_round_support_extent_instead_of_box_corners() {
    let mut p = PhysicsRuntime::empty_for_tests();
    let angle = std::f32::consts::FRAC_PI_4;
    let q = (angle * 0.5).sin() / 2.0_f32.sqrt();
    p.upsert_body_from_script(&json!({"entity":1,"body_kind":"dynamic","position":[0,2,0],
        "rotation":[q,0,q,(angle*0.5).cos()],"shape":{"kind":"cylinder","radius":0.3,"half_height":0.6}}),0).unwrap();
    let cfg = p.character_foot_config(1).unwrap();
    let expected = 0.6 * angle.cos() + 0.3 * angle.sin();
    assert!((cfg.radius + cfg.half_height - expected).abs() < 1e-6);
}
