use super::*;

impl Scene3dRuntime {
    pub(super) fn ensure_geometry_buffer_capacity(
        &mut self,
        required_vertices: u32,
        required_shadow_vertices: u32,
    ) -> Result<(), String> {
        let Some(mut gpu) = self.gpu else {
            return Err("3D scene GPU resources are not initialized".to_owned());
        };
        let render = RenderClient::new();
        let vertices = grown_vertex_ring(
            &render,
            required_vertices,
            gpu.vertex_capacity,
            "newviso.first_scene.vertices.grown",
        )?;
        let shadows = match grown_vertex_ring(
            &render,
            required_shadow_vertices,
            gpu.shadow_vertex_capacity,
            "newviso.first_scene.shadow_vertices.grown",
        ) {
            Ok(shadows) => shadows,
            Err(error) => {
                if let Some((buffers, _)) = vertices {
                    for buffer in buffers {
                        render.destroy_buffer(buffer);
                    }
                }
                return Err(error);
            }
        };
        // Commit only after both allocations succeed. A failed shadow growth
        // must not destroy the still-authoritative scene vertex ring.
        let changed = vertices.is_some() || shadows.is_some();
        if let Some((buffers, capacity)) = vertices {
            for old in gpu.vertex_buffers {
                render.destroy_buffer(old);
            }
            gpu.vertex_buffers = buffers;
            gpu.vertex_capacity = capacity;
        }
        if let Some((buffers, capacity)) = shadows {
            for old in gpu.shadow_vertex_buffers {
                render.destroy_buffer(old);
            }
            gpu.shadow_vertex_buffers = buffers;
            gpu.shadow_vertex_capacity = capacity;
        }

        if changed {
            host_runtime::debug(
                "newviso.scene",
                format!(
                    "scene geometry buffers grown vertices={} shadow_vertices={}",
                    gpu.vertex_capacity, gpu.shadow_vertex_capacity
                ),
            );
            self.gpu = Some(gpu);
        }
        Ok(())
    }

