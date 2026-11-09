use super::animation::ReactionPhase;
use super::*;

impl EngineApplication {
    pub(crate) fn ped_getup_query_ignore(&self, id: &str) -> Vec<u64> {
        let body = &self.ped_physics.bodies[id];
        let mut ignored = body
            .endorphin
            .ragdoll
            .as_ref()
            .map_or_else(|| vec![body.entity], |r| r.entities().to_vec());
        if let Some(physics) = self.physics.as_ref() {
            for entity in self.items.pickup_body_entities() {
                if physics.endorphin_recovery_pickup_yields(entity, body.mass_kg)
                    && !ignored.contains(&entity)
                {
                    ignored.push(entity);
                }
            }
        }
        ignored
    }
    fn ped_getup_anchor(&self, id: &str) -> Option<[f32; 3]> {
        let body = self.ped_physics.bodies.get(id)?;
        if body.reset_requested || body.was_dead || self.peds.is_dead(id) {
            return None;
        }
        let reaction = body.reaction.as_ref()?;
        if reaction.phase != ReactionPhase::GetUp || reaction.cause != "physical_recovery" {
            return None;
        }
        if body.endorphin.recovery_support.is_some() {
            return self.ped_getup_support_sample(id).map(|s| s.capsule_center);
        }
        Some(
            body.endorphin
                .getup_match
                .placement
                .as_ref()?
                .selected?
                .capsule_center,
        )
    }
    pub(crate) fn ped_getup_space_clear(&self, id: &str) -> bool {
        self.ped_getup_clearance(id).clear
    }

    pub(crate) fn ped_getup_clearance(
        &self,
        id: &str,
    ) -> crate::application_endorphin::GetUpClearance {
        use crate::application_endorphin::GetUpClearance;
        let body = &self.ped_physics.bodies[id];
        let Some(physics) = self.physics.as_ref() else {
            return GetUpClearance::default();
        };
        let Some(pose) = physics.vehicle_body_state(body.entity) else {
            return GetUpClearance::default();
        };
        let ground = if self.ped_physical_getup_active(id) {
            let Some(ground) = self.ped_getup_observed_ground(id) else {
                return GetUpClearance::default();
            };
            ground
        } else {
            self.ped_support_height(id).unwrap_or(
                pose.position[1]
                    - animation::capsule_vertical_extent(
                        pose.rotation,
                        body.radius,
                        body.half_height,
                    ),
            )
        };
        let skin = physics.contact_skin();
        let center = self.ped_getup_anchor(id).unwrap_or(pose.position);
        let origin = [center[0], ground + body.radius + skin, center[2]];
        let delta = [0.0, 2.0 * body.half_height, 0.0];
        let min = [origin[0] - body.radius, ground, origin[2] - body.radius];
        let max = [
            origin[0] + body.radius,
            origin[1] + delta[1] + body.radius,
            origin[2] + body.radius,
        ];
        let solids = self.scene.physics_static_solid_aabbs_near(&[(min, max)]);
        let ignore = self.ped_getup_query_ignore(id);
        let hit = physics.endorphin_getup_obstruction(
            origin,
            delta[1],
            (body.radius - skin).max(f32::EPSILON),
            &ignore,
            &solids,
        );
        GetUpClearance {
            valid: true,
            clear: hit.is_none(),
            ground: Some(ground),
            obstacle: hit.and_then(|h| h.entity),
            fraction: hit.map(|h| h.fraction),
            normal: hit.map(|h| h.normal),
        }
    }

    pub(crate) fn stand_ped_body(&mut self, id: &str, ground: f32) -> Result<(), String> {
        let body = &self.ped_physics.bodies[id];
        let physics = self.physics.as_mut().expect("native ped has physics");
        let Some(pose) = physics.vehicle_body_state(body.entity) else {
            return Ok(());
        };
        let anchor = self.ped_getup_anchor(id).unwrap_or(pose.position);
        let center = [anchor[0], ground + body.center_offset + 0.002, anchor[2]];
        let velocity = self
            .ped_getup_support_sample(id)
            .map_or([0.0; 3], |s| s.velocity);
        self.stand_ped_body_with_velocity(id, center, velocity)
    }

