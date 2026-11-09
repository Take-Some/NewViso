//! Continuous collision of linearly moving strike and receiver capsules.
//! The conservative advancement bound includes both endpoints of both segments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrikeCapsule {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub radius: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrikeContact {
    pub fraction: f32,
    pub point: [f32; 3],
    /// From the receiving surface towards the strike.
    pub normal: [f32; 3],
}
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    a.map(|v| v * s)
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    add(a, scale(sub(b, a), t))
}
impl StrikeCapsule {
    pub fn valid(self) -> bool {
        self.start.iter().chain(&self.end).all(|v| v.is_finite())
            && self.radius.is_finite()
            && self.radius > 0.0
    }
    pub fn interpolate(self, other: Self, t: f32) -> Self {
        Self {
            start: mix(self.start, other.start, t),
            end: mix(self.end, other.end, t),
            radius: self.radius + (other.radius - self.radius) * t,
        }
    }
    pub fn bounds(self, other: Self) -> ([f32; 3], [f32; 3]) {
        let radius = self.radius.max(other.radius);
        (
            std::array::from_fn(|i| {
                self.start[i]
                    .min(self.end[i])
                    .min(other.start[i])
                    .min(other.end[i])
                    - radius
            }),
            std::array::from_fn(|i| {
                self.start[i]
                    .max(self.end[i])
                    .max(other.start[i])
                    .max(other.end[i])
                    + radius
            }),
        )
    }
}
/// Closest points of finite, possibly degenerate segments.
pub fn closest_segment_points(
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    d: [f32; 3],
) -> ([f32; 3], [f32; 3]) {
    let u = sub(b, a);
    let v = sub(d, c);
    let w = sub(a, c);
    let aa = dot(u, u);
    let bb = dot(u, v);
    let cc = dot(v, v);
    let dd = dot(u, w);
    let ee = dot(v, w);
    let tiny = f32::EPSILON * f32::EPSILON;
    let (s, t) = if aa <= tiny && cc <= tiny {
        (0.0, 0.0)
    } else if aa <= tiny {
        (0.0, (ee / cc).clamp(0.0, 1.0))
    } else if cc <= tiny {
        ((-dd / aa).clamp(0.0, 1.0), 0.0)
    } else {
        let denom = aa * cc - bb * bb;
        let mut s = if denom > tiny {
            (bb * ee - cc * dd) / denom
        } else {
            0.0
        }
        .clamp(0.0, 1.0);
        let raw = (bb * s + ee) / cc;
        let t = raw.clamp(0.0, 1.0);
        if raw < 0.0 {
            s = (-dd / aa).clamp(0.0, 1.0);
        } else if raw > 1.0 {
            s = ((bb - dd) / aa).clamp(0.0, 1.0);
        }
        (s, t)
    };
    (add(a, scale(u, s)), add(c, scale(v, t)))
}
pub fn sweep_strike_capsules(
    previous: StrikeCapsule,
    current: StrikeCapsule,
    receiver_previous: StrikeCapsule,
    receiver_current: StrikeCapsule,
) -> Option<StrikeContact> {
    if ![previous, current, receiver_previous, receiver_current]
        .iter()
        .all(|c| c.valid())
    {
        return None;
    }
    let (amin, amax) = previous.bounds(current);
    let (bmin, bmax) = receiver_previous.bounds(receiver_current);
    if (0..3).any(|i| amax[i] < bmin[i] || bmax[i] < amin[i]) {
        return None;
    }
    let speed = length(sub(current.start, previous.start))
        .max(length(sub(current.end, previous.end)))
        + length(sub(receiver_current.start, receiver_previous.start))
            .max(length(sub(receiver_current.end, receiver_previous.end)))
        + (current.radius - previous.radius).abs()
        + (receiver_current.radius - receiver_previous.radius).abs();
    // Accuracy scales with geometry, independently of world origin and frame rate.
    let tolerance = previous.radius.min(receiver_previous.radius) * 0.0001;
    let mut t = 0.0;
    for _ in 0..128 {
        let a = previous.interpolate(current, t);
        let b = receiver_previous.interpolate(receiver_current, t);
        let (pa, pb) = closest_segment_points(a.start, a.end, b.start, b.end);
        let offset = sub(pa, pb);
        let distance = length(offset);
        let gap = distance - a.radius - b.radius;
        if gap <= tolerance {
            let normal = if distance > tolerance {
                scale(offset, 1.0 / distance)
            } else {
                let travel = sub(
                    sub(previous.start, current.start),
                    sub(receiver_previous.start, receiver_current.start),
                );
                let n = length(travel);
                if n > tolerance {
                    scale(travel, 1.0 / n)
                } else {
                    [0.0, 1.0, 0.0]
                }
            };
            return Some(StrikeContact {
                fraction: t,
                point: add(pb, scale(normal, b.radius)),
                normal,
            });
        }
        if speed <= tolerance {
            return None;
        }
        let next = t + gap / speed;
        if next > 1.0 {
            return None;
        }
        if next <= t {
            return None;
        }
        t = next;
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    fn cap(x: f32, y: f32, z: f32) -> StrikeCapsule {
        StrikeCapsule {
            start: [x, y, z],
            end: [x, y + 0.4, z],
            radius: 0.05,
        }
    }
    #[test]
    fn high_speed_crossing_hits_between_endpoints() {
        let h = sweep_strike_capsules(
            cap(-4.0, 0.0, 0.0),
            cap(4.0, 0.0, 0.0),
            cap(0.0, 0.0, 0.0),
            cap(0.0, 0.0, 0.0),
        )
        .unwrap();
        assert!((h.fraction - 0.4875).abs() < 0.0001);
        assert!((h.point[0] + 0.05).abs() < 0.0001);
    }
    #[test]
    fn moving_receiver_and_rotating_long_weapon_are_continuous() {
        let a = StrikeCapsule {
            start: [0.0; 3],
            end: [2.0, 0.0, 0.0],
            radius: 0.04,
        };
        let b = StrikeCapsule {
            end: [0.0, 0.0, 2.0],
            ..a
        };
        let r = cap(0.8, -0.2, 0.8);
        assert!(sweep_strike_capsules(a, b, r, r).is_some());
        assert!(sweep_strike_capsules(
            cap(0.0, 0.0, 0.0),
            cap(0.0, 0.0, 0.0),
            cap(-1.0, 0.0, 0.0),
            cap(1.0, 0.0, 0.0)
        )
        .is_some());
    }
    #[test]
    fn near_miss_and_invalid_inputs_do_not_invent_contacts() {
        assert!(sweep_strike_capsules(
            cap(-1.0, 0.0, 0.2),
            cap(1.0, 0.0, 0.2),
            cap(0.0, 0.0, 0.0),
            cap(0.0, 0.0, 0.0)
        )
        .is_none());
        assert!(sweep_strike_capsules(
            cap(f32::NAN, 0.0, 0.0),
            cap(1.0, 0.0, 0.0),
            cap(0.0, 0.0, 0.0),
            cap(0.0, 0.0, 0.0)
        )
        .is_none());
    }
    #[test]
    fn partitioning_the_same_motion_preserves_earliest_contact() {
        let start = cap(-2.0, 0.0, 0.0);
        let end = cap(2.0, 0.0, 0.0);
        let r = cap(0.0, 0.0, 0.0);
        let whole = sweep_strike_capsules(start, end, r, r).unwrap().fraction;
        for count in [2, 7, 60] {
            let mut found = None;
            for n in 0..count {
                let a = start.interpolate(end, n as f32 / count as f32);
                let b = start.interpolate(end, (n + 1) as f32 / count as f32);
                if let Some(h) = sweep_strike_capsules(a, b, r, r) {
                    found = Some((n as f32 + h.fraction) / count as f32);
                    break;
                }
            }
            assert!((whole - found.unwrap()).abs() < 0.0001);
        }
    }
    #[test]
    fn degenerate_and_parallel_segments_have_correct_closest_points() {
        let (a, b) = closest_segment_points([0.0; 3], [0.0; 3], [1.0, 0.0, 0.0], [1.0, 2.0, 0.0]);
        assert_eq!(a, [0.0; 3]);
        assert_eq!(b, [1.0, 0.0, 0.0]);
        let (a, b) =
            closest_segment_points([0.0; 3], [0.0, 2.0, 0.0], [1.0, 1.0, 0.0], [1.0, 3.0, 0.0]);
        assert!((length(sub(a, b)) - 1.0).abs() < 0.00001);
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn inside_triangle(p: [f32; 3], tri: [[f32; 3]; 3], n: [f32; 3]) -> bool {
    (0..3).all(|i| dot(cross(sub(tri[(i + 1) % 3], tri[i]), sub(p, tri[i])), n) >= -f32::EPSILON)
}
fn closest_triangle_point(p: [f32; 3], tri: [[f32; 3]; 3]) -> [f32; 3] {
    let normal = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
    let n2 = dot(normal, normal);
    if n2 > f32::EPSILON * f32::EPSILON {
        let projected = sub(p, scale(normal, dot(sub(p, tri[0]), normal) / n2));
        if inside_triangle(projected, tri, normal) {
            return projected;
        }
    }
    (0..3)
        .map(|i| closest_segment_points(p, p, tri[i], tri[(i + 1) % 3]).1)
        .min_by(|a, b| dot(sub(*a, p), sub(*a, p)).total_cmp(&dot(sub(*b, p), sub(*b, p))))
        .unwrap_or(tri[0])
}
fn closest_segment_triangle(a: [f32; 3], b: [f32; 3], tri: [[f32; 3]; 3]) -> ([f32; 3], [f32; 3]) {
    let normal = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
    let denom = dot(sub(b, a), normal);
    if denom.abs() > f32::EPSILON {
        let t = dot(sub(tri[0], a), normal) / denom;
        if (0.0..=1.0).contains(&t) {
            let p = mix(a, b, t);
            if inside_triangle(p, tri, normal) {
                return (p, p);
            }
        }
    }
    let mut best = (a, closest_triangle_point(a, tri));
    let mut consider = |pair: ([f32; 3], [f32; 3])| {
        if dot(sub(pair.0, pair.1), sub(pair.0, pair.1))
            < dot(sub(best.0, best.1), sub(best.0, best.1))
        {
            best = pair;
        }
    };
    consider((b, closest_triangle_point(b, tri)));
    for i in 0..3 {
        consider(closest_segment_points(a, b, tri[i], tri[(i + 1) % 3]));
    }
    best
}
/// Exact segment-to-triangle distance, advanced conservatively through time.
pub fn sweep_strike_triangle(
    previous: StrikeCapsule,
    current: StrikeCapsule,
    triangle: [[f32; 3]; 3],
) -> Option<StrikeContact> {
    if !previous.valid() || !current.valid() || triangle.iter().flatten().any(|v| !v.is_finite()) {
        return None;
    }
    let speed = length(sub(current.start, previous.start))
        .max(length(sub(current.end, previous.end)))
        + (current.radius - previous.radius).abs();
    let tolerance = previous.radius.min(current.radius) * 0.0001;
    let mut t = 0.0;
    for _ in 0..128 {
        let c = previous.interpolate(current, t);
        let (a, b) = closest_segment_triangle(c.start, c.end, triangle);
        let delta = sub(a, b);
        let distance = length(delta);
        let gap = distance - c.radius;
        if gap <= tolerance {
            let normal = if distance > tolerance {
                scale(delta, 1.0 / distance)
            } else {
                let n = cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
                let n = scale(n, 1.0 / length(n).max(f32::EPSILON));
                if dot(n, sub(current.start, previous.start)) > 0.0 {
                    scale(n, -1.0)
                } else {
                    n
                }
            };
            return Some(StrikeContact {
                fraction: t,
                point: b,
                normal,
            });
        }
        if speed <= tolerance {
            return None;
        }
        let next = t + gap / speed;
        if next > 1.0 || next <= t {
            return None;
        }
        t = next;
    }
    None
}
/// Primitive fallback uses the actual box surface, rather than inflated point rays.
pub fn sweep_strike_aabb(
    previous: StrikeCapsule,
    current: StrikeCapsule,
    min: [f32; 3],
    max: [f32; 3],
) -> Option<StrikeContact> {
    if !previous.valid() || !current.valid() {
        return None;
    }
    let (a, b) = previous.bounds(current);
    if (0..3).any(|i| a[i] > max[i] || b[i] < min[i]) {
        return None;
    }
    if [previous.start, previous.end]
        .iter()
        .any(|p| (0..3).all(|i| p[i] >= min[i] && p[i] <= max[i]))
    {
        return Some(StrikeContact {
            fraction: 0.0,
            point: previous.start,
            normal: [0.0, 1.0, 0.0],
        });
    }
    let p: [[f32; 3]; 8] = std::array::from_fn(|n| {
        std::array::from_fn(|i| if n & (1 << i) == 0 { min[i] } else { max[i] })
    });
    let faces = [
        [0, 2, 3],
        [0, 3, 1],
        [4, 5, 7],
        [4, 7, 6],
        [0, 1, 5],
        [0, 5, 4],
        [2, 6, 7],
        [2, 7, 3],
        [0, 4, 6],
        [0, 6, 2],
        [1, 3, 7],
        [1, 7, 5],
    ];
    faces
        .into_iter()
        .filter_map(|f| sweep_strike_triangle(previous, current, f.map(|i| p[i])))
        .min_by(|a, b| a.fraction.total_cmp(&b.fraction))
}
#[cfg(test)]
mod surface_tests {
    use super::*;
    #[test]
    fn rotating_shaft_hits_wall_at_its_middle_without_endpoint_hits() {
        let a = StrikeCapsule {
            start: [-1.0, 0.0, -1.0],
            end: [1.0, 0.0, -1.0],
            radius: 0.03,
        };
        let b = StrikeCapsule {
            start: [-1.0, 0.0, 1.0],
            end: [1.0, 0.0, 1.0],
            radius: 0.03,
        };
        let tri = [[-0.1, -1.0, 0.0], [0.1, -1.0, 0.0], [0.0, 1.0, 0.0]];
        let h = sweep_strike_triangle(a, b, tri).unwrap();
        assert!((h.fraction - 0.485).abs() < 0.0001);
        assert!(h.point[0].abs() < 0.101);
    }
    #[test]
    fn solid_box_and_triangle_gaps_are_distinguished() {
        let a = StrikeCapsule {
            start: [-2.0, 0.0, 0.0],
            end: [-2.0, 0.5, 0.0],
            radius: 0.05,
        };
        let b = StrikeCapsule {
            start: [2.0, 0.0, 0.0],
            end: [2.0, 0.5, 0.0],
            radius: 0.05,
        };
        let h = sweep_strike_aabb(a, b, [-0.1, -1.0, -1.0], [0.1, 1.0, 1.0]).unwrap();
        assert!((h.fraction - 0.4625).abs() < 0.0001);
        assert!(
            sweep_strike_triangle(a, b, [[0.0, 2.0, -1.0], [0.0, 3.0, 1.0], [0.0, 2.0, 1.0]])
                .is_none()
        );
    }
}
