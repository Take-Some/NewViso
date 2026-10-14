use super::*;

#[derive(Clone, Debug, Default)]
pub(super) struct PoseLayers {
    physical: Option<PhysicalPose>,
    additive: Option<AdditivePlayback>,
}

#[derive(Clone, Debug)]
struct PhysicalPose {
    frozen_locals: Arc<[[f32; 16]]>,
    globals: Arc<[[f32; 16]]>,
    palette: Arc<[[f32; 16]]>,
}

#[derive(Clone, Debug)]
struct AdditivePlayback {
    clip: Arc<ModelAnimationClip>,
    time: f32,
    weight: f32,
}

#[derive(Clone, Default)]
pub(super) struct PoseLayerSample {
    physical_palette: Option<Arc<[[f32; 16]]>>,
    additive: Option<AdditivePlayback>,
}

impl PoseLayerSample {
    pub(super) fn is_physical(&self) -> bool {
        self.physical_palette.is_some()
    }
}

impl PoseLayers {
    pub(super) fn sample(&self) -> PoseLayerSample {
        PoseLayerSample {
            physical_palette: self.physical.as_ref().map(|p| p.palette.clone()),
            additive: self.additive.clone(),
        }
    }
    pub(super) fn is_physical(&self) -> bool {
        self.physical.is_some()
    }
    pub(super) fn advance(&mut self, dt: f32) -> bool {
        let Some(additive) = self.additive.as_mut() else {
            return false;
        };
        additive.time += dt;
        if additive.time >= additive.clip.duration_seconds {
            self.additive = None;
            return true;
        }
        false
    }
}

pub(super) fn invalidate_pose(state: &mut SkinnedEntityAnimationState) {
    state.animation_generation = state.animation_generation.wrapping_add(1).max(1);
    state.presentation_blend = None;
    state.palette_blend = None;
    if let Some(playback) = state.playback.as_mut() {
        playback.generation = state.animation_generation;
        playback.last_sample_elapsed_seconds = None;
    }
}

pub(super) fn globals_from_palette(
    state: &SkinnedEntityAnimationState,
    palette: &[[f32; 16]],
) -> Result<Vec<[f32; 16]>, String> {
    palette
        .iter()
        .zip(state.skeleton.joints.iter())
        .map(|(matrix, joint)| {
            let bind = inverse_affine(joint.inverse_bind_matrix)
                .ok_or_else(|| "singular skeleton bind matrix".to_owned())?;
            Ok(mul_mat4(mul_mat4(*matrix, state.source_to_model), bind))
        })
        .collect()
}

fn palette_from_globals(
    skeleton: &ModelSkeleton,
    globals: &[[f32; 16]],
    model_to_source: [f32; 16],
) -> Vec<[f32; 16]> {
    globals
        .iter()
        .zip(skeleton.joints.iter())
        .map(|(global, joint)| {
            mul_mat4(
                mul_mat4(*global, joint.inverse_bind_matrix),
                model_to_source,
            )
        })
        .collect()
}

fn locals_from_globals(
    skeleton: &ModelSkeleton,
    globals: &[[f32; 16]],
) -> Result<Vec<[f32; 16]>, String> {
    skeleton
        .joints
        .iter()
        .enumerate()
        .map(|(index, joint)| {
            if let Some(parent) = joint.parent {
                let inverse = inverse_affine(globals[parent as usize])
                    .ok_or_else(|| "singular animated joint matrix".to_owned())?;
                Ok(mul_mat4(inverse, globals[index]))
            } else {
                Ok(globals[index])
            }
        })
        .collect()
}

