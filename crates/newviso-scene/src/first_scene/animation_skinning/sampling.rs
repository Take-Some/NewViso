//! Clip sampling, palette blending and CPU vertex skinning.
use super::*;

pub(super) fn advance_clip_time(clip: &ModelAnimationClip, time_seconds: f32, delta_seconds: f32) -> f32 {
    let duration = clip.duration_seconds.max(1.0e-6);
    let next = time_seconds + delta_seconds.max(0.0);
    if clip.looping {
        next.rem_euclid(duration)
    } else {
        next.min(duration)
    }
}

pub(super) fn sample_clip_local_pose(
    skeleton: &ModelSkeleton,
    clip: &ModelAnimationClip,
    time_seconds: f32,
) -> Result<LocalPose, String> {
    let mut translations = skeleton
        .joints
        .iter()
        .map(|joint| joint.bind_translation)
        .collect::<Vec<_>>();
    let mut rotations = skeleton
        .joints
        .iter()
        .map(|joint| joint.bind_rotation)
        .collect::<Vec<_>>();
    let mut scales = skeleton
        .joints
        .iter()
        .map(|joint| joint.bind_scale)
        .collect::<Vec<_>>();
    let mut tracked = vec![false; skeleton.joints.len()];

    for track in &clip.tracks {
        let joint = track.joint as usize;
        if joint >= skeleton.joints.len() {
            return Err(format!(
                "animation clip '{}' references joint={} skeleton_joints={}",
                clip.name,
                joint,
                skeleton.joints.len()
            ));
        }
        tracked[joint] = true;
        translations[joint] = sample_vec3_clip(
            &track.translations,
            track.translation_interpolation,
            time_seconds,
            clip.looping,
            clip.duration_seconds,
            translations[joint],
        );
        rotations[joint] = sample_quat_clip(
            &track.rotations,
            track.rotation_interpolation,
            time_seconds,
            clip.looping,
            clip.duration_seconds,
            rotations[joint],
        );
        scales[joint] = sample_vec3_clip(
            &track.scales,
            track.scale_interpolation,
            time_seconds,
            clip.looping,
            clip.duration_seconds,
            scales[joint],
        );
    }

    Ok((translations, rotations, scales, tracked))
}

pub(super) fn build_palette_from_local_pose(
    skeleton: &ModelSkeleton,
    derived_joint_drivers: &[Option<DerivedJointDriver>],
    translations: &[[f32; 3]],
    rotations: &[[f32; 4]],
    scales: &[[f32; 3]],
    tracked: &[bool],
    source_to_model: [f32; 16],
    model_to_source: [f32; 16],
) -> Result<Vec<[f32; 16]>, String> {
    if derived_joint_drivers.len() != skeleton.joints.len()
        || translations.len() != skeleton.joints.len()
        || rotations.len() != skeleton.joints.len()
        || scales.len() != skeleton.joints.len()
        || tracked.len() != skeleton.joints.len()
    {
        return Err(format!(
            "skeleton '{}' animation pose dimensions do not match joints={}",
            skeleton.name,
            skeleton.joints.len()
        ));
    }

    let mut globals = Vec::with_capacity(skeleton.joints.len());
    let mut palette = Vec::with_capacity(skeleton.joints.len());
    for (index, joint) in skeleton.joints.iter().enumerate() {
        let local = trs_matrix(translations[index], rotations[index], scales[index]);
        let global = if !tracked[index] {
            if let Some(driver) = derived_joint_drivers[index] {
                let driver_index = driver.joint as usize;
                if driver_index >= index {
                    return Err(format!(
                        "skeleton '{}' derived helper joint={} has non-topological driver={}",
                        skeleton.name, index, driver_index
                    ));
                }
                mul_mat4(globals[driver_index], driver.bind_offset)
            } else {
                match joint.parent {
                    Some(parent) => {
                        let parent = parent as usize;
                        if parent >= index {
                            return Err(format!(
                                "skeleton '{}' joint={} has non-topological parent={}",
                                skeleton.name, index, parent
                            ));
                        }
                        mul_mat4(globals[parent], local)
                    }
                    None => local,
                }
            }
        } else {
            match joint.parent {
                Some(parent) => {
                    let parent = parent as usize;
                    if parent >= index {
                        return Err(format!(
                            "skeleton '{}' joint={} has non-topological parent={}",
                            skeleton.name, index, parent
                        ));
                    }
                    mul_mat4(globals[parent], local)
                }
                None => local,
            }
        };
        globals.push(global);
        let source_palette = mul_mat4(global, joint.inverse_bind_matrix);
        palette.push(mul_mat4(
            mul_mat4(source_to_model, source_palette),
            model_to_source,
        ));
    }
    Ok(palette)
}

