use crate::types::{Quat, Vec3};

pub(crate) const EPSILON: f32 = 1.0e-5;
pub(crate) const WORLD_UP: Vec3 = [0.0, 1.0, 0.0];

pub(crate) fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn mul(v: Vec3, scalar: f32) -> Vec3 {
    [v[0] * scalar, v[1] * scalar, v[2] * scalar]
}

pub(crate) fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn length(v: Vec3) -> f32 {
    dot(v, v).sqrt()
}

pub(crate) fn normalize(v: Vec3) -> Vec3 {
    let len = length(v);
    if len <= EPSILON || !len.is_finite() {
        [0.0; 3]
    } else {
        mul(v, 1.0 / len)
    }
}

pub(crate) fn normalize_or(v: Vec3, fallback: Vec3) -> Vec3 {
    let result = normalize(v);
    if length(result) <= EPSILON {
        normalize(fallback)
    } else {
        result
    }
}

pub(crate) fn rotate_vec(q: Quat, v: Vec3) -> Vec3 {
    let q = normalize_quat(q);
    let u = [q[0], q[1], q[2]];
    let s = q[3];
    add(
        add(mul(u, 2.0 * dot(u, v)), mul(v, s * s - dot(u, u))),
        mul(cross(u, v), 2.0 * s),
    )
}

fn normalize_quat(q: Quat) -> Quat {
    let len = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if len <= EPSILON || !len.is_finite() {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        q.map(|v| v / len)
    }
}

pub(crate) fn project_on_plane(value: Vec3, normal: Vec3) -> Vec3 {
    sub(value, mul(normal, dot(value, normal)))
}

pub(crate) fn rotate_around_axis(value: Vec3, axis: Vec3, angle: f32) -> Vec3 {
    let axis = normalize_or(axis, WORLD_UP);
    let (sin, cos) = angle.sin_cos();
    add(
        add(mul(value, cos), mul(cross(axis, value), sin)),
        mul(axis, dot(axis, value) * (1.0 - cos)),
    )
}

pub(crate) fn wrap_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

pub(crate) fn traction_coefficient(
    slip: f32,
    peak_slip: f32,
    end_slip: f32,
    maximum: f32,
    minimum: f32,
) -> f32 {
    let slip = slip.abs();
    let peak = peak_slip.max(EPSILON);
    let end = end_slip.max(peak + EPSILON);
    if slip < peak {
        maximum * (slip / peak)
    } else if slip < end {
        maximum + (minimum - maximum) * ((slip - peak) / (end - peak))
    } else {
        minimum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_quaternion_keeps_vector() {
        assert_eq!(
            rotate_vec([0.0, 0.0, 0.0, 1.0], [1.0, 2.0, 3.0]),
            [1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn tyre_curve_reaches_peak_and_falls_to_minimum() {
        let before = traction_coefficient(0.05, 0.1, 0.4, 2.5, 2.0);
        let peak = traction_coefficient(0.1, 0.1, 0.4, 2.5, 2.0);
        let after = traction_coefficient(0.3, 0.1, 0.4, 2.5, 2.0);
        let far = traction_coefficient(1.0, 0.1, 0.4, 2.5, 2.0);
        assert!(before < peak);
        assert!(after < peak);
        assert!((far - 2.0).abs() < 1.0e-6);
    }
}
