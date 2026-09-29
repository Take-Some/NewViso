use super::*;

const STATIC_COLLIDER_ID_BASE: u64 = 1_u64 << 63;
const MIN_PHYSICS_HZ: f32 = 10.0;
const MAX_PHYSICS_HZ: f32 = 1000.0;
const MAX_PHYSICS_STEPS_LIMIT: usize = 64;
const SCENE_COLLIDER_BROADPHASE_MARGIN: f32 = 32.0;
const CONTACT_SURFACE_MAX_DISTANCE: f32 = 0.35;

#[derive(Clone, Copy, Debug)]
struct PhysicsWorldSettings {
    fixed_hz: f32,
    max_steps_per_frame: usize,
    gravity: f32,
    contact_skin: f32,
    scene_colliders_enabled: bool,
    scene_material: PhysicsMaterial,
    scene_participates_in_queries: bool,
    scene_casts_contacts: bool,
}

impl Default for PhysicsWorldSettings {
    fn default() -> Self {
        Self {
            fixed_hz: 60.0,
            max_steps_per_frame: 8,
            gravity: 0.0,
            contact_skin: 0.002,
            scene_colliders_enabled: false,
            scene_material: PhysicsMaterial {
                friction: 0.5,
                restitution: 0.0,
                density: 1.0,
            },
            scene_participates_in_queries: true,
            scene_casts_contacts: true,
        }
    }
}

