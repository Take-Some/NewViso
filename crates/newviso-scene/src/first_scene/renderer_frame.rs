use super::*;

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
        // SceneWorld owns process cadence. Advance it before process consumers
        // (animation, scripting, movers) inspect the current frame's tickets.
        self.update_scene_world(dt)?;

        if dt.is_finite() && dt > 0.0 {
            let cycle_delta_seconds = dt * self.sky_time_scale;
            self.sky_time_seconds = (self.sky_time_seconds + cycle_delta_seconds)
                .rem_euclid(self.timecycle_backend.duration_seconds);

            // GTA's large sky-cloud base noise is not driven by time-of-day.
            // Its phase integrates real frame time along normalized global-air
            // wind direction. Wind magnitude does not alter phase speed; the
            // dedicated large_speed scalar does.
            let volumetric_clouds_enabled = self
                .sky
                .as_ref()
                .is_some_and(|sky| sky.volumetric_clouds.enabled);
            if self.sky_clouds.enabled || volumetric_clouds_enabled {
                let wind = self.sky_clouds.speed;
                let wind_len = (wind[0] * wind[0] + wind[1] * wind[1]).sqrt();
                let direction = if wind_len > 1.0e-6 {
                    [wind[0] / wind_len, wind[1] / wind_len]
                } else {
                    [1.0, 0.0]
                };
                let phase_scale = self.sky_clouds.noise_phase_scale;
                let phase_step = self.sky_clouds.large_speed * phase_scale * phase_scale * dt;
                self.sky_cloud_noise_phase[0] =
                    (self.sky_cloud_noise_phase[0] + direction[0] * phase_step).rem_euclid(1.0);
                self.sky_cloud_noise_phase[1] =
                    (self.sky_cloud_noise_phase[1] + direction[1] * phase_step).rem_euclid(1.0);
            }

            // GTA's small/overall/edge cloud phases use continuous time-cycle
            // time measured in days (daysBetween + dayRatio), not elapsed seconds.
            self.sky_cloud_cycle_time_days = (self.sky_cloud_cycle_time_days
                + cycle_delta_seconds / self.timecycle_backend.duration_seconds)
                .rem_euclid(4096.0);

            self.update_atmospheric_clouds(dt);
            self.update_weather_gpu_fx(dt);
            self.update_particles(dt);
            self.update_skinned_animations(dt)?;
            // Joint constraints are presentation transforms and must observe the
            // current animation clock, not the previous script-command phase.
            self.update_joint_attachments()?;
        }
        Ok(())
    }
    /// Rebuilds the initial camera/streaming plan without submitting a frame.
    ///
    /// Startup uses this while the loading compositor still owns the window so
    /// the resource runtime can discover the exact models needed around spawn.
    pub fn refresh_streaming_plan(&mut self, width: u32, height: u32) -> f32 {
        let width = width.max(1);
        let height = height.max(1);
        let aspect = width as f32 / height as f32;
        self.world.refresh_focus_for_streaming(self.camera.position);
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
        aspect
    }

    /// Materializes renderer-side static map resources without beginning or
    /// presenting a playable frame. Deferred texture uploads are intentionally
    /// left in the renderer queue for the loading-screen warmup pump.
    pub fn warmup_static_asset_gpu(&mut self) -> Result<(), String> {
        if self.gpu.is_none() {
            return Err("3D scene GPU resources are not initialized".to_owned());
        }
        self.sync_static_asset_gpu()?;
        self.sync_startup_asset_material_gpu()?;
        let required_vertices = self.vertex_count();
        let required_shadow_vertices = self.shadow_vertex_count();
        self.ensure_geometry_buffer_capacity(required_vertices, required_shadow_vertices)
    }

    /// Exact renderer-side readiness for the entire initial streaming radius.
    ///
    /// asset_gpu_textures only contains textures whose backend residency query
    /// already returned Ready. Pending entries are therefore not treated as ready,
    /// even when the global upload queue happens to be empty between batches.
    pub fn startup_gpu_warmup_status(&self) -> SceneGpuWarmupStatus {
        let mut required_textures = BTreeSet::<u64>::new();
        let mut required_materials = BTreeSet::<(u64, u32)>::new();
        let mut material_entities = 0usize;

        for id in &self.frame_plan.streaming_entities {
            let Some(mesh) = self.asset_meshes.get(&id.0) else {
                continue;
            };
            material_entities = material_entities.saturating_add(1);
            for (slot, material) in mesh.materials.iter().enumerate() {
                if let Ok(slot) = u32::try_from(slot) {
                    required_materials.insert((mesh.model_id.0, slot));
                }
                for texture in [
                    material.textures.base_color.as_ref(),
                    material.textures.normal.as_ref(),
                    material.textures.specular.as_ref(),
                    material.textures.emissive.as_ref(),
                    material.textures.environment.as_ref(),
                ]
                .into_iter()
                .flatten()
                .chain(material.textures.auxiliary_textures.values())
                {
                    required_textures.insert(texture.id.0);
                }
            }
        }

        let ready_textures = required_textures
            .iter()
            .filter(|asset_id| self.asset_gpu_textures.contains_key(asset_id))
            .count();
        let pending_textures = required_textures
            .iter()
            .filter(|asset_id| self.asset_gpu_pending_textures.contains_key(asset_id))
            .count();
        let missing_textures = required_textures
            .len()
            .saturating_sub(ready_textures.saturating_add(pending_textures));
        let ready_materials = required_materials
            .iter()
            .filter(|key| self.asset_gpu_materials.contains_key(key))
            .count();

        SceneGpuWarmupStatus {
            streaming_entities: self.frame_plan.streaming_entities.len(),
            material_entities,
            required_textures: required_textures.len(),
            ready_textures,
            pending_textures,
            missing_textures,
            required_materials: required_materials.len(),
            ready_materials,
        }
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

        let render_perf_start = std::time::Instant::now();
        let mut render_perf_mark = render_perf_start;

        let width = width.max(1);
        let height = height.max(1);
        let aspect = self.refresh_streaming_plan(width, height);
        let perf_visibility_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        render_perf_mark = std::time::Instant::now();

        self.sync_static_asset_gpu()?;
        let perf_geometry_sync_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        render_perf_mark = std::time::Instant::now();
        self.sync_static_asset_material_gpu()?;
        let perf_material_sync_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        let perf_gpu_sync_ms = perf_geometry_sync_ms + perf_material_sync_ms;
        render_perf_mark = std::time::Instant::now();
        let required_vertices = self.vertex_count();
        let required_shadow_vertices = self.shadow_vertex_count();
        self.ensure_geometry_buffer_capacity(required_vertices, required_shadow_vertices)?;
        let mut gpu = self
            .gpu
            .ok_or_else(|| "3D scene GPU resources disappeared".to_owned())?;
        let vertex_data = self.build_cube_vertices(aspect);
        let shadow_vertex_data = self.build_shadow_vertices();
        let flare_vertex_data = self.build_lens_flare_vertices(aspect);
        let flare_vertex_count =
            u32::try_from(flare_vertex_data.len() / FLARE_FLOATS_PER_VERTEX).unwrap_or(0);
        self.sync_particle_material_gpu()?;
        let (particle_vertex_data, particle_batches) = self.build_particle_vertices();
        let (frame_uniform, shadow_enabled) =
            self.scene_frame_uniform(aspect, gpu.shadow_resolution);
        let perf_geometry_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        render_perf_mark = std::time::Instant::now();

        let render = RenderClient::new();
        self.sync_mass_instance_gpu(&render, aspect)?;
        self.sync_atmospheric_cloud_gpu_residency(&render)?;
        self.ensure_volumetric_cloud_targets(&render, width, height)?;

        let frame_index = self.frame_index;
        let frame_state = render.begin_frame_acquire(self.clear_color, frame_index)?;
        if frame_state.frames_in_flight != SCENE_FRAME_SLOTS {
            if frame_state.active {
                render.abort_frame();
            }
            return Err(format!(
                "renderer frame-ring mismatch scene={} renderer={}",
                SCENE_FRAME_SLOTS, frame_state.frames_in_flight
            ));
        }
        if frame_state.frame_slot >= SCENE_FRAME_SLOTS {
            if frame_state.active {
                render.abort_frame();
            }
            return Err(format!(
                "renderer returned invalid frame slot {} for {} slots",
                frame_state.frame_slot, SCENE_FRAME_SLOTS
            ));
        }
        if !frame_state.active {
            // WSI/back-pressure can defer a presentation attempt. No frame-owned
            // resource is writable until the backend has actually acquired its slot.
            self.frame_index = self.frame_index.wrapping_add(1);
            return Ok(());
        }
        let recording = FrameRecordingScope::new(|| render.abort_frame());
        let frame_slot = frame_state.frame_slot;
        self.sync_dashboard_material_gpu(&render, frame_slot)?;
        self.ensure_particle_vertex_capacity(
            &render,
            frame_slot,
            particle_vertex_data.len() / FLOATS_PER_VERTEX,
        )?;
        // Lighting is frame-owned state. Publish it only after the renderer has
        // successfully acquired a writable frame slot so deferred resolve cannot
        // consume stale/default ambient or local-light packets.
        let renderer_lighting = self.renderer_lighting_environment();
        let renderer_local_lights = self.renderer_local_lights();
        render.set_frame_lighting(renderer_lighting)?;
        render.set_frame_lights(&renderer_local_lights)?;
        gpu = self.sync_skinned_vertex_gpu(&render, frame_slot)?;
        self.sync_gpu_instance_table(&render, gpu, frame_slot)?;

        let perf_residency_sync_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        render_perf_mark = std::time::Instant::now();

        if !vertex_data.is_empty() {
            render.write_buffer_f32(gpu.vertex_buffers[frame_slot], 0, &vertex_data)?;
        }
        if !shadow_vertex_data.is_empty() {
            render.write_buffer_f32(
                gpu.shadow_vertex_buffers[frame_slot],
                0,
                &shadow_vertex_data,
            )?;
        }
        if flare_vertex_count > 0 {
            render.write_buffer_f32(gpu.flare_vertex_buffers[frame_slot], 0, &flare_vertex_data)?;
        }
        if !particle_vertex_data.is_empty() {
            render.write_buffer_f32(
                gpu.particle_vertex_buffers[frame_slot],
                0,
                &particle_vertex_data,
            )?;
        }
        render.write_buffer_f32(gpu.frame_uniforms[frame_slot], 0, &frame_uniform)?;
        self.upload_weather_uniforms(&render, frame_slot, aspect)?;
        if let Some(gpu_sky) = self.gpu_sky {
            let sky_frame = self.sky_frame_uniform(width as f32 / height as f32);
            render.write_buffer_f32(gpu_sky.camera_uniforms[frame_slot], 0, &sky_frame)?;
        }
        if let Some(gpu_clouds) = self.gpu_atmospheric_clouds.as_ref() {
            let cloud_frames = self.atmospheric_cloud_uniforms(width, height);
            if cloud_frames.len() != gpu_clouds.layers.len() {
                return Err(format!(
                    "atmospheric cloud CPU/GPU layer count mismatch cpu={} gpu={}",
                    cloud_frames.len(),
                    gpu_clouds.layers.len()
                ));
            }
            for (frame, layer) in cloud_frames.iter().zip(gpu_clouds.layers.iter()) {
                if let Some(layer) = layer {
                    render.write_buffer_f32(layer.uniform_buffers[frame_slot], 0, frame)?;
                }
            }
            let depth_frame = self.atmospheric_soft_depth_uniform(aspect);
            render.write_buffer_f32(gpu_clouds.soft_depth_uniforms[frame_slot], 0, &depth_frame)?;
        }

        if let Some(volume) = self.gpu_volumetric_clouds.as_ref() {
            if let Some(targets) = volume.targets.as_ref() {
                let frame = self.volumetric_cloud_uniform(
                    width,
                    height,
                    targets.width,
                    targets.height,
                    volume.previous_view_projection,
                    volume.history_valid,
                )?;
                render.write_buffer_f32(volume.uniform_buffers[frame_slot], 0, &frame)?;
                let depth_frame = self.atmospheric_soft_depth_uniform(aspect);
                render.write_buffer_f32(
                    volume.depth_uniform_buffers[frame_slot],
                    0,
                    &depth_frame,
                )?;
            }
        }

        let perf_uploads_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        render_perf_mark = std::time::Instant::now();

        let previous_hiz_camera_compatible = self
            .last_render_camera
            .as_ref()
            .is_some_and(|previous| previous_hiz_camera_compatible(previous, &self.camera));
        self.render_frame_inner(
            &render,
            gpu,
            width,
            height,
            frame_index,
            frame_slot,
            shadow_enabled,
            flare_vertex_count,
            &particle_batches,
            previous_hiz_camera_compatible,
            overlay,
        )?;
        recording.finish();

        let perf_inner_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        let perf_total_ms = render_perf_start.elapsed().as_secs_f64() * 1000.0;
        if perf_total_ms >= 25.0 || frame_index % 60 == 0 {
            host_runtime::info(
                "newviso.perf.render",
                format!(
                    "frame={} total_ms={:.2} visibility_ms={:.2} gpu_sync_ms={:.2} geometry_sync_ms={:.2} material_sync_ms={:.2} geometry_ms={:.2} residency_sync_ms={:.2} uploads_ms={:.2} inner_ms={:.2} visible={} resident={} spatial_candidates={} instance_batches={} instances={} hiz_draws={} opaque_groups={} direct_opaque={} alpha_draws={} mass_draws={} mass_instances={} mass_chunks={} graph_passes={} graph_skipped={} graph_cpu_ms={:.3}",
                    frame_index,
                    perf_total_ms,
                    perf_visibility_ms,
                    perf_gpu_sync_ms,
                    perf_geometry_sync_ms,
                    perf_material_sync_ms,
                    perf_geometry_ms,
                    perf_residency_sync_ms,
                    perf_uploads_ms,
                    perf_inner_ms,
                    self.frame_plan.visible_count,
                    self.frame_plan.resident_count,
                    self.frame_plan.spatial_candidate_count,
                    self.last_submission_stats.instance_batches,
                    self.last_submission_stats.instance_count,
                    self.last_submission_stats.hiz_draws,
                    self.last_submission_stats.opaque_indirect_groups,
                    self.last_submission_stats.direct_opaque_draws,
                    self.last_submission_stats.alpha_draws,
                    self.last_submission_stats.mass_draws,
                    self.last_submission_stats.mass_instances,
                    self.mass_instance_chunk_count(),
                    self.last_submission_stats.graph_executed_passes,
                    self.last_submission_stats.graph_skipped_passes,
                    self.last_submission_stats.graph_cpu_record_ms
                ),
            );
        }

        self.last_render_camera = Some(self.camera.clone());
        self.frame_index = self.frame_index.wrapping_add(1);
        Ok(())
    }
}

