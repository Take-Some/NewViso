use super::*;

impl PhysicsRuntime {
    pub(crate) fn scene_activity_updates(&self) -> Vec<PhysicsBodyActivityUpdate> {
        self.last_output
            .activity_updates
            .iter()
            .copied()
            .filter(|update| {
                self.bodies.get(&update.entity).is_some_and(|body| {
                    matches!(
                        body.kind,
                        PhysicsBodyKind::Dynamic | PhysicsBodyKind::Kinematic
                    )
                })
            })
            .collect()
    }

    pub(crate) fn scene_pose_updates(&self) -> Vec<PhysicsBodyPoseUpdate> {
        self.last_output
            .pose_updates
            .iter()
            .copied()
            .filter_map(|mut pose| {
                if !self
                    .bodies
                    .get(&pose.entity)
                    .is_some_and(|body| body.kind == PhysicsBodyKind::Dynamic)
                {
                    return None;
                }
                if let Some(offset) = self.scene_pose_offsets.get(&pose.entity).copied() {
                    let offset = rotate_camera_vector(offset, pose.rotation);
                    pose.position = std::array::from_fn(|axis| pose.position[axis] + offset[axis]);
                }
                if let Some(rotation_offset) =
                    self.scene_rotation_offsets.get(&pose.entity).copied()
                {
                    pose.rotation = quaternion_multiply(pose.rotation, rotation_offset);
                }
                Some(pose)
            })
            .collect()
    }

    pub(crate) fn body_collision_hulls(&self, entity: u64) -> Option<Vec<Vec<[f32; 3]>>> {
        self.bodies
            .get(&entity)
            .map(|body| body.convex_hulls.clone())
    }

    pub(crate) fn restore_body_collision_hulls(&mut self, entity: u64, hulls: Vec<Vec<[f32; 3]>>) {
        if let Some(body) = self.bodies.get_mut(&entity) {
            body.convex_hulls = hulls;
        }
    }

    pub(crate) fn vehicle_body_state(
        &self,
        entity: u64,
    ) -> Option<newviso_vehicle::VehicleBodyState> {
        self.bodies
            .get(&entity)
            .map(|body| newviso_vehicle::VehicleBodyState {
                position: body.position,
                rotation: body.rotation,
                linear_velocity: body.linear_velocity,
                angular_velocity: body.angular_velocity,
            })
    }

    /// Remove a separately represented collision child only when its entire
    /// hull belongs to the detached region. Never discard an enclosing chassis

    pub(crate) fn pickup_body_pose(&self, entity: u64) -> Option<newviso_items::PickupBodyPose> {
        self.bodies
            .get(&entity)
            .map(|body| newviso_items::PickupBodyPose {
                position: body.position,
                rotation: body.rotation,
                linear_velocity: body.linear_velocity,
                angular_velocity: body.angular_velocity,
            })
    }

    pub(crate) fn body_linear_velocity(&self, entity: u64) -> [f32; 3] {
        self.bodies
            .get(&entity)
            .map(|body| body.linear_velocity)
            .unwrap_or([0.0; 3])
    }

    pub(crate) fn has_body(&self, entity: u64) -> bool {
        self.bodies.contains_key(&entity)
    }

    pub(crate) fn runtime_state(&self) -> Value {
        // Physics remains authoritative on a fixed timestep, while presentation
        // is sampled at the platform frame rate. Predict only across the residual
        // accumulator so high-refresh rendering does not repeat fixed snapshots.
        // body.position itself stays authoritative for simulation and contacts.
        let fixed_dt = self.settings.fixed_dt();
        let presentation_lead_seconds = self.accumulator.clamp(0.0, fixed_dt);
        let presentation_alpha = if fixed_dt > 0.0 {
            (presentation_lead_seconds / fixed_dt).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let bodies = self
            .bodies
            .values()
            .map(|body| {
                let presentation_position = match body.kind {
                    PhysicsBodyKind::Static => body.position,
                    PhysicsBodyKind::Dynamic | PhysicsBodyKind::Kinematic => [
                        body.position[0] + body.linear_velocity[0] * presentation_lead_seconds,
                        body.position[1] + body.linear_velocity[1] * presentation_lead_seconds,
                        body.position[2] + body.linear_velocity[2] * presentation_lead_seconds,
                    ],
                };
                json!({
                    "entity": body.entity,
                    "kind": match body.kind {
                        PhysicsBodyKind::Static => "static",
                        PhysicsBodyKind::Dynamic => "dynamic",
                        PhysicsBodyKind::Kinematic => "kinematic",
                    },
                    "position": body.position,
                    "presentation_position": presentation_position,
                    "rotation": body.rotation,
                    "linear_velocity": body.linear_velocity,
                    "angular_velocity": body.angular_velocity,
                })
            })
            .collect::<Vec<_>>();

        let damage_contacts = self
            .damage_contacts()
            .into_iter()
            .map(|contact| {
                json!({
                    "source": contact.source,
                    "target": contact.target,
                    "damage_kind": contact.damage_kind.as_str(),
                    "direct_damage": contact.direct_damage,
                    "contact_impulse": contact.contact_impulse,
                    "point": contact.point,
                    "impulse_direction": contact.impulse_direction,
                })
            })
            .collect::<Vec<_>>();
        let ballistic_impacts = self
            .ballistic_impacts
            .iter()
            .map(|impact| {
                json!({
                    "source": impact.source,
                    "target": impact.target,
                    "point": impact.point,
                    "normal": impact.normal,
                    "distance": impact.distance,
                    "surface_entity": impact.surface_entity,
                    "surface_id": impact.surface_id,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "enabled": true,
            "fixed_hz": self.settings.fixed_hz,
            "max_steps_per_frame": self.settings.max_steps_per_frame,
            "presentation_lead_seconds": presentation_lead_seconds,
            "presentation_alpha": presentation_alpha,
            "gravity": self.settings.gravity,
            "contact_skin": self.settings.contact_skin,
            "scene_colliders_enabled": self.settings.scene_colliders_enabled,
            "fixed_tick": self.fixed_tick,
            "streamed_mesh_colliders": self.streamed_colliders.len(),
            "surface_resolvable_mesh_colliders": self.streamed_surface_ids
                .values()
                .filter(|ids| !ids.is_empty())
                .count(),
            "pending_streamed_mesh_colliders": self.pending_streamed_colliders.len(),
            "bodies": bodies,
            "events": self.last_output.events,
            "damage_contacts": damage_contacts,
            "ballistic_impacts": ballistic_impacts,
            "report": self.last_output.report,
        })
    }
}