impl PhysicsWorldSettings {
    fn fixed_dt(self) -> f32 {
        1.0 / self.fixed_hz
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PhysicsDamageContact {
    pub source: u64,
    pub target: u64,
    pub direct_damage: f32,
    pub contact_impulse: f32,
    pub point: [f32; 3],
    pub impulse_direction: [f32; 3],
}

pub(super) struct PreparedStreamedCollision {
    entity: u64,
    collider: MeshCollider,
    sweep_mesh: newviso_collision::SphereSweepMesh,
    surface_ids: Vec<u32>,
    position: [f32; 3],
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
    nav_source: newviso_navigation::NavTileSource,
}

#[derive(Debug)]
pub(super) struct PhysicsRuntime {
    client: PhysicsClient,
    bodies: BTreeMap<u64, PhysicsBodySnapshot>,
    streamed_colliders: BTreeMap<u64, PhysicsFrameColliderSnapshot>,
    camera_collision_meshes: BTreeMap<u64, newviso_collision::SphereSweepMesh>,
    streamed_surface_ids: BTreeMap<u64, Vec<u32>>,
    pending_streamed_colliders: BTreeMap<u64, PhysicsFrameColliderSnapshot>,
    pending_commands: Vec<PhysicsCommand>,
    damage_sources: BTreeMap<u64, f32>,
    scene_pose_offsets: BTreeMap<u64, [f32; 3]>,
    scene_rotation_offsets: BTreeMap<u64, [f32; 4]>,
    frame_index: u64,
    fixed_tick: u64,
    next_command_seq: u64,
    accumulator: f32,
    settings: PhysicsWorldSettings,
    last_output: PhysicsFrameOutput,
}

impl PhysicsRuntime {
    #[cfg(test)]
    pub(super) fn empty_for_tests() -> Self {
        Self {
            client: PhysicsClient::new(),
            bodies: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            last_output: PhysicsFrameOutput::default(),
        }
    }

    pub(super) fn connect() -> Result<Self, String> {
        let client = PhysicsClient::new();
        let negotiation = client.negotiate(
            vec![
                PhysicsFeature::StaticColliders,
                PhysicsFeature::DynamicBodies,
                PhysicsFeature::MeshColliders,
            ],
            vec![
                PhysicsFeature::Contacts,
                PhysicsFeature::Queries,
                PhysicsFeature::NativeBackend,
            ],
        )?;

        host::info(
            "newviso.physics",
            format!(
                "physics runtime connected version={}.{}.{} features={:?}",
                negotiation.backend_version.major,
                negotiation.backend_version.minor,
                negotiation.backend_version.patch,
                negotiation.enabled_features
            ),
        );

        Ok(Self {
            client,
            bodies: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            last_output: PhysicsFrameOutput::default(),
        })
    }

    pub(super) fn scene_collider_interests(&self) -> Option<Vec<([f32; 3], [f32; 3])>> {
        if !self.settings.scene_colliders_enabled {
            return None;
        }

        let horizon =
            (self.settings.fixed_dt() * self.settings.max_steps_per_frame as f32).clamp(0.0, 0.25);
        Some(
            self.bodies
                .values()
                .filter(|body| {
                    body.kind != PhysicsBodyKind::Static
                        && body.flags.casts_contacts
                        && !body.flags.is_trigger
                })
                .map(|body| {
                    let mut min = body.bounds_min;
                    let mut max = body.bounds_max;
                    for axis in 0..3 {
                        let swept = body.linear_velocity[axis].abs() * horizon;
                        let margin = SCENE_COLLIDER_BROADPHASE_MARGIN + swept;
                        min[axis] -= margin;
                        max[axis] += margin;
                    }
                    (min, max)
                })
                .collect(),
        )
    }

    pub(super) fn step(
        &mut self,
        dt: f32,
        scene_solids: &[(u64, [f32; 3], [f32; 3])],
        vehicles: &mut VehicleRuntime,
    ) -> Result<(), String> {
        if !dt.is_finite() || dt <= 0.0 {
            return Ok(());
        }

        self.frame_index = self.frame_index.wrapping_add(1);
        self.accumulator += dt;

        let fixed_dt = self.settings.fixed_dt();
        let max_steps = self.settings.max_steps_per_frame;
        let mut steps = 0usize;
        while self.accumulator + 1.0e-7 >= fixed_dt && steps < max_steps {
            self.fixed_tick = self.fixed_tick.wrapping_add(1);

            let mut bodies = if self.settings.scene_colliders_enabled {
                static_scene_bodies(scene_solids, self.settings)
                    .into_iter()
                    // A resident CollisionMeshResource with the same scene entity is
                    // already represented by its exact streamed triangle mesh. Never
                    // add the generic SceneWorld AABB for that entity as a second body:
                    // it can create a false support plane (notably the 1 m fallback
                    // bounds on YBN-only entities).
                    .filter(|body| {
                        !self.bodies.contains_key(&body.entity)
                            && !self.streamed_colliders.contains_key(&body.entity)
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            bodies.extend(self.bodies.values().cloned());

            let vehicle_states = vehicles
                .entity_ids()
                .filter_map(|entity| {
                    self.bodies.get(&entity).map(|body| {
                        (
                            entity,
                            VehicleBodyState {
                                position: body.position,
                                rotation: body.rotation,
                                linear_velocity: body.linear_velocity,
                                angular_velocity: body.angular_velocity,
                            },
                        )
                    })
                })
                .collect::<BTreeMap<_, _>>();
            let vehicle_plan =
                vehicles.prepare_frame(fixed_dt, self.settings.gravity.abs(), &vehicle_states);

            let mut commands = if steps == 0 {
                std::mem::take(&mut self.pending_commands)
            } else {
                Vec::new()
            };
            for impulse in vehicle_plan.impulses {
                let seq = self.next_command_seq;
                self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
                commands.push(PhysicsCommand {
                    seq,
                    kind: PhysicsCommandKind::ApplyImpulse {
                        entity: impulse.vehicle,
                        impulse: impulse.impulse,
                        point: impulse.point,
                    },
                });
            }
            for angular in vehicle_plan.angular_velocity_deltas {
                let Some(body) = self.bodies.get(&angular.vehicle) else {
                    continue;
                };
                let velocity =
                    std::array::from_fn(|axis| body.angular_velocity[axis] + angular.delta[axis]);
                let seq = self.next_command_seq;
                self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
                commands.push(PhysicsCommand {
                    seq,
                    kind: PhysicsCommandKind::SetAngularVelocity {
                        entity: angular.vehicle,
                        velocity,
                    },
                });
            }

            let colliders = if steps == 0 {
                std::mem::take(&mut self.pending_streamed_colliders)
                    .into_values()
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let queries = vehicle_plan
                .probes
                .into_iter()
                .map(|probe| PhysicsQuery {
                    seq: probe.seq,
                    ignore_entity: Some(probe.vehicle),
                    kind: PhysicsQueryKind::Ray {
                        origin: probe.origin,
                        dir: probe.direction,
                        max_t: probe.max_distance,
                    },
                })
                .collect::<Vec<_>>();

            let input = PhysicsFrameInput {
                frame_index: self.frame_index,
                fixed_tick: self.fixed_tick,
                dt: fixed_dt,
                gravity: self.settings.gravity,
                contact_skin: self.settings.contact_skin,
                bodies,
                colliders,
                commands,
                queries,
            };
            let retry_colliders = input.colliders.clone();
            let retry_commands = input.commands.clone();
            let mut output = match self.client.step_frame(input) {
                Ok(output) => output,
                Err(error) => {
                    for collider in retry_colliders {
                        self.pending_streamed_colliders
                            .insert(collider.entity, collider);
                    }
                    if !retry_commands.is_empty() {
                        self.pending_commands.splice(0..0, retry_commands);
                    }
                    return Err(error);
                }
            };

            self.enrich_contact_surfaces(&mut output);
            self.apply_output(&output);
            let vehicle_hits = output
                .query_hits
                .iter()
                .map(|hit| VehicleProbeHit {
                    seq: hit.seq,
                    position: hit.position,
                    normal: hit.normal,
                    distance: hit.distance,
                    surface_entity: Some(hit.entity),
                    surface_id: self.resolve_surface_id(hit.entity, hit.position),
                })
                .collect::<Vec<_>>();
            vehicles.accept_probe_hits(&vehicle_hits);
            self.last_output = output;
            self.accumulator -= fixed_dt;
            steps += 1;
        }

        if steps == max_steps && self.accumulator >= fixed_dt {
            self.accumulator = self.accumulator.min(fixed_dt);
        }

        Ok(())
    }

    fn resolve_surface_id(&self, entity: u64, world_point: [f32; 3]) -> Option<u32> {
        let surface_ids = self.streamed_surface_ids.get(&entity)?;
        if surface_ids.is_empty() {
            return None;
        }
        let mesh = self.camera_collision_meshes.get(&entity)?;
        let collider = self.streamed_colliders.get(&entity)?;
        let local_point = std::array::from_fn(|axis| world_point[axis] - collider.position[axis]);
        let hit = mesh.nearest_triangle(local_point, CONTACT_SURFACE_MAX_DISTANCE)?;
        surface_ids.get(hit.triangle_index as usize).copied()
    }

    fn enrich_contact_surfaces(&self, output: &mut PhysicsFrameOutput) {
        for event in &mut output.events {
            let contact = match event {
                PhysicsEvent::ContactBegin(contact) | PhysicsEvent::ContactPersist(contact) => {
                    contact
                }
                _ => continue,
            };

            // Provider DTOs remain replaceable and may not expose a native mesh subshape id.
            // Resolve the exact authored triangle against the already-resident collision BVH.
            contact.surface_id = None;
            contact.surface_entity = None;
            contact.surface_triangle = None;
            for entity in [contact.a, contact.b] {
                let Some(surface_ids) = self.streamed_surface_ids.get(&entity) else {
                    continue;
                };
                if surface_ids.is_empty() {
                    continue;
                }
                let Some(mesh) = self.camera_collision_meshes.get(&entity) else {
                    continue;
                };
                let Some(collider) = self.streamed_colliders.get(&entity) else {
                    continue;
                };
                let local_point =
                    std::array::from_fn(|axis| contact.point[axis] - collider.position[axis]);
                let Some(hit) = mesh.nearest_triangle(local_point, CONTACT_SURFACE_MAX_DISTANCE)
                else {
                    continue;
                };
                let Some(surface_id) = surface_ids.get(hit.triangle_index as usize).copied() else {
                    continue;
                };

                contact.surface_id = Some(surface_id);
                contact.surface_entity = Some(entity);
                contact.surface_triangle = Some(hit.triangle_index);
                break;
            }
        }
    }

    fn apply_output(&mut self, output: &PhysicsFrameOutput) {
        for pose in &output.pose_updates {
            let Some(body) = self.bodies.get_mut(&pose.entity) else {
                continue;
            };
            body.position = pose.position;
            body.rotation = pose.rotation;
            refresh_bounds(body);
        }

        for velocity in &output.velocity_updates {
            let Some(body) = self.bodies.get_mut(&velocity.entity) else {
                continue;
            };
            body.linear_velocity = velocity.linear_velocity;
            body.angular_velocity = velocity.angular_velocity;
        }
    }

    pub(super) fn prepare_streamed_collision(
        entity: u64,
        collision: &CollisionMeshResource,
        position: [f32; 3],
        rotation_degrees: [f32; 3],
        scale: [f32; 3],
    ) -> Result<PreparedStreamedCollision, String> {
        collision.validate()?;
        if position
            .iter()
            .chain(rotation_degrees.iter())
            .chain(scale.iter())
            .any(|value| !value.is_finite())
        {
            return Err(format!(
                "streamed collision '{}' has non-finite placement transform",
                collision.name
            ));
        }

        let vertices = collision
            .vertices
            .iter()
            .copied()
            .map(|vertex| transform_collision_vertex(vertex, scale, rotation_degrees))
            .collect::<Vec<_>>();
        let collider = MeshCollider {
            vertices,
            triangles: collision.triangles.clone(),
            // RSC7 surface ids remain preserved on CollisionMeshResource. The current
            // Jolt packet ABI has no mesh-material table, so every physics triangle must
            // resolve to the single collider material until surface mapping is added.
            material_indices: vec![0; collision.triangles.len()],
        };
        collider.validate()?;
        let (local_min, local_max) = collider_bounds(&collider)?;
        let bounds_min = [
            local_min[0] + position[0],
            local_min[1] + position[1],
            local_min[2] + position[2],
        ];
        let bounds_max = [
            local_max[0] + position[0],
            local_max[1] + position[1],
            local_max[2] + position[2],
        ];

        // BVH construction is deliberately part of background preparation.
        // Large North Yankton collision meshes must never build this tree on
        // the gameplay/owner thread.
        let sweep_mesh =
            newviso_collision::SphereSweepMesh::new(&collider.vertices, &collider.triangles)?;
        let nav_source = newviso_navigation::NavTileSource {
            id: format!("scene.collider.{entity}"),
            vertices: collider
                .vertices
                .iter()
                .map(|vertex| {
                    [
                        vertex[0] + position[0],
                        vertex[1] + position[1],
                        vertex[2] + position[2],
                    ]
                })
                .collect(),
            triangles: collider.triangles.clone(),
        };

        Ok(PreparedStreamedCollision {
            entity,
            collider,
            sweep_mesh,
            surface_ids: collision.material_indices.clone(),
            position,
            bounds_min,
            bounds_max,
            nav_source,
        })
    }

    pub(super) fn install_prepared_streamed_collision(
        &mut self,
        prepared: PreparedStreamedCollision,
    ) -> Result<(bool, Option<newviso_navigation::NavTileSource>), String> {
        let PreparedStreamedCollision {
            entity,
            collider,
            sweep_mesh,
            surface_ids,
            position,
            bounds_min,
            bounds_max,
            nav_source,
        } = prepared;

        let snapshot = PhysicsFrameColliderSnapshot {
            entity,
            collider: PhysicsCollider::Mesh(collider),
            flags: PhysicsBodyFlags {
                is_trigger: false,
                participates_in_queries: true,
                casts_contacts: true,
                continuous_collision: false,
            },
            material: self.settings.scene_material,
            position,
            rotation: [0.0, 0.0, 0.0, 1.0],
            bounds_min,
            bounds_max,
        };

        if self.streamed_colliders.get(&entity) == Some(&snapshot)
            && self.streamed_surface_ids.get(&entity) == Some(&surface_ids)
        {
            return Ok((false, None));
        }

        self.camera_collision_meshes.insert(entity, sweep_mesh);
        self.streamed_surface_ids.insert(entity, surface_ids);
        self.streamed_colliders.insert(entity, snapshot.clone());
        self.pending_streamed_colliders.insert(entity, snapshot);
        Ok((true, Some(nav_source)))
    }

    #[cfg(test)]
    pub(super) fn install_streamed_collision(
        &mut self,
        entity: u64,
        collision: &CollisionMeshResource,
        position: [f32; 3],
        rotation_degrees: [f32; 3],
        scale: [f32; 3],
    ) -> Result<bool, String> {
        let prepared =
            Self::prepare_streamed_collision(entity, collision, position, rotation_degrees, scale)?;
        self.install_prepared_streamed_collision(prepared)
            .map(|(changed, _)| changed)
    }

    pub(super) fn remove_streamed_collision(&mut self, entity: u64) -> bool {
        self.pending_streamed_colliders.remove(&entity);
        self.camera_collision_meshes.remove(&entity);
        self.streamed_surface_ids.remove(&entity);
        let removed = self.streamed_colliders.remove(&entity).is_some();
        if removed {
            let seq = self.next_command_seq;
            self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
            self.pending_commands.push(PhysicsCommand {
                seq,
                kind: PhysicsCommandKind::DestroyBody { entity },
            });
        }
        removed
    }

    pub(super) fn scene_activity_updates(&self) -> Vec<PhysicsBodyActivityUpdate> {
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

    pub(super) fn scene_pose_updates(&self) -> Vec<PhysicsBodyPoseUpdate> {
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

    pub(super) fn damage_contacts(&self) -> Vec<PhysicsDamageContact> {
        let mut out = Vec::new();
        for event in &self.last_output.events {
            let PhysicsEvent::ContactBegin(contact) = event else {
                continue;
            };
            let relative = contact.relative_velocity.unwrap_or([0.0; 3]);
            let relative_length = relative
                .iter()
                .map(|value| value * value)
                .sum::<f32>()
                .sqrt();
            let relative_direction = if relative_length > 1.0e-5 {
                std::array::from_fn(|axis| relative[axis] / relative_length)
            } else {
                normalize_vec3(contact.normal)
            };

            for (source, target, direct_damage, sign) in [
                (
                    contact.a,
                    contact.b,
                    self.damage_sources.get(&contact.a).copied().unwrap_or(0.0),
                    1.0_f32,
                ),
                (
                    contact.b,
                    contact.a,
                    self.damage_sources.get(&contact.b).copied().unwrap_or(0.0),
                    -1.0_f32,
                ),
            ] {
                out.push(PhysicsDamageContact {
                    source,
                    target,
                    direct_damage,
                    contact_impulse: contact.impulse.max(0.0),
                    point: contact.point,
                    impulse_direction: std::array::from_fn(|axis| relative_direction[axis] * sign),
                });
            }
        }
        out
    }

    pub(super) fn promote_scene_destructible(
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
    /// No extra physics step, delayed query result, or render-geometry approximation.
    pub(super) fn constrain_camera(
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

    #[cfg(test)]
    pub(super) fn navigation_tile_source(
        &self,
        entity: u64,
    ) -> Option<newviso_navigation::NavTileSource> {
        let snapshot = self.streamed_colliders.get(&entity)?;
        let PhysicsCollider::Mesh(mesh) = &snapshot.collider else {
            return None;
        };
        Some(newviso_navigation::NavTileSource {
            id: format!("scene.collider.{entity}"),
            vertices: mesh
                .vertices
                .iter()
                .map(|vertex| {
                    [
                        vertex[0] + snapshot.position[0],
                        vertex[1] + snapshot.position[1],
                        vertex[2] + snapshot.position[2],
                    ]
                })
                .collect(),
            triangles: mesh.triangles.clone(),
        })
    }

    pub(super) fn character_sweep_sphere(
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

    fn snap_body_position_to_ground(
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

    pub(super) fn body_linear_velocity(&self, entity: u64) -> [f32; 3] {
        self.bodies
            .get(&entity)
            .map(|body| body.linear_velocity)
            .unwrap_or([0.0; 3])
    }

    pub(super) fn has_body(&self, entity: u64) -> bool {
        self.bodies.contains_key(&entity)
    }

    pub(super) fn runtime_state(&self) -> Value {
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
            "report": self.last_output.report,
        })
    }

    pub(super) fn configure_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let mut next = self.settings;

        if command.get("fixed_hz").is_some() {
            next.fixed_hz = command_number(command, "fixed_hz", command_index)?;
        }
        if !(MIN_PHYSICS_HZ..=MAX_PHYSICS_HZ).contains(&next.fixed_hz) {
            return Err(format!(
                "script command[{command_index}] physics.world.configure fixed_hz must be in {MIN_PHYSICS_HZ}..={MAX_PHYSICS_HZ}"
            ));
        }

        if let Some(value) = command.get("max_steps_per_frame") {
            let value = value.as_u64().ok_or_else(|| {
                format!(
                    "script command[{command_index}] physics.world.configure max_steps_per_frame must be unsigned integer"
                )
            })?;
            let value = usize::try_from(value).map_err(|_| {
                format!(
                    "script command[{command_index}] physics.world.configure max_steps_per_frame out of range"
                )
            })?;
            if value == 0 || value > MAX_PHYSICS_STEPS_LIMIT {
                return Err(format!(
                    "script command[{command_index}] physics.world.configure max_steps_per_frame must be in 1..={MAX_PHYSICS_STEPS_LIMIT}"
                ));
            }
            next.max_steps_per_frame = value;
        }

        if command.get("gravity").is_some() {
            next.gravity = command_number(command, "gravity", command_index)?;
        }
        if !next.gravity.is_finite() || next.gravity.abs() > 1000.0 {
            return Err(format!(
                "script command[{command_index}] physics.world.configure gravity is invalid"
            ));
        }

        if command.get("contact_skin").is_some() {
            next.contact_skin = command_number(command, "contact_skin", command_index)?;
        }
        if !next.contact_skin.is_finite() || !(0.0..=1.0).contains(&next.contact_skin) {
            return Err(format!(
                "script command[{command_index}] physics.world.configure contact_skin must be in 0..=1"
            ));
        }

        if let Some(scene) = command.get("scene_colliders") {
            let scene = scene.as_object().ok_or_else(|| {
                format!(
                    "script command[{command_index}] physics.world.configure scene_colliders must be an object"
                )
            })?;
            if let Some(enabled) = scene.get("enabled") {
                next.scene_colliders_enabled = enabled.as_bool().ok_or_else(|| {
                    format!(
                        "script command[{command_index}] physics.world.configure scene_colliders.enabled must be boolean"
                    )
                })?;
            }
            if let Some(value) = scene.get("friction") {
                next.scene_material.friction = value.as_f64().map(|v| v as f32).filter(|v| v.is_finite()).ok_or_else(|| {
                    format!("script command[{command_index}] physics.world.configure scene_colliders.friction must be finite numeric")
                })?;
            }
            if let Some(value) = scene.get("restitution") {
                next.scene_material.restitution = value.as_f64().map(|v| v as f32).filter(|v| v.is_finite()).ok_or_else(|| {
                    format!("script command[{command_index}] physics.world.configure scene_colliders.restitution must be finite numeric")
                })?;
            }
            if let Some(value) = scene.get("density") {
                next.scene_material.density = value.as_f64().map(|v| v as f32).filter(|v| v.is_finite()).ok_or_else(|| {
                    format!("script command[{command_index}] physics.world.configure scene_colliders.density must be finite numeric")
                })?;
            }
            if let Some(value) = scene.get("participates_in_queries") {
                next.scene_participates_in_queries = value.as_bool().ok_or_else(|| {
                    format!("script command[{command_index}] physics.world.configure scene_colliders.participates_in_queries must be boolean")
                })?;
            }
            if let Some(value) = scene.get("casts_contacts") {
                next.scene_casts_contacts = value.as_bool().ok_or_else(|| {
                    format!("script command[{command_index}] physics.world.configure scene_colliders.casts_contacts must be boolean")
                })?;
            }
        }

        if next.scene_material.friction < 0.0
            || next.scene_material.restitution < 0.0
            || next.scene_material.density <= 0.0
        {
            return Err(format!(
                "script command[{command_index}] physics.world.configure scene collider material is invalid"
            ));
        }

        self.settings = next;
        self.accumulator = self.accumulator.min(next.fixed_dt());
        Ok(())
    }

    pub(super) fn upsert_body_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let entity = command_u64(command, "entity", command_index)?;
        if entity >= STATIC_COLLIDER_ID_BASE {
            return Err(format!(
                "script command[{command_index}] physics body entity {entity} is in the reserved engine range"
            ));
        }

        let body_kind = match command
            .get("body_kind")
            .or_else(|| command.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("dynamic")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "static" => PhysicsBodyKind::Static,
            "dynamic" => PhysicsBodyKind::Dynamic,
            "kinematic" => PhysicsBodyKind::Kinematic,
            other => {
                return Err(format!(
                    "script command[{command_index}] physics.body.upsert has invalid body_kind '{other}'"
                ))
            }
        };

        let shape_value = command.get("shape").ok_or_else(|| {
            format!("script command[{command_index}] physics.body.upsert requires object 'shape'")
        })?;
        let shape_kind = shape_value
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "script command[{command_index}] physics.body.upsert shape requires string 'kind'"
                )
            })?
            .trim()
            .to_ascii_lowercase();

        let shape = match shape_kind.as_str() {
            "sphere" => CollisionShape::Sphere {
                radius: command_number(shape_value, "radius", command_index)?,
            },
            "box" => CollisionShape::Box {
                half_extents: command_vec3(shape_value, "half_extents", command_index)?,
            },
            "capsule" => CollisionShape::Capsule {
                radius: command_number(shape_value, "radius", command_index)?,
                half_height: command_number(shape_value, "half_height", command_index)?,
            },
            "cylinder" => CollisionShape::Cylinder {
                radius: command_number(shape_value, "radius", command_index)?,
                half_height: command_number(shape_value, "half_height", command_index)?,
            },
            other => {
                return Err(format!(
                    "script command[{command_index}] physics.body.upsert has invalid shape kind '{other}'"
                ))
            }
        };

        let mut position = command_vec3(command, "position", command_index)?;
        let rotation = command
            .get("rotation")
            .map(|_| command_vec4(command, "rotation", command_index))
            .transpose()?
            .unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let linear_velocity = command
            .get("linear_velocity")
            .map(|_| command_vec3(command, "linear_velocity", command_index))
            .transpose()?
            .unwrap_or([0.0; 3]);
        let angular_velocity = command
            .get("angular_velocity")
            .map(|_| command_vec3(command, "angular_velocity", command_index))
            .transpose()?
            .unwrap_or([0.0; 3]);

        if let Some(ground_snap) = command.get("ground_snap") {
            let required = optional_bool(ground_snap, "required", false)?;
            match self.snap_body_position_to_ground(
                shape,
                position,
                ground_snap,
                entity,
                command_index,
            )? {
                Some(snapped) => position = snapped,
                None if required => {
                    host::debug(
                        "newviso.physics",
                        format!(
                            "deferred physics.body.upsert entity={} because required ground support is not resident",
                            entity
                        ),
                    );
                    return Ok(());
                }
                None => {}
            }
        }

        let material = PhysicsMaterial {
            friction: optional_number(command, "friction", 0.55)?,
            restitution: optional_number(command, "restitution", 0.25)?,
            density: optional_number(command, "density", 1.0)?,
        };
        let flags = PhysicsBodyFlags {
            is_trigger: optional_bool(command, "is_trigger", false)?,
            participates_in_queries: optional_bool(command, "participates_in_queries", true)?,
            casts_contacts: optional_bool(command, "casts_contacts", true)?,
            continuous_collision: optional_bool(command, "continuous_collision", false)?,
        };

        let linear_damping = optional_nullable_number(command, "linear_damping")?;
        let angular_damping = optional_nullable_number(command, "angular_damping")?;
        let (bounds_min, bounds_max) = shape_bounds(shape, position);

        self.bodies.insert(
            entity,
            PhysicsBodySnapshot {
                entity,
                kind: body_kind,
                shape,
                flags,
                material,
                position,
                rotation,
                linear_velocity,
                angular_velocity,
                linear_damping,
                angular_damping,
                bounds_min,
                bounds_max,
            },
        );
        match optional_nullable_number(command, "damage")? {
            Some(damage) if damage < 0.0 => {
                return Err(format!(
                "script command[{command_index}] physics.body.upsert damage must be non-negative"
            ))
            }
            Some(damage) if damage > 0.0 => {
                self.damage_sources.insert(entity, damage);
            }
            _ => {
                self.damage_sources.remove(&entity);
            }
        }
        Ok(())
    }

    pub(super) fn destroy_body_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let entity = command_u64(command, "entity", command_index)?;
        self.bodies.remove(&entity);
        self.damage_sources.remove(&entity);
        self.scene_pose_offsets.remove(&entity);
        self.scene_rotation_offsets.remove(&entity);
        Ok(())
    }

    pub(super) fn set_body_velocity_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let entity = command_u64(command, "entity", command_index)?;
        if !self.bodies.contains_key(&entity) {
            return Err(format!(
                "script command[{command_index}] physics.body.velocity.set references unknown body {entity}"
            ));
        }
        let velocity = command_vec3(command, "velocity", command_index)?;
        let seq = self.next_command_seq;
        self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
        self.pending_commands.push(PhysicsCommand {
            seq,
            kind: PhysicsCommandKind::SetLinearVelocity { entity, velocity },
        });
        Ok(())
    }

    pub(super) fn set_body_pose_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let entity = command_u64(command, "entity", command_index)?;
        if !self.bodies.contains_key(&entity) {
            return Err(format!(
                "script command[{command_index}] physics.body.pose.set references unknown body {entity}"
            ));
        }
        let shape = self
            .bodies
            .get(&entity)
            .map(|body| body.shape)
            .expect("body existence checked above");
        let mut position = command_vec3(command, "position", command_index)?;
        if let Some(ground_snap) = command.get("ground_snap") {
            let required = optional_bool(ground_snap, "required", false)?;
            match self.snap_body_position_to_ground(
                shape,
                position,
                ground_snap,
                entity,
                command_index,
            )? {
                Some(snapped) => position = snapped,
                None if required => return Ok(()),
                None => {}
            }
        }
        let rotation = command
            .get("rotation")
            .map(|_| command_vec4(command, "rotation", command_index))
            .transpose()?
            .unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let seq = self.next_command_seq;
        self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
        self.pending_commands.push(PhysicsCommand {
            seq,
            kind: PhysicsCommandKind::SetBodyPose {
                entity,
                position,
                rotation,
            },
        });
        Ok(())
    }

    pub(super) fn apply_impulse_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let entity = command_u64(command, "entity", command_index)?;
        let impulse = command_vec3(command, "impulse", command_index)?;
        let point = command
            .get("point")
            .map(|_| command_vec3(command, "point", command_index))
            .transpose()?
            .or_else(|| self.bodies.get(&entity).map(|body| body.position))
            .ok_or_else(|| {
                format!(
                    "script command[{command_index}] physics.body.impulse references unknown body {entity}"
                )
            })?;

        let seq = self.next_command_seq;
        self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
        self.pending_commands.push(PhysicsCommand {
            seq,
            kind: PhysicsCommandKind::ApplyImpulse {
                entity,
                impulse,
                point,
            },
        });
        Ok(())
    }
}

fn rotate_camera_vector(v: [f32; 3], q: [f32; 4]) -> [f32; 3] {
    let norm = q.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < 1.0e-8 {
        return v;
    }
    let [x, y, z, w] = [q[0] / norm, q[1] / norm, q[2] / norm, q[3] / norm];
    let t = [
        2.0 * (y * v[2] - z * v[1]),
        2.0 * (z * v[0] - x * v[2]),
        2.0 * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + y * t[2] - z * t[1],
        v[1] + w * t[1] + z * t[0] - x * t[2],
        v[2] + w * t[2] + x * t[1] - y * t[0],
    ]
}

fn inverse_rotate_camera_vector(v: [f32; 3], q: [f32; 4]) -> [f32; 3] {
    let norm = q.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < 1.0e-8 {
        return v;
    }
    let [x, y, z, w] = [-q[0] / norm, -q[1] / norm, -q[2] / norm, q[3] / norm];
    let t = [
        2.0 * (y * v[2] - z * v[1]),
        2.0 * (z * v[0] - x * v[2]),
        2.0 * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + y * t[2] - z * t[1],
        v[1] + w * t[1] + z * t[0] - x * t[2],
        v[2] + w * t[2] + x * t[1] - y * t[0],
    ]
}

fn quaternion_multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    let q = [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ];
    let norm = q.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm <= 1.0e-8 || !norm.is_finite() {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        q.map(|value| value / norm)
    }
}

