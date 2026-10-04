use super::*;

impl Scene3dRuntime {
    pub(super) fn build_alpha_submission(
        &self,
        gpu: GpuScene,
        frame_slot: usize,
        visible_instance_slots: &BTreeMap<u64, u32>,
    ) -> Result<Vec<AlphaAssetDraw>, String> {
        let mut alpha_draws = Vec::<AlphaAssetDraw>::new();
        for id in &self.frame_plan.visible_entities {
            let Some(instance_index) = visible_instance_slots.get(&id.0).copied() else {
                continue;
            };
            let Some(mesh) = self.asset_meshes.get(&id.0) else {
                continue;
            };
            let Some(transform) = self.world.entity(*id).map(|entity| entity.transform) else {
                continue;
            };

            for range_index in mesh.alpha_draw_range_indices.iter().copied() {
                let range = &mesh.local_draw_ranges[range_index as usize];
                if !self.main_view_mesh_visibility.visible(
                    id.0,
                    &range.mesh_name,
                    &range.joint_lineage,
                ) {
                    continue;
                }
                let material_group = match range.material_slot {
                    Some(slot) => {
                        let Some(material) = self.asset_gpu_materials.get(&(mesh.model_id.0, slot))
                        else {
                            continue;
                        };
                        self.dashboard_material_group(id.0, slot, frame_slot)
                            .unwrap_or(material.bind_group)
                    }
                    None => gpu.default_material_bind_group,
                };
                let (vertex_buffer, vertex_offset) =
                    asset_mesh_vertex_binding(mesh, gpu, frame_slot)?;
                // Transparent parts of one vehicle need their own depth order.
                // Sorting every range by the chassis origin lets the dashboard
                // overwrite the steering wheel as the view changes.
                let mut center = range.local_center;
                if let Some(pose) = mesh
                    .last_fragment_poses
                    .values()
                    .filter(|pose| {
                        pose.mesh_names
                            .iter()
                            .any(|name| name.as_str() == range.mesh_name.as_ref())
                    })
                    .min_by_key(|pose| pose.mesh_names.len())
                {
                    let pivot = Vec3::new(pose.pivot[0], pose.pivot[1], pose.pivot[2]);
                    let moved = transform_point(
                        center.sub(pivot),
                        Vec3::new(pose.scale[0], pose.scale[1], pose.scale[2]),
                        Vec3::new(
                            pose.rotation_degrees[0],
                            pose.rotation_degrees[1],
                            pose.rotation_degrees[2],
                        ),
                        Vec3::ZERO,
                    );
                    center = pivot
                        .add(Vec3::new(
                            pose.translation[0],
                            pose.translation[1],
                            pose.translation[2],
                        ))
                        .add(Vec3::new(
                            moved.x * pose.post_rotation_scale[0],
                            moved.y * pose.post_rotation_scale[1],
                            moved.z * pose.post_rotation_scale[2],
                        ));
                }
                let center = transform_point(
                    center,
                    transform.scale,
                    transform.rotation_degrees,
                    transform.position,
                );
                let delta = center.sub(self.camera.position);
                let distance_sq = delta.dot(delta);
                alpha_draws.push(AlphaAssetDraw {
                    distance_sq,
                    material_group,
                    vertex_buffer,
                    vertex_offset,
                    first_index: mesh.first_vertex.saturating_add(range.first_vertex),
                    index_count: range.vertex_count,
                    instance_index,
                });
            }
        }
        Ok(alpha_draws)
    }
}
