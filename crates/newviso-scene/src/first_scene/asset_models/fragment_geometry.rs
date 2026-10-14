//! Authored fragment articulation, dents and precise surface hits.
use super::*;

/// Build render and detached geometry from the same authored vertex and captured
/// articulation. Hidden source triangles are collapsed for rendering, but must
/// not become collapsed geometry when handed to an independent fragment body.
pub(super) fn posed_fragment_vertex(
    bind: AssetTriangleVertex,
    pose: &SceneModelPartPose,
    force_visible: bool,
) -> (Vec3, Vec3, Vec3) {
    let visible = pose.visible || force_visible;
    let pivot = Vec3::new(pose.pivot[0], pose.pivot[1], pose.pivot[2]);
    let center = pivot.add(Vec3::new(
        pose.translation[0],
        pose.translation[1],
        pose.translation[2],
    ));
    let rotation = Vec3::new(
        pose.rotation_degrees[0],
        pose.rotation_degrees[1],
        pose.rotation_degrees[2],
    );
    let scale = if visible {
        Vec3::new(pose.scale[0], pose.scale[1], pose.scale[2])
    } else {
        Vec3::ZERO
    };
    let mut position = transform_point(bind.position.sub(pivot), scale, rotation, center);
    let mut normal = if visible {
        transform_point(bind.normal, Vec3::new(1.0, 1.0, 1.0), rotation, Vec3::ZERO).normalized()
    } else {
        bind.normal
    };
    let bind_tangent = Vec3::new(bind.tangent[0], bind.tangent[1], bind.tangent[2]);
    let mut tangent = if visible {
        transform_point(bind_tangent, Vec3::new(1.0, 1.0, 1.0), rotation, Vec3::ZERO).normalized()
    } else {
        bind_tangent
    };
    if visible && pose.post_rotation_scale != [1.0; 3] {
        let q = position.sub(center);
        position = center.add(Vec3::new(
            q.x * pose.post_rotation_scale[0],
            q.y * pose.post_rotation_scale[1],
            q.z * pose.post_rotation_scale[2],
        ));
        normal = Vec3::new(
            normal.x / pose.post_rotation_scale[0],
            normal.y / pose.post_rotation_scale[1],
            normal.z / pose.post_rotation_scale[2],
        )
        .normalized();
        tangent = Vec3::new(
            tangent.x * pose.post_rotation_scale[0],
            tangent.y * pose.post_rotation_scale[1],
            tangent.z * pose.post_rotation_scale[2],
        )
        .normalized();
    }
    if visible && !pose.dents.is_empty() {
        (position, normal, tangent) = apply_fragment_dents(position, normal, tangent, &pose.dents);
    }
    (position, normal, tangent)
}

pub(super) fn apply_fragment_dents(
    mut p: Vec3,
    mut n: Vec3,
    mut t: Vec3,
    dents: &[SceneModelDent],
) -> (Vec3, Vec3, Vec3) {
    for dent in dents {
        let centre = Vec3::new(dent.point[0], dent.point[1], dent.point[2]);
        let delta = Vec3::new(
            dent.displacement[0],
            dent.displacement[1],
            dent.displacement[2],
        );
        let v = p.sub(centre);
        let radius_sq = dent.radius * dent.radius;
        let q = (v.x * v.x + v.y * v.y + v.z * v.z) / radius_sq.max(0.0001);
        if q >= 1.0 {
            continue;
        }
        let w = (1.0 - q).powi(2);
        let g = Vec3::new(v.x, v.y, v.z).mul(-4.0 * (1.0 - q) / radius_sq);
        let dot = |a: Vec3, b: Vec3| a.x * b.x + a.y * b.y + a.z * b.z;
        let denominator = (1.0 + dot(g, delta)).max(0.2);
        n = n.sub(g.mul(dot(delta, n) / denominator)).normalized();
        t = t.add(delta.mul(dot(g, t))).normalized();
        p = p.add(delta.mul(w));
    }
    (p, n, t)
}

/// Closest point including edges, with barycentrics for the original glass UVs.
pub(super) fn closest_fragment_triangle(p: [f32; 3], t: [[f32; 3]; 3]) -> Option<([f32; 3], [f32; 3])> {
    let sub = |a: [f32; 3], b: [f32; 3]| std::array::from_fn(|i| a[i] - b[i]);
    let dot = |a: [f32; 3], b: [f32; 3]| (0..3).map(|i| a[i] * b[i]).sum::<f32>();
    let ab = sub(t[1], t[0]);
    let ac = sub(t[2], t[0]);
    let ap = sub(p, t[0]);
    let d00 = dot(ab, ab);
    let d01 = dot(ab, ac);
    let d11 = dot(ac, ac);
    let determinant = d00 * d11 - d01 * d01;
    if determinant <= 1.0e-12 {
        return None;
    }
    let u = (d11 * dot(ap, ab) - d01 * dot(ap, ac)) / determinant;
    let v = (d00 * dot(ap, ac) - d01 * dot(ap, ab)) / determinant;
    if u >= 0.0 && v >= 0.0 && u + v <= 1.0 {
        let weights = [1.0 - u - v, u, v];
        return Some((
            std::array::from_fn(|a| (0..3).map(|i| weights[i] * t[i][a]).sum()),
            weights,
        ));
    }
    let mut best = None;
    for (i, j) in [(0, 1), (1, 2), (2, 0)] {
        let edge = sub(t[j], t[i]);
        let f = (dot(sub(p, t[i]), edge) / dot(edge, edge).max(1.0e-12)).clamp(0.0, 1.0);
        let hit = std::array::from_fn(|a| t[i][a] + edge[a] * f);
        let distance = dot(sub(p, hit), sub(p, hit));
        let mut weights = [0.0; 3];
        weights[i] = 1.0 - f;
        weights[j] = f;
        if best.as_ref().is_none_or(|(d, _, _)| distance < *d) {
            best = Some((distance, hit, weights));
        }
    }
    best.map(|(_, hit, weights)| (hit, weights))
}
