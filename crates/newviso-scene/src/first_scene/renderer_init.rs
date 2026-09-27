use super::*;

impl Scene3dRuntime {
    pub fn initialize_renderer(&mut self) -> Result<(), String> {
        self.validate_render_policy()?;
        let render = RenderClient::new();

        if self.sky.is_some() {
            self.initialize_sky_renderer(&render)?;
        }

        let vertex_capacity = self.vertex_capacity().max(1);
        let shadow_vertex_capacity = self.shadow_vertex_capacity().max(1);
        let vertex_buffer = render.create_buffer(
            "newviso.first_scene.vertices",
            vertex_capacity as u64 * VERTEX_STRIDE,
            "Vertex",
            "CpuToGpu",
        )?;
        let shadow_vertex_buffer = render.create_buffer(
            "newviso.first_scene.shadow_vertices",
            shadow_vertex_capacity as u64 * VERTEX_STRIDE,
            "Vertex",
            "CpuToGpu",
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
            "CpuToGpu",
        )?;
        let asset_index_buffer = render.create_buffer(
            "newviso.scene.static_asset_indices",
            asset_vertex_capacity as u64 * std::mem::size_of::<u32>() as u64,
            "Index",
            "CpuToGpu",
        )?;
        let asset_instance_buffer = render.create_buffer(
            "newviso.scene.static_asset_instances",
            DEFAULT_ASSET_INSTANCE_CAPACITY as u64 * INSTANCE_STRIDE,
            "Vertex",
            "CpuToGpu",
        )?;
        let visibility_candidate_buffer = render.create_buffer(
            "newviso.scene.visibility.candidates",
            MAX_HIZ_DRAW_CANDIDATES as u64 * HIZ_CANDIDATE_STRIDE,
            "Storage",
            "CpuToGpu",
        )?;
        let visibility_indirect_buffer = render.create_buffer(
            "newviso.scene.visibility.indirect",
            MAX_HIZ_DRAW_CANDIDATES as u64 * HIZ_INDIRECT_STRIDE,
            "Indirect",
            "CpuToGpu",
        )?;
        let frame_uniform = render.create_buffer(
            "newviso.scene.frame_uniform",
            (SCENE_FRAME_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
            "Uniform",
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
        let bind_group = render.create_bind_group(
            "newviso.scene.frame_bind_group",
            bind_group_layout,
            [Some(shadow_texture), None, None, None],
            Some(shadow_sampler),
            Some((
                frame_uniform,
                0,
                (SCENE_FRAME_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
            )),
        )?;

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
        let shadow_bind_group = render.create_bind_group(
            "newviso.scene.shadow_bind_group",
            shadow_bind_group_layout,
            [None, None, None, None],
            None,
            Some((
                frame_uniform,
                0,
                (SCENE_FRAME_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
            )),
        )?;

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
            bind_group_layouts: &[bind_group_layout, material_bind_group_layout],
            color_format: "Bgra8Unorm",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: true,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.scene.rage_material.opaque.v3",
        })?;
        let alpha_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.first_scene.alpha_pipeline",
            vertex_shader,
            fragment_shader,
            vertex_stride: VERTEX_STRIDE,
            attributes: &attributes,
            topology: "TriangleList",
            bind_group_layouts: &[bind_group_layout, material_bind_group_layout],
            color_format: "Bgra8Unorm",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: false,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Alpha",
            cache_key: "newviso.scene.rage_material.alpha.v3",
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
                bind_group_layouts: &[bind_group_layout, material_bind_group_layout],
                color_format: "Bgra8Unorm",
                depth_format: Some("Depth32Float"),
                depth_test: true,
                depth_write: true,
                depth_compare: "LessOrEqual",
                cull_mode: "None",
                blend_mode: "Opaque",
                cache_key: "newviso.scene.asset_instanced.opaque.v1",
            },
            &asset_vertex_layouts,
        )?;
        let asset_alpha_pipeline = render.create_pipeline_with_layouts(
            GraphicsPipelineDesc {
                label: "newviso.scene.asset_instanced.alpha_pipeline",
                vertex_shader: asset_vertex_shader,
                fragment_shader,
                vertex_stride: VERTEX_STRIDE,
                attributes: &attributes,
                topology: "TriangleList",
                bind_group_layouts: &[bind_group_layout, material_bind_group_layout],
                color_format: "Bgra8Unorm",
                depth_format: Some("Depth32Float"),
                depth_test: true,
                depth_write: false,
                depth_compare: "LessOrEqual",
                cull_mode: "None",
                blend_mode: "Alpha",
                cache_key: "newviso.scene.asset_instanced.alpha.v1",
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

        let flare_vertex_buffer = render.create_buffer(
            "newviso.scene.lens_flare.vertices",
            (self.render_policy.lens_flare_capacity * self.render_policy.flare_element_capacity * 6)
                .max(1) as u64
                * FLARE_VERTEX_STRIDE,
            "Vertex",
            "CpuToGpu",
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
            color_format: "Bgra8Unorm",
            depth_format: Some("Depth32Float"),
            depth_test: false,
            depth_write: false,
            depth_compare: "Always",
            cull_mode: "None",
            blend_mode: "Additive",
            cache_key: "newviso.scene.lens_flare.v1",
        })?;

        self.gpu = Some(GpuScene {
            vertex_buffer,
            vertex_capacity,
            cube_capacity: self.cubes.len() + self.render_policy.runtime_cube_capacity,
            shadow_vertex_buffer,
            shadow_vertex_capacity,
            asset_vertex_buffer,
            asset_vertex_capacity,
            asset_vertex_count: 0,
            asset_index_buffer,
            asset_index_capacity: asset_vertex_capacity,
            asset_instance_buffer,
            asset_instance_capacity: DEFAULT_ASSET_INSTANCE_CAPACITY,
            visibility_candidate_buffer,
            visibility_indirect_buffer,
            frame_uniform,
            bind_group_layout,
            bind_group,
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
            shadow_bind_group,
            shadow_render_target,
            shadow_sampler,
            vertex_shader,
            fragment_shader,
            pipeline,
            alpha_pipeline,
            asset_pipeline,
            asset_alpha_pipeline,
            shadow_vertex_shader,
            shadow_fragment_shader,
            shadow_pipeline,
            asset_shadow_pipeline,
            asset_vertex_shader,
            asset_shadow_vertex_shader,
            shadow_resolution: self.render_policy.shadow_resolution,
            flare_vertex_buffer,
            flare_vertex_shader,
            flare_fragment_shader,
            flare_pipeline,
        });
        self.invalidate_gpu_instance_upload();

        host_runtime::info(
            "newviso.scene",
            format!(
                "GPU scene ready buffer={} shadow_buffer={} asset_buffer={} instance_buffer={} pipeline={} asset_pipeline={} shadow_pipeline={} flare_pipeline={} shadow_map={}x{} sky={}",
                vertex_buffer,
                shadow_vertex_buffer,
                asset_vertex_buffer,
                asset_instance_buffer,
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
        let camera_uniform = render.create_buffer(
            "newviso.sky.frame",
            (SKY_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
            "Uniform",
            "CpuToGpu",
        )?;

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
        let bind_group = render.create_bind_group(
            "newviso.sky.bind_group",
            bind_group_layout,
            [
                Some(base_noise),
                Some(starfield),
                Some(detail_noise),
                Some(billboard_texture.unwrap_or(detail_noise)),
            ],
            Some(sampler),
            Some((
                camera_uniform,
                0,
                (SKY_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
            )),
        )?;

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
            color_format: "Bgra8Unorm",
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
            bind_group,
            camera_uniform,
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
            let new_buffer = render.create_buffer(
                "newviso.first_scene.vertices.grown",
                next as u64 * VERTEX_STRIDE,
                "Vertex",
                "CpuToGpu",
            )?;
            render.destroy_buffer(gpu.vertex_buffer);
            gpu.vertex_buffer = new_buffer;
            gpu.vertex_capacity = next;
            changed = true;
        }

        if required_shadow_vertices > gpu.shadow_vertex_capacity {
            let next = required_shadow_vertices
                .checked_next_power_of_two()
                .unwrap_or(required_shadow_vertices)
                .max(1);
            let new_buffer = render.create_buffer(
                "newviso.first_scene.shadow_vertices.grown",
                next as u64 * VERTEX_STRIDE,
                "Vertex",
                "CpuToGpu",
            )?;
            render.destroy_buffer(gpu.shadow_vertex_buffer);
            gpu.shadow_vertex_buffer = new_buffer;
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
        if self.asset_upload_from_float.is_none() && self.asset_skin_upload_ranges.is_empty() {
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

        // Skinning changes positions/normals/tangents but never topology. Upload
        // only the mutated character spans and never rewrite the index buffer.
        // Any part already covered by the static tail upload above is skipped.
        let static_coverage_from = upload_from_float;
        let mut skin_ranges = self.asset_skin_upload_ranges.clone();
        skin_ranges.sort_unstable_by_key(|range| range.0);
        let mut merged = Vec::<(usize, usize)>::new();
        for (start, end) in skin_ranges {
            let start = start.min(self.asset_vertex_data.len());
            let end = end
                .min(self.asset_vertex_data.len())
                .min(static_coverage_from);
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

        let mut skin_uploaded_floats = 0usize;
        for (start, end) in &merged {
            let byte_offset = (*start as u64)
                .checked_mul(std::mem::size_of::<f32>() as u64)
                .ok_or_else(|| "skinned asset upload byte offset overflow".to_owned())?;
            render.write_buffer_f32(
                gpu.asset_vertex_buffer,
                byte_offset,
                &self.asset_vertex_data[*start..*end],
            )?;
            skin_uploaded_floats = skin_uploaded_floats.saturating_add(end - start);
        }

        gpu.asset_vertex_count = required_vertices;
        self.gpu = Some(gpu);
        self.asset_upload_from_float = None;
        self.asset_skin_upload_ranges.clear();

        // Do not emit a per-frame debug line for animation-only uploads. Besides
        // log noise, serializing one line every render frame is measurable work.
        if had_static_upload || buffer_grew {
            host_runtime::debug(
                "newviso.scene",
                format!(
                    "static asset GPU sync instances={} vertices={} capacity={} uploaded_from_vertex={} skin_ranges={} skin_floats={}",
                    self.asset_meshes.len(),
                    required_vertices,
                    gpu.asset_vertex_capacity,
                    upload_from_float / FLOATS_PER_VERTEX,
                    merged.len(),
                    skin_uploaded_floats
                ),
            );
        }
        Ok(())
    }
    pub(super) fn sync_static_asset_material_gpu(&mut self) -> Result<(), String> {
        let sync_started = std::time::Instant::now();
        let mut textures_created = 0usize;
        let Some(gpu) = self.gpu else {
            return Err("3D scene GPU resources are not initialized".to_owned());
        };

        let visible_model_ids = self
            .frame_plan
            .visible_entities
            .iter()
            .filter_map(|id| self.asset_meshes.get(&id.0).map(|mesh| mesh.model_id.0))
            .collect::<std::collections::BTreeSet<_>>();

        let mut pending_textures = BTreeMap::<u64, std::sync::Arc<TextureResource>>::new();
        for mesh in self
            .asset_meshes
            .values()
            .filter(|mesh| visible_model_ids.contains(&mesh.model_id.0))
        {
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

        let upload_report = render.pump_uploads(
            STREAMED_TEXTURE_UPLOAD_BUDGET_BYTES,
            STREAMED_TEXTURE_UPLOAD_BUDGET_JOBS,
            STREAMED_TEXTURE_UPLOAD_BLOCKING_MS,
        )?;
        if upload_report.failed_jobs > 0 {
            host_runtime::warn(
                "newviso.scene",
                format!(
                    "streamed texture upload pump reported failed_jobs={} remaining_jobs={} remaining_bytes={}",
                    upload_report.failed_jobs,
                    upload_report.remaining_jobs,
                    upload_report.remaining_bytes
                ),
            );
        }

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
        for mesh in self
            .asset_meshes
            .values()
            .filter(|mesh| visible_model_ids.contains(&mesh.model_id.0))
        {
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
                    "material_gpu_sync ms={:.2} visible_models={} textures_queued={} materials_created={} gpu_textures_ready={} gpu_textures_pending={} gpu_materials_total={} upload_remaining_jobs={} upload_remaining_bytes={}",
                    elapsed_ms,
                    visible_model_ids.len(),
                    textures_created,
                    materials_created,
                    self.asset_gpu_textures.len(),
                    self.asset_gpu_pending_textures.len(),
                    self.asset_gpu_materials.len(),
                    upload_report.remaining_jobs,
                    upload_report.remaining_bytes
                ),
            );
        }
        Ok(())
    }

    pub fn shutdown_renderer(&mut self) {
        let render = RenderClient::new();
        if let Some(sky) = self.gpu_sky.take() {
            render.destroy_pipeline(sky.pipeline);
            render.destroy_shader(sky.fragment_shader);
            render.destroy_shader(sky.vertex_shader);
            render.destroy_bind_group(sky.bind_group);
            render.destroy_bind_group_layout(sky.bind_group_layout);
            render.destroy_sampler(sky.sampler);
            render.destroy_buffer(sky.camera_uniform);
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
        render.destroy_pipeline(gpu.asset_shadow_pipeline);
        render.destroy_pipeline(gpu.shadow_pipeline);
        render.destroy_pipeline(gpu.asset_alpha_pipeline);
        render.destroy_pipeline(gpu.asset_pipeline);
        render.destroy_pipeline(gpu.alpha_pipeline);
        render.destroy_pipeline(gpu.pipeline);
        render.destroy_shader(gpu.flare_fragment_shader);
        render.destroy_shader(gpu.flare_vertex_shader);
        render.destroy_shader(gpu.shadow_fragment_shader);
        render.destroy_shader(gpu.asset_shadow_vertex_shader);
        render.destroy_shader(gpu.shadow_vertex_shader);
        render.destroy_shader(gpu.asset_vertex_shader);
        render.destroy_shader(gpu.fragment_shader);
        render.destroy_shader(gpu.vertex_shader);
        render.destroy_bind_group(gpu.shadow_bind_group);
        render.destroy_bind_group(gpu.bind_group);
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
        render.destroy_buffer(gpu.frame_uniform);
        render.destroy_buffer(gpu.flare_vertex_buffer);
        render.destroy_buffer(gpu.visibility_indirect_buffer);
        render.destroy_buffer(gpu.visibility_candidate_buffer);
        render.destroy_buffer(gpu.asset_instance_buffer);
        render.destroy_buffer(gpu.asset_index_buffer);
        render.destroy_buffer(gpu.asset_vertex_buffer);
        for buffer in self.retired_asset_vertex_buffers.drain(..) {
            render.destroy_buffer(buffer);
        }
        render.destroy_buffer(gpu.shadow_vertex_buffer);
        render.destroy_buffer(gpu.vertex_buffer);
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
