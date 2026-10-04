use super::*;

impl PhysicsRuntime {
    pub(crate) fn deform_body_hulls(
        &mut self,
        entity: u64,
        point: [f32; 3],
        displacement: [f32; 3],
        radius: f32,
    ) {
        let Some(body) = self.bodies.get_mut(&entity) else {
            return;
        };
        let radius_sq = radius * radius;
        for hull in &mut body.convex_hulls {
            for vertex in hull {
                let distance = (0..3).map(|i| (vertex[i] - point[i]).powi(2)).sum::<f32>();
                if distance < radius_sq {
                    let weight = (1.0 - distance / radius_sq).powi(2);
                    for i in 0..3 {
                        vertex[i] += displacement[i] * weight;
                    }
                }
            }
        }
    }

    pub(crate) fn damage_contacts(&self) -> Vec<PhysicsDamageContact> {
        let mut out = Vec::new();
        for event in &self.last_output.events {
            let contact = match event {
                PhysicsEvent::ContactBegin(contact) | PhysicsEvent::ContactPersist(contact) => {
                    contact
                }
                _ => continue,
            };
            // Provider normals point from B to A. The impulse pushing target B
            // into its body is -normal; target A receives +normal.
            let relative_direction =
                normalize_vec3(std::array::from_fn(|axis| -contact.normal[axis]));

            for (source, target, damage_source, sign) in [
                (
                    contact.a,
                    contact.b,
                    self.damage_sources.get(&contact.a).copied(),
                    1.0_f32,
                ),
                (
                    contact.b,
                    contact.a,
                    self.damage_sources.get(&contact.b).copied(),
                    -1.0_f32,
                ),
            ] {
                let direct_damage = damage_source.map_or(0.0, |source| source.damage);
                let damage_kind = damage_source
                    .map(|source| source.kind)
                    .unwrap_or(PhysicsDamageKind::Collision);
                out.push(PhysicsDamageContact {
                    source,
                    target,
                    damage_kind,
                    direct_damage,
                    contact_impulse: contact.impulse.max(0.0),
                    point: contact.point,
                    impulse_direction: std::array::from_fn(|axis| relative_direction[axis] * sign),
                });
            }
        }
        out.extend(self.ballistic_damage_contacts.iter().copied());
        out
    }

    pub(crate) fn queue_ballistic_ray(
        &mut self,
        source: u64,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        damage: f32,
        impulse: f32,
        ignore_entity: Option<u64>,
        max_hits: u16,
        falloff_min: f32,
        falloff_max: f32,
        falloff_modifier: f32,
    ) -> Result<u64, String> {
        if origin
            .iter()
            .chain(direction.iter())
            .chain([&max_distance, &damage, &impulse])
            .any(|value| !value.is_finite())
        {
            return Err("ballistic ray contains non-finite values".to_owned());
        }
        if max_distance <= 0.0
            || damage < 0.0
            || impulse < 0.0
            || !falloff_min.is_finite()
            || !falloff_max.is_finite()
            || !falloff_modifier.is_finite()
            || falloff_min < 0.0
            || falloff_max < falloff_min
            || !(0.0..=1.0).contains(&falloff_modifier)
        {
            return Err(format!(
                "invalid ballistic ray max_distance={max_distance} damage={damage} impulse={impulse} falloff={falloff_min}..{falloff_max} modifier={falloff_modifier}"
            ));
        }
        let direction = normalize_vec3(direction);
        if direction.iter().map(|v| v * v).sum::<f32>() <= 1.0e-8 {
            return Err("ballistic ray direction is zero".to_owned());
        }
        let seq = self.next_command_seq;
        self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
        self.pending_queries.push(PhysicsQuery {
            seq,
            ignore_entity,
            kind: PhysicsQueryKind::BallisticRay {
                origin,
                dir: direction,
                max_t: max_distance,
                max_hits: max_hits.max(1),
                collide_back_faces: false,
            },
        });
        self.pending_ballistic_damage.insert(
            seq,
            PendingBallisticDamage {
                source,
                damage,
                damage_kind: PhysicsDamageKind::Bullet,
                impulse,
                direction,
                falloff_min,
                falloff_max,
                falloff_modifier,
            },
        );
        Ok(seq)
    }

