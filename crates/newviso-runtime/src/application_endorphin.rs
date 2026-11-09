//! Host adapter for Endorphin. Gameplay owns vitals/equipment, Endorphin owns
//! the body model, articulation planning and physical recovery evidence.
use super::*;
use crate::application_peds::physics::PedBody;
use newviso_endorphin::{BodyObservation, EndorphinModel, RecoveryState, RigState};
use std::sync::Arc;
pub(crate) mod audio;
mod control;
mod damage;
mod feet;
mod getup;
pub(crate) mod protection;
mod ragdoll;

pub(super) fn default_model_asset() -> String {
    "physics/endorphin/humanoid.json".into()
}
#[derive(Clone, Debug, Default, serde::Serialize)]
pub(crate) struct GetUpClearance {
    pub valid: bool,
    pub clear: bool,
    pub ground: Option<f32>,
    pub obstacle: Option<u64>,
    pub fraction: Option<f32>,
    pub normal: Option<[f32; 3]>,
}

#[derive(Debug, Default)]
pub(crate) struct EndorphinRuntime {
    pub(crate) protection: protection::ProtectionRuntime,
    feet: feet::FeetRuntime,
    models: BTreeMap<String, Arc<EndorphinModel>>,
    joint_maps: BTreeMap<String, BTreeMap<String, String>>,
}
impl EndorphinRuntime {
    pub(crate) fn character_body_owner(&self, body: u64) -> Option<u64> {
        self.feet.body_owners.get(&body).copied()
    }
    pub(crate) fn weapon_rigs_snapshot(&self) -> Value {
        json!(self
            .models
            .iter()
            .filter_map(|(key, model)| model.weapon_rig.as_ref().map(|rig| (key, rig)))
            .collect::<BTreeMap<_, _>>())
    }
}
#[derive(Debug)]
pub(crate) struct EndorphinActor {
    pub model_asset: String,
    pub model: Arc<EndorphinModel>,
    pub bound_skeleton: Option<Arc<newviso_model::ModelSkeleton>>,
    pub weapon_binding_error: Option<String>,
    pub recovery: RecoveryState,
    pub getup_match: getup::GetUpMatchStatus,
    pub recovery_support: Option<newviso_endorphin::RecoverySupport>,
    pub getup_support: crate::application_peds::physics::recovery_support::GetupSupportStatus,
    getup_pose_cache: Option<getup::GetUpPoseCache>,
    pub getup_clearance: Option<GetUpClearance>,
    pub last_getup_block: Option<GetUpClearance>,
    pub articulation_pending: bool,
    pub ragdoll: Option<RigState>,
    pub damage: newviso_endorphin::BodyDamageState,
    pub control: newviso_endorphin::BodyControlState,
    pub protection_profile: Arc<newviso_endorphin::ProtectionProfile>,
    pub protection_offsets: Vec<[f32; 3]>,
    control_clock: newviso_endorphin::ObservationClock,
    balance_contacts: Vec<[f32; 3]>,
    support_contacts: Vec<newviso_endorphin::SupportContact>,
    contact_points: Vec<[f32; 3]>,
    clock: newviso_endorphin::ObservationClock,
    support_height: Option<f32>,
}
impl EngineApplication {
    pub(crate) fn load_ped_endorphin_model(&mut self, id: &str) -> Result<String, String> {
        let path = self
            .world_actor_presentations
            .get(id)
            .and_then(|p| p.locomotion.as_ref())
            .map(|l| l.endorphin_model.clone())
            .unwrap_or_else(default_model_asset);
        if !self.endorphin.models.contains_key(&path) {
            let value = newviso_assets_client::AssetClient::new().json(&path)?;
            let model: EndorphinModel = serde_json::from_value(value)
                .map_err(|e| format!("invalid Endorphin model '{path}': {e}"))?;
            let joints = model.joint_map.clone();
            let model = model.bind_joints()?;
            self.endorphin.joint_maps.insert(path.clone(), joints);
            self.endorphin.models.insert(path.clone(), Arc::new(model));
        }
        Ok(path)
    }
    pub(crate) fn ped_semantic_joints(
        &mut self,
        id: &str,
    ) -> Result<BTreeMap<String, String>, String> {
        let path = self.load_ped_endorphin_model(id)?;
        Ok(self
            .endorphin
            .joint_maps
            .get(&path)
            .cloned()
            .unwrap_or_default())
    }
    pub(crate) fn create_endorphin_actor(&mut self, id: &str) -> Result<EndorphinActor, String> {
        let path = self.load_ped_endorphin_model(id)?;
        let mut damage = newviso_endorphin::BodyDamageState::default();
        if let Some(saved) = self
            .living_world
            .fact_value(&format!(
                "ped.{:016x}.vitals",
                crate::application_peds::actor_hash(id)
            ))
            .and_then(|v| v.get("body_damage"))
        {
            if !saved.is_null() {
                damage = serde_json::from_value(saved.clone())
                    .map_err(|e| format!("invalid saved body damage: {e}"))?;
            }
        }
        if self.endorphin.models[&path]
            .anatomy
            .as_ref()
            .is_some_and(|a| !damage.valid(a))
        {
            return Err("saved body damage does not match the installed anatomy model".into());
        }
        let protection_profile = self.load_npc_protection(&path)?;
        Ok(EndorphinActor {
            protection_profile,
            protection_offsets: Vec::new(),
            model: self.endorphin.models[&path].clone(),
            model_asset: path,
            bound_skeleton: None,
            weapon_binding_error: Some("skeleton binding pending".into()),
            recovery: RecoveryState::default(),
            getup_match: Default::default(),
            recovery_support: None,
            getup_support: Default::default(),
            getup_pose_cache: None,
            getup_clearance: None,
            last_getup_block: None,
            articulation_pending: false,
            ragdoll: None,
            damage,
            control: Default::default(),
            control_clock: Default::default(),
            balance_contacts: Vec::new(),
            support_contacts: Vec::new(),
            contact_points: Vec::new(),
            clock: Default::default(),
            support_height: None,
        })
    }
    pub(crate) fn observe_ped_endorphin(&mut self, id: &str) {
        let Some(body) = self.ped_physics.bodies.get(id) else {
            return;
        };
        if body.reset_requested || self.peds.is_dead(id) {
            let state = &mut self
                .ped_physics
                .bodies
                .get_mut(id)
                .expect("ped body")
                .endorphin;
            state.recovery.reset();
            state.clock.reset();
            state.support_height = None;
            return;
        }
        let Some(physics) = self.physics.as_ref() else {
            return;
        };
        let (tick, fixed_dt) = physics.fixed_clock();
        let Some(dt) = self
            .ped_physics
            .bodies
            .get_mut(id)
            .expect("ped body")
            .endorphin
            .clock
            .sample(tick, fixed_dt)
        else {
            return;
        };
        let body = &self.ped_physics.bodies[id];
        let Some(pose) = physics.vehicle_body_state(body.entity) else {
            return;
        };
        use newviso_endorphin::ReactionPhase;
        let articulated = body.endorphin.ragdoll.is_some();
        let down = articulated
            || body
                .reaction
                .as_ref()
                .is_some_and(|r| matches!(r.phase, ReactionPhase::Knockdown | ReactionPhase::Down));
        let recovering = down
            || body
                .reaction
                .as_ref()
                .is_some_and(|r| r.phase == ReactionPhase::GetUp);
        let capacity = self.peds.state(id).map_or(1.0, |p| {
            let vitality = (p.health / p.profile.max_health).clamp(0.0, 1.0);
            let stamina = p
                .melee
                .as_ref()
                .zip(p.profile.melee.as_ref())
                .map_or(1.0, |(s, m)| (s.stamina / m.max_stamina).clamp(0.0, 1.0));
            let stance = body.endorphin.model.anatomy.as_ref().map_or(1.0, |a| {
                (body.endorphin.damage.function_capacity(a, "left_stance")
                    + body.endorphin.damage.function_capacity(a, "right_stance"))
                    * 0.5
            });
            (vitality * stamina).sqrt() * stance
        });
        let entities = body
            .endorphin
            .ragdoll
            .as_ref()
            .map_or(std::slice::from_ref(&body.entity), |r| r.entities());
        let motor_support = if articulated || recovering {
            None
        } else {
            physics.character_stair_support(body.entity).map(|step| {
                (
                    step.y - body.radius - body.half_height,
                    newviso_endorphin::Support {
                        normal: step.normal,
                        velocity: step
                            .entity
                            .map_or([0.0; 3], |e| physics.body_linear_velocity(e)),
                    },
                )
            })
        };
        // A controlled transfer uses the measured tread plane. A curved
        // capsule contact normal describes the contact envelope, not body lean.
        let support = motor_support.or_else(|| {
            physics.endorphin_support(
                entities,
                body.endorphin.model.recovery.minimum_support_cosine,
                pose.position[1],
            )
        });
        let motion_energy_per_mass = body.endorphin.ragdoll.as_ref().and_then(|rig| {
            rig.kinetic_energy_per_mass(support.map_or([0.0; 3], |(_, s)| s.velocity), |e| {
                physics
                    .vehicle_body_state(e)
                    .map(|p| newviso_endorphin::BodyPose {
                        position: p.position,
                        rotation: p.rotation,
                        linear_velocity: p.linear_velocity,
                        angular_velocity: p.angular_velocity,
                    })
            })
        });
        let mut observation = BodyObservation {
            rotation: pose.rotation,
            linear_velocity: pose.linear_velocity,
            angular_velocity: pose.angular_velocity,
            radius: body.radius,
            half_height: body.half_height,
            gravity: physics.gravity_magnitude(),
            capacity,
            support: support.map(|(_, s)| s),
            motion_energy_per_mass,
            space_clear: true,
            down,
        };
        self.ped_physics
            .bodies
            .get_mut(id)
            .expect("ped body")
            .endorphin
            .support_height = support.map(|(height, _)| height);
        let clearance = recovering.then(|| self.ped_getup_clearance(id));
        observation.space_clear = !recovering
            || ((!articulated || motion_energy_per_mass.is_some())
                && clearance.as_ref().is_some_and(|c| c.clear));
        let state = &mut self
            .ped_physics
            .bodies
            .get_mut(id)
            .expect("ped body")
            .endorphin;
        state.getup_clearance = clearance;
        state
            .recovery
            .observe(&state.model.recovery, observation, dt);
    }
    pub(crate) fn enter_ped_physical_fall(
        &mut self,
        id: &str,
        push: [f32; 3],
    ) -> Result<(), String> {
        let body = &self.ped_physics.bodies[id];
        let entity = body.entity;
        let damping = body.endorphin.model.ragdoll.angular_damping;
        let point = self
            .physics
            .as_ref()
            .and_then(|p| p.vehicle_body_state(entity))
            .map_or([0.0; 3], |p| p.position);
        if !self.activate_ped_ragdoll(id, push, point, 0.0)? {
            if let Some(physics) = self.physics.as_mut() {
                physics.release_character_body(entity, damping)?;
            }
        }
        Ok(())
    }
    pub(crate) fn ped_support_height(&self, id: &str) -> Option<f32> {
        self.ped_physics.bodies.get(id)?.endorphin.support_height
    }
    pub(crate) fn begin_ped_physical_getup(&mut self, id: &str) -> Result<bool, String> {
        let Some(clips) = self.ped_reaction_clips(id) else {
            return Ok(false);
        };
        let Some(ground) = self.ped_support_height(id) else {
            return Ok(false);
        };
        let body = &self.ped_physics.bodies[id];
        let balanced = body.endorphin.model.anatomy.as_ref().is_some_and(|a| {
            body.endorphin.control.balance.supported
                && body.endorphin.control.balance.uprightness >= a.balance.recovery_upright_cosine
                && body.endorphin.control.balance.support_margin >= 0.0
        });
        if balanced {
            self.clear_ped_ragdoll(id)?;
            self.stand_ped_body(id, ground)?;
            let body = self.ped_physics.bodies.get_mut(id).expect("balanced ped");
            body.reaction = None;
            body.recovery_seconds = 0.0;
            body.endorphin.recovery.reset();
            host::publish_event_json(
                "ped.body.balanced",
                "newviso.endorphin",
                json!({"actor_id":id}),
            )?;
            return Ok(true);
        }
        let Some(selection) = self.select_ped_getup_pose(id, &clips.get_up) else {
            return Ok(false);
        };
        let Some(placement) = self.place_ped_getup(id, selection) else {
            return Ok(false);
        };
        let direction = selection.direction;
        let mut clip = clips.get_up.select(direction);
        clip.duration_seconds = selection.duration_seconds;
        let remaining = clip.duration_seconds - selection.time_seconds;
        let entity = self.ped_physics.bodies[id].entity;
        let Some(loaded) = self.load_cached_scene_animation_clip(entity, &clip.clip_ref, true)?
        else {
            return Ok(false);
        };
        let model = self
            .scene
            .entity_transform_matrix(entity)
            .ok_or("missing recovery model")?;
        let scale: [f32; 3] = std::array::from_fn(|i| {
            newviso_endorphin::math::length([model[i * 4], model[i * 4 + 1], model[i * 4 + 2]])
        });
        let (sin, cos) = (placement.heading_radians * 0.5).sin_cos();
        let mut target =
            newviso_endorphin::math::matrix(placement.model_origin, [0.0, sin, 0.0, cos]);
        for column in 0..3 {
            for row in 0..3 {
                target[column * 4 + row] *= scale[column];
            }
        }
        let body = &self.ped_physics.bodies[id];
        let gravity = self
            .physics
            .as_ref()
            .expect("ped physics")
            .gravity_magnitude();
        let duration = selection.blend_seconds.map_or_else(
            || {
                ((2.0 * (body.radius + body.half_height) / gravity).sqrt()
                    * body.endorphin.model.recovery.response_time_scale)
                    .min(remaining * 0.5)
            },
            |authored| authored.min(remaining),
        );
        let captured = match self.scene.capture_entity_physical_recovery(
            entity,
            target,
            &loaded,
            selection.time_seconds,
            duration,
        ) {
            Ok(pose) => pose,
            Err(error) => {
                self.ped_physics
                    .bodies
                    .get_mut(id)
                    .expect("physical ped")
                    .endorphin
                    .getup_match
                    .errors
                    .push(error);
                return Ok(false);
            }
        };
        let velocity = self.ped_physics.bodies[id]
            .endorphin
            .getup_support
            .current
            .map_or([0.0; 3], |s| s.velocity);
        self.stand_ped_body_with_velocity(id, placement.capsule_center, velocity)?;
        self.clear_ped_ragdoll(id)?;
        self.scene
            .begin_entity_physical_recovery(entity, captured)?;
        let body = self.ped_physics.bodies.get_mut(id).expect("physical ped");
        body.death_heading = placement.heading_radians.to_degrees().rem_euclid(360.0);
        body.animation_generation += 1;
        body.recovery_seconds = remaining;
        body.reaction = Some(newviso_endorphin::ReactionState {
            phase: newviso_endorphin::ReactionPhase::GetUp,
            cause: "physical_recovery",
            direction,
            clip,
            elapsed: selection.time_seconds,
            minimum_vertical_speed: 0.0,
            generation: body.animation_generation,
        });
        body.endorphin.recovery.reset();
        host::publish_event_json(
            "ped.getup.started",
            "newviso.endorphin",
            json!({"actor_id":id,"clip_ref":body.reaction.as_ref().expect("get up").clip.clip_ref,"physical_recovery":true,"match_time_seconds":selection.time_seconds,"placement":placement}),
        )?;
        Ok(true)
    }
    pub(crate) fn endorphin_snapshot(&self, body: &PedBody) -> Value {
        json!({"system":"Endorphin","articulation_pending":body.endorphin.articulation_pending,"model_asset":body.endorphin.model_asset,
            "getup_support":body.endorphin.getup_support,"getup_attachment":body.endorphin.recovery_support,"pose_blend":self.scene.entity_physical_recovery(body.entity),"recovery":body.endorphin.recovery,"getup_match":body.endorphin.getup_match,"getup_clearance":body.endorphin.getup_clearance,"last_getup_block":body.endorphin.last_getup_block,"body_damage":body.endorphin.damage,"control":body.endorphin.control,
            "joint_drives_supported":self.physics.as_ref().is_some_and(|p|p.supports_articulated_drives()),"clock":body.endorphin.clock,"response_time_scale":body.endorphin.model.recovery.response_time_scale})
    }
}
