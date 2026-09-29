use super::*;

fn persistent_instance_cell(position: Vec3) -> (i32, i32, i32) {
    let q = |value: f32| (value / ASSET_INSTANCE_CELL_SIZE).floor() as i32;
    (q(position.x), q(position.y), q(position.z))
}

fn batch_sphere(world: &SceneWorld, stable_ids: &[u64]) -> Option<[f32; 4]> {
    let mut min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut any = false;
    for stable_id in stable_ids {
        let entity = world.entity(SceneEntityId(*stable_id))?;
        min.x = min.x.min(entity.bounds.min.x);
        min.y = min.y.min(entity.bounds.min.y);
        min.z = min.z.min(entity.bounds.min.z);
        max.x = max.x.max(entity.bounds.max.x);
        max.y = max.y.max(entity.bounds.max.y);
        max.z = max.z.max(entity.bounds.max.z);
        any = true;
    }
    if !any {
        return None;
    }
    let center = Vec3::new(
        (min.x + max.x) * 0.5,
        (min.y + max.y) * 0.5,
        (min.z + max.z) * 0.5,
    );
    Some([
        center.x,
        center.y,
        center.z,
        max.sub(center).length().max(0.05),
    ])
}

impl Scene3dRuntime {
    pub(super) fn sync_gpu_instance_table(
        &mut self,
        render: &RenderClient,
        gpu: GpuScene,
        frame_slot: usize,
    ) -> Result<(), String> {
        if frame_slot >= SCENE_FRAME_SLOTS {
            return Err(format!(
                "scene frame slot {frame_slot} exceeds ring size {SCENE_FRAME_SLOTS}"
            ));
        }
        let render_epoch = self.world.static_render_epoch();
        let asset_epoch = self.static_asset_instance_epoch;
        let rebuild = self.gpu_instance_table.source_render_epoch != render_epoch
            || self.gpu_instance_table.source_asset_epoch != asset_epoch;

        if rebuild {
            let rebuild_count = self.gpu_instance_table.rebuild_count.wrapping_add(1);
            let upload_count = self.gpu_instance_table.upload_count;
            let generation = self.gpu_instance_table.generation.wrapping_add(1).max(1);
            let mut grouped = BTreeMap::<GpuInstanceBatchKey, Vec<u64>>::new();

            for (stable_id, mesh) in &self.asset_meshes {
                let Some(entity) = self.world.entity(SceneEntityId(*stable_id)) else {
                    continue;
                };
                if entity.mobility != SceneMobility::Static
                    || !entity.is_renderable()
                    || !entity.visibility.all_visible()
                {
                    continue;
                }
                let (cx, cy, cz) = persistent_instance_cell(entity.transform.position);
                grouped
                    .entry((mesh.model_id.0, cx, cy, cz, 0))
                    .or_default()
                    .push(*stable_id);
            }

            let instance_count = grouped.values().map(Vec::len).sum::<usize>();
            if instance_count > gpu.asset_instance_capacity as usize {
                return Err(format!(
                    "resident static asset instance count {} exceeds GPU instance capacity {}",
                    instance_count, gpu.asset_instance_capacity
                ));
            }

            let mut table = GpuInstanceTable {
                source_render_epoch: render_epoch,
                source_asset_epoch: asset_epoch,
                instance_data: Vec::with_capacity(instance_count.saturating_mul(INSTANCE_FLOATS)),
                batches: BTreeMap::new(),
                entity_slots: BTreeMap::new(),
                entity_batches: BTreeMap::new(),
                generation,
                uploaded_generation: [0; SCENE_FRAME_SLOTS],
                rebuild_count,
                upload_count,
            };

            for (key, stable_ids) in grouped {
                let first_instance = u32::try_from(table.instance_data.len() / INSTANCE_FLOATS)
                    .map_err(|_| "static asset instance offset exceeds u32".to_owned())?;
                for stable_id in &stable_ids {
                    let entity = self
                        .world
                        .entity(SceneEntityId(*stable_id))
                        .ok_or_else(|| format!("static asset entity {} disappeared", stable_id))?;
                    let slot = u32::try_from(table.instance_data.len() / INSTANCE_FLOATS)
                        .map_err(|_| "static asset instance index exceeds u32".to_owned())?;
                    table
                        .instance_data
                        .extend_from_slice(&geometry::instance_model_matrix(
                            entity.transform.position,
                            entity.transform.rotation_degrees,
                            entity.transform.scale,
                        ));
                    table.entity_slots.insert(*stable_id, slot);
                    table.entity_batches.insert(*stable_id, key);
                }
                let count = u32::try_from(stable_ids.len())
                    .map_err(|_| "static asset instance count exceeds u32".to_owned())?;
                let sphere = batch_sphere(&self.world, &stable_ids)
                    .ok_or_else(|| "static asset batch lost scene entities".to_owned())?;
                table.batches.insert(
                    key,
                    GpuResidentInstanceBatch {
                        model_id: key.0,
                        first_instance,
                        instance_count: count,
                        stable_ids,
                        sphere,
                    },
                );
            }
            self.gpu_instance_table = table;
        }

        if self.gpu_instance_table.uploaded_generation[frame_slot]
            != self.gpu_instance_table.generation
        {
            if !self.gpu_instance_table.instance_data.is_empty() {
                render.write_buffer_f32(
                    gpu.asset_instance_buffers[frame_slot],
                    0,
                    &self.gpu_instance_table.instance_data,
                )?;
            }
            self.gpu_instance_table.uploaded_generation[frame_slot] =
                self.gpu_instance_table.generation;
            self.gpu_instance_table.upload_count =
                self.gpu_instance_table.upload_count.wrapping_add(1);
        }
        Ok(())
    }

    pub(super) fn invalidate_gpu_instance_upload(&mut self) {
        self.gpu_instance_table.uploaded_generation = [u64::MAX; SCENE_FRAME_SLOTS];
    }
}
