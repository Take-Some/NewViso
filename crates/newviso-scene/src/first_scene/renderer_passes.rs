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

        let mut volumetric_composite: Option<(u32, u32, u32)> = None;
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

        if shadow_enabled && (self.shadow_vertex_count() > 0 || !instance_batches.is_empty()) {
            render.begin_render_target(
                gpu.shadow_render_target,
                Some([1.0, 1.0, 1.0, 1.0]),
                Some(1.0),
            )?;
            render.set_viewport(gpu.shadow_resolution, gpu.shadow_resolution)?;
            render.set_scissor(gpu.shadow_resolution, gpu.shadow_resolution)?;
            render.set_pipeline(gpu.shadow_pipeline)?;
            render.set_bind_group(0, gpu.shadow_bind_groups[frame_slot])?;
            render.set_bind_group(1, gpu.default_material_bind_group)?;

            let dynamic_shadow_vertices = self.shadow_vertex_count();
            if dynamic_shadow_vertices > 0 {
                render.set_vertex_buffer(0, gpu.shadow_vertex_buffers[frame_slot], 0)?;
                render.draw(dynamic_shadow_vertices)?;
            }

            if !instance_batches.is_empty() {
                render.set_pipeline(gpu.asset_shadow_pipeline)?;
                render.set_bind_group(0, gpu.shadow_bind_groups[frame_slot])?;
                render.set_vertex_buffer(1, gpu.asset_instance_buffers[frame_slot], 0)?;
                render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;

                let mut bindings = MaterialVertexBindings::new(None, None);
                for batch in &instance_batches {
                    let Some(stable_id) = batch.stable_ids.first() else {
                        continue;
                    };
                    let mesh = self
                        .asset_meshes
                        .get(stable_id)
                        .ok_or_else(|| format!("asset instance {} disappeared", stable_id))?;
                    debug_assert_eq!(mesh.model_id.0, batch.model_id);
                    if mesh.main_view_only {
                        continue;
                    }
                    let (vertex_buffer, vertex_offset) =
                        asset_mesh_vertex_binding(mesh, gpu, frame_slot)?;
                    bindings.bind_vertex(render, vertex_buffer)?;

                    if mesh.local_draw_ranges.is_empty() {
                        bindings.bind_material(render, gpu.default_material_bind_group)?;
                        render.draw_indexed_range_instanced(
                            mesh.vertex_count,
                            mesh.first_vertex,
                            vertex_offset,
                            batch.instance_count,
                            batch.first_instance,
                        )?;
                        continue;
                    }

                    for range in mesh.local_draw_ranges.iter() {
                        let material_group = match range.material_slot {
                            Some(slot) => {
                                let Some(material) =
                                    self.asset_gpu_materials.get(&(mesh.model_id.0, slot))
                                else {
                                    // Preserve cutout/alpha-tested shadow
                                    // semantics: an authored slot without a
                                    // resident material is not a white material.
                                    continue;
                                };
                                material.bind_group
                            }
                            None => gpu.default_material_bind_group,
                        };
                        bindings.bind_material(render, material_group)?;
                        render.draw_indexed_range_instanced(
                            range.vertex_count,
                            mesh.first_vertex.saturating_add(range.first_vertex),
                            vertex_offset,
                            batch.instance_count,
                            batch.first_instance,
                        )?;
                    }
                }
            }
            render.end_render_target()?;
        }

        // Build a low-resolution linear scene-depth proxy from the current
        // camera before drawing CloudHat geometry. This is provider-neutral and
        // gives the cloud shader a sampled opaque-scene distance for soft
        // intersection, while the main framebuffer keeps normal hardware depth.
        if let Some(clouds) = self.gpu_atmospheric_clouds.as_ref() {
            let has_resident_cloud = self
                .atmospheric_cloud_runtime
                .iter()
                .any(|runtime| runtime.resident);
            if has_resident_cloud {
                let clear_linear_depth = self.camera.far.max(1.0) * 2.0;
                render.begin_render_target(
                    clouds.soft_depth_render_target,
                    Some([clear_linear_depth, 0.0, 0.0, 0.0]),
                    Some(1.0),
                )?;
                render.set_viewport(clouds.soft_depth_resolution, clouds.soft_depth_resolution)?;
                render.set_scissor(clouds.soft_depth_resolution, clouds.soft_depth_resolution)?;
                render.set_pipeline(clouds.soft_depth_pipeline)?;
                render.set_bind_group(0, clouds.soft_depth_bind_groups[frame_slot])?;

                let dynamic_depth_vertices = self.shadow_vertex_count();
                if dynamic_depth_vertices > 0 {
                    render.set_vertex_buffer(0, gpu.shadow_vertex_buffers[frame_slot], 0)?;
                    render.draw(dynamic_depth_vertices)?;
                }

                if !hiz_draws.is_empty() || !direct_opaque_fallback.is_empty() {
                    render.set_pipeline(clouds.soft_depth_instanced_pipeline)?;
                    render.set_bind_group(0, clouds.soft_depth_bind_groups[frame_slot])?;
                    render.set_vertex_buffer(0, gpu.asset_vertex_buffer, 0)?;
                    render.set_vertex_buffer(1, gpu.asset_instance_buffers[frame_slot], 0)?;
                    render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;

                    // Preserve alpha-test coverage while retaining material multi-draw batches.
                    draw_material_indirect_runs(
                        render,
                        gpu.visibility_indirect_buffers[frame_slot],
                        &hiz_draws,
                    )?;
                    let mut bindings =
                        MaterialVertexBindings::new(None, Some(gpu.asset_vertex_buffer));
                    for draw in &direct_opaque_fallback {
                        bindings.bind(render, draw.material_group, draw.vertex_buffer)?;
                        render.draw_indexed_range_instanced(
                            draw.index_count,
                            draw.first_index,
                            draw.vertex_offset,
                            draw.instance_count,
                            draw.first_instance,
                        )?;
                    }
                }
                render.end_render_target()?;
            }
        }

        if let Some(pass) = self.gpu_volumetric_clouds.as_ref().and_then(|volume| {
            volume.targets.as_ref().map(|targets| {
                let write_index = (frame_index as usize) & 1;
                (
                    targets.width,
                    targets.height,
                    targets.scene_depth_target,
                    targets.raw_target,
                    targets.history_targets[write_index],
                    volume.depth_pipeline,
                    volume.depth_instanced_pipeline,
                    volume.depth_bind_groups[frame_slot],
                    volume.raymarch_pipeline,
                    targets.raymarch_bind_groups[frame_slot],
                    volume.temporal_pipeline,
                    targets.temporal_bind_groups[frame_slot][write_index],
                    volume.composite_pipeline,
                    targets.composite_bind_groups[write_index],
                    volume.fullscreen_vertex_buffer,
                )
            })
        }) {
            let (
                cloud_width,
                cloud_height,
                depth_target,
                raw_target,
                history_target,
                depth_pipeline,
                depth_instanced_pipeline,
                depth_bind_group,
                raymarch_pipeline,
                raymarch_bind_group,
                temporal_pipeline,
                temporal_bind_group,
                composite_pipeline,
                composite_bind_group,
                fullscreen_vertex_buffer,
            ) = pass;

            let clear_linear_depth = self
                .sky
                .as_ref()
                .map(|sky| sky.volumetric_clouds.max_distance)
                .unwrap_or(self.camera.far)
                .max(self.camera.far)
                * 2.0;

            // Low-resolution opaque scene distance. The raymarch terminates at
            // this distance, so mountains/buildings cut the volume correctly.
            render.begin_render_target(
                depth_target,
                Some([clear_linear_depth, 0.0, 0.0, 0.0]),
                Some(1.0),
            )?;
            render.set_viewport(cloud_width, cloud_height)?;
            render.set_scissor(cloud_width, cloud_height)?;
            render.set_pipeline(depth_pipeline)?;
            render.set_bind_group(0, depth_bind_group)?;

            let dynamic_depth_vertices = self.shadow_vertex_count();
            if dynamic_depth_vertices > 0 {
                render.set_vertex_buffer(0, gpu.shadow_vertex_buffers[frame_slot], 0)?;
                render.draw(dynamic_depth_vertices)?;
            }

            if !hiz_draws.is_empty() || !direct_opaque_fallback.is_empty() {
                render.set_pipeline(depth_instanced_pipeline)?;
                render.set_bind_group(0, depth_bind_group)?;
                render.set_vertex_buffer(0, gpu.asset_vertex_buffer, 0)?;
                render.set_vertex_buffer(1, gpu.asset_instance_buffers[frame_slot], 0)?;
                render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;

                draw_material_indirect_runs(
                    render,
                    gpu.visibility_indirect_buffers[frame_slot],
                    &hiz_draws,
                )?;
                let mut bindings = MaterialVertexBindings::new(None, Some(gpu.asset_vertex_buffer));
                for draw in &direct_opaque_fallback {
                    bindings.bind(render, draw.material_group, draw.vertex_buffer)?;
                    render.draw_indexed_range_instanced(
                        draw.index_count,
                        draw.first_index,
                        draw.vertex_offset,
                        draw.instance_count,
                        draw.first_instance,
                    )?;
                }
            }
            render.end_render_target()?;

            // True participating-medium integration through the world-space
            // altitude slab.
            render.begin_render_target(raw_target, Some([0.0, 0.0, 0.0, 0.0]), None)?;
            render.set_viewport(cloud_width, cloud_height)?;
            render.set_scissor(cloud_width, cloud_height)?;
            render.set_pipeline(raymarch_pipeline)?;
            render.set_bind_group(0, raymarch_bind_group)?;
            render.set_vertex_buffer(0, fullscreen_vertex_buffer, 0)?;
            render.draw(3)?;
            render.end_render_target()?;

            // Reproject previous history into the current camera and clamp it
            // to the current-frame neighborhood before accumulation.
            render.begin_render_target(history_target, Some([0.0, 0.0, 0.0, 0.0]), None)?;
            render.set_viewport(cloud_width, cloud_height)?;
            render.set_scissor(cloud_width, cloud_height)?;
            render.set_pipeline(temporal_pipeline)?;
            render.set_bind_group(0, temporal_bind_group)?;
            render.set_vertex_buffer(0, fullscreen_vertex_buffer, 0)?;
            render.draw(3)?;
            render.end_render_target()?;

            volumetric_composite = Some((
                composite_pipeline,
                composite_bind_group,
                fullscreen_vertex_buffer,
            ));

            let current_view_projection =
                camera_view_projection(&self.camera, width as f32 / height.max(1) as f32);
            if let Some(volume) = self.gpu_volumetric_clouds.as_mut() {
                volume.previous_view_projection = current_view_projection;
                volume.history_valid = true;
            }
        }

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
        render.set_pipeline(gpu.gbuffer_pipeline)?;
        render.set_bind_group(0, gpu.bind_groups[frame_slot])?;
        render.set_bind_group(1, gpu.default_material_bind_group)?;
        render.set_bind_group(2, weather_material_group)?;
        let dynamic_vertices = self.vertex_count();
        if dynamic_vertices > 0 {
            render.set_vertex_buffer(0, gpu.vertex_buffers[frame_slot], 0)?;
            render.draw(dynamic_vertices)?;
        }

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
            let mut bindings = MaterialVertexBindings::new(
                hiz_draws.last().map(|draw| draw.0),
                Some(gpu.asset_vertex_buffer),
            );
            for draw in direct_opaque_fallback {
                bindings.bind(render, draw.material_group, draw.vertex_buffer)?;
                render.draw_indexed_range_instanced(
                    draw.index_count,
                    draw.first_index,
                    draw.vertex_offset,
                    draw.instance_count,
                    draw.first_instance,
                )?;
            }
        }

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

        render.set_draw_list_kind(None)?;
        render.set_render_phase(None)?;
        let graph_report = render.submit_render_graph(main_deferred_hdr_render_graph(
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
        ))?;
        self.last_submission_stats.graph_executed_passes = graph_report.executed_passes;
        self.last_submission_stats.graph_skipped_passes = graph_report.skipped_passes;
        self.last_submission_stats.graph_cpu_record_ms = graph_report.cpu_record_ms;

        overlay()?;
        render.end_frame()
    }
}
