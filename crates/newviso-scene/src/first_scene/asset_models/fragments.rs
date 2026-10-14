//! Fragment state queries, detached installation and part deformation.
use super::*;

impl Scene3dRuntime {
    pub fn entity_fragment_mesh_bounds(
        &self,
        entity: u64,
        names: &[String],
    ) -> Option<([f32; 3], [f32; 3])> {
        let mesh = self.asset_meshes.get(&entity)?;
        if let Some(bounds) = mesh.fragment_mesh_bounds.borrow().get(names) {
            return *bounds;
        }
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut found = false;
        for range in mesh.local_draw_ranges.iter().filter(|range| {
            names
                .iter()
                .any(|name| name.as_str() == range.mesh_name.as_ref())
        }) {
            let start =
                (mesh.first_vertex as usize + range.first_vertex as usize) * FLOATS_PER_VERTEX;
            let end = start + range.vertex_count as usize * FLOATS_PER_VERTEX;
            for vertex in self
                .asset_vertex_data
                .get(start..end)?
                .chunks_exact(FLOATS_PER_VERTEX)
            {
                for axis in 0..3 {
                    min[axis] = min[axis].min(vertex[axis]);
                    max[axis] = max[axis].max(vertex[axis]);
                }
                found = true;
            }
        }
        let bounds = found.then_some((min, max));
        mesh.fragment_mesh_bounds.borrow_mut().insert(names.to_vec(), bounds);
        bounds
    }

    /// Nearest non-degenerate live fragment triangle: squared distance, point and UV.
    pub fn entity_fragment_surface_hit(
        &self,
        entity: u64,
        names: &[String],
        point: [f32; 3],
    ) -> Option<(f32, [f32; 3], [f32; 2])> {
        let mesh = self.asset_meshes.get(&entity)?;
        let mut nearest = None;
        for range in mesh
            .local_draw_ranges
            .iter()
            .filter(|r| names.iter().any(|n| n.as_str() == r.mesh_name.as_ref()))
        {
            let start =
                (mesh.first_vertex as usize + range.first_vertex as usize) * FLOATS_PER_VERTEX;
            let end = start + range.vertex_count as usize * FLOATS_PER_VERTEX;
            for triangle in self
                .asset_vertex_data
                .get(start..end)?
                .chunks_exact(3 * FLOATS_PER_VERTEX)
            {
                let vertices: [[f32; 3]; 3] = std::array::from_fn(|i| {
                    std::array::from_fn(|a| triangle[i * FLOATS_PER_VERTEX + a])
                });
                let Some((hit, weights)) = closest_fragment_triangle(point, vertices) else {
                    continue;
                };
                let distance = (0..3).map(|a| (point[a] - hit[a]).powi(2)).sum::<f32>();
                if nearest.as_ref().is_none_or(|(best, _, _)| distance < *best) {
                    let uv = std::array::from_fn(|a| {
                        (0..3)
                            .map(|i| weights[i] * triangle[i * FLOATS_PER_VERTEX + 11 + a])
                            .sum()
                    });
                    nearest = Some((distance, hit, uv));
                }
            }
        }
        nearest
    }

    pub fn entity_fragment_part_pose(
        &self,
        entity: u64,
        names: &[String],
    ) -> Option<SceneModelPartPose> {
        self.asset_meshes
            .get(&entity)?
            .last_fragment_poses
            .values()
            .find(|p| p.visible && p.mesh_names.iter().any(|n| names.contains(n)))
            .cloned()
    }

    /// Apply exactly the live rigid pose used for render vertices to authored
    /// collision/CG points, including parent articulation and deformation.
    pub fn entity_fragment_physics_point(
        &self,
        entity: u64,
        names: &[String],
        point: [f32; 3],
    ) -> [f32; 3] {
        let Some(pose) = self.entity_fragment_part_pose(entity, names) else {
            return point;
        };
        let pivot = Vec3::new(pose.pivot[0], pose.pivot[1], pose.pivot[2]);
        let center = pivot.add(Vec3::new(
            pose.translation[0],
            pose.translation[1],
            pose.translation[2],
        ));
        let q = transform_point(
            Vec3::new(point[0], point[1], point[2]).sub(pivot),
            Vec3::new(pose.scale[0], pose.scale[1], pose.scale[2]),
            Vec3::new(
                pose.rotation_degrees[0],
                pose.rotation_degrees[1],
                pose.rotation_degrees[2],
            ),
            center,
        );
        let d = q.sub(center);
        let mut q = center.add(Vec3::new(
            d.x * pose.post_rotation_scale[0],
            d.y * pose.post_rotation_scale[1],
            d.z * pose.post_rotation_scale[2],
        ));
        if !pose.dents.is_empty() {
            q = apply_fragment_dents(q, Vec3::Y, Vec3::new(1.0, 0.0, 0.0), &pose.dents).0;
        }
        [q.x, q.y, q.z]
    }

