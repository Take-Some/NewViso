//! Continuous sphere queries against immutable triangle collision geometry.
//! The BVH is built at residency time, never in the camera's per-frame path.
type V = [f32; 3];
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V, s: f32) -> V {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[derive(Debug)]
struct Node {
    min: V,
    max: V,
    start: usize,
    end: usize,
    children: Option<(usize, usize)>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SphereSweepHit {
    pub fraction: f32,
    pub normal: V,
}

#[derive(Clone, Copy, Debug)]
struct IndexedTriangle {
    vertices: [V; 3],
    source_index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriangleSurfaceHit {
    pub triangle_index: u32,
    pub distance: f32,
}

#[derive(Debug)]
pub struct SphereSweepMesh {
    triangles: Vec<IndexedTriangle>,
    nodes: Vec<Node>,
}

impl SphereSweepMesh {
    pub fn new(vertices: &[V], indices: &[[u32; 3]]) -> Result<Self, String> {
        if vertices.iter().flatten().any(|v| !v.is_finite()) {
            return Err("sphere sweep mesh contains non-finite vertices".into());
        }
        let mut triangles = Vec::with_capacity(indices.len());
        for (source_index, ids) in indices.iter().enumerate() {
            let mut tri = [[0.0; 3]; 3];
            for i in 0..3 {
                tri[i] = *vertices
                    .get(ids[i] as usize)
                    .ok_or("sphere sweep mesh index out of bounds")?;
            }
            triangles.push(IndexedTriangle {
                vertices: tri,
                source_index: u32::try_from(source_index)
                    .map_err(|_| "sphere sweep mesh triangle index exceeds u32")?,
            });
        }
        let mut mesh = Self {
            triangles,
            nodes: Vec::new(),
        };
        if !mesh.triangles.is_empty() {
            mesh.build(0, mesh.triangles.len());
        }
        Ok(mesh)
    }

    fn build(&mut self, start: usize, end: usize) -> usize {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for tri in &self.triangles[start..end] {
            for p in &tri.vertices {
                for k in 0..3 {
                    min[k] = min[k].min(p[k]);
                    max[k] = max[k].max(p[k]);
                }
            }
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            min,
            max,
            start,
            end,
            children: None,
        });
        if end - start > 8 {
            let axis = (0..3)
                .max_by(|a, b| (max[*a] - min[*a]).total_cmp(&(max[*b] - min[*b])))
                .unwrap();
            let mid = start + (end - start) / 2;
            self.triangles[start..end].select_nth_unstable_by(mid - start, |a, b| {
                let ca = a.vertices[0][axis] + a.vertices[1][axis] + a.vertices[2][axis];
                let cb = b.vertices[0][axis] + b.vertices[1][axis] + b.vertices[2][axis];
                ca.total_cmp(&cb)
            });
            let left = self.build(start, mid);
            let right = self.build(mid, end);
            self.nodes[index].children = Some((left, right));
        }
        index
    }

    /// Returns the first contact fraction in [0,1], with both triangle sides enabled.
    pub fn sweep(&self, origin: V, delta: V, radius: f32) -> Option<f32> {
        self.sweep_hit(origin, delta, radius)
            .map(|hit| hit.fraction)
    }

    /// Returns the nearest contact together with a stable outward normal.
    pub fn sweep_hit(&self, origin: V, delta: V, radius: f32) -> Option<SphereSweepHit> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best: Option<SphereSweepHit> = None;
        let mut stack = vec![0usize];
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index];
            let best_fraction = best.map(|hit| hit.fraction).unwrap_or(1.0);
            if sweep_sphere_aabb(origin, delta, radius, node.min, node.max)
                .is_none_or(|t| t > best_fraction)
            {
                continue;
            }
            if let Some((a, b)) = node.children {
                stack.push(a);
                stack.push(b);
            } else {
                for tri in &self.triangles[node.start..node.end] {
                    if let Some(hit) = sweep_triangle_hit(origin, delta, radius, tri.vertices) {
                        if best.is_none_or(|current| hit.fraction <= current.fraction) {
                            best = Some(hit);
                        }
                    }
                }
            }
        }
        best
    }

    /// Resolve the authored source triangle nearest to a local-space contact point.
    ///
    /// BVH construction reorders triangles for locality, therefore every BVH triangle
    /// keeps the source index needed to recover per-triangle physical material metadata.
    pub fn nearest_triangle(&self, point: V, max_distance: f32) -> Option<TriangleSurfaceHit> {
        if self.nodes.is_empty()
            || point.iter().any(|value| !value.is_finite())
            || !max_distance.is_finite()
            || max_distance < 0.0
        {
            return None;
        }

        let mut best_distance_sq = max_distance * max_distance;
        let mut best: Option<(u32, f32)> = None;
        let mut stack = vec![0usize];
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index];
            if point_aabb_distance_squared(point, node.min, node.max) > best_distance_sq {
                continue;
            }
            if let Some((a, b)) = node.children {
                stack.push(a);
                stack.push(b);
                continue;
            }

            for tri in &self.triangles[node.start..node.end] {
                let distance_sq = point_triangle_distance_squared(point, tri.vertices);
                let replace = match best {
                    None => distance_sq <= best_distance_sq,
                    Some((old_index, old_distance_sq)) => {
                        distance_sq < old_distance_sq
                            || (distance_sq == old_distance_sq && tri.source_index < old_index)
                    }
                };
                if replace {
                    best_distance_sq = distance_sq;
                    best = Some((tri.source_index, distance_sq));
                }
            }
        }

        best.map(|(triangle_index, distance_sq)| TriangleSurfaceHit {
            triangle_index,
            distance: distance_sq.sqrt(),
        })
    }
}

