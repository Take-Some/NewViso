use super::*;

#[derive(Default)]
pub(super) struct PresentationState {
    materialized: bool,
    position: [f32; 3],
    entity_id: Option<u64>,
    materializations: u64,
    dematerializations: u64,
}

// JSON null represents the supported unlimited streaming distance.
pub(super) mod distance_serde {
    use serde::{Deserialize, Serialize};
    pub fn serialize<S: serde::Serializer>(value: &f32, serializer: S) -> Result<S::Ok, S::Error> {
        value.is_finite().then_some(*value).serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
        Ok(Option::<f32>::deserialize(deserializer)?.unwrap_or(f32::INFINITY))
    }
}

impl WorldActorPresentationBinding {
    pub(super) fn validate(&self, actor_id: &str) -> Result<(), String> {
        if actor_id.trim().is_empty() || self.scene_key.trim().is_empty() {
            return Err("world presentation actor and scene key must not be empty".into());
        }
        if self.materialized_representations.is_empty()
            || self
                .materialized_representations
                .iter()
                .any(|v| !matches!(v.as_str(), "physical" | "proxy" | "abstract"))
        {
            return Err(
                "world presentation requires physical, proxy or abstract representations".into(),
            );
        }
        if self
            .position_offset
            .iter()
            .chain(&self.rotation_degrees)
            .chain(&self.scale)
            .chain(&self.bounds_half_extent)
            .chain(&self.base_color)
            .any(|v| !v.is_finite())
            || self.scale.iter().any(|v| *v <= 0.0)
            || self.bounds_half_extent.iter().any(|v| *v <= 0.0)
            || [self.visible_distance, self.stream_distance]
                .iter()
                .any(|v| v.is_nan() || *v <= 0.0)
            || !self.fade_range.is_finite()
            || self.fade_range < 0.0
        {
            return Err("invalid world presentation geometry or streaming distance".into());
        }
        Ok(())
    }
}

impl EngineApplication {
    pub(super) fn sync_world_actor_presentations(&mut self) -> Result<(), String> {
        let actor_views = self.living_world.actor_runtime_views();
        for (actor_id, binding) in &self.world_actor_presentations {
            let view = actor_views.iter().find(|view| &view.id == actor_id);
            let active = view.is_some_and(|view| {
                view.enabled
                    && binding
                        .materialized_representations
                        .iter()
                        .any(|representation| representation == view.representation)
            });
            let state = self
                .world_presentation_states
                .entry(actor_id.clone())
                .or_default();
            if !active {
                if state.materialized && self.scene.runtime_entity_exists(&binding.scene_key) {
                    self.scene
                        .set_runtime_entity_materialized(&binding.scene_key, false)?;
                    state.dematerializations += 1;
                }
                state.materialized = false;
                continue;
            }
            let view = view.expect("active presentation has an actor");
            let position = std::array::from_fn(|i| view.position[i] + binding.position_offset[i]);
            if state.entity_id.is_none() || !self.scene.runtime_entity_exists(&binding.scene_key) {
                state.entity_id = Some(self.scene.upsert_runtime_dynamic_entity(
                    &binding.scene_key,
                    SceneRuntimeEntityDesc {
                        visual: binding.visual,
                        asset_ref: binding.asset_ref.clone(),
                        position,
                        rotation_degrees: binding.rotation_degrees,
                        scale: binding.scale,
                        bounds_half_extent: binding.bounds_half_extent,
                        base_color: binding.base_color,
                        solid: binding.solid,
                        visible_distance: binding.visible_distance,
                        stream_distance: binding.stream_distance,
                        fade_range: binding.fade_range,
                    },
                )?);
            } else if state.position != position {
                self.scene.set_runtime_entity_transform(
                    &binding.scene_key,
                    Some(position),
                    None,
                    None,
                )?;
            }
            if !state.materialized {
                self.scene
                    .set_runtime_entity_materialized(&binding.scene_key, true)?;
                state.materializations += 1;
            }
            state.position = position;
            state.materialized = true;
        }
        Ok(())
    }

    pub(super) fn world_presentations_state(&self) -> Value {
        Value::Array(
            self.world_actor_presentations
                .iter()
                .map(|(actor, binding)| {
                    let state = self.world_presentation_states.get(actor);
                    json!({"actor_id": actor, "scene_key": binding.scene_key,
                "materialized": state.is_some_and(|s| s.materialized),
                "entity_id": state.and_then(|s| s.entity_id),
                "materializations": state.map_or(0, |s| s.materializations),
                "dematerializations": state.map_or(0, |s| s.dematerializations)})
                })
                .collect(),
        )
    }

    pub(super) fn bind_world_actor_presentation(
        &mut self,
        actor_id: String,
        binding: WorldActorPresentationBinding,
    ) -> Result<(), String> {
        binding.validate(&actor_id)?;
        if self
            .world_actor_presentations
            .iter()
            .any(|(id, b)| id != &actor_id && b.scene_key == binding.scene_key)
        {
            return Err("world presentation scene key is already bound to another actor".into());
        }
        if let Some(previous) = self.world_actor_presentations.get(&actor_id) {
            if self.scene.runtime_entity_exists(&previous.scene_key) {
                self.scene
                    .set_runtime_entity_materialized(&previous.scene_key, false)?;
            }
        }
        self.world_presentation_states.remove(&actor_id);
        self.world_actor_presentations.insert(actor_id, binding);
        self.sync_world_actor_presentations()
    }

    pub(super) fn unbind_world_actor_presentation(&mut self, actor_id: &str) -> Result<(), String> {
        if let Some(binding) = self.world_actor_presentations.remove(actor_id.trim()) {
            if self.scene.runtime_entity_exists(&binding.scene_key) {
                self.scene
                    .set_runtime_entity_materialized(&binding.scene_key, false)?;
            }
        }
        self.world_presentation_states.remove(actor_id.trim());
        Ok(())
    }
}
