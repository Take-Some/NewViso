use super::*;

impl EngineApplication {
    pub(super) fn bind_scene_entity_animation(
        &mut self,
        stable_id: u64,
        binding: SceneAnimationBinding,
    ) -> Result<bool, String> {
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
        if !self.scene.entity_model_installed(stable_id) {
            return Ok(false);
        }
        let Some(model_address) = self.scene_stream_claims.get(&stable_id).cloned() else {
            return Ok(false);
        };
        let Some(model) = self.asset_streamer.get::<ModelResource>(&model_address) else {
            return Ok(false);
        };
        let skeleton = model.skeleton.as_ref().ok_or_else(|| {
            format!(
                "scene entity {} model '{}' has no skeleton for animation '{}'",
                stable_id, model.name, binding.clip_ref
            )
        })?;
        let cache_key = (model.id.0, binding.clip_ref.clone());
        let clip = if let Some(clip) = self.animation_clip_cache.get(&cache_key).cloned() {
            clip
        } else {
            let clip = Arc::new(load_model_animation_clip(&binding.clip_ref, skeleton)?);
            self.animation_clip_cache.insert(cache_key, clip.clone());
            clip
        };
        let changed = self.scene.play_entity_animation(
            stable_id,
            clip,
            binding.playback_rate,
            binding.restart_if_same,
        )?;
        let _ = self.scene.set_animation_process_active(
            stable_id,
            "engine.animation.scene_entity",
            true,
        )?;
        Ok(changed)
    }
}
