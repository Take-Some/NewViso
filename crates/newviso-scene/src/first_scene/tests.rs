use super::*;

#[test]
fn camera_view_projection_maps_origin_in_front_of_camera() {
    let camera = Camera {
        position: Vec3::new(0.0, 0.0, 5.0),
        target: Vec3::ZERO,
        up: Vec3::Y,
        fov_y_degrees: 60.0,
        near: 0.1,
        far: 100.0,
    };

    let matrix = camera_view_projection(&camera, 16.0 / 9.0);
    assert!(matrix.iter().all(|value| value.is_finite()));

    let clip = mul_mat4_vec4(matrix, [0.0, 0.0, 0.0, 1.0]);
    assert!(clip[0].abs() < 0.001);
    assert!(clip[1].abs() < 0.001);
    assert!(clip[3] > 0.0);
    assert!(clip[2] > 0.0 && clip[2] < clip[3]);

    let behind = mul_mat4_vec4(matrix, [0.0, 0.0, 6.0, 1.0]);
    assert!(behind[3] < 0.0);
}

fn mul_mat4_vec4(matrix: [f32; 16], vector: [f32; 4]) -> [f32; 4] {
    [
        matrix[0] * vector[0]
            + matrix[4] * vector[1]
            + matrix[8] * vector[2]
            + matrix[12] * vector[3],
        matrix[1] * vector[0]
            + matrix[5] * vector[1]
            + matrix[9] * vector[2]
            + matrix[13] * vector[3],
        matrix[2] * vector[0]
            + matrix[6] * vector[1]
            + matrix[10] * vector[2]
            + matrix[14] * vector[3],
        matrix[3] * vector[0]
            + matrix[7] * vector[1]
            + matrix[11] * vector[2]
            + matrix[15] * vector[3],
    ]
}

#[test]
fn reverse_z_perspective_is_monotonic_and_reconstructs_metre_distances() {
    let near=0.1f32;
    let far=20_000.0f32;
    let matrix=perspective_view_projection_reverse_z(
        Vec3::ZERO,Vec3::new(0.0,0.0,1.0),Vec3::Y,
        60.0,16.0/9.0,near,far,
    );
    let mut previous=2.0f32;
    for z in [near,0.11,1.0,100.0,1_000.0,5_000.0,19_000.0,far] {
        let clip=mul_mat4_vec4(matrix,[0.0,0.0,z,1.0]);
        assert!(clip[3]>0.0,"Z must stay forward");
        let device_depth=clip[2]/clip[3];
        assert!(device_depth<=previous+1.0e-5,"reversed depth must decrease with distance");
        assert!((-1.0e-5..=1.00001).contains(&device_depth));
        let metric=reverse_z_view_depth(device_depth,near,far);
        assert!((metric-z).abs()<z*0.0001+0.001,
            "reconstruction failure z={z}, got={metric} depth={device_depth}");
        previous=device_depth;
    }
    assert!((reverse_z_view_depth(1.0,near,far)-near).abs()<1.0e-4);
    assert!((reverse_z_view_depth(0.0,near,far)-far).abs()<0.01);
    // Conventional shadow projection remains unchanged.
    let shadow=perspective_view_projection(Vec3::ZERO,
        Vec3::new(0.0,0.0,1.0),Vec3::Y,60.0,1.0,near,far);
    let near_clip=mul_mat4_vec4(shadow,[0.0,0.0,near,1.0]);
    assert!(near_clip[2]/near_clip[3]<=1.0e-5);
}

