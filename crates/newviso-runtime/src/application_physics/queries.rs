use super::*;
mod body;
mod getup;
mod recovery;
mod support;
pub(super) use body::sweep_body_box;

impl PhysicsRuntime {
    pub(super) fn current_contact_point(&self, entity: u64, point: [f32; 3]) -> bool {
        self.bodies.get(&entity).is_some_and(|b| {
            (0..3).all(|i| {
                point[i] >= b.bounds_min[i] - self.settings.contact_skin
                    && point[i] <= b.bounds_max[i] + self.settings.contact_skin
            })
        })
    }
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
        self.camera_collision_meshes
            .iter()
            .filter_map(|(id, mesh)| {
                let collider = self.streamed_colliders.get(id)?;
                if collider.flags.is_trigger
                    || !collider.flags.participates_in_queries
                    || x < collider.bounds_min[0]
                    || x > collider.bounds_max[0]
                    || z < collider.bounds_min[2]
                    || z > collider.bounds_max[2]
                {
                    return None;
                }
                let local = std::array::from_fn(|i| origin[i] - collider.position[i]);
                mesh.sweep(local, delta, 0.001)
                    .map(|fraction| top - 512.0 * fraction - 0.001)
            })
            .max_by(f32::total_cmp)
    }

    pub(crate) fn character_ground_contact_height(&self, entity: u64) -> Option<f32> {
        // Manifolds describe current support and remain resident between fixed steps.
        // Per-render-frame events are edges and may legitimately be empty.
        if !self.last_output.contact_manifolds.is_empty() {
            return self
                .last_output
                .contact_manifolds
                .iter()
                .filter_map(|m| {
                    let own_a = m.a == entity;
                    if !own_a && m.b != entity {
                        return None;
                    }
                    m.points
                        .iter()
                        .filter_map(|p| {
                            let y = if own_a { p.normal[1] } else { -p.normal[1] };
                            (y > 0.45 && self.current_contact_point(entity, p.position))
                                .then_some(p.position[1])
                        })
                        .max_by(f32::total_cmp)
                })
                .max_by(f32::total_cmp);
        }
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
                (supported && self.current_contact_point(entity, contact.point))
                    .then_some(contact.point[1])
            })
            .max_by(f32::total_cmp)
    }

    /// Query the complete articulated family in one pass through fixed-step
    /// contacts. Entities are sorted once when Endorphin builds its rig.
    /// A contact above the root COM cannot become the actor's standing floor.
    /// Latest support points for a sorted subset of a collision family.
    pub(crate) fn endorphin_contact_points(
        &self,
        entities: &[u64],
        family: &[u64],
        minimum_cosine: f32,
        points: &mut Vec<[f32; 3]>,
    ) {
        points.clear();
        if !self.last_output.contact_manifolds.is_empty() {
            for m in &self.last_output.contact_manifolds {
                let own_a = entities.binary_search(&m.a).is_ok();
                let own_b = entities.binary_search(&m.b).is_ok();
                if own_a == own_b || family.binary_search(&if own_a { m.b } else { m.a }).is_ok() {
                    continue;
                }
                for p in &m.points {
                    let normal_y = if own_a { p.normal[1] } else { -p.normal[1] };
                    if normal_y >= minimum_cosine
                        && self.current_contact_point(if own_a { m.a } else { m.b }, p.position)
                    {
                        points.push(p.position);
                    }
                }
            }
            return;
        }
        let mut seen = BTreeSet::new();
        for event in self.last_output.events.iter().rev() {
            let (a, b, c) = match event {
                PhysicsEvent::ContactBegin(c) | PhysicsEvent::ContactPersist(c) => {
                    (c.a, c.b, Some(c))
                }
                PhysicsEvent::ContactEnd { a, b } => (*a, *b, None),
                _ => continue,
            };
            let own_a = entities.binary_search(&a).is_ok();
            let own_b = entities.binary_search(&b).is_ok();
            if own_a == own_b || !seen.insert((a.min(b), a.max(b))) {
                continue;
            }
            let Some(c) = c else {
                continue;
            };
            let other = if own_a { b } else { a };
            if family.binary_search(&other).is_ok() {
                continue;
            }
            let y = if own_a { c.normal[1] } else { -c.normal[1] };
            if y >= minimum_cosine && self.current_contact_point(if own_a { a } else { b }, c.point)
            {
                points.push(c.point);
            }
        }
    }

    /// Resident materials use the same combined-friction convention as Bullet.
    pub(crate) fn surface_friction(&self, entity: Option<u64>) -> f32 {
        entity
            .and_then(|id| {
                self.bodies
                    .get(&id)
                    .map(|body| body.material.friction)
                    .or_else(|| {
                        self.streamed_colliders
                            .get(&id)
                            .map(|body| body.material.friction)
                    })
            })
            .unwrap_or(self.settings.scene_material.friction)
            .clamp(0.0, 10.0)
    }

    pub(crate) fn endorphin_foot_ground(
        &self,
        point: [f32; 3],
        radius: f32,
        reach: f32,
        family: &[u64],
        scene_solids: &[([f32; 3], [f32; 3])],
        minimum_cosine: f32,
    ) -> Option<newviso_endorphin::StepGround> {
        let origin = [point[0], point[1] + reach, point[2]];
        let delta = [0.0, -2.0 * reach, 0.0];
        let hit =
            self.character_sweep_sphere_ignoring(origin, delta, radius, family, scene_solids)?;
        if hit.normal[1] < minimum_cosine {
            return None;
        }
        let position = std::array::from_fn(|i| origin[i] + delta[i] * hit.fraction);
        // The sweep returns the sole probe centre. Surface motion belongs at
        // the actual contact point, including angular motion of its support.
        let contact = newviso_endorphin::math::sub(position, hit.normal.map(|v| v * radius));
        let velocity = hit
            .entity
            .and_then(|e| self.vehicle_body_state(e))
            .map_or([0.0; 3], |p| {
                newviso_endorphin::math::add(
                    p.linear_velocity,
                    newviso_endorphin::math::cross(
                        p.angular_velocity,
                        newviso_endorphin::math::sub(contact, p.position),
                    ),
                )
            });
        let ground = newviso_endorphin::StepGround {
            position,
            normal: hit.normal,
            velocity,
            support: None,
        };
        match hit
            .entity
            .and_then(|entity| self.vehicle_body_state(entity).map(|p| (entity, p)))
        {
            Some((entity, p)) => ground.bind(
                entity,
                newviso_endorphin::BodyPose {
                    position: p.position,
                    rotation: p.rotation,
                    linear_velocity: p.linear_velocity,
                    angular_velocity: p.angular_velocity,
                },
                radius,
            ),
            None => Some(ground),
        }
    }
    pub(crate) fn endorphin_foot_path_clear(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        radius: f32,
        family: &[u64],
        scene_solids: &[([f32; 3], [f32; 3])],
    ) -> bool {
        let skin = radius * 0.02;
        let from = [from[0], from[1] + skin, from[2]];
        let to = [to[0], to[1] + skin, to[2]];
        let delta = std::array::from_fn(|i| to[i] - from[i]);
        self.character_sweep_sphere_ignoring(from, delta, radius, family, scene_solids)
            .is_none_or(|h| {
                h.fraction >= 1.0 - skin / newviso_endorphin::math::length(delta).max(skin)
                    || newviso_endorphin::math::dot(delta, h.normal) >= 0.0
            })
    }
    pub(crate) fn endorphin_support(
        &self,
        entities: &[u64],
        minimum_cosine: f32,
        max_height: f32,
    ) -> Option<(f32, newviso_endorphin::Support)> {
        let mut seen = BTreeSet::new();
        let mut best: Option<(f32, newviso_endorphin::Support)> = None;
        for event in self.last_output.events.iter().rev() {
            let (a, b, contact) = match event {
                PhysicsEvent::ContactBegin(c) | PhysicsEvent::ContactPersist(c) => {
                    (c.a, c.b, Some(c))
                }
                PhysicsEvent::ContactEnd { a, b } => (*a, *b, None),
                _ => continue,
            };
            let own_a = entities.binary_search(&a).is_ok();
            let own_b = entities.binary_search(&b).is_ok();
            if own_a == own_b || !seen.insert((a.min(b), a.max(b))) {
                continue;
            }
            let Some(contact) = contact else {
                continue;
            };
            let (normal, other) = if own_a {
                (contact.normal, b)
            } else {
                (contact.normal.map(|v| -v), a)
            };
            if normal[1] < minimum_cosine
                || contact.point[1] > max_height
                || !self.current_contact_point(if own_a { a } else { b }, contact.point)
            {
                continue;
            }
            if best.is_some_and(|(height, _)| height >= contact.point[1]) {
                continue;
            }
            let velocity = self.vehicle_body_state(other).map_or([0.0; 3], |p| {
                newviso_endorphin::math::add(
                    p.linear_velocity,
                    newviso_endorphin::math::cross(
                        p.angular_velocity,
                        newviso_endorphin::math::sub(contact.point, p.position),
                    ),
                )
            });
            best = Some((
                contact.point[1],
                newviso_endorphin::Support { normal, velocity },
            ));
        }
        best
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
        self.character_sweep_sphere_ignoring(
            origin,
            delta,
            radius,
            ignore_entity.as_slice(),
            scene_solids,
        )
    }

    pub(crate) fn character_sweep_sphere_ignoring(
        &self,
        origin: [f32; 3],
        delta: [f32; 3],
        radius: f32,
        ignore_entities: &[u64],
        scene_solids: &[([f32; 3], [f32; 3])],
    ) -> Option<newviso_character::SweepHit> {
        self.character_sweep_sphere_ignoring_filtered(
            origin,
            delta,
            radius,
            ignore_entities,
            scene_solids,
            |_| true,
        )
    }

    fn character_sweep_sphere_ignoring_filtered(
        &self,
        origin: [f32; 3],
        delta: [f32; 3],
        radius: f32,
        ignore_entities: &[u64],
        scene_solids: &[([f32; 3], [f32; 3])],
        accept: impl Fn(newviso_character::SweepHit) -> bool,
    ) -> Option<newviso_character::SweepHit> {
        let mut best: Option<newviso_character::SweepHit> = None;
        let mut consider = |candidate: newviso_character::SweepHit| {
            if accept(candidate) && best.is_none_or(|current| candidate.fraction < current.fraction)
            {
                best = Some(candidate);
            }
        };

        for (id, mesh) in &self.camera_collision_meshes {
            let Some(collider) = self.streamed_colliders.get(id) else {
                continue;
            };
            if ignore_entities.contains(id)
                || collider.flags.is_trigger
                || !collider.flags.participates_in_queries
            {
                continue;
            }
            let local_origin = std::array::from_fn(|i| origin[i] - collider.position[i]);
            if let Some(hit) = mesh.sweep_hit_filtered(local_origin, delta, radius, |hit| {
                accept(newviso_character::SweepHit {
                    fraction: hit.fraction,
                    normal: rotate_camera_vector(hit.normal, collider.rotation),
                    entity: Some(*id),
                })
            }) {
                consider(newviso_character::SweepHit {
                    fraction: hit.fraction,
                    normal: rotate_camera_vector(hit.normal, collider.rotation),
                    entity: Some(*id),
                });
            }
        }

        for body in self.bodies.values() {
            if ignore_entities.contains(&body.entity)
                || body.flags.is_trigger
                || !body.flags.participates_in_queries
            {
                continue;
            }
            if let Some(hit) = sweep_body_box(body, origin, delta, radius) {
                consider(hit);
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
                min: [-2.0, 6.0, -2.0],
                max: [2.0, 6.0, 2.0],
            },
            vertices: vec![
                [-2.0, 6.0, -2.0],
                [2.0, 6.0, -2.0],
                [-2.0, 6.0, 2.0],
                [2.0, 6.0, 2.0],
            ],
            triangles: vec![[0, 2, 1], [1, 2, 3]],
            material_indices: vec![0, 0],
        };
        runtime
            .install_streamed_collision(42, &roof, [10.0, 0.0, 0.0], [0.0; 3], [1.0; 3])
            .unwrap();
        assert!(runtime.weather_camera_is_sheltered([10.0, 3.0, 0.0]));
        assert!((runtime.weather_surface_height(10.0, 0.0, 100.0).unwrap() - 6.0).abs() < 0.01);
        assert!(runtime.weather_surface_height(13.0, 0.0, 100.0).is_none());
        assert!(!runtime.weather_camera_is_sheltered([13.0, 3.0, 0.0]));
        assert!(!runtime.weather_camera_is_sheltered([10.0, 7.0, 0.0]));
        runtime
            .streamed_colliders
            .get_mut(&42)
            .unwrap()
            .flags
            .is_trigger = true;
        assert!(runtime.weather_surface_height(10.0, 0.0, 100.0).is_none());
        assert!(!runtime.weather_camera_is_sheltered([10.0, 3.0, 0.0]));
        let collider = runtime.streamed_colliders.get_mut(&42).unwrap();
        collider.flags.is_trigger = false;
        collider.flags.participates_in_queries = false;
        assert!(!runtime.weather_camera_is_sheltered([10.0, 3.0, 0.0]));
    }
}