pub(super) fn apply_layers(
    mut palette: Vec<[f32; 16]>,
    sample: &PoseLayerSample,
    skeleton: &ModelSkeleton,
    derived_joint_drivers: &[Option<DerivedJointDriver>],
    source_to_model: [f32; 16],
    model_to_source: [f32; 16],
) -> Result<Vec<[f32; 16]>, String> {
    if let Some(physical) = &sample.physical_palette {
        return Ok(physical.to_vec());
    }
    let Some(additive) = &sample.additive else {
        return Ok(palette);
    };
    let globals: Vec<_> = palette
        .iter()
        .zip(skeleton.joints.iter())
        .map(|(matrix, joint)| {
            let bind =
                inverse_affine(joint.inverse_bind_matrix).expect("validated skeleton inverse bind");
            mul_mat4(mul_mat4(*matrix, source_to_model), bind)
        })
        .collect();
    if derived_joint_drivers.len() != skeleton.joints.len() {
        return Err("additive helper dimensions do not match skeleton".into());
    }
    let mut locals = locals_from_globals(skeleton, &globals)?;
    let mut tracked = vec![false; skeleton.joints.len()];
    for track in &additive.clip.tracks {
        let index = track.joint as usize;
        if index >= locals.len() {
            return Err("additive track is outside skeleton".into());
        }
        tracked[index] = true;
        let delta_translation = sample_vec3_clip(
            &track.translations,
            track.translation_interpolation,
            additive.time,
            false,
            additive.clip.duration_seconds,
            [0.0; 3],
        );
        let delta_rotation = sample_quat_clip(
            &track.rotations,
            track.rotation_interpolation,
            additive.time,
            false,
            additive.clip.duration_seconds,
            [0.0, 0.0, 0.0, 1.0],
        );
        let rotation = slerp_quat([0.0, 0.0, 0.0, 1.0], delta_rotation, additive.weight);
        let mut local = mul_mat4(locals[index], trs_matrix([0.0; 3], rotation, [1.0; 3]));
        for axis in 0..3 {
            local[12 + axis] = locals[index][12 + axis] + delta_translation[axis] * additive.weight;
        }
        locals[index] = local;
    }
    let mut result = Vec::with_capacity(locals.len());
    for (index, joint) in skeleton.joints.iter().enumerate() {
        let driver = derived_joint_drivers[index].filter(|_| !tracked[index]);
        result.push(if let Some(driver) = driver {
            let inverse = inverse_affine(globals[driver.joint as usize])
                .ok_or_else(|| "singular additive helper driver".to_owned())?;
            mul_mat4(
                result[driver.joint as usize],
                mul_mat4(inverse, globals[index]),
            )
        } else if let Some(parent) = joint.parent {
            mul_mat4(result[parent as usize], locals[index])
        } else {
            locals[index]
        });
    }
    palette = palette_from_globals(skeleton, &result, model_to_source);
    Ok(palette)
}

impl Scene3dRuntime {
    /// Lowest authored contact point relative to the scene model origin.
    /// Uses the same blended/additive pose as skinning, including helper drivers.
    pub fn entity_skeletal_floor_offset(
        &self,
        entity: u64,
        contacts: &[(&str, f32)],
    ) -> Result<Option<f32>, String> {
        let globals = self.entity_pose_model_matrices(entity)?;
        let state = &self.skinned_entities[&entity];
        let model = self
            .world
            .entity(SceneEntityId(entity))
            .ok_or_else(|| "missing skeletal entity".to_owned())?;
        let transform = geometry::instance_model_matrix(
            Vec3::ZERO,
            model.transform.rotation_degrees,
            model.transform.scale,
        );
        let mut lowest = None::<f32>;
        for (name, radius) in contacts {
            if let Some(index) = state
                .skeleton
                .joints
                .iter()
                .position(|j| j.name.eq_ignore_ascii_case(name))
            {
                let y = mul_mat4(transform, globals[index])[13] - radius;
                lowest = Some(lowest.map_or(y, |v| v.min(y)));
            }
        }
        Ok(lowest)
    }

    pub fn entity_has_physical_pose(&self, entity: u64) -> bool {
        self.skinned_entities
            .get(&entity)
            .is_some_and(|s| s.pose_layers.is_physical())
    }

    pub fn play_entity_additive_animation(
        &mut self,
        entity: u64,
        clip: Arc<ModelAnimationClip>,
        weight: f32,
    ) -> Result<bool, String> {
        if !weight.is_finite() || !(0.0..=2.0).contains(&weight) {
            return Err("invalid additive animation weight".into());
        }
        let state = self
            .skinned_entities
            .get_mut(&entity)
            .ok_or_else(|| format!("entity {entity} has no skinned model"))?;
        if state.pose_layers.is_physical() {
            return Ok(false);
        }
        state.pose_layers.additive = Some(AdditivePlayback {
            clip,
            time: 0.0,
            weight,
        });
        invalidate_pose(state);
        Ok(true)
    }

