//! Coplanar connected support regions derived from collision topology.
//! Internal triangulation edges disappear; holes and disconnected treads do not.
use std::collections::{BTreeMap, BTreeSet};
type V = [f32; 3];
type P = [f64; 2];
const WELD: f64 = 0.00001;
const EPS: f64 = 0.000002;
fn sub(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: V, b: V) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn p(v: V) -> P {
    [v[0] as f64, v[2] as f64]
}
fn side(a: P, b: P, c: P) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn key(v: V) -> [i64; 3] {
    v.map(|x| (f64::from(x) / WELD).round() as i64)
}
fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}
#[derive(Clone, Debug)]
struct PointNode {
    min: P,
    max: P,
    start: usize,
    end: usize,
    children: Option<(usize, usize)>,
}
#[derive(Clone, Debug)]
pub struct SupportSurface {
    pub id: u32,
    pub normal: V,
    pub boundary: Vec<[V; 2]>,
    triangles: Vec<[V; 3]>,
    origin: V,
    point_nodes: Vec<PointNode>,
}
#[derive(Debug, Default)]
pub(super) struct SupportSurfaceIndex {
    pub surfaces: Vec<SupportSurface>,
    pub triangle_surfaces: Vec<Option<usize>>,
}
impl SupportSurfaceIndex {
    pub fn new(triangles: &[[V; 3]]) -> Self {
        let planes = triangles
            .iter()
            .map(|t| {
                let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
                let len = dot(n, n).sqrt();
                if len < 1e-8 || n[1].abs() / len < 0.1 {
                    None
                } else {
                    Some(n.map(|v| v / len * n[1].signum()))
                }
            })
            .collect::<Vec<_>>();
        let mut parent = (0..triangles.len()).collect::<Vec<_>>();
        let mut edges: BTreeMap<([i64; 3], [i64; 3]), Vec<usize>> = BTreeMap::new();
        for (i, t) in triangles
            .iter()
            .enumerate()
            .filter(|(i, _)| planes[*i].is_some())
        {
            for k in 0..3 {
                let (a, b) = (key(t[k]), key(t[(k + 1) % 3]));
                edges.entry((a.min(b), a.max(b))).or_default().push(i);
            }
        }
        for ids in edges.values() {
            for (k, &a) in ids.iter().enumerate() {
                for &b in &ids[k + 1..] {
                    let na = planes[a].unwrap();
                    let nb = planes[b].unwrap();
                    if dot(na, nb) > 0.999999
                        && triangles[b]
                            .iter()
                            .all(|&v| dot(na, sub(v, triangles[a][0])).abs() < 0.00001)
                    {
                        let ra = root(&mut parent, a);
                        let rb = root(&mut parent, b);
                        parent[ra.max(rb)] = ra.min(rb);
                    }
                }
            }
        }
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for i in 0..triangles.len() {
            if planes[i].is_some() {
                let r = root(&mut parent, i);
                groups.entry(r).or_default().push(i);
            }
        }
        let mut result = Self {
            surfaces: Vec::new(),
            triangle_surfaces: vec![None; triangles.len()],
        };
        for (r, ids) in groups {
            let mut border: BTreeMap<([i64; 3], [i64; 3]), (usize, [V; 2])> = BTreeMap::new();
            for &i in &ids {
                result.triangle_surfaces[i] = Some(result.surfaces.len());
                for k in 0..3 {
                    let a = triangles[i][k];
                    let b = triangles[i][(k + 1) % 3];
                    let (ka, kb) = (key(a), key(b));
                    let e = border
                        .entry((ka.min(kb), ka.max(kb)))
                        .or_insert((0, [a, b]));
                    e.0 += 1;
                }
            }
            let mut surface = SupportSurface {
                id: r as u32,
                normal: planes[r].unwrap(),
                origin: triangles[r][0],
                boundary: border
                    .into_values()
                    .filter_map(|(n, e)| (n != 2).then_some(e))
                    .collect(),
                triangles: ids.iter().map(|&i| triangles[i]).collect(),
                point_nodes: Vec::new(),
            };
            surface.build_points(0, surface.triangles.len());
            result.surfaces.push(surface);
        }
        result
    }
}
impl SupportSurface {
    /// Oriented primitive face with the same footprint contract as triangle regions.
    pub fn from_quad(id: u32, corners: [V; 4]) -> Option<Self> {
        let n = cross(sub(corners[1], corners[0]), sub(corners[2], corners[0]));
        let len = dot(n, n).sqrt();
        if len < 1e-8 || n[1].abs() / len < 0.1 {
            return None;
        }
        let mut surface = Self {
            id,
            normal: n.map(|v| v / len * n[1].signum()),
            origin: corners[0],
            boundary: (0..4).map(|i| [corners[i], corners[(i + 1) % 4]]).collect(),
            triangles: vec![
                [corners[0], corners[1], corners[2]],
                [corners[0], corners[2], corners[3]],
            ],
            point_nodes: Vec::new(),
        };
        surface.build_points(0, surface.triangles.len());
        Some(surface)
    }

