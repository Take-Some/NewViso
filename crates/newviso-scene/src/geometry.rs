use super::{transform_point, Aabb, Cube, Vec3};

const CORNERS: [Vec3; 8] = [
    Vec3::new(-0.5, -0.5, -0.5),
    Vec3::new(0.5, -0.5, -0.5),
    Vec3::new(0.5, 0.5, -0.5),
    Vec3::new(-0.5, 0.5, -0.5),
    Vec3::new(-0.5, -0.5, 0.5),
    Vec3::new(0.5, -0.5, 0.5),
    Vec3::new(0.5, 0.5, 0.5),
    Vec3::new(-0.5, 0.5, 0.5),
];

const SPHERE_LAT_SEGMENTS: u32 = 6;
const SPHERE_LON_SEGMENTS: u32 = 10;
pub(super) const SPHERE_VERTEX_COUNT: u32 = SPHERE_LAT_SEGMENTS * SPHERE_LON_SEGMENTS * 6;

impl Cube {
    fn world_corners(&self) -> [Vec3; 8] {
        CORNERS.map(|p| transform_point(p, self.scale, self.rotation_degrees, self.position))
    }

    pub(super) fn bounds(&self) -> Aabb {
        let mut bounds = Aabb {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        };
        for p in self.world_corners() {
            for (axis, value) in [p.x, p.y, p.z].iter().enumerate() {
                bounds.min[axis] = bounds.min[axis].min(*value);
                bounds.max[axis] = bounds.max[axis].max(*value);
            }
        }
        bounds
    }

    pub(super) fn append_vertices(&self, out: &mut Vec<f32>) {
        let corners = self.world_corners();
        let faces: [([usize; 6], Vec3); 6] = [
            ([4, 5, 6, 4, 6, 7], Vec3::new(0.0, 0.0, 1.0)),
            ([1, 0, 3, 1, 3, 2], Vec3::new(0.0, 0.0, -1.0)),
            ([0, 4, 7, 0, 7, 3], Vec3::new(-1.0, 0.0, 0.0)),
            ([5, 1, 2, 5, 2, 6], Vec3::new(1.0, 0.0, 0.0)),
            ([3, 7, 6, 3, 6, 2], Vec3::new(0.0, 1.0, 0.0)),
            ([0, 1, 5, 0, 5, 4], Vec3::new(0.0, -1.0, 0.0)),
        ];

        for (indices, local_normal) in faces {
            let normal =
                transform_point(local_normal, Vec3::ONE, self.rotation_degrees, Vec3::ZERO)
                    .normalized();

            for index in indices {
                append_world_vertex(out, corners[index], normal, self.base_color);
            }
        }
    }
}

pub(super) fn append_sphere_vertices(
    center: Vec3,
    radius: f32,
    rotation_degrees: Vec3,
    color: [f32; 4],
    marker_color: Option<[f32; 4]>,
    marker_direction: Vec3,
    marker_threshold: f32,
    out: &mut Vec<f32>,
) {
    let tau = std::f32::consts::TAU;
    let pi = std::f32::consts::PI;

    for lat in 0..SPHERE_LAT_SEGMENTS {
        let v0 = lat as f32 / SPHERE_LAT_SEGMENTS as f32;
        let v1 = (lat + 1) as f32 / SPHERE_LAT_SEGMENTS as f32;
        let phi0 = -pi * 0.5 + v0 * pi;
        let phi1 = -pi * 0.5 + v1 * pi;

        for lon in 0..SPHERE_LON_SEGMENTS {
            let u0 = lon as f32 / SPHERE_LON_SEGMENTS as f32;
            let u1 = (lon + 1) as f32 / SPHERE_LON_SEGMENTS as f32;
            let theta0 = u0 * tau;
            let theta1 = u1 * tau;

            let n00 = sphere_normal(theta0, phi0);
            let n10 = sphere_normal(theta1, phi0);
            let n01 = sphere_normal(theta0, phi1);
            let n11 = sphere_normal(theta1, phi1);

            for local_normal in [n00, n10, n11, n00, n11, n01] {
                let normal = transform_point(local_normal, Vec3::ONE, rotation_degrees, Vec3::ZERO)
                    .normalized();
                let world = Vec3::new(
                    center.x + normal.x * radius,
                    center.y + normal.y * radius,
                    center.z + normal.z * radius,
                );

                // The project defines the local-space marker mask. The renderer
                // only evaluates a generic directional threshold.
                let marker_direction = marker_direction.normalized();
                let vertex_color = marker_color
                    .filter(|_| local_normal.dot(marker_direction) >= marker_threshold)
                    .unwrap_or(color);
                append_world_vertex(out, world, normal, vertex_color);
            }
        }
    }
}

pub(super) fn append_overlay_vertex(out: &mut Vec<f32>, clip_x: f32, clip_y: f32, color: [f32; 4]) {
    // position.w = 1 marks a pre-projected overlay vertex. World geometry uses 0.
    out.extend_from_slice(&[clip_x, clip_y, 0.0001, 1.0]);
    out.extend_from_slice(&[0.0, 0.0, 1.0]);
    out.extend_from_slice(&color);
}

fn append_world_vertex(out: &mut Vec<f32>, position: Vec3, normal: Vec3, color: [f32; 4]) {
    out.extend_from_slice(&[position.x, position.y, position.z, 0.0]);
    out.extend_from_slice(&[normal.x, normal.y, normal.z]);
    out.extend_from_slice(&color);
}

fn sphere_normal(theta: f32, phi: f32) -> Vec3 {
    let cos_phi = phi.cos();
    Vec3::new(cos_phi * theta.cos(), phi.sin(), cos_phi * theta.sin())
}
