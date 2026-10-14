use super::recovery::ped_body_settled;
use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct ReactionClip {
    pub clip_ref: String,
    pub duration_seconds: f32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct DirectionalClips {
    front: ReactionClip,
    back: ReactionClip,
    left: ReactionClip,
    right: ReactionClip,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct PedReactionClips {
    knockdown: DirectionalClips,
    // Each entry matches the FINAL pose of its corresponding knockdown clip.
    // A front impact may finish on the back, so names are impact directions.
    get_up: DirectionalClips,
    airborne: ReactionClip,
    landing: ReactionClip,
    hard_landing: ReactionClip,
    #[serde(default)]
    pub(super) hit: Option<DirectionalClips>,
}

impl PedReactionClips {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(hit) = &self.hit {
            for c in [&hit.front, &hit.back, &hit.left, &hit.right] {
                if c.clip_ref.trim().is_empty()
                    || !c.duration_seconds.is_finite()
                    || !(0.05..=60.0).contains(&c.duration_seconds)
                {
                    return Err("invalid hit reaction clip".into());
                }
            }
        }
        for clip in [
            &self.knockdown.front,
            &self.knockdown.back,
            &self.knockdown.left,
            &self.knockdown.right,
            &self.get_up.front,
            &self.get_up.back,
            &self.get_up.left,
            &self.get_up.right,
            &self.airborne,
            &self.landing,
            &self.hard_landing,
        ] {
            if clip.clip_ref.trim().is_empty()
                || clip.clip_ref.len() > 256
                || !clip.clip_ref.contains('@')
                || !clip.duration_seconds.is_finite()
                || !(0.05..=60.0).contains(&clip.duration_seconds)
            {
                return Err("invalid pedestrian reaction clip or duration".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FallDirection {
    Front,
    Back,
    Left,
    Right,
}

impl DirectionalClips {
    pub(super) fn select(&self, direction: FallDirection) -> ReactionClip {
        match direction {
            FallDirection::Front => &self.front,
            FallDirection::Back => &self.back,
            FallDirection::Left => &self.left,
            FallDirection::Right => &self.right,
        }
        .clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReactionPhase {
    Airborne,
    Knockdown,
    Down,
    GetUp,
    Landing,
}

impl ReactionPhase {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Airborne => "airborne",
            Self::Knockdown => "fall",
            Self::Down => "down",
            Self::GetUp => "get_up",
            Self::Landing => "land",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct PedReactionState {
    pub phase: ReactionPhase,
    pub cause: &'static str,
    pub direction: FallDirection,
    pub clip: ReactionClip,
    pub elapsed: f32,
    pub minimum_vertical_speed: f32,
    pub generation: u64,
}

impl PedReactionState {
    pub(super) fn clip_time(&self) -> f32 {
        if self.phase == ReactionPhase::Down {
            self.clip.duration_seconds
        } else {
            self.elapsed.min(self.clip.duration_seconds)
        }
    }
}

pub(super) fn impact_direction(push: [f32; 3], heading: f32) -> FallDirection {
    let (s, c) = heading.to_radians().sin_cos();
    let front = -push[0] * s - push[2] * c;
    let right = -push[0] * c + push[2] * s;
    if front.abs() >= right.abs() {
        if front >= 0.0 {
            FallDirection::Front
        } else {
            FallDirection::Back
        }
    } else if right >= 0.0 {
        FallDirection::Right
    } else {
        FallDirection::Left
    }
}

pub(super) fn capsule_vertical_extent(rotation: [f32; 4], radius: f32, half_height: f32) -> f32 {
    let [x, _, z, _] = rotation;
    radius + half_height * (1.0 - 2.0 * (x * x + z * z)).abs().min(1.0)
}

impl EngineApplication {
    pub(super) fn ped_reaction_clips(&self, id: &str) -> Option<PedReactionClips> {
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
        body.recovery_seconds = VEHICLE_FALL_SECONDS + clip.duration_seconds - start_time;
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
        let Some(pose) = self
            .physics
            .as_ref()
            .and_then(|p| p.vehicle_body_state(entity))
        else {
            return Ok(());
        };
        let ground = self
            .physics
            .as_ref()
            .and_then(|p| p.character_ground_contact_height(entity));
        let phase = self.ped_physics.bodies[id]
            .reaction
            .as_ref()
            .map(|r| r.phase);
        if dead && !matches!(phase, Some(ReactionPhase::Knockdown | ReactionPhase::Down)) {
            self.start_ped_fall_animation(id, [0.0, 0.0, -1.0], "death");
        } else if !dead && phase.is_none() {
            let tilted = capsule_vertical_extent(pose.rotation, 0.0, 1.0) < 0.70;
            if tilted {
                let direction = [pose.rotation[2], 0.0, -pose.rotation[0]];
                self.start_ped_fall_animation(id, direction, "tipped");
            } else if ground.is_none() && pose.linear_velocity[1] < -2.0 {
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
        if reaction.phase == ReactionPhase::Airborne && ground.is_some() && reaction.elapsed > 0.08
        {
            if reaction.minimum_vertical_speed < -5.0 {
                self.start_ped_fall_animation(id, [0.0, 0.0, 1.0], "hard_landing");
                self.physics
                    .as_mut()
                    .expect("physics")
                    .release_character_body(entity)?;
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
            if let Some(_ground) = ground.filter(|_| {
                self.ped_getup_space_clear(id) && ped_body_settled(pose.linear_velocity)
            }) {
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
        } else if !dead && reaction.phase == ReactionPhase::GetUp && !self.ped_getup_space_clear(id)
        {
            self.start_ped_fall_animation(id, [0.0, 0.0, 1.0], "blocked_getup");
            self.physics
                .as_mut()
                .expect("physics")
                .release_character_body(entity)?;
        } else if !dead
            && matches!(
                reaction.phase,
                ReactionPhase::GetUp | ReactionPhase::Landing
            )
            && reaction.elapsed >= reaction.clip.duration_seconds
        {
            if reaction.phase == ReactionPhase::GetUp {
                if let Some(ground) = ground {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn impact_directions_follow_actor_heading_and_incoming_force() {
        assert_eq!(
            impact_direction([0.0, 0.0, -5.0], 0.0),
            FallDirection::Front
        );
        assert_eq!(impact_direction([0.0, 0.0, 5.0], 0.0), FallDirection::Back);
        assert_eq!(impact_direction([5.0, 0.0, 0.0], 0.0), FallDirection::Left);
        assert_eq!(
            impact_direction([-5.0, 0.0, 0.0], 0.0),
            FallDirection::Right
        );
        assert_eq!(
            impact_direction([-5.0, 0.0, 0.0], 90.0),
            FallDirection::Front
        );
    }
    #[test]
    fn presentation_anchor_accounts_for_tipped_capsule_extent() {
        assert!((capsule_vertical_extent([0.0, 0.0, 0.0, 1.0], 0.3, 0.6) - 0.9).abs() < 1e-5);
        assert!(
            (capsule_vertical_extent([0.70710677, 0.0, 0.0, 0.70710677], 0.3, 0.6) - 0.3).abs()
                < 1e-5
        );
    }
}
