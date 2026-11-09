//! Shared runtime protective reaching for every resident NPC and the configured player.
use super::*;
use newviso_endorphin::{math as em, ProtectionProfile, ProtectionState};
mod binding;
pub(crate) mod physical;
#[derive(Debug, Default)]
pub(crate) struct ProtectionRuntime {
    profiles: BTreeMap<String, Arc<ProtectionProfile>>,
    actors: BTreeMap<u64, ProtectedActor>,
}
#[derive(Debug, Default)]
struct ProtectedActor {
    actor_id: String,
    body_entity: u64,
    profile_key: String,
    skeleton: Option<Arc<newviso_model::ModelSkeleton>>,
    error: Option<String>,
    observation_error: Option<String>,
    clock: newviso_endorphin::ObservationClock,
    state: ProtectionState,
    mode: &'static str,
}
impl ProtectionRuntime {
    pub(crate) fn blocks_source(&self, entity: u64) -> bool {
        self.actors
            .values()
            .any(|a| a.body_entity == entity && a.state.active())
    }
    pub(crate) fn blocks_actor(&self, id: &str) -> bool {
        self.actors
            .values()
            .any(|a| a.actor_id == id && a.state.active())
    }
    pub(crate) fn snapshot(&self, scene: &newviso_scene::Scene3dRuntime) -> Value {
        json!({"actors":self.actors.iter().map(|(entity,a)| {
            let profile=self.profiles.get(&a.profile_key);
            let achieved=profile.filter(|_|a.state.active()).map(|p| {
                let matrices=scene.entity_joint_world_matrices(*entity,&p.joints()).ok();
                p.arms.iter().map(|arm|{
                let point=|name:&str|matrices.as_ref().and_then(|m|m.get(name)).map(|m|em::position(*m));
                let palm_normal=matrices.as_ref().and_then(|m|m.get(&arm.hand_joint)).map(|m|em::rotate(em::rotation(*m),arm.palm_normal_local));
                json!({"hand":arm.hand_joint,"shoulder":point(&arm.upper_joint),"elbow":point(&arm.lower_joint),
                    "wrist":point(&arm.hand_joint),"palm":point(&arm.contact_joint),"palm_normal":palm_normal})
            }).collect::<Vec<_>>() }).unwrap_or_default();
            json!({"actor_id":a.actor_id,"entity_key":entity.to_string(),"body_entity":a.body_entity,
                "profile":a.profile_key,"mode":a.mode,"ready":a.error.is_none()&&a.observation_error.is_none()&&a.skeleton.is_some(),"error":a.error.as_ref().or(a.observation_error.as_ref()),
                "combat_reserved":a.state.active(),"state":a.state,"achieved":achieved})
        }).collect::<Vec<_>>()})
    }
}
impl EngineApplication {
    fn load_protection_asset(
        &mut self,
        key: &str,
        asset: &str,
    ) -> Result<Arc<ProtectionProfile>, String> {
        if let Some(p) = self.endorphin.protection.profiles.get(key) {
            return Ok(p.clone());
        }
        let value = newviso_assets_client::AssetClient::new().json(asset)?;
        let profile: ProtectionProfile = serde_json::from_value(value)
            .map_err(|e| format!("invalid protection profile '{asset}': {e}"))?;
        profile.validate()?;
        let profile = Arc::new(profile);
        self.endorphin
            .protection
            .profiles
            .insert(key.into(), profile.clone());
        Ok(profile)
    }
    pub(crate) fn load_npc_protection(
        &mut self,
        path: &str,
    ) -> Result<Arc<ProtectionProfile>, String> {
        let key = format!("npc:{path}");
        if let Some(p) = self.endorphin.protection.profiles.get(&key) {
            return Ok(p.clone());
        }
        if let Some(profile) = self
            .endorphin
            .models
            .get(path)
            .and_then(|m| m.protection.clone())
        {
            profile.validate()?;
            let p = Arc::new(profile);
            self.endorphin.protection.profiles.insert(key, p.clone());
            return Ok(p);
        }
        self.load_protection_asset(&key, "physics/endorphin/protection_humanoid.json")
    }
    pub(crate) fn protective_combat_blocked(&self, id: &str) -> bool {
        self.endorphin.protection.blocks_actor(id)
    }
    pub(crate) fn update_protective_reaching(&mut self) -> Result<(), String> {
        let Some(physics) = self.physics.as_ref() else {
            return Ok(());
        };
        let (tick, fixed_dt) = physics.fixed_clock();
        let solids = physics
            .scene_collider_interests()
            .map(|bounds| self.scene.physics_static_solid_colliders_near(&bounds))
            .unwrap_or_default()
            .into_iter()
            .map(|(_, a, b)| (a, b))
            .collect::<Vec<_>>();
        let mut infos = self
            .ped_physics
            .bodies
            .iter()
            .map(|(id, b)| {
                (
                    id.clone(),
                    b.entity,
                    b.entity,
                    format!("npc:{}", b.endorphin.model_asset),
                    false,
                )
            })
            .collect::<Vec<_>>();
        if let Some(config) = self.settings.variables.get("engine_player") {
            if config.get("enabled").and_then(Value::as_bool) != Some(false) {
                if let Some(id) = config.get("scene_id").and_then(Value::as_str) {
                    if let Some(entity) = self.scene.runtime_entity_stable_id(id) {
                        let body = self
                            .peds
                            .states()
                            .find(|p| p.profile.actor_id == id)
                            .and_then(|p| p.profile.damage_entity)
                            .filter(|e| physics.has_body(*e))
                            .or_else(|| {
                                self.settings
                                    .variables
                                    .get("engine_peds")
                                    .and_then(|v| v.pointer("/player_proxy/physics_entity"))
                                    .and_then(Value::as_u64)
                                    .filter(|e| physics.has_body(*e))
                            })
                            .or_else(|| {
                                config
                                    .get("physics_entity")
                                    .and_then(Value::as_u64)
                                    .filter(|e| physics.has_body(*e))
                            });
                        if let (Some(body), Some(asset)) = (
                            body,
                            config.get("protection_profile").and_then(Value::as_str),
                        ) {
                            infos.push((
                                id.to_owned(),
                                entity,
                                body,
                                format!("asset:{asset}"),
                                true,
                            ));
                        }
                    }
                }
            }
        }
        let mut seen = BTreeSet::new();
        for (id, entity, body_entity, key, player) in infos {
            if !seen.insert(entity) {
                continue;
            }
            let profile = if player {
                self.load_protection_asset(&key, key.trim_start_matches("asset:"))?
            } else {
                self.ped_physics.bodies[&id]
                    .endorphin
                    .protection_profile
                    .clone()
            };
            let mut actor = self
                .endorphin
                .protection
                .actors
                .remove(&entity)
                .unwrap_or_default();
            actor.actor_id = id.clone();
            actor.body_entity = body_entity;
            if actor.profile_key != key {
                actor = ProtectedActor {
                    actor_id: id.clone(),
                    body_entity,
                    profile_key: key.clone(),
                    ..Default::default()
                };
            }
            let skeleton = self.scene.entity_skeleton(entity).cloned();
            if skeleton.as_ref().is_some_and(|s| {
                actor
                    .skeleton
                    .as_ref()
                    .is_none_or(|old| !Arc::ptr_eq(old, s))
            }) {
                actor.error = binding::validate(&profile, skeleton.as_ref().unwrap()).err();
                actor.skeleton = skeleton;
            }
            let dead = self.peds.is_dead(&id)
                || self
                    .peds
                    .states()
                    .any(|p| p.profile.damage_entity == Some(body_entity) && p.dead);
            let articulated = self
                .ped_physics
                .bodies
                .get(&id)
                .is_some_and(|b| b.endorphin.ragdoll.is_some());
            let was = actor.state.active();
            if dead || actor.error.is_some() || actor.skeleton.is_none() {
                actor.state.reset();
                actor.mode = "inactive";
                self.scene
                    .set_entity_protective_arm_ik(entity, Vec::new())?;
            } else if let Some(physics) = self.physics.as_ref() {
                let vitality = self.peds.state(&id).map_or(1.0, |p| {
                    (p.health / p.profile.max_health).clamp(0.0, 1.0).sqrt()
                });
                let capacities = profile
                    .arms
                    .iter()
                    .map(|a| {
                        self.ped_physics.bodies.get(&id).map_or(vitality, |b| {
                            vitality
                                * a.trauma_parts
                                    .iter()
                                    .map(|p| b.endorphin.damage.part_capacity(p))
                                    .fold(1.0, f32::min)
                        })
                    })
                    .collect::<Vec<_>>();
                match binding::observe(
                    &self.scene,
                    entity,
                    &profile,
                    physics.gravity_magnitude(),
                    &capacities,
                ) {
                    Err(error) => {
                        actor.observation_error = Some(error);
                        actor.mode = "waiting";
                        actor.state.reset();
                        self.scene
                            .set_entity_protective_arm_ik(entity, Vec::new())?;
                    }
                    Ok(obs) => {
                        actor.observation_error = None;
                        if let Some(body) = self.ped_physics.bodies.get_mut(&id) {
                            body.endorphin.protection_offsets =
                                obs.arms.iter().map(|a| a.palm_offset_local).collect();
                        }
                        if articulated {
                            actor.state = self.ped_physics.bodies[&id]
                                .endorphin
                                .control
                                .protection
                                .clone();
                            actor.mode = "articulated";
                            self.scene
                                .set_entity_protective_arm_ik(entity, Vec::new())?;
                        } else if let Some(dt) = actor.clock.sample(tick, fixed_dt) {
                            actor.mode = "animated";
                            if let Some(pose) = physics.vehicle_body_state(body_entity) {
                                let reach = obs.arms.iter().map(|a| a.reach()).sum::<f32>()
                                    / obs.arms.len() as f32;
                                let center = obs
                                    .arms
                                    .iter()
                                    .fold([0.0; 3], |s, a| em::add(s, a.shoulder))
                                    .map(|v| v / obs.arms.len() as f32);
                                let horizon = profile.tuning.horizon(
                                    reach,
                                    obs.gravity,
                                    em::length(pose.linear_velocity),
                                );
                                let radius = self
                                    .settings
                                    .variables
                                    .get("engine_player")
                                    .filter(|_| player)
                                    .and_then(|v| v.get("body_radius"))
                                    .and_then(Value::as_f64)
                                    .map(|v| v as f32)
                                    .unwrap_or_else(|| {
                                        self.ped_physics.bodies.get(&id).map_or(
                                            reach * profile.tuning.sensor_radius_fraction,
                                            |b| b.radius,
                                        )
                                    })
                                    * profile.tuning.sensor_radius_fraction;
                                let grounded = physics.character_has_support(body_entity);
                                let acceleration = if grounded {
                                    [0.0; 3]
                                } else {
                                    [0.0, -obs.gravity, 0.0]
                                };
                                let landing = if grounded {
                                    None
                                } else {
                                    physics.protective_support(
                                        body_entity,
                                        pose.linear_velocity,
                                        obs.gravity,
                                        horizon,
                                        &solids,
                                    )
                                };
                                let mut threat = None;
                                for origin in [obs.head, center] {
                                    if let Some(t) = physics.protective_threat(
                                        origin,
                                        pose.linear_velocity,
                                        acceleration,
                                        radius,
                                        reach,
                                        horizon,
                                        &[body_entity],
                                        &solids,
                                        landing,
                                    ) {
                                        if threat.as_ref().is_none_or(
                                            |old: &newviso_endorphin::ImpactThreat| {
                                                t.time_to_impact < old.time_to_impact
                                            },
                                        ) {
                                            threat = Some(t);
                                        }
                                    }
                                }
                                actor.state.observe(&profile, &obs, threat, dt);
                            } else {
                                actor.state.reset();
                            }
                            let owner = if player {
                                self.settings
                                    .variables
                                    .get("engine_items")
                                    .and_then(|v| v.get("owner_id"))
                                    .and_then(Value::as_str)
                                    .unwrap_or(&id)
                            } else {
                                &id
                            };
                            let holding = self.items.equipped_item(owner, "hands").is_some();
                            let constraints = actor
                                .state
                                .arms
                                .iter()
                                .map(|goal| {
                                    let arm = &profile.arms[goal.arm];
                                    newviso_scene::SceneArmIkConstraint {
                                        upper_joint: arm.upper_joint.clone(),
                                        lower_joint: arm.lower_joint.clone(),
                                        hand_joint: arm.hand_joint.clone(),
                                        tip_joint: arm.hand_joint.clone(),
                                        target: goal.wrist,
                                        pole: goal.pole,
                                        rotation_degrees: quaternion_degrees(goal.hand_rotation),
                                        weight: goal.weight,
                                        hand_pose_weight: if holding { 0.0 } else { 1.0 },
                                        joint_rotations: if holding {
                                            Vec::new()
                                        } else {
                                            arm.open_hand
                                                .iter()
                                                .map(|j| (j.joint.clone(), j.rotation_degrees))
                                                .collect()
                                        },
                                    }
                                })
                                .collect();
                            self.scene
                                .set_entity_protective_arm_ik(entity, constraints)?;
                            if let Some(body) = self.ped_physics.bodies.get_mut(&id) {
                                body.endorphin.control.protection = actor.state.clone();
                            }
                        }
                    }
                }
            }
            if let Some(body) = self.ped_physics.bodies.get_mut(&id) {
                body.endorphin.control.protection = actor.state.clone();
            }
            if !was && actor.state.active() {
                self.cancel_ped_melee(&id);
            }
            self.endorphin.protection.actors.insert(entity, actor);
        }
        let retired = self
            .endorphin
            .protection
            .actors
            .keys()
            .filter(|id| !seen.contains(id))
            .copied()
            .collect::<Vec<_>>();
        for entity in retired {
            self.endorphin.protection.actors.remove(&entity);
            if self.scene.entity_skeleton(entity).is_some() {
                self.scene
                    .set_entity_protective_arm_ik(entity, Vec::new())?;
            }
        }
        Ok(())
    }
}
pub(super) fn quaternion_degrees(q: [f32; 4]) -> [f32; 3] {
    let [x, y, z, w] = q;
    [
        (2.0 * (w * x + y * z))
            .atan2(1.0 - 2.0 * (x * x + y * y))
            .to_degrees(),
        (2.0 * (w * y - z * x)).clamp(-1.0, 1.0).asin().to_degrees(),
        (2.0 * (w * z + x * y))
            .atan2(1.0 - 2.0 * (y * y + z * z))
            .to_degrees(),
    ]
}