    pub(super) fn sync_static_asset_gpu(&mut self) -> Result<(), String> {
        self.rebuild_static_asset_vertex_data()?;
        if self.asset_upload_from_float.is_none() {
            return Ok(());
        }

        let required_vertices = u32::try_from(self.asset_vertex_data.len() / FLOATS_PER_VERTEX)
            .map_err(|_| "static asset vertex count exceeds u32".to_owned())?;
        let Some(mut gpu) = self.gpu else {
            return Err("3D scene GPU resources are not initialized".to_owned());
        };
        let render = RenderClient::new();
        let had_static_upload = self.asset_upload_from_float.is_some();
        let mut upload_from_float = self
            .asset_upload_from_float
            .unwrap_or(self.asset_vertex_data.len())
            .min(self.asset_vertex_data.len());
        let mut buffer_grew = false;

        if required_vertices > gpu.asset_vertex_capacity
            || required_vertices > gpu.asset_index_capacity
        {
            let next = required_vertices
                .checked_next_power_of_two()
                .unwrap_or(required_vertices)
                .max(1);
            let new_buffer = render.create_buffer(
                "newviso.scene.static_asset_vertices.grown",
                next as u64 * VERTEX_STRIDE,
                "Vertex",
                "GpuOnly",
            )?;
            let new_index_buffer = render.create_buffer(
                "newviso.scene.static_asset_indices.grown",
                next as u64 * std::mem::size_of::<u32>() as u64,
                "Index",
                "GpuOnly",
            )?;
            self.retired_asset_vertex_buffers
                .push(gpu.asset_vertex_buffer);
            self.retired_asset_vertex_buffers
                .push(gpu.asset_index_buffer);
            gpu.asset_vertex_buffer = new_buffer;
            gpu.asset_vertex_capacity = next;
            gpu.asset_index_buffer = new_index_buffer;
            gpu.asset_index_capacity = next;
            upload_from_float = 0;
            buffer_grew = true;
        }

        // Static installation/appends still upload one contiguous tail and update
        // the sequential index buffer for newly appended geometry.
        if upload_from_float < self.asset_vertex_data.len() {
            let byte_offset = (upload_from_float as u64)
                .checked_mul(std::mem::size_of::<f32>() as u64)
                .ok_or_else(|| "static asset upload byte offset overflow".to_owned())?;
            render.write_buffer_f32(
                gpu.asset_vertex_buffer,
                byte_offset,
                &self.asset_vertex_data[upload_from_float..],
            )?;

            let first_index = upload_from_float / FLOATS_PER_VERTEX;
            let mut index_bytes = Vec::with_capacity(
                (required_vertices as usize).saturating_sub(first_index)
                    * std::mem::size_of::<u32>(),
            );
            for index in first_index..required_vertices as usize {
                let index = u32::try_from(index)
                    .map_err(|_| "static asset sequential index exceeds u32".to_owned())?;
                index_bytes.extend_from_slice(&index.to_le_bytes());
            }
            render.write_buffer(
                gpu.asset_index_buffer,
                first_index as u64 * std::mem::size_of::<u32>() as u64,
                &index_bytes,
            )?;
        }

        gpu.asset_vertex_count = required_vertices;
        self.gpu = Some(gpu);
        self.asset_upload_from_float = None;

        // Do not emit a per-frame debug line for animation-only uploads. Besides
        // log noise, serializing one line every render frame is measurable work.
        if had_static_upload || buffer_grew {
            host_runtime::debug(
                "newviso.scene",
                format!(
                    "static asset GPU sync instances={} vertices={} capacity={} uploaded_from_vertex={}",
                    self.asset_meshes.len(),
                    required_vertices,
                    gpu.asset_vertex_capacity,
                    upload_from_float / FLOATS_PER_VERTEX
                ),
            );
        }
        Ok(())
    }
    pub(super) fn sync_skinned_vertex_gpu(
        &mut self,
        render: &RenderClient,
        frame_slot: usize,
    ) -> Result<GpuScene, String> {
        if frame_slot >= SCENE_FRAME_SLOTS {
            return Err(format!(
                "skinned vertex frame slot {frame_slot} exceeds ring size {SCENE_FRAME_SLOTS}"
            ));
        }

        let required_vertices =
            u32::try_from(self.skinned_vertex_data.len() / FLOATS_PER_VERTEX)
                .map_err(|_| "compact skinned vertex count exceeds u32".to_owned())?;
        let mut gpu = self
            .gpu
            .ok_or_else(|| "3D scene GPU resources are not initialized".to_owned())?;

        if required_vertices > gpu.skinned_vertex_capacity {
            let next = required_vertices
                .checked_next_power_of_two()
                .unwrap_or(required_vertices)
                .max(DEFAULT_SKINNED_VERTEX_CAPACITY);
            let mut new_buffers = [0u32; SCENE_FRAME_SLOTS];
            for slot in 0..SCENE_FRAME_SLOTS {
                new_buffers[slot] = render.create_frame_buffer(
                    slot,
                    &format!("newviso.scene.skinned_vertices.grown.slot.{slot}"),
                    next as u64 * VERTEX_STRIDE,
                    "Vertex",
                    "CpuToGpu",
                )?;
            }
            for old_buffer in gpu.skinned_vertex_buffers {
                render.destroy_buffer(old_buffer);
            }
            gpu.skinned_vertex_buffers = new_buffers;
            gpu.skinned_vertex_capacity = next;
            let full = (0, self.skinned_vertex_data.len());
            for ranges in &mut self.skinned_dirty_ranges {
                ranges.clear();
                if full.1 != 0 {
                    ranges.push(full);
                }
            }
            self.gpu = Some(gpu);
        }

        if self.skinned_dirty_ranges[frame_slot].is_empty() {
            return Ok(gpu);
        }

        let mut ranges = std::mem::take(&mut self.skinned_dirty_ranges[frame_slot]);
        ranges.sort_unstable_by_key(|range| range.0);
        let mut merged = Vec::<(usize, usize)>::new();
        for (start, end) in ranges {
            let start = start.min(self.skinned_vertex_data.len());
            let end = end.min(self.skinned_vertex_data.len());
            if start >= end {
                continue;
            }
            if let Some(last) = merged.last_mut() {
                if start <= last.1 {
                    last.1 = last.1.max(end);
                    continue;
                }
            }
            merged.push((start, end));
        }

        if !merged.is_empty() {
            render.write_buffer_f32_ranges(
                gpu.skinned_vertex_buffers[frame_slot],
                &self.skinned_vertex_data,
                &merged,
            )?;
        }
        Ok(gpu)
    }
}

fn grown_vertex_ring(
    render: &RenderClient,
    required: u32,
    capacity: u32,
    label: &str,
) -> Result<Option<([u32; SCENE_FRAME_SLOTS], u32)>, String> {
    if required <= capacity {
        return Ok(None);
    }
    let next = required
        .checked_next_power_of_two()
        .unwrap_or(required)
        .max(1);
    let buffers = create_frame_buffer_ring(render, label, next as u64 * VERTEX_STRIDE, "Vertex")?;
    Ok(Some((buffers, next)))
}
