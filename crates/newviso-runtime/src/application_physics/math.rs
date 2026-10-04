use super::*;

pub(super) fn rotate_camera_vector(v: [f32; 3], q: [f32; 4]) -> [f32; 3] {
    let norm = q.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < 1.0e-8 {
        return v;
    }
    let [x, y, z, w] = [q[0] / norm, q[1] / norm, q[2] / norm, q[3] / norm];
    let t = [
        2.0 * (y * v[2] - z * v[1]),
        2.0 * (z * v[0] - x * v[2]),
        2.0 * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + y * t[2] - z * t[1],
        v[1] + w * t[1] + z * t[0] - x * t[2],
        v[2] + w * t[2] + x * t[1] - y * t[0],
    ]
}

pub(super) fn inverse_rotate_camera_vector(v: [f32; 3], q: [f32; 4]) -> [f32; 3] {
    let norm = q.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < 1.0e-8 {
        return v;
    }
    let [x, y, z, w] = [-q[0] / norm, -q[1] / norm, -q[2] / norm, q[3] / norm];
    let t = [
        2.0 * (y * v[2] - z * v[1]),
        2.0 * (z * v[0] - x * v[2]),
        2.0 * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + y * t[2] - z * t[1],
        v[1] + w * t[1] + z * t[0] - x * t[2],
        v[2] + w * t[2] + x * t[1] - y * t[0],
    ]
}

pub(super) fn quaternion_multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    let q = [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ];
    let norm = q.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm <= 1.0e-8 || !norm.is_finite() {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        q.map(|value| value / norm)
    }
}

pub(super) fn normalize_vec3(value: [f32; 3]) -> [f32; 3] {
    let length = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if !length.is_finite() || length <= 1.0e-6 {
        return [0.0, 1.0, 0.0];
    }
    std::array::from_fn(|axis| value[axis] / length)
}

pub(super) fn euler_degrees_to_quaternion(rotation_degrees: [f32; 3]) -> [f32; 4] {
    let [rx, ry, rz] = rotation_degrees.map(|value| value.to_radians() * 0.5);
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    let quaternion = [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ];
    let norm = quaternion
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if norm <= 1.0e-8 || !norm.is_finite() {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        quaternion.map(|value| value / norm)
    }
}

pub(super) fn transform_collision_vertex(
    vertex: [f32; 3],
    scale: [f32; 3],
    rotation_degrees: [f32; 3],
) -> [f32; 3] {
    let mut x = vertex[0] * scale[0];
    let mut y = vertex[1] * scale[1];
    let mut z = vertex[2] * scale[2];

    let rx = rotation_degrees[0].to_radians();
    let (sin_x, cos_x) = rx.sin_cos();
    let next_y = y * cos_x - z * sin_x;
    let next_z = y * sin_x + z * cos_x;
    y = next_y;
    z = next_z;

    let ry = rotation_degrees[1].to_radians();
    let (sin_y, cos_y) = ry.sin_cos();
    let next_x = x * cos_y + z * sin_y;
    let next_z = -x * sin_y + z * cos_y;
    x = next_x;
    z = next_z;

    let rz = rotation_degrees[2].to_radians();
    let (sin_z, cos_z) = rz.sin_cos();
    let next_x = x * cos_z - y * sin_z;
    let next_y = x * sin_z + y * cos_z;
    [next_x, next_y, z]
}

pub(super) fn collider_bounds(collider: &MeshCollider) -> Result<([f32; 3], [f32; 3]), String> {
    let first = *collider
        .vertices
        .first()
        .ok_or_else(|| "physics mesh collider is empty".to_owned())?;
    let mut min = first;
    let mut max = first;
    for vertex in collider.vertices.iter().skip(1) {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    Ok((min, max))
}

pub(super) fn static_scene_bodies(
    solids: &[(u64, [f32; 3], [f32; 3])],
    settings: PhysicsWorldSettings,
) -> Vec<PhysicsBodySnapshot> {
    solids
        .iter()
        .map(|(entity, min, max)| {
            let center = [
                (min[0] + max[0]) * 0.5,
                (min[1] + max[1]) * 0.5,
                (min[2] + max[2]) * 0.5,
            ];
            let half_extents = [
                ((max[0] - min[0]) * 0.5).max(0.001),
                ((max[1] - min[1]) * 0.5).max(0.001),
                ((max[2] - min[2]) * 0.5).max(0.001),
            ];
            PhysicsBodySnapshot {
                entity: *entity,
                kind: PhysicsBodyKind::Static,
                shape: CollisionShape::Box { half_extents },
                flags: PhysicsBodyFlags {
                    is_trigger: false,
                    participates_in_queries: settings.scene_participates_in_queries,
                    casts_contacts: settings.scene_casts_contacts,
                    continuous_collision: false,
                },
                material: settings.scene_material,
                position: center,
                rotation: [0.0, 0.0, 0.0, 1.0],
                linear_velocity: [0.0; 3],
                angular_velocity: [0.0; 3],
                linear_damping: Some(0.0),
                angular_damping: Some(0.0),
                mass_properties: None,
                convex_hulls: Vec::new(),
                bounds_min: *min,
                bounds_max: *max,
            }
        })
        .collect()
}

pub(super) fn shape_bounds(shape: CollisionShape, position: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let half = match shape {
        CollisionShape::Box { half_extents } => half_extents,
        CollisionShape::Sphere { radius } => [radius; 3],
        CollisionShape::Capsule {
            radius,
            half_height,
        }
        | CollisionShape::Cylinder {
            radius,
            half_height,
        } => [radius, radius + half_height, radius],
    };
    (
        [
            position[0] - half[0],
            position[1] - half[1],
            position[2] - half[2],
        ],
        [
            position[0] + half[0],
            position[1] + half[1],
            position[2] + half[2],
        ],
    )
}

pub(super) fn refresh_bounds(body: &mut PhysicsBodySnapshot) {
    if body.convex_hulls.is_empty() {
        let (min, max) = shape_bounds(body.shape, body.position);
        body.bounds_min = min;
        body.bounds_max = max;
    } else {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for point in body.convex_hulls.iter().flatten() {
            let rotated = rotate_camera_vector(*point, body.rotation);
            for axis in 0..3 {
                let world = rotated[axis] + body.position[axis];
                min[axis] = min[axis].min(world);
                max[axis] = max[axis].max(world);
            }
        }
        body.bounds_min = min;
        body.bounds_max = max;
    }
}
