pub type V3 = [f64; 3];
pub fn add(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] + b[i])
}
pub fn sub(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] - b[i])
}
pub fn mul(a: V3, k: f64) -> V3 {
    a.map(|v| v * k)
}
pub fn dot(a: V3, b: V3) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn length(a: V3) -> f64 {
    dot(a, a).sqrt()
}
pub fn normal(a: V3) -> V3 {
    let n = length(a);
    if n > 1e-12 {
        mul(a, 1.0 / n)
    } else {
        [0.0; 3]
    }
}
pub fn axis(i: usize) -> V3 {
    std::array::from_fn(|k| if i == k { 1.0 } else { 0.0 })
}
pub fn rotate(v: V3, euler: V3) -> V3 {
    let [x, y, z] = euler.map(f64::to_radians);
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    let v = [v[0], v[1] * cx - v[2] * sx, v[1] * sx + v[2] * cx];
    let v = [v[0] * cy + v[2] * sy, v[1], -v[0] * sy + v[2] * cy];
    [v[0] * cz - v[1] * sz, v[0] * sz + v[1] * cz, v[2]]
}
pub fn unrotate(v: V3, e: V3) -> V3 {
    std::array::from_fn(|i| dot(v, rotate(axis(i), e)))
}
pub fn rotate_axis(v: V3, a: V3, radians: f64) -> V3 {
    let (s, c) = radians.sin_cos();
    add(
        add(mul(v, c), mul(cross(a, v), s)),
        mul(a, dot(a, v) * (1.0 - c)),
    )
}
pub fn compose_rotation(euler: V3, a: V3, degrees: f64) -> V3 {
    let columns: [V3; 3] =
        std::array::from_fn(|i| rotate_axis(rotate(axis(i), euler), a, degrees.to_radians()));
    let pitch = (-columns[0][2]).clamp(-1.0, 1.0).asin();
    let (roll, yaw) = if pitch.cos().abs() > 1e-7 {
        (
            columns[1][2].atan2(columns[2][2]),
            columns[0][1].atan2(columns[0][0]),
        )
    } else {
        ((-columns[2][1]).atan2(columns[1][1]), 0.0)
    };
    [roll.to_degrees(), pitch.to_degrees(), yaw.to_degrees()]
}
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: V3,
    pub direction: V3,
}
impl Ray {
    pub fn at(self, t: f64) -> V3 {
        add(self.origin, mul(self.direction, t))
    }
    pub fn plane(self, point: V3, normal: V3) -> Option<V3> {
        let denom = dot(self.direction, normal);
        if denom.abs() < 1e-8 {
            return None;
        }
        let t = dot(sub(point, self.origin), normal) / denom;
        (t >= 0.0 && t.is_finite()).then(|| self.at(t))
    }
    pub fn axis_parameter(self, point: V3, axis: V3) -> Option<f64> {
        let d = dot(self.direction, axis);
        let denom = 1.0 - d * d;
        if denom < 1e-6 {
            return None;
        }
        let w = sub(self.origin, point);
        Some((dot(w, axis) - d * dot(w, self.direction)) / denom)
    }
    pub fn bounds(self, min: V3, max: V3) -> Option<f64> {
        let mut lo: f64 = 0.0;
        let mut hi = f64::INFINITY;
        for i in 0..3 {
            if self.direction[i].abs() < 1e-12 {
                if self.origin[i] < min[i] || self.origin[i] > max[i] {
                    return None;
                }
            } else {
                let a = (min[i] - self.origin[i]) / self.direction[i];
                let b = (max[i] - self.origin[i]) / self.direction[i];
                lo = lo.max(a.min(b));
                hi = hi.min(a.max(b));
                if hi < lo {
                    return None;
                }
            }
        }
        Some(lo)
    }
}
pub fn segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let denom = d[0] * d[0] + d[1] * d[1];
    let t = if denom > 1e-9 {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1])
}