    pub fn height(&self, x: f32, z: f32) -> f32 {
        self.origin[1]
            - (self.normal[0] * (x - self.origin[0]) + self.normal[2] * (z - self.origin[2]))
                / self.normal[1]
    }
    fn build_points(&mut self, start: usize, end: usize) -> usize {
        let min = std::array::from_fn(|k| {
            self.triangles[start..end]
                .iter()
                .flatten()
                .map(|v| p(*v)[k])
                .fold(f64::INFINITY, f64::min)
        });
        let max = std::array::from_fn(|k| {
            self.triangles[start..end]
                .iter()
                .flatten()
                .map(|v| p(*v)[k])
                .fold(f64::NEG_INFINITY, f64::max)
        });
        let i = self.point_nodes.len();
        self.point_nodes.push(PointNode {
            min,
            max,
            start,
            end,
            children: None,
        });
        if end - start > 8 {
            let axis = usize::from(max[1] - min[1] > max[0] - min[0]);
            let mid = start + (end - start) / 2;
            self.triangles[start..end].select_nth_unstable_by(mid - start, |a, b| {
                let centre = |t: &[V; 3]| t.iter().map(|v| p(*v)[axis]).sum::<f64>();
                centre(a).total_cmp(&centre(b))
            });
            let a = self.build_points(start, mid);
            let b = self.build_points(mid, end);
            self.point_nodes[i].children = Some((a, b));
        }
        i
    }
    fn contains_point(&self, q: P) -> bool {
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            let n = &self.point_nodes[i];
            if (0..2).any(|k| q[k] < n.min[k] - EPS || q[k] > n.max[k] + EPS) {
                continue;
            }
            if let Some((a, b)) = n.children {
                stack.push(a);
                stack.push(b);
            } else if self.triangles[n.start..n.end].iter().any(|t| {
                let [a, b, c] = t.map(p);
                let s = [side(a, b, q), side(b, c, q), side(c, a, q)];
                s.iter().all(|v| *v >= -EPS) || s.iter().all(|v| *v <= EPS)
            }) {
                return true;
            }
        }
        false
    }
    /// Exact boundary test, including concave edges and holes fully inside the sole.
    pub fn contains_footprint(&self, corners: [[f32; 2]; 4]) -> bool {
        let q = corners.map(|v| v.map(f64::from));
        if !q.iter().all(|&v| self.contains_point(v)) {
            return false;
        }
        let inside_strict = |v: P| {
            let s = [0usize, 1, 2, 3].map(|i| side(q[i], q[(i + 1) % 4], v));
            s.iter().all(|x| *x > EPS) || s.iter().all(|x| *x < -EPS)
        };
        let min = std::array::from_fn::<_, 2, _>(|k| {
            q.iter().map(|v| v[k]).fold(f64::INFINITY, f64::min)
        });
        let max = std::array::from_fn::<_, 2, _>(|k| {
            q.iter().map(|v| v[k]).fold(f64::NEG_INFINITY, f64::max)
        });
        for edge in &self.boundary {
            let [a, b] = edge.map(p);
            if (0..2).any(|k| a[k].min(b[k]) > max[k] + EPS || a[k].max(b[k]) < min[k] - EPS) {
                continue;
            }
            if inside_strict(a)
                || inside_strict(b)
                || inside_strict([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5])
            {
                return false;
            }
            for i in 0..4 {
                let c = q[i];
                let d = q[(i + 1) % 4];
                let opposite = |x: f64, y: f64| (x > EPS && y < -EPS) || (y > EPS && x < -EPS);
                if opposite(side(a, b, c), side(a, b, d)) && opposite(side(c, d, a), side(c, d, b))
                {
                    return false;
                }
            }
        }
        true
    }
    /// Closest feasible ankle translation. Never attracts a valid foot to a centre.
    pub fn fit_footprint(
        &self,
        desired: [f32; 2],
        offsets: [[f32; 2]; 4],
        max_shift: f32,
    ) -> Option<V> {
        if !max_shift.is_finite()
            || max_shift < 0.0
            || desired
                .iter()
                .chain(offsets.iter().flatten())
                .any(|v| !v.is_finite())
        {
            return None;
        }
        let corners =
            |v: P| offsets.map(|o| [(v[0] + o[0] as f64) as f32, (v[1] + o[1] as f64) as f32]);
        let d = desired.map(f64::from);
        if self.contains_footprint(corners(d)) {
            return Some([desired[0], self.height(desired[0], desired[1]), desired[1]]);
        }
        let radius = f64::from(max_shift)
            + offsets
                .iter()
                .map(|o| f64::from(o[0]).hypot(f64::from(o[1])))
                .fold(0.0, f64::max);
        let mut lines = Vec::<(P, f64)>::new();
        for edge in &self.boundary {
            let [a, b] = edge.map(p);
            if (0..2).any(|k| a[k].min(b[k]) > d[k] + radius || a[k].max(b[k]) < d[k] - radius) {
                continue;
            }
            let len = (b[0] - a[0]).hypot(b[1] - a[1]);
            if len < EPS {
                continue;
            }
            let n = [(a[1] - b[1]) / len, (b[0] - a[0]) / len];
            for o in offsets {
                let c = n[0] * (a[0] - o[0] as f64) + n[1] * (a[1] - o[1] as f64);
                lines.push((n, c));
            }
        }
        // Foot-edge/region-vertex contacts also bound the configuration-space region.
        for edge in &self.boundary {
            for v in edge {
                let v = p(*v);
                if (0..2).any(|k| (v[k] - d[k]).abs() > radius) {
                    continue;
                }
                for i in 0..4 {
                    let a = offsets[i].map(f64::from);
                    let b = offsets[(i + 1) % 4].map(f64::from);
                    let len = (b[0] - a[0]).hypot(b[1] - a[1]);
                    if len < EPS {
                        continue;
                    }
                    let n = [(a[1] - b[1]) / len, (b[0] - a[0]) / len];
                    lines.push((n, n[0] * (v[0] - a[0]) + n[1] * (v[1] - a[1])));
                }
            }
        }
        let mut unique = BTreeSet::new();
        lines.retain(|(n, c)| {
            unique.insert((
                (n[0] / EPS).round() as i64,
                (n[1] / EPS).round() as i64,
                (c / EPS).round() as i64,
            ))
        });
        let mut best = None;
        let mut best_dist = f64::from(max_shift).powi(2) + EPS * EPS;
        let mut consider = |q: P| {
            let dist = (q[0] - d[0]).powi(2) + (q[1] - d[1]).powi(2);
            if dist < best_dist && self.contains_footprint(corners(q)) {
                best = Some(q);
                best_dist = dist;
            }
        };
        for (i, &(n, c)) in lines.iter().enumerate() {
            let t = c - n[0] * d[0] - n[1] * d[1];
            consider([d[0] + n[0] * t, d[1] + n[1] * t]);
            for &(m, h) in &lines[..i] {
                let det = n[0] * m[1] - n[1] * m[0];
                if det.abs() < EPS {
                    continue;
                }
                consider([(c * m[1] - n[1] * h) / det, (n[0] * h - c * m[0]) / det]);
            }
        }
        best.map(|q| {
            [
                q[0] as f32,
                self.height(q[0] as f32, q[1] as f32),
                q[1] as f32,
            ]
        })
    }
}
#[cfg(test)]
mod tests;