    pub(crate) fn promote_scene_destructible(
        &mut self,
        activation: SceneDestructionActivation,
        contact: PhysicsDamageContact,
    ) -> Result<(), String> {
        let center = std::array::from_fn(|axis| {
            (activation.bounds_min[axis] + activation.bounds_max[axis]) * 0.5
        });
        let half_extents = std::array::from_fn(|axis| {
            ((activation.bounds_max[axis] - activation.bounds_min[axis]) * 0.5).max(0.03)
        });
        if center
            .iter()
            .chain(half_extents.iter())
            .any(|value| !value.is_finite())
        {
            return Err(format!(
                "destructible scene entity {} has invalid bounds",
                activation.entity
            ));
        }

        let visual_rotation = euler_degrees_to_quaternion(activation.rotation_degrees);
        // The static collider bounds are already world-aligned. Promote that
        // world AABB with an identity physical basis, and retain the authored
        // visual orientation separately to avoid a double rotation.
        let body_rotation = [0.0, 0.0, 0.0, 1.0];
        let world_visual_offset =
            std::array::from_fn(|axis| activation.scene_position[axis] - center[axis]);
        let local_visual_offset = inverse_rotate_camera_vector(world_visual_offset, body_rotation);

        let shape = CollisionShape::Box { half_extents };
        let (bounds_min, bounds_max) = shape_bounds(shape, center);
        self.bodies.insert(
            activation.entity,
            PhysicsBodySnapshot {
                entity: activation.entity,
                kind: PhysicsBodyKind::Dynamic,
                shape,
                flags: PhysicsBodyFlags {
                    is_trigger: false,
                    participates_in_queries: true,
                    casts_contacts: true,
                    continuous_collision: false,
                },
                material: PhysicsMaterial {
                    friction: activation.friction,
                    restitution: activation.restitution,
                    density: activation.density,
                },
                position: center,
                rotation: body_rotation,
                linear_velocity: [0.0; 3],
                angular_velocity: [0.0; 3],
                linear_damping: Some(activation.linear_damping),
                angular_damping: Some(activation.angular_damping),
                mass_properties: None,
                convex_hulls: Vec::new(),
                bounds_min,
                bounds_max,
            },
        );
        self.scene_pose_offsets
            .insert(activation.entity, local_visual_offset);
        self.scene_rotation_offsets
            .insert(activation.entity, visual_rotation);

        let transfer_magnitude =
            contact.contact_impulse.max(contact.direct_damage * 0.4) * activation.impulse_transfer;
        if transfer_magnitude > 1.0e-5 {
            let direction = normalize_vec3(contact.impulse_direction);
            let impulse = std::array::from_fn(|axis| direction[axis] * transfer_magnitude);
            let seq = self.next_command_seq;
            self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
            self.pending_commands.push(PhysicsCommand {
                seq,
                kind: PhysicsCommandKind::ApplyImpulse {
                    entity: activation.entity,
                    impulse,
                    point: contact.point,
                },
            });
        }
        Ok(())
    }

    /// Resolve against this frame's resident physical geometry before camera submission.

    /// Remove a separately represented collision child only when its entire
    /// hull belongs to the detached region. Never discard an enclosing chassis
    /// hull or the last hull merely because its centre overlaps a small panel.
    pub(crate) fn remove_fragment_collision_region(
        &mut self,
        entity: u64,
        center: [f32; 3],
        extent: [f32; 3],
    ) {
        let Some(body) = self.bodies.get_mut(&entity) else {
            return;
        };
        let retained: Vec<_> = body
            .convex_hulls
            .iter()
            .filter(|hull| {
                !hull.iter().all(|point| {
                    (0..3).all(|axis| (point[axis] - center[axis]).abs() <= extent[axis] + 0.04)
                })
            })
            .cloned()
            .collect();
        if !retained.is_empty() {
            body.convex_hulls = retained;
        }
    }
}