    pub(crate) fn stand_ped_body_with_velocity(
        &mut self,
        id: &str,
        center: [f32; 3],
        velocity: [f32; 3],
    ) -> Result<(), String> {
        if !center.iter().chain(&velocity).all(|v| v.is_finite()) {
            return Err("invalid recovery capsule center".into());
        }
        let body = &self.ped_physics.bodies[id];
        let physics = self.physics.as_mut().expect("native ped has physics");
        physics
            .upsert_body_from_script(
                &capsule_body_command(
                    body.entity,
                    center,
                    velocity,
                    body.radius,
                    body.half_height,
                    body.mass_kg,
                ),
                0,
            )
            .map(|_| ())
    }

    pub(super) fn drive_ped_getup_body(&mut self, id: &str) -> Result<(), String> {
        let body = &self.ped_physics.bodies[id];
        let Some(root_name) = self.ped_getup_root_joint(id) else {
            return Ok(());
        };
        let Some(root) = self
            .scene
            .entity_joint_world_matrix(body.entity, root_name)
            .ok()
        else {
            return Ok(());
        };
        let mut axis = [root[4], root[5], root[6]];
        let length = axis.iter().map(|v| v * v).sum::<f32>().sqrt();
        if length < 0.01 {
            return Ok(());
        }
        axis = axis.map(|v| v / length);
        // A capsule has the same shape for either sign of its longitudinal axis.
        if axis[1] < 0.0 {
            axis = axis.map(|v| -v);
        }
        let anchor = self.ped_getup_anchor(id);
        let motion = self.ped_getup_support_sample(id);
        if motion.is_some() {
            if let Some(model) = self.scene.entity_transform_matrix(body.entity) {
                let scene_heading = model[8].atan2(model[0]);
                let yaw = (body.death_heading.to_radians() - scene_heading) * 0.5;
                axis = newviso_endorphin::math::rotate([0.0, yaw.sin(), 0.0, yaw.cos()], axis);
            }
        }
        let w = ((1.0 + axis[1]) * 0.5).sqrt();
        let rotation = [axis[2] / (2.0 * w), 0.0, -axis[0] / (2.0 * w), w];
        let observed_ground = self.ped_getup_observed_ground(id);
        let physical_getup = self.ped_physical_getup_active(id);
        let physics = self.physics.as_mut().expect("ped physics");
        let Some(pose) = physics.vehicle_body_state(body.entity) else {
            return Ok(());
        };
        let Some(ground) = (if physical_getup {
            observed_ground
        } else {
            physics.character_ground_contact_height(body.entity)
        }) else {
            // Missing support must let native gravity act. Reusing the old
            // capsule bottom here held a recovering actor in mid-air forever.
            return Ok(());
        };
        let extent = body.radius + body.half_height * axis[1];
        let center = anchor.unwrap_or(pose.position);
        let position = [center[0], ground + extent + 0.002, center[2]];
        // The recovery clip supplies orientation and height while the body
        // continues to participate in native contacts, rays and vehicle hits.
        physics.drive_character_body_pose(
            body.entity,
            position,
            rotation,
            motion.map_or([0.0; 3], |s| s.velocity),
        )
    }

    pub(super) fn finish_ped_getup(&mut self, id: &str) -> Result<(), String> {
        let body = self.ped_physics.bodies.get_mut(id).expect("ped body");
        let source = body.vehicle_response.as_ref().map(|r| r.source);
        if body.endorphin.recovery_support.take().is_some() {
            body.endorphin.getup_support.status = "completed";
        }
        body.reaction = None;
        body.vehicle_response = None;
        body.recovery_seconds = 0.0;
        host::publish_event_json(
            if source.is_some() {
                "ped.vehicle.recovered"
            } else {
                "ped.recovered"
            },
            "newviso.peds",
            json!({"actor_id": id, "source_entity_key": source.map(|s| s.to_string())}),
        )
        .map(|_| ())
    }

