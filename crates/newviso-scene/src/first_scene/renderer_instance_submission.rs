//! Visible instance runs and dynamic instance uploads.
use super::*;
impl Scene3dRuntime {
    pub(super) fn build_frame_asset_instances(&self,render:&RenderClient,gpu:GpuScene,
        frame_slot:usize)->Result<(Vec<GpuResidentInstanceBatch>,BTreeMap<u64,u32>),String>{
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
                dynamic_instance_data.extend_from_slice(
                    &self.asset_instance_model_matrix(*stable_id, entity.transform),
                );
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

        Ok((instance_batches,visible_instance_slots))
    }
}