#[test]
fn shadow_focus_follows_continuously_and_handles_teleports() {
    let camera = Vec3::new(10.0, 2.0, 0.0);
    let first = track_shadow_focus(None, Vec3::ZERO, 1.0 / 60.0);
    let a = track_shadow_focus(Some(first), camera, 1.0 / 60.0);
    let b = track_shadow_focus(Some(a), camera, 1.0 / 60.0);
    assert!(a.x > 0.0 && a.x < 10.0);
    assert!(b.x > a.x && b.x < 10.0);
    // Frame-rate-independent integration for constant target.
    let one_frame = track_shadow_focus(Some(Vec3::ZERO), camera, 1.0 / 30.0);
    let two_frames = track_shadow_focus(
        Some(track_shadow_focus(Some(Vec3::ZERO), camera, 1.0 / 60.0)),
        camera,
        1.0 / 60.0,
    );
    assert!((one_frame.x - two_frames.x).abs() < 1.0e-4);
    // Do not smear shadows across a cut or teleport.
    let jump = Vec3::new(100.0, 0.0, 0.0);
    let reset = track_shadow_focus(Some(a), jump, 1.0 / 60.0);
    assert!((reset.x - 100.0).abs() < 1.0e-5);
    assert_eq!(track_shadow_focus(Some(a), camera, 0.0).x, a.x);
}

#[test]
fn directional_shadow_projection_interpolates_across_texel_boundaries() {
    let direction = Vec3::new(-0.35, -1.0, -0.2).normalized();
    let shadow_distance = 96.0;
    let shadow_resolution = 4096;
    let texel = shadow_distance * 1.1 / shadow_resolution as f32;
    let (right, _, _) = view_basis(direction, Vec3::Y);
    let origin = directional_shadow_view_projection(
        direction,
        Vec3::ZERO,
        shadow_distance,
        shadow_resolution,
    );
    let quarter = directional_shadow_view_projection(
        direction,
        right.mul(texel * 0.25),
        shadow_distance,
        shadow_resolution,
    );
    let projection_delta = (quarter[12] - origin[12]).abs();
    assert!(projection_delta > 1.0e-8);
    assert!(projection_delta < 2.0 / shadow_resolution as f32);
    let before = continuous_shadow_texel(texel * (1.0 - 1.0e-3), texel);
    let at = continuous_shadow_texel(texel, texel);
    let after = continuous_shadow_texel(texel * (1.0 + 1.0e-3), texel);
    assert!(before <= at && at <= after);
    assert!((before - at).abs() < texel * 0.0001);
    assert!((after - at).abs() < texel * 0.0001);
}

#[test]
fn orbit_camera_rotates_only_while_dragging() {
    let camera = Camera {
        position: Vec3::new(0.0, 0.0, 5.0),
        target: Vec3::ZERO,
        up: Vec3::Y,
        fov_y_degrees: 60.0,
        near: 0.1,
        far: 100.0,
    };
    let mut orbit = OrbitCamera::from_camera(&camera);
    orbit.rotate_sensitivity = 0.005;
    orbit.zoom_sensitivity = 0.0015;
    orbit.min_distance = 2.0;
    orbit.max_distance = 40.0;
    orbit.min_pitch_degrees = -85.0;
    orbit.max_pitch_degrees = 85.0;
    orbit.configured = true;
    let yaw = orbit.yaw;
    orbit.apply_mouse(100.0, 0.0, 0.0, false);
    assert!((orbit.yaw - yaw).abs() < f32::EPSILON);
    orbit.apply_mouse(100.0, 0.0, 0.0, true);
    assert!((orbit.yaw - yaw).abs() > 0.1);
}

#[test]
fn orbit_camera_wheel_changes_distance_with_clamp() {
    let camera = Camera {
        position: Vec3::new(0.0, 0.0, 5.0),
        target: Vec3::ZERO,
        up: Vec3::Y,
        fov_y_degrees: 60.0,
        near: 0.1,
        far: 100.0,
    };
    let mut orbit = OrbitCamera::from_camera(&camera);
    orbit.rotate_sensitivity = 0.005;
    orbit.zoom_sensitivity = 0.0015;
    orbit.min_distance = 2.0;
    orbit.max_distance = 40.0;
    orbit.min_pitch_degrees = -85.0;
    orbit.max_pitch_degrees = 85.0;
    orbit.configured = true;
    let distance = orbit.distance;
    orbit.apply_mouse(0.0, 0.0, 120.0, false);
    assert!(orbit.distance < distance);
    orbit.apply_mouse(0.0, 0.0, 100000.0, false);
    assert_eq!(orbit.distance, orbit.min_distance);
}