    pub(super) fn sync_ped_body_reaction(
        &mut self,
        id: &str,
        dt: f32,
        dead: bool,
    ) -> Result<(), String> {
        let body = &self.ped_physics.bodies[id];
        if !dead && body.endorphin.ragdoll.is_some() && (body.was_dead || body.reset_requested) {
            self.clear_ped_ragdoll(id)?;
        }
        if self.ped_has_ragdoll(id) {
            let body = self.ped_physics.bodies.get_mut(id).expect("physical ped");
            body.was_dead = dead;
            body.endorphin
                .ragdoll
                .as_mut()
                .expect("ragdoll")
                .advance(dt);
            if !dead && body.endorphin.recovery.ready {
                self.begin_ped_physical_getup(id)?;
            }
            return Ok(());
        }
        if dead {
            let point = self
                .physics
                .as_ref()
                .and_then(|p| p.vehicle_body_state(self.ped_physics.bodies[id].entity))
                .map_or([0.0; 3], |p| p.position);
            if self.activate_ped_ragdoll(id, [0.0; 3], point, 0.0)? {
                return Ok(());
            }
        }
        let body = &self.ped_physics.bodies[id];
        let (entity, radius, half_height, mass_kg, was_dead, reset_requested) = (
            body.entity,
            body.radius,
            body.half_height,
            body.mass_kg,
            body.was_dead,
            body.reset_requested,
        );
        let already_fallen =
            body.vehicle_response
                .as_ref()
                .is_some_and(|r| r.kind != VehiclePedHit::Bump)
                || body.reaction.as_ref().is_some_and(|r| {
                    matches!(r.phase, ReactionPhase::Knockdown | ReactionPhase::Down)
                });
        if !dead && (was_dead || reset_requested) {
            self.scene.cancel_entity_physical_recovery(entity);
            if let Some(ground) = self
                .physics
                .as_ref()
                .and_then(|p| p.character_ground_contact_height(entity))
            {
                self.stand_ped_body(id, ground)?;
            } else if let Some(pose) = self
                .physics
                .as_ref()
                .and_then(|p| p.vehicle_body_state(entity))
            {
                let command = capsule_body_command(
                    entity,
                    pose.position,
                    [0.0; 3],
                    radius,
                    half_height,
                    mass_kg,
                );
                self.physics
                    .as_mut()
                    .expect("physics")
                    .upsert_body_from_script(&command, 0)?;
            }
            let body = self.ped_physics.bodies.get_mut(id).expect("ped body");
            body.recovery_seconds = 0.0;
            body.vehicle_response = None;
            body.reaction = None;
            body.hit_reaction = None;
            body.reset_requested = false;
            body.endorphin.recovery.reset();
            body.endorphin.getup_match = Default::default();
            body.endorphin.recovery_support = None;
            body.endorphin.getup_support = Default::default();
        } else if dead && !was_dead {
            if !already_fallen {
                self.physics
                    .as_mut()
                    .expect("physics")
                    .release_character_body(
                        entity,
                        self.ped_physics.bodies[id]
                            .endorphin
                            .model
                            .ragdoll
                            .angular_damping,
                    )?;
            }
            self.start_ped_fall_animation(id, [0.0, 0.0, -1.0], "death");
        }
        let body = self.ped_physics.bodies.get_mut(id).expect("ped body");
        body.was_dead = dead;
        body.recovery_seconds = (body.recovery_seconds - dt).max(0.0);
        if let Some(response) = body.vehicle_response.as_mut() {
            response.elapsed += dt;
        }
        self.advance_ped_animation(id, dt, dead)?;
        let body = &self.ped_physics.bodies[id];
        if !dead && body.reaction.is_none() && body.recovery_seconds <= 0.0 {
            if body
                .vehicle_response
                .as_ref()
                .is_some_and(|r| r.kind != VehiclePedHit::Bump)
            {
                let physics = self.physics.as_ref().expect("physics");
                let ground = physics.character_ground_contact_height(body.entity);
                if self.ped_getup_space_clear(id)
                    && ground.is_some()
                    && body.endorphin.recovery.ready
                {
                    self.stand_ped_body(id, ground.expect("ground checked"))?;
                    self.finish_ped_getup(id)?;
                } else {
                    self.ped_physics
                        .bodies
                        .get_mut(id)
                        .expect("ped body")
                        .recovery_seconds = 0.0;
                }
            } else {
                self.ped_physics
                    .bodies
                    .get_mut(id)
                    .expect("ped body")
                    .vehicle_response = None;
            }
        }
        Ok(())
    }
}
