pub(super) fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
pub(super) fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(super) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(super) fn length(v: [f32; 3]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}
pub(super) fn inverse(q: [f32; 4]) -> [f32; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}
pub(super) fn normalize(q: [f32; 4]) -> [f32; 4] {
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-8);
    q.map(|v| v / n)
}
pub(super) fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    normalize([
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ])
}
pub(super) fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let t = cross([q[0], q[1], q[2]], v).map(|x| x * 2.0);
    add(v, add(t.map(|x| x * q[3]), cross([q[0], q[1], q[2]], t)))
}
pub(super) fn from_axis(axis: [f32; 3]) -> [f32; 4] {
    let n = length(axis);
    if n < 1e-5 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let a = axis.map(|v| v / n);
    if a[1] < -0.9999 {
        return [1.0, 0.0, 0.0, 0.0];
    }
    normalize([a[2], 0.0, -a[0], 1.0 + a[1]])
}
pub(super) fn matrix(position: [f32; 3], q: [f32; 4]) -> [f32; 16] {
    let q = normalize(q);
    let [x, y, z, w] = q;
    [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + w * z),
        2.0 * (x * z - w * y),
        0.0,
        2.0 * (x * y - w * z),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + w * x),
        0.0,
        2.0 * (x * z + w * y),
        2.0 * (y * z - w * x),
        1.0 - 2.0 * (x * x + y * y),
        0.0,
        position[0],
        position[1],
        position[2],
        1.0,
    ]
}
pub(super) fn product(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    std::array::from_fn(|index| {
        let row = index % 4;
        let col = index / 4;
        (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum()
    })
}
pub(super) fn inverse_pose(position: [f32; 3], rotation: [f32; 4]) -> [f32; 16] {
    let q = inverse(rotation);
    matrix(rotate(q, position.map(|v| -v)), q)
}
pub(super) fn position(m: [f32; 16]) -> [f32; 3] {
    [m[12], m[13], m[14]]
}
pub(super) fn rotation(m: [f32; 16]) -> [f32; 4] {
    let mut r = [[0.0; 3]; 3];
    for col in 0..3 {
        let n = length([m[col * 4], m[col * 4 + 1], m[col * 4 + 2]]).max(1e-8);
        for row in 0..3 {
            r[row][col] = m[col * 4 + row] / n;
        }
    }
    let trace = r[0][0] + r[1][1] + r[2][2];
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [
            (r[2][1] - r[1][2]) / s,
            (r[0][2] - r[2][0]) / s,
            (r[1][0] - r[0][1]) / s,
            s * 0.25,
        ]
    } else if r[0][0] > r[1][1] && r[0][0] > r[2][2] {
        let s = (1.0 + r[0][0] - r[1][1] - r[2][2]).sqrt() * 2.0;
        [
            s * 0.25,
            (r[0][1] + r[1][0]) / s,
            (r[0][2] + r[2][0]) / s,
            (r[2][1] - r[1][2]) / s,
        ]
    } else if r[1][1] > r[2][2] {
        let s = (1.0 + r[1][1] - r[0][0] - r[2][2]).sqrt() * 2.0;
        [
            (r[0][1] + r[1][0]) / s,
            s * 0.25,
            (r[1][2] + r[2][1]) / s,
            (r[0][2] - r[2][0]) / s,
        ]
    } else {
        let s = (1.0 + r[2][2] - r[0][0] - r[1][1]).sqrt() * 2.0;
        [
            (r[0][2] + r[2][0]) / s,
            (r[1][2] + r[2][1]) / s,
            s * 0.25,
            (r[1][0] - r[0][1]) / s,
        ]
    };
    normalize(q)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bone_body_rotation_roundtrip_preserves_all_principal_axes() {
        for axis in [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, -1.0],
        ] {
            let q = from_axis(axis);
            let actual = rotate(rotation(matrix([0.0; 3], q)), [0.0, 1.0, 0.0]);
            assert!(length(sub(actual, axis)) < 1e-5);
            let m = product(
                matrix([3.0, 2.0, -1.0], q),
                inverse_pose([3.0, 2.0, -1.0], q),
            );
            assert!(length(position(m)) < 1e-5);
            assert!(
                (m[0] - 1.0).abs() < 1e-5
                    && (m[5] - 1.0).abs() < 1e-5
                    && (m[10] - 1.0).abs() < 1e-5
            );
        }
    }
}