#[cfg(test)]
mod endorphin_tests {
    use super::*;
    use newviso_physics_client::PhysicsContactEvent;
    fn contact(a: u64, b: u64, point: [f32; 3], normal: [f32; 3]) -> PhysicsContactEvent {
        PhysicsContactEvent {
            a,
            b,
            point,
            normal,
            impulse: 1.0,
            relative_velocity: None,
            materials: Default::default(),
            surface_id: None,
            surface_entity: None,
            surface_triangle: None,
        }
    }
    fn add_supported_body(runtime: &mut PhysicsRuntime, entity: u64, position: [f32; 3]) {
        runtime
            .upsert_body_from_script(
                &json!({"entity":entity,"body_kind":"dynamic",
            "position":position,"shape":{"kind":"box","half_extents":[0.1,0.1,0.1]}}),
                0,
            )
            .unwrap();
    }
    #[test]
    fn ground_height_uses_current_manifolds_without_replaying_frame_events() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        add_supported_body(&mut runtime, 7, [0.0, 0.1, 0.0]);
        runtime.last_output.contact_manifolds=serde_json::from_value(json!([
            {"a":7,"b":100,"points":[{"position":[0.0,0.0,0.0],"normal":[0.0,1.0,0.0],"impulse":1.0}]}
        ])).unwrap();
        assert!(runtime.last_output.events.is_empty());
        assert_eq!(runtime.character_ground_contact_height(7), Some(0.0));
        runtime.last_output.contact_manifolds=serde_json::from_value(json!([
            {"a":100,"b":7,"points":[{"position":[0.0,0.0,0.0],"normal":[0.0,-1.0,0.0],"impulse":1.0}]}
        ])).unwrap();
        assert_eq!(runtime.character_ground_contact_height(7), Some(0.0));
        runtime.last_output.contact_manifolds[0].points[0].normal = [0.0, 1.0, 0.0];
        assert_eq!(
            runtime.character_ground_contact_height(7),
            None,
            "ceiling is not a floor"
        );
        runtime.last_output.contact_manifolds.clear();
        assert_eq!(
            runtime.character_ground_contact_height(7),
            None,
            "ended support is released"
        );
    }

    #[test]
    fn articulated_support_uses_latest_pair_lifetime_and_excludes_own_limbs() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        add_supported_body(&mut runtime, 7, [0.0, 0.1, 0.0]);
        add_supported_body(&mut runtime, 8, [0.0, 0.2, 0.0]);
        runtime.last_output.events = vec![
            PhysicsEvent::ContactBegin(contact(7, 100, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0])),
            PhysicsEvent::ContactEnd { a: 100, b: 7 },
            PhysicsEvent::ContactBegin(contact(7, 8, [0.0, 0.8, 0.0], [0.0, 1.0, 0.0])),
            PhysicsEvent::ContactPersist(contact(100, 8, [0.0, 0.1, 0.0], [0.0, -1.0, 0.0])),
            PhysicsEvent::ContactBegin(contact(7, 200, [0.0, 1.4, 0.0], [0.0, 1.0, 0.0])),
        ];
        let (height, support) = runtime.endorphin_support(&[7, 8], 0.65, 0.2).unwrap();
        assert_eq!(height, 0.1);
        assert_eq!(support.normal, [0.0, 1.0, 0.0]);
        runtime
            .last_output
            .events
            .push(PhysicsEvent::ContactEnd { a: 8, b: 100 });
        assert!(runtime.endorphin_support(&[7, 8], 0.65, 0.2).is_none());
    }
    #[test]
    fn support_velocity_includes_rotating_platform_contact_motion() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        runtime
            .upsert_body_from_script(
                &json!({"entity":100,"body_kind":"dynamic",
            "position":[0,0,0],"linear_velocity":[2,0,0],
            "shape":{"kind":"box","half_extents":[2,0.1,2]}}),
                0,
            )
            .unwrap();
        add_supported_body(&mut runtime, 7, [0.0, 0.2, 2.0]);
        runtime.set_character_angular_velocity(100, [0.0, 1.0, 0.0]);
        runtime.last_output.events = vec![PhysicsEvent::ContactBegin(contact(
            7,
            100,
            [0.0, 0.1, 2.0],
            [0.0, 1.0, 0.0],
        ))];
        let (_, support) = runtime.endorphin_support(&[7], 0.65, 1.0).unwrap();
        assert_eq!(support.velocity, [4.0, 0.0, 0.0]);
    }
}

