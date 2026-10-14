//! Offscreen depth preparation and volumetric cloud history.
use super::*;
impl Scene3dRuntime {
    pub(super) fn record_cloud_prepasses(&mut self,render:&RenderClient,gpu:GpuScene,
        width:u32,height:u32,frame_index:u64,frame_slot:usize,
        hiz_draws:&[(u32,u64)],direct_opaque_fallback:&[DirectAssetDraw])
        ->Result<Option<(u32,u32,u32)>,String>{
        let mut volumetric_composite=None;
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

                self.draw_indexed_terrain_depth(render,frame_slot)?;
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
                    draw_direct_draws(render,direct_opaque_fallback,None,Some(gpu.asset_vertex_buffer))?;
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
            self.draw_indexed_terrain_depth(render,frame_slot)?;

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
                draw_direct_draws(render,direct_opaque_fallback,None,Some(gpu.asset_vertex_buffer))?;
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

        Ok(volumetric_composite)
    }
}
