use super::asset_models::AssetAlphaMode;
use super::*;
use newviso_resource_runtime::AssetId;
use std::sync::Arc;

/// Copy a skeletal branch from bind geometry. Authored scripts own its pose and
/// visibility; this API contains no player, camera, or character naming policy.
impl Scene3dRuntime {
    pub fn copy_entity_joint_mesh(
        &mut self,
        source: u64,
        target: u64,
        roots: &[String],
        root_joint: &str,
        tip_joint: &str,
        direction: [f32; 3],
        up: [f32; 3],
    ) -> Result<bool, String> {
        if source == target
            || roots.is_empty()
            || roots.len() > 128
            || roots
                .iter()
                .any(|name| name.trim().is_empty() || name.len() > 256)
        {
            return Err("joint mesh copy requires a distinct target and valid joint roots".into());
        }
        let direction = Vec3::new(direction[0], direction[1], direction[2]);
        let up = Vec3::new(up[0], up[1], up[2]);
        if ![direction.x, direction.y, direction.z, up.x, up.y, up.z]
            .iter()
            .all(|x| x.is_finite())
            || direction.dot(direction) < 1.0e-8
            || up.cross(direction).dot(up.cross(direction)) < 1.0e-8
        {
            return Err("joint mesh direction and up must form a finite basis".into());
        }
        let Some(source_mesh) = self.asset_meshes.get(&source).cloned() else {
            return Ok(false); // Retry after asynchronous model materialization.
        };
        if self.asset_meshes.get(&target).is_some_and(|mesh| {
            mesh.main_view_only && mesh.source_model_id == source_mesh.source_model_id
        }) {
            return Ok(true);
        }
        if !self.entity_skeletons.contains_key(&source) {
            return Ok(false);
        }
        let root_matrix = self.bind_joint_model_matrix(source, root_joint)?;
        let tip_matrix = self.bind_joint_model_matrix(source, tip_joint)?;
        let pivot = Vec3::new(root_matrix[12], root_matrix[13], root_matrix[14]);
        let tip = Vec3::new(tip_matrix[12], tip_matrix[13], tip_matrix[14]);
        let axis = tip.sub(pivot);
        if axis.dot(axis) < 1.0e-8 {
            return Err("joint mesh root and tip are coincident".into());
        }
        let from = view_basis(axis, Vec3::Y);
        let to = view_basis(direction, up);
        let rotate = |v: Vec3| {
            to.0.mul(v.dot(from.0))
                .add(to.1.mul(v.dot(from.1)))
                .add(to.2.mul(v.dot(from.2)))
        };
        let bind = self
            .asset_model_cache
            .get(&source_mesh.source_model_id.0)
            .ok_or_else(|| "joint mesh source bind geometry is missing".to_owned())?;
        let mut vertices = Vec::new();
        let mut ranges = Vec::new();
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for range in source_mesh.local_draw_ranges.iter().filter(|range| {
            roots.iter().any(|root| {
                range
                    .joint_lineage
                    .iter()
                    .any(|joint| joint.eq_ignore_ascii_case(root))
            })
        }) {
            let start = range.first_vertex as usize;
            let end = start
                .checked_add(range.vertex_count as usize)
                .ok_or_else(|| "joint mesh source range overflow".to_owned())?;
            let selected = bind
                .get(start..end)
                .ok_or_else(|| "joint mesh source range is invalid".to_owned())?;
            let first = u32::try_from(vertices.len())
                .map_err(|_| "joint mesh vertex offset exceeds u32".to_owned())?;
            for source_vertex in selected {
                let mut vertex = *source_vertex;
                vertex.position = rotate(vertex.position.sub(pivot));
                vertex.normal = rotate(vertex.normal).normalized();
                let tangent = rotate(Vec3::new(
                    vertex.tangent[0],
                    vertex.tangent[1],
                    vertex.tangent[2],
                ))
                .normalized();
                vertex.tangent = [tangent.x, tangent.y, tangent.z, vertex.tangent[3]];
                vertex.skin_influences = 0;
                let position = [vertex.position.x, vertex.position.y, vertex.position.z];
                for axis in 0..3 {
                    min[axis] = min[axis].min(position[axis]);
                    max[axis] = max[axis].max(position[axis]);
                }
                vertices.push(vertex);
            }
            ranges.push(AssetDrawRange {
                first_vertex: first,
                local_center: vertices[first as usize..]
                    .iter()
                    .fold(Vec3::ZERO, |sum, v| sum.add(v.position))
                    .mul(1.0 / selected.len().max(1) as f32),
                ..range.clone()
            });
        }
        if vertices.is_empty() {
            return Err("joint mesh selection contains no geometry".into());
        }
        let entity = self
            .world
            .entity(SceneEntityId(target))
            .ok_or_else(|| "joint mesh target does not exist".to_owned())?;
        let transform = entity.transform;
        let local_bounds = SceneBounds {
            min: Vec3::new(min[0], min[1], min[2]),
            max: Vec3::new(max[0], max[1], max[2]),
        };
        let first_vertex = u32::try_from(self.asset_vertex_data.len() / FLOATS_PER_VERTEX)
            .map_err(|_| "joint mesh GPU offset exceeds u32".to_owned())?;
        let vertex_count = u32::try_from(vertices.len())
            .map_err(|_| "joint mesh vertex count exceeds u32".to_owned())?;
        let upload_from = self.asset_vertex_data.len();
        asset_models::append_local_asset_vertices(&mut self.asset_vertex_data, &vertices);
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
                    source_mesh.source_model_id.0 ^ target.rotate_left(23) ^ 0x4a4f_494e_544d_4553,
                ),
                local_bounds,
                local_draw_ranges: Arc::from(ranges),
                opaque_draw_range_indices: Arc::from(opaque),
                alpha_draw_range_indices: Arc::from(alpha),
                materials: source_mesh.materials,
                first_vertex,
                vertex_count,
                skinned_first_vertex: None,
                fragment_deformable: false,
                main_view_only: true,
                last_fragment_poses: Default::default(),
            },
        );
        self.world
            .entity_mut(SceneEntityId(target))
            .unwrap()
            .resident_geometry = true;
        self.world.update_spatial_from(
            SceneEntityId(target),
            transform,
            asset_models::transformed_local_bounds(local_bounds, transform)?,
            SceneMutationSource::Streaming,
        )?;
        Ok(true)
    }
}
