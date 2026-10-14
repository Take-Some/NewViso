use super::*;

impl PhysicsRuntime {
    /// Immediate upward rain probe against the resident collision BVHs. It uses
    /// both triangle sides and never treats a terrain's global AABB as a roof.
    /// Actor bodies are omitted so the camera's own head cannot shelter it.
    pub(crate) fn weather_camera_is_sheltered(&self, eye: [f32; 3]) -> bool {
        let origin = [eye[0], eye[1] + 0.02, eye[2]];
        let delta = [0.0, 10_000.0, 0.0];
        self.camera_collision_meshes.iter().any(|(id, mesh)| {
            let Some(collider) = self.streamed_colliders.get(id) else {
                return false;
            };
            if collider.flags.is_trigger || !collider.flags.participates_in_queries {
                return false;
            }
            let local_origin = std::array::from_fn(|i| origin[i] - collider.position[i]);
            mesh.sweep(local_origin, delta, 0.01).is_some()
        })
    }


    /// Highest resident triangle surface, ignoring actors and trigger volumes.
    pub(crate) fn weather_surface_height(&self, x: f32, z: f32, top: f32) -> Option<f32> {
        let origin = [x, top, z];
        let delta = [0.0, -512.0, 0.0];
        self.camera_collision_meshes.iter().filter_map(|(id, mesh)| {
            let collider = self.streamed_colliders.get(id)?;
            if collider.flags.is_trigger || !collider.flags.participates_in_queries
                || x < collider.bounds_min[0] || x > collider.bounds_max[0]
                || z < collider.bounds_min[2] || z > collider.bounds_max[2] {
                return None;
            }
            let local = std::array::from_fn(|i| origin[i] - collider.position[i]);
            mesh.sweep(local, delta, 0.001)
                .map(|fraction| top - 512.0 * fraction - 0.001)
        }).max_by(f32::total_cmp)
    }

    pub(crate) fn character_ground_contact_height(&self, entity: u64) -> Option<f32> {
        self.last_output
            .events
            .iter()
            .filter_map(|event| {
                let contact = match event {
                    PhysicsEvent::ContactBegin(c) | PhysicsEvent::ContactPersist(c) => c,
                    _ => return None,
                };
                let supported = (contact.a == entity && contact.normal[1] > 0.45)
                    || (contact.b == entity && contact.normal[1] < -0.45);
                supported.then_some(contact.point[1])
            })
            .max_by(f32::total_cmp)
    }

    pub(crate) fn contact_closing_speed(&self, source: u64, target: u64) -> f32 {
        self.last_output
            .events
            .iter()
            .filter_map(|event| {
                let contact = match event {
                    newviso_physics_client::PhysicsEvent::ContactBegin(c)
                    | newviso_physics_client::PhysicsEvent::ContactPersist(c) => c,
                    _ => return None,
                };
                if !((contact.a == source && contact.b == target)
                    || (contact.b == source && contact.a == target))
                {
                    return None;
                }
                contact.relative_velocity.map(|velocity| {
                    (0..3)
                        .map(|i| velocity[i] * contact.normal[i])
                        .sum::<f32>()
                        .abs()
                })
            })
            .fold(0.0, f32::max)
    }

    pub(crate) fn is_ballistic_damage_contact(&self, contact: &PhysicsDamageContact) -> bool {
        self.ballistic_damage_contacts.iter().any(|hit| {
            hit.source == contact.source
                && hit.target == contact.target
                && hit.point == contact.point
        })
    }

    pub(crate) fn scene_collision_queries_enabled(&self) -> bool {
        self.settings.scene_colliders_enabled && self.settings.scene_participates_in_queries
    }

