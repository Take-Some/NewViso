use super::*;

impl Scene3dRuntime {
    pub(super) fn render_frame_inner<F>(
        &mut self,
        render: &RenderClient,
        gpu: GpuScene,
        width: u32,
        height: u32,
        frame_index: u64,
        frame_slot: usize,
        shadow_enabled: bool,
        flare_vertex_count: u32,
        particle_batches: &[ParticleDrawBatch],
        previous_hiz_camera_compatible: bool,
        overlay: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        let FrameAssetSubmission {
            instance_batches,
            visible_instance_slots,
            hiz_draws,
            direct_opaque_fallback,
        } = self.build_asset_submission(render, gpu, frame_slot)?;

        if previous_hiz_camera_compatible && !hiz_draws.is_empty() {
            let forward = self.camera.target.sub(self.camera.position).normalized();
            render.set_render_phase(Some("VisibilityCull"))?;
            render.dispatch_visibility_indirect_cull(
                gpu.visibility_candidate_buffers[frame_slot],
                gpu.visibility_indirect_buffers[frame_slot],
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

        self.record_shadow_pass(render,gpu,frame_slot,shadow_enabled,&instance_batches)?;

        let volumetric_composite=self.record_cloud_prepasses(render,gpu,width,height,
            frame_index,frame_slot,&hiz_draws,&direct_opaque_fallback)?;

        let water_weather = self.weather_material_bind_group(frame_slot)?;
        self.prepare_water_optics(
            render, gpu, frame_slot, &instance_batches, particle_batches, water_weather,
        )?;

        // From this point onward, swapchain work is recorded into the backend
        // draw-list arena and executed by the real RenderGraph. Offscreen shadow
        // and cloud preparation above intentionally remains imperative for this
        // migration step.
        render.set_render_phase(Some("ForwardOpaque"))?;
        render.set_draw_list_kind(Some(RenderDrawListKind::OpaqueForward))?;
        render.set_viewport(width, height)?;
        render.set_scissor(width, height)?;

        if let Some(sky) = self.gpu_sky {
            render.set_pipeline(sky.pipeline)?;
            render.set_bind_group(0, sky.bind_groups[frame_slot])?;
            render.set_vertex_buffer(0, sky.vertex_buffer, 0)?;
            render.set_index_buffer(sky.index_buffer, 0, sky.index_format)?;
            render.draw_indexed(sky.index_count)?;
        }

        // Opaque world geometry is a true MRT GBuffer producer. Background stays
        // isolated in ForwardOpaque so sky/cloud atmosphere cannot masquerade as
        // material geometry during the deferred resolve.
        render.set_render_phase(Some("GBuffer"))?;
        render.set_draw_list_kind(Some(RenderDrawListKind::OpaqueForward))?;

        let weather_material_group = self.weather_material_bind_group(frame_slot)?;
        self.record_primitive_gbuffer(render,gpu,frame_slot,weather_material_group)?;

        let (mass_draws, mass_instance_count) = self.draw_mass_instances(
            render,
            gpu,
            frame_slot,
            weather_material_group,
            width as f32 / height.max(1) as f32,
        )?;

        let mut alpha_draws =
            self.build_alpha_submission(gpu, frame_slot, &visible_instance_slots)?;

        let opaque_indirect_groups = material_indirect_runs(&hiz_draws).count();
        self.last_submission_stats = RenderSubmissionStats {
            instance_batches: instance_batches.len(),
            instance_count: instance_batches
                .iter()
                .map(|batch| batch.instance_count as usize)
                .sum(),
            hiz_draws: hiz_draws.len(),
            opaque_indirect_groups,
            direct_opaque_draws: direct_opaque_fallback.len(),
            alpha_draws: alpha_draws.len(),
            mass_draws,
            mass_instances: mass_instance_count,
            graph_executed_passes: 0,
            graph_skipped_passes: 0,
            graph_cpu_record_ms: 0.0,
        };

        if !hiz_draws.is_empty() || !direct_opaque_fallback.is_empty() {
            render.set_pipeline(gpu.asset_gbuffer_pipeline)?;
            render.set_bind_group(0, gpu.bind_groups[frame_slot])?;
            render.set_bind_group(2, weather_material_group)?;
            render.set_vertex_buffer(0, gpu.asset_vertex_buffer, 0)?;
            render.set_vertex_buffer(1, gpu.asset_instance_buffers[frame_slot], 0)?;
            render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;
            draw_material_indirect_runs(
                render,
                gpu.visibility_indirect_buffers[frame_slot],
                &hiz_draws,
            )?;
            // The helper left the last material bound.
            draw_direct_draws(render,&direct_opaque_fallback,
                hiz_draws.last().map(|draw|draw.0),Some(gpu.asset_vertex_buffer))?;
        }

        self.draw_water_surface(render,gpu,frame_slot,weather_material_group)?;
        render.set_render_phase(Some("Transparent"))?;
        render.set_draw_list_kind(Some(RenderDrawListKind::Transparent))?;

        if let Some((pipeline, bind_group, fullscreen_vertex_buffer)) = volumetric_composite {
            render.set_viewport(width, height)?;
            render.set_scissor(width, height)?;
            render.set_pipeline(pipeline)?;
            render.set_bind_group(0, bind_group)?;
            render.set_vertex_buffer(0, fullscreen_vertex_buffer, 0)?;
            render.draw(3)?;
        }

        // Authored atmospheric cloud geometry is deliberately separate from the
        // procedural sky dome. It depth-tests against opaque world geometry,
        // never writes depth, and alpha-blends before ordinary transparent
        // scene surfaces so windows/particles can still composite in front.
        if let Some(clouds) = self.gpu_atmospheric_clouds.as_ref() {
            if !clouds.layers.is_empty() {
                render.set_pipeline(clouds.pipeline)?;
                render.set_bind_group(1, clouds.soft_sample_bind_group)?;
                for (layer, runtime) in clouds
                    .layers
                    .iter()
                    .zip(self.atmospheric_cloud_runtime.iter())
                {
                    if !runtime.resident {
                        continue;
                    }
                    let Some(layer) = layer else {
                        return Err("resident atmospheric cloud has no GPU resources".to_owned());
                    };
                    render.set_bind_group(0, layer.bind_groups[frame_slot])?;
                    render.set_vertex_buffer(0, layer.vertex_buffer, 0)?;
                    render.set_index_buffer(layer.index_buffer, 0, layer.index_format)?;
                    render.draw_indexed(layer.index_count)?;
                }
            }
        }

        // Execute authored GTA WeatherGpuFx after cloud volumes and before
        // ordinary transparent scene materials. The pass depth-tests against
        // opaque geometry while never modifying scene depth.
        self.draw_weather_world_fx(render, frame_slot)?;

        if !alpha_draws.is_empty() {
            alpha_draws.sort_by(|a, b| {
                b.distance_sq
                    .partial_cmp(&a.distance_sq)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            render.set_pipeline(gpu.asset_alpha_pipeline)?;
            render.set_bind_group(0, gpu.bind_groups[frame_slot])?;
            render.set_bind_group(2, weather_material_group)?;
            render.set_vertex_buffer(1, gpu.asset_instance_buffers[frame_slot], 0)?;
            render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;
            let mut bindings = MaterialVertexBindings::new(None, None);
            for draw in alpha_draws {
                bindings.bind(render, draw.material_group, draw.vertex_buffer)?;
                render.draw_indexed_range_instanced(
                    draw.index_count,
                    draw.first_index,
                    draw.vertex_offset,
                    1,
                    draw.instance_index,
                )?;
            }
        }

        for batch in particle_batches {
            let material = batch
                .material
                .as_ref()
                .and_then(|key| self.particle_gpu_materials.get(key))
                .map_or(gpu.default_material_bind_group, |material| {
                    material.bind_group
                });
            render.set_pipeline(if batch.blend == SceneParticleBlend::Additive {
                gpu.particle_additive_pipeline
            } else {
                gpu.alpha_pipeline
            })?;
            render.set_bind_group(0, gpu.bind_groups[frame_slot])?;
            render.set_bind_group(1, material)?;
            render.set_bind_group(2, weather_material_group)?;
            render.set_vertex_buffer(
                0,
                gpu.particle_vertex_buffers[frame_slot],
                batch.first_vertex as u64 * VERTEX_STRIDE,
            )?;
            render.draw(batch.vertex_count)?;
        }

        if flare_vertex_count > 0 {
            render.set_pipeline(gpu.flare_pipeline)?;
            render.set_vertex_buffer(0, gpu.flare_vertex_buffers[frame_slot], 0)?;
            render.draw(flare_vertex_count)?;
        }

        // Lens rain/water is deliberately the final 3D transparent layer.
        // UI is submitted afterwards and therefore remains optically clean.
        self.draw_weather_lens_fx(render, frame_slot)?;
        self.draw_underwater(render, frame_slot)?;

        render.set_draw_list_kind(None)?;
        render.set_render_phase(None)?;
        let mut graph = main_deferred_hdr_render_graph(
            frame_index,
            width,
            height,
            FrameCameraContext {
                position_ws: [
                    self.camera.position.x,
                    self.camera.position.y,
                    self.camera.position.z,
                ],
                forward_ws: {
                    let graph_forward = self.camera.target.sub(self.camera.position).normalized();
                    [graph_forward.x, graph_forward.y, graph_forward.z]
                },
                up_ws: [self.camera.up.x, self.camera.up.y, self.camera.up.z],
                fov_y: self.camera.fov_y_degrees.to_radians(),
                near: self.camera.near,
                far: self.camera.far,
            },
        );
        self.attach_water_graph(&mut graph);
        let graph_report = render.submit_render_graph(graph)?;
        self.last_submission_stats.graph_executed_passes = graph_report.executed_passes;
        self.last_submission_stats.graph_skipped_passes = graph_report.skipped_passes;
        self.last_submission_stats.graph_cpu_record_ms = graph_report.cpu_record_ms;

        overlay()?;
        render.end_frame()
    }
}
