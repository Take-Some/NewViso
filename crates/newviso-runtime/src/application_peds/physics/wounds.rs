use super::*;

#[derive(Clone, Debug)]
pub(crate) struct HitReaction {
    pub clip_ref: String,
    pub elapsed: f32,
    pub duration: f32,
    pub weight: f32,
    pub generation: u64,
}

impl EngineApplication {
    pub(crate) fn ped_grip_ready(&self, id: &str) -> bool {
        self.ped_physics
            .bodies
            .get(id)
            .is_some_and(|body| !body.reset_requested)
    }

    pub(crate) fn ped_weapon_support(
        &self,
        id: &str,
        model: &newviso_agent::peds::grip::WeaponGripModel,
    ) -> [f32; 2] {
        let animated = model.reaction_support["animated"];
        let Some(body) = self.ped_physics.bodies.get(id) else {
            return animated;
        };
        let state = if body.endorphin.ragdoll.is_some() {
            "ragdoll"
        } else if self.ped_vehicle_falling(id) {
            "fall"
        } else if body.hit_reaction.is_some() {
            "hit"
        } else {
            "animated"
        };
        let mut target = model
            .reaction_support
            .get(state)
            .copied()
            .unwrap_or(animated);
        if body.endorphin.ragdoll.is_some() {
            let c = &body.endorphin.control;
            let authority = if c.balance.supported {
                c.activation * c.balance.uprightness.powi(2) * (1.0 - c.brace)
            } else {
                0.0
            };
            target = std::array::from_fn(|i| target[i] + authority * (animated[i] - target[i]));
        }
        let mut target = if let Some(reaction) = &body.hit_reaction {
            let phase =
                (reaction.elapsed / reaction.duration.max(f32::MIN_POSITIVE)).clamp(0.0, 1.0);
            let envelope = (std::f32::consts::PI * phase).sin() * reaction.weight;
            std::array::from_fn(|i| animated[i] + envelope * (target[i] - animated[i]))
        } else {
            target
        };
        if let Some(anatomy) = &body.endorphin.model.anatomy {
            for (i, chain) in ["right_grip", "left_grip"].iter().enumerate() {
                target[i] *= body.endorphin.damage.function_capacity(anatomy, chain);
            }
        }
        target
    }

    pub(crate) fn start_ped_hit_reaction(
        &mut self,
        id: &str,
        push: [f32; 3],
        damage: f32,
    ) -> Result<(), String> {
        if self.peds.is_dead(id) {
            return Ok(());
        }
        let Some(clips) = self.ped_reaction_clips(id).and_then(|c| c.hit) else {
            return Ok(());
        };
        let heading = self
            .world_presentation_states
            .get(id)
            .map_or(180.0, |s| s.rotation_degrees[1]);
        let model_yaw = self
            .world_actor_presentations
            .get(id)
            .map_or(0.0, |p| p.rotation_degrees[1]);
        let clip = clips.select(animation::impact_direction(push, heading - model_yaw));
        let Some(body) = self.ped_physics.bodies.get_mut(id) else {
            return Ok(());
        };
        if body.endorphin.ragdoll.is_some() {
            return Ok(());
        }
        let fraction = self
            .peds
            .state(id)
            .map_or(0.0, |p| damage / p.profile.max_health);
        let (weight, duration) = body
            .endorphin
            .model
            .hit_response(fraction, clip.duration_seconds);
        self.peds
            .interrupt_combat(id, self.living_world.world_seconds(), duration);
        // Additive hit reactions retain locomotion ownership. The combat
        // interruption supplies stun; fall/get-up recovery owns the body timer.
        let entity = body.entity;
        body.animation_generation += 1;
        body.hit_reaction = Some(HitReaction {
            clip_ref: clip.clip_ref.clone(),
            elapsed: 0.0,
            duration: clip.duration_seconds,
            weight,
            generation: body.animation_generation,
        });
        let applied = self.apply_scene_additive_reaction(entity, &clip.clip_ref, weight)?;
        host::publish_event_json(
            "ped.hit.reaction",
            "newviso.peds",
            json!({"actor_id":id,"clip_ref":clip.clip_ref,"skeletal_applied":applied}),
        )?;
        Ok(())
    }

    pub(super) fn advance_ped_hit_reaction(&mut self, id: &str, dt: f32) {
        let Some(body) = self.ped_physics.bodies.get_mut(id) else {
            return;
        };
        if let Some(reaction) = body.hit_reaction.as_mut() {
            reaction.elapsed += dt;
            if reaction.elapsed >= reaction.duration {
                body.hit_reaction = None;
            }
        }
    }
}
