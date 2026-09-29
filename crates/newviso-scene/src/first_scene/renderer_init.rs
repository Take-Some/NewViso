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
        let mut vertex_buffers = [0u32; SCENE_FRAME_SLOTS];
        let mut shadow_vertex_buffers = [0u32; SCENE_FRAME_SLOTS];
        let mut asset_instance_buffers = [0u32; SCENE_FRAME_SLOTS];
        let mut skinned_vertex_buffers = [0u32; SCENE_FRAME_SLOTS];
        let mut visibility_candidate_buffers = [0u32; SCENE_FRAME_SLOTS];
        let mut visibility_indirect_buffers = [0u32; SCENE_FRAME_SLOTS];
        let mut frame_uniforms = [0u32; SCENE_FRAME_SLOTS];
        let mut flare_vertex_buffers = [0u32; SCENE_FRAME_SLOTS];
        let mut particle_vertex_buffers = [0u32; SCENE_FRAME_SLOTS];

        for slot in 0..SCENE_FRAME_SLOTS {
            vertex_buffers[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.first_scene.vertices.slot.{slot}"),
                vertex_capacity as u64 * VERTEX_STRIDE,
                "Vertex",
                "CpuToGpu",
            )?;
            shadow_vertex_buffers[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.first_scene.shadow_vertices.slot.{slot}"),
                shadow_vertex_capacity as u64 * VERTEX_STRIDE,
                "Vertex",
                "CpuToGpu",
            )?;
            asset_instance_buffers[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.scene.static_asset_instances.slot.{slot}"),
                DEFAULT_ASSET_INSTANCE_CAPACITY as u64 * INSTANCE_STRIDE,
                "Vertex",
                "CpuToGpu",
            )?;
            skinned_vertex_buffers[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.scene.skinned_vertices.slot.{slot}"),
                DEFAULT_SKINNED_VERTEX_CAPACITY as u64 * VERTEX_STRIDE,
                "Vertex",
                "CpuToGpu",
            )?;
            visibility_candidate_buffers[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.scene.visibility.candidates.slot.{slot}"),
                MAX_HIZ_DRAW_CANDIDATES as u64 * HIZ_CANDIDATE_STRIDE,
                "Storage",
                "CpuToGpu",
            )?;
            visibility_indirect_buffers[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.scene.visibility.indirect.slot.{slot}"),
                MAX_HIZ_DRAW_CANDIDATES as u64 * HIZ_INDIRECT_STRIDE,
                "Indirect",
                "CpuToGpu",
            )?;
            frame_uniforms[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.scene.frame_uniform.slot.{slot}"),
                (SCENE_FRAME_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                "Uniform",
                "CpuToGpu",
            )?;
            flare_vertex_buffers[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.scene.lens_flare.vertices.slot.{slot}"),
                (self.render_policy.lens_flare_capacity
                    * self.render_policy.flare_element_capacity
                    * 6)
                .max(1) as u64
                    * FLARE_VERTEX_STRIDE,
                "Vertex",
                "CpuToGpu",
            )?;
            particle_vertex_buffers[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.scene.particles.vertices.slot.{slot}"),
                (self.render_policy.particle_capacity.max(1) * 6) as u64 * VERTEX_STRIDE,
                "Vertex",
                "CpuToGpu",
            )?;
        }

        let asset_vertex_capacity = u32::try_from(self.asset_vertex_data.len() / FLOATS_PER_VERTEX)
            .unwrap_or(u32::MAX)
            .max(DEFAULT_ASSET_VERTEX_CAPACITY)
            .checked_next_power_of_two()
            .unwrap_or(u32::MAX);
        let asset_vertex_buffer = render.create_buffer(
            "newviso.scene.static_asset_vertices",
            asset_vertex_capacity as u64 * VERTEX_STRIDE,
            "Vertex",
            "CpuToGpu",
        )?;
        let asset_index_buffer = render.create_buffer(
            "newviso.scene.static_asset_indices",
            asset_vertex_capacity as u64 * std::mem::size_of::<u32>() as u64,
            "Index",
            "CpuToGpu",
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

        let vertex_shader = render.create_shader_spirv(
            "newviso.first_scene.vertex",
            ShaderStage::Vertex,
            VERTEX_SHADER,
        )?;
        let fragment_shader = render.create_shader_spirv(
            "newviso.first_scene.fragment",
            ShaderStage::Fragment,
            FRAGMENT_SHADER,
        )?;
        let gbuffer_fragment_shader = render.create_shader_spirv(
            "newviso.first_scene.gbuffer.fragment",
            ShaderStage::Fragment,
            GBUFFER_FRAGMENT_SHADER,
        )?;
        let asset_vertex_shader = render.create_shader_spirv(
            "newviso.scene.asset_instanced.vertex",
            ShaderStage::Vertex,
            ASSET_INSTANCED_VERTEX_SHADER,
        )?;
        let shadow_vertex_shader = render.create_shader_spirv(
            "newviso.scene.shadow.vertex",
            ShaderStage::Vertex,
            SHADOW_VERTEX_SHADER,
        )?;
        let shadow_fragment_shader = render.create_shader_spirv(
            "newviso.scene.shadow.fragment",
            ShaderStage::Fragment,
            SHADOW_FRAGMENT_SHADER,
        )?;
        let asset_shadow_vertex_shader = render.create_shader_spirv(
            "newviso.scene.asset_instanced.shadow.vertex",
            ShaderStage::Vertex,
            ASSET_INSTANCED_SHADOW_VERTEX_SHADER,
        )?;

        let attributes = [
            VertexAttribute {
                location: 0,
                offset: 0,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 1,
                offset: 16,
                format: VertexFormat::Float32x3,
            },
            VertexAttribute {
                location: 2,
                offset: 28,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 3,
                offset: 44,
                format: VertexFormat::Float32x2,
            },
            VertexAttribute {
                location: 4,
                offset: 52,
                format: VertexFormat::Float32x4,
            },
        ];

        let instance_attributes = [
            VertexAttribute {
                location: 5,
                offset: 0,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 6,
                offset: 16,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 7,
                offset: 32,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 8,
                offset: 48,
                format: VertexFormat::Float32x4,
            },
        ];

        let pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.first_scene.pipeline",
            vertex_shader,
            fragment_shader,
            vertex_stride: VERTEX_STRIDE,
            attributes: &attributes,
            topology: "TriangleList",
            bind_group_layouts: &[
                bind_group_layout,
                material_bind_group_layout,
                weather_material_bind_group_layout,
            ],
            color_format: "Rgba16Float",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: true,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.scene.rsc7_material.opaque.weather.v4",
        })?;
        let scene_vertex_layouts = [VertexLayoutDesc {
            stride: VERTEX_STRIDE,
            attributes: &attributes,
            step_mode: VertexStepMode::Vertex,
        }];
        let gbuffer_pipeline = render.create_pipeline_mrt_with_layouts(
            GraphicsPipelineDesc {
                label: "newviso.first_scene.gbuffer.pipeline",
                vertex_shader,
                fragment_shader: gbuffer_fragment_shader,
                vertex_stride: VERTEX_STRIDE,
                attributes: &attributes,
                topology: "TriangleList",
                bind_group_layouts: &[
                    bind_group_layout,
                    material_bind_group_layout,
                    weather_material_bind_group_layout,
                ],
                color_format: "Rgba8Unorm",
                depth_format: Some("Depth32Float"),
                depth_test: true,
                depth_write: true,
                depth_compare: "LessOrEqual",
                cull_mode: "None",
                blend_mode: "Opaque",
                cache_key: "newviso.scene.gbuffer.rsc7.weather.v1",
            },
            &scene_vertex_layouts,
            &["Rgba8Unorm", "Rgba16Float", "Rgba8Unorm"],
        )?;
        let alpha_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.first_scene.alpha_pipeline",
            vertex_shader,
            fragment_shader,
            vertex_stride: VERTEX_STRIDE,
            attributes: &attributes,
            topology: "TriangleList",
            bind_group_layouts: &[
                bind_group_layout,
                material_bind_group_layout,
                weather_material_bind_group_layout,
            ],
            color_format: "Rgba16Float",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: false,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Alpha",
            cache_key: "newviso.scene.rsc7_material.alpha.weather.v4",
        })?;

        let particle_additive_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.scene.particles.additive_pipeline",
            vertex_shader,
            fragment_shader,
            vertex_stride: VERTEX_STRIDE,
            attributes: &attributes,
            topology: "TriangleList",
            bind_group_layouts: &[
                bind_group_layout,
                material_bind_group_layout,
                weather_material_bind_group_layout,
            ],
            color_format: "Rgba16Float",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: false,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Additive",
            cache_key: "newviso.scene.particles.additive.weather.v2",
        })?;

        let asset_vertex_layouts = [
            VertexLayoutDesc {
                stride: VERTEX_STRIDE,
                attributes: &attributes,
                step_mode: VertexStepMode::Vertex,
            },
            VertexLayoutDesc {
                stride: INSTANCE_STRIDE,
                attributes: &instance_attributes,
                step_mode: VertexStepMode::Instance,
            },
        ];
        let asset_pipeline = render.create_pipeline_with_layouts(
            GraphicsPipelineDesc {
                label: "newviso.scene.asset_instanced.pipeline",
                vertex_shader: asset_vertex_shader,
                fragment_shader,
                vertex_stride: VERTEX_STRIDE,
                attributes: &attributes,
                topology: "TriangleList",
                bind_group_layouts: &[
                    bind_group_layout,
                    material_bind_group_layout,
                    weather_material_bind_group_layout,
                ],
                color_format: "Rgba16Float",
                depth_format: Some("Depth32Float"),
                depth_test: true,
                depth_write: true,
                depth_compare: "LessOrEqual",
                cull_mode: "None",
                blend_mode: "Opaque",
                cache_key: "newviso.scene.asset_instanced.opaque.weather.v2",
            },
            &asset_vertex_layouts,
        )?;
        let asset_gbuffer_pipeline = render.create_pipeline_mrt_with_layouts(
            GraphicsPipelineDesc {
                label: "newviso.scene.asset_instanced.gbuffer.pipeline",
                vertex_shader: asset_vertex_shader,
                fragment_shader: gbuffer_fragment_shader,
                vertex_stride: VERTEX_STRIDE,
                attributes: &attributes,
                topology: "TriangleList",
                bind_group_layouts: &[
                    bind_group_layout,
                    material_bind_group_layout,
                    weather_material_bind_group_layout,
                ],
                color_format: "Rgba8Unorm",
                depth_format: Some("Depth32Float"),
                depth_test: true,
                depth_write: true,
                depth_compare: "LessOrEqual",
                cull_mode: "None",
                blend_mode: "Opaque",
                cache_key: "newviso.scene.asset_instanced.gbuffer.v1",
            },
            &asset_vertex_layouts,
            &["Rgba8Unorm", "Rgba16Float", "Rgba8Unorm"],
        )?;
        let asset_alpha_pipeline = render.create_pipeline_with_layouts(
            GraphicsPipelineDesc {
                label: "newviso.scene.asset_instanced.alpha_pipeline",
                vertex_shader: asset_vertex_shader,
                fragment_shader,
                vertex_stride: VERTEX_STRIDE,
                attributes: &attributes,
                topology: "TriangleList",
                bind_group_layouts: &[
                    bind_group_layout,
                    material_bind_group_layout,
                    weather_material_bind_group_layout,
                ],
                color_format: "Rgba16Float",
                depth_format: Some("Depth32Float"),
                depth_test: true,
                depth_write: false,
                depth_compare: "LessOrEqual",
                cull_mode: "None",
                blend_mode: "Alpha",
                cache_key: "newviso.scene.asset_instanced.alpha.weather.v2",
            },
            &asset_vertex_layouts,
        )?;

        let shadow_attributes = [
            VertexAttribute {
                location: 0,
                offset: 0,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 3,
                offset: 44,
                format: VertexFormat::Float32x2,
            },
        ];
        let shadow_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.scene.shadow.pipeline",
            vertex_shader: shadow_vertex_shader,
            fragment_shader: shadow_fragment_shader,
            vertex_stride: VERTEX_STRIDE,
            attributes: &shadow_attributes,
            topology: "TriangleList",
            bind_group_layouts: &[shadow_bind_group_layout, material_bind_group_layout],
            color_format: "R32Float",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: true,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.scene.shadow.material_alpha.v2",
        })?;

        let asset_shadow_layouts = [
            VertexLayoutDesc {
                stride: VERTEX_STRIDE,
                attributes: &shadow_attributes,
                step_mode: VertexStepMode::Vertex,
            },
            VertexLayoutDesc {
                stride: INSTANCE_STRIDE,
                attributes: &instance_attributes,
                step_mode: VertexStepMode::Instance,
            },
        ];
        let asset_shadow_pipeline = render.create_pipeline_with_layouts(
            GraphicsPipelineDesc {
                label: "newviso.scene.asset_instanced.shadow.pipeline",
                vertex_shader: asset_shadow_vertex_shader,
                fragment_shader: shadow_fragment_shader,
                vertex_stride: VERTEX_STRIDE,
                attributes: &shadow_attributes,
                topology: "TriangleList",
                bind_group_layouts: &[shadow_bind_group_layout, material_bind_group_layout],
                color_format: "R32Float",
                depth_format: Some("Depth32Float"),
                depth_test: true,
                depth_write: true,
                depth_compare: "LessOrEqual",
                cull_mode: "None",
                blend_mode: "Opaque",
                cache_key: "newviso.scene.asset_instanced.shadow.v1",
            },
            &asset_shadow_layouts,
        )?;

        let flare_vertex_shader = render.create_shader_spirv(
            "newviso.scene.lens_flare.vertex",
            ShaderStage::Vertex,
            FLARE_VERTEX_SHADER,
        )?;
        let flare_fragment_shader = render.create_shader_spirv(
            "newviso.scene.lens_flare.fragment",
            ShaderStage::Fragment,
            FLARE_FRAGMENT_SHADER,
        )?;
        let flare_attributes = [
            VertexAttribute {
                location: 0,
                offset: 0,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 1,
                offset: 16,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 2,
                offset: 32,
                format: VertexFormat::Float32x4,
            },
        ];
        let flare_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.scene.lens_flare.pipeline",
            vertex_shader: flare_vertex_shader,
            fragment_shader: flare_fragment_shader,
            vertex_stride: FLARE_VERTEX_STRIDE,
            attributes: &flare_attributes,
            topology: "TriangleList",
            bind_group_layouts: &[],
            color_format: "Rgba16Float",
            depth_format: Some("Depth32Float"),
            depth_test: false,
            depth_write: false,
            depth_compare: "Always",
            cull_mode: "None",
            blend_mode: "Additive",
            cache_key: "newviso.scene.lens_flare.v1",
        })?;

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
    pub(super) fn initialize_sky_renderer(&mut self, render: &RenderClient) -> Result<(), String> {
        let sky = self
            .sky
            .as_ref()
            .ok_or_else(|| "sky resources are not assigned".to_owned())?;
        let mesh = &sky.mesh;
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            return Err("sky runtime mesh is empty".to_owned());
        }

        let sky_center = [
            (mesh.bounds_min[0] + mesh.bounds_max[0]) * 0.5,
            (mesh.bounds_min[1] + mesh.bounds_max[1]) * 0.5,
            (mesh.bounds_min[2] + mesh.bounds_max[2]) * 0.5,
        ];
        let mut static_vertices = Vec::with_capacity(mesh.vertices.len() * SKY_FLOATS_PER_VERTEX);
        for (index, vertex) in mesh.vertices.iter().enumerate() {
            let x = vertex.position[0] - sky_center[0];
            let y = vertex.position[1] - sky_center[1];
            let z = vertex.position[2] - sky_center[2];
            let length = (x * x + y * y + z * z).sqrt();
            if !length.is_finite() || length <= 1.0e-6 {
                return Err(format!("sky dome vertex {index} is at the mesh centre"));
            }
            static_vertices.extend_from_slice(&[
                x / length,
                y / length,
                z / length,
                vertex.uv[0],
                vertex.uv[1],
            ]);
        }

        let index_bytes = match mesh.index_format {
            SkyIndexFormat::U16 => {
                let mut bytes = Vec::with_capacity(mesh.indices.len() * 2);
                for &index in &mesh.indices {
                    let value = u16::try_from(index).map_err(|_| {
                        format!("sky index {index} does not fit declared U16 index format")
                    })?;
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                bytes
            }
            SkyIndexFormat::U32 => {
                let mut bytes = Vec::with_capacity(mesh.indices.len() * 4);
                for &index in &mesh.indices {
                    bytes.extend_from_slice(&index.to_le_bytes());
                }
                bytes
            }
        };

        let vertex_buffer = render.create_buffer(
            "newviso.sky.vertices",
            mesh.vertices.len() as u64 * SKY_VERTEX_STRIDE,
            "Vertex",
            "CpuToGpu",
        )?;
        let index_buffer = render.create_buffer(
            "newviso.sky.indices",
            index_bytes.len() as u64,
            "Index",
            "CpuToGpu",
        )?;
        let mut camera_uniforms = [0u32; SCENE_FRAME_SLOTS];
        for slot in 0..SCENE_FRAME_SLOTS {
            camera_uniforms[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.sky.frame.slot.{slot}"),
                (SKY_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                "Uniform",
                "CpuToGpu",
            )?;
        }

        render.write_buffer_f32(vertex_buffer, 0, &static_vertices)?;
        render.write_buffer(index_buffer, 0, &index_bytes)?;

        let base_noise = upload_sky_texture(render, "newviso.sky.base_noise", &sky.base_noise)?;
        let starfield = upload_sky_texture(render, "newviso.sky.starfield", &sky.starfield)?;
        let detail_noise =
            upload_sky_texture(render, "newviso.sky.detail_noise", &sky.detail_noise)?;
        let billboard_texture = sky
            .billboard_texture
            .as_ref()
            .map(|texture| upload_sky_texture(render, "newviso.sky.billboard", texture))
            .transpose()?;
        let sampler = render.create_sampler_repeat_linear("newviso.sky.sampler")?;
        let bind_group_layout = render.create_bind_group_layout(
            "newviso.sky.bindings",
            &[
                "UniformBuffer",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Sampler",
            ],
        )?;
        let mut bind_groups = [0u32; SCENE_FRAME_SLOTS];
        for slot in 0..SCENE_FRAME_SLOTS {
            bind_groups[slot] = render.create_bind_group(
                &format!("newviso.sky.bind_group.slot.{slot}"),
                bind_group_layout,
                [
                    Some(base_noise),
                    Some(starfield),
                    Some(detail_noise),
                    Some(billboard_texture.unwrap_or(detail_noise)),
                ],
                Some(sampler),
                Some((
                    camera_uniforms[slot],
                    0,
                    (SKY_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                )),
            )?;
        }

        let vertex_shader = render.create_shader_spirv(
            "newviso.sky.vertex",
            ShaderStage::Vertex,
            SKY_VERTEX_SHADER,
        )?;
        let fragment_shader = render.create_shader_spirv(
            "newviso.sky.fragment",
            ShaderStage::Fragment,
            SKY_FRAGMENT_SHADER,
        )?;

        let attributes = [
            VertexAttribute {
                location: 0,
                offset: 0,
                format: VertexFormat::Float32x3,
            },
            VertexAttribute {
                location: 1,
                offset: 12,
                format: VertexFormat::Float32x2,
            },
        ];
        let pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.sky.pipeline",
            vertex_shader,
            fragment_shader,
            vertex_stride: SKY_VERTEX_STRIDE,
            attributes: &attributes,
            topology: "TriangleList",
            bind_group_layouts: &[bind_group_layout],
            color_format: "Rgba16Float",
            depth_format: Some("Depth32Float"),
            depth_test: false,
            depth_write: false,
            depth_compare: "Always",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.sky.semantic.v3",
        })?;

        let index_format = match mesh.index_format {
            SkyIndexFormat::U16 => "U16",
            SkyIndexFormat::U32 => "U32",
        };
        self.gpu_sky = Some(GpuSky {
            vertex_buffer,
            index_buffer,
            base_noise,
            starfield,
            detail_noise,
            billboard_texture,
            sampler,
            bind_group_layout,
            bind_groups,
            camera_uniforms,
            vertex_shader,
            fragment_shader,
            pipeline,
            index_count: mesh.indices.len() as u32,
            index_format,
        });

        host_runtime::info(
            "newviso.scene",
            format!(
                "sky dome GPU ready model='{}' material='{}' vertices={} indices={} textures=[{},{},{}] billboard={}",
                sky.model_name,
                sky.material_name,
                mesh.vertices.len(),
                mesh.indices.len(),
                sky.base_noise.name,
                sky.starfield.name,
                sky.detail_noise.name,
                sky.billboard_texture
                    .as_ref()
                    .map(|texture| texture.name.as_str())
                    .unwrap_or("-")
            ),
        );
        Ok(())
    }
    pub(super) fn initialize_atmospheric_cloud_renderer(
        &mut self,
        render: &RenderClient,
    ) -> Result<(), String> {
        let resources = self
            .atmospheric_clouds
            .as_ref()
            .ok_or_else(|| "atmospheric cloud resources are not assigned".to_owned())?;
        if resources.layers.is_empty() {
            return Ok(());
        }

        let soft_depth_resolution = resources.soft_depth_resolution;
        let soft_depth_render_target = render.create_render_target(
            "newviso.atmospheric_clouds.soft_depth",
            soft_depth_resolution,
            soft_depth_resolution,
            "R32Float",
            Some("Depth32Float"),
        )?;
        let soft_depth_texture = render.render_target_color_texture(soft_depth_render_target)?;
        let mut soft_depth_uniforms = [0u32; SCENE_FRAME_SLOTS];
        for slot in 0..SCENE_FRAME_SLOTS {
            soft_depth_uniforms[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.atmospheric_clouds.soft_depth.uniform.slot.{slot}"),
                (ATMOSPHERIC_SOFT_DEPTH_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                "Uniform",
                "CpuToGpu",
            )?;
        }
        let soft_depth_bind_group_layout = render.create_bind_group_layout(
            "newviso.atmospheric_clouds.soft_depth.bindings",
            &["UniformBuffer"],
        )?;
        let mut soft_depth_bind_groups = [0u32; SCENE_FRAME_SLOTS];
        for slot in 0..SCENE_FRAME_SLOTS {
            soft_depth_bind_groups[slot] = render.create_bind_group(
                &format!("newviso.atmospheric_clouds.soft_depth.bind_group.slot.{slot}"),
                soft_depth_bind_group_layout,
                [None, None, None, None],
                None,
                Some((
                    soft_depth_uniforms[slot],
                    0,
                    (ATMOSPHERIC_SOFT_DEPTH_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                )),
            )?;
        }

        let soft_depth_vertex_shader = render.create_shader_spirv(
            "newviso.atmospheric_clouds.soft_depth.vertex",
            ShaderStage::Vertex,
            ATMOSPHERIC_DEPTH_VERTEX_SHADER,
        )?;
        let soft_depth_instanced_vertex_shader = render.create_shader_spirv(
            "newviso.atmospheric_clouds.soft_depth.instanced.vertex",
            ShaderStage::Vertex,
            ATMOSPHERIC_DEPTH_INSTANCED_VERTEX_SHADER,
        )?;
        let soft_depth_fragment_shader = render.create_shader_spirv(
            "newviso.atmospheric_clouds.soft_depth.fragment",
            ShaderStage::Fragment,
            ATMOSPHERIC_DEPTH_FRAGMENT_SHADER,
        )?;
        let soft_depth_instanced_fragment_shader = render.create_shader_spirv(
            "newviso.atmospheric_clouds.soft_depth.instanced.fragment",
            ShaderStage::Fragment,
            ATMOSPHERIC_DEPTH_INSTANCED_FRAGMENT_SHADER,
        )?;
        let material_bind_group_layout = self
            .gpu
            .as_ref()
            .ok_or("cloud depth requires scene material bindings")?
            .material_bind_group_layout;
        let soft_depth_attributes = [VertexAttribute {
            location: 0,
            offset: 0,
            format: VertexFormat::Float32x4,
        }];
        let soft_depth_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.atmospheric_clouds.soft_depth.pipeline",
            vertex_shader: soft_depth_vertex_shader,
            fragment_shader: soft_depth_fragment_shader,
            vertex_stride: VERTEX_STRIDE,
            attributes: &soft_depth_attributes,
            topology: "TriangleList",
            bind_group_layouts: &[soft_depth_bind_group_layout],
            color_format: "R32Float",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: true,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.atmospheric_clouds.soft_depth.dynamic.v1",
        })?;
        let soft_depth_instance_attributes = [
            VertexAttribute {
                location: 5,
                offset: 0,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 6,
                offset: 16,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 7,
                offset: 32,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 8,
                offset: 48,
                format: VertexFormat::Float32x4,
            },
        ];
        let soft_depth_asset_attributes = [
            VertexAttribute {
                location: 0,
                offset: 0,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 2,
                offset: 28,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 3,
                offset: 44,
                format: VertexFormat::Float32x2,
            },
        ];
        let soft_depth_layouts = [
            VertexLayoutDesc {
                stride: VERTEX_STRIDE,
                attributes: &soft_depth_asset_attributes,
                step_mode: VertexStepMode::Vertex,
            },
            VertexLayoutDesc {
                stride: INSTANCE_STRIDE,
                attributes: &soft_depth_instance_attributes,
                step_mode: VertexStepMode::Instance,
            },
        ];
        let soft_depth_instanced_pipeline = render.create_pipeline_with_layouts(
            GraphicsPipelineDesc {
                label: "newviso.atmospheric_clouds.soft_depth.instanced.pipeline",
                vertex_shader: soft_depth_instanced_vertex_shader,
                fragment_shader: soft_depth_instanced_fragment_shader,
                vertex_stride: VERTEX_STRIDE,
                attributes: &soft_depth_asset_attributes,
                topology: "TriangleList",
                bind_group_layouts: &[soft_depth_bind_group_layout, material_bind_group_layout],
                color_format: "R32Float",
                depth_format: Some("Depth32Float"),
                depth_test: true,
                depth_write: true,
                depth_compare: "LessOrEqual",
                cull_mode: "None",
                blend_mode: "Opaque",
                cache_key: "newviso.atmospheric_clouds.soft_depth.instanced.v2",
            },
            &soft_depth_layouts,
        )?;

        let sampler = render.create_sampler_repeat_linear("newviso.atmospheric_clouds.sampler")?;
        let bind_group_layout = render.create_bind_group_layout(
            "newviso.atmospheric_clouds.bindings",
            &[
                "UniformBuffer",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Sampler",
            ],
        )?;
        let soft_sample_bind_group_layout = render.create_bind_group_layout(
            "newviso.atmospheric_clouds.soft_sample_bindings",
            &["Texture2D", "Sampler"],
        )?;
        let soft_sample_bind_group = render.create_bind_group(
            "newviso.atmospheric_clouds.soft_sample_bind_group",
            soft_sample_bind_group_layout,
            [Some(soft_depth_texture), None, None, None],
            Some(sampler),
            None,
        )?;
        let vertex_shader = render.create_shader_spirv(
            "newviso.atmospheric_clouds.vertex",
            ShaderStage::Vertex,
            ATMOSPHERIC_CLOUD_VERTEX_SHADER,
        )?;
        let fragment_shader = render.create_shader_spirv(
            "newviso.atmospheric_clouds.fragment",
            ShaderStage::Fragment,
            ATMOSPHERIC_CLOUD_FRAGMENT_SHADER,
        )?;
        let attributes = [
            VertexAttribute {
                location: 0,
                offset: 0,
                format: VertexFormat::Float32x3,
            },
            VertexAttribute {
                location: 1,
                offset: 12,
                format: VertexFormat::Float32x3,
            },
            VertexAttribute {
                location: 2,
                offset: 24,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 3,
                offset: 40,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 4,
                offset: 56,
                format: VertexFormat::Float32x2,
            },
        ];
        let pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.atmospheric_clouds.pipeline",
            vertex_shader,
            fragment_shader,
            vertex_stride: ATMOSPHERIC_CLOUD_VERTEX_STRIDE,
            attributes: &attributes,
            topology: "TriangleList",
            bind_group_layouts: &[bind_group_layout, soft_sample_bind_group_layout],
            color_format: "Rgba16Float",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: false,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Alpha",
            cache_key: "newviso.atmospheric_clouds.cloudsps.v2",
        })?;

        let mut texture_cache = BTreeMap::<String, u32>::new();
        for layer in &resources.layers {
            let textures = [
                &layer.textures.density,
                &layer.textures.normal,
                &layer.textures.detail_density,
                &layer.textures.detail_normal,
                &layer.textures.detail_density2,
                &layer.textures.detail_normal2,
            ];
            for (reference, texture) in layer.textures.refs.iter().zip(textures) {
                if texture_cache.contains_key(reference) {
                    continue;
                }
                let handle = upload_sky_texture(
                    render,
                    &format!("newviso.atmospheric_clouds.texture.{}", texture_cache.len()),
                    texture,
                )?;
                texture_cache.insert(reference.clone(), handle);
            }
        }

        let gpu_layers = vec![None; resources.layers.len()];

        host_runtime::info(
            "newviso.scene",
            format!(
                "atmospheric cloud GPU ready authored_layers={} resident_layers=0 unique_textures={} pipeline={} source='CloudsPS' soft_depth={}x{}",
                gpu_layers.len(),
                texture_cache.len(),
                pipeline,
                soft_depth_resolution,
                soft_depth_resolution
            ),
        );
        self.gpu_atmospheric_clouds = Some(GpuAtmosphericClouds {
            sampler,
            bind_group_layout,
            vertex_shader,
            fragment_shader,
            pipeline,
            layers: gpu_layers,
            soft_depth_render_target,
            soft_depth_uniforms,
            soft_depth_bind_group_layout,
            soft_depth_bind_groups,
            soft_sample_bind_group_layout,
            soft_sample_bind_group,
            texture_cache,
            soft_depth_vertex_shader,
            soft_depth_instanced_vertex_shader,
            soft_depth_fragment_shader,
            soft_depth_instanced_fragment_shader,
            soft_depth_pipeline,
            soft_depth_instanced_pipeline,
            soft_depth_resolution,
        });
        Ok(())
    }

    pub(super) fn sync_atmospheric_cloud_gpu_residency(
        &mut self,
        render: &RenderClient,
    ) -> Result<(), String> {
        let Some(resources) = self.atmospheric_clouds.as_ref() else {
            return Ok(());
        };
        let residency = self
            .atmospheric_cloud_runtime
            .iter()
            .map(|runtime| runtime.resident)
            .collect::<Vec<_>>();
        let Some(gpu) = self.gpu_atmospheric_clouds.as_mut() else {
            return Ok(());
        };
        if gpu.layers.len() != resources.layers.len() || residency.len() != resources.layers.len() {
            return Err(format!(
                "atmospheric cloud residency table mismatch resources={} runtime={} gpu={}",
                resources.layers.len(),
                residency.len(),
                gpu.layers.len()
            ));
        }

        for index in 0..resources.layers.len() {
            if residency[index] && gpu.layers[index].is_none() {
                let layer = &resources.layers[index];
                let mut vertex_data =
                    Vec::with_capacity(layer.mesh.vertices.len() * ATMOSPHERIC_CLOUD_VERTEX_FLOATS);
                for vertex in &layer.mesh.vertices {
                    vertex_data.extend_from_slice(&vertex.position);
                    vertex_data.extend_from_slice(&vertex.normal);
                    vertex_data.extend_from_slice(&vertex.tangent);
                    vertex_data.extend_from_slice(&vertex.color);
                    vertex_data.extend_from_slice(&vertex.uv);
                }
                let index_bytes = match layer.mesh.index_format {
                    SkyIndexFormat::U16 => {
                        let mut bytes = Vec::with_capacity(layer.mesh.indices.len() * 2);
                        for &raw_index in &layer.mesh.indices {
                            let value = u16::try_from(raw_index).map_err(|_| {
                                format!(
                                    "atmospheric cloud '{}' index {raw_index} does not fit U16",
                                    layer.desc.id
                                )
                            })?;
                            bytes.extend_from_slice(&value.to_le_bytes());
                        }
                        bytes
                    }
                    SkyIndexFormat::U32 => {
                        let mut bytes = Vec::with_capacity(layer.mesh.indices.len() * 4);
                        for &raw_index in &layer.mesh.indices {
                            bytes.extend_from_slice(&raw_index.to_le_bytes());
                        }
                        bytes
                    }
                };

                let vertex_buffer = render.create_buffer(
                    &format!("newviso.atmospheric_clouds.layer.{index}.vertices"),
                    vertex_data.len() as u64 * std::mem::size_of::<f32>() as u64,
                    "Vertex",
                    "CpuToGpu",
                )?;
                let index_buffer = render.create_buffer(
                    &format!("newviso.atmospheric_clouds.layer.{index}.indices"),
                    index_bytes.len() as u64,
                    "Index",
                    "CpuToGpu",
                )?;
                let mut uniform_buffers = [0u32; SCENE_FRAME_SLOTS];
                for slot in 0..SCENE_FRAME_SLOTS {
                    uniform_buffers[slot] = render.create_frame_buffer(
                        slot,
                        &format!("newviso.atmospheric_clouds.layer.{index}.uniform.slot.{slot}"),
                        (ATMOSPHERIC_CLOUD_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                        "Uniform",
                        "CpuToGpu",
                    )?;
                }
                render.write_buffer_f32(vertex_buffer, 0, &vertex_data)?;
                render.write_buffer(index_buffer, 0, &index_bytes)?;

                let texture_handles = layer
                    .textures
                    .refs
                    .iter()
                    .map(|reference| {
                        gpu.texture_cache.get(reference).copied().ok_or_else(|| {
                            format!(
                                "atmospheric cloud '{}' texture '{}' is absent from GPU cache",
                                layer.desc.id, reference
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let textures: [u32; 6] = texture_handles
                    .try_into()
                    .map_err(|_| "CloudsPS texture closure must contain six textures".to_owned())?;

                let mut bind_groups = [0u32; SCENE_FRAME_SLOTS];
                for slot in 0..SCENE_FRAME_SLOTS {
                    bind_groups[slot] = match render.create_bind_group6(
                        &format!("newviso.atmospheric_clouds.layer.{index}.bind_group.slot.{slot}"),
                        gpu.bind_group_layout,
                        [
                            Some(textures[0]),
                            Some(textures[1]),
                            Some(textures[2]),
                            Some(textures[3]),
                            Some(textures[4]),
                            Some(textures[5]),
                        ],
                        Some(gpu.sampler),
                        Some((
                            uniform_buffers[slot],
                            0,
                            (ATMOSPHERIC_CLOUD_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                        )),
                    ) {
                        Ok(group) => group,
                        Err(error) => {
                            for group in bind_groups.into_iter().filter(|group| *group != 0) {
                                render.destroy_bind_group(group);
                            }
                            for buffer in uniform_buffers {
                                render.destroy_buffer(buffer);
                            }
                            render.destroy_buffer(index_buffer);
                            render.destroy_buffer(vertex_buffer);
                            return Err(error);
                        }
                    };
                }

                gpu.layers[index] = Some(GpuAtmosphericCloudLayer {
                    vertex_buffer,
                    index_buffer,
                    uniform_buffers,
                    bind_groups,
                    index_count: layer.mesh.indices.len() as u32,
                    index_format: match layer.mesh.index_format {
                        SkyIndexFormat::U16 => "U16",
                        SkyIndexFormat::U32 => "U32",
                    },
                });
                host_runtime::info(
                    "newviso.scene",
                    format!(
                        "atmospheric cloud layer paged in id='{}' cost={:.3} vertices={} indices={}",
                        layer.desc.id,
                        layer.desc.cost_factor,
                        layer.mesh.vertices.len(),
                        layer.mesh.indices.len()
                    ),
                );
            } else if !residency[index] {
                if let Some(layer_gpu) = gpu.layers[index].take() {
                    for group in layer_gpu.bind_groups {
                        render.destroy_bind_group(group);
                    }
                    for buffer in layer_gpu.uniform_buffers {
                        render.destroy_buffer(buffer);
                    }
                    render.destroy_buffer(layer_gpu.index_buffer);
                    render.destroy_buffer(layer_gpu.vertex_buffer);
                    host_runtime::info(
                        "newviso.scene",
                        format!(
                            "atmospheric cloud layer paged out id='{}' cost={:.3}",
                            resources.layers[index].desc.id,
                            resources.layers[index].desc.cost_factor
                        ),
                    );
                }
            }
        }
        Ok(())
    }

    pub(super) fn ensure_geometry_buffer_capacity(
        &mut self,
        required_vertices: u32,
        required_shadow_vertices: u32,
    ) -> Result<(), String> {
        let Some(mut gpu) = self.gpu else {
            return Err("3D scene GPU resources are not initialized".to_owned());
        };
        let render = RenderClient::new();
        let mut changed = false;

        if required_vertices > gpu.vertex_capacity {
            let next = required_vertices
                .checked_next_power_of_two()
                .unwrap_or(required_vertices)
                .max(1);
            let mut new_buffers = [0u32; SCENE_FRAME_SLOTS];
            for slot in 0..SCENE_FRAME_SLOTS {
                new_buffers[slot] = render.create_frame_buffer(
                    slot,
                    &format!("newviso.first_scene.vertices.grown.slot.{slot}"),
                    next as u64 * VERTEX_STRIDE,
                    "Vertex",
                    "CpuToGpu",
                )?;
            }
            for old_buffer in gpu.vertex_buffers {
                render.destroy_buffer(old_buffer);
            }
            gpu.vertex_buffers = new_buffers;
            gpu.vertex_capacity = next;
            changed = true;
        }

        if required_shadow_vertices > gpu.shadow_vertex_capacity {
            let next = required_shadow_vertices
                .checked_next_power_of_two()
                .unwrap_or(required_shadow_vertices)
                .max(1);
            let mut new_buffers = [0u32; SCENE_FRAME_SLOTS];
            for slot in 0..SCENE_FRAME_SLOTS {
                new_buffers[slot] = render.create_frame_buffer(
                    slot,
                    &format!("newviso.first_scene.shadow_vertices.grown.slot.{slot}"),
                    next as u64 * VERTEX_STRIDE,
                    "Vertex",
                    "CpuToGpu",
                )?;
            }
            for old_buffer in gpu.shadow_vertex_buffers {
                render.destroy_buffer(old_buffer);
            }
            gpu.shadow_vertex_buffers = new_buffers;
            gpu.shadow_vertex_capacity = next;
            changed = true;
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
                "CpuToGpu",
            )?;
            let new_index_buffer = render.create_buffer(
                "newviso.scene.static_asset_indices.grown",
                next as u64 * std::mem::size_of::<u32>() as u64,
                "Index",
                "CpuToGpu",
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

    pub(super) fn sync_static_asset_material_gpu(&mut self) -> Result<(), String> {
        let sync_started = std::time::Instant::now();
        let mut textures_created = 0usize;
        let Some(gpu) = self.gpu else {
            return Err("3D scene GPU resources are not initialized".to_owned());
        };

        // Build the material working set from visibility directly. The old
        // path first collected model ids and then scanned every resident mesh
        // twice per frame. Large streamed maps make that O(all residents) even
        // when only a small neighborhood is on screen.
        let mut visible_model_ids = std::collections::BTreeSet::<u64>::new();
        let mut visible_model_representatives = Vec::<u64>::new();
        for id in &self.frame_plan.visible_entities {
            let Some(mesh) = self.asset_meshes.get(&id.0) else {
                continue;
            };
            if visible_model_ids.insert(mesh.model_id.0) {
                visible_model_representatives.push(id.0);
            }
        }

        let mut pending_textures = BTreeMap::<u64, std::sync::Arc<TextureResource>>::new();
        for stable_id in &visible_model_representatives {
            let Some(mesh) = self.asset_meshes.get(stable_id) else {
                continue;
            };
            for material in mesh.materials.iter() {
                for texture in [
                    material.textures.base_color.as_ref(),
                    material.textures.normal.as_ref(),
                    material.textures.specular.as_ref(),
                    material.textures.emissive.as_ref(),
                    material.textures.environment.as_ref(),
                ]
                .into_iter()
                .flatten()
                {
                    if !self.asset_gpu_textures.contains_key(&texture.id.0) {
                        pending_textures
                            .entry(texture.id.0)
                            .or_insert_with(|| texture.clone());
                    }
                }
            }
        }

        let render = RenderClient::new();

        // Do not execute upload work here. Vulkan BeginFrame owns the only
        // frame-budgeted upload pump, so a streamed city cell cannot consume a
        // second transfer budget inside scene material synchronization.
        let pending_ids = self
            .asset_gpu_pending_textures
            .iter()
            .map(|(asset_id, texture_id)| (*asset_id, *texture_id))
            .collect::<Vec<_>>();
        for (asset_id, texture_id) in pending_ids {
            match render.texture_residency(texture_id)? {
                residency if residency.state == TextureResidencyState::Ready => {
                    self.asset_gpu_pending_textures.remove(&asset_id);
                    self.asset_gpu_textures.insert(asset_id, texture_id);
                }
                residency if residency.state == TextureResidencyState::Failed => {
                    self.asset_gpu_pending_textures.remove(&asset_id);
                    render.destroy_texture(texture_id);
                    host_runtime::warn(
                        "newviso.scene",
                        format!(
                            "streamed texture upload failed asset={} texture={} message={}",
                            asset_id,
                            texture_id,
                            residency.message.as_deref().unwrap_or("<none>")
                        ),
                    );
                }
                _ => {}
            }
        }

        let texture_candidates = pending_textures
            .into_iter()
            .filter(|(asset_id, _)| {
                !self.asset_gpu_pending_textures.contains_key(asset_id)
                    && !self.asset_gpu_textures.contains_key(asset_id)
            })
            .take(MAX_STREAMED_TEXTURE_UPLOADS_PER_FRAME)
            .collect::<Vec<_>>();
        for (asset_id, texture) in texture_candidates {
            // World-streamed materials do not need 4K/8K top mips at the
            // distances where they become resident. Rebase an existing mip as
            // level 0 instead of doing an expensive CPU resample.
            let first_mip_index = texture
                .mips
                .iter()
                .position(|mip| {
                    mip.width <= MAX_STREAMED_TEXTURE_DIMENSION
                        && mip.height <= MAX_STREAMED_TEXTURE_DIMENSION
                })
                .unwrap_or(0);
            let selected_mips = &texture.mips[first_mip_index..];
            let (gpu_width, gpu_height) = selected_mips
                .first()
                .map(|mip| (mip.width, mip.height))
                .unwrap_or((texture.width, texture.height));

            let mut data = Vec::new();
            let mut mips = Vec::with_capacity(selected_mips.len());
            for (rebased_level, mip) in selected_mips.iter().enumerate() {
                let offset = data.len() as u64;
                data.extend_from_slice(&mip.data);
                mips.push(TextureMipUpload {
                    level: u32::try_from(rebased_level)
                        .map_err(|_| "streamed texture mip level exceeds u32".to_owned())?,
                    width: mip.width,
                    height: mip.height,
                    offset,
                    byte_len: mip.data.len() as u64,
                });
            }
            let texture_label = format!("newviso.scene.material.texture.{}", texture.name);
            let gpu_texture = render.create_texture_deferred(
                &texture_label,
                gpu_width,
                gpu_height,
                texture_format_wire_name(texture.format),
                &mips,
                &data,
            )?;
            self.asset_gpu_pending_textures
                .insert(asset_id, gpu_texture);
            textures_created = textures_created.saturating_add(1);
            host_runtime::debug(
                "newviso.scene",
                format!(
                    "material texture GPU queued name='{}' source={}x{} gpu={}x{} mips={}/{} bytes={} format={}",
                    texture.name,
                    texture.width,
                    texture.height,
                    gpu_width,
                    gpu_height,
                    mips.len(),
                    texture.mips.len(),
                    data.len(),
                    texture_format_wire_name(texture.format),
                ),
            );
        }

        let mut pending_materials = BTreeMap::<(u64, u32), CpuAssetMaterial>::new();
        for stable_id in &visible_model_representatives {
            let Some(mesh) = self.asset_meshes.get(stable_id) else {
                continue;
            };
            for (slot, material) in mesh.materials.iter().enumerate() {
                let slot = u32::try_from(slot)
                    .map_err(|_| "material slot index exceeds u32".to_owned())?;
                let key = (mesh.model_id.0, slot);
                if !self.asset_gpu_materials.contains_key(&key) {
                    pending_materials
                        .entry(key)
                        .or_insert_with(|| material.clone());
                }
            }
        }

        let mut materials_created = 0usize;
        for (key, material) in pending_materials {
            if materials_created >= MAX_STREAMED_MATERIAL_CREATIONS_PER_FRAME {
                break;
            }

            // Never permanently bind a fallback for a texture that is known to
            // exist but merely has not crossed the GPU upload budget yet.
            let required_textures_ready = [
                material.textures.base_color.as_ref(),
                material.textures.normal.as_ref(),
                material.textures.specular.as_ref(),
                material.textures.emissive.as_ref(),
                material.textures.environment.as_ref(),
            ]
            .into_iter()
            .flatten()
            .all(|texture| self.asset_gpu_textures.contains_key(&texture.id.0));
            if !required_textures_ready {
                continue;
            }

            let gpu_texture = |texture: Option<&std::sync::Arc<TextureResource>>, fallback: u32| {
                texture
                    .and_then(|texture| self.asset_gpu_textures.get(&texture.id.0).copied())
                    .unwrap_or(fallback)
            };
            let base_color = gpu_texture(
                material.textures.base_color.as_ref(),
                gpu.default_base_color_texture,
            );
            let normal = gpu_texture(
                material.textures.normal.as_ref(),
                gpu.default_normal_texture,
            );
            let specular = gpu_texture(
                material.textures.specular.as_ref(),
                gpu.default_specular_texture,
            );
            let emissive = gpu_texture(
                material.textures.emissive.as_ref(),
                gpu.default_emissive_texture,
            );
            let environment = gpu_texture(
                material.textures.environment.as_ref(),
                gpu.default_environment_texture,
            );

            let uniform_buffer = render.create_buffer(
                &format!("newviso.scene.material.params.{}.{}", key.0, key.1),
                (12 * std::mem::size_of::<f32>()) as u64,
                "Uniform",
                "CpuToGpu",
            )?;
            let params = material.uniform_data();
            render.write_buffer_f32(uniform_buffer, 0, &params)?;

            let bind_group = match render.create_bind_group6(
                &format!("newviso.scene.material.bind.{}.{}", key.0, key.1),
                gpu.material_bind_group_layout,
                [
                    Some(base_color),
                    Some(normal),
                    Some(specular),
                    Some(emissive),
                    Some(environment),
                    None,
                ],
                Some(gpu.material_sampler),
                Some((uniform_buffer, 0, (12 * std::mem::size_of::<f32>()) as u64)),
            ) {
                Ok(group) => group,
                Err(error) => {
                    render.destroy_buffer(uniform_buffer);
                    return Err(error);
                }
            };
            self.asset_gpu_materials.insert(
                key,
                GpuAssetMaterial {
                    uniform_buffer,
                    bind_group,
                },
            );
            materials_created = materials_created.saturating_add(1);
            host_runtime::debug(
                "newviso.scene",
                format!(
                    "material GPU ready model={} slot={} name='{}' shader='{}' two_sided={} normal_strength={:.3} spec_intensity={:.3} spec_falloff={:.1} fresnel={:.3} emissive={:.3} env_reflect={:.3} env_texture={} opacity={:.3} alpha={:?} bucket={}",
                    key.0,
                    key.1,
                    material.name,
                    material.shader_name.as_deref().unwrap_or("<inline>"),
                    material.two_sided,
                    material.normal_strength,
                    material.specular_intensity,
                    material.specular_falloff,
                    material.specular_fresnel,
                    material.emissive_multiplier,
                    material.environment_reflection,
                    material.textures.environment.is_some(),
                    material.opacity,
                    material.alpha_mode,
                    material.render_bucket,
                ),
            );
        }

        let elapsed_ms = sync_started.elapsed().as_secs_f64() * 1000.0;
        if elapsed_ms >= 10.0 {
            host_runtime::info(
                "newviso.perf",
                format!(
                    "material_gpu_sync ms={:.2} visible_models={} textures_queued={} materials_created={} gpu_textures_ready={} gpu_textures_pending={} gpu_materials_total={} upload_scheduler=backend_begin_frame",
                    elapsed_ms,
                    visible_model_ids.len(),
                    textures_created,
                    materials_created,
                    self.asset_gpu_textures.len(),
                    self.asset_gpu_pending_textures.len(),
                    self.asset_gpu_materials.len()
                ),
            );
        }
        Ok(())
    }

    pub fn shutdown_renderer(&mut self) {
        let render = RenderClient::new();
        self.shutdown_mass_instance_gpu(&render);
        self.shutdown_weather_gpu_fx_renderer(&render);
        self.shutdown_volumetric_cloud_renderer(&render);
        if let Some(clouds) = self.gpu_atmospheric_clouds.take() {
            for layer in clouds.layers.into_iter().flatten() {
                for group in layer.bind_groups {
                    render.destroy_bind_group(group);
                }
                for buffer in layer.uniform_buffers {
                    render.destroy_buffer(buffer);
                }
                render.destroy_buffer(layer.index_buffer);
                render.destroy_buffer(layer.vertex_buffer);
            }
            render.destroy_pipeline(clouds.pipeline);
            render.destroy_shader(clouds.fragment_shader);
            render.destroy_shader(clouds.vertex_shader);
            render.destroy_bind_group_layout(clouds.bind_group_layout);
            render.destroy_bind_group(clouds.soft_sample_bind_group);
            render.destroy_bind_group_layout(clouds.soft_sample_bind_group_layout);
            for texture in clouds.texture_cache.into_values() {
                render.destroy_texture(texture);
            }
            render.destroy_pipeline(clouds.soft_depth_instanced_pipeline);
            render.destroy_pipeline(clouds.soft_depth_pipeline);
            render.destroy_shader(clouds.soft_depth_instanced_fragment_shader);
            render.destroy_shader(clouds.soft_depth_fragment_shader);
            render.destroy_shader(clouds.soft_depth_instanced_vertex_shader);
            render.destroy_shader(clouds.soft_depth_vertex_shader);
            for group in clouds.soft_depth_bind_groups {
                render.destroy_bind_group(group);
            }
            render.destroy_bind_group_layout(clouds.soft_depth_bind_group_layout);
            for buffer in clouds.soft_depth_uniforms {
                render.destroy_buffer(buffer);
            }
            render.destroy_render_target(clouds.soft_depth_render_target);
            render.destroy_sampler(clouds.sampler);
        }
        if let Some(sky) = self.gpu_sky.take() {
            render.destroy_pipeline(sky.pipeline);
            render.destroy_shader(sky.fragment_shader);
            render.destroy_shader(sky.vertex_shader);
            for group in sky.bind_groups {
                render.destroy_bind_group(group);
            }
            render.destroy_bind_group_layout(sky.bind_group_layout);
            render.destroy_sampler(sky.sampler);
            for buffer in sky.camera_uniforms {
                render.destroy_buffer(buffer);
            }
            if let Some(texture) = sky.billboard_texture {
                render.destroy_texture(texture);
            }
            render.destroy_texture(sky.detail_noise);
            render.destroy_texture(sky.starfield);
            render.destroy_texture(sky.base_noise);
            render.destroy_buffer(sky.index_buffer);
            render.destroy_buffer(sky.vertex_buffer);
        }

        let Some(gpu) = self.gpu.take() else {
            return;
        };
        for material in std::mem::take(&mut self.asset_gpu_materials).into_values() {
            render.destroy_bind_group(material.bind_group);
            render.destroy_buffer(material.uniform_buffer);
        }
        for texture in std::mem::take(&mut self.asset_gpu_textures).into_values() {
            render.destroy_texture(texture);
        }
        for texture in std::mem::take(&mut self.asset_gpu_pending_textures).into_values() {
            render.destroy_texture(texture);
        }
        render.destroy_pipeline(gpu.flare_pipeline);
        render.destroy_pipeline(gpu.particle_additive_pipeline);
        render.destroy_pipeline(gpu.asset_shadow_pipeline);
        render.destroy_pipeline(gpu.shadow_pipeline);
        render.destroy_pipeline(gpu.asset_alpha_pipeline);
        render.destroy_pipeline(gpu.asset_gbuffer_pipeline);
        render.destroy_pipeline(gpu.asset_pipeline);
        render.destroy_pipeline(gpu.alpha_pipeline);
        render.destroy_pipeline(gpu.gbuffer_pipeline);
        render.destroy_pipeline(gpu.pipeline);
        render.destroy_shader(gpu.flare_fragment_shader);
        render.destroy_shader(gpu.flare_vertex_shader);
        render.destroy_shader(gpu.shadow_fragment_shader);
        render.destroy_shader(gpu.asset_shadow_vertex_shader);
        render.destroy_shader(gpu.shadow_vertex_shader);
        render.destroy_shader(gpu.asset_vertex_shader);
        render.destroy_shader(gpu.gbuffer_fragment_shader);
        render.destroy_shader(gpu.fragment_shader);
        render.destroy_shader(gpu.vertex_shader);
        for group in gpu.shadow_bind_groups {
            render.destroy_bind_group(group);
        }
        for group in gpu.bind_groups {
            render.destroy_bind_group(group);
        }
        render.destroy_bind_group(gpu.default_material_bind_group);
        render.destroy_bind_group_layout(gpu.shadow_bind_group_layout);
        render.destroy_bind_group_layout(gpu.bind_group_layout);
        render.destroy_bind_group_layout(gpu.material_bind_group_layout);
        render.destroy_sampler(gpu.material_sampler);
        render.destroy_buffer(gpu.default_material_uniform);
        render.destroy_texture(gpu.default_environment_texture);
        render.destroy_texture(gpu.default_emissive_texture);
        render.destroy_texture(gpu.default_specular_texture);
        render.destroy_texture(gpu.default_normal_texture);
        render.destroy_texture(gpu.default_base_color_texture);
        render.destroy_sampler(gpu.shadow_sampler);
        render.destroy_render_target(gpu.shadow_render_target);
        for buffer in gpu.frame_uniforms {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.flare_vertex_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.particle_vertex_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.visibility_indirect_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.visibility_candidate_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.asset_instance_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.skinned_vertex_buffers {
            render.destroy_buffer(buffer);
        }
        render.destroy_buffer(gpu.asset_index_buffer);
        render.destroy_buffer(gpu.asset_vertex_buffer);
        for buffer in self.retired_asset_vertex_buffers.drain(..) {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.shadow_vertex_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.vertex_buffers {
            render.destroy_buffer(buffer);
        }
    }
}
fn texture_format_wire_name(format: TextureFormat) -> &'static str {
    match format {
        TextureFormat::Rgba8Unorm => "Rgba8Unorm",
        TextureFormat::Rgba8Srgb => "Rgba8Srgb",
        TextureFormat::Bc1RgbaUnorm => "Bc1RgbaUnorm",
        TextureFormat::Bc1RgbaSrgb => "Bc1RgbaSrgb",
        TextureFormat::Bc2RgbaUnorm => "Bc2RgbaUnorm",
        TextureFormat::Bc2RgbaSrgb => "Bc2RgbaSrgb",
        TextureFormat::Bc3RgbaUnorm => "Bc3RgbaUnorm",
        TextureFormat::Bc3RgbaSrgb => "Bc3RgbaSrgb",
        TextureFormat::Bc5RgUnorm => "Bc5RgUnorm",
        TextureFormat::Bc6hUf16 => "Bc6hUf16",
        TextureFormat::Bc6hSf16 => "Bc6hSf16",
        TextureFormat::Bc7RgbaUnorm => "Bc7RgbaUnorm",
        TextureFormat::Bc7RgbaSrgb => "Bc7RgbaSrgb",
    }
}