impl PhysicsRuntime {
    pub(crate) fn melee_world_sweep(
        &self,
        previous: newviso_collision::StrikeCapsule,
        current: newviso_collision::StrikeCapsule,
        ignore: &[u64],
        solids: &[([f32; 3], [f32; 3])],
    ) -> Option<(u64, newviso_collision::StrikeContact)> {
        let mut best: Option<(u64, newviso_collision::StrikeContact)> = None;
        let mut consider = |entity, hit: newviso_collision::StrikeContact| {
            if best.as_ref().is_none_or(|(old, h)| {
                hit.fraction < h.fraction || (hit.fraction == h.fraction && entity < *old)
            }) {
                best = Some((entity, hit));
            }
        };
        for (id, mesh) in &self.camera_collision_meshes {
            let Some(collider) = self.streamed_colliders.get(id) else {
                continue;
            };
            if ignore.binary_search(id).is_ok()
                || collider.flags.is_trigger
                || !collider.flags.participates_in_queries
            {
                continue;
            }
            let local = |c: newviso_collision::StrikeCapsule| newviso_collision::StrikeCapsule {
                start: std::array::from_fn(|i| c.start[i] - collider.position[i]),
                end: std::array::from_fn(|i| c.end[i] - collider.position[i]),
                radius: c.radius,
            };
            if let Some((mut hit, _)) = mesh.sweep_capsule_hit(local(previous), local(current)) {
                for i in 0..3 {
                    hit.point[i] += collider.position[i];
                }
                consider(*id, hit);
            }
        }
        for body in self.bodies.values() {
            if ignore.binary_search(&body.entity).is_ok()
                || body.flags.is_trigger
                || !body.flags.participates_in_queries
            {
                continue;
            }
            let local = |c: newviso_collision::StrikeCapsule| newviso_collision::StrikeCapsule {
                start: inverse_rotate_camera_vector(
                    std::array::from_fn(|i| c.start[i] - body.position[i]),
                    body.rotation,
                ),
                end: inverse_rotate_camera_vector(
                    std::array::from_fn(|i| c.end[i] - body.position[i]),
                    body.rotation,
                ),
                radius: c.radius,
            };
            let (min, max) = shape_bounds(body.shape, [0.0; 3]);
            if let Some(mut hit) =
                newviso_collision::sweep_strike_aabb(local(previous), local(current), min, max)
            {
                hit.point = std::array::from_fn(|i| {
                    rotate_camera_vector(hit.point, body.rotation)[i] + body.position[i]
                });
                hit.normal = rotate_camera_vector(hit.normal, body.rotation);
                consider(body.entity, hit);
            }
        }
        if self.settings.scene_colliders_enabled && self.settings.scene_participates_in_queries {
            for (i, (min, max)) in solids.iter().enumerate() {
                if let Some(hit) =
                    newviso_collision::sweep_strike_aabb(previous, current, *min, *max)
                {
                    consider(STATIC_COLLIDER_ID_BASE | i as u64, hit);
                }
            }
        }
        best
    }
}

