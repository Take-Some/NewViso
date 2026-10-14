//! Animation clock, worker results, presentation and job scheduling.
use super::*;

impl Scene3dRuntime {
    pub(in crate::first_scene) fn update_skinned_animations(&mut self, dt: f32) -> Result<(), String> {
        if !dt.is_finite() || dt <= 0.0 || self.skinned_entities.is_empty() {
            return Ok(());
        }

        // Animation time is simulation time, not a presentation smoothing
        // budget. Never discard a long frame: doing so permanently desynchronizes
        // authored cameras, actor poses and mover tracks. Only packed-vertex
        // presentation interpolation remains clamped below.
        let animation_dt = dt.max(0.0);
        let presentation_dt = animation_dt.clamp(0.0, MAX_ANIMATION_FRAME_DT);
        let mut mover_updates = Vec::<(u64, [f32; 3], [f32; 3], [f32; 3])>::new();
        for (stable_id, state) in self.skinned_entities.iter_mut() {
            if state.pose_layers.advance(animation_dt) {
                pose_layers::invalidate_pose(state);
            }
            if state.pose_layers.is_physical() {
                continue;
            }
            let Some(playback) = state.playback.as_mut() else {
                continue;
            };
            playback.time_seconds = advance_clip_time(
                &playback.clip,
                playback.time_seconds,
                animation_dt * playback.playback_rate,
            );
            let transition_finished = if let Some(transition) = playback.transition_from.as_mut() {
                transition.time_seconds = advance_clip_time(
                    &transition.clip,
                    transition.time_seconds,
                    animation_dt * transition.playback_rate,
                );
                transition.elapsed_seconds += animation_dt;
                transition.elapsed_seconds >= transition.duration_seconds
            } else {
                false
            };
            if transition_finished {
                playback.transition_from = None;
            }

            if let (Some(origin), Some(mover)) =
                (playback.mover_origin, playback.clip.mover.as_ref())
            {
                let translation = sample_vec3_clip(
                    &mover.translations,
                    mover.translation_interpolation,
                    playback.time_seconds,
                    playback.clip.looping,
                    playback.clip.duration_seconds,
                    [0.0; 3],
                );
                let rotation = sample_quat_clip(
                    &mover.rotations,
                    mover.rotation_interpolation,
                    playback.time_seconds,
                    playback.clip.looping,
                    playback.clip.duration_seconds,
                    [0.0, 0.0, 0.0, 1.0],
                );
                let base = geometry::instance_model_matrix(
                    Vec3::new(origin.position[0], origin.position[1], origin.position[2]),
                    Vec3::new(
                        origin.rotation_degrees[0],
                        origin.rotation_degrees[1],
                        origin.rotation_degrees[2],
                    ),
                    Vec3::new(origin.scale[0], origin.scale[1], origin.scale[2]),
                );
                let mover_matrix = trs_matrix(translation, rotation, [1.0, 1.0, 1.0]);
                let world = mul_mat4(base, mover_matrix);
                mover_updates.push((
                    *stable_id,
                    [world[12], world[13], world[14]],
                    matrix_rotation_degrees(world)?,
                    origin.scale,
                ));
            }
        }
        for (stable_id, position, rotation_degrees, scale) in mover_updates {
            self.apply_animation_transform(stable_id, position, rotation_degrees, scale)?;
        }

        let render_frame = self.frame_index;
        // The frame plan already provides sorted, unique visible entity ids.
        // Do not allocate and sort a second BTreeSet on every animation frame.
        let visible = &self.frame_plan.visible_entities;
        let camera_position = self.camera.position;
        let elapsed_seconds = self.world.process_elapsed_seconds();

        let mut applied = 0usize;
        while applied < MAX_SKINNING_RESULTS_APPLIED_PER_FRAME {
            let result = match self.animation_skinning_pool.try_recv() {
                Ok(SkinningCompletion::Ready(result)) => result,
                Ok(SkinningCompletion::Failed { stable_id, error }) => {
                    self.animation_skinning_in_flight.remove(&stable_id);
                    host_runtime::warn(
                        "newviso.animation",
                        format!("background skinning job failed entity={stable_id}: {error}"),
                    );
                    continue;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            };
            self.animation_skinning_in_flight.remove(&result.stable_id);

            let Some(mesh) = self.asset_meshes.get(&result.stable_id) else {
                continue;
            };
            if mesh.source_model_id.0 != result.source_model_id
                || mesh.first_vertex as usize * FLOATS_PER_VERTEX != result.upload_from
                || mesh.vertex_count as usize != result.expected_vertex_count
            {
                // Entity was removed/reinstalled while the worker was running.
                continue;
            }

            let result_is_current = self
                .skinned_entities
                .get(&result.stable_id)
                .and_then(|state| state.playback.as_ref())
                .is_some_and(|playback| playback.generation == result.generation);
            if !result_is_current {
                // A clip switch or stop happened while this immutable worker job
                // was running. Never upload a stale pose over the new animation.
                continue;
            }

            if result.gpu_skinning {
                let state = self.skinned_entities.get_mut(&result.stable_id).unwrap();
                if result.palette.len() != state.presented_palette.len()
                    || result.palette.iter().flatten().any(|value| !value.is_finite())
                { return Err("GPU skin result has invalid bone palette".into()); }
                let first = state.last_skinning_result_elapsed_seconds.is_none();
                let duration = state.last_skinning_result_elapsed_seconds
                    .map(|last| (elapsed_seconds-last).max(0.) as f32).unwrap_or(0.)
                    .clamp(MIN_SKIN_PRESENTATION_BLEND_SECONDS,MAX_SKIN_PRESENTATION_BLEND_SECONDS);
                state.last_skinning_result_elapsed_seconds = Some(elapsed_seconds);
                state.presented_generation = Some(result.generation);
                if first {
                    state.presented_palette = result.palette;
                    state.presented_feet = result.feet;
                    state.palette_blend = None;
                } else {
                    state.palette_blend = Some(PalettePresentationBlend {
                        from: state.presented_palette.clone(), to: result.palette,
                        from_feet: state.presented_feet, to_feet: result.feet,
                        elapsed_seconds: 0., duration_seconds: duration,
                    });
                }
                applied = applied.saturating_add(1);
                continue;
            }

            if result.packed.len() != result.expected_vertex_count * FLOATS_PER_VERTEX {
                return Err(format!(
                    "skinned packed vertex count changed entity={} actual_floats={} expected_floats={}",
                    result.stable_id,
                    result.packed.len(),
                    result.expected_vertex_count * FLOATS_PER_VERTEX
                ));
            }
            let compact_first_vertex = mesh.skinned_first_vertex.ok_or_else(|| {
                format!(
                    "skinned entity {} has no compact GPU vertex range",
                    result.stable_id
                )
            })? as usize;
            let compact_start = compact_first_vertex
                .checked_mul(FLOATS_PER_VERTEX)
                .ok_or_else(|| "compact skinned vertex offset overflow".to_owned())?;
            let compact_end = compact_start
                .checked_add(result.packed.len())
                .ok_or_else(|| "compact skinned vertex upload range overflow".to_owned())?;
            if compact_end > self.skinned_vertex_data.len() {
                return Err(format!(
                    "skinned compact upload exceeds resident stream entity={} range={}..{} floats={}",
                    result.stable_id,
                    compact_start,
                    compact_end,
                    self.skinned_vertex_data.len()
                ));
            }
            let state = self
                .skinned_entities
                .get_mut(&result.stable_id)
                .ok_or_else(|| format!("skinned entity {} disappeared", result.stable_id))?;
            // The first pose replaces bind data directly: no presentation blend
            // and no need to duplicate potentially megabytes of packed vertices.
            let first_result = state.last_skinning_result_elapsed_seconds.is_none();
            let current_packed = if first_result {
                Vec::new()
            } else {
                self.skinned_vertex_data[compact_start..compact_end].to_vec()
            };
            let blend_duration = state
                .last_skinning_result_elapsed_seconds
                .map(|last| (elapsed_seconds - last).max(0.0) as f32)
                .unwrap_or(0.0)
                .clamp(
                    MIN_SKIN_PRESENTATION_BLEND_SECONDS,
                    MAX_SKIN_PRESENTATION_BLEND_SECONDS,
                );
            state.last_skinning_result_elapsed_seconds = Some(elapsed_seconds);

            if first_result {
                self.skinned_vertex_data[compact_start..compact_end]
                    .copy_from_slice(&result.packed);
                state.presented_feet = result.feet;
                state.presented_generation = Some(result.generation);
                state.presentation_blend = None;
                for ranges in &mut self.skinned_dirty_ranges {
                    ranges.push((compact_start, compact_end));
                }
            } else {
                state.presentation_blend = Some(SkinningPresentationBlend {
                    from_packed: current_packed,
                    to_packed: result.packed,
                    from_feet: state.presented_feet,
                    to_feet: result.feet,
                    elapsed_seconds: 0.0,
                    duration_seconds: blend_duration,
                });
                state.presented_generation = Some(result.generation);
            }
            applied = applied.saturating_add(1);
        }

        // Worker results are intentionally sparse. Interpolate their packed
        // skinned-vertex snapshots at render cadence so animation presentation
        // stays smooth without re-running full CPU skinning every frame.
        {
            let asset_meshes = &self.asset_meshes;
            let skinned_entities = &mut self.skinned_entities;
            let skinned_vertex_data = &mut self.skinned_vertex_data;
            let skinned_dirty_ranges = &mut self.skinned_dirty_ranges;

            for stable_id in visible.iter().map(|id| id.0) {
                let Some(state) = skinned_entities.get_mut(&stable_id) else {
                    continue;
                };
                if let Some(blend) = state.palette_blend.as_mut() {
                    blend.elapsed_seconds += presentation_dt;
                    let alpha = (blend.elapsed_seconds/blend.duration_seconds).clamp(0.,1.);
                    interpolate_skin_palette(&mut state.presented_palette, &blend.from, &blend.to, alpha);
                    state.presented_feet = interpolate_presented_feet(blend.from_feet,blend.to_feet,alpha);
                    if alpha >= 1. { state.palette_blend = None; }
                }
                let Some(blend) = state.presentation_blend.as_mut() else {
                    continue;
                };
                let Some(mesh) = asset_meshes.get(&stable_id) else {
                    state.presentation_blend = None;
                    continue;
                };
                let compact_first_vertex = mesh.skinned_first_vertex.ok_or_else(|| {
                    format!("skinned entity {stable_id} has no compact GPU vertex range")
                })? as usize;
                let compact_start = compact_first_vertex
                    .checked_mul(FLOATS_PER_VERTEX)
                    .ok_or_else(|| "compact skinned vertex offset overflow".to_owned())?;
                let compact_end = compact_start
                    .checked_add(blend.to_packed.len())
                    .ok_or_else(|| "compact skinned presentation range overflow".to_owned())?;
                if compact_end > skinned_vertex_data.len()
                    || blend.from_packed.len() != blend.to_packed.len()
                {
                    return Err(format!(
                        "skinned presentation range mismatch entity={stable_id} range={}..{} from={} to={} resident={}",
                        compact_start,
                        compact_end,
                        blend.from_packed.len(),
                        blend.to_packed.len(),
                        skinned_vertex_data.len()
                    ));
                }

                blend.elapsed_seconds += presentation_dt;
                let alpha = if blend.duration_seconds > 0.0 {
                    (blend.elapsed_seconds / blend.duration_seconds).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                for (dst, (from, to)) in skinned_vertex_data[compact_start..compact_end]
                    .iter_mut()
                    .zip(blend.from_packed.iter().zip(blend.to_packed.iter()))
                {
                    *dst = *from + (*to - *from) * alpha;
                }
                for ranges in skinned_dirty_ranges.iter_mut() {
                    ranges.push((compact_start, compact_end));
                }
                // Rendered feet advance on the very same alpha as packed GPU
                // vertex data; contacts cannot lead the displayed skeleton.
                state.presented_feet = interpolate_presented_feet(
                    blend.from_feet, blend.to_feet, alpha,
                );

                if alpha >= 1.0 {
                    state.presentation_blend = None;
                }
            }
        }

        // Skinning is presentation-only. Build candidates from the last
        // visibility plan and schedule immutable jobs onto persistent workers.
        // The main thread never waits for a skinning result.
        let mut candidates = Vec::<(f64, f32, u64, f64)>::new();
        for stable_id in visible.iter().map(|id| id.0) {
            if self.animation_skinning_in_flight.contains(&stable_id) {
                continue;
            }
            let Some(state) = self.skinned_entities.get(&stable_id) else {
                continue;
            };
            let Some(playback) = state.playback.as_ref() else {
                continue;
            };
            let Some(entity) = self.world.entity(SceneEntityId(stable_id)) else {
                continue;
            };
            let delta = entity.transform.position.sub(camera_position);
            let distance_sq = delta.dot(delta);

            let target_hz = if distance_sq <= 20.0 * 20.0 {
                if renderer_skinning::gpu_skinning_enabled() { 120.0_f64 } else { 30.0_f64 }
            } else if distance_sq <= 45.0 * 45.0 {
                20.0
            } else if distance_sq <= 90.0 * 90.0 {
                12.0
            } else {
                6.0
            };
            let target_interval = 1.0 / target_hz;
            let age = playback
                .last_sample_elapsed_seconds
                .map(|last| (elapsed_seconds - last).max(0.0))
                .unwrap_or(f64::INFINITY);
            if age.is_finite() && age + 1.0e-6 < target_interval {
                continue;
            }

            // Higher overdue ratio wins. Distance is the tie breaker, so close
            // actors get first service without starving actors that have waited.
            let urgency = if age.is_finite() {
                age / target_interval
            } else {
                f64::INFINITY
            };
            candidates.push((urgency, distance_sq, stable_id, elapsed_seconds));
        }
        let available =
            MAX_SKINNING_IN_FLIGHT.saturating_sub(self.animation_skinning_in_flight.len());
        scheduling::select_best_candidates(&mut candidates, available);
        let mut submitted = 0usize;
        for (_, _, stable_id, elapsed_seconds) in candidates {
            let (
                source_model_id,
                source_to_model,
                model_to_source,
                skeleton,
                derived_joint_drivers,
                foot_joint_indices,
                clip,
                sample_time,
                transition_from,
                generation,
            ) =
                {
                    let state = self
                        .skinned_entities
                        .get_mut(&stable_id)
                        .ok_or_else(|| format!("skinned entity {stable_id} disappeared"))?;
                    let playback = state
                        .playback
                        .as_mut()
                        .ok_or_else(|| format!("skinned entity {stable_id} lost playback"))?;
                    playback.last_sample_elapsed_seconds = Some(elapsed_seconds);
                    let transition_from = playback.transition_from.as_ref().map(|transition| {
                        AnimationTransitionSample {
                            clip: transition.clip.clone(),
                            sample_time: transition.time_seconds,
                            alpha: if transition.duration_seconds > 0.0 {
                                (transition.elapsed_seconds / transition.duration_seconds)
                                    .clamp(0.0, 1.0)
                            } else {
                                1.0
                            },
                        }
                    });
                    (
                        state.source_model_id,
                        state.source_to_model,
                        state.model_to_source,
                        state.skeleton.clone(),
                        state.derived_joint_drivers.clone(),
                        state.foot_joint_indices,
                        playback.clip.clone(),
                        playback.time_seconds,
                        transition_from,
                        playback.generation,
                    )
                };

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
            let mesh = self.asset_meshes.get(&stable_id).ok_or_else(|| {
                format!("skinned entity {} lost installed render mesh", stable_id)
            })?;
            let job = SkinningJob {
                gpu_skinning: renderer_skinning::gpu_skinning_enabled(),
                arm_ik: self.model_arm_ik(stable_id)?,
                pose_layers: self.skinned_entities[&stable_id].pose_layers.sample(),
                foot_joint_indices,
                stable_id,
                source_model_id: source_model_id.0,
                generation,
                skeleton,
                derived_joint_drivers,
                clip,
                sample_time,
                transition_from,
                source_to_model,
                model_to_source,
                bind_vertices,
                upload_from: mesh.first_vertex as usize * FLOATS_PER_VERTEX,
                expected_vertex_count: mesh.vertex_count as usize,
            };
            self.animation_skinning_pool.submit(job)?;
            self.animation_skinning_in_flight.insert(stable_id);
            submitted = submitted.saturating_add(1);
        }

        if render_frame % 60 == 0 {
            host_runtime::info(
                "newviso.perf.animation",
                format!(
                    "frame={} visible_skinned={} submitted={} applied={} in_flight={} total_skinned_entities={}",
                    render_frame,
                    visible
                        .iter()
                        .filter(|stable_id| self.skinned_entities.contains_key(&stable_id.0))
                        .count(),
                    submitted,
                    applied,
                    self.animation_skinning_in_flight.len(),
                    self.skinned_entities.len()
                ),
            );
        }
        Ok(())
    }
}
