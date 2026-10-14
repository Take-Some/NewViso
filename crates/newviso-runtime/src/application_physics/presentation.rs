use super::*;

impl PhysicsRuntime {
    pub(crate) fn fixed_clock(&self) -> (u64, f32) {
        (self.fixed_tick, self.settings.fixed_dt())
    }
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

    pub(crate) fn body_local_bounds(&self, entity: u64) -> Option<([f32; 3], [f32; 3])> {
        let body = self.bodies.get(&entity)?;
        if body.convex_hulls.is_empty() {
            return Some(shape_bounds(body.shape, [0.0; 3]));
        }
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for vertex in body.convex_hulls.iter().flatten() {
            for i in 0..3 {
                min[i] = min[i].min(vertex[i]);
                max[i] = max[i].max(vertex[i]);
            }
        }
        Some((min, max))
    }

    pub(crate) fn body_local_point(&self, entity: u64, point: [f32; 3]) -> Option<[f32; 3]> {
        let body = self.bodies.get(&entity)?;
        Some(inverse_rotate_camera_vector(
            std::array::from_fn(|i| point[i] - body.position[i]),
            body.rotation,
        ))
    }

    pub(crate) fn restore_body_collision_hulls(&mut self, entity: u64, hulls: Vec<Vec<[f32; 3]>>) {
        if let Some(body) = self.bodies.get_mut(&entity) {
            body.convex_hulls = hulls;
            if let Some((original, live)) = self.body_hull_components.get_mut(&entity) {
                if original.len() == body.convex_hulls.len() {
                    *live = original.clone();
                }
            }
            refresh_bounds(body);
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
        self.state_with_events(&self.last_output.events, "fixed_steps")
    }

    pub(crate) fn script_frame_state(&self, native_particles: &BTreeSet<u64>) -> Value {
        // Native particle simulation already consumes its fixed-step contacts.
        // Scripts receive lifecycle edges and interactions with gameplay bodies,
        // but do not need every continuing shard/terrain or shard/shard manifold.
        let sampled = self
            .last_output
            .events
            .iter()
            .copied()
            .filter(|event| {
                let PhysicsEvent::ContactPersist(contact) = event else {
                    return true;
                };
                if self.damage_sources.contains_key(&contact.a)
                    || self.damage_sources.contains_key(&contact.b)
                {
                    return true;
                }
                let incidental = |particle, other| {
                    native_particles.contains(&particle)
                        && (native_particles.contains(&other)
                            || self
                                .bodies
                                .get(&other)
                                .is_none_or(|body| body.kind == PhysicsBodyKind::Static))
                };
                !incidental(contact.a, contact.b) && !incidental(contact.b, contact.a)
            })
            .collect::<Vec<_>>();
        let events = super::frame_events::compact_frame_events(&sampled);
        let mut state = self.state_with_events(&events, "frame_peaks");
        state["particle_persist_sampling"] = json!("gameplay_pairs");
        state["particle_persist_suppressed"] = json!(self.last_output.events.len() - sampled.len());
        state
    }

    fn state_with_events(&self, contact_events: &[PhysicsEvent], contact_sampling: &str) -> Value {
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
                    "entity_key": body.entity.to_string(),
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
                    "bounds_min": body.bounds_min,
                    "bounds_max": body.bounds_max,
                    "mass_properties":body.mass_properties,
                    "convex_hull_count":body.convex_hulls.len(),
                    "participates_in_queries":body.flags.participates_in_queries,
                    "casts_contacts":body.flags.casts_contacts,
                })
            })
            .collect::<Vec<_>>();

        let damage_contacts = self
            .damage_contacts_from_events(contact_events)
            .into_iter()
            // Collision impulses are already represented by events in script
            // frames. Full diagnostics keep the complete derived contact list.
            .filter(|contact| {
                contact_sampling == "fixed_steps"
                    || contact.direct_damage > 0.0
                    || contact.damage_kind != PhysicsDamageKind::Collision
            })
            .map(|contact| {
                json!({
                    "source": contact.source,
                    "target": contact.target,
                    "source_key": contact.source.to_string(),
                    "target_key": contact.target.to_string(),
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
                    "source_key": impact.source.to_string(),
                    "target_key": impact.target.to_string(),
                    "point": impact.point,
                    "normal": impact.normal,
                    "distance": impact.distance,
                    "surface_entity": impact.surface_entity,
                    "surface_id": impact.surface_id,
                })
            })
            .collect::<Vec<_>>();

        let events = contact_events
            .iter()
            .map(|event| {
                let mut value = serde_json::to_value(event).expect("physics event must serialize");
                if let PhysicsEvent::ContactBegin(contact) | PhysicsEvent::ContactPersist(contact) =
                    event
                {
                    let key = if matches!(event, PhysicsEvent::ContactBegin(_)) {
                        "ContactBegin"
                    } else {
                        "ContactPersist"
                    };
                    value[key]["a_key"] = json!(contact.a.to_string());
                    value[key]["b_key"] = json!(contact.b.to_string());
                }
                value
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
            "collision_pair_filtering":self.supports_collision_filters,
            "disabled_collision_pairs":self.separation_pairs.iter().map(|[a,b]|[a.to_string(),b.to_string()]).collect::<Vec<_>>(),
            "collision_components":self.body_hull_components.iter().map(|(id,(_,ids))|json!({"entity_key":id.to_string(),"components":ids})).collect::<Vec<_>>(),
            "events": events,
            "contact_sampling": contact_sampling,
            "damage_contact_sampling": if contact_sampling == "fixed_steps" { "all_contacts" } else { "direct_sources" },
            "fixed_step_event_count": self.last_output.events.len(),
            "damage_contacts": damage_contacts,
            "ballistic_impacts": ballistic_impacts,
            "report": self.last_output.report,
        })
    }
}
