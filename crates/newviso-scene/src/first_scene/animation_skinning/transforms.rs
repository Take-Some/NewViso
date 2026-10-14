//! Joint, camera and affine transform math.
use super::*;

pub(super) fn matrix_scale(matrix: [f32; 16]) -> Result<[f32; 3], String> {
    let sx = (matrix[0] * matrix[0] + matrix[1] * matrix[1] + matrix[2] * matrix[2]).sqrt();
    let sy = (matrix[4] * matrix[4] + matrix[5] * matrix[5] + matrix[6] * matrix[6]).sqrt();
    let sz = (matrix[8] * matrix[8] + matrix[9] * matrix[9] + matrix[10] * matrix[10]).sqrt();
    if [sx, sy, sz]
        .iter()
        .any(|value| !value.is_finite() || *value <= 1.0e-8)
    {
        return Err("matrix has singular/non-finite scale".to_owned());
    }
    Ok([sx, sy, sz])
}

pub(super) fn joint_camera_animation_angles(
    bind: [f32; 16], animated: [f32; 16],
    bind_root: [f32; 16], animated_root: [f32; 16],
) -> Result<[f32; 3], String> {
    let inverse_bind = inverse_affine(bind)
        .ok_or_else(|| "camera anchor bind transform is singular".to_owned())?;
    let inverse_root = inverse_affine(animated_root)
        .ok_or_else(|| "camera anchor root transform is singular".to_owned())?;
    let delta = mul_mat4(mul_mat4(animated, inverse_bind), mul_mat4(bind_root, inverse_root));
    matrix_scale(delta)?;
    // Camera forward is -Z, up is +Y. Rz*Ry*Rx skeleton Euler angles have an
    // equivalent +/-180 degree pitch and roll around a half-turn heading.
    // Decompose the camera basis instead, keeping pitch in [-90, 90].
    let front = Vec3::new(-delta[8], -delta[9], -delta[10]).normalized();
    let up = Vec3::new(delta[4], delta[5], delta[6]).normalized();
    if !front.x.is_finite() || !front.y.is_finite() || !front.z.is_finite()
        || !up.x.is_finite() || !up.y.is_finite() || !up.z.is_finite() {
        return Err("camera anchor has a non-finite rotation basis".to_owned());
    }
    let pitch = front.y.clamp(-1.0, 1.0).asin();
    let heading = front.x.atan2(-front.z);
    let right = Vec3::new(heading.cos(), 0.0, heading.sin());
    let neutral_up = right.cross(front).normalized();
    let roll = (-up.dot(right)).atan2(up.dot(neutral_up));
    Ok([pitch.to_degrees(), heading.to_degrees(), roll.to_degrees()])
}

pub(super) fn matrix_rotation_degrees(matrix: [f32; 16]) -> Result<[f32; 3], String> {
    let mut x = [matrix[0], matrix[1], matrix[2]];
    let mut y = [matrix[4], matrix[5], matrix[6]];
    let mut z = [matrix[8], matrix[9], matrix[10]];
    for axis in [&mut x, &mut y, &mut z] {
        let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        if !length.is_finite() || length <= 1.0e-8 {
            return Err("joint world matrix has singular rotation basis".to_owned());
        }
        axis[0] /= length;
        axis[1] /= length;
        axis[2] /= length;
    }

    // Matrix convention is Rz * Ry * Rx, matching transform_point / instance_model_matrix.
    let sy = (-x[2]).clamp(-1.0, 1.0);
    let ry = sy.asin();
    let cy = ry.cos();
    let (rx, rz) = if cy.abs() > 1.0e-5 {
        (y[2].atan2(z[2]), x[1].atan2(x[0]))
    } else {
        ((-z[1]).atan2(y[1]), 0.0)
    };
    Ok([rx.to_degrees(), ry.to_degrees(), rz.to_degrees()])
}

pub(super) fn trs_matrix(translation: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) -> [f32; 16] {
    let [x, y, z, w] = normalize_quat(rotation);
    let xx = x * x;
    let yy = y * y;
    let zz = z * z;
    let xy = x * y;
    let xz = x * z;
    let yz = y * z;
    let wx = w * x;
    let wy = w * y;
    let wz = w * z;
    [
        (1.0 - 2.0 * (yy + zz)) * scale[0],
        (2.0 * (xy + wz)) * scale[0],
        (2.0 * (xz - wy)) * scale[0],
        0.0,
        (2.0 * (xy - wz)) * scale[1],
        (1.0 - 2.0 * (xx + zz)) * scale[1],
        (2.0 * (yz + wx)) * scale[1],
        0.0,
        (2.0 * (xz + wy)) * scale[2],
        (2.0 * (yz - wx)) * scale[2],
        (1.0 - 2.0 * (xx + yy)) * scale[2],
        0.0,
        translation[0],
        translation[1],
        translation[2],
        1.0,
    ]
}

pub(super) fn mul_mat4(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[column * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum();
        }
    }
    out
}

pub(super) fn transform_point_mat4(matrix: [f32; 16], point: Vec3) -> Vec3 {
    Vec3::new(
        matrix[0] * point.x + matrix[4] * point.y + matrix[8] * point.z + matrix[12],
        matrix[1] * point.x + matrix[5] * point.y + matrix[9] * point.z + matrix[13],
        matrix[2] * point.x + matrix[6] * point.y + matrix[10] * point.z + matrix[14],
    )
}

pub(super) fn transform_vector_mat4(matrix: [f32; 16], vector: Vec3) -> Vec3 {
    Vec3::new(
        matrix[0] * vector.x + matrix[4] * vector.y + matrix[8] * vector.z,
        matrix[1] * vector.x + matrix[5] * vector.y + matrix[9] * vector.z,
        matrix[2] * vector.x + matrix[6] * vector.y + matrix[10] * vector.z,
    )
}

pub(super) fn inverse_affine(matrix: [f32; 16]) -> Option<[f32; 16]> {
    let a00 = matrix[0];
    let a01 = matrix[4];
    let a02 = matrix[8];
    let a10 = matrix[1];
    let a11 = matrix[5];
    let a12 = matrix[9];
    let a20 = matrix[2];
    let a21 = matrix[6];
    let a22 = matrix[10];

    let c00 = a11 * a22 - a12 * a21;
    let c01 = a12 * a20 - a10 * a22;
    let c02 = a10 * a21 - a11 * a20;
    let det = a00 * c00 + a01 * c01 + a02 * c02;
    if !det.is_finite() || det.abs() < 1.0e-10 {
        return None;
    }
    let inv_det = 1.0 / det;
    let i00 = c00 * inv_det;
    let i01 = (a02 * a21 - a01 * a22) * inv_det;
    let i02 = (a01 * a12 - a02 * a11) * inv_det;
    let i10 = c01 * inv_det;
    let i11 = (a00 * a22 - a02 * a20) * inv_det;
    let i12 = (a02 * a10 - a00 * a12) * inv_det;
    let i20 = c02 * inv_det;
    let i21 = (a01 * a20 - a00 * a21) * inv_det;
    let i22 = (a00 * a11 - a01 * a10) * inv_det;
    let t = [matrix[12], matrix[13], matrix[14]];
    let it = [
        -(i00 * t[0] + i01 * t[1] + i02 * t[2]),
        -(i10 * t[0] + i11 * t[1] + i12 * t[2]),
        -(i20 * t[0] + i21 * t[1] + i22 * t[2]),
    ];
    Some([
        i00, i10, i20, 0.0, i01, i11, i21, 0.0, i02, i12, i22, 0.0, it[0], it[1], it[2], 1.0,
    ])
}
