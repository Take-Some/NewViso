pub type V3 = [f32; 3];
pub fn dot(a: V3, b: V3) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
pub fn sub(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] - b[i])
}
pub fn add(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] + b[i])
}
pub fn mul(a: V3, s: f32) -> V3 {
    a.map(|x| x * s)
}
pub fn rotate(q: [f32; 4], v: V3) -> V3 {
    let [x, y, z, w] = q;
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
pub fn quaternion_product(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [x, y, z, w] = a;
    let [i, j, k, r] = b;
    [
        w * i + x * r + y * k - z * j,
        w * j - x * k + y * r + z * i,
        w * k + x * j - y * i + z * r,
        w * r - x * i - y * j - z * k,
    ]
}
pub fn twist(closed: [f32; 4], current: [f32; 4], frame: [f32; 4]) -> f32 {
    let inv = |q: [f32; 4]| [-q[0], -q[1], -q[2], q[3]];
    let q = quaternion_product(
        inv(frame),
        quaternion_product(quaternion_product(inv(closed), current), frame),
    );
    let sign = if q[3] < 0.0 { -1.0 } else { 1.0 };
    2.0 * (q[1] * sign).atan2(q[3] * sign)
}
/// Finite horizontal segment separation, including endpoints and parallel segments.
pub fn segment_distance(a: V3, b: V3, c: V3, d: V3) -> f32 {
    let cross = |u: V3, v: V3| u[0] * v[2] - u[2] * v[0];
    let ab = sub(b, a);
    let cd = sub(d, c);
    let ac = sub(c, a);
    let det = cross(ab, cd);
    if det.abs() > 1e-7 {
        let t = cross(ac, cd) / det;
        let u = cross(ac, ab) / det;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
            return 0.0;
        }
    }
    let point = |p: V3, x: V3, y: V3| {
        let v = sub(y, x);
        let t = if v[0] * v[0] + v[2] * v[2] > 0.0 {
            ((p[0] - x[0]) * v[0] + (p[2] - x[2]) * v[2]) / (v[0] * v[0] + v[2] * v[2])
        } else {
            0.0
        };
        let q = add(x, mul(v, t.clamp(0.0, 1.0)));
        (p[0] - q[0]).hypot(p[2] - q[2])
    };
    point(a, c, d)
        .min(point(b, c, d))
        .min(point(c, a, b))
        .min(point(d, a, b))
}