impl PhysicsRuntime {
    /// Resident native obstacles participate even when they were created by scripts.
    pub(crate) fn tactical_obstacle_bounds(
        &self,
        min: [f32; 3],
        max: [f32; 3],
    ) -> Vec<([f32; 3], [f32; 3])> {
        self.bodies
            .values()
            .filter(|b| {
                b.kind != PhysicsBodyKind::Dynamic
                    && b.flags.participates_in_queries
                    && !b.flags.is_trigger
            })
            .filter_map(|b| {
                let (lo, hi) = shape_bounds(b.shape, [0.0; 3]);
                let mut a = [f32::INFINITY; 3];
                let mut z = [f32::NEG_INFINITY; 3];
                for x in [lo[0], hi[0]] {
                    for y in [lo[1], hi[1]] {
                        for v in [lo[2], hi[2]] {
                            let p = rotate_camera_vector([x, y, v], b.rotation);
                            for i in 0..3 {
                                a[i] = a[i].min(p[i] + b.position[i]);
                                z[i] = z[i].max(p[i] + b.position[i]);
                            }
                        }
                    }
                }
                (0..3)
                    .all(|i| z[i] >= min[i] && a[i] <= max[i])
                    .then_some((a, z))
            })
            .collect()
    }
    /// A tactical point is accepted only on a real floor, with standing clearance
    /// and a traversable approach. Agent navigation still owns actual movement.
    pub(crate) fn tactical_standing_position(
        &self,
        from: [f32; 3],
        point: [f32; 3],
        height: f32,
        radius: f32,
        clearance: f32,
        minimum_cosine: f32,
        ignore: &[u64],
        solids: &[([f32; 3], [f32; 3])],
    ) -> Option<[f32; 3]> {
        let origin = [point[0], point[1] + height, point[2]];
        let delta = [0.0, -height * 2.0, 0.0];
        let hit = self.character_sweep_sphere_ignoring(origin, delta, radius, ignore, solids)?;
        if hit.normal[1] < minimum_cosine {
            return None;
        }
        let feet = [
            point[0],
            origin[1] + delta[1] * hit.fraction - radius + clearance,
            point[2],
        ];
        for y in [radius + clearance, height * 0.5, height - radius] {
            let goal = [feet[0], feet[1] + y, feet[2]];
            if self
                .character_sweep_sphere_ignoring(goal, [0.0; 3], radius, ignore, solids)
                .is_some()
            {
                return None;
            }
            let start = [from[0], from[1] + y + clearance, from[2]];
            let movement = std::array::from_fn(|i| goal[i] - start[i]);
            if self
                .character_sweep_sphere_ignoring(start, movement, radius, ignore, solids)
                .is_some_and(|h| h.fraction < 1.0)
            {
                return None;
            }
        }
        Some(feet)
    }
}

