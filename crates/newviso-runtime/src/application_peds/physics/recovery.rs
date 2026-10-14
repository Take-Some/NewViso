use super::animation::ReactionPhase;
use super::*;

pub(super) fn ped_body_settled(velocity: [f32; 3]) -> bool {
    velocity[1].abs() < 1.0 && velocity[0].hypot(velocity[2]) < 2.0
}

impl EngineApplication {
    pub(super) fn ped_getup_space_clear(&self, id: &str) -> bool {
        let body = &self.ped_physics.bodies[id];
        let Some(physics) = self.physics.as_ref() else {
            return false;
        };
        let Some(pose) = physics.vehicle_body_state(body.entity) else {
            return false;
        };
        !self.vehicles.entity_ids().any(|vehicle| {
            let Some(local) = physics.body_local_point(vehicle, pose.position) else {
                return false;
            };
            let Some((min, max)) = physics.body_local_bounds(vehicle) else {
                return false;
            };
            local[0] >= min[0] - body.radius
                && local[0] <= max[0] + body.radius
                && local[2] >= min[2] - body.radius
                && local[2] <= max[2] + body.radius
                && local[1] < max[1] + body.radius
                && local[1] > min[1] - body.center_offset - body.radius
        })
    }

    pub(super) fn stand_ped_body(&mut self, id: &str, ground: f32) -> Result<(), String> {
        let body = &self.ped_physics.bodies[id];
        let physics = self.physics.as_mut().expect("native ped has physics");
        let Some(pose) = physics.vehicle_body_state(body.entity) else {
            return Ok(());
        };
        let center = [
            pose.position[0],
            ground + body.center_offset + 0.002,
            pose.position[2],
        ];
        physics
            .upsert_body_from_script(
                &capsule_body_command(body.entity, center, [0.0; 3], body.radius, body.half_height),
                0,
            )
            .map(|_| ())
    }

    pub(super) fn drive_ped_getup_body(&mut self, id: &str) -> Result<(), String> {
        let body = &self.ped_physics.bodies[id];
        let Some(root) = self
            .scene
            .entity_joint_world_matrix(body.entity, "SKEL_ROOT")
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
        let w = ((1.0 + axis[1]) * 0.5).sqrt();
        let rotation = [axis[2] / (2.0 * w), 0.0, -axis[0] / (2.0 * w), w];
        let physics = self.physics.as_mut().expect("ped physics");
        let Some(pose) = physics.vehicle_body_state(body.entity) else {
            return Ok(());
        };
        let ground = physics
            .character_ground_contact_height(body.entity)
            .unwrap_or(
                pose.position[1]
                    - animation::capsule_vertical_extent(
                        pose.rotation,
                        body.radius,
                        body.half_height,
                    ),
            );
        let extent = body.radius + body.half_height * axis[1];
        let position = [pose.position[0], ground + extent + 0.002, pose.position[2]];
        // The recovery clip supplies orientation and height while the body
        // continues to participate in native contacts, rays and vehicle hits.
        physics.drive_character_body_pose(body.entity, position, rotation, [0.0; 3])
    }

    pub(super) fn finish_ped_getup(&mut self, id: &str) -> Result<(), String> {
        let body = self.ped_physics.bodies.get_mut(id).expect("ped body");
        let source = body.vehicle_response.as_ref().map(|r| r.source);
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
        if !dead && self.ped_physics.bodies[id].ragdoll.is_some() {
            self.clear_ped_ragdoll(id)?;
        }
        if dead {
            if let Some(ragdoll) = self
                .ped_physics
                .bodies
                .get_mut(id)
                .and_then(|b| b.ragdoll.as_mut())
            {
                ragdoll.advance(dt);
                return Ok(());
            }
            let point = self
                .physics
                .as_ref()
                .and_then(|p| p.vehicle_body_state(self.ped_physics.bodies[id].entity))
                .map_or([0.0; 3], |p| p.position);
            if self.activate_ped_ragdoll(id, [0.0, 0.0, -1.0], point, 0.0)? {
                return Ok(());
            }
        }
        let before = self.ped_physics.bodies[id].clone();
        if !dead && (before.was_dead || before.reset_requested) {
            if let Some(ground) = self
                .physics
                .as_ref()
                .and_then(|p| p.character_ground_contact_height(before.entity))
            {
                self.stand_ped_body(id, ground)?;
            } else if let Some(pose) = self
                .physics
                .as_ref()
                .and_then(|p| p.vehicle_body_state(before.entity))
            {
                let command = capsule_body_command(
                    before.entity,
                    pose.position,
                    [0.0; 3],
                    before.radius,
                    before.half_height,
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
        } else if dead && !before.was_dead {
            let already_fallen = before
                .vehicle_response
                .as_ref()
                .is_some_and(|r| r.kind != VehiclePedHit::Bump)
                || before.reaction.as_ref().is_some_and(|r| {
                    matches!(r.phase, ReactionPhase::Knockdown | ReactionPhase::Down)
                });
            if !already_fallen {
                self.physics
                    .as_mut()
                    .expect("physics")
                    .release_character_body(before.entity)?;
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
                let pose = physics.vehicle_body_state(body.entity).expect("ped body");
                let ground = physics.character_ground_contact_height(body.entity);
                if self.ped_getup_space_clear(id)
                    && ground.is_some()
                    && ped_body_settled(pose.linear_velocity)
                {
                    self.stand_ped_body(id, ground.expect("ground checked"))?;
                    self.finish_ped_getup(id)?;
                } else {
                    self.ped_physics
                        .bodies
                        .get_mut(id)
                        .expect("ped body")
                        .recovery_seconds = 0.1;
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