    /// Resolve against this frame's resident physical geometry before camera submission.
    /// No extra physics step, delayed query result, or render-geometry approximation.
    pub(crate) fn constrain_camera(
        &self,
        origin: [f32; 3],
        desired: [f32; 3],
        radius: f32,
        ignore_entity: Option<u64>,
        scene_solids: &[([f32; 3], [f32; 3])],
    ) -> [f32; 3] {
        let delta = std::array::from_fn(|i| desired[i] - origin[i]);
        let distance = delta.iter().map(|v| v * v).sum::<f32>().sqrt();
        if distance < 1.0e-6 {
            return desired;
        }
        let mut fraction = 1.0_f32;
        for (id, mesh) in &self.camera_collision_meshes {
            let Some(collider) = self.streamed_colliders.get(id) else {
                continue;
            };
            if Some(*id) == ignore_entity
                || collider.flags.is_trigger
                || !collider.flags.participates_in_queries
            {
                continue;
            }
            let local_origin = std::array::from_fn(|i| origin[i] - collider.position[i]);
            if let Some(hit) = mesh.sweep(local_origin, delta, radius) {
                fraction = fraction.min(hit);
            }
        }
        for body in self.bodies.values() {
            if Some(body.entity) == ignore_entity
                || body.flags.is_trigger
                || !body.flags.participates_in_queries
            {
                continue;
            }
            // A sphere swept through conservative local primitive bounds also covers
            // rotated dynamic objects without relying on their axis-aligned snapshot.
            let local_origin = inverse_rotate_camera_vector(
                std::array::from_fn(|i| origin[i] - body.position[i]),
                body.rotation,
            );
            let local_delta = inverse_rotate_camera_vector(delta, body.rotation);
            let (min, max) = shape_bounds(body.shape, [0.0; 3]);
            if let Some(hit) =
                newviso_collision::sweep_sphere_aabb(local_origin, local_delta, radius, min, max)
            {
                fraction = fraction.min(hit);
            }
        }
        if self.settings.scene_colliders_enabled && self.settings.scene_participates_in_queries {
            for (min, max) in scene_solids {
                if let Some(hit) =
                    newviso_collision::sweep_sphere_aabb(origin, delta, radius, *min, *max)
                {
                    fraction = fraction.min(hit);
                }
            }
        }
        // Leave a small numerical skin in addition to the camera volume.
        if fraction < 1.0 {
            fraction = (fraction - 0.01 / distance).max(0.0);
        }
        std::array::from_fn(|i| origin[i] + delta[i] * fraction)
    }

    pub(crate) fn character_sweep_sphere(
        &self,
        origin: [f32; 3],
        delta: [f32; 3],
        radius: f32,
        ignore_entity: Option<u64>,
        scene_solids: &[([f32; 3], [f32; 3])],
    ) -> Option<newviso_character::SweepHit> {
        let mut best: Option<newviso_character::SweepHit> = None;
        let mut consider = |candidate: newviso_character::SweepHit| {
            if best.is_none_or(|current| candidate.fraction < current.fraction) {
                best = Some(candidate);
            }
        };

        for (id, mesh) in &self.camera_collision_meshes {
            let Some(collider) = self.streamed_colliders.get(id) else {
                continue;
            };
            if Some(*id) == ignore_entity
                || collider.flags.is_trigger
                || !collider.flags.participates_in_queries
            {
                continue;
            }
            let local_origin = std::array::from_fn(|i| origin[i] - collider.position[i]);
            if let Some(hit) = mesh.sweep_hit(local_origin, delta, radius) {
                consider(newviso_character::SweepHit {
                    fraction: hit.fraction,
                    normal: rotate_camera_vector(hit.normal, collider.rotation),
                    entity: Some(*id),
                });
            }
        }

        for body in self.bodies.values() {
            if Some(body.entity) == ignore_entity
                || body.flags.is_trigger
                || !body.flags.participates_in_queries
            {
                continue;
            }
            let local_origin = inverse_rotate_camera_vector(
                std::array::from_fn(|i| origin[i] - body.position[i]),
                body.rotation,
            );
            let local_delta = inverse_rotate_camera_vector(delta, body.rotation);
            let (min, max) = shape_bounds(body.shape, [0.0; 3]);
            if let Some(hit) = newviso_collision::sweep_sphere_aabb_hit(
                local_origin,
                local_delta,
                radius,
                min,
                max,
            ) {
                consider(newviso_character::SweepHit {
                    fraction: hit.fraction,
                    normal: rotate_camera_vector(hit.normal, body.rotation),
                    entity: Some(body.entity),
                });
            }
        }

        if self.settings.scene_colliders_enabled && self.settings.scene_participates_in_queries {
            for (index, (min, max)) in scene_solids.iter().enumerate() {
                if let Some(hit) =
                    newviso_collision::sweep_sphere_aabb_hit(origin, delta, radius, *min, *max)
                {
                    consider(newviso_character::SweepHit {
                        fraction: hit.fraction,
                        normal: hit.normal,
                        entity: Some(STATIC_COLLIDER_ID_BASE | index as u64),
                    });
                }
            }
        }

        best
    }