fn normalize_vec3(value: [f32; 3]) -> [f32; 3] {
    let length = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if !length.is_finite() || length <= 1.0e-6 {
        return [0.0, 1.0, 0.0];
    }
    std::array::from_fn(|axis| value[axis] / length)
}

fn euler_degrees_to_quaternion(rotation_degrees: [f32; 3]) -> [f32; 4] {
    let [rx, ry, rz] = rotation_degrees.map(|value| value.to_radians() * 0.5);
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    let quaternion = [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ];
    let norm = quaternion
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if norm <= 1.0e-8 || !norm.is_finite() {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        quaternion.map(|value| value / norm)
    }
}

fn transform_collision_vertex(
    vertex: [f32; 3],
    scale: [f32; 3],
    rotation_degrees: [f32; 3],
) -> [f32; 3] {
    let mut x = vertex[0] * scale[0];
    let mut y = vertex[1] * scale[1];
    let mut z = vertex[2] * scale[2];

    let rx = rotation_degrees[0].to_radians();
    let (sin_x, cos_x) = rx.sin_cos();
    let next_y = y * cos_x - z * sin_x;
    let next_z = y * sin_x + z * cos_x;
    y = next_y;
    z = next_z;

    let ry = rotation_degrees[1].to_radians();
    let (sin_y, cos_y) = ry.sin_cos();
    let next_x = x * cos_y + z * sin_y;
    let next_z = -x * sin_y + z * cos_y;
    x = next_x;
    z = next_z;

    let rz = rotation_degrees[2].to_radians();
    let (sin_z, cos_z) = rz.sin_cos();
    let next_x = x * cos_z - y * sin_z;
    let next_y = x * sin_z + y * cos_z;
    [next_x, next_y, z]
}