fn point_aabb_distance_squared(point: V, min: V, max: V) -> f32 {
    let mut distance_sq = 0.0;
    for axis in 0..3 {
        let delta = if point[axis] < min[axis] {
            min[axis] - point[axis]
        } else if point[axis] > max[axis] {
            point[axis] - max[axis]
        } else {
            0.0
        };
        distance_sq += delta * delta;
    }
    distance_sq
}

fn point_segment_distance_squared(point: V, a: V, b: V) -> f32 {
    let ab = sub(b, a);
    let len_sq = dot(ab, ab);
    if len_sq <= 1.0e-12 {
        let delta = sub(point, a);
        return dot(delta, delta);
    }
    let t = (dot(sub(point, a), ab) / len_sq).clamp(0.0, 1.0);
    let delta = sub(point, add(a, mul(ab, t)));
    dot(delta, delta)
}

fn point_triangle_distance_squared(point: V, triangle: [V; 3]) -> f32 {
    let [a, b, c] = triangle;
    let ab = sub(b, a);
    let ac = sub(c, a);
    let normal = cross(ab, ac);
    let normal_len_sq = dot(normal, normal);
    if normal_len_sq <= 1.0e-12 {
        return point_segment_distance_squared(point, a, b)
            .min(point_segment_distance_squared(point, b, c))
            .min(point_segment_distance_squared(point, c, a));
    }

    let ap = sub(point, a);
    let projected = sub(point, mul(normal, dot(ap, normal) / normal_len_sq));
    if point_in_triangle(projected, triangle, normal) {
        let delta = sub(point, projected);
        return dot(delta, delta);
    }

    point_segment_distance_squared(point, a, b)
        .min(point_segment_distance_squared(point, b, c))
        .min(point_segment_distance_squared(point, c, a))
}

/// Conservative sphere sweep for primitive bounds; also used as BVH broad phase.
pub fn sweep_sphere_aabb(o: V, d: V, r: f32, min: V, max: V) -> Option<f32> {
    let mut enter: f32 = 0.0;
    let mut exit: f32 = 1.0;
    for k in 0..3 {
        let lo = min[k] - r;
        let hi = max[k] + r;
        if d[k].abs() < 1e-8 {
            if o[k] < lo || o[k] > hi {
                return None;
            }
        } else {
            let a = (lo - o[k]) / d[k];
            let b = (hi - o[k]) / d[k];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
            if enter > exit {
                return None;
            }
        }
    }
    (exit >= 0.0 && enter <= 1.0).then_some(enter.max(0.0))
}

pub fn sweep_sphere_aabb_hit(o: V, d: V, r: f32, min: V, max: V) -> Option<SphereSweepHit> {
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    let mut enter_normal = [0.0; 3];
    let mut starts_inside = true;
    for k in 0..3 {
        let lo = min[k] - r;
        let hi = max[k] + r;
        starts_inside &= o[k] >= lo && o[k] <= hi;
        if d[k].abs() < 1.0e-8 {
            if o[k] < lo || o[k] > hi {
                return None;
            }
            continue;
        }

        let t0 = (lo - o[k]) / d[k];
        let t1 = (hi - o[k]) / d[k];
        let (near, far, normal_sign) = if t0 <= t1 {
            (t0, t1, -1.0)
        } else {
            (t1, t0, 1.0)
        };
        if near > enter {
            enter = near;
            enter_normal = [0.0; 3];
            enter_normal[k] = normal_sign;
        }
        exit = exit.min(far);
        if enter > exit {
            return None;
        }
    }

    if exit < 0.0 || enter > 1.0 {
        return None;
    }
    if starts_inside {
        let normal = if dot(d, d) > 1.0e-12 {
            normalize(mul(d, -1.0))
        } else {
            [0.0, 1.0, 0.0]
        };
        return Some(SphereSweepHit {
            fraction: 0.0,
            normal,
        });
    }
    Some(SphereSweepHit {
        fraction: enter.max(0.0),
        normal: enter_normal,
    })
}