#[cfg(test)]
mod stale_support_tests {
    use super::*;
    #[test]
    fn moving_a_body_invalidates_spatially_obsolete_support_before_next_physics_tick() {
        let mut physics = PhysicsRuntime::empty_for_tests();
        physics
            .upsert_body_from_script(
                &json!({"entity":2,"body_kind":"dynamic","position":[0,0.9,0],
   "shape":{"kind":"capsule","radius":0.3,"half_height":0.6},"density":70,"casts_contacts":true}),
                0,
            )
            .unwrap();
        let contact = serde_json::from_value::<newviso_physics_client::PhysicsContactEvent>(
            json!({"a":2,"b":99,"point":[0,0,0],"normal":[0,1,0],"impulse":1}),
        )
        .unwrap();
        physics.last_output.events = vec![PhysicsEvent::ContactPersist(contact)];
        assert!(physics.endorphin_support(&[2], 0.65, 3.0).is_some());
        let b = physics.bodies.get_mut(&2).unwrap();
        b.position[1] += 2.0;
        b.bounds_min[1] += 2.0;
        b.bounds_max[1] += 2.0;
        assert!(physics.endorphin_support(&[2], 0.65, 3.0).is_none());
        assert!(physics.character_ground_contact_height(2).is_none());
    }
}
