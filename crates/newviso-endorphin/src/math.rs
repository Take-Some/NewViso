pub fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
pub fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn length(v: [f32; 3]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}
pub fn inverse(q: [f32; 4]) -> [f32; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}
pub fn normalize(q: [f32; 4]) -> [f32; 4] {
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-8);
    q.map(|v| v / n)
}
pub fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    normalize([
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ])
}
pub fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let t = cross([q[0], q[1], q[2]], v).map(|x| x * 2.0);
    add(v, add(t.map(|x| x * q[3]), cross([q[0], q[1], q[2]], t)))
}
pub fn from_axis(axis: [f32; 3]) -> [f32; 4] {
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
pub fn matrix(position: [f32; 3], q: [f32; 4]) -> [f32; 16] {
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
pub fn product(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    std::array::from_fn(|index| {
        let row = index % 4;
        let col = index / 4;
        (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum()
    })
}
pub fn inverse_pose(position: [f32; 3], rotation: [f32; 4]) -> [f32; 16] {
    let q = inverse(rotation);
    matrix(rotate(q, position.map(|v| -v)), q)
}
pub fn position(m: [f32; 16]) -> [f32; 3] {
    [m[12], m[13], m[14]]
}
pub fn rotation(m: [f32; 16]) -> [f32; 4] {
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

pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
pub fn capsule_inertia(mass: f32, radius: f32, half_height: f32) -> [f32; 3] {
    let cylinder = 2.0 * half_height;
    let cylinder_mass = mass * cylinder / (cylinder + 4.0 * radius / 3.0);
    let sphere_mass = mass - cylinder_mass;
    let axial = cylinder_mass * radius * radius * 0.5 + sphere_mass * radius * radius * 0.4;
    let transverse = cylinder_mass * (3.0 * radius * radius + cylinder * cylinder) / 12.0
        + sphere_mass
            * (0.4 * radius * radius + half_height * half_height + 0.75 * half_height * radius);
    [transverse, axial, transverse]
}
pub fn tensor(rotation: [f32; 4], diagonal: [f32; 3]) -> [[f32; 3]; 3] {
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|v| rotate(rotation, v));
    std::array::from_fn(|i| {
        std::array::from_fn(|j| (0..3).map(|k| axes[k][i] * diagonal[k] * axes[k][j]).sum())
    })
}
pub fn tensor_vector(a: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    a.map(|r| dot(r, v))
}
pub fn solve_symmetric(a: [[f32; 3]; 3], v: [f32; 3]) -> Option<[f32; 3]> {
    let [x, y, z] = a;
    let c = [cross(y, z), cross(z, x), cross(x, y)];
    let determinant = dot(x, c[0]);
    if !determinant.is_finite() || determinant <= f32::MIN_POSITIVE {
        return None;
    }
    let result = std::array::from_fn(|i| (0..3).map(|j| c[j][i] * v[j]).sum::<f32>() / determinant);
    result.iter().all(|v| v.is_finite()).then_some(result)
}

pub fn axis_angle(v: [f32; 3]) -> [f32; 4] {
    let a = length(v);
    if a <= f32::EPSILON {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let k = (a * 0.5).sin() / a;
    normalize([v[0] * k, v[1] * k, v[2] * k, (a * 0.5).cos()])
}
pub fn swing_twist(a: [f32; 3]) -> [f32; 4] {
    multiply(axis_angle([a[0], 0.0, a[2]]), axis_angle([0.0, a[1], 0.0]))
}
pub fn slerp(a: [f32; 4], mut b: [f32; 4], t: f32) -> [f32; 4] {
    let mut d = (0..4).map(|i| a[i] * b[i]).sum::<f32>();
    if d < 0.0 {
        b = b.map(|v| -v);
        d = -d;
    }
    if d > 0.9995 {
        return normalize(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t));
    }
    let angle = d.clamp(-1.0, 1.0).acos();
    let x = ((1.0 - t) * angle).sin() / angle.sin();
    let y = (t * angle).sin() / angle.sin();
    normalize(std::array::from_fn(|i| a[i] * x + b[i] * y))
}
pub fn unit(v: [f32; 3]) -> Result<[f32; 3], String> {
    let n = length(v);
    if !n.is_finite() || n <= f32::EPSILON {
        return Err("degenerate anatomical reference axis".into());
    }
    Ok(v.map(|v| v / n))
}
pub fn anatomical_frame(
    axis: [f32; 3],
    forward: [f32; 3],
    right: [f32; 3],
) -> Result<[f32; 4], String> {
    let y = unit(axis)?;
    let cross_forward = cross(y, forward);
    let x = if length(cross_forward) > 1e-5 {
        unit(cross_forward)?
    } else {
        unit(sub(right, y.map(|v| v * dot(right, y))))?
    };
    let z = unit(cross(x, y))?;
    let mut m = matrix([0.0; 3], [0.0, 0.0, 0.0, 1.0]);
    for (i, v) in [x, y, z].iter().enumerate() {
        for j in 0..3 {
            m[i * 4 + j] = v[j];
        }
    }
    Ok(rotation(m))
}

pub fn align_axis(from: [f32; 3], to: [f32; 3]) -> [f32; 4] {
    let Ok(from) = unit(from) else {
        return [0.0, 0.0, 0.0, 1.0];
    };
    let Ok(to) = unit(to) else {
        return [0.0, 0.0, 0.0, 1.0];
    };
    let c = cross(from, to);
    let w = 1.0 + dot(from, to);
    if w > 1e-6 {
        normalize([c[0], c[1], c[2], w])
    } else {
        let basis = if from[0].abs() < from[1].abs() {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        axis_angle(
            unit(cross(from, basis))
                .unwrap_or([0.0, 0.0, 1.0])
                .map(|v| v * std::f32::consts::PI),
        )
    }
}
#[cfg(test)]
mod align_tests {
    use super::*;
    #[test]
    fn surface_alignment_preserves_rotation_and_handles_opposing_normals() {
        for (a, b) in [
            ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            ([0.0, -1.0, 0.0], [0.0, 1.0, 0.0]),
            ([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]),
        ] {
            let q = align_axis(a, b);
            assert!(length(sub(rotate(q, a), b)) < 1e-5);
            assert!((q.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() < 1e-6);
        }
    }
}
