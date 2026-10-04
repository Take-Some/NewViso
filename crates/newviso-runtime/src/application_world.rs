use super::*;

#[derive(Default)]
pub(super) struct PresentationState {
    materialized: bool,
    position: [f32; 3],
    rotation_degrees: [f32; 3],
    velocity: [f32; 3],
    speed: f32,
    locomotion_state: String,
    animation_clip: Option<String>,
    entity_id: Option<u64>,
    materializations: u64,
    dematerializations: u64,
    was_grounded: Option<bool>,
    land_until_seconds: f64,
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
        if let Some(locomotion) = &self.locomotion {
            if !locomotion.walk_speed_threshold.is_finite()
                || locomotion.walk_speed_threshold < 0.0
                || !locomotion.run_speed_threshold.is_finite()
                || locomotion.run_speed_threshold < locomotion.walk_speed_threshold
                || !locomotion.animation_rate_hz.is_finite()
                || !(1.0..=240.0).contains(&locomotion.animation_rate_hz)
                || [
                    locomotion.idle_clip.as_deref(),
                    locomotion.walk_clip.as_deref(),
                    locomotion.run_clip.as_deref(),
                    locomotion.jump_clip.as_deref(),
                    locomotion.fall_clip.as_deref(),
                    locomotion.land_clip.as_deref(),
                ]
                .into_iter()
                .flatten()
                .any(|clip| clip.trim().is_empty() || clip.len() > 256)
            {
                return Err("invalid world presentation locomotion parameters".into());
            }
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

fn heading_degrees_from_velocity(velocity: [f32; 3]) -> Option<f32> {
    let horizontal_speed_sq = velocity[0] * velocity[0] + velocity[2] * velocity[2];
    (horizontal_speed_sq > 1.0e-10).then(|| velocity[0].atan2(velocity[2]).to_degrees())
}

fn sample_locomotion(
    binding: &WorldActorPresentationBinding,
    velocity: [f32; 3],
    grounded: Option<bool>,
    landing_latched: bool,
) -> (f32, f32, &'static str, Option<String>) {
    let horizontal_speed = (velocity[0] * velocity[0] + velocity[2] * velocity[2]).sqrt();
    let heading = heading_degrees_from_velocity(velocity).unwrap_or(0.0);
    let Some(locomotion) = &binding.locomotion else {
        return (horizontal_speed, heading, "none", None);
    };

    if landing_latched {
        if let Some(clip) = locomotion.land_clip.clone() {
            return (horizontal_speed, heading, "land", Some(clip));
        }
    }

    if grounded == Some(false) {
        if velocity[1] > 0.10 {
            return (
                horizontal_speed,
                heading,
                "jump",
                locomotion
                    .jump_clip
                    .clone()
                    .or_else(|| locomotion.fall_clip.clone())
                    .or_else(|| locomotion.run_clip.clone())
                    .or_else(|| locomotion.walk_clip.clone()),
            );
        }
        return (
            horizontal_speed,
            heading,
            "fall",
            locomotion
                .fall_clip
                .clone()
                .or_else(|| locomotion.jump_clip.clone())
                .or_else(|| locomotion.run_clip.clone())
                .or_else(|| locomotion.walk_clip.clone()),
        );
    }

    if horizontal_speed >= locomotion.run_speed_threshold {
        (
            horizontal_speed,
            heading,
            "run",
            locomotion
                .run_clip
                .clone()
                .or_else(|| locomotion.walk_clip.clone()),
        )
    } else if horizontal_speed >= locomotion.walk_speed_threshold {
        (
            horizontal_speed,
            heading,
            "walk",
            locomotion.walk_clip.clone(),
        )
    } else {
        (
            horizontal_speed,
            heading,
            "idle",
            locomotion.idle_clip.clone(),
        )
    }
}

impl EngineApplication {
    pub(super) fn sync_world_actor_presentations(&mut self) -> Result<(), String> {
        const ANIMATION_OWNER: &str = "engine.animation.world_actor";
        const LAND_PRESENTATION_SECONDS: f64 = 0.34;
        let now_seconds = self.elapsed_seconds;
        let actor_views = self.living_world.actor_runtime_views();
        let mut animation_actions = Vec::<(u64, Option<SceneAnimationBinding>)>::new();
        for (actor_id, binding) in &self.world_actor_presentations {
            let view = actor_views.iter().find(|view| &view.id == actor_id);
            let physical_state = self.physical_characters.state(actor_id);
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
                    if let Some(entity_id) = state.entity_id {
                        let _ = self.scene.set_animation_process_active(
                            entity_id,
                            ANIMATION_OWNER,
                            false,
                        )?;
                    }
                    self.scene
                        .set_runtime_entity_materialized(&binding.scene_key, false)?;
                    state.dematerializations += 1;
                }
                if let Some(entity_id) = state.entity_id {
                    if state.animation_clip.is_some() {
                        animation_actions.push((entity_id, None));
                    }
                }
                state.materialized = false;
                state.velocity = [0.0; 3];
                state.speed = 0.0;
                state.locomotion_state = "abstract".to_owned();
                state.animation_clip = None;
                state.was_grounded = None;
                state.land_until_seconds = 0.0;
                continue;
            }

            let view = view.expect("active presentation has an actor");
            let position = std::array::from_fn(|i| view.position[i] + binding.position_offset[i]);
            let grounded = physical_state.map(|physical| physical.grounded);
            if state.was_grounded == Some(false) && grounded == Some(true) {
                if binding
                    .locomotion
                    .as_ref()
                    .and_then(|locomotion| locomotion.land_clip.as_ref())
                    .is_some()
                {
                    state.land_until_seconds = now_seconds + LAND_PRESENTATION_SECONDS;
                }
            } else if grounded == Some(false) {
                state.land_until_seconds = 0.0;
            }
            let landing_latched = grounded == Some(true) && now_seconds < state.land_until_seconds;
            let resolved_velocity = physical_state
                .map(|physical| physical.velocity)
                .unwrap_or(view.velocity);
            let (speed, velocity_heading, mut locomotion_state, mut animation_clip) =
                sample_locomotion(binding, resolved_velocity, grounded, landing_latched);
            let facing_velocity = self
                .physical_characters
                .presentation_facing_velocity(actor_id)
                .unwrap_or(resolved_velocity);
            let heading =
                heading_degrees_from_velocity(facing_velocity).unwrap_or(velocity_heading);
            let mut rotation_degrees = binding.rotation_degrees;
            if binding
                .locomotion
                .as_ref()
                .is_some_and(|locomotion| locomotion.face_velocity)
            {
                if speed > 1.0e-5 {
                    rotation_degrees[1] += heading;
                } else if state.materialized {
                    rotation_degrees[1] = state.rotation_degrees[1];
                }
            }

            let intent = self.agents.presentation(actor_id, view.position);
            if let Some(heading) = intent.heading_degrees {
                rotation_degrees[1] = binding.rotation_degrees[1] + heading;
            }
            if let Some(clip) = intent.clip_ref { animation_clip = Some(clip); locomotion_state = "action"; }
            if let Some(ped) = self.peds.state(actor_id).filter(|p| p.dead) {
                animation_clip = ped.profile.death_clip.clone().or(animation_clip);
                locomotion_state = "dead";
            }

            let created =
                state.entity_id.is_none() || !self.scene.runtime_entity_exists(&binding.scene_key);
            if created {
                state.entity_id = Some(self.scene.upsert_runtime_dynamic_entity(
                    &binding.scene_key,
                    SceneRuntimeEntityDesc {
                        visual: binding.visual,
                        asset_ref: binding.asset_ref.clone(),
                        texture_dictionary: None,
                        position,
                        rotation_degrees,
                        scale: binding.scale,
                        bounds_half_extent: binding.bounds_half_extent,
                        base_color: binding.base_color,
                        solid: binding.solid,
                        visible_distance: binding.visible_distance,
                        stream_distance: binding.stream_distance,
                        fade_range: binding.fade_range,
                    },
                )?);
            } else if state.position != position || state.rotation_degrees != rotation_degrees {
                self.scene.set_runtime_entity_transform(
                    &binding.scene_key,
                    Some(position),
                    Some(rotation_degrees),
                    None,
                )?;
            }

            if let Some(entity_id) = state.entity_id {
                if let Some(locomotion) = &binding.locomotion {
                    if created || !state.materialized {
                        self.scene.set_entity_process_rate_hz(
                            entity_id,
                            "animation",
                            locomotion.animation_rate_hz,
                        )?;
                    }
                    let _ = self.scene.set_animation_process_active(
                        entity_id,
                        ANIMATION_OWNER,
                        true,
                    )?;
                } else {
                    let _ = self.scene.set_animation_process_active(
                        entity_id,
                        ANIMATION_OWNER,
                        false,
                    )?;
                }
            }

            if !state.materialized {
                self.scene
                    .set_runtime_entity_materialized(&binding.scene_key, true)?;
                state.materializations += 1;
            }
            let animation_changed = state.animation_clip != animation_clip;
            if let Some(entity_id) = state.entity_id {
                if animation_changed || created || !state.materialized {
                    animation_actions.push((
                        entity_id,
                        animation_clip
                            .as_ref()
                            .map(|clip_ref| SceneAnimationBinding {
                                clip_ref: clip_ref.clone(),
                                playback_rate: 1.0,
                                restart_if_same: false,
                                start_time_seconds: 0.0,
                                blend_seconds: 0.18,
                                bound_elapsed_seconds: 0.0,
                                apply_mover: false,
                            }),
                    ));
                }
            }
            state.position = position;
            state.rotation_degrees = rotation_degrees;
            state.velocity = resolved_velocity;
            state.speed = speed;
            state.locomotion_state = locomotion_state.to_owned();
            state.animation_clip = animation_clip;
            state.was_grounded = grounded;
            state.materialized = true;
        }

        // WorldActorPresentation decides the locomotion state; the generic
        // scene animation bridge owns clip decoding, caching and playback.
        // Queueing these actions until after the presentation-map iteration
        // avoids borrowing the whole EngineApplication while a state entry is
        // mutably borrowed. Bindings are retained even when the model is still
        // streaming; application_streaming applies them after skin installation.
        for (entity_id, animation) in animation_actions {
            match animation {
                Some(binding) => {
                    let _ = self.bind_scene_entity_animation(entity_id, binding)?;
                }
                None => {
                    let _ = self.unbind_scene_entity_animation(entity_id)?;
                }
            }
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
                "velocity": state.map_or([0.0; 3], |s| s.velocity),
                "speed": state.map_or(0.0, |s| s.speed),
                "heading_degrees": state.map_or(0.0, |s| s.rotation_degrees[1]),
                "facing_velocity": self.physical_characters
                    .presentation_facing_velocity(actor),
                "locomotion_state": state.map_or("none", |s| s.locomotion_state.as_str()),
                "animation_clip": state.and_then(|s| s.animation_clip.as_deref()),
                "grounded": self.physical_characters.state(actor).map(|s| s.grounded),
                "air_phase": self.physical_characters.state(actor).map(|s| {
                    if s.grounded { "grounded" } else if s.velocity[1] > 0.10 { "jump" } else { "fall" }
                }),
                "animation_rate_hz": binding.locomotion.as_ref().map(|v| v.animation_rate_hz),
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
                if let Some(entity_id) = self
                    .world_presentation_states
                    .get(&actor_id)
                    .and_then(|state| state.entity_id)
                {
                    let _ = self.scene.set_animation_process_active(
                        entity_id,
                        "engine.animation.world_actor",
                        false,
                    )?;
                }
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
                if let Some(entity_id) = self
                    .world_presentation_states
                    .get(actor_id.trim())
                    .and_then(|state| state.entity_id)
                {
                    let _ = self.scene.set_animation_process_active(
                        entity_id,
                        "engine.animation.world_actor",
                        false,
                    )?;
                }
                self.scene
                    .set_runtime_entity_materialized(&binding.scene_key, false)?;
            }
        }
        self.world_presentation_states.remove(actor_id.trim());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locomotion_binding() -> WorldActorPresentationBinding {
        WorldActorPresentationBinding {
            scene_key: "npc.test".to_owned(),
            visual: SceneRuntimeVisualKind::None,
            asset_ref: Some("models/npc.ydd".to_owned()),
            position_offset: [0.0; 3],
            rotation_degrees: [0.0; 3],
            scale: [1.0; 3],
            bounds_half_extent: [0.4, 0.9, 0.4],
            base_color: [1.0; 4],
            solid: false,
            visible_distance: 100.0,
            stream_distance: 120.0,
            fade_range: 10.0,
            materialized_representations: vec!["physical".to_owned(), "proxy".to_owned()],
            locomotion: Some(WorldActorLocomotionBinding {
                idle_clip: Some("idle".to_owned()),
                walk_clip: Some("walk".to_owned()),
                run_clip: Some("run".to_owned()),
                jump_clip: Some("jump".to_owned()),
                fall_clip: Some("fall".to_owned()),
                land_clip: Some("land".to_owned()),
                walk_speed_threshold: 0.15,
                run_speed_threshold: 3.5,
                animation_rate_hz: 30.0,
                face_velocity: true,
            }),
        }
    }