fn collider_bounds(collider: &MeshCollider) -> Result<([f32; 3], [f32; 3]), String> {
    let first = *collider
        .vertices
        .first()
        .ok_or_else(|| "physics mesh collider is empty".to_owned())?;
    let mut min = first;
    let mut max = first;
    for vertex in collider.vertices.iter().skip(1) {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    Ok((min, max))
}

fn static_scene_bodies(
    solids: &[(u64, [f32; 3], [f32; 3])],
    settings: PhysicsWorldSettings,
) -> Vec<PhysicsBodySnapshot> {
    solids
        .iter()
        .map(|(entity, min, max)| {
            let center = [
                (min[0] + max[0]) * 0.5,
                (min[1] + max[1]) * 0.5,
                (min[2] + max[2]) * 0.5,
            ];
            let half_extents = [
                ((max[0] - min[0]) * 0.5).max(0.001),
                ((max[1] - min[1]) * 0.5).max(0.001),
                ((max[2] - min[2]) * 0.5).max(0.001),
            ];
            PhysicsBodySnapshot {
                entity: *entity,
                kind: PhysicsBodyKind::Static,
                shape: CollisionShape::Box { half_extents },
                flags: PhysicsBodyFlags {
                    is_trigger: false,
                    participates_in_queries: settings.scene_participates_in_queries,
                    casts_contacts: settings.scene_casts_contacts,
                    continuous_collision: false,
                },
                material: settings.scene_material,
                position: center,
                rotation: [0.0, 0.0, 0.0, 1.0],
                linear_velocity: [0.0; 3],
                angular_velocity: [0.0; 3],
                linear_damping: Some(0.0),
                angular_damping: Some(0.0),
                bounds_min: *min,
                bounds_max: *max,
            }
        })
        .collect()
}

fn shape_bounds(shape: CollisionShape, position: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let half = match shape {
        CollisionShape::Box { half_extents } => half_extents,
        CollisionShape::Sphere { radius } => [radius; 3],
        CollisionShape::Capsule {
            radius,
            half_height,
        }
        | CollisionShape::Cylinder {
            radius,
            half_height,
        } => [radius, radius + half_height, radius],
    };
    (
        [
            position[0] - half[0],
            position[1] - half[1],
            position[2] - half[2],
        ],
        [
            position[0] + half[0],
            position[1] + half[1],
            position[2] + half[2],
        ],
    )
}

fn refresh_bounds(body: &mut PhysicsBodySnapshot) {
    let (min, max) = shape_bounds(body.shape, body.position);
    body.bounds_min = min;
    body.bounds_max = max;
}

fn command_u64(value: &Value, key: &str, index: usize) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("script command[{index}] requires unsigned integer '{key}'"))
}

