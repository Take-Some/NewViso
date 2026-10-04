use super::*;

pub(super) fn append_particle_visual(
    particle: &SceneRuntimeParticle,
    camera_right: Vec3,
    camera_up: Vec3,
    forward: Vec3,
    previous: Option<[f32; 3]>,
    out: &mut Vec<f32>,
) {
    if let Some(physical) = &particle.physical {
        physical.append(particle.desc.position, out);
        return;
    }
    let t = (particle.age_seconds / particle.desc.lifetime_seconds).clamp(0.0, 1.0);
    let mut size = std::array::from_fn(|i| {
        particle.desc.size[i] + (particle.desc.end_size[i] - particle.desc.size[i]) * t
    });
    let mut color = std::array::from_fn(|i| {
        particle.desc.color[i] + (particle.desc.end_color[i] - particle.desc.color[i]) * t
    });
    let style = particle.desc.style.as_ref();
    if let Some(style) = style {
        color = sample_particle_curve(&style.color_keys, t, color);
        size = sample_particle_curve(&style.size_keys, t, size);
    }
    let center = Vec3::new(
        particle.desc.position[0],
        particle.desc.position[1],
        particle.desc.position[2],
    );
    let normal = forward.mul(-1.0);
    if let Some(style) = style.filter(|s| s.model.is_some()) {
        let mesh = style.model.as_ref().unwrap();
        let depth = sample_particle_curve(&style.depth_keys, t, [size[0].min(size[1])])[0];
        let dimensions = [size[0], size[1], depth];
        let rotation = std::array::from_fn(|i| {
            style.model_rotation[i] + style.model_spin[i] * particle.age_seconds
        });
        let axes = style.model_basis.map(|v| Vec3::new(v[0], v[1], v[2]));
        for v in &mesh.vertices {
            let p = rotate_particle_vector(std::array::from_fn(|i| v[i] * dimensions[i]), rotation);
            let n = rotate_particle_vector([v[3], v[4], v[5]], rotation);
            let world = center
                .add(axes[0].mul(p[0]))
                .add(axes[1].mul(p[1]))
                .add(axes[2].mul(p[2]));
            let normal = axes[0]
                .mul(n[0])
                .add(axes[1].mul(n[1]))
                .add(axes[2].mul(n[2]))
                .normalized();
            geometry::append_particle_vertex(out, world, normal, color, [v[6], v[7]]);
        }
        return;
    }
    if style.is_some_and(|s| s.trail) {
        let points = previous
            .map(|p| vec![p, particle.desc.position])
            .unwrap_or_else(|| particle.trail_history.clone());
        let width = size[0].max(size[1]) * 0.5;
        for pair in points.windows(2) {
            let a = Vec3::new(pair[0][0], pair[0][1], pair[0][2]);
            let b = Vec3::new(pair[1][0], pair[1][1], pair[1][2]);
            let segment = b.sub(a);
            if segment.length() < 0.00001 {
                continue;
            }
            let side = segment.cross(forward).normalized();
            let side = if side.length() < 0.0001 {
                camera_right
            } else {
                side
            };
            append_particle_quad(
                [
                    a.sub(side.mul(width)),
                    a.add(side.mul(width)),
                    b.add(side.mul(width)),
                    b.sub(side.mul(width)),
                ],
                normal,
                color,
                style,
                particle,
                out,
            );
        }
        return;
    }
    let angle = particle.desc.rotation_degrees.to_radians();
    let (s, c) = angle.sin_cos();
    let right = camera_right.mul(c).add(camera_up.mul(s));
    let up = camera_up.mul(c).sub(camera_right.mul(s));
    let (hx, hy) = (size[0] * 0.5, size[1] * 0.5);
    append_particle_quad(
        [
            center.sub(right.mul(hx)).sub(up.mul(hy)),
            center.add(right.mul(hx)).sub(up.mul(hy)),
            center.add(right.mul(hx)).add(up.mul(hy)),
            center.sub(right.mul(hx)).add(up.mul(hy)),
        ],
        normal,
        color,
        style,
        particle,
        out,
    );
}

fn append_particle_quad(
    corners: [Vec3; 4],
    normal: Vec3,
    color: [f32; 4],
    style: Option<&SceneParticleStyle>,
    particle: &SceneRuntimeParticle,
    out: &mut Vec<f32>,
) {
    for (corner, uv) in [
        (0usize, [0.0, 1.0]),
        (1, [1.0, 1.0]),
        (2, [1.0, 0.0]),
        (0, [0.0, 1.0]),
        (2, [1.0, 0.0]),
        (3, [0.0, 0.0]),
    ] {
        let uv = style.map_or(uv, |s| {
            s.uv(uv, particle.age_seconds, particle.desc.lifetime_seconds)
        });
        geometry::append_particle_vertex(out, corners[corner], normal, color, uv);
    }
}

fn rotate_particle_vector(v: [f32; 3], rotation: [f32; 3]) -> [f32; 3] {
    let [sx, sy, sz] = rotation.map(|a| a.to_radians().sin());
    let [cx, cy, cz] = rotation.map(|a| a.to_radians().cos());
    let y = v[1] * cx - v[2] * sx;
    let z = v[1] * sx + v[2] * cx;
    let x = v[0] * cy + z * sy;
    let z = -v[0] * sy + z * cy;
    [x * cz - y * sz, x * sz + y * cz, z]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_rotation_preserves_length() {
        let p = rotate_particle_vector([0.0, 1.0, 0.0], [90.0, 0.0, 0.0]);
        assert!((p[2] - 1.0).abs() < 0.0001 && p[1].abs() < 0.0001);
    }
}
