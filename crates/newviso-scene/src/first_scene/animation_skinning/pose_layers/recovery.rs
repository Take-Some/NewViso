//! Captured physical pose -> authored pose in local TRS, never matrix/vertex lerp.
use super::*;

#[derive(Clone, Debug)]
struct JointPose {
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}

#[derive(Clone, Debug)]
pub(super) struct RecoveryBlend {
    locals: Arc<[JointPose]>,
    clip_name: String,
    elapsed: f32,
    duration: f32,
    started: bool,
    rebase_error: f32,
    initial_pose_committed: bool,
}

/// Prepared against a particular installed skeleton before physical teardown.
#[derive(Debug)]
pub struct ScenePhysicalRecovery {
    skeleton: Arc<ModelSkeleton>,
    blend: RecoveryBlend,
    initial_palette: Vec<[f32; 16]>,
    packed: Option<(usize, Vec<f32>)>,
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn decompose(m: [f32; 16]) -> Result<JointPose, String> {
    if !m.iter().all(|v| v.is_finite()) {
        return Err("nonfinite recovery transform".into());
    }
    let mut scale = matrix_scale(m)?;
    let mut a = [[0.0; 3]; 3];
    for col in 0..3 {
        for row in 0..3 {
            a[col][row] = m[col * 4 + row] / scale[col];
        }
    }
    if dot(cross(a[0], a[1]), a[2]) < 0.0 {
        scale[2] = -scale[2];
        a[2] = a[2].map(|v| -v);
    }
    if (0..3).any(|i| (0..i).any(|j| dot(a[i], a[j]).abs() > 0.002)) {
        return Err("recovery transform contains shear".into());
    }
    let (m00, m11, m22) = (a[0][0], a[1][1], a[2][2]);
    let q = if m00 + m11 + m22 > 0.0 {
        let s = (m00 + m11 + m22 + 1.0).sqrt() * 2.0;
        [
            (a[1][2] - a[2][1]) / s,
            (a[2][0] - a[0][2]) / s,
            (a[0][1] - a[1][0]) / s,
            s * 0.25,
        ]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [
            s * 0.25,
            (a[1][0] + a[0][1]) / s,
            (a[2][0] + a[0][2]) / s,
            (a[1][2] - a[2][1]) / s,
        ]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [
            (a[1][0] + a[0][1]) / s,
            s * 0.25,
            (a[2][1] + a[1][2]) / s,
            (a[2][0] - a[0][2]) / s,
        ]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [
            (a[2][0] + a[0][2]) / s,
            (a[2][1] + a[1][2]) / s,
            s * 0.25,
            (a[0][1] - a[1][0]) / s,
        ]
    };
    let pose = JointPose {
        translation: [m[12], m[13], m[14]],
        rotation: normalize_quat(q),
        scale,
    };
    if !pose.rotation.iter().all(|v| v.is_finite()) {
        return Err("invalid recovery rotation".into());
    }
    Ok(pose)
}
fn parent(s: &ModelSkeleton, drivers: &[Option<DerivedJointDriver>], i: usize) -> Option<usize> {
    drivers[i]
        .map(|d| d.joint as usize)
        .or(s.joints[i].parent.map(|p| p as usize))
}
fn local_poses(
    s: &ModelSkeleton,
    drivers: &[Option<DerivedJointDriver>],
    globals: &[[f32; 16]],
) -> Result<Vec<JointPose>, String> {
    if globals.len() != s.joints.len() || drivers.len() != globals.len() {
        return Err("recovery dimensions do not match skeleton".into());
    }
    (0..globals.len())
        .map(|i| {
            let m = if let Some(p) = parent(s, drivers, i) {
                if p >= i {
                    return Err("non-topological recovery hierarchy".into());
                }
                mul_mat4(
                    inverse_affine(globals[p]).ok_or("singular recovery parent")?,
                    globals[i],
                )
            } else {
                globals[i]
            };
            decompose(m)
        })
        .collect()
}
fn globals(
    s: &ModelSkeleton,
    drivers: &[Option<DerivedJointDriver>],
    locals: &[JointPose],
) -> Vec<[f32; 16]> {
    let mut out = Vec::with_capacity(locals.len());
    for (i, l) in locals.iter().enumerate() {
        let m = trs_matrix(l.translation, l.rotation, l.scale);
        out.push(parent(s, drivers, i).map_or(m, |p| mul_mat4(out[p], m)));
    }
    out
}

/// Spherical direction interpolation prevents equal-length local bone offsets
/// collapsing at a half turn. Only explicit authored endpoint lengths can vary.
fn offset(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    let (la, lb) = (length(a), length(b));
    if la < 1e-7 || lb < 1e-7 {
        return lerp_vec3(a, b, t);
    }
    let (u, v) = (a.map(|x| x / la), b.map(|x| x / lb));
    let c = dot(u, v).clamp(-1.0, 1.0);
    let direction = if c > 0.9995 {
        let d = lerp_vec3(u, v, t);
        let n = length(d);
        d.map(|x| x / n)
    } else if c < -0.9995 {
        let axis = if u[0].abs() < 0.8 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let tangent: [f32; 3] = std::array::from_fn(|i| v[i] - c * u[i]);
        let side = if length(tangent) > 1e-5 {
            tangent
        } else {
            cross(u, axis)
        };
        let n = length(side);
        let angle = c.acos() * t;
        std::array::from_fn(|i| u[i] * angle.cos() + side[i] / n * angle.sin())
    } else {
        let angle = c.acos();
        let denom = angle.sin();
        std::array::from_fn(|i| {
            (u[i] * ((1.0 - t) * angle).sin() + v[i] * (t * angle).sin()) / denom
        })
    };
    let magnitude = la + (lb - la) * t;
    direction.map(|v| v * magnitude)
}
impl RecoveryBlend {
    pub(super) fn start_for_clip(&mut self, name: &str) -> bool {
        if self.clip_name != name {
            return false;
        }
        self.started = true;
        true
    }
    pub(super) fn advance(&mut self, dt: f32) {
        if self.started && dt.is_finite() && dt > 0.0 {
            self.elapsed = (self.elapsed + dt).min(self.duration);
        }
    }
    fn alpha(&self) -> f32 {
        let t = (self.elapsed / self.duration).clamp(0.0, 1.0);
        // Zero endpoint slope while the destination clip continues to advance.
        t * t * (3.0 - 2.0 * t)
    }
    pub(super) fn apply(
        &self,
        palette: Vec<[f32; 16]>,
        s: &ModelSkeleton,
        drivers: &[Option<DerivedJointDriver>],
        source_to_model: [f32; 16],
        model_to_source: [f32; 16],
    ) -> Result<Vec<[f32; 16]>, String> {
        if self.alpha() >= 1.0 {
            return Ok(palette);
        }
        let target_globals = palette
            .iter()
            .zip(&s.joints)
            .map(|(p, j)| {
                Ok(mul_mat4(
                    mul_mat4(*p, source_to_model),
                    inverse_affine(j.inverse_bind_matrix).ok_or("singular recovery bind")?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let target = local_poses(s, drivers, &target_globals)?;
        let t = self.alpha();
        let locals = self
            .locals
            .iter()
            .zip(target)
            .enumerate()
            .map(|(i, (a, b))| {
                if (0..3).any(|axis| a.scale[axis] * b.scale[axis] <= 0.0) {
                    return Err("recovery changes scale handedness".into());
                }
                Ok(JointPose {
                    translation: if parent(s, drivers, i).is_none() {
                        lerp_vec3(a.translation, b.translation, t)
                    } else {
                        offset(a.translation, b.translation, t)
                    },
                    rotation: slerp_quat(a.rotation, b.rotation, t),
                    scale: lerp_vec3(a.scale, b.scale, t),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(palette_from_globals(
            s,
            &globals(s, drivers, &locals),
            model_to_source,
        ))
    }
}

fn prepare_recovery(
    state: &SkinnedEntityAnimationState,
    current: [f32; 16],
    target_model: [f32; 16],
    clip: &ModelAnimationClip,
    start_time_seconds: f32,
    duration: f32,
) -> Result<ScenePhysicalRecovery, String> {
    if !start_time_seconds.is_finite()
        || start_time_seconds < 0.0
        || !clip.duration_seconds.is_finite()
        || start_time_seconds >= clip.duration_seconds
        || !duration.is_finite()
        || duration <= 0.0
        || clip.name.is_empty()
    {
        return Err("invalid recovery blend timing/clip".into());
    }
    let pose = state
        .pose_layers
        .physical
        .as_ref()
        .ok_or("missing physical recovery pose")?;
    let change = mul_mat4(
        inverse_affine(target_model).ok_or("singular recovery target")?,
        current,
    );
    let rebased: Vec<_> = pose.globals.iter().map(|m| mul_mat4(change, *m)).collect();
    let locals = local_poses(&state.skeleton, &state.derived_joint_drivers, &rebased)?;
    let rebuilt = globals(&state.skeleton, &state.derived_joint_drivers, &locals);
    let mut error = 0.0_f32;
    for (old, new) in pose.globals.iter().zip(&rebuilt) {
        let (a, b) = (mul_mat4(current, *old), mul_mat4(target_model, *new));
        error = error.max(length(std::array::from_fn(|i| a[12 + i] - b[12 + i])));
    }
    if !error.is_finite() {
        return Err("invalid recovery rebase".into());
    }
    let blend = RecoveryBlend {
        locals: locals.into(),
        clip_name: clip.name.clone(),
        elapsed: 0.0,
        duration,
        started: false,
        rebase_error: error,
        initial_pose_committed: false,
    };
    let target_palette = build_skin_palette(
        &state.skeleton,
        &state.derived_joint_drivers,
        clip,
        start_time_seconds,
        state.source_to_model,
        state.model_to_source,
    )?;
    // Fail before physical ownership is released if the authored target
    // cannot be represented by this captured rigid hierarchy.
    blend.apply(
        target_palette,
        &state.skeleton,
        &state.derived_joint_drivers,
        state.source_to_model,
        state.model_to_source,
    )?;
    Ok(ScenePhysicalRecovery {
        skeleton: state.skeleton.clone(),
        blend,
        initial_palette: palette_from_globals(&state.skeleton, &rebuilt, state.model_to_source),
        packed: None,
    })
}

/// Relative metrology is meaningful only when four position ulps can resolve
/// a 0.1% change. Absolute error still covers every edge, including face helpers.
fn measured_length_error(actual: f32, expected: f32, spatial: f32) -> (f32, Option<f32>) {
    let absolute = (actual - expected).abs();
    let positional_resolution = 4.0 * f32::EPSILON * (spatial + expected + 1.0);
    (
        absolute,
        (expected > positional_resolution / 0.001).then_some(absolute / expected),
    )
}

impl Scene3dRuntime {
    /// Read-only capture/rebase; callers can reject it before touching physics.
    pub fn capture_entity_physical_recovery(
        &self,
        entity: u64,
        target_model: [f32; 16],
        clip: &ModelAnimationClip,
        start_time_seconds: f32,
        duration: f32,
    ) -> Result<ScenePhysicalRecovery, String> {
        let state = self
            .skinned_entities
            .get(&entity)
            .ok_or("missing recovery skeleton")?;
        let current = self
            .entity_transform_matrix(entity)
            .ok_or("missing recovery entity transform")?;
        let mut capture = prepare_recovery(
            state,
            current,
            target_model,
            clip,
            start_time_seconds,
            duration,
        )?;
        if !renderer_skinning::gpu_skinning_enabled() {
            let mesh = self
                .asset_meshes
                .get(&entity)
                .ok_or("missing recovery mesh")?;
            let vertices = self
                .asset_model_cache
                .get(&state.source_model_id.0)
                .ok_or("missing recovery source vertices")?;
            let packed = skin_vertices_to_packed(vertices, &capture.initial_palette)?;
            if packed.len() != mesh.vertex_count as usize * FLOATS_PER_VERTEX {
                return Err("recovery vertex dimensions changed".into());
            }
            let start = (mesh
                .skinned_first_vertex
                .ok_or("missing recovery compact stream")? as usize)
                .checked_mul(FLOATS_PER_VERTEX)
                .ok_or("recovery compact offset overflow")?;
            let end = start
                .checked_add(packed.len())
                .ok_or("recovery compact range overflow")?;
            if end > self.skinned_vertex_data.len() {
                return Err("recovery compact range is not resident".into());
            }
            capture.packed = Some((start, packed));
        }
        Ok(capture)
    }

    pub fn begin_entity_physical_recovery(
        &mut self,
        entity: u64,
        mut captured: ScenePhysicalRecovery,
    ) -> Result<(), String> {
        let state = self
            .skinned_entities
            .get(&entity)
            .ok_or("missing recovery skeleton")?;
        if state.pose_layers.physical.is_some() || !Arc::ptr_eq(&state.skeleton, &captured.skeleton)
        {
            return Err("recovery ownership/skeleton changed after capture".into());
        }
        if let Some((start, packed)) = &captured.packed {
            if start
                .checked_add(packed.len())
                .is_none_or(|end| end > self.skinned_vertex_data.len())
            {
                return Err("recovery presentation stream changed after capture".into());
            }
        }
        let state = self
            .skinned_entities
            .get_mut(&entity)
            .expect("validated recovery skeleton");
        invalidate_pose(state);
        state.presented_feet = state.foot_joint_indices.and_then(|indices| {
            skin_palette_feet_positions(
                &state.skeleton,
                &captured.initial_palette,
                state.source_to_model,
                indices,
            )
        });
        state.presented_palette = captured.initial_palette;
        state.presented_generation = Some(state.animation_generation);
        if let Some((start, packed)) = captured.packed {
            let end = start + packed.len();
            self.skinned_vertex_data[start..end].copy_from_slice(&packed);
            for ranges in &mut self.skinned_dirty_ranges {
                ranges.push((start, end));
            }
        }
        captured.blend.initial_pose_committed = true;
        state.pose_layers.recovery = Some(captured.blend);
        state.pose_layers.additive = None;
        Ok(())
    }
    pub fn cancel_entity_physical_recovery(&mut self, entity: u64) -> bool {
        let Some(state) = self.skinned_entities.get_mut(&entity) else {
            return false;
        };
        if state.pose_layers.recovery.take().is_none() {
            return false;
        }
        invalidate_pose(state);
        true
    }

    pub fn entity_physical_recovery(&self, entity: u64) -> Option<serde_json::Value> {
        let state = self.skinned_entities.get(&entity)?;
        let r = state.pose_layers.recovery.as_ref()?;
        let mut value = json!({"active":r.started&&r.elapsed<r.duration,"started":r.started,
            "complete":r.elapsed>=r.duration,"elapsed_seconds":r.elapsed,
            "duration_seconds":r.duration,"animation_weight":r.alpha(),
            "initial_pose_committed":r.initial_pose_committed,"captured_joints":r.locals.len(),"world_rebase_error":r.rebase_error,
            "clip_time_seconds":state.playback.as_ref().filter(|p|r.started&&p.clip.name==r.clip_name).map(|p|p.time_seconds),
            "palette_interpolation_active":state.palette_blend.is_some(),
            "vertex_interpolation_active":state.presentation_blend.is_some()});
        let measured = (|| -> Result<[f32; 5], String> {
            let playback = state
                .playback
                .as_ref()
                .ok_or("recovery playback not started")?;
            let target = build_skin_palette(
                &state.skeleton,
                &state.derived_joint_drivers,
                &playback.clip,
                playback.time_seconds,
                state.source_to_model,
                state.model_to_source,
            )?;
            let target = local_poses(
                &state.skeleton,
                &state.derived_joint_drivers,
                &globals_from_palette(state, &target)?,
            )?;
            let actual_globals = self.entity_pose_model_matrices(entity)?;
            let actual = local_poses(
                &state.skeleton,
                &state.derived_joint_drivers,
                &actual_globals,
            )?;
            let mut errors = [0.0_f32; 5];
            let t = r.alpha();
            for (i, ((a, b), got)) in r.locals.iter().zip(target).zip(actual).enumerate() {
                if parent(&state.skeleton, &state.derived_joint_drivers, i).is_some() {
                    let expected =
                        length(a.translation) + (length(b.translation) - length(a.translation)) * t;
                    let p =
                        parent(&state.skeleton, &state.derived_joint_drivers, i).expect("parent");
                    let spatial = length([
                        actual_globals[i][12],
                        actual_globals[i][13],
                        actual_globals[i][14],
                    ]) + length([
                        actual_globals[p][12],
                        actual_globals[p][13],
                        actual_globals[p][14],
                    ]);
                    let (absolute, relative) =
                        measured_length_error(length(got.translation), expected, spatial);
                    errors[3] = errors[3].max(absolute);
                    if let Some(relative) = relative {
                        errors[0] = errors[0].max(relative);
                    } else {
                        errors[4] += 1.0;
                    }
                }
                for axis in 0..3 {
                    let expected = a.scale[axis] + (b.scale[axis] - a.scale[axis]) * t;
                    errors[1] = errors[1].max((got.scale[axis] / expected - 1.0).abs());
                }
                let m = if let Some(p) = parent(&state.skeleton, &state.derived_joint_drivers, i) {
                    mul_mat4(
                        inverse_affine(actual_globals[p])
                            .ok_or("singular measured recovery parent")?,
                        actual_globals[i],
                    )
                } else {
                    actual_globals[i]
                };
                for c in 0..3 {
                    for d in 0..c {
                        let a = [m[c * 4], m[c * 4 + 1], m[c * 4 + 2]];
                        let b = [m[d * 4], m[d * 4 + 1], m[d * 4 + 2]];
                        errors[2] = errors[2].max(dot(a, b).abs() / (length(a) * length(b)));
                    }
                }
            }
            Ok(errors)
        })();
        match measured {
            Ok(e) => {
                value["max_relative_local_length_error"] = json!(e[0]);
                value["max_relative_scale_error"] = json!(e[1]);
                value["max_basis_shear"] = json!(e[2]);
                value["max_absolute_local_length_error"] = json!(e[3]);
                value["subresolution_bones"] = json!(e[4] as usize);
            }
            Err(e) => {
                value["measurement_error"] = json!(e);
            }
        }
        Some(value)
    }
}
#[cfg(test)]
mod tests;
