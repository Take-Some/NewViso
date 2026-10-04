use super::*;

impl PhysicsRuntime {
    pub(crate) fn scene_collider_interests(&self) -> Option<Vec<([f32; 3], [f32; 3])>> {
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

    pub(super) fn resolve_surface_id(&self, entity: u64, world_point: [f32; 3]) -> Option<u32> {
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

    pub(super) fn enrich_contact_surfaces(&self, output: &mut PhysicsFrameOutput) {
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

    pub(crate) fn prepare_streamed_collision(
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

    pub(crate) fn install_prepared_streamed_collision(
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
    pub(crate) fn install_streamed_collision(
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

    pub(crate) fn remove_streamed_collision(&mut self, entity: u64) -> bool {
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

    #[cfg(test)]
    pub(crate) fn navigation_tile_source(
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
}
