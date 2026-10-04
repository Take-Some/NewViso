use super::*;

pub(super) fn reconstruct(start: u64, end: u64, came_from: &BTreeMap<u64, u64>) -> Vec<u64> {
    let mut out = vec![end];
    let mut current = end;
    while current != start {
        let Some(previous) = came_from.get(&current).copied() else {
            return Vec::new();
        };
        current = previous;
        out.push(current);
    }
    out.reverse();
    out
}

pub(super) fn orient_portal(a: Vec3, b: Vec3, from: Vec3, to: Vec3) -> (Vec3, Vec3) {
    let dir = [to[0] - from[0], to[2] - from[2]];
    let edge = [b[0] - a[0], b[2] - a[2]];
    if cross2(dir, edge) >= 0.0 {
        (a, b)
    } else {
        (b, a)
    }
}

pub(super) fn string_pull(start: Vec3, end: Vec3, portals: &[(Vec3, Vec3)]) -> Vec<Vec3> {
    if portals.is_empty() {
        return vec![start, end];
    }

    let mut all = Vec::with_capacity(portals.len() + 2);
    all.push((start, start));
    all.extend_from_slice(portals);
    all.push((end, end));

    let mut points = vec![start];
    let mut apex = start;
    let mut left = all[1].0;
    let mut right = all[1].1;
    let mut apex_index: usize;
    let mut left_index = 1usize;
    let mut right_index = 1usize;
    let mut i = 2usize;

    while i < all.len() {
        let (new_left, new_right) = all[i];

        if tri_area2(apex, right, new_right) <= 0.0 {
            if same_xz(apex, right) || tri_area2(apex, left, new_right) > 0.0 {
                right = new_right;
                right_index = i;
            } else {
                points.push(left);
                apex = left;
                apex_index = left_index;
                left = apex;
                right = apex;
                left_index = apex_index;
                right_index = apex_index;
                i = apex_index + 1;
                continue;
            }
        }

        if tri_area2(apex, left, new_left) >= 0.0 {
            if same_xz(apex, left) || tri_area2(apex, right, new_left) < 0.0 {
                left = new_left;
                left_index = i;
            } else {
                points.push(right);
                apex = right;
                apex_index = right_index;
                left = apex;
                right = apex;
                left_index = apex_index;
                right_index = apex_index;
                i = apex_index + 1;
                continue;
            }
        }

        i += 1;
    }

    if points
        .last()
        .is_none_or(|point| distance(*point, end) > 1.0e-4)
    {
        points.push(end);
    }
    points
}

pub(super) fn tri_area2(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    (b[0] - a[0]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[0] - a[0])
}
pub(super) fn cross2(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}
pub(super) fn same_xz(a: Vec3, b: Vec3) -> bool {
    (a[0] - b[0]).abs() < 1.0e-5 && (a[2] - b[2]).abs() < 1.0e-5
}

pub(super) fn triangle_normal(v: [Vec3; 3]) -> Vec3 {
    normalize(cross(sub(v[1], v[0]), sub(v[2], v[0])))
}
pub(super) fn centroid(v: [Vec3; 3]) -> Vec3 {
    mul(add(add(v[0], v[1]), v[2]), 1.0 / 3.0)
}
pub(super) fn closest_point_on_triangle(p: Vec3, tri: [Vec3; 3]) -> Vec3 {
    // Christer Ericson, Real-Time Collision Detection.
    let a = tri[0];
    let b = tri[1];
    let c = tri[2];
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, mul(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, mul(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return add(b, mul(sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    add(a, add(mul(ab, v), mul(ac, w)))
}

pub(super) fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    let dx = a[0] - b[0];
    let dz = a[2] - b[2];
    (dx * dx + dz * dz).sqrt()
}
pub(super) fn distance(a: Vec3, b: Vec3) -> f32 {
    length(sub(a, b))
}
pub(super) fn length(v: Vec3) -> f32 {
    dot(v, v).sqrt()
}
pub(super) fn normalize(v: Vec3) -> Vec3 {
    let len = length(v);
    if len <= 1.0e-8 {
        [0.0, 1.0, 0.0]
    } else {
        mul(v, 1.0 / len)
    }
}
pub(super) fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub(super) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(super) fn mul(v: Vec3, s: f32) -> Vec3 {
    [v[0] * s, v[1] * s, v[2] * s]
}
pub(super) fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(super) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
