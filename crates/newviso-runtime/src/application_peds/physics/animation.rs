use super::*;
pub(crate) use newviso_endorphin::{
    capsule_vertical_extent, impact_direction, FallDirection, ReactionClips as PedReactionClips,
    ReactionPhase, ReactionState as PedReactionState,
};

impl EngineApplication {
    pub(crate) fn ped_reaction_clips(&self, id: &str) -> Option<PedReactionClips> {
        self.world_actor_presentations
            .get(id)?
            .locomotion
            .as_ref()?
            .reactions
            .clone()
    }

    pub(super) fn start_ped_fall_animation(
        &mut self,
        id: &str,
        push: [f32; 3],
        cause: &'static str,
    ) -> bool {
        let Some(clips) = self.ped_reaction_clips(id) else {
            return false;
        };
        let model_yaw = self
            .world_actor_presentations
            .get(id)
            .map_or(0.0, |b| b.rotation_degrees[1]);
        let heading = self
            .world_presentation_states
            .get(id)
            .map_or(model_yaw, |s| s.rotation_degrees[1]);
        let interrupted_getup = self.ped_physics.bodies.get(id).is_some_and(|b| {
            b.reaction
                .as_ref()
                .is_some_and(|r| r.phase == ReactionPhase::GetUp)
        });
        let uprightness = if interrupted_getup {
            let entity = self.ped_physics.bodies[id].entity;
            self.scene
                .entity_joint_world_position(entity, "SKEL_Head")
                .ok()
                .zip(
                    self.scene
                        .entity_joint_world_position(entity, "SKEL_Pelvis")
                        .ok(),
                )
                .map(|(head, pelvis)| {
                    let delta: [f32; 3] = std::array::from_fn(|i| head[i] - pelvis[i]);
                    (delta[1] / delta.iter().map(|v| v * v).sum::<f32>().sqrt().max(0.01))
                        .clamp(0.0, 1.0)
                })
                .unwrap_or(1.0)
        } else {
            1.0
        };
        let Some(body) = self.ped_physics.bodies.get_mut(id) else {
            return false;
        };
        if body
            .reaction
            .as_ref()
            .is_some_and(|r| matches!(r.phase, ReactionPhase::Knockdown | ReactionPhase::Down))
        {
            // Suspension contacts continue every tick; they must not restart a clip.
            if cause == "vehicle_runover" {
                body.reaction.as_mut().expect("reaction checked").cause = cause;
            }
            return false;
        }
        body.death_heading = heading;
        let direction = impact_direction(push, heading - model_yaw);
        let clip = if cause == "hard_landing" {
            clips.hard_landing
        } else {
            clips.knockdown.select(direction)
        };
        body.animation_generation += 1;
        // A second impact (or death) must not make a lying ped stand up just
        // to play the first frame of a standing KO. Seek toward its down pose.
        let start_time = clip.duration_seconds * (1.0 - uprightness);
        body.endorphin.recovery.reset();
        body.recovery_seconds = clip.duration_seconds - start_time;
        body.reaction = Some(PedReactionState {
            phase: ReactionPhase::Knockdown,
            cause,
            direction: if cause == "hard_landing" {
                FallDirection::Back
            } else {
                direction
            },
            clip,
            elapsed: start_time,
            minimum_vertical_speed: 0.0,
            generation: body.animation_generation,
        });
        true
    }

    pub(crate) fn ped_reaction_animation(
        &self,
        id: &str,
    ) -> Option<(&'static str, SceneAnimationBinding, u64)> {
        let reaction = self.ped_physics.bodies.get(id)?.reaction.as_ref()?;
        let death_clip = self
            .peds
            .state(id)
            .filter(|p| p.dead)
            .and_then(|p| p.profile.death_clip.as_ref());
        Some((
            reaction.phase.as_str(),
            SceneAnimationBinding {
                clip_ref: death_clip.unwrap_or(&reaction.clip.clip_ref).clone(),
                planar_root_lock: true,
                playback_rate: 1.0,
                restart_if_same: true,
                start_time_seconds: if death_clip.is_some() {
                    0.0
                } else {
                    reaction.clip_time()
                },
                blend_seconds: 0.16,
                bound_elapsed_seconds: 0.0,
                apply_mover: false,
            },
            reaction.generation,
        ))
    }