    #[test]
    fn locomotion_selects_idle_walk_run_and_heading() {
        let binding = locomotion_binding();

        let (speed, heading, state, clip) =
            sample_locomotion(&binding, [0.0, 0.0, 0.0], Some(true), false);
        assert_eq!(speed, 0.0);
        assert_eq!(heading, 0.0);
        assert_eq!(state, "idle");
        assert_eq!(clip.as_deref(), Some("idle"));

        let (speed, heading, state, clip) =
            sample_locomotion(&binding, [2.0, 0.0, 0.0], Some(true), false);
        assert!((speed - 2.0).abs() < 1.0e-6);
        assert!((heading - 90.0).abs() < 1.0e-5);
        assert_eq!(state, "walk");
        assert_eq!(clip.as_deref(), Some("walk"));

        let (speed, _, state, clip) =
            sample_locomotion(&binding, [0.0, 0.0, -4.0], Some(true), false);
        assert!((speed - 4.0).abs() < 1.0e-6);
        assert_eq!(state, "run");
        assert_eq!(clip.as_deref(), Some("run"));

        let (_, _, state, clip) = sample_locomotion(&binding, [0.0, 4.5, 0.0], Some(false), false);
        assert_eq!(state, "jump");
        assert_eq!(clip.as_deref(), Some("jump"));

        let (_, _, state, clip) = sample_locomotion(&binding, [0.0, -2.0, 0.0], Some(false), false);
        assert_eq!(state, "fall");
        assert_eq!(clip.as_deref(), Some("fall"));

        let (_, _, state, clip) = sample_locomotion(&binding, [0.0, 0.0, 0.0], Some(true), true);
        assert_eq!(state, "land");
        assert_eq!(clip.as_deref(), Some("land"));
    }

    #[test]
    fn legacy_presentation_binding_without_locomotion_deserializes() {
        let mut value = serde_json::to_value(locomotion_binding()).unwrap();
        value.as_object_mut().unwrap().remove("locomotion");
        let binding: WorldActorPresentationBinding = serde_json::from_value(value).unwrap();
        assert!(binding.locomotion.is_none());
        binding.validate("npc.legacy").unwrap();
    }
}
