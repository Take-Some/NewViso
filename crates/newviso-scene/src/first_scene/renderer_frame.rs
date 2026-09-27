use super::*;
use std::collections::BTreeSet;

fn instance_cell(position: Vec3) -> (i32, i32, i32) {
    let q = |value: f32| (value / ASSET_INSTANCE_CELL_SIZE).floor() as i32;
    (q(position.x), q(position.y), q(position.z))
}

fn instance_ids_sphere(world: &SceneWorld, stable_ids: &[u64]) -> Option<[f32; 4]> {
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
    let radius = max.sub(center).length().max(0.05);
    Some([center.x, center.y, center.z, radius])
}

fn append_hiz_candidate(out: &mut Vec<u8>, sphere: [f32; 4]) {
    for value in sphere {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in [1u32, 0, 0, 0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn append_indexed_indirect_command(
    out: &mut Vec<u8>,
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    first_instance: u32,
) {
    out.extend_from_slice(&index_count.to_le_bytes());
    out.extend_from_slice(&instance_count.to_le_bytes());
    out.extend_from_slice(&first_index.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&first_instance.to_le_bytes());
}

impl Scene3dRuntime {
    pub fn update_native_input_from_snapshot(
        &mut self,
        input: &InputSnapshot,
        dt: f32,
        camera_navigation_enabled: bool,
    ) -> Result<(), String> {
        if camera_navigation_enabled {
            let [dx, dy] = input.mouse_delta();
            let wheel_y = input.mouse_wheel_y();
            let rotating = input.mouse_button_down(self.orbit.rotate_button);
            if ((dx != 0.0 || dy != 0.0) && rotating) || wheel_y != 0.0 {
                self.orbit.apply_mouse(dx, dy, wheel_y, rotating);
                self.camera.position = self.orbit.position(self.camera.target);
                self.sync_runtime_camera_to_flecs()?;
            }
        }
        self.tick(dt)
    }
    pub fn tick(&mut self, dt: f32) -> Result<(), String> {
        if dt.is_finite() && dt > 0.0 {
            self.sky_time_seconds = (self.sky_time_seconds + dt * self.sky_time_scale)
                .rem_euclid(self.timecycle_backend.duration_seconds);
            self.update_skinned_animations(dt)?;
        }
        self.update_scene_world(dt)
    }
    pub fn render_frame(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.render_frame_with_overlay(width, height, || Ok(()))
    }
    pub fn render_frame_with_overlay<F>(
        &mut self,
        width: u32,
        height: u32,
        overlay: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        if self.gpu.is_none() {
            return Err("3D scene GPU resources are not initialized".to_owned());
        }

        let width = width.max(1);
        let height = height.max(1);
        let aspect = width as f32 / height as f32;
        let forward = self.camera.target.sub(self.camera.position).normalized();
        let view = SceneView {
            position: self.camera.position,
            forward,
            up: self.camera.up,
            near: self.camera.near,
            far: self.camera.far,
            fov_y_radians: self.camera.fov_y_degrees.to_radians(),
            aspect,
        };
        let mut visibility_candidates = self.world.visibility_candidates(view);
        if let Some(graph) = self.portal_visibility.as_mut() {
            graph.filter_candidates(&self.camera, aspect, &mut visibility_candidates);
        }
        self.frame_plan = self
            .world
            .scan_visibility_candidates(view, visibility_candidates);
        self.sync_static_asset_gpu()?;
        self.sync_static_asset_material_gpu()?;
        let required_vertices = self.vertex_count();
        let required_shadow_vertices = self.shadow_vertex_count();
        self.ensure_geometry_buffer_capacity(required_vertices, required_shadow_vertices)?;
        let gpu = self
            .gpu
            .ok_or_else(|| "3D scene GPU resources disappeared".to_owned())?;
        let vertex_data = self.build_cube_vertices(aspect);
        let shadow_vertex_data = self.build_shadow_vertices();
        let flare_vertex_data = self.build_lens_flare_vertices(aspect);
        let flare_vertex_count =
            u32::try_from(flare_vertex_data.len() / FLARE_FLOATS_PER_VERTEX).unwrap_or(0);
        let (frame_uniform, shadow_enabled) =
            self.scene_frame_uniform(aspect, gpu.shadow_resolution);
        let render = RenderClient::new();
        self.sync_gpu_instance_table(&render, gpu)?;

        if !vertex_data.is_empty() {
            render.write_buffer_f32(gpu.vertex_buffer, 0, &vertex_data)?;
        }
        if !shadow_vertex_data.is_empty() {
            render.write_buffer_f32(gpu.shadow_vertex_buffer, 0, &shadow_vertex_data)?;
        }
        if flare_vertex_count > 0 {
            render.write_buffer_f32(gpu.flare_vertex_buffer, 0, &flare_vertex_data)?;
        }
        render.write_buffer_f32(gpu.frame_uniform, 0, &frame_uniform)?;
        if let Some(gpu_sky) = self.gpu_sky {
            let sky_frame = self.sky_frame_uniform(width as f32 / height as f32);
            render.write_buffer_f32(gpu_sky.camera_uniform, 0, &sky_frame)?;
        }

        let frame_index = self.frame_index;
        if let Err(error) = self.render_frame_inner(
            &render,
            gpu,
            width,
            height,
            frame_index,
            shadow_enabled,
            flare_vertex_count,
            overlay,
        ) {
            render.abort_frame();
            return Err(error);
        }

        self.frame_index = self.frame_index.wrapping_add(1);
        Ok(())
    }
    pub(super) fn render_frame_inner<F>(
        &self,
        render: &RenderClient,
        gpu: GpuScene,
        width: u32,
        height: u32,
        frame_index: u64,
        shadow_enabled: bool,
        flare_vertex_count: u32,
        overlay: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
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
            let visible_members = batch
                .stable_ids
                .iter()
                .filter_map(|stable_id| {
                    visible_instance_slots
                        .get(stable_id)
                        .copied()
                        .map(|slot| (*stable_id, slot))
                })
                .collect::<Vec<_>>();
            if visible_members.len() == batch.stable_ids.len() {
                instance_batches.push(batch.clone());
            } else {
                for (stable_id, slot) in visible_members {
                    let sphere = instance_ids_sphere(&self.world, &[stable_id])
                        .ok_or_else(|| "visible static instance lost scene entity".to_owned())?;
                    instance_batches.push(GpuResidentInstanceBatch {
                        model_id: batch.model_id,
                        first_instance: slot,
                        instance_count: 1,
                        stable_ids: vec![stable_id],
                        sphere,
                    });
                }
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
                gpu.asset_instance_buffer,
                static_instance_count as u64 * INSTANCE_STRIDE,
                &dynamic_instance_data,
            )?;
        }

        // Build one fail-open indirect command per opaque/cutout primitive
        // range and spatial instance batch. The Vulkan visibility provider may
        // zero instance_count using previous-frame Hi-Z; if it cannot, these
        // CPU-initialized commands remain fully drawable.
        let mut hiz_candidate_bytes = Vec::<u8>::new();
        let mut hiz_indirect_bytes = Vec::<u8>::new();
        let mut hiz_draws = Vec::<(u32, u64)>::new();
        let mut direct_opaque_fallback = Vec::<(u32, u32, u32, u32, u32)>::new();

        // Split only main-view submissions that contain an instance override.
        // Shared geometry and the full shadow batches remain unchanged.
        for (batch, member) in instance_batches.iter().flat_map(|batch| {
            let split = batch.instance_count > 1
                && batch.stable_ids.iter().any(|id| {
                    self.main_view_mesh_visibility.has_override(*id)
                });
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
            let instance_count = if member.is_some() { 1 } else { batch.instance_count };
            let first_instance = batch.first_instance + member.unwrap_or(0) as u32;

            let mut queue_range = |material_group: u32,
                                   first_index: u32,
                                   index_count: u32|
             -> Result<(), String> {
                let candidate_index = hiz_draws.len();
                if ENABLE_ASSET_HIZ_OCCLUSION && candidate_index < MAX_HIZ_DRAW_CANDIDATES as usize
                {
                    append_hiz_candidate(&mut hiz_candidate_bytes, sphere);
                    append_indexed_indirect_command(
                        &mut hiz_indirect_bytes,
                        index_count,
                        instance_count,
                        first_index,
                        first_instance,
                    );
                    hiz_draws.push((material_group, candidate_index as u64 * HIZ_INDIRECT_STRIDE));
                } else {
                    direct_opaque_fallback.push((
                        material_group,
                        index_count,
                        first_index,
                        instance_count,
                        first_instance,
                    ));
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

            for range in mesh.local_draw_ranges.iter() {
                if !self.main_view_mesh_visibility.visible(*stable_id, &range.mesh_name) {
                    continue;
                }
                let material = range
                    .material_slot
                    .and_then(|slot| mesh.materials.get(slot as usize));
                if material.is_some_and(|material| material.alpha_mode == AssetAlphaMode::Blend) {
                    continue;
                }
                let material_group = range
                    .material_slot
                    .and_then(|slot| self.asset_gpu_materials.get(&(mesh.model_id.0, slot)))
                    .map(|material| material.bind_group)
                    .unwrap_or(gpu.default_material_bind_group);
                queue_range(
                    material_group,
                    mesh.first_vertex.saturating_add(range.first_vertex),
                    range.vertex_count,
                )?;
            }
        }

        if !hiz_candidate_bytes.is_empty() {
            render.write_buffer(gpu.visibility_candidate_buffer, 0, &hiz_candidate_bytes)?;
            render.write_buffer(gpu.visibility_indirect_buffer, 0, &hiz_indirect_bytes)?;
        }

        render.begin_frame(self.clear_color, frame_index)?;
        if !hiz_draws.is_empty() {
            let forward = self.camera.target.sub(self.camera.position).normalized();
            render.set_render_phase(Some("VisibilityCull"))?;
            render.dispatch_visibility_indirect_cull(
                gpu.visibility_candidate_buffer,
                gpu.visibility_indirect_buffer,
                u32::try_from(hiz_draws.len())
                    .map_err(|_| "Hi-Z draw count exceeds u32".to_owned())?,
                [width, height],
                [
                    self.camera.position.x,
                    self.camera.position.y,
                    self.camera.position.z,
                ],
                [forward.x, forward.y, forward.z],
                [self.camera.up.x, self.camera.up.y, self.camera.up.z],
                self.camera.fov_y_degrees.to_radians(),
                self.camera.near,
                self.camera.far,
            )?;
            render.set_render_phase(None)?;
        }

        if shadow_enabled && (self.shadow_vertex_count() > 0 || !instance_batches.is_empty()) {
            render.begin_render_target(
                gpu.shadow_render_target,
                Some([1.0, 1.0, 1.0, 1.0]),
                Some(1.0),
            )?;
            render.set_viewport(gpu.shadow_resolution, gpu.shadow_resolution)?;
            render.set_scissor(gpu.shadow_resolution, gpu.shadow_resolution)?;
            render.set_pipeline(gpu.shadow_pipeline)?;
            render.set_bind_group(0, gpu.shadow_bind_group)?;
            render.set_bind_group(1, gpu.default_material_bind_group)?;

            let dynamic_shadow_vertices = self.shadow_vertex_count();
            if dynamic_shadow_vertices > 0 {
                render.set_vertex_buffer(0, gpu.shadow_vertex_buffer, 0)?;
                render.draw(dynamic_shadow_vertices)?;
            }

            if !instance_batches.is_empty() {
                render.set_pipeline(gpu.asset_shadow_pipeline)?;
                render.set_bind_group(0, gpu.shadow_bind_group)?;
                render.set_vertex_buffer(0, gpu.asset_vertex_buffer, 0)?;
                render.set_vertex_buffer(1, gpu.asset_instance_buffer, 0)?;
                render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;

                let mut bound_material = None;
                for batch in &instance_batches {
                    let Some(stable_id) = batch.stable_ids.first() else {
                        continue;
                    };
                    let mesh = self
                        .asset_meshes
                        .get(stable_id)
                        .ok_or_else(|| format!("asset instance {} disappeared", stable_id))?;
                    debug_assert_eq!(mesh.model_id.0, batch.model_id);

                    if mesh.local_draw_ranges.is_empty() {
                        if bound_material != Some(gpu.default_material_bind_group) {
                            render.set_bind_group(1, gpu.default_material_bind_group)?;
                            bound_material = Some(gpu.default_material_bind_group);
                        }
                        render.draw_indexed_range_instanced(
                            mesh.vertex_count,
                            mesh.first_vertex,
                            0,
                            batch.instance_count,
                            batch.first_instance,
                        )?;
                        continue;
                    }

                    for range in mesh.local_draw_ranges.iter() {
                        let material_group = range
                            .material_slot
                            .and_then(|slot| self.asset_gpu_materials.get(&(mesh.model_id.0, slot)))
                            .map(|material| material.bind_group)
                            .unwrap_or(gpu.default_material_bind_group);
                        if bound_material != Some(material_group) {
                            render.set_bind_group(1, material_group)?;
                            bound_material = Some(material_group);
                        }
                        render.draw_indexed_range_instanced(
                            range.vertex_count,
                            mesh.first_vertex.saturating_add(range.first_vertex),
                            0,
                            batch.instance_count,
                            batch.first_instance,
                        )?;
                    }
                }
            }
            render.end_render_target()?;
        }

        render.set_viewport(width, height)?;
        render.set_scissor(width, height)?;

        if let Some(sky) = self.gpu_sky {
            render.set_pipeline(sky.pipeline)?;
            render.set_bind_group(0, sky.bind_group)?;
            render.set_vertex_buffer(0, sky.vertex_buffer, 0)?;
            render.set_index_buffer(sky.index_buffer, 0, sky.index_format)?;
            render.draw_indexed(sky.index_count)?;
        }

        render.set_pipeline(gpu.pipeline)?;
        render.set_bind_group(0, gpu.bind_group)?;
        render.set_bind_group(1, gpu.default_material_bind_group)?;
        let dynamic_vertices = self.vertex_count();
        if dynamic_vertices > 0 {
            render.set_vertex_buffer(0, gpu.vertex_buffer, 0)?;
            render.draw(dynamic_vertices)?;
        }

        let mut alpha_draws = Vec::<(f32, u32, u32, u32, u32)>::new();
        for id in &self.frame_plan.visible_entities {
            let Some(instance_index) = visible_instance_slots.get(&id.0).copied() else {
                continue;
            };
            let Some(mesh) = self.asset_meshes.get(&id.0) else {
                continue;
            };
            let distance_sq = self
                .world
                .entity(*id)
                .map(|entity| {
                    let delta = entity.transform.position.sub(self.camera.position);
                    delta.dot(delta)
                })
                .unwrap_or(0.0);

            for range in mesh.local_draw_ranges.iter() {
                if !self.main_view_mesh_visibility.visible(id.0, &range.mesh_name) {
                    continue;
                }
                let material = range
                    .material_slot
                    .and_then(|slot| mesh.materials.get(slot as usize));
                if !material.is_some_and(|material| material.alpha_mode == AssetAlphaMode::Blend) {
                    continue;
                }
                let material_group = range
                    .material_slot
                    .and_then(|slot| self.asset_gpu_materials.get(&(mesh.model_id.0, slot)))
                    .map(|material| material.bind_group)
                    .unwrap_or(gpu.default_material_bind_group);
                alpha_draws.push((
                    distance_sq,
                    material_group,
                    mesh.first_vertex.saturating_add(range.first_vertex),
                    range.vertex_count,
                    instance_index,
                ));
            }
        }

        if !hiz_draws.is_empty() || !direct_opaque_fallback.is_empty() {
            render.set_pipeline(gpu.asset_pipeline)?;
            render.set_bind_group(0, gpu.bind_group)?;
            render.set_vertex_buffer(0, gpu.asset_vertex_buffer, 0)?;
            render.set_vertex_buffer(1, gpu.asset_instance_buffer, 0)?;
            render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;
            let mut bound_material = None;

            let mut draw_index = 0usize;
            while draw_index < hiz_draws.len() {
                let (material_group, indirect_offset) = hiz_draws[draw_index];
                let mut draw_count = 1usize;
                while draw_index + draw_count < hiz_draws.len()
                    && hiz_draws[draw_index + draw_count].0 == material_group
                    && hiz_draws[draw_index + draw_count].1
                        == indirect_offset + draw_count as u64 * HIZ_INDIRECT_STRIDE
                {
                    draw_count += 1;
                }

                if bound_material != Some(material_group) {
                    render.set_bind_group(1, material_group)?;
                    bound_material = Some(material_group);
                }
                render.draw_indexed_indirect(
                    gpu.visibility_indirect_buffer,
                    indirect_offset,
                    u32::try_from(draw_count)
                        .map_err(|_| "Hi-Z multi-draw count exceeds u32".to_owned())?,
                    HIZ_INDIRECT_STRIDE as u32,
                )?;
                draw_index += draw_count;
            }

            for (material_group, index_count, first_index, instance_count, first_instance) in
                direct_opaque_fallback
            {
                if bound_material != Some(material_group) {
                    render.set_bind_group(1, material_group)?;
                    bound_material = Some(material_group);
                }
                render.draw_indexed_range_instanced(
                    index_count,
                    first_index,
                    0,
                    instance_count,
                    first_instance,
                )?;
            }
        }

        if !alpha_draws.is_empty() {
            alpha_draws.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            render.set_pipeline(gpu.asset_alpha_pipeline)?;
            render.set_bind_group(0, gpu.bind_group)?;
            render.set_vertex_buffer(0, gpu.asset_vertex_buffer, 0)?;
            render.set_vertex_buffer(1, gpu.asset_instance_buffer, 0)?;
            render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;
            let mut bound_material = None;
            for (_, material_group, first_index, index_count, instance_index) in alpha_draws {
                if bound_material != Some(material_group) {
                    render.set_bind_group(1, material_group)?;
                    bound_material = Some(material_group);
                }
                render.draw_indexed_range_instanced(
                    index_count,
                    first_index,
                    0,
                    1,
                    instance_index,
                )?;
            }
        }

        if flare_vertex_count > 0 {
            render.set_pipeline(gpu.flare_pipeline)?;
            render.set_vertex_buffer(0, gpu.flare_vertex_buffer, 0)?;
            render.draw(flare_vertex_count)?;
        }

        overlay()?;
        render.end_frame()
    }
}
