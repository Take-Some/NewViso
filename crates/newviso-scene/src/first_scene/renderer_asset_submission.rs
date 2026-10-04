use super::*;

pub(super) struct FrameAssetSubmission {
    pub(super) instance_batches: Vec<GpuResidentInstanceBatch>,
    pub(super) visible_instance_slots: BTreeMap<u64, u32>,
    pub(super) hiz_draws: Vec<(u32, u64)>,
    pub(super) direct_opaque_fallback: Vec<DirectAssetDraw>,
}

impl Scene3dRuntime {
    pub(super) fn build_asset_submission(
        &self,
        render: &RenderClient,
        gpu: GpuScene,
        frame_slot: usize,
    ) -> Result<FrameAssetSubmission, String> {
        let mut instance_batches = Vec::<GpuResidentInstanceBatch>::new();
        let mut selected_static_batches = BTreeSet::<GpuInstanceBatchKey>::new();
        let mut dynamic_groups = BTreeMap::<GpuInstanceBatchKey, Vec<u64>>::new();
        let mut visible_instance_slots = BTreeMap::<u64, u32>::new();

        for id in &self.frame_plan.visible_entities {
            let Some(mesh) = self.asset_meshes.get(&id.0) else {
                continue;
            };
            let Some(entity) = self.world.entity(*id) else {
                continue;
            };
            if entity.mobility == SceneMobility::Static {
                if let Some(key) = self.gpu_instance_table.entity_batches.get(&id.0) {
                    selected_static_batches.insert(*key);
                }
                if let Some(slot) = self.gpu_instance_table.entity_slots.get(&id.0) {
                    visible_instance_slots.insert(id.0, *slot);
                }
            } else {
                let (cx, cy, cz) = instance_cell(entity.transform.position);
                dynamic_groups
                    .entry((mesh.model_id.0, cx, cy, cz, 1))
                    .or_default()
                    .push(id.0);
            }
        }

        for key in selected_static_batches {
            let Some(batch) = self.gpu_instance_table.batches.get(&key) else {
                continue;
            };

            // RSC7-style instanced submission: visibility may cut holes in a
            // resident spatial batch, but it must not automatically degrade the
            // entire batch to one draw per entity. Preserve each contiguous run
            // of resident instance slots as one instanced submission.
            for run in visible_batch_runs(batch, &visible_instance_slots) {
                let end = run.first_member + run.member_count;
                let stable_ids = batch.stable_ids[run.first_member..end].to_vec();
                let full_batch =
                    run.first_member == 0 && run.member_count == batch.stable_ids.len();
                let sphere = if full_batch {
                    batch.sphere
                } else {
                    instance_ids_sphere(&self.world, &stable_ids)
                        .ok_or_else(|| "visible static instance run lost scene entity".to_owned())?
                };
                instance_batches.push(GpuResidentInstanceBatch {
                    model_id: batch.model_id,
                    first_instance: batch.first_instance.saturating_add(run.first_member as u32),
                    instance_count: u32::try_from(run.member_count)
                        .map_err(|_| "visible static run count exceeds u32".to_owned())?,
                    stable_ids,
                    sphere,
                });
            }
        }

        let static_instance_count = self.gpu_instance_table.instance_data.len() / INSTANCE_FLOATS;
        let dynamic_instance_count = dynamic_groups.values().map(Vec::len).sum::<usize>();
        let total_instance_count = static_instance_count.saturating_add(dynamic_instance_count);
        if total_instance_count > gpu.asset_instance_capacity as usize {
            return Err(format!(
                "resident static + visible dynamic asset instances {} exceed GPU instance capacity {}",
                total_instance_count, gpu.asset_instance_capacity
            ));
        }

        let mut dynamic_instance_data =
            Vec::<f32>::with_capacity(dynamic_instance_count.saturating_mul(INSTANCE_FLOATS));
        for (key, stable_ids) in dynamic_groups {
            let first_instance = u32::try_from(
                static_instance_count + dynamic_instance_data.len() / INSTANCE_FLOATS,
            )
            .map_err(|_| "dynamic asset instance offset exceeds u32".to_owned())?;
            for stable_id in &stable_ids {
                let entity = self
                    .world
                    .entity(SceneEntityId(*stable_id))
                    .ok_or_else(|| format!("dynamic asset entity {} disappeared", stable_id))?;
                let slot = u32::try_from(
                    static_instance_count + dynamic_instance_data.len() / INSTANCE_FLOATS,
                )
                .map_err(|_| "dynamic asset instance index exceeds u32".to_owned())?;
                dynamic_instance_data.extend_from_slice(&geometry::instance_model_matrix(
                    entity.transform.position,
                    entity.transform.rotation_degrees,
                    entity.transform.scale,
                ));
                visible_instance_slots.insert(*stable_id, slot);
            }
            let count = u32::try_from(stable_ids.len())
                .map_err(|_| "dynamic asset instance count exceeds u32".to_owned())?;
            let sphere = instance_ids_sphere(&self.world, &stable_ids)
                .ok_or_else(|| "dynamic asset batch lost scene entities".to_owned())?;
            instance_batches.push(GpuResidentInstanceBatch {
                model_id: key.0,
                first_instance,
                instance_count: count,
                stable_ids,
                sphere,
            });
        }
        if !dynamic_instance_data.is_empty() {
            render.write_buffer_f32(
                gpu.asset_instance_buffers[frame_slot],
                static_instance_count as u64 * INSTANCE_STRIDE,
                &dynamic_instance_data,
            )?;
        }

        // Build one fail-open indirect command per opaque/cutout primitive
        // range and spatial instance batch. The Vulkan visibility provider may
        // zero instance_count using previous-frame Hi-Z; if it cannot, these
        // CPU-initialized commands remain fully drawable.
        let mut pending_hiz_draws = Vec::<PendingHizAssetDraw>::new();
        let mut direct_opaque_fallback = Vec::<DirectAssetDraw>::new();

        // Split only main-view submissions that contain an instance override.
        // Shared geometry and the full shadow batches remain unchanged.
        for (batch, member) in instance_batches.iter().flat_map(|batch| {
            let split = batch.instance_count > 1
                && batch
                    .stable_ids
                    .iter()
                    .any(|id| self.main_view_mesh_visibility.has_override(*id));
            let count = if split { batch.stable_ids.len() } else { 1 };
            (0..count).map(move |member| (batch, split.then_some(member)))
        }) {
            let Some(stable_id) = batch.stable_ids.get(member.unwrap_or(0)) else {
                continue;
            };
            let mesh = self
                .asset_meshes
                .get(stable_id)
                .ok_or_else(|| format!("asset instance {} disappeared", stable_id))?;
            debug_assert_eq!(mesh.model_id.0, batch.model_id);
            let sphere = batch.sphere;
            let instance_count = if member.is_some() {
                1
            } else {
                batch.instance_count
            };
            let first_instance = batch.first_instance + member.unwrap_or(0) as u32;

            let (vertex_buffer, vertex_offset) = asset_mesh_vertex_binding(mesh, gpu, frame_slot)?;
            let mut queue_range =
                |material_group: u32, first_index: u32, index_count: u32| -> Result<(), String> {
                    let candidate_index = pending_hiz_draws.len();
                    let static_hiz_compatible =
                        vertex_buffer == gpu.asset_vertex_buffer && vertex_offset == 0;
                    if static_hiz_compatible
                        && ENABLE_ASSET_HIZ_OCCLUSION
                        && candidate_index < MAX_HIZ_DRAW_CANDIDATES as usize
                    {
                        pending_hiz_draws.push(PendingHizAssetDraw {
                            material_group,
                            sphere,
                            index_count,
                            first_index,
                            instance_count,
                            first_instance,
                        });
                    } else {
                        direct_opaque_fallback.push(DirectAssetDraw {
                            material_group,
                            vertex_buffer,
                            vertex_offset,
                            index_count,
                            first_index,
                            instance_count,
                            first_instance,
                        });
                    }
                    Ok(())
                };

            if mesh.local_draw_ranges.is_empty() {
                queue_range(
                    gpu.default_material_bind_group,
                    mesh.first_vertex,
                    mesh.vertex_count,
                )?;
                continue;
            }

            for range_index in mesh.opaque_draw_range_indices.iter().copied() {
                let range = &mesh.local_draw_ranges[range_index as usize];
                if !self.main_view_mesh_visibility.visible(
                    *stable_id,
                    &range.mesh_name,
                    &range.joint_lineage,
                ) {
                    continue;
                }
                let material_group = match range.material_slot {
                    Some(slot) => {
                        let Some(material) = self.asset_gpu_materials.get(&(mesh.model_id.0, slot))
                        else {
                            // An authored material slot is not equivalent to
                            // "no material". Do not flash/render the model with
                            // the white fallback while its textures are pending.
                            continue;
                        };
                        self.dashboard_material_group(*stable_id, slot, frame_slot)
                            .unwrap_or(material.bind_group)
                    }
                    None => gpu.default_material_bind_group,
                };
                queue_range(
                    material_group,
                    mesh.first_vertex.saturating_add(range.first_vertex),
                    range.vertex_count,
                )?;
            }
        }

        // Opaque/cutout geometry is order-independent. Build a compact draw
        // list sorted by material/state before uploading it, like RSC7 draw
        // lists do, so one material can execute as one contiguous indirect
        // multi-draw instead of many alternating service calls.
        pending_hiz_draws.sort_unstable_by_key(|draw| {
            (
                draw.material_group,
                draw.first_index,
                draw.first_instance,
                draw.instance_count,
            )
        });
        direct_opaque_fallback.sort_unstable_by_key(|draw| {
            (
                draw.material_group,
                draw.vertex_buffer,
                draw.vertex_offset,
                draw.first_index,
                draw.first_instance,
            )
        });

        let mut hiz_candidate_bytes =
            Vec::<u8>::with_capacity(pending_hiz_draws.len() * HIZ_CANDIDATE_STRIDE as usize);
        let mut hiz_indirect_bytes =
            Vec::<u8>::with_capacity(pending_hiz_draws.len() * HIZ_INDIRECT_STRIDE as usize);
        let mut hiz_draws = Vec::<(u32, u64)>::with_capacity(pending_hiz_draws.len());
        for (candidate_index, draw) in pending_hiz_draws.iter().enumerate() {
            append_hiz_candidate(&mut hiz_candidate_bytes, draw.sphere);
            append_indexed_indirect_command(
                &mut hiz_indirect_bytes,
                draw.index_count,
                draw.instance_count,
                draw.first_index,
                draw.first_instance,
            );
            hiz_draws.push((
                draw.material_group,
                candidate_index as u64 * HIZ_INDIRECT_STRIDE,
            ));
        }

        if !hiz_candidate_bytes.is_empty() {
            render.write_buffer(
                gpu.visibility_candidate_buffers[frame_slot],
                0,
                &hiz_candidate_bytes,
            )?;
            render.write_buffer(
                gpu.visibility_indirect_buffers[frame_slot],
                0,
                &hiz_indirect_bytes,
            )?;
        }
        Ok(FrameAssetSubmission {
            instance_batches,
            visible_instance_slots,
            hiz_draws,
            direct_opaque_fallback,
        })
    }
}
