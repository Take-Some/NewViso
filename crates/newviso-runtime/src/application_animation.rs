use super::*;

impl EngineApplication {
    pub(crate) fn apply_scene_additive_reaction(
        &mut self,
        entity: u64,
        clip_ref: &str,
        weight: f32,
    ) -> Result<bool, String> {
        let Some(address) = self.scene_stream_claims.get(&entity).cloned() else {
            return Ok(false);
        };
        let Some(model) = self.asset_streamer.get::<ModelResource>(&address) else {
            return Ok(false);
        };
        let Some(skeleton) = model.skeleton.as_ref() else {
            return Ok(false);
        };
        let key = (model.id.0, clip_ref.to_owned(), false);
        let clip = if let Some(clip) = self.animation_clip_cache.get(&key) {
            clip.clone()
        } else {
            let clip = Arc::new(load_model_animation_clip(clip_ref, skeleton)?);
            self.animation_clip_cache.insert(key, clip.clone());
            clip
        };
        self.scene
            .play_entity_additive_animation(entity, clip, weight)
    }

    pub(super) fn bind_scene_entity_animation(
        &mut self,
        stable_id: u64,
        mut binding: SceneAnimationBinding,
    ) -> Result<bool, String> {
        // Preserve the request time even when model/skeleton streaming applies
        // the binding several frames later.
        binding.bound_elapsed_seconds = self.elapsed_seconds;
        self.scene_animation_bindings.insert(stable_id, binding);
        self.apply_scene_animation_binding(stable_id)
    }

    pub(super) fn unbind_scene_entity_animation(&mut self, stable_id: u64) -> Result<bool, String> {
        self.scene_animation_bindings.remove(&stable_id);
        let stopped = self.scene.stop_entity_animation(stable_id);
        if self.scene.entity_state(stable_id).is_some() {
            let _ = self.scene.set_animation_process_active(
                stable_id,
                "engine.animation.scene_entity",
                false,
            )?;
        }
        Ok(stopped)
    }

    pub(super) fn apply_scene_animation_binding(&mut self, stable_id: u64) -> Result<bool, String> {
        let Some(binding) = self.scene_animation_bindings.get(&stable_id).cloned() else {
            return Ok(false);
        };
        let Some(clip) = self.load_cached_scene_animation_clip(
            stable_id, &binding.clip_ref, binding.planar_root_lock,
        )? else { return Ok(false); };
        let deferred_seconds =
            (self.elapsed_seconds - binding.bound_elapsed_seconds).max(0.0) as f32;
        let effective_start_time_seconds =
            binding.start_time_seconds + deferred_seconds * binding.playback_rate;
        let changed = self.scene.play_entity_animation(
            stable_id,
            clip,
            binding.playback_rate,
            binding.restart_if_same,
            effective_start_time_seconds,
            binding.blend_seconds,
            binding.apply_mover,
        )?;
        let _ = self.scene.set_animation_process_active(
            stable_id,
            "engine.animation.scene_entity",
            true,
        )?;
        Ok(changed)
    }
    fn load_cached_scene_animation_clip(
        &mut self, stable_id: u64, clip_ref: &str, planar_root_lock: bool,
    ) -> Result<Option<Arc<ModelAnimationClip>>, String> {
        if !self.scene.entity_model_installed(stable_id) {
            return Ok(None);
        }
        let Some(model_address) = self.scene_stream_claims.get(&stable_id).cloned() else {
            return Ok(None);
        };
        let Some(model) = self.asset_streamer.get::<ModelResource>(&model_address) else {
            return Ok(None);
        };
        let skeleton = model.skeleton.as_ref().ok_or_else(|| {
            format!(
                "scene entity {} model '{}' has no skeleton for animation '{}'",
                stable_id, model.name, clip_ref
            )
        })?;
        let cache_key = (
            model.id.0,
            clip_ref.to_owned(),
            planar_root_lock,
        );
        let clip = if let Some(clip) = self.animation_clip_cache.get(&cache_key).cloned() {
            clip
        } else {
            let mut clip = load_model_animation_clip(clip_ref, skeleton)?;
            if planar_root_lock {
                // Physics owns planar motion. Keep the original vertical root
                // track and skeletal rotations without adding authored sliding.
                for track in &mut clip.tracks {
                    let joint = &skeleton.joints[track.joint as usize];
                    if joint.parent.is_none() {
                        for key in &mut track.translations {
                            key.value[0] = joint.bind_translation[0];
                            key.value[2] = joint.bind_translation[2];
                        }
                    }
                }
            }
            let clip = Arc::new(clip);
            self.animation_clip_cache.insert(cache_key, clip.clone());
            clip
        };
        Ok(Some(clip))
    }

    pub(super) fn queue_scene_animation_preloads(
        &mut self, stable_id: u64, clips: Vec<String>, planar_root_lock: bool,
    ) {
        self.scene_animation_preloads.entry(stable_id).or_default()
            .extend(clips.into_iter().map(|clip| (clip, planar_root_lock)));
    }

    pub(super) fn pump_scene_animation_preloads(&mut self) -> Result<(), String> {
        // Admit one clip per streaming pump while startup owns presentation.
        // Off-screen requests remain deferred until their model becomes resident.
        let entities = self.scene_animation_preloads.keys().copied().collect::<Vec<_>>();
        for stable_id in entities {
            if self.scene.entity_state(stable_id).is_none() {
                self.scene_animation_preloads.remove(&stable_id);
                continue;
            }
            let requests = self.scene_animation_preloads[&stable_id].iter().cloned().collect::<Vec<_>>();
            for (clip, planar_root_lock) in requests {
                if self.load_cached_scene_animation_clip(stable_id, &clip, planar_root_lock)?.is_some() {
                    let pending = self.scene_animation_preloads.get_mut(&stable_id).expect("request retained");
                    pending.remove(&(clip, planar_root_lock));
                    if pending.is_empty() { self.scene_animation_preloads.remove(&stable_id); }
                    return Ok(());
                }
            }
        }
        Ok(())
    }

}