    pub(super) fn advance_ped_animation(
        &mut self,
        id: &str,
        dt: f32,
        dead: bool,
    ) -> Result<(), String> {
        let Some(clips) = self.ped_reaction_clips(id) else {
            return Ok(());
        };
        let entity = self.ped_physics.bodies[id].entity;
        if !self.advance_ped_recovery_support(id)? {
            return Ok(());
        }
        let Some(pose) = self
            .physics
            .as_ref()
            .and_then(|p| p.vehicle_body_state(entity))
        else {
            return Ok(());
        };
        let ground = if self.ped_physical_getup_active(id) {
            self.ped_getup_observed_ground(id)
        } else {
            self.physics
                .as_ref()
                .and_then(|p| p.character_ground_contact_height(entity))
        };
        let phase = self.ped_physics.bodies[id]
            .reaction
            .as_ref()
            .map(|r| r.phase);
        if dead && !matches!(phase, Some(ReactionPhase::Knockdown | ReactionPhase::Down)) {
            self.start_ped_fall_animation(id, [0.0, 0.0, -1.0], "death");
        } else if !dead && phase.is_none() {
            let physical_response = &self.ped_physics.bodies[id].endorphin.recovery;
            let tilted = physical_response.tipped;
            if tilted {
                let direction = [pose.rotation[2], 0.0, -pose.rotation[0]];
                self.start_ped_fall_animation(id, direction, "tipped");
                self.enter_ped_physical_fall(id, direction)?;
                return Ok(());
            } else if physical_response.falling {
                let body = self.ped_physics.bodies.get_mut(id).expect("ped body");
                body.death_heading = self
                    .world_presentation_states
                    .get(id)
                    .map_or(180.0, |s| s.rotation_degrees[1]);
                body.animation_generation += 1;
                body.reaction = Some(PedReactionState {
                    phase: ReactionPhase::Airborne,
                    cause: "falling",
                    direction: FallDirection::Back,
                    clip: clips.airborne.clone(),
                    elapsed: 0.0,
                    minimum_vertical_speed: pose.linear_velocity[1],
                    generation: body.animation_generation,
                });
            }
        }
        let reaction = {
            let Some(reaction) = self
                .ped_physics
                .bodies
                .get_mut(id)
                .and_then(|b| b.reaction.as_mut())
            else {
                return Ok(());
            };
            reaction.elapsed += dt;
            reaction.minimum_vertical_speed =
                reaction.minimum_vertical_speed.min(pose.linear_velocity[1]);
            reaction.clone()
        };
        if !dead
            && reaction.phase == ReactionPhase::Airborne
            && reaction.cause != "high_fall"
            && ground.is_none()
            && self.ped_physics.bodies[id].endorphin.recovery.high_fall
        {
            self.ped_physics
                .bodies
                .get_mut(id)
                .expect("airborne ped")
                .reaction
                .as_mut()
                .expect("airborne reaction")
                .cause = "high_fall";
            // Transfer the current momentum in mid-air. No additional launch impulse.
            self.enter_ped_physical_fall(id, [0.0; 3])?;
            return Ok(());
        }
        if reaction.phase == ReactionPhase::Airborne && ground.is_some() {
            if self.ped_physics.bodies[id].endorphin.recovery.hard_landing {
                self.start_ped_fall_animation(id, [0.0, 0.0, 1.0], "hard_landing");
                self.enter_ped_physical_fall(id, [0.0, 0.0, 1.0])?;
            } else {
                let body = self.ped_physics.bodies.get_mut(id).expect("ped body");
                let reaction = body.reaction.as_mut().expect("airborne reaction");
                reaction.phase = ReactionPhase::Landing;
                reaction.clip = clips.landing;
                reaction.elapsed = 0.0;
                body.animation_generation += 1;
                reaction.generation = body.animation_generation;
                body.recovery_seconds = reaction.clip.duration_seconds;
            }
        } else if reaction.phase == ReactionPhase::Knockdown
            && reaction.elapsed >= reaction.clip.duration_seconds
        {
            let reaction = self
                .ped_physics
                .bodies
                .get_mut(id)
                .expect("ped body")
                .reaction
                .as_mut()
                .expect("reaction");
            reaction.phase = ReactionPhase::Down;
            reaction.elapsed = 0.0;
        } else if !dead
            && reaction.phase == ReactionPhase::Down
            && self.ped_physics.bodies[id].recovery_seconds <= 0.0
        {
            if let Some(_ground) =
                ground.filter(|_| self.ped_physics.bodies[id].endorphin.recovery.ready)
            {
                let body = self.ped_physics.bodies.get_mut(id).expect("ped body");
                let reaction = body.reaction.as_mut().expect("down reaction");
                reaction.clip = clips.get_up.select(reaction.direction);
                reaction.phase = ReactionPhase::GetUp;
                reaction.elapsed = 0.0;
                body.animation_generation += 1;
                reaction.generation = body.animation_generation;
                body.recovery_seconds = reaction.clip.duration_seconds;
                host::publish_event_json(
                    "ped.getup.started",
                    "newviso.peds",
                    json!({"actor_id": id, "clip_ref": reaction.clip.clip_ref}),
                )?;
            }
        } else if !dead
            && reaction.phase == ReactionPhase::GetUp
            && self.ped_physics.bodies[id].endorphin.recovery.falling
            && reaction.cause != "physical_recovery"
        {
            // A real loss of support invalidates the animated recovery. Transfer
            // the latest blended pose and native momentum without another push.
            self.start_ped_fall_animation(id, [0.0; 3], "recovery_support_lost");
            self.enter_ped_physical_fall(id, [0.0; 3])?;
        } else if !dead && reaction.phase == ReactionPhase::GetUp && !self.ped_getup_space_clear(id)
        {
            let obstruction = self.ped_getup_clearance(id);
            self.ped_physics
                .bodies
                .get_mut(id)
                .expect("recovering ped")
                .endorphin
                .last_getup_block = Some(obstruction);
            self.start_ped_fall_animation(id, [0.0, 0.0, 1.0], "blocked_getup");
            self.enter_ped_physical_fall(id, [0.0, 0.0, 1.0])?;
        } else if !dead
            && matches!(
                reaction.phase,
                ReactionPhase::GetUp | ReactionPhase::Landing
            )
            && reaction.elapsed >= reaction.clip.duration_seconds
        {
            if reaction.phase == ReactionPhase::GetUp {
                if let Some(ground) = self
                    .ped_getup_support_sample(id)
                    .map(|s| s.ground_point[1])
                    .or(ground)
                {
                    self.stand_ped_body(id, ground)?;
                }
            }
            self.finish_ped_getup(id)?;
        } else if !dead && reaction.phase == ReactionPhase::GetUp {
            self.drive_ped_getup_body(id)?;
        }
        Ok(())
    }
}
