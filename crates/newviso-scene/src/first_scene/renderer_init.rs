use super::*;

impl Scene3dRuntime {
    pub fn initialize_renderer(&mut self) -> Result<(), String> {
        self.validate_render_policy()?;
        let render = RenderClient::new();
        // One authoritative renderer-owned residency scheduler. Deferred
        // texture work is queued by scene streaming and admitted here, at
        // BeginFrame, under one bounded budget instead of multiple pumps.
        render.set_work_budget(4 * 1024 * 1024, 2, 1, 1.25)?;

        if self.sky.is_some() {
            self.initialize_sky_renderer(&render)?;
        }
        // Weather owns a persistent global material bind group even when the
        // optional Shared GTA closure is unavailable. That keeps scene shader
        // ABI stable and makes wetness a renderer concern rather than a
        // project/material permutation.
        self.initialize_weather_gpu_fx_renderer(&render)?;
        let weather_material_bind_group_layout = self.weather_material_bind_group_layout()?;

        let vertex_capacity = self.vertex_capacity().max(1);
        let shadow_vertex_capacity = self.shadow_vertex_capacity().max(1);
        let vertex_buffers = create_frame_buffer_ring(
            &render,
            "newviso.first_scene.vertices",
            vertex_capacity as u64 * VERTEX_STRIDE,
            "Vertex",
        )?;
        let shadow_vertex_buffers = create_frame_buffer_ring(
            &render,
            "newviso.first_scene.shadow_vertices",
            shadow_vertex_capacity as u64 * VERTEX_STRIDE,
            "Vertex",
        )?;
        let asset_instance_buffers = create_frame_buffer_ring(
            &render,
            "newviso.scene.static_asset_instances",
            DEFAULT_ASSET_INSTANCE_CAPACITY as u64 * INSTANCE_STRIDE,
            "Vertex",
        )?;
        let skinned_vertex_buffers = create_frame_buffer_ring(
            &render,
            "newviso.scene.skinned_vertices",
            DEFAULT_SKINNED_VERTEX_CAPACITY as u64 * VERTEX_STRIDE,
            "Vertex",
        )?;
        let visibility_candidate_buffers = create_frame_buffer_ring(
            &render,
            "newviso.scene.visibility.candidates",
            MAX_HIZ_DRAW_CANDIDATES as u64 * HIZ_CANDIDATE_STRIDE,
            "Storage",
        )?;
        let visibility_indirect_buffers = create_frame_buffer_ring(
            &render,
            "newviso.scene.visibility.indirect",
            MAX_HIZ_DRAW_CANDIDATES as u64 * HIZ_INDIRECT_STRIDE,
            "Indirect",
        )?;
        let frame_uniforms = create_frame_buffer_ring(
            &render,
            "newviso.scene.frame_uniform",
            (SCENE_FRAME_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
            "Uniform",
        )?;
        let flare_vertex_buffers = create_frame_buffer_ring(
            &render,
            "newviso.scene.lens_flare.vertices",
            (self.render_policy.lens_flare_capacity * self.render_policy.flare_element_capacity * 6)
                .max(1) as u64
                * FLARE_VERTEX_STRIDE,
            "Vertex",
        )?;
        let particle_vertex_buffers = create_frame_buffer_ring(
            &render,
            "newviso.scene.particles.vertices",
            (self.render_policy.particle_capacity.max(1) * 6) as u64 * VERTEX_STRIDE,
            "Vertex",
        )?;

        let asset_vertex_capacity = u32::try_from(self.asset_vertex_data.len() / FLOATS_PER_VERTEX)
            .unwrap_or(u32::MAX)
            .max(DEFAULT_ASSET_VERTEX_CAPACITY)
            .checked_next_power_of_two()
            .unwrap_or(u32::MAX);
        let asset_vertex_buffer = render.create_buffer(
            "newviso.scene.static_asset_vertices",
            asset_vertex_capacity as u64 * VERTEX_STRIDE,
            "Vertex",
            "GpuOnly",
        )?;
        let asset_index_buffer = render.create_buffer(
            "newviso.scene.static_asset_indices",
            asset_vertex_capacity as u64 * std::mem::size_of::<u32>() as u64,
            "Index",
            "GpuOnly",
        )?;
        let shadow_render_target = render.create_render_target(
            "newviso.scene.shadow_map",
            self.render_policy.shadow_resolution,
            self.render_policy.shadow_resolution,
            "R32Float",
            Some("Depth32Float"),
        )?;
        let shadow_texture = render.render_target_color_texture(shadow_render_target)?;
        let shadow_sampler = render.create_sampler_clamp_linear("newviso.scene.shadow_sampler")?;

        let bind_group_layout = render.create_bind_group_layout(
            "newviso.scene.frame_bindings",
            &["UniformBuffer", "Texture2D", "Sampler"],
        )?;
        let mut bind_groups = [0u32; SCENE_FRAME_SLOTS];
        for slot in 0..SCENE_FRAME_SLOTS {
            bind_groups[slot] = render.create_bind_group(
                &format!("newviso.scene.frame_bind_group.slot.{slot}"),
                bind_group_layout,
                [Some(shadow_texture), None, None, None],
                Some(shadow_sampler),
                Some((
                    frame_uniforms[slot],
                    0,
                    (SCENE_FRAME_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                )),
            )?;
        }

        let material_bind_group_layout = render.create_bind_group_layout(
            "newviso.scene.material_bindings",
            &[
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Sampler",
                "UniformBuffer",
            ],
        )?;
        let one_pixel_mip = [TextureMipUpload {
            level: 0,
            width: 1,
            height: 1,
            offset: 0,
            byte_len: 4,
        }];
        let default_base_color_texture = render.create_texture(
            "newviso.scene.material.default_base_color",
            1,
            1,
            "Rgba8Srgb",
            &one_pixel_mip,
            &[255, 255, 255, 255],
        )?;
        let default_normal_texture = render.create_texture(
            "newviso.scene.material.default_normal",
            1,
            1,
            "Rgba8Unorm",
            &one_pixel_mip,
            &[128, 128, 255, 255],
        )?;
        let default_specular_texture = render.create_texture(
            "newviso.scene.material.default_specular",
            1,
            1,
            "Rgba8Unorm",
            &one_pixel_mip,
            &[255, 255, 255, 255],
        )?;
        let default_emissive_texture = render.create_texture(
            "newviso.scene.material.default_emissive",
            1,
            1,
            "Rgba8Srgb",
            &one_pixel_mip,
            &[0, 0, 0, 255],
        )?;
        let default_environment_texture = render.create_texture(
            "newviso.scene.material.default_environment",
            1,
            1,
            "Rgba8Srgb",
            &one_pixel_mip,
            &[0, 0, 0, 255],
        )?;
        let default_material_uniform = render.create_buffer(
            "newviso.scene.material.default_params",
            (12 * std::mem::size_of::<f32>()) as u64,
            "Uniform",
            "CpuToGpu",
        )?;
        render.write_buffer_f32(
            default_material_uniform,
            0,
            &[1.0, 0.0, 32.0, 0.0, 0.0, 0.5, 64.0, 1.0, 0.0, 0.0, 0.0, 0.0],
        )?;
        let material_sampler =
            render.create_sampler_repeat_linear("newviso.scene.material_sampler")?;
        let default_material_bind_group = render.create_bind_group6(
            "newviso.scene.material.default_bind_group",
            material_bind_group_layout,
            [
                Some(default_base_color_texture),
                Some(default_normal_texture),
                Some(default_specular_texture),
                Some(default_emissive_texture),
                Some(default_environment_texture),
                None,
            ],
            Some(material_sampler),
            Some((
                default_material_uniform,
                0,
                (12 * std::mem::size_of::<f32>()) as u64,
            )),
        )?;

        let shadow_bind_group_layout =
            render.create_bind_group_layout("newviso.scene.shadow_bindings", &["UniformBuffer"])?;
        let mut shadow_bind_groups = [0u32; SCENE_FRAME_SLOTS];
        for slot in 0..SCENE_FRAME_SLOTS {
            shadow_bind_groups[slot] = render.create_bind_group(
                &format!("newviso.scene.shadow_bind_group.slot.{slot}"),
                shadow_bind_group_layout,
                [None, None, None, None],
                None,
                Some((
                    frame_uniforms[slot],
                    0,
                    (SCENE_FRAME_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                )),
            )?;
        }

        let ScenePipelines {
            vertex_shader,
            fragment_shader,
            gbuffer_fragment_shader,
            asset_vertex_shader,
            shadow_vertex_shader,
            shadow_fragment_shader,
            asset_shadow_vertex_shader,
            pipeline,
            gbuffer_pipeline,
            alpha_pipeline,
            particle_additive_pipeline,
            asset_pipeline,
            asset_gbuffer_pipeline,
            asset_alpha_pipeline,
            shadow_pipeline,
            asset_shadow_pipeline,
            flare_vertex_shader,
            flare_fragment_shader,
            flare_pipeline,
        } = create_scene_pipelines(
            &render,
            bind_group_layout,
            material_bind_group_layout,
            weather_material_bind_group_layout,
            shadow_bind_group_layout,
        )?;

        self.gpu = Some(GpuScene {
            vertex_buffers,
            vertex_capacity,
            cube_capacity: self.cubes.len() + self.render_policy.runtime_cube_capacity,
            shadow_vertex_buffers,
            shadow_vertex_capacity,
            asset_vertex_buffer,
            asset_vertex_capacity,
            asset_vertex_count: 0,
            skinned_vertex_buffers,
            skinned_vertex_capacity: DEFAULT_SKINNED_VERTEX_CAPACITY,
            asset_index_buffer,
            asset_index_capacity: asset_vertex_capacity,
            asset_instance_buffers,
            asset_instance_capacity: DEFAULT_ASSET_INSTANCE_CAPACITY,
            visibility_candidate_buffers,
            visibility_indirect_buffers,
            frame_uniforms,
            bind_group_layout,
            bind_groups,
            material_bind_group_layout,
            default_base_color_texture,
            default_normal_texture,
            default_specular_texture,
            default_emissive_texture,
            default_environment_texture,
            default_material_uniform,
            material_sampler,
            default_material_bind_group,
            shadow_bind_group_layout,
            shadow_bind_groups,
            shadow_render_target,
            shadow_sampler,
            vertex_shader,
            fragment_shader,
            gbuffer_fragment_shader,
            pipeline,
            gbuffer_pipeline,
            alpha_pipeline,
            asset_pipeline,
            asset_gbuffer_pipeline,
            asset_alpha_pipeline,
            shadow_vertex_shader,
            shadow_fragment_shader,
            shadow_pipeline,
            asset_shadow_pipeline,
            asset_vertex_shader,
            asset_shadow_vertex_shader,
            shadow_resolution: self.render_policy.shadow_resolution,
            flare_vertex_buffers,
            flare_vertex_shader,
            flare_fragment_shader,
            flare_pipeline,
            particle_vertex_buffers,
            particle_vertex_capacities: [(self.render_policy.particle_capacity.max(1) * 6) as u32;
                SCENE_FRAME_SLOTS],
            particle_additive_pipeline,
        });
        self.invalidate_gpu_instance_upload();

        // Cloud depth pipelines share the scene material layout for alpha coverage.
        if self
            .sky
            .as_ref()
            .is_some_and(|sky| sky.volumetric_clouds.enabled)
        {
            self.initialize_volumetric_cloud_renderer(&render)?;
        }
        if self
            .atmospheric_clouds
            .as_ref()
            .is_some_and(|clouds| clouds.enabled && !clouds.layers.is_empty())
        {
            self.initialize_atmospheric_cloud_renderer(&render)?;
        }

        host_runtime::info(
            "newviso.scene",
            format!(
                "GPU scene ready buffer={} shadow_buffer={} asset_buffer={} instance_buffer={} pipeline={} asset_pipeline={} shadow_pipeline={} flare_pipeline={} shadow_map={}x{} sky={}",
                vertex_buffers[0],
                shadow_vertex_buffers[0],
                asset_vertex_buffer,
                asset_instance_buffers[0],
                pipeline,
                asset_pipeline,
                shadow_pipeline,
                flare_pipeline,
                self.render_policy.shadow_resolution,
                self.render_policy.shadow_resolution,
                self.gpu_sky.is_some()
            ),
        );
        Ok(())
    }
}
