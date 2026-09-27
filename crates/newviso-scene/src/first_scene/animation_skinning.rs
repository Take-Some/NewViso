use super::*;
use newviso_model::{
    AnimationInterpolation, AnimationQuatKey, AnimationVec3Key, ModelAnimationClip, ModelSkeleton,
};
use newviso_resource_runtime::AssetId;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug)]
pub(super) struct SkinnedEntityAnimationState {
    source_model_id: AssetId,
    source_to_model: [f32; 16],
    model_to_source: [f32; 16],
    skeleton: Arc<ModelSkeleton>,
    derived_joint_drivers: Arc<[Option<DerivedJointDriver>]>,
    playback: Option<AnimationPlayback>,
}

#[derive(Clone, Copy, Debug)]
struct DerivedJointDriver {
    joint: u16,
    bind_offset: [f32; 16],
}

#[derive(Clone, Debug)]
struct AnimationPlayback {
    clip: Arc<ModelAnimationClip>,
    time_seconds: f32,
    playback_rate: f32,
}

impl SkinnedEntityAnimationState {
    pub(super) fn new(
        source_model_id: AssetId,
        source_to_model: [f32; 16],
        skeleton: Arc<ModelSkeleton>,
    ) -> Result<Self, String> {
        let model_to_source = inverse_affine(source_to_model)
            .ok_or_else(|| "skinned model source_to_model transform is singular".to_owned())?;
        let derived_joint_drivers = build_derived_joint_drivers(&skeleton)?;
        Ok(Self {
            source_model_id,
            source_to_model,
            model_to_source,
            skeleton,
            derived_joint_drivers,
            playback: None,
        })
    }
}

impl Scene3dRuntime {
    pub fn play_entity_animation(
        &mut self,
        stable_id: u64,
        clip: Arc<ModelAnimationClip>,
        playback_rate: f32,
        restart_if_same: bool,
    ) -> Result<bool, String> {
        if !playback_rate.is_finite() || playback_rate <= 0.0 {
            return Err(format!(
                "animation playback rate must be finite and > 0, got {playback_rate}"
            ));
        }
        if !clip.duration_seconds.is_finite() || clip.duration_seconds <= 0.0 {
            return Err(format!(
                "animation clip '{}' has invalid duration {}",
                clip.name, clip.duration_seconds
            ));
        }
        let state = self.skinned_entities.get_mut(&stable_id).ok_or_else(|| {
            format!("scene entity {stable_id} is not an installed skinned model")
        })?;
        for track in &clip.tracks {
            if track.joint as usize >= state.skeleton.joints.len() {
                return Err(format!(
                    "animation clip '{}' track joint={} exceeds skeleton joints={}",
                    clip.name,
                    track.joint,
                    state.skeleton.joints.len()
                ));
            }
        }
        if !restart_if_same
            && state
                .playback
                .as_ref()
                .is_some_and(|playback| playback.clip.name == clip.name)
        {
            if let Some(playback) = state.playback.as_mut() {
                playback.playback_rate = playback_rate;
            }
            return Ok(false);
        }
        state.playback = Some(AnimationPlayback {
            clip,
            time_seconds: 0.0,
            playback_rate,
        });
        Ok(true)
    }

    pub fn stop_entity_animation(&mut self, stable_id: u64) -> bool {
        self.skinned_entities
            .get_mut(&stable_id)
            .and_then(|state| state.playback.take())
            .is_some()
    }

    pub fn entity_animation_name(&self, stable_id: u64) -> Option<&str> {
        self.skinned_entities
            .get(&stable_id)?
            .playback
            .as_ref()
            .map(|playback| playback.clip.name.as_str())
    }