    /// Copy only the selected live fragment geometry into a separate dynamic
    /// entity. Materials and textures are shared; vertices and bounds are local
    /// to the detached body, independent of the source vehicle's visibility.
    pub fn install_detached_fragment(
        &mut self,
        source: u64,
        target: u64,
        mesh_names: &[String],
    ) -> Result<([f32; 3], [f32; 3], Vec<[f32; 3]>), String> {
        let source_mesh = self
            .asset_meshes
            .get(&source)
            .cloned()
            .ok_or_else(|| format!("fragment source {source} has no model"))?;
        if self.world.entity(SceneEntityId(target)).is_none() {
            return Err(format!("fragment target {target} does not exist"));
        }
        let mut data = Vec::new();
        let mut ranges = Vec::new();
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for range in source_mesh.local_draw_ranges.iter().filter(|range| {
            mesh_names
                .iter()
                .any(|name| name.as_str() == range.mesh_name.as_ref())
        }) {
            let first = data.len() / FLOATS_PER_VERTEX;
            let start = (source_mesh.first_vertex as usize + range.first_vertex as usize)
                * FLOATS_PER_VERTEX;
            let end = start + range.vertex_count as usize * FLOATS_PER_VERTEX;
            let mut vertices = self
                .asset_vertex_data
                .get(start..end)
                .ok_or_else(|| "fragment source vertex range is invalid".to_owned())?
                .to_vec();
            if source_mesh.fragment_deformable {
                let bind = self
                    .asset_model_cache
                    .get(&source_mesh.source_model_id.0)
                    .ok_or("fragment bind geometry is missing")?;
                let local_first = range.first_vertex as usize;
                let local_end = local_first
                    .checked_add(range.vertex_count as usize)
                    .ok_or("fragment bind range overflow")?;
                let bind = bind
                    .get(local_first..local_end)
                    .ok_or("fragment bind range is invalid")?;
                let pose = source_mesh
                    .last_fragment_poses
                    .iter()
                    .find(|(names, _)| names.iter().any(|n| n.as_str() == range.mesh_name.as_ref()))
                    .map(|(_, pose)| pose.clone())
                    .unwrap_or_default();
                for (vertex, &bind) in vertices.chunks_exact_mut(FLOATS_PER_VERTEX).zip(bind) {
                    let (position, normal, tangent) = posed_fragment_vertex(bind, &pose, true);
                    vertex[0..3].copy_from_slice(&[position.x, position.y, position.z]);
                    vertex[4..7].copy_from_slice(&[normal.x, normal.y, normal.z]);
                    vertex[13..16].copy_from_slice(&[tangent.x, tangent.y, tangent.z]);
                }
            }
            for vertex in vertices.chunks_exact(FLOATS_PER_VERTEX) {
                for axis in 0..3 {
                    min[axis] = min[axis].min(vertex[axis]);
                    max[axis] = max[axis].max(vertex[axis]);
                }
            }
            data.extend_from_slice(&vertices);
            ranges.push(AssetDrawRange {
                first_vertex: u32::try_from(first)
                    .map_err(|_| "fragment vertex offset overflow".to_owned())?,
                local_center: {
                    let mut sum = Vec3::ZERO;
                    for vertex in vertices.chunks_exact(FLOATS_PER_VERTEX) {
                        sum = sum.add(Vec3::new(vertex[0], vertex[1], vertex[2]));
                    }
                    sum.mul(1.0 / (vertices.len() / FLOATS_PER_VERTEX).max(1) as f32)
                },
                ..range.clone()
            });
        }
        if data.is_empty() {
            return Err("fragment has no separately bound render geometry".to_owned());
        }
        let center: [f32; 3] = std::array::from_fn(|axis| (min[axis] + max[axis]) * 0.5);
        let extent: [f32; 3] =
            std::array::from_fn(|axis| ((max[axis] - min[axis]) * 0.5).max(0.025));
        for range in &mut ranges {
            range.local_center = range
                .local_center
                .sub(Vec3::new(center[0], center[1], center[2]));
        }
        let mut hull = Vec::new();
        let mut hull_keys = std::collections::BTreeSet::new();
        for vertex in data.chunks_exact_mut(FLOATS_PER_VERTEX) {
            for axis in 0..3 {
                vertex[axis] -= center[axis];
            }
            let point = [vertex[0], vertex[1], vertex[2]];
            if hull_keys.insert(point.map(f32::to_bits)) {
                hull.push(point);
            }
        }
        let first_vertex = u32::try_from(self.asset_vertex_data.len() / FLOATS_PER_VERTEX)
            .map_err(|_| "fragment GPU vertex offset overflow".to_owned())?;
        let vertex_count = u32::try_from(data.len() / FLOATS_PER_VERTEX)
            .map_err(|_| "fragment vertex count overflow".to_owned())?;
        let upload_from = self.asset_vertex_data.len();
        self.asset_vertex_data.extend_from_slice(&data);
        self.asset_upload_from_float = Some(
            self.asset_upload_from_float
                .map_or(upload_from, |old| old.min(upload_from)),
        );
        let mut opaque = Vec::new();
        let mut alpha = Vec::new();
        for (index, range) in ranges.iter().enumerate() {
            let mode = range
                .material_slot
                .and_then(|slot| source_mesh.materials.get(slot as usize))
                .map(|material| material.alpha_mode)
                .unwrap_or(AssetAlphaMode::Opaque);
            if mode == AssetAlphaMode::Blend {
                alpha.push(index as u32);
            } else {
                opaque.push(index as u32);
            }
        }
        self.asset_meshes.insert(
            target,
            CpuAssetMesh {
                source_model_id: source_mesh.source_model_id,
                model_id: AssetId(
                    source_mesh.source_model_id.0 ^ target.rotate_left(23) ^ 0x4652_4147_4d45_4e54,
                ),
                local_bounds: SceneBounds {
                    min: Vec3::new(-extent[0], -extent[1], -extent[2]),
                    max: Vec3::new(extent[0], extent[1], extent[2]),
                },
                local_draw_ranges: Arc::from(ranges),
                opaque_draw_range_indices: Arc::from(opaque),
                alpha_draw_range_indices: Arc::from(alpha),
                materials: source_mesh.materials,
                first_vertex,
                vertex_count,
                skinned_first_vertex: None,
                fragment_deformable: false,
                main_view_only: false,
                last_fragment_poses: std::collections::BTreeMap::new(),
                fragment_mesh_bounds: Default::default(),
            },
        );
        let id = SceneEntityId(target);
        // Generated fragments have no streamable asset_ref or cube render slot.
        // Publish their resident mesh to SceneWorld so visibility submission
        // includes the body after its source triangles have been hidden.
        self.world.entity_mut(id).unwrap().resident_geometry = true;
        if let Some(entity) = self.world.entity(id) {
            let transform = entity.transform;
            let bounds = transformed_local_bounds(
                SceneBounds {
                    min: Vec3::new(-extent[0], -extent[1], -extent[2]),
                    max: Vec3::new(extent[0], extent[1], extent[2]),
                },
                transform,
            )?;
            self.world.update_spatial_from(
                id,
                transform,
                bounds,
                SceneMutationSource::Streaming,
            )?;
        }
        Ok((center, extent, hull))
    }