fn optional_number(value: &Value, key: &str, default: f32) -> Result<f32, String> {
    match value.get(key) {
        Some(value) => value
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("physics field '{key}' must be finite numeric")),
        None => Ok(default),
    }
}

fn optional_nullable_number(value: &Value, key: &str) -> Result<Option<f32>, String> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .map(Some)
            .ok_or_else(|| format!("physics field '{key}' must be finite numeric or null")),
    }
}

fn optional_bool(value: &Value, key: &str, default: bool) -> Result<bool, String> {
    match value.get(key) {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("physics field '{key}' must be boolean")),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use newviso_physics_client::{PhysicsContactEvent, PhysicsContactMaterialPair};

    #[test]
    fn streamed_mesh_colliders_are_sent_once_then_persist_backend_side() {
        // Lifecycle invariant: large static geometry is staged for one packet
        // after install/change, then omitted from steady-state frame packets.
        let mut runtime = PhysicsRuntime {
            client: PhysicsClient::new(),
            bodies: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            last_output: PhysicsFrameOutput::default(),
        };
        let collision = CollisionMeshResource {
            id: newviso_resource_runtime::AssetId(1),
            name: "ground".to_owned(),
            bounds: newviso_collision::CollisionBounds {
                min: [-1.0, 0.0, -1.0],
                max: [1.0, 0.0, 1.0],
            },
            vertices: vec![
                [-1.0, 0.0, -1.0],
                [1.0, 0.0, -1.0],
                [1.0, 0.0, 1.0],
                [-1.0, 0.0, 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 3, 2]],
            material_indices: vec![0, 0],
        };
        runtime
            .install_streamed_collision(
                42,
                &collision,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [1.0, 1.0, 1.0],
            )
            .expect("install");
        assert_eq!(runtime.streamed_colliders.len(), 1);
        assert_eq!(runtime.pending_streamed_colliders.len(), 1);

        // Reinstalling byte-identical placement does not restage geometry.
        let changed = runtime
            .install_streamed_collision(
                42,
                &collision,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [1.0, 1.0, 1.0],
            )
            .expect("reinstall");
        assert!(!changed);
        assert_eq!(runtime.pending_streamed_colliders.len(), 1);

        assert!(runtime.remove_streamed_collision(42));
        assert!(runtime.pending_streamed_colliders.is_empty());
        assert!(runtime.pending_commands.iter().any(|command| {
            matches!(command.kind, PhysicsCommandKind::DestroyBody { entity: 42 })
        }));
    }

    #[test]
    fn contact_surface_is_resolved_from_authored_triangle_material() {
        let mut runtime = PhysicsRuntime {
            client: PhysicsClient::new(),
            bodies: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            last_output: PhysicsFrameOutput::default(),
        };
        let collision = CollisionMeshResource {
            id: newviso_resource_runtime::AssetId(77),
            name: "surface_test".to_owned(),
            bounds: newviso_collision::CollisionBounds {
                min: [-2.0, 0.0, -1.0],
                max: [2.0, 0.0, 1.0],
            },
            vertices: vec![
                [-2.0, 0.0, -1.0],
                [0.0, 0.0, -1.0],
                [-2.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [2.0, 0.0, -1.0],
                [2.0, 0.0, 1.0],
            ],
            triangles: vec![[0, 1, 2], [1, 3, 2], [1, 4, 3], [4, 5, 3]],
            material_indices: vec![1, 1, 55, 55],
        };
        runtime
            .install_streamed_collision(42, &collision, [10.0, 0.0, 0.0], [0.0; 3], [1.0; 3])
            .expect("install surface collider");

        let mut output = PhysicsFrameOutput::default();
        output
            .events
            .push(PhysicsEvent::ContactBegin(PhysicsContactEvent {
                a: 7,
                b: 42,
                point: [11.25, 0.02, 0.0],
                normal: [0.0, -1.0, 0.0],
                impulse: 1.0,
                relative_velocity: None,
                materials: PhysicsContactMaterialPair::default(),
                surface_id: None,
                surface_entity: None,
                surface_triangle: None,
            }));
        runtime.enrich_contact_surfaces(&mut output);

        let PhysicsEvent::ContactBegin(contact) = output.events[0] else {
            panic!("expected contact begin");
        };
        assert_eq!(contact.surface_id, Some(55));
        assert_eq!(contact.surface_entity, Some(42));
        assert!(matches!(contact.surface_triangle, Some(2 | 3)));
    }

    #[test]
    fn camera_sphere_sweep_stops_before_streamed_mesh_and_can_ignore_owner() {
        let mut runtime = PhysicsRuntime {
            client: PhysicsClient::new(),
            bodies: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            last_output: PhysicsFrameOutput::default(),
        };
        let wall = CollisionMeshResource {
            id: newviso_resource_runtime::AssetId(2),
            name: "camera_wall".to_owned(),
            bounds: newviso_collision::CollisionBounds {
                min: [-2.0, -2.0, 0.0],
                max: [2.0, 2.0, 0.0],
            },
            vertices: vec![
                [-2.0, -2.0, 0.0],
                [2.0, -2.0, 0.0],
                [2.0, 2.0, 0.0],
                [-2.0, 2.0, 0.0],
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            material_indices: vec![0, 0],
        };
        runtime
            .install_streamed_collision(42, &wall, [0.0, 0.0, -2.0], [0.0; 3], [1.0; 3])
            .expect("install wall");

        let constrained =
            runtime.constrain_camera([0.0, 0.0, 0.0], [0.0, 0.0, -4.0], 0.25, None, &[]);
        assert!((constrained[2] + 1.74).abs() < 1.0e-4, "{constrained:?}");

        let ignored =
            runtime.constrain_camera([0.0, 0.0, 0.0], [0.0, 0.0, -4.0], 0.25, Some(42), &[]);
        assert_eq!(ignored, [0.0, 0.0, -4.0]);
    }

    #[test]
    fn scene_collider_interests_are_disabled_or_swept_around_dynamic_bodies() {
        let mut runtime = PhysicsRuntime {
            client: PhysicsClient::new(),
            bodies: BTreeMap::new(),
            streamed_colliders: BTreeMap::new(),
            camera_collision_meshes: BTreeMap::new(),
            streamed_surface_ids: BTreeMap::new(),
            pending_streamed_colliders: BTreeMap::new(),
            pending_commands: Vec::new(),
            damage_sources: BTreeMap::new(),
            scene_pose_offsets: BTreeMap::new(),
            scene_rotation_offsets: BTreeMap::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            settings: PhysicsWorldSettings::default(),
            last_output: PhysicsFrameOutput::default(),
        };
        assert!(runtime.scene_collider_interests().is_none());

        runtime.settings.scene_colliders_enabled = true;
        runtime.bodies.insert(
            99,
            PhysicsBodySnapshot {
                entity: 99,
                kind: PhysicsBodyKind::Dynamic,
                shape: CollisionShape::Box {
                    half_extents: [1.0, 1.0, 1.0],
                },
                flags: PhysicsBodyFlags {
                    is_trigger: false,
                    participates_in_queries: true,
                    casts_contacts: true,
                    continuous_collision: false,
                },
                material: PhysicsMaterial {
                    friction: 0.5,
                    restitution: 0.0,
                    density: 1.0,
                },
                position: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                linear_velocity: [12.0, 0.0, 0.0],
                angular_velocity: [0.0; 3],
                linear_damping: None,
                angular_damping: None,
                bounds_min: [-1.0, -1.0, -1.0],
                bounds_max: [1.0, 1.0, 1.0],
            },
        );

        let interests = runtime.scene_collider_interests().unwrap();
        assert_eq!(interests.len(), 1);
        assert!(interests[0].0[0] <= -33.0);
        assert!(interests[0].1[0] >= 34.5);
        assert!(interests[0].0[1] <= -33.0);
        assert!(interests[0].1[1] >= 33.0);
    }

    #[test]
    fn static_scene_body_uses_reserved_id_and_box_bounds() {
        let mut settings = PhysicsWorldSettings::default();
        settings.scene_colliders_enabled = true;
        let bodies = static_scene_bodies(&[(77, [-2.0, 0.0, -3.0], [2.0, 1.0, 3.0])], settings);
        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0].entity, 77);
        assert_eq!(bodies[0].position, [0.0, 0.5, 0.0]);
        assert_eq!(
            bodies[0].shape,
            CollisionShape::Box {
                half_extents: [2.0, 0.5, 3.0]
            }
        );
    }

    #[test]
    fn physics_contacts_carry_projectile_damage_toward_scene_target() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        runtime.damage_sources.insert(1_000_001, 37.5);
        runtime
            .last_output
            .events
            .push(PhysicsEvent::ContactBegin(PhysicsContactEvent {
                a: 1_000_001,
                b: 77,
                point: [2.0, 1.0, -3.0],
                normal: [0.0, 1.0, 0.0],
                impulse: 14.0,
                relative_velocity: Some([0.0, -100.0, 0.0]),
                materials: PhysicsContactMaterialPair::default(),
                surface_id: None,
                surface_entity: None,
                surface_triangle: None,
            }));

        let contacts = runtime.damage_contacts();
        assert_eq!(contacts.len(), 2);
        assert_eq!(contacts[0].source, 1_000_001);
        assert_eq!(contacts[0].target, 77);
        assert!((contacts[0].direct_damage - 37.5).abs() < 1.0e-6);
        assert!((contacts[0].contact_impulse - 14.0).abs() < 1.0e-6);
        assert_eq!(contacts[1].source, 77);
        assert_eq!(contacts[1].target, 1_000_001);
        assert_eq!(contacts[1].direct_damage, 0.0);
    }

    #[test]
    fn broken_scene_prop_promotes_to_dynamic_body_without_visual_pose_snap() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        let activation = SceneDestructionActivation {
            entity: 88,
            scene_position: [0.0, 0.0, 0.0],
            rotation_degrees: [0.0, 45.0, 0.0],
            bounds_min: [-0.25, 0.0, -0.25],
            bounds_max: [0.25, 6.0, 0.25],
            density: 40.0,
            friction: 0.65,
            restitution: 0.04,
            linear_damping: 0.08,
            angular_damping: 0.16,
            impulse_transfer: 0.9,
        };
        runtime
            .promote_scene_destructible(
                activation,
                PhysicsDamageContact {
                    source: 1_000_002,
                    target: 88,
                    direct_damage: 55.0,
                    contact_impulse: 20.0,
                    point: [0.0, 1.0, 0.0],
                    impulse_direction: [1.0, 0.0, 0.0],
                },
            )
            .unwrap();

        let body = runtime.bodies.get(&88).expect("promoted body");
        assert_eq!(body.kind, PhysicsBodyKind::Dynamic);
        assert_eq!(body.position, [0.0, 3.0, 0.0]);
        assert_eq!(body.rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(runtime.scene_pose_offsets.get(&88), Some(&[0.0, -3.0, 0.0]));
        assert!(runtime.scene_rotation_offsets.contains_key(&88));
        assert!(runtime.pending_commands.iter().any(|command| {
            matches!(
                command.kind,
                PhysicsCommandKind::ApplyImpulse { entity: 88, .. }
            )
        }));
    }

    #[test]
    fn runtime_state_predicts_presentation_pose_across_fixed_step_remainder() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        runtime.settings.fixed_hz = 120.0;
        runtime.accumulator = runtime.settings.fixed_dt() * 0.5;
        runtime.bodies.insert(
            7,
            PhysicsBodySnapshot {
                entity: 7,
                kind: PhysicsBodyKind::Dynamic,
                shape: CollisionShape::Capsule {
                    radius: 0.3,
                    half_height: 0.6,
                },
                flags: PhysicsBodyFlags {
                    is_trigger: false,
                    participates_in_queries: true,
                    casts_contacts: true,
                    continuous_collision: true,
                },
                material: PhysicsMaterial {
                    friction: 0.5,
                    restitution: 0.0,
                    density: 1.0,
                },
                position: [1.0, 2.0, 3.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                linear_velocity: [12.0, -3.0, 6.0],
                angular_velocity: [0.0; 3],
                linear_damping: None,
                angular_damping: None,
                bounds_min: [0.7, 1.1, 2.7],
                bounds_max: [1.3, 2.9, 3.3],
            },
        );

        let state = runtime.runtime_state();
        let body = &state["bodies"][0];
        let presentation = body["presentation_position"]
            .as_array()
            .expect("presentation position array");
        let actual = body["position"]
            .as_array()
            .expect("authoritative position array");

        let value = |array: &Vec<Value>, index: usize| {
            array[index].as_f64().expect("finite numeric position")
        };
        assert!((value(actual, 0) - 1.0).abs() < 1.0e-6);
        assert!((value(presentation, 0) - 1.05).abs() < 1.0e-5);
        assert!((value(presentation, 1) - 1.9875).abs() < 1.0e-5);
        assert!((value(presentation, 2) - 3.025).abs() < 1.0e-5);
        assert!(
            (state["presentation_alpha"]
                .as_f64()
                .expect("presentation alpha")
                - 0.5)
                .abs()
                < 1.0e-6
        );
    }

    #[test]
    fn sphere_bounds_follow_physics_pose() {
        let (min, max) = shape_bounds(CollisionShape::Sphere { radius: 0.25 }, [1.0, 2.0, 3.0]);
        assert_eq!(min, [0.75, 1.75, 2.75]);
        assert_eq!(max, [1.25, 2.25, 3.25]);
    }
}