fn normalize(v: V) -> V {
    let len2 = dot(v, v);
    if len2 <= 1.0e-12 {
        [0.0, 1.0, 0.0]
    } else {
        mul(v, len2.sqrt().recip())
    }
}

fn point_in_triangle(p: V, t: [V; 3], n: V) -> bool {
    (0..3).all(|i| dot(cross(sub(t[(i + 1) % 3], t[i]), sub(p, t[i])), n) >= -1e-6)
}
fn sphere_hit(o: V, d: V, c: V, r: f32) -> Option<f32> {
    let oc = sub(o, c);
    let a = dot(d, d);
    let c = dot(oc, oc) - r * r;
    if c <= 0.0 {
        return Some(0.0);
    }
    if a < 1e-12 {
        return None;
    }
    let b = dot(oc, d);
    let h = b * b - a * c;
    if h < 0.0 {
        return None;
    }
    let t = (-b - h.sqrt()) / a;
    (0.0..=1.0).contains(&t).then_some(t)
}
fn edge_hit(o: V, d: V, a: V, b: V, r: f32) -> Option<f32> {
    let edge = sub(b, a);
    let len2 = dot(edge, edge);
    let rel = sub(o, a);
    let mut best = sphere_hit(o, d, a, r)
        .into_iter()
        .chain(sphere_hit(o, d, b, r))
        .min_by(f32::total_cmp);
    if len2 < 1e-12 {
        return best;
    }
    let u = dot(rel, edge) / len2;
    let closest = add(a, mul(edge, u.clamp(0.0, 1.0)));
    if dot(sub(o, closest), sub(o, closest)) <= r * r {
        return Some(0.0);
    }
    let perp_o = sub(rel, mul(edge, u));
    let du = dot(d, edge) / len2;
    let perp_d = sub(d, mul(edge, du));
    let aa = dot(perp_d, perp_d);
    let bb = dot(perp_o, perp_d);
    let cc = dot(perp_o, perp_o) - r * r;
    let h = bb * bb - aa * cc;
    if aa > 1e-12 && h >= 0.0 {
        let t = (-bb - h.sqrt()) / aa;
        let along = u + t * du;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&along) {
            best = Some(best.map_or(t, |old| old.min(t)));
        }
    }
    best
}
fn sphere_hit_record(o: V, d: V, c: V, r: f32) -> Option<SphereSweepHit> {
    let t = sphere_hit(o, d, c, r)?;
    let center = add(o, mul(d, t));
    Some(SphereSweepHit {
        fraction: t,
        normal: normalize(sub(center, c)),
    })
}

fn edge_hit_record(o: V, d: V, a: V, b: V, r: f32) -> Option<SphereSweepHit> {
    let t = edge_hit(o, d, a, b, r)?;
    let center = add(o, mul(d, t));
    let edge = sub(b, a);
    let len2 = dot(edge, edge);
    let closest = if len2 <= 1.0e-12 {
        a
    } else {
        let u = (dot(sub(center, a), edge) / len2).clamp(0.0, 1.0);
        add(a, mul(edge, u))
    };
    Some(SphereSweepHit {
        fraction: t,
        normal: normalize(sub(center, closest)),
    })
}