#[test]
fn instance_matrix_matches_cpu_transform_order() {
    let position = Vec3::new(7.0, -2.0, 11.0);
    let rotation = Vec3::new(21.0, -37.0, 13.0);
    let scale = Vec3::new(1.5, 0.75, -2.0);
    let point = Vec3::new(3.0, -4.0, 2.0);
    let matrix = geometry::instance_model_matrix(position, rotation, scale);
    let gpu = mul_mat4_vec4(matrix, [point.x, point.y, point.z, 1.0]);
    let cpu = transform_point(point, scale, rotation, position);
    assert!((gpu[0] - cpu.x).abs() < 1.0e-5);
    assert!((gpu[1] - cpu.y).abs() < 1.0e-5);
    assert!((gpu[2] - cpu.z).abs() < 1.0e-5);
    assert!((gpu[3] - 1.0).abs() < 1.0e-5);
}

#[test]
fn sky_visual_direction_is_opposite_directional_light_ray_for_same_rotation() {
    let rotation = Vec3::new(-35.0, 115.0, 0.0);
    let light_ray = light_direction(rotation);
    let sky_direction = sky_visual_direction(rotation);
    assert!((light_ray.dot(sky_direction) + 1.0).abs() < 1.0e-5);
    assert!(sky_direction.y > 0.0);
}

#[test]
fn flare_quad_uses_six_vertices_with_procedural_payload() {
    let mut out = Vec::new();
    append_flare_quad(
        &mut out,
        [0.25, -0.1],
        [0.05, 0.08],
        [1.0, 0.8, 0.4],
        0.5,
        2.0,
    );
    assert_eq!(out.len(), 6 * FLARE_FLOATS_PER_VERTEX);
    assert_eq!(out[4..8], [1.0, 0.8, 0.4, 0.5]);
    assert_eq!(out[8], 2.0);
}

#[test]
fn scene_ray_occlusion_hits_forward_aabb_and_rejects_side_aabb() {
    let origin = Vec3::ZERO;
    let direction = Vec3::new(0.0, 0.0, -1.0);
    let forward =
        SceneBounds::from_center_half_extent(Vec3::new(0.0, 0.0, -5.0), Vec3::new(1.0, 1.0, 1.0));
    let side =
        SceneBounds::from_center_half_extent(Vec3::new(5.0, 0.0, -5.0), Vec3::new(1.0, 1.0, 1.0));
    assert!(ray_hits_aabb(origin, direction, forward, 0.05));
    assert!(!ray_hits_aabb(origin, direction, side, 0.05));
}

#[test]
fn cube_output_matches_world_vertex_layout_and_collision_bounds() {
    let cube = Cube {
        position: Vec3::new(2.0, 1.0, -3.0),
        rotation_degrees: Vec3::ZERO,
        scale: Vec3::new(4.0, 2.0, 6.0),
        base_color: [1.0; 4],
    };
    let mut out = Vec::new();
    cube.append_vertices(&mut out);
    assert_eq!(out.len(), CUBE_VERTEX_COUNT as usize * FLOATS_PER_VERTEX);
    assert_eq!(VERTEX_STRIDE, 76);
    assert_eq!(cube.bounds().min, [0.0, 0.0, -6.0]);
    assert_eq!(cube.bounds().max, [4.0, 2.0, 0.0]);
    assert_eq!(out[3], 0.0);
    assert_eq!(&out[4..7], &[0.0, 0.0, 1.0]);
    assert_eq!(&out[7..11], &[1.0, 1.0, 1.0, 1.0]);
    assert_eq!(&out[11..13], &[0.0, 0.0]);
    assert_eq!(&out[13..17], &[1.0, 0.0, 0.0, 1.0]);
}
