//! Terrain-aware lower-limb targets shared by every upright physical character.
use super::*;
use newviso_endorphin::math as m;
use newviso_scene::{SceneFootSupport, SceneLegIkConstraint};
mod placement;
use newviso_character::FootLeg as LegBinding;
use placement::{BoundSoles, NeutralSole};
mod motion;
mod sources;
use sources::FootSource;
#[derive(Debug, Default)]
pub(super) struct FeetRuntime {
    profiles: BTreeMap<String, Arc<Vec<LegBinding>>>,
    motion_profile: Option<newviso_character::FootMotionProfile>,
    motion: BTreeMap<u64, motion::ActorMotion>,
    published: Vec<Value>,
    entities: [BTreeSet<u64>; 2],
    bindings: BTreeMap<u64, BoundSoles>,
    posture: BTreeMap<u64, (f32, u64)>,
    pub(super) body_owners: BTreeMap<u64, u64>,
    entity_bodies: BTreeMap<u64, u64>,
}
impl EngineApplication {
    pub(crate) fn foot_motion_snapshot(&self) -> Value {
        Value::Array(self.endorphin.feet.published.clone())
    }
    pub(crate) fn publish_foot_motion_snapshot(&mut self) {
        let mut snapshot = self.scene.entity_leg_ik_snapshot();
        if let Some(actors) = snapshot.as_array_mut() {
            for actor in actors {
                let state = actor
                    .get("entity_key")
                    .and_then(Value::as_str)
                    .and_then(|key| key.parse::<u64>().ok())
                    .and_then(|id| self.endorphin.feet.motion.get(&id));
                if let Some(state) = state {
                    actor["sample_seconds"] = Value::from(state.at);
                }
            }
        }
        self.endorphin.feet.published = snapshot.as_array().cloned().unwrap_or_default();
    }
    pub(crate) fn update_character_feet(&mut self, player_only: bool) -> Result<(), String> {
        let Some(physics) = self.physics.as_ref() else {
            return Ok(());
        };
        let solids = physics
            .scene_collider_interests()
            .map(|b| self.scene.physics_static_solid_colliders_near(&b))
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();
        let player_entity = self
            .settings
            .variables
            .get("engine_player")
            .filter(|p| p.get("enabled").and_then(Value::as_bool) != Some(false))
            .and_then(|p| p.get("scene_id"))
            .and_then(Value::as_str)
            .and_then(|id| self.scene.runtime_entity_stable_id(id));
        let generic = self.entity_foot_sources(player_only)?;
        let overrides = self
            .scene
            .character_bindings()
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        let mut infos: Vec<FootSource> = self
            .ped_physics
            .bodies
            .iter()
            .filter(|(id, b)| {
                !player_only
                    && Some(b.entity) != player_entity
                    && !overrides.contains(&b.entity)
                    && b.authority(self.peds.is_dead(id)).locomotion
            })
            .map(|(_, b)| {
                let key = format!("rig:{}", b.endorphin.model_asset);
                let legs = self
                    .endorphin
                    .feet
                    .profiles
                    .entry(key)
                    .or_insert_with(|| {
                        let parts = &b.endorphin.model.rig.parts;
                        Arc::new(
                            parts
                                .iter()
                                .filter_map(|foot| {
                                    let sole = foot.sole?;
                                    let lower = parts
                                        .iter()
                                        .find(|p| Some(&p.name) == foot.parent.as_ref())?;
                                    let upper = parts
                                        .iter()
                                        .find(|p| Some(&p.name) == lower.parent.as_ref())?;
                                    Some(LegBinding {
                                        hip: upper.joint.clone(),
                                        knee: lower.joint.clone(),
                                        foot: foot.joint.clone(),
                                        toe: foot.end_joint.clone()?,
                                        sole_half_width: Some(
                                            sole.width_radius_scale * foot.radius,
                                        ),
                                        heel_length: Some(sole.heel_radius_scale * foot.radius),
                                        sole_height: sole.vertical_offset_radius_scale.abs()
                                            * foot.radius,
                                    })
                                })
                                .collect::<Vec<_>>(),
                        )
                    })
                    .clone();
                (b.entity, b.entity, legs, None)
            })
            .collect::<Vec<_>>();
        if player_only {
            let player = self
                .settings
                .variables
                .get("engine_player")
                .filter(|p| p.get("enabled").and_then(Value::as_bool) != Some(false))
                .and_then(|p| {
                    Some((
                        p.get("scene_id")?.as_str()?.to_owned(),
                        p.get("physics_entity")?.as_u64()?,
                        p.get("foot_placement_profile")?.as_str()?.to_owned(),
                    ))
                });
            if let Some((id, body, asset)) = player {
                if let Some(entity) = self.scene.runtime_entity_stable_id(&id) {
                    if !overrides.contains(&entity)
                        && !self
                            .peds
                            .states()
                            .any(|p| p.profile.actor_id == id && p.dead)
                    {
                        let profile = self.foot_profile_asset(&asset)?;
                        infos.push((entity, body, profile, None));
                    }
                }
            }
        }
        infos.extend(generic);
        let motion_profile = self.foot_motion_profile()?;
        let physics = self.physics.as_ref().unwrap();
        let mut seen = BTreeSet::new();
        for (entity, body, legs, anatomy) in infos {
            if !seen.insert(entity) {
                continue;
            }
            if self.endorphin.feet.entity_bodies.insert(entity, body) != Some(body) {
                self.endorphin.feet.posture.remove(&entity);
                self.endorphin.feet.motion.remove(&entity);
            }
            // Reading the authored animation without yesterday's correction
            // prevents IK feedback/drift. The arm channels stay intact.
            self.scene.set_entity_leg_ik(entity, Vec::new())?;
            self.scene.set_entity_ground_pose_offset(entity, [0.0; 3])?;
            let config = if anatomy.is_some() {
                physics.character_foot_config(body)
            } else {
                physics.character_stair_config(body)
            };
            let Some(cfg) = config else {
                self.endorphin.feet.posture.remove(&entity);
                self.endorphin.feet.motion.remove(&entity);
                continue;
            };
            let Some(pose) = physics.vehicle_body_state(body) else {
                continue;
            };
            if legs.is_empty() || self.scene.entity_has_physical_pose(entity) {
                self.endorphin.feet.posture.remove(&entity);
                self.endorphin.feet.motion.remove(&entity);
                continue;
            }
            let Some(skeleton) = self.scene.entity_skeleton(entity).cloned() else {
                continue;
            };
            let Some(transform) = self.scene.entity_transform_matrix(entity) else {
                continue;
            };
            let Some((_, _, scale)) = self.scene.entity_transform_values(entity) else {
                continue;
            };
            let scale = scale.map(f32::abs);
            let cached = self.endorphin.feet.bindings.get(&entity);
            if cached.is_none_or(|c| {
                !Arc::ptr_eq(&c.skeleton, &skeleton)
                    || !Arc::ptr_eq(&c.profile, &legs)
                    || c.scale != scale
            }) {
                sources::validate_leg_skeleton(&skeleton, &legs)
                    .map_err(|e| format!("character entity {entity}: {e}"))?;
                let mut floor_offset = f32::INFINITY;
                let mut soles = Vec::new();
                let up = legs
                    .iter()
                    .filter_map(|leg| {
                        let hip = self
                            .scene
                            .entity_joint_bind_world_matrix(entity, &leg.hip)
                            .ok()?;
                        let foot = self
                            .scene
                            .entity_joint_bind_world_matrix(entity, &leg.foot)
                            .ok()?;
                        Some(m::sub(m::position(hip), m::position(foot)))
                    })
                    .fold([0.0; 3], m::add);

                for leg in legs.iter() {
                    let (Ok(foot), Ok(toe)) = (
                        self.scene.entity_joint_bind_world_matrix(entity, &leg.foot),
                        self.scene.entity_joint_bind_world_matrix(entity, &leg.toe),
                    ) else {
                        break;
                    };
                    let Some(sole) = NeutralSole::bind(
                        foot,
                        toe,
                        leg,
                        scale.into_iter().fold(0.0, f32::max),
                        up,
                    ) else {
                        break;
                    };
                    if let Ok(axis) = m::unit(up) {
                        let local = m::sub(m::position(foot), m::position(transform));
                        floor_offset = floor_offset.min(
                            m::dot(local, axis)
                                - leg.sole_height * scale.into_iter().fold(0.0, f32::max),
                        );
                    }
                    soles.push(sole);
                }
                self.endorphin.feet.motion.remove(&entity);
                self.endorphin.feet.bindings.insert(
                    entity,
                    BoundSoles {
                        skeleton,
                        profile: legs.clone(),
                        scale,
                        soles,
                        floor_offset,
                    },
                );
            }
            let soles = &self.endorphin.feet.bindings[&entity].soles;
            if soles.len() != legs.len() {
                continue;
            }
            if let Some(binding) = anatomy.as_ref().filter(|b| b.sync_position) {
                let root_y = pose.position[1]
                    + binding.root_offset_y.unwrap_or(
                        -cfg.radius
                            - cfg.half_height
                            - self.endorphin.feet.bindings[&entity].floor_offset,
                    );
                let Some((_, rotation, scale)) = self.scene.entity_transform_values(entity) else {
                    continue;
                };
                self.scene.set_entity_transform(
                    entity,
                    [pose.position[0], root_y, pose.position[2]],
                    rotation,
                    scale,
                )?;
            }
            if (physics.character_stair_support(body).is_none()
                && physics.character_ground_contact_height(body).is_none())
                || (physics.character_stair_support(body).is_none()
                    && pose.linear_velocity[1] > 0.25)
                || (anatomy.is_some()
                    && m::rotate(pose.rotation, [0.0, 1.0, 0.0])[1]
                        < cfg.max_slope_degrees.to_radians().cos())
            {
                self.endorphin.feet.posture.remove(&entity);
                self.endorphin.feet.motion.remove(&entity);
                continue;
            }
            let names = legs
                .iter()
                .flat_map(|l| {
                    [
                        l.hip.as_str(),
                        l.knee.as_str(),
                        l.foot.as_str(),
                        l.toe.as_str(),
                    ]
                })
                .collect::<Vec<_>>();
            let Ok(joints) = self.scene.entity_joint_world_matrices(entity, &names) else {
                continue;
            };
            let point = |name: &str| joints.get(name).map(|p| m::position(*p));
            let Some((animation_root, _, _)) = self.scene.entity_transform_values(entity) else {
                continue;
            };
            let mut plans = Vec::new();
            let (tick, fixed_dt) = physics.fixed_clock();
            let previous = self
                .endorphin
                .feet
                .posture
                .get(&entity)
                .copied()
                .unwrap_or((0.0, tick));
            let elapsed = tick.saturating_sub(previous.1) as f32 * fixed_dt;
            // Reserve the observed sampling interval plus query skin. The next
            // animation/body update must not exhaust a previously straight leg.
            let reserve = cfg.skin + cfg.step_speed * elapsed.max(fixed_dt);
            let max_drop = legs
                .iter()
                .filter_map(|leg| {
                    Some(
                        m::length(m::sub(point(&leg.hip)?, point(&leg.knee)?))
                            + m::length(m::sub(point(&leg.knee)?, point(&leg.foot)?)),
                    )
                })
                .fold(f32::INFINITY, f32::min)
                * motion_profile.pelvis_drop_ratio;
            let mut constraints = Vec::new();
            let now = self.elapsed_seconds;
            let actor_motion =
                self.endorphin
                    .feet
                    .motion
                    .entry(entity)
                    .or_insert_with(|| motion::ActorMotion {
                        at: now,
                        legs: Vec::new(),
                    });
            let motion_dt = (now - actor_motion.at).clamp(0.0, 0.25) as f32;
            actor_motion.at = now;
            actor_motion.legs.resize_with(legs.len(), Default::default);
            for (leg_index, (l, sole)) in legs.iter().zip(soles.iter()).enumerate() {
                let (Some(hip), Some(knee), Some(foot)) =
                    (point(&l.hip), point(&l.knee), point(&l.foot))
                else {
                    continue;
                };
                let reach = m::length(m::sub(hip, knee)) + m::length(m::sub(knee, foot));
                let sole_length = m::length(m::sub(sole.corners[0], sole.corners[3]));
                let foot_rotation = m::rotation(joints[&l.foot]);
                let source_floor = sole
                    .corners
                    .iter()
                    .map(|p| m::rotate(foot_rotation, *p)[1] + foot[1])
                    .fold(f32::INFINITY, f32::min);
                let input = newviso_character::FootMotionInput {
                    authored: foot,
                    relative_height: source_floor - animation_root[1],
                    root: animation_root,
                    root_velocity: pose.linear_velocity,
                    sole_length,
                    leg_length: reach,
                    skin: cfg.skin,
                    gravity: physics.gravity_magnitude(),
                    dt: motion_dt,
                };
                let mut anchor_hip = hip;
                anchor_hip[1] += previous.0;
                let Some(mut best) = motion::plan(
                    physics,
                    &solids,
                    &mut actor_motion.legs[leg_index],
                    motion_profile,
                    input,
                    cfg,
                    *sole,
                    hip,
                    anchor_hip,
                    knee,
                    m::rotation(joints[&l.foot]),
                    body,
                    reserve,
                    max_drop,
                ) else {
                    continue;
                };
                let upper = m::length(m::sub(hip, knee));
                let lower = m::length(m::sub(knee, foot));
                let mut interval =
                    placement::root_interval(hip, best.target, upper, lower, max_drop, reserve);
                if interval.is_none() {
                    motion::project_reach(&mut best, anchor_hip, upper, lower, reserve);
                    interval =
                        placement::root_interval(hip, best.target, upper, lower, max_drop, reserve);
                }
                let interval = interval.unwrap_or((-max_drop, 0.0));
                plans.push((
                    l,
                    sole,
                    foot,
                    knee,
                    best.source_lift,
                    best,
                    interval,
                    leg_index,
                ));
            }
            plans.sort_by(|a, b| a.4.total_cmp(&b.4));
            let mut allowed = (-max_drop, 0.0_f32);
            for p in &plans {
                let lo = allowed.0.max(p.6.0);
                let hi = allowed.1.min(p.6.1);
                if lo <= hi {
                    allowed = (lo, hi);
                }
            }
            let state = self
                .endorphin
                .feet
                .posture
                .entry(entity)
                .or_insert((0.0, tick));
            let wanted = if plans.is_empty() { 0.0 } else { allowed.1 };
            let offset = state.0
                + (wanted - state.0)
                    .clamp(-cfg.step_speed * elapsed, cfg.ground_snap_speed * elapsed);
            *state = (offset, tick);
            self.scene
                .set_entity_ground_pose_offset(entity, [0.0, offset, 0.0])?;
            for (l, sole, foot, mut knee, _lift, mut best, interval, leg_index) in plans {
                if best.reach_projected || offset < interval.0 - 1e-5 || offset > interval.1 + 1e-5
                {
                    if let Some(hip) = point(&l.hip) {
                        let actual_hip = m::add(hip, [0.0, offset, 0.0]);
                        if let Some(actor) = self.endorphin.feet.motion.get_mut(&entity) {
                            motion::constrain_reach_contact(
                                physics,
                                &solids,
                                &mut actor.legs[leg_index],
                                &mut best,
                                actual_hip,
                                m::length(m::sub(hip, knee)),
                                m::length(m::sub(knee, foot)),
                                reserve,
                                body,
                                cfg,
                            );
                        }
                    }
                }
                let actual_contact = best
                    .reach_projected
                    .then(|| physics.character_foot_clearance(best.corners, body, &solids, cfg))
                    .flatten();
                let clearance = actual_contact.map_or(0.0, |v| v.0.max(0.0));
                if best.reach_projected {
                    best.contact_point = actual_contact.map(|v| v.3);
                    best.contact_gap = actual_contact.map(|v| (clearance - v.0).max(0.0));
                }
                if clearance > 0.0 {
                    best.target[1] += clearance;
                    best.corners = best.corners.map(|p| m::add(p, [0.0, clearance, 0.0]));
                    best.collision_adjustment += clearance;
                    if clearance > cfg.skin * 0.1 {
                        best.planted = false;
                    }
                }
                let constraint_response = self
                    .endorphin
                    .feet
                    .motion
                    .get_mut(&entity)
                    .map_or([0.0; 3], |actor| {
                        motion::commit(&mut actor.legs[leg_index], &best, foot)
                    });
                if let Some(hip) = point(&l.hip) {
                    knee = motion::bend_pole(hip, knee, foot, best.target, offset);
                }
                constraints.push(SceneLegIkConstraint {
                    hip_joint: l.hip.clone(),
                    knee_joint: l.knee.clone(),
                    foot_joint: l.foot.clone(),
                    target: best.target,
                    pole: knee,
                    rotation_degrees: protection::quaternion_degrees(best.rotation),
                    weight: 1.0,
                    support: Some(SceneFootSupport {
                        surface_id: best.geometry.surface_id,
                        normal: best.geometry.normal,
                        authored_ankle: foot,
                        target_sole: best.corners,
                        local_sole: sole.corners,
                        planted: best.planted,
                        contact_weight: best.contact,
                        contact_point: best.contact_point,
                        contact_gap: best.contact_gap,
                        phase_weight: best.phase_weight,
                        landing_weight: best.landing_weight,
                        contact_demand: best.demand,
                        contact_sole: best.geometry.corners,
                        contact_target: best.geometry.target,
                        authored_rotation: m::rotation(joints[&l.foot]),
                        correction_velocity: best.velocity,
                        constraint_response,
                        held: best.held,
                        anchored_goal: best.anchored_goal,
                        preview_goal: best.preview_goal,
                        surface_complete: best.geometry.complete,
                        has_surface: best.geometry.has_surface,
                        collision_adjustment: best.collision_adjustment,
                        orientation_adjustment: best.orientation_adjustment,
                        horizontal_adjustment: best.horizontal_adjustment,
                        reach_projected: best.reach_projected,
                        root_offset: offset,
                    }),
                });
            }
            self.scene.set_entity_leg_ik(entity, constraints)?;
        }
        let phase = usize::from(player_only);
        for id in self.endorphin.feet.entities[phase].difference(&seen) {
            let other = 1 - phase;
            if self.endorphin.feet.entities[other].contains(id)
                || self.scene.character_bindings().get(id).is_some_and(|b| {
                    b.enabled
                        && (b.phase == newviso_scene::CharacterUpdatePhase::Script) != player_only
                })
            {
                continue;
            }

            if self.scene.entity_skeleton(*id).is_some() {
                self.scene.set_entity_leg_ik(*id, Vec::new())?;
                self.scene.set_entity_ground_pose_offset(*id, [0.0; 3])?;
            }
        }
        self.endorphin.feet.entities[phase] = seen;
        let feet = &mut self.endorphin.feet;
        let live = &feet.entities;
        feet.bindings
            .retain(|id, _| live.iter().any(|set| set.contains(id)));
        feet.posture
            .retain(|id, _| live.iter().any(|set| set.contains(id)));
        feet.entity_bodies
            .retain(|id, _| live.iter().any(|set| set.contains(id)));
        feet.motion
            .retain(|id, _| live.iter().any(|set| set.contains(id)));
        Ok(())
    }
}
