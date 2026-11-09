use super::*;
fn frame() -> BodyPose {
    BodyPose {
        position: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        linear_velocity: [0.0; 3],
        angular_velocity: [0.0; 3],
    }
}
fn placement(scale: f32) -> RecoveryPlacement {
    RecoveryPlacement {
        capsule_center: [scale, 0.902 * scale, -0.4 * scale],
        model_origin: [1.1 * scale, 1.1 * scale, -0.6 * scale],
        root_world: [0.0; 3],
        ground: 0.0,
        heading_radians: 0.3,
        radial_sample: 0,
    }
}
fn near(a: [f32; 3], b: [f32; 3], scale: f32) {
    assert!(
        math::length(math::sub(a, b)) < 0.00001 * scale,
        "{a:?} != {b:?}"
    );
}
#[test]
fn translated_rotated_support_carries_the_ground_point_and_heading_at_all_body_scales() {
    for scale in [0.1, 1.0, 3.0] {
        let p = placement(scale);
        let anchor = RecoverySupport::bind(42, frame(), p, [0.0, 1.0, 0.0]).unwrap();
        let mut f = frame();
        f.position = [3.0 * scale, 2.0 * scale, -2.0 * scale];
        f.rotation = math::axis_angle([0.0, 0.7, 0.0]);
        let s = anchor.sample(Some(f), 0.65).unwrap();
        let ground = math::add(
            f.position,
            math::rotate(f.rotation, [scale, 0.0, -0.4 * scale]),
        );
        near(s.ground_point, ground, scale);
        near(
            s.capsule_center,
            [ground[0], ground[1] + 0.902 * scale, ground[2]],
            scale,
        );
        near(
            s.model_origin,
            math::add(ground, [0.1 * scale, 1.1 * scale, -0.2 * scale]),
            scale,
        );
        assert!((s.heading_radians - 1.0).abs() < 0.00001);
    }
}
#[test]
fn ground_contact_rather_than_capsule_com_is_transported_on_a_tilting_surface() {
    let a = RecoverySupport::bind(7, frame(), placement(1.0), [0.0, 1.0, 0.0]).unwrap();
    let mut f = frame();
    f.rotation = math::axis_angle([0.2, 0.3, -0.1]);
    let s = a.sample(Some(f), 0.65).unwrap();
    near(s.normal, math::rotate(f.rotation, [0.0, 1.0, 0.0]), 1.0);
    near(
        math::sub(s.capsule_center, s.ground_point),
        [0.0, 0.902, 0.0],
        1.0,
    );
}
#[test]
fn velocity_is_measured_at_the_local_contact_including_angular_motion() {
    let a = RecoverySupport::bind(7, frame(), placement(1.0), [0.0, 1.0, 0.0]).unwrap();
    let mut f = frame();
    f.position = [1.0, 2.0, 3.0];
    f.linear_velocity = [2.0, -0.4, 0.6];
    f.angular_velocity = [0.2, 1.0, -0.3];
    let s = a.sample(Some(f), 0.65).unwrap();
    near(
        s.velocity,
        math::add(
            f.linear_velocity,
            math::cross(f.angular_velocity, math::sub(s.ground_point, f.position)),
        ),
        1.0,
    );
}
#[test]
fn acceleration_stopping_and_repeated_sampling_cannot_integrate_a_stale_velocity() {
    let a = RecoverySupport::bind(7, frame(), placement(1.0), [0.0, 1.0, 0.0]).unwrap();
    let mut f = frame();
    f.position = [2.0, 0.0, 0.0];
    f.linear_velocity = [100.0, 0.0, 0.0];
    let moving = a.sample(Some(f), 0.65).unwrap();
    f.linear_velocity = [0.0; 3];
    for _ in 0..100 {
        let stopped = a.sample(Some(f), 0.65).unwrap();
        near(stopped.ground_point, moving.ground_point, 1.0);
        assert_eq!(stopped.velocity, [0.0; 3]);
    }
}
#[test]
fn missing_invalid_or_unwalkable_support_has_no_frozen_world_fallback() {
    let a = RecoverySupport::bind(7, frame(), placement(1.0), [0.0, 1.0, 0.0]).unwrap();
    assert!(a.sample(None, 0.65).is_none());
    let mut f = frame();
    f.position[0] = f32::NAN;
    assert!(a.sample(Some(f), 0.65).is_none());
    f = frame();
    f.rotation = [0.0; 4];
    assert!(a.sample(Some(f), 0.65).is_none());
    f = frame();
    f.rotation = math::axis_angle([1.3, 0.0, 0.0]);
    assert!(a.sample(Some(f), 0.65).is_none());
    assert!(a.sample(Some(frame()), f32::NAN).is_none());
}
#[test]
fn initial_nonidentity_support_frame_and_quaternion_sign_preserve_the_admitted_pose() {
    let mut f = frame();
    f.position = [10.0, 4.0, -3.0];
    f.rotation = math::axis_angle([0.1, 0.4, -0.05]);
    let p = placement(1.0);
    let a = RecoverySupport::bind(7, f, p, [0.0, 1.0, 0.0]).unwrap();
    let s = a.sample(Some(f), 0.65).unwrap();
    near(s.capsule_center, p.capsule_center, 1.0);
    near(s.model_origin, p.model_origin, 1.0);
    f.rotation = f.rotation.map(|v| -v);
    let signed = a.sample(Some(f), 0.65).unwrap();
    near(signed.ground_point, s.ground_point, 1.0);
    assert!((signed.heading_radians - p.heading_radians).abs() < 0.00001);
}
#[test]
fn invalid_admission_never_creates_a_support_binding() {
    let mut p = placement(1.0);
    p.ground = f32::NAN;
    assert!(RecoverySupport::bind(7, frame(), p, [0.0, 1.0, 0.0]).is_none());
    p = placement(1.0);
    p.capsule_center[1] = -1.0;
    assert!(RecoverySupport::bind(7, frame(), p, [0.0, 1.0, 0.0]).is_none());
    assert!(RecoverySupport::bind(7, frame(), placement(1.0), [0.0; 3]).is_none());
}
#[test]
fn heading_wrap_does_not_introduce_a_half_turn_when_the_platform_crosses_pi() {
    let mut p = placement(1.0);
    p.heading_radians = 3.0;
    let a = RecoverySupport::bind(7, frame(), p, [0.0, 1.0, 0.0]).unwrap();
    let mut f = frame();
    f.rotation = math::axis_angle([0.0, 0.4, 0.0]);
    let s = a.sample(Some(f), 0.65).unwrap();
    near(
        [s.heading_radians.sin(), 0.0, s.heading_radians.cos()],
        [3.4_f32.sin(), 0.0, 3.4_f32.cos()],
        1.0,
    );
}