    pub(super) fn snap_body_position_to_ground(
        &self,
        shape: CollisionShape,
        requested: [f32; 3],
        config: &Value,
        ignore_entity: u64,
        command_index: usize,
    ) -> Result<Option<[f32; 3]>, String> {
        if !config.is_object() {
            return Err(format!(
                "script command[{command_index}] ground_snap must be an object"
            ));
        }

        let max_drop = optional_number(config, "max_drop", 64.0)?;
        let max_rise = optional_number(config, "max_rise", 0.75)?;
        let search_radius = optional_number(config, "search_radius", 0.0)?;
        let search_step = optional_number(config, "search_step", 3.0)?;
        let clearance = optional_number(config, "clearance", 0.015)?;
        let min_up_normal = optional_number(config, "min_up_normal", 0.55)?;
        let samples_per_ring = optional_number(config, "samples_per_ring", 16.0)?
            .round()
            .clamp(4.0, 64.0) as usize;

        if max_drop <= 0.0
            || max_rise < 0.0
            || search_radius < 0.0
            || search_step <= 0.0
            || clearance < 0.0
            || !(0.0..=1.0).contains(&min_up_normal)
        {
            return Err(format!(
                "script command[{command_index}] ground_snap parameters are invalid"
            ));
        }

        let (radius, center_to_sphere) = match shape {
            CollisionShape::Sphere { radius } => (radius, 0.0),
            CollisionShape::Capsule {
                radius,
                half_height,
            }
            | CollisionShape::Cylinder {
                radius,
                half_height,
            } => (radius, half_height),
            CollisionShape::Box { half_extents } => {
                let radius = half_extents[0]
                    .min(half_extents[1])
                    .min(half_extents[2])
                    .max(0.01);
                (radius, (half_extents[1] - radius).max(0.0))
            }
        };
        if !radius.is_finite() || radius <= 0.0 {
            return Err(format!(
                "script command[{command_index}] ground_snap requires a positive body radius"
            ));
        }

        let mut offsets = Vec::with_capacity(
            1 + ((search_radius / search_step).ceil() as usize) * samples_per_ring,
        );
        offsets.push([0.0_f32, 0.0_f32]);
        let mut ring = search_step;
        while ring <= search_radius + 1.0e-4 {
            for sample in 0..samples_per_ring {
                let angle = std::f32::consts::TAU * (sample as f32) / (samples_per_ring as f32);
                offsets.push([ring * angle.cos(), ring * angle.sin()]);
            }
            ring += search_step;
        }

        let sweep_distance = max_rise + max_drop;
        for offset in offsets {
            let origin = [
                requested[0] + offset[0],
                requested[1] - center_to_sphere + max_rise,
                requested[2] + offset[1],
            ];
            let Some(hit) = self.character_sweep_sphere(
                origin,
                [0.0, -sweep_distance, 0.0],
                radius,
                Some(ignore_entity),
                &[],
            ) else {
                continue;
            };
            if hit.normal[1] < min_up_normal {
                continue;
            }

            let hit_center_y = origin[1] - sweep_distance * hit.fraction;
            return Ok(Some([
                origin[0],
                hit_center_y + center_to_sphere + clearance,
                origin[2],
            ]));
        }

        Ok(None)
    }
}


#[cfg(test)]
mod weather_shelter_tests {
    use super::*;
    #[test]
    fn weather_shelter_uses_roof_triangles_and_collision_flags() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        let roof = CollisionMeshResource {
            id: newviso_resource_runtime::AssetId(91),
            name: "weather.roof".into(),
            bounds: newviso_collision::CollisionBounds {
                min: [-2.0,6.0,-2.0], max: [2.0,6.0,2.0],
            },
            vertices: vec![[-2.0,6.0,-2.0],[2.0,6.0,-2.0],
                [-2.0,6.0,2.0],[2.0,6.0,2.0]],
            triangles: vec![[0,2,1],[1,2,3]],
            material_indices: vec![0,0],
        };
        runtime.install_streamed_collision(42, &roof, [10.0,0.0,0.0],
            [0.0;3], [1.0;3]).unwrap();
        assert!(runtime.weather_camera_is_sheltered([10.0,3.0,0.0]));
        assert!((runtime.weather_surface_height(10.0, 0.0, 100.0).unwrap() - 6.0).abs() < 0.01);
        assert!(runtime.weather_surface_height(13.0, 0.0, 100.0).is_none());
        assert!(!runtime.weather_camera_is_sheltered([13.0,3.0,0.0]));
        assert!(!runtime.weather_camera_is_sheltered([10.0,7.0,0.0]));
        runtime.streamed_colliders.get_mut(&42).unwrap().flags.is_trigger = true;
        assert!(runtime.weather_surface_height(10.0, 0.0, 100.0).is_none());
        assert!(!runtime.weather_camera_is_sheltered([10.0,3.0,0.0]));
        let collider = runtime.streamed_colliders.get_mut(&42).unwrap();
        collider.flags.is_trigger = false;
        collider.flags.participates_in_queries = false;
        assert!(!runtime.weather_camera_is_sheltered([10.0,3.0,0.0]));
    }
}
