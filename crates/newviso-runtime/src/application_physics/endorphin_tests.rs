use super::*;

#[test]
fn foot_probe_tracks_contact_velocity_on_a_rotating_support_at_any_probe_radius() {
    let mut physics = PhysicsRuntime::empty_for_tests();
    physics
        .upsert_body_from_script(
            &json!({
                "entity":42, "position":[0,-0.25,0],
                "shape":{"kind":"box","half_extents":[2,0.25,2]}
            }),
            0,
        )
        .unwrap();
    let platform = physics.bodies.get_mut(&42).unwrap();
    platform.linear_velocity = [3.0, 0.0, 1.0];
    platform.angular_velocity = [0.0, 2.0, 1.0];
    for radius in [0.03, 0.12] {
        for x in [-1.0, 1.0] {
            let ground = physics
                .endorphin_foot_ground([x, 0.4, 0.0], radius, 1.0, &[], &[], 0.65)
                .unwrap();
            let expected = [2.75, x, 1.0 - 2.0 * x];
            for (actual, expected) in ground.velocity.into_iter().zip(expected) {
                assert!(
                    (actual - expected).abs() < 1e-5,
                    "surface velocity {actual} vs {expected}"
                );
            }
            assert!((ground.position[1] - radius).abs() < 1e-5);
            assert!(physics
                .endorphin_foot_ground([x, 0.4, 0.0], radius, 1.0, &[42], &[], 0.65)
                .is_none());
        }
    }
}

#[test]
fn a_sampled_endorphin_surface_tracks_the_resident_body_and_rejects_retirement() {
    let mut physics = PhysicsRuntime::empty_for_tests();
    physics
        .upsert_body_from_script(
            &json!({
                "entity":42,"body_kind":"kinematic","position":[0,-0.25,0],
                "shape":{"kind":"box","half_extents":[2,0.25,2]}
            }),
            0,
        )
        .unwrap();
    let mut ground = physics
        .endorphin_foot_ground([1.0, 0.4, 0.0], 0.03, 1.0, &[], &[], 0.65)
        .unwrap();
    assert_eq!(ground.support.as_ref().unwrap().entity, 42);
    let platform = physics.bodies.get_mut(&42).unwrap();
    platform.position = [2.0, 0.75, -1.0];
    platform.rotation =
        newviso_endorphin::math::axis_angle([0.0, std::f32::consts::FRAC_PI_2, 0.0]);
    platform.linear_velocity = [3.0, 0.0, 0.0];
    let sample = |id| {
        physics
            .vehicle_body_state(id)
            .map(|p| newviso_endorphin::BodyPose {
                position: p.position,
                rotation: p.rotation,
                linear_velocity: p.linear_velocity,
                angular_velocity: p.angular_velocity,
            })
    };
    assert!(ground.refresh(1.0 / 120.0, sample));
    assert!(
        newviso_endorphin::math::length(newviso_endorphin::math::sub(
            ground.position,
            [2.0, 1.03, -2.0]
        )) < 1e-5
    );
    physics.bodies.get_mut(&42).unwrap().linear_velocity = [0.0; 3];
    for _ in 0..100 {
        assert!(
            ground.refresh(1.0 / 120.0, |id| physics.vehicle_body_state(id).map(|p| {
                newviso_endorphin::BodyPose {
                    position: p.position,
                    rotation: p.rotation,
                    linear_velocity: p.linear_velocity,
                    angular_velocity: p.angular_velocity,
                }
            }))
        );
    }
    assert!(
        newviso_endorphin::math::length(newviso_endorphin::math::sub(
            ground.position,
            [2.0, 1.03, -2.0]
        )) < 1e-5
    );
    physics.bodies.remove(&42);
    assert!(!ground.refresh(1.0 / 120.0, |_| None));
}