    pub(super) fn update_skinned_animations(&mut self, dt: f32) -> Result<(), String> {
        if !dt.is_finite() || dt <= 0.0 || self.skinned_entities.is_empty() {
            return Ok(());
        }
        let animated_ids = self
            .skinned_entities
            .iter()
            .filter_map(|(stable_id, state)| state.playback.as_ref().map(|_| *stable_id))
            .collect::<Vec<_>>();

        for stable_id in animated_ids {
            let (
                source_model_id,
                source_to_model,
                model_to_source,
                skeleton,
                derived_joint_drivers,
                clip,
                sample_time,
            ) = {
                let state = self
                    .skinned_entities
                    .get_mut(&stable_id)
                    .ok_or_else(|| format!("skinned entity {stable_id} disappeared"))?;
                let playback = state
                    .playback
                    .as_mut()
                    .ok_or_else(|| format!("skinned entity {stable_id} lost playback"))?;
                let duration = playback.clip.duration_seconds.max(1.0e-6);
                playback.time_seconds += dt * playback.playback_rate;
                if playback.clip.looping {
                    playback.time_seconds = playback.time_seconds.rem_euclid(duration);
                } else {
                    playback.time_seconds = playback.time_seconds.min(duration);
                }
                (
                    state.source_model_id,
                    state.source_to_model,
                    state.model_to_source,
                    state.skeleton.clone(),
                    state.derived_joint_drivers.clone(),
                    playback.clip.clone(),
                    playback.time_seconds,
                )
            };

            let palette = build_skin_palette(
                &skeleton,
                &derived_joint_drivers,
                &clip,
                sample_time,
                source_to_model,
                model_to_source,
            )?;
            let bind_vertices = self
                .asset_model_cache
                .get(&source_model_id.0)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "skinned entity {} lost cached source geometry model={}",
                        stable_id, source_model_id.0
                    )
                })?;
            let skinned_vertices = skin_vertices(&bind_vertices, &palette)?;
            let mesh = self.asset_meshes.get(&stable_id).ok_or_else(|| {
                format!("skinned entity {} lost installed render mesh", stable_id)
            })?;
            if skinned_vertices.len() != mesh.vertex_count as usize {
                return Err(format!(
                    "skinned vertex count changed entity={} actual={} expected={}",
                    stable_id,
                    skinned_vertices.len(),
                    mesh.vertex_count
                ));
            }
            let upload_from = mesh.first_vertex as usize * FLOATS_PER_VERTEX;
            let mut packed =
                Vec::with_capacity(skinned_vertices.len().saturating_mul(FLOATS_PER_VERTEX));
            asset_models::append_local_asset_vertices(&mut packed, &skinned_vertices);
            let upload_end = upload_from
                .checked_add(packed.len())
                .ok_or_else(|| "skinned vertex upload range overflow".to_owned())?;
            if upload_end > self.asset_vertex_data.len() {
                return Err(format!(
                    "skinned vertex upload exceeds resident buffer entity={} range={}..{} floats={}",
                    stable_id,
                    upload_from,
                    upload_end,
                    self.asset_vertex_data.len()
                ));
            }
            self.asset_vertex_data[upload_from..upload_end].copy_from_slice(&packed);
            // Animation mutates one exact resident vertex range. Keeping this
            // separate from the static append marker prevents a per-frame upload
            // of every asset located after the character in the shared buffer.
            self.asset_skin_upload_ranges.push((upload_from, upload_end));
        }
        Ok(())
    }
}

fn build_derived_joint_drivers(
    skeleton: &ModelSkeleton,
) -> Result<Arc<[Option<DerivedJointDriver>]>, String> {
    let mut names = BTreeMap::<&str, usize>::new();
    let mut bind_globals = Vec::<[f32; 16]>::with_capacity(skeleton.joints.len());

    for (index, joint) in skeleton.joints.iter().enumerate() {
        if names.insert(joint.name.as_str(), index).is_some() {
            return Err(format!(
                "skeleton '{}' contains duplicate joint name '{}'",
                skeleton.name, joint.name
            ));
        }
        let local = trs_matrix(joint.bind_translation, joint.bind_rotation, joint.bind_scale);
        let global = match joint.parent {
            Some(parent) => {
                let parent = parent as usize;
                if parent >= index {
                    return Err(format!(
                        "skeleton '{}' joint={} has non-topological parent={}",
                        skeleton.name, index, parent
                    ));
                }
                mul_mat4(bind_globals[parent], local)
            }
            None => local,
        };
        bind_globals.push(global);
    }

    let mut drivers = vec![None; skeleton.joints.len()];
    for (index, joint) in skeleton.joints.iter().enumerate() {
        let Some(base_name) = joint.name.strip_suffix("_helper") else {
            continue;
        };
        let Some(&driver_index) = names.get(base_name) else {
            continue;
        };
        if driver_index >= index || driver_index > u16::MAX as usize {
            continue;
        }
        let bind_offset = mul_mat4(
            skeleton.joints[driver_index].inverse_bind_matrix,
            bind_globals[index],
        );
        drivers[index] = Some(DerivedJointDriver {
            joint: driver_index as u16,
            bind_offset,
        });
    }

    Ok(Arc::from(drivers))
}