fn sweep_triangle_hit(o: V, d: V, r: f32, t: [V; 3]) -> Option<SphereSweepHit> {
    let mut best = None::<SphereSweepHit>;
    let raw_n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let len = dot(raw_n, raw_n).sqrt();
    if len > 1.0e-8 {
        let n = mul(raw_n, 1.0 / len);
        let distance = dot(sub(o, t[0]), n);
        if distance.abs() <= r && point_in_triangle(sub(o, mul(n, distance)), t, n) {
            let normal = if distance.abs() > 1.0e-6 {
                mul(n, distance.signum())
            } else if dot(d, n) > 0.0 {
                mul(n, -1.0)
            } else {
                n
            };
            return Some(SphereSweepHit {
                fraction: 0.0,
                normal,
            });
        }
        let speed = dot(d, n);
        if speed.abs() > 1.0e-8 {
            for sign in [-1.0, 1.0] {
                let time = (sign * r - distance) / speed;
                if (0.0..=1.0).contains(&time) {
                    let point = sub(add(o, mul(d, time)), mul(n, sign * r));
                    if point_in_triangle(point, t, n) {
                        let hit = SphereSweepHit {
                            fraction: time,
                            normal: mul(n, sign),
                        };
                        if best.is_none_or(|current| hit.fraction < current.fraction) {
                            best = Some(hit);
                        }
                    }
                }
            }
        }
    }

    for i in 0..3 {
        if let Some(hit) = edge_hit_record(o, d, t[i], t[(i + 1) % 3], r) {
            if best.is_none_or(|current| hit.fraction < current.fraction) {
                best = Some(hit);
            }
        }
    }
    for vertex in t {
        if let Some(hit) = sphere_hit_record(o, d, vertex, r) {
            if best.is_none_or(|current| hit.fraction < current.fraction) {
                best = Some(hit);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wall() -> SphereSweepMesh {
        SphereSweepMesh::new(
            &[
                [-2.0, -2.0, 0.0],
                [2.0, -2.0, 0.0],
                [2.0, 2.0, 0.0],
                [-2.0, 2.0, 0.0],
            ],
            &[[0, 1, 2], [0, 2, 3]],
        )
        .unwrap()
    }
    #[test]
    fn sweep_hit_reports_surface_normal() {
        let hit = wall()
            .sweep_hit([0.0, 0.0, 2.0], [0.0, 0.0, -4.0], 0.2)
            .unwrap();
        assert!((hit.fraction - 0.45).abs() < 1.0e-5);
        assert!(hit.normal[2] > 0.99);
    }

    #[test]
    fn aabb_hit_reports_entry_face_normal() {
        let hit = sweep_sphere_aabb_hit(
            [0.0, 1.0, 3.0],
            [0.0, 0.0, -5.0],
            0.25,
            [-1.0, 0.0, -1.0],
            [1.0, 2.0, 1.0],
        )
        .unwrap();
        assert!(hit.normal[2] > 0.99);
    }

    #[test]
    fn wall_stops_sphere_from_both_sides() {
        for sign in [-1.0, 1.0] {
            let t = wall()
                .sweep([0.0, 0.0, 2.0 * sign], [0.0, 0.0, -4.0 * sign], 0.2)
                .unwrap();
            assert!((t - 0.45).abs() < 1e-5);
        }
    }
    #[test]
    fn radius_catches_edges_that_center_ray_misses() {
        assert!(wall()
            .sweep([2.1, 0.0, 2.0], [0.0, 0.0, -4.0], 0.2)
            .is_some());
        assert!(wall()
            .sweep([2.3, 0.0, 2.0], [0.0, 0.0, -4.0], 0.2)
            .is_none());
        assert!(wall()
            .sweep([2.1, 2.1, 2.0], [0.0, 0.0, -4.0], 0.2)
            .is_some());
    }
    #[test]
    fn initial_overlap_and_long_motion_are_detected() {
        assert_eq!(
            wall().sweep([0.0, 0.0, 0.1], [0.0, 0.0, 4.0], 0.2),
            Some(0.0)
        );
        assert!(wall()
            .sweep([0.0, 0.0, 50.0], [0.0, 0.0, -100.0], 0.2)
            .is_some());
        assert!(wall()
            .sweep([0.0, 0.0, 2.0], [1.0, 0.0, 0.0], 0.2)
            .is_none());
    }
    #[test]
    fn nearest_triangle_preserves_source_index_after_bvh_reordering() {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for x in (0..20).rev() {
            let base = vertices.len() as u32;
            let xf = x as f32;
            vertices.extend([[xf, 0.0, 0.0], [xf + 0.8, 0.0, 0.0], [xf, 0.0, 0.8]]);
            indices.push([base, base + 1, base + 2]);
        }
        let mesh = SphereSweepMesh::new(&vertices, &indices).unwrap();
        let hit = mesh
            .nearest_triangle([5.1, 0.02, 0.1], 0.25)
            .expect("nearest authored triangle");
        assert_eq!(hit.triangle_index, 14);
        assert!(hit.distance < 0.021);
    }

    #[test]
    fn nearest_triangle_respects_max_distance() {
        let mesh = wall();
        assert!(mesh.nearest_triangle([0.0, 0.0, 0.1], 0.11).is_some());
        assert!(mesh.nearest_triangle([0.0, 0.0, 0.2], 0.11).is_none());
    }

    #[test]
    fn bvh_selects_nearest_surface_and_preserves_empty_space() {
        let mut v = vec![];
        let mut ids = vec![];
        for z in 0..20 {
            let i = v.len() as u32;
            v.extend([
                [-1.0, -1.0, z as f32],
                [1.0, -1.0, z as f32],
                [0.0, 1.0, z as f32],
            ]);
            ids.push([i, i + 1, i + 2]);
        }
        let mesh = SphereSweepMesh::new(&v, &ids).unwrap();
        assert!(
            (mesh
                .sweep([0.0, 0.0, 22.0], [0.0, 0.0, -30.0], 0.2)
                .unwrap()
                - 2.8 / 30.0)
                .abs()
                < 1e-5
        );
        assert!(mesh
            .sweep([4.0, 0.0, 22.0], [0.0, 0.0, -30.0], 0.2)
            .is_none());
    }
}