pub(super) fn build_skin_palette(
    skeleton: &ModelSkeleton,
    derived_joint_drivers: &[Option<DerivedJointDriver>],
    clip: &ModelAnimationClip,
    time_seconds: f32,
    source_to_model: [f32; 16],
    model_to_source: [f32; 16],
) -> Result<Vec<[f32; 16]>, String> {
    let (translations, rotations, scales, tracked) =
        sample_clip_local_pose(skeleton, clip, time_seconds)?;
    build_palette_from_local_pose(
        skeleton,
        derived_joint_drivers,
        &translations,
        &rotations,
        &scales,
        &tracked,
        source_to_model,
        model_to_source,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_blended_skin_palette(
    skeleton: &ModelSkeleton,
    derived_joint_drivers: &[Option<DerivedJointDriver>],
    from_clip: &ModelAnimationClip,
    from_time_seconds: f32,
    to_clip: &ModelAnimationClip,
    to_time_seconds: f32,
    alpha: f32,
    source_to_model: [f32; 16],
    model_to_source: [f32; 16],
) -> Result<Vec<[f32; 16]>, String> {
    let (from_t, from_r, from_s, from_tracked) =
        sample_clip_local_pose(skeleton, from_clip, from_time_seconds)?;
    let (to_t, to_r, to_s, to_tracked) =
        sample_clip_local_pose(skeleton, to_clip, to_time_seconds)?;
    let alpha = alpha.clamp(0.0, 1.0);

    let mut translations = Vec::with_capacity(skeleton.joints.len());
    let mut rotations = Vec::with_capacity(skeleton.joints.len());
    let mut scales = Vec::with_capacity(skeleton.joints.len());
    let mut tracked = Vec::with_capacity(skeleton.joints.len());
    for joint in 0..skeleton.joints.len() {
        translations.push(lerp_vec3(from_t[joint], to_t[joint], alpha));
        rotations.push(slerp_quat(
            normalize_quat(from_r[joint]),
            normalize_quat(to_r[joint]),
            alpha,
        ));
        scales.push(lerp_vec3(from_s[joint], to_s[joint], alpha));
        tracked.push(from_tracked[joint] || to_tracked[joint]);
    }

    build_palette_from_local_pose(
        skeleton,
        derived_joint_drivers,
        &translations,
        &rotations,
        &scales,
        &tracked,
        source_to_model,
        model_to_source,
    )
}

pub(super) fn lerp_vec3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

pub(super) fn skin_vertices_to_packed(
    bind_vertices: &[AssetTriangleVertex],
    palette: &[[f32; 16]],
) -> Result<Vec<f32>, String> {
    let mut packed = Vec::with_capacity(bind_vertices.len().saturating_mul(FLOATS_PER_VERTEX));
    for vertex in bind_vertices.iter().copied() {
        let mut skinned = vertex;
        let count = vertex.skin_influences as usize;
        if count != 0 {
            let mut position = Vec3::ZERO;
            let mut normal = Vec3::ZERO;
            let tangent_bind = Vec3::new(vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]);
            let mut tangent = Vec3::ZERO;
            for lane in 0..count {
                let joint = vertex.joints[lane] as usize;
                let weight = vertex.weights[lane];
                let matrix = palette.get(joint).ok_or_else(|| {
                    format!(
                        "skin influence references joint={} palette_joints={}",
                        joint,
                        palette.len()
                    )
                })?;
                position = position.add(transform_point_mat4(*matrix, vertex.position).mul(weight));
                normal = normal.add(transform_vector_mat4(*matrix, vertex.normal).mul(weight));
                tangent = tangent.add(transform_vector_mat4(*matrix, tangent_bind).mul(weight));
            }
            skinned.position = position;
            skinned.normal = normal.normalized();
            let tangent = tangent
                .sub(skinned.normal.mul(tangent.dot(skinned.normal)))
                .normalized();
            skinned.tangent[0] = tangent.x;
            skinned.tangent[1] = tangent.y;
            skinned.tangent[2] = tangent.z;
        }

        geometry::append_local_asset_vertex(
            &mut packed,
            skinned.position,
            skinned.normal,
            skinned.tangent,
            skinned.color,
            skinned.uv,
        );
    }
    Ok(packed)
}

pub(super) fn sample_vec3_clip(
    keys: &[AnimationVec3Key],
    interpolation: AnimationInterpolation,
    time: f32,
    looping: bool,
    duration_seconds: f32,
    fallback: [f32; 3],
) -> [f32; 3] {
    if !looping || keys.len() < 2 || duration_seconds <= 1.0e-8 {
        return sample_vec3(keys, interpolation, time, fallback);
    }

    let time = time.rem_euclid(duration_seconds);
    let first = keys[0];
    let last = keys[keys.len() - 1];
    if time > last.time_seconds && last.time_seconds < duration_seconds {
        if interpolation == AnimationInterpolation::Step {
            return last.value;
        }
        let seam_span = (duration_seconds - last.time_seconds + first.time_seconds).max(1.0e-8);
        let t = ((time - last.time_seconds) / seam_span).clamp(0.0, 1.0);
        return lerp_vec3(last.value, first.value, t);
    }

    if time < first.time_seconds {
        if interpolation == AnimationInterpolation::Step {
            return last.value;
        }
        let previous_time = last.time_seconds - duration_seconds;
        let span = (first.time_seconds - previous_time).max(1.0e-8);
        let t = ((time - previous_time) / span).clamp(0.0, 1.0);
        return lerp_vec3(last.value, first.value, t);
    }

    sample_vec3(keys, interpolation, time, fallback)
}

pub(super) fn sample_quat_clip(
    keys: &[AnimationQuatKey],
    interpolation: AnimationInterpolation,
    time: f32,
    looping: bool,
    duration_seconds: f32,
    fallback: [f32; 4],
) -> [f32; 4] {
    if !looping || keys.len() < 2 || duration_seconds <= 1.0e-8 {
        return sample_quat(keys, interpolation, time, fallback);
    }

    let time = time.rem_euclid(duration_seconds);
    let first = keys[0];
    let last = keys[keys.len() - 1];
    if time > last.time_seconds && last.time_seconds < duration_seconds {
        if interpolation == AnimationInterpolation::Step {
            return normalize_quat(last.value);
        }
        let seam_span = (duration_seconds - last.time_seconds + first.time_seconds).max(1.0e-8);
        let t = ((time - last.time_seconds) / seam_span).clamp(0.0, 1.0);
        return slerp_quat(normalize_quat(last.value), normalize_quat(first.value), t);
    }

    if time < first.time_seconds {
        if interpolation == AnimationInterpolation::Step {
            return normalize_quat(last.value);
        }
        let previous_time = last.time_seconds - duration_seconds;
        let span = (first.time_seconds - previous_time).max(1.0e-8);
        let t = ((time - previous_time) / span).clamp(0.0, 1.0);
        return slerp_quat(normalize_quat(last.value), normalize_quat(first.value), t);
    }

    sample_quat(keys, interpolation, time, fallback)
}

pub(super) fn sample_vec3(
    keys: &[AnimationVec3Key],
    interpolation: AnimationInterpolation,
    time: f32,
    fallback: [f32; 3],
) -> [f32; 3] {
    let Some(first) = keys.first() else {
        return fallback;
    };
    if keys.len() == 1 || time <= first.time_seconds {
        return first.value;
    }
    let upper = keys.partition_point(|key| key.time_seconds <= time);
    if upper >= keys.len() {
        return keys.last().map_or(fallback, |key| key.value);
    }
    let a = keys[upper - 1];
    let b = keys[upper];
    if interpolation == AnimationInterpolation::Step {
        return a.value;
    }
    let span = (b.time_seconds - a.time_seconds).max(1.0e-8);
    let t = ((time - a.time_seconds) / span).clamp(0.0, 1.0);
    [
        a.value[0] + (b.value[0] - a.value[0]) * t,
        a.value[1] + (b.value[1] - a.value[1]) * t,
        a.value[2] + (b.value[2] - a.value[2]) * t,
    ]
}

pub(super) fn sample_quat(
    keys: &[AnimationQuatKey],
    interpolation: AnimationInterpolation,
    time: f32,
    fallback: [f32; 4],
) -> [f32; 4] {
    let Some(first) = keys.first() else {
        return normalize_quat(fallback);
    };
    if keys.len() == 1 || time <= first.time_seconds {
        return normalize_quat(first.value);
    }
    let upper = keys.partition_point(|key| key.time_seconds <= time);
    if upper >= keys.len() {
        return normalize_quat(keys.last().map_or(fallback, |key| key.value));
    }
    let a = keys[upper - 1];
    let b = keys[upper];
    if interpolation == AnimationInterpolation::Step {
        return normalize_quat(a.value);
    }
    let span = (b.time_seconds - a.time_seconds).max(1.0e-8);
    let t = ((time - a.time_seconds) / span).clamp(0.0, 1.0);
    slerp_quat(normalize_quat(a.value), normalize_quat(b.value), t)
}

pub(super) fn normalize_quat(mut q: [f32; 4]) -> [f32; 4] {
    let length_sq = q.iter().map(|v| v * v).sum::<f32>();
    if !length_sq.is_finite() || length_sq <= 1.0e-12 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let inv = length_sq.sqrt().recip();
    for value in &mut q {
        *value *= inv;
    }
    q
}

pub(super) fn slerp_quat(a: [f32; 4], mut b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a.iter().zip(b.iter()).map(|(a, b)| a * b).sum::<f32>();
    if dot < 0.0 {
        for value in &mut b {
            *value = -*value;
        }
        dot = -dot;
    }
    if dot > 0.9995 {
        return normalize_quat([
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ]);
    }
    let theta = dot.clamp(-1.0, 1.0).acos();
    let sin_theta = theta.sin().max(1.0e-8);
    let wa = ((1.0 - t) * theta).sin() / sin_theta;
    let wb = (t * theta).sin() / sin_theta;
    normalize_quat([
        a[0] * wa + b[0] * wb,
        a[1] * wa + b[1] * wb,
        a[2] * wa + b[2] * wb,
        a[3] * wa + b[3] * wb,
    ])
}

// Linear palette interpolation preserves weighted positions (including an
// interrupted blend) while normal/tangent normalization remains on the GPU.
pub(super) fn interpolate_skin_palette(destination: &mut [[f32;16]], from: &[[f32;16]],
    to: &[[f32;16]], alpha: f32)
{
    for (out,(a,b)) in destination.iter_mut().zip(from.iter().zip(to)) {
        for i in 0..16 { out[i] = a[i] + (b[i]-a[i])*alpha; }
    }
}
