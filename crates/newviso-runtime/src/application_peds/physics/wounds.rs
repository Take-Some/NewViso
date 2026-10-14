use super::*;

#[derive(Clone, Debug)]
pub(super) struct HitReaction {
    pub clip_ref: String,
    pub elapsed: f32,
    pub duration: f32,
    pub generation: u64,
}

impl EngineApplication {
    pub(crate) fn start_ped_hit_reaction(
        &mut self,
        id: &str,
        push: [f32; 3],
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
        if body.ragdoll.is_some() {
            return Ok(());
        }
        let entity = body.entity;
        body.animation_generation += 1;
        body.hit_reaction = Some(HitReaction {
            clip_ref: clip.clip_ref.clone(),
            elapsed: 0.0,
            duration: clip.duration_seconds,
            generation: body.animation_generation,
        });
        let applied = self.apply_scene_additive_reaction(entity, &clip.clip_ref, 1.0)?;
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