// An acquired frame must be ended or aborted on every exit, including upload,
// lighting, dashboard, and cloud errors before render_frame_inner is called.
struct FrameRecordingScope<F: FnOnce()> {
    abort: Option<F>,
}
impl<F: FnOnce()> FrameRecordingScope<F> {
    fn new(abort: F) -> Self {
        Self { abort: Some(abort) }
    }
    fn finish(mut self) {
        self.abort.take();
    }
}
impl<F: FnOnce()> Drop for FrameRecordingScope<F> {
    fn drop(&mut self) {
        if let Some(abort) = self.abort.take() {
            abort();
        }
    }
}

#[cfg(test)]
mod frame_lifecycle_tests {
    use super::FrameRecordingScope;
    use std::cell::Cell;

    #[test]
    fn upload_error_aborts_acquired_frame_once() {
        let aborted = Cell::new(0);
        let result: Result<(), &str> = (|| {
            let _recording = FrameRecordingScope::new(|| aborted.set(aborted.get() + 1));
            Err::<(), _>("buffer upload failed")?;
            Ok(())
        })();
        assert_eq!(result, Err("buffer upload failed"));
        assert_eq!(aborted.get(), 1);
    }

    #[test]
    fn completed_frame_is_not_aborted() {
        let aborted = Cell::new(0);
        let recording = FrameRecordingScope::new(|| aborted.set(aborted.get() + 1));
        recording.finish();
        assert_eq!(aborted.get(), 0);
    }
}