    pub fn set_entity_model_part_poses(
        &mut self,
        stable_id: u64,
        poses: &[SceneModelPartPose],
    ) -> Result<usize, String> {
        if poses.is_empty() {
            return Ok(0);
        }
        let mesh = self
            .asset_meshes
            .get(&stable_id)
            .ok_or_else(|| format!("scene entity {stable_id} has no installed model"))?;
        if !mesh.fragment_deformable {
            return Err(format!(
                "scene entity {stable_id} model is not fragment-deformable"
            ));
        }
        if mesh.skinned_first_vertex.is_some() {
            return Err(format!(
                "scene entity {stable_id} cannot use fragment CPU poses while skinning is active"
            ));
        }
        let previous_poses = mesh.last_fragment_poses.clone();
        let source_model_id = mesh.source_model_id.0;
        let first_vertex = mesh.first_vertex as usize;
        let local_draw_ranges = mesh.local_draw_ranges.clone();
        let bind_vertices = self
            .asset_model_cache
            .get(&source_model_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "scene entity {stable_id} fragment bind geometry model={source_model_id} is missing"
                )
            })?;

        let mut modified_vertices = 0usize;
        let mut first_modified_float = None::<usize>;
        for pose in poses {
            if previous_poses
                .get(&pose.mesh_names)
                .is_some_and(|previous| previous == pose)
            {
                continue;
            }
            if pose
                .glass_damage
                .is_some_and(|v| v.iter().any(|x| !x.is_finite()) || !(0.0..=1.0).contains(&v[2]))
            {
                return Err("invalid glass damage appearance".to_owned());
            }
            if pose
                .pivot
                .iter()
                .chain(pose.translation.iter())
                .chain(pose.rotation_degrees.iter())
                .chain(pose.scale.iter())
                .chain(pose.post_rotation_scale.iter())
                .any(|value| !value.is_finite())
                || pose.post_rotation_scale.iter().any(|value| *value <= 0.0)
            {
                return Err(format!(
                    "scene entity {stable_id} fragment pose contains non-finite values"
                ));
            }
            for range in local_draw_ranges.iter().filter(|range| {
                pose.mesh_names
                    .iter()
                    .any(|name| name.as_str() == range.mesh_name.as_ref())
            }) {
                let local_first = range.first_vertex as usize;
                let local_end = local_first
                    .checked_add(range.vertex_count as usize)
                    .ok_or_else(|| "fragment vertex range overflow".to_owned())?;
                if local_end > bind_vertices.len() {
                    return Err(format!(
                        "scene entity {stable_id} fragment range {}..{} exceeds bind vertices={}",
                        local_first,
                        local_end,
                        bind_vertices.len()
                    ));
                }
                for local_index in local_first..local_end {
                    let bind = bind_vertices[local_index];
                    let (position, normal, tangent) = posed_fragment_vertex(bind, pose, false);
                    let global_vertex = first_vertex
                        .checked_add(local_index)
                        .ok_or_else(|| "fragment global vertex overflow".to_owned())?;
                    let base = global_vertex
                        .checked_mul(FLOATS_PER_VERTEX)
                        .ok_or_else(|| "fragment vertex float offset overflow".to_owned())?;
                    let end = base + FLOATS_PER_VERTEX;
                    if end > self.asset_vertex_data.len() {
                        return Err(format!(
                            "scene entity {stable_id} fragment write {}..{} exceeds vertex floats={}",
                            base,
                            end,
                            self.asset_vertex_data.len()
                        ));
                    }
                    if first_modified_float.is_none() {
                        // Clear before the first write, including partial/error paths.
                        self.asset_meshes.get(&stable_id).unwrap()
                            .fragment_mesh_bounds.borrow_mut().clear();
                    }
                    let dst = &mut self.asset_vertex_data[base..end];
                    dst[0] = position.x;
                    dst[1] = position.y;
                    dst[2] = position.z;
                    dst[4] = normal.x;
                    dst[5] = normal.y;
                    dst[6] = normal.z;
                    dst[7..11].copy_from_slice(&bind.color);
                    if let Some([u, v, damage]) = pose.glass_damage.filter(|v| v[2] > 0.0) {
                        dst[7..11].copy_from_slice(&[u, v, damage, -1.0]);
                    }
                    dst[13] = tangent.x;
                    dst[14] = tangent.y;
                    dst[15] = tangent.z;
                    first_modified_float =
                        Some(first_modified_float.map_or(base, |value| value.min(base)));
                    modified_vertices = modified_vertices.saturating_add(1);
                }
            }
        }
        if let Some(mesh) = self.asset_meshes.get_mut(&stable_id) {
            for pose in poses {
                mesh.last_fragment_poses
                    .insert(pose.mesh_names.clone(), pose.clone());
            }
        }
        if let Some(offset) = first_modified_float {
            self.asset_upload_from_float = Some(
                self.asset_upload_from_float
                    .map_or(offset, |existing| existing.min(offset)),
            );
        }
        Ok(modified_vertices)
    }
}