    pub fn entity_additive_animation(&self, entity: u64) -> Option<serde_json::Value> {
        let a = self
            .skinned_entities
            .get(&entity)?
            .pose_layers
            .additive
            .as_ref()?;
        Some(
            json!({"clip":a.clip.name,"time_seconds":a.time,"duration_seconds":a.clip.duration_seconds,"looping":false,"weight":a.weight}),
        )
    }

    pub(super) fn entity_pose_model_matrices(&self, entity: u64) -> Result<Vec<[f32; 16]>, String> {
        let state = self
            .skinned_entities
            .get(&entity)
            .ok_or_else(|| format!("entity {entity} has no skinned model"))?;
        if let Some(physical) = &state.pose_layers.physical {
            return Ok(physical.globals.to_vec());
        }
        let mut palette = if let Some(playback) = &state.playback {
            if let Some(from) = &playback.transition_from {
                build_blended_skin_palette(
                    &state.skeleton,
                    &state.derived_joint_drivers,
                    &from.clip,
                    from.time_seconds,
                    &playback.clip,
                    playback.time_seconds,
                    (from.elapsed_seconds / from.duration_seconds.max(0.001)).clamp(0.0, 1.0),
                    state.source_to_model,
                    state.model_to_source,
                )?
            } else {
                build_skin_palette(
                    &state.skeleton,
                    &state.derived_joint_drivers,
                    &playback.clip,
                    playback.time_seconds,
                    state.source_to_model,
                    state.model_to_source,
                )?
            }
        } else {
            let translations: Vec<_> = state
                .skeleton
                .joints
                .iter()
                .map(|j| j.bind_translation)
                .collect();
            let rotations: Vec<_> = state
                .skeleton
                .joints
                .iter()
                .map(|j| j.bind_rotation)
                .collect();
            let scales: Vec<_> = state.skeleton.joints.iter().map(|j| j.bind_scale).collect();
            build_palette_from_local_pose(
                &state.skeleton,
                &state.derived_joint_drivers,
                &translations,
                &rotations,
                &scales,
                &vec![true; translations.len()],
                state.source_to_model,
                state.model_to_source,
            )?
        };
        palette = apply_layers(
            palette,
            &state.pose_layers.sample(),
            &state.skeleton,
            &state.derived_joint_drivers,
            state.source_to_model,
            state.model_to_source,
        )?;
        arm_ik::apply_to_palette(
            &mut palette,
            &state.skeleton,
            &state.derived_joint_drivers,
            state.source_to_model,
            &self.model_arm_ik(entity)?,
        )?;
        globals_from_palette(state, &palette)
    }

    /// Transfers ownership of named bone world transforms to external physics.
    /// Unspecified descendants retain their captured local pose. Bone helpers
    /// follow their existing driver rather than being left at the bind pose.
    pub fn set_entity_physical_pose(
        &mut self,
        entity: u64,
        joints: &[(String, [f32; 16])],
    ) -> Result<(), String> {
        let frozen = if !self.entity_has_physical_pose(entity) {
            Some(self.entity_pose_model_matrices(entity)?)
        } else {
            None
        };
        let e = self
            .world
            .entity(SceneEntityId(entity))
            .ok_or_else(|| format!("missing scene entity {entity}"))?;
        let model = geometry::instance_model_matrix(
            e.transform.position,
            e.transform.rotation_degrees,
            e.transform.scale,
        );
        let inverse =
            inverse_affine(model).ok_or_else(|| "singular physical model transform".to_owned())?;
        let state = self
            .skinned_entities
            .get_mut(&entity)
            .ok_or_else(|| format!("entity {entity} has no skinned model"))?;
        let mut requested = BTreeMap::new();
        for (name, world) in joints {
            let index = state
                .skeleton
                .joints
                .iter()
                .position(|j| j.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("physical pose references missing joint {name}"))?;
            if world.iter().any(|v| !v.is_finite()) {
                return Err("physical joint matrix is nonfinite".into());
            }
            requested.insert(index, mul_mat4(inverse, *world));
        }
        if let Some(globals) = frozen {
            let locals = locals_from_globals(&state.skeleton, &globals)?;
            state.pose_layers.physical = Some(PhysicalPose {
                frozen_locals: locals.into(),
                globals: globals.into(),
                palette: Vec::new().into(),
            });
            state.pose_layers.additive = None;
            invalidate_pose(state);
        }
        let pose = state
            .pose_layers
            .physical
            .as_mut()
            .expect("physical pose installed");
        let mut globals = Vec::with_capacity(state.skeleton.joints.len());
        for (index, joint) in state.skeleton.joints.iter().enumerate() {
            let global = if let Some(matrix) = requested.get(&index) {
                *matrix
            } else if let Some(driver) = state.derived_joint_drivers[index] {
                mul_mat4(globals[driver.joint as usize], driver.bind_offset)
            } else if let Some(parent) = joint.parent {
                mul_mat4(globals[parent as usize], pose.frozen_locals[index])
            } else {
                pose.frozen_locals[index]
            };
            globals.push(global);
        }
        pose.palette =
            palette_from_globals(&state.skeleton, &globals, state.model_to_source).into();
        pose.globals = globals.into();
        Ok(())
    }