fn build_skin_palette(
    skeleton: &ModelSkeleton,
    derived_joint_drivers: &[Option<DerivedJointDriver>],
    clip: &ModelAnimationClip,
    time_seconds: f32,
    source_to_model: [f32; 16],
    model_to_source: [f32; 16],
) -> Result<Vec<[f32; 16]>, String> {
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
    if derived_joint_drivers.len() != skeleton.joints.len() {
        return Err(format!(
            "skeleton '{}' derived driver count={} does not match joints={}",
            skeleton.name,
            derived_joint_drivers.len(),
            skeleton.joints.len()
        ));
    }
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
        translations[joint] = sample_vec3(
            &track.translations,
            track.translation_interpolation,
            time_seconds,
            translations[joint],
        );
        rotations[joint] = sample_quat(
            &track.rotations,
            track.rotation_interpolation,
            time_seconds,
            rotations[joint],
        );
        scales[joint] = sample_vec3(
            &track.scales,
            track.scale_interpolation,
            time_seconds,
            scales[joint],
        );
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
                // Locomotion clips animate the compact deform rig, while the
                // authored skin can also reference a parallel *_helper/roll rig.
                // Drive duplicate helpers from their core joint and preserve
                // their bind-space offset; specialized children then inherit it.
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

fn skin_vertices(
    bind_vertices: &[AssetTriangleVertex],
    palette: &[[f32; 16]],
) -> Result<Vec<AssetTriangleVertex>, String> {
    bind_vertices
        .iter()
        .copied()
        .map(|mut vertex| {
            let count = vertex.skin_influences as usize;
            if count == 0 {
                return Ok(vertex);
            }
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
            vertex.position = position;
            vertex.normal = normal.normalized();
            let tangent = tangent
                .sub(vertex.normal.mul(tangent.dot(vertex.normal)))
                .normalized();
            vertex.tangent[0] = tangent.x;
            vertex.tangent[1] = tangent.y;
            vertex.tangent[2] = tangent.z;
            Ok(vertex)
        })
        .collect()
}

fn sample_vec3(
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

fn sample_quat(
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

fn normalize_quat(mut q: [f32; 4]) -> [f32; 4] {
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

fn slerp_quat(a: [f32; 4], mut b: [f32; 4], t: f32) -> [f32; 4] {
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

fn trs_matrix(translation: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) -> [f32; 16] {
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

fn mul_mat4(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[column * 4 + row] =
                (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum();
        }
    }
    out
}

fn transform_point_mat4(matrix: [f32; 16], point: Vec3) -> Vec3 {
    Vec3::new(
        matrix[0] * point.x + matrix[4] * point.y + matrix[8] * point.z + matrix[12],
        matrix[1] * point.x + matrix[5] * point.y + matrix[9] * point.z + matrix[13],
        matrix[2] * point.x + matrix[6] * point.y + matrix[10] * point.z + matrix[14],
    )
}

fn transform_vector_mat4(matrix: [f32; 16], vector: Vec3) -> Vec3 {
    Vec3::new(
        matrix[0] * vector.x + matrix[4] * vector.y + matrix[8] * vector.z,
        matrix[1] * vector.x + matrix[5] * vector.y + matrix[9] * vector.z,
        matrix[2] * vector.x + matrix[6] * vector.y + matrix[10] * vector.z,
    )
}

fn inverse_affine(matrix: [f32; 16]) -> Option<[f32; 16]> {
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
        i00, i10, i20, 0.0, i01, i11, i21, 0.0, i02, i12, i22, 0.0, it[0], it[1], it[2],
        1.0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quaternion_slerp_preserves_unit_length() {
        let q = slerp_quat([0.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 0.0], 0.5);
        let length = q.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((length - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn untracked_helper_chain_follows_animated_core_joint() {
        let identity = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        let child_inverse_bind = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            -1.0, 0.0, 0.0, 1.0,
        ];
        let skeleton = ModelSkeleton {
            name: "helper-test".to_owned(),
            joints: vec![
                newviso_model::ModelJoint {
                    name: "root".to_owned(),
                    tag: 1,
                    parent: None,
                    inverse_bind_matrix: identity,
                    bind_translation: [0.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                },
                newviso_model::ModelJoint {
                    name: "root_helper".to_owned(),
                    tag: 2,
                    parent: None,
                    inverse_bind_matrix: identity,
                    bind_translation: [0.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                },
                newviso_model::ModelJoint {
                    name: "roll".to_owned(),
                    tag: 3,
                    parent: Some(1),
                    inverse_bind_matrix: child_inverse_bind,
                    bind_translation: [1.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                },
            ],
        };
        let drivers = build_derived_joint_drivers(&skeleton).unwrap();
        assert_eq!(drivers[1].map(|driver| driver.joint), Some(0));

        let clip = ModelAnimationClip {
            name: "move-root".to_owned(),
            duration_seconds: 1.0,
            looping: true,
            tracks: vec![newviso_model::JointAnimationTrack {
                joint: 0,
                translation_interpolation: AnimationInterpolation::Linear,
                rotation_interpolation: AnimationInterpolation::Linear,
                scale_interpolation: AnimationInterpolation::Linear,
                translations: vec![AnimationVec3Key {
                    time_seconds: 0.0,
                    value: [2.0, 0.0, 0.0],
                }],
                rotations: Vec::new(),
                scales: Vec::new(),
            }],
        };
        let palette =
            build_skin_palette(&skeleton, &drivers, &clip, 0.0, identity, identity).unwrap();

        for joint in 0..3 {
            let moved = transform_point_mat4(palette[joint], Vec3::ZERO);
            assert!(
                (moved.x - 2.0).abs() < 1.0e-5,
                "joint {joint} did not inherit core motion: x={}",
                moved.x
            );
        }
    }

    #[test]
    fn affine_inverse_roundtrip_is_identity() {
        let m = trs_matrix(
            [1.0, 2.0, -3.0],
            [0.0, 0.38268343, 0.0, 0.9238795],
            [1.2, 0.8, 1.1],
        );
        let inv = inverse_affine(m).unwrap();
        let product = mul_mat4(m, inv);
        for (index, value) in product.iter().enumerate() {
            let expected = if index % 5 == 0 { 1.0 } else { 0.0 };
            assert!((value - expected).abs() < 1.0e-4);
        }
    }
}
