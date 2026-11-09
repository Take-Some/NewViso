use super::*;
#[derive(Clone, Debug, Default, serde::Serialize)]
pub(super) struct PhysicalSkeletonIntegrity {
    constrained_bones: usize,
    subresolution_bones: usize,
    max_bone_length_error: f32,
    max_relative_bone_length_error: f32,
    /// Intrinsic edge length before adding the f32 world translation.
    max_relative_local_bone_length_error: f32,
    max_relative_scale_error: f32,
    max_basis_shear: f32,
}
fn physical_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn physical_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn physical_length(v: [f32; 3]) -> f32 {
    physical_dot(v, v).sqrt()
}
fn physical_axis(m: [f32; 16], column: usize) -> [f32; 3] {
    [m[column * 4], m[column * 4 + 1], m[column * 4 + 2]]
}
fn physical_unit(v: [f32; 3]) -> Result<[f32; 3], String> {
    let n = physical_length(v);
    if !n.is_finite() || n <= f32::EPSILON {
        return Err("singular physical bone rotation".into());
    }
    Ok(v.map(|v| v / n))
}
/// Physics owns orientation. Rest scale and connected local offsets belong to
/// the skeleton; finite socket-solver error must not stretch skinning bones.
fn physical_local(
    candidate: [f32; 16],
    rest: [f32; 16],
    connected: bool,
) -> Result<[f32; 16], String> {
    let x = physical_unit(physical_axis(candidate, 0))?;
    let raw_y = physical_axis(candidate, 1);
    let d = physical_dot(x, raw_y);
    let y = physical_unit(std::array::from_fn(|i| raw_y[i] - d * x[i]))?;
    let handedness = if physical_dot(
        physical_cross(physical_axis(rest, 0), physical_axis(rest, 1)),
        physical_axis(rest, 2),
    ) < 0.0
    {
        -1.0
    } else {
        1.0
    };
    let z = physical_cross(x, y).map(|v| v * handedness);
    let mut result = candidate;
    for (column, axis) in [x, y, z].into_iter().enumerate() {
        let scale = physical_length(physical_axis(rest, column));
        if !scale.is_finite() || scale <= f32::EPSILON {
            return Err("singular captured bone scale".into());
        }
        for row in 0..3 {
            result[column * 4 + row] = axis[row] * scale;
        }
        result[column * 4 + 3] = 0.0;
    }
    if connected {
        result[12..15].copy_from_slice(&rest[12..15]);
    }
    result[15] = 1.0;
    Ok(result)
}
pub(super) fn physical_globals(
    skeleton: &ModelSkeleton,
    frozen: &[[f32; 16]],
    requested: &BTreeMap<usize, [f32; 16]>,
    drivers: &[Option<DerivedJointDriver>],
) -> Result<(Vec<[f32; 16]>, PhysicalSkeletonIntegrity), String> {
    if frozen.len() != skeleton.joints.len() || drivers.len() != frozen.len() {
        return Err("physical skeleton dimensions do not match".into());
    }
    let mut globals = Vec::with_capacity(frozen.len());
    let mut anchored = vec![false; frozen.len()];
    let mut integrity = PhysicalSkeletonIntegrity::default();
    for (index, joint) in skeleton.joints.iter().enumerate() {
        let parent = joint.parent.map(|p| p as usize);
        let connected = parent.is_some_and(|p| anchored[p]);
        let mut helper = false;
        let (global, local) = if let Some(matrix) = requested.get(&index) {
            let candidate = if let Some(p) = parent {
                let inverse = inverse_affine(globals[p])
                    .ok_or_else(|| "singular physical parent transform".to_owned())?;
                mul_mat4(inverse, *matrix)
            } else {
                *matrix
            };
            let local = physical_local(candidate, frozen[index], connected)?;
            anchored[index] = true;
            (parent.map_or(local, |p| mul_mat4(globals[p], local)), local)
        } else if let Some(driver) = drivers[index] {
            helper = true;
            anchored[index] = anchored[driver.joint as usize];
            (
                mul_mat4(globals[driver.joint as usize], driver.bind_offset),
                frozen[index],
            )
        } else {
            anchored[index] = connected;
            (
                parent.map_or(frozen[index], |p| mul_mat4(globals[p], frozen[index])),
                frozen[index],
            )
        };
        if connected && !helper {
            integrity.constrained_bones += 1;
            let p = parent.expect("connected parent");
            let actual =
                physical_length(std::array::from_fn(|i| global[12 + i] - globals[p][12 + i]));
            let expected = physical_length(std::array::from_fn(|row| {
                (0..3)
                    .map(|col| globals[p][col * 4 + row] * frozen[index][12 + col])
                    .sum()
            }));
            let local_length = physical_length(std::array::from_fn(|row| {
                (0..3)
                    .map(|col| globals[p][col * 4 + row] * local[12 + col])
                    .sum()
            }));
            if expected > f32::EPSILON {
                integrity.max_relative_local_bone_length_error = integrity
                    .max_relative_local_bone_length_error
                    .max((local_length - expected).abs() / expected);
            }
            let error = (actual - expected).abs();
            integrity.max_bone_length_error = integrity.max_bone_length_error.max(error);
            // Tiny import offsets on coincident helpers can be below the
            // precision of global translations. Absolute error still covers
            // every edge; only resolved lengths contribute a relative error.
            let precision = 4.0
                * f32::EPSILON
                * (physical_length([global[12], global[13], global[14]])
                    + physical_length([globals[p][12], globals[p][13], globals[p][14]])
                    + expected);
            if expected > precision {
                integrity.max_relative_bone_length_error = integrity
                    .max_relative_bone_length_error
                    .max(error / expected);
            } else {
                integrity.subresolution_bones += 1;
            }
        }
        if requested.contains_key(&index) || (connected && !helper) {
            let axes = [0, 1, 2].map(|i| physical_axis(local, i));
            let scales = axes.map(physical_length);
            for column in 0..3 {
                let expected = physical_length(physical_axis(frozen[index], column));
                if expected > f32::EPSILON {
                    integrity.max_relative_scale_error = integrity
                        .max_relative_scale_error
                        .max((scales[column] / expected - 1.0).abs());
                }
                for other in 0..column {
                    let denominator = scales[column] * scales[other];
                    if denominator > f32::EPSILON {
                        integrity.max_basis_shear = integrity
                            .max_basis_shear
                            .max(physical_dot(axes[column], axes[other]).abs() / denominator);
                    }
                }
            }
        }
        globals.push(global);
    }
    Ok((globals, integrity))
}

#[cfg(test)]
mod tests;