    pub fn clear_entity_physical_pose(&mut self, entity: u64) -> bool {
        let Some(state) = self.skinned_entities.get_mut(&entity) else {
            return false;
        };
        if state.pose_layers.physical.take().is_none() {
            return false;
        }
        invalidate_pose(state);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn additive_hit_keeps_base_translation_and_bone_length() {
        let identity = trs_matrix([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
        let joint = |name: &str, parent, translation| newviso_model::ModelJoint {
            name: name.into(),
            tag: 1,
            parent,
            bind_translation: translation,
            bind_rotation: [0.0, 0.0, 0.0, 1.0],
            bind_scale: [1.0; 3],
            inverse_bind_matrix: identity,
        };
        let skeleton = ModelSkeleton {
            name: "hit-test".into(),
            joints: vec![
                joint("root", None, [0.0; 3]),
                joint("child", Some(0), [0.0, 1.0, 0.0]),
                joint("helper", Some(0), [0.0, 2.0, 0.0]),
            ],
        };
        let mut clip = ModelAnimationClip {
            name: "additive".into(),
            duration_seconds: 1.0,
            looping: false,
            mover: None,
            tracks: vec![newviso_model::JointAnimationTrack {
                joint: 0,
                translation_interpolation: AnimationInterpolation::Linear,
                rotation_interpolation: AnimationInterpolation::Linear,
                scale_interpolation: AnimationInterpolation::Linear,
                translations: vec![AnimationVec3Key {
                    time_seconds: 0.0,
                    value: [0.0; 3],
                }],
                rotations: vec![AnimationQuatKey {
                    time_seconds: 0.0,
                    value: [0.0, 0.0, 0.70710677, 0.70710677],
                }],
                scales: vec![],
            }],
        };
        let mut child = clip.tracks[0].clone();
        child.joint = 1;
        clip.tracks.push(child);
        let base = vec![
            trs_matrix([3.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
            trs_matrix([3.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
            trs_matrix([3.0, 2.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
        ];
        let sample = PoseLayerSample {
            physical_palette: None,
            additive: Some(AdditivePlayback {
                clip: Arc::new(clip),
                time: 0.0,
                weight: 1.0,
            }),
        };
        let drivers = [
            None,
            None,
            Some(DerivedJointDriver {
                joint: 1,
                bind_offset: trs_matrix([0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]),
            }),
        ];
        let result = apply_layers(base, &sample, &skeleton, &drivers, identity, identity).unwrap();
        assert!((result[0][12] - 3.0).abs() < 1e-5);
        assert!((result[1][12] - 2.0).abs() < 1e-5);
        assert!(result[1][13].abs() < 1e-5);
        let distance = (0..3)
            .map(|i| (result[0][12 + i] - result[1][12 + i]).powi(2))
            .sum::<f32>()
            .sqrt();
        assert!((distance - 1.0).abs() < 1e-5);
        assert!((result[2][12] - 2.0).abs() < 1e-5);
        assert!((result[2][13] + 1.0).abs() < 1e-5);
    }
}
