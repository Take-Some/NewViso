use super::*;

pub(super) fn append_flare_quad(
    out: &mut Vec<f32>,
    center: [f32; 2],
    half_size: [f32; 2],
    color: [f32; 3],
    alpha: f32,
    kind_code: f32,
) {
    let corners = [
        (
            [-1.0, -1.0],
            [center[0] - half_size[0], center[1] - half_size[1]],
        ),
        (
            [1.0, -1.0],
            [center[0] + half_size[0], center[1] - half_size[1]],
        ),
        (
            [1.0, 1.0],
            [center[0] + half_size[0], center[1] + half_size[1]],
        ),
        (
            [-1.0, 1.0],
            [center[0] - half_size[0], center[1] + half_size[1]],
        ),
    ];
    for index in [0usize, 1, 2, 0, 2, 3] {
        let (local, clip) = corners[index];
        out.extend_from_slice(&[
            clip[0], clip[1], local[0], local[1], color[0], color[1], color[2], alpha, kind_code,
            0.0, 0.0, 0.0,
        ]);
    }
}
pub(super) fn ray_hits_aabb(
    origin: Vec3,
    direction: Vec3,
    bounds: SceneBounds,
    min_t: f32,
) -> bool {
    let mut t_min = f32::NEG_INFINITY;
    let mut t_max = f32::INFINITY;

    for axis in 0..3 {
        let (origin_axis, dir_axis, min_axis, max_axis) = match axis {
            0 => (origin.x, direction.x, bounds.min.x, bounds.max.x),
            1 => (origin.y, direction.y, bounds.min.y, bounds.max.y),
            _ => (origin.z, direction.z, bounds.min.z, bounds.max.z),
        };

        if dir_axis.abs() < 1.0e-6 {
            if origin_axis < min_axis || origin_axis > max_axis {
                return false;
            }
            continue;
        }

        let inv = 1.0 / dir_axis;
        let mut a = (min_axis - origin_axis) * inv;
        let mut b = (max_axis - origin_axis) * inv;
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        t_min = t_min.max(a);
        t_max = t_max.min(b);
        if t_max < t_min {
            return false;
        }
    }

    t_max >= min_t && t_max >= 0.0
}
pub(super) fn sky_visual_direction(rotation_degrees: Vec3) -> Vec3 {
    transform_point(
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::ONE,
        rotation_degrees,
        Vec3::ZERO,
    )
    .normalized()
}
pub(super) fn identity_matrix() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
pub(super) fn matrix_from_rows(rows: [[f32; 4]; 4]) -> [f32; 16] {
    [
        rows[0][0], rows[1][0], rows[2][0], rows[3][0], rows[0][1], rows[1][1], rows[2][1],
        rows[3][1], rows[0][2], rows[1][2], rows[2][2], rows[3][2], rows[0][3], rows[1][3],
        rows[2][3], rows[3][3],
    ]
}
pub(super) fn view_basis(forward: Vec3, up_hint: Vec3) -> (Vec3, Vec3, Vec3) {
    let forward = forward.normalized();
    let fallback_up = if forward.dot(Vec3::Y).abs() > 0.98 {
        Vec3::new(0.0, 0.0, 1.0)
    } else {
        up_hint
    };
    let right = forward.cross(fallback_up).normalized();
    let up = right.cross(forward).normalized();
    (right, up, forward)
}
pub(super) fn camera_view_projection(camera: &Camera, aspect: f32) -> [f32; 16] {
    perspective_view_projection(
        camera.position,
        camera.target.sub(camera.position).normalized(),
        camera.up,
        camera.fov_y_degrees,
        aspect,
        camera.near.max(0.001),
        camera.far.max(camera.near + 0.001),
    )
}
pub(super) fn perspective_view_projection(
    eye: Vec3,
    forward: Vec3,
    up_hint: Vec3,
    fov_y_degrees: f32,
    aspect: f32,
    near: f32,
    far: f32,
) -> [f32; 16] {
    let (right, up, forward) = view_basis(forward, up_hint);
    let inv_tan = 1.0 / (fov_y_degrees.to_radians() * 0.5).tan().max(0.0001);
    let x_scale = inv_tan / aspect.max(0.0001);
    let y_scale = inv_tan;
    let depth_scale = far / (far - near);
    let depth_bias = -far * near / (far - near);

    matrix_from_rows([
        [
            right.x * x_scale,
            right.y * x_scale,
            right.z * x_scale,
            -right.dot(eye) * x_scale,
        ],
        [
            -up.x * y_scale,
            -up.y * y_scale,
            -up.z * y_scale,
            up.dot(eye) * y_scale,
        ],
        [
            forward.x * depth_scale,
            forward.y * depth_scale,
            forward.z * depth_scale,
            depth_bias - forward.dot(eye) * depth_scale,
        ],
        [forward.x, forward.y, forward.z, -forward.dot(eye)],
    ])
}
pub(super) fn orthographic_view_projection(
    eye: Vec3,
    forward: Vec3,
    up_hint: Vec3,
    half_extent: f32,
    near: f32,
    far: f32,
) -> [f32; 16] {
    let (right, up, forward) = view_basis(forward, up_hint);
    let extent = half_extent.max(0.001);
    let depth_range = (far - near).max(0.001);
    let depth_scale = 1.0 / depth_range;

    matrix_from_rows([
        [
            right.x / extent,
            right.y / extent,
            right.z / extent,
            -right.dot(eye) / extent,
        ],
        [
            -up.x / extent,
            -up.y / extent,
            -up.z / extent,
            up.dot(eye) / extent,
        ],
        [
            forward.x * depth_scale,
            forward.y * depth_scale,
            forward.z * depth_scale,
            (-forward.dot(eye) - near) * depth_scale,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ])
}
pub(super) fn light_direction(rotation_degrees: Vec3) -> Vec3 {
    transform_point(
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::ONE,
        rotation_degrees,
        Vec3::ZERO,
    )
    .normalized()
}
/// Exponential, framerate-independent shadow-camera tracking. Movement
/// cannot move the shadow projection an entire world texel in one simulation
/// update unless the player actually teleports. Jump discontinuities reset the
/// filter, preventing delayed shadows after scene cuts or fast travel.
pub(super) fn track_shadow_focus(previous: Option<Vec3>, camera: Vec3, dt: f32) -> Vec3 {
    let Some(old) = previous else { return camera };
    if !dt.is_finite() || dt <= 0.0 { return old; }
    let movement = camera.sub(old);
    if movement.dot(movement) > 24.0 * 24.0 { return camera; }
    let factor = 1.0 - (-dt.min(0.25) / 0.12).exp();
    old.add(movement.mul(factor))
}

/// Continuous C1 light-space texel interpolation. No full-texel jumps
/// on cell crossings, and zero derivative on the integer boundaries.
pub(super) fn continuous_shadow_texel(value: f32, texel_size: f32) -> f32 {
    if !texel_size.is_finite() || texel_size <= 0.0 { return value; }
    let scaled = value / texel_size;
    let cell = scaled.floor();
    let phase = scaled - cell;
    let eased = phase * phase * (3.0 - 2.0 * phase);
    (cell + eased) * texel_size
}

pub(super) fn directional_shadow_view_projection(
    direction: Vec3,
    focus_position: Vec3,
    shadow_distance: f32,
    shadow_resolution: u32,
) -> [f32; 16] {
    let direction = direction.normalized();
    let distance = shadow_distance.max(8.0);
    let half_extent = distance * 0.55;
    let (right, up, forward) = view_basis(direction, Vec3::Y);

    // Light space follows continuously. Hard rounding caused a whole-texel
    // jump at camera grid boundaries; subtexel easing preserves continuity.
    let world_units_per_texel = (half_extent * 2.0) / shadow_resolution.max(1) as f32;
    let light_x = focus_position.dot(right);
    let light_y = focus_position.dot(up);
    let light_z = focus_position.dot(forward);
    let snapped_x = continuous_shadow_texel(light_x, world_units_per_texel);
    let snapped_y = continuous_shadow_texel(light_y, world_units_per_texel);
    let snapped_center = right
        .mul(snapped_x)
        .add(up.mul(snapped_y))
        .add(forward.mul(light_z));

    let eye = snapped_center.sub(direction.mul(distance));
    orthographic_view_projection(eye, direction, Vec3::Y, half_extent, 0.1, distance * 2.0)
}
pub(super) fn spot_shadow_view_projection(
    position: Vec3,
    direction: Vec3,
    outer_cone_degrees: f32,
    range: f32,
) -> [f32; 16] {
    perspective_view_projection(
        position,
        direction,
        Vec3::Y,
        (outer_cone_degrees * 2.0).clamp(1.0, 175.0),
        1.0,
        0.05,
        range.max(0.1),
    )
}
