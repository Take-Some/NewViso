use super::*;

impl Scene3dRuntime {
    pub fn initialize_renderer(&mut self) -> Result<(), String> {
        let render = RenderClient::new();

        if self.sky.is_some() {
            self.initialize_sky_renderer(&render)?;
        }

        let vertex_buffer = render.create_buffer(
            "newviso.first_scene.vertices",
            self.vertex_capacity() as u64 * VERTEX_STRIDE,
            "Vertex",
            "CpuToGpu",
        )?;
        let shadow_vertex_buffer = render.create_buffer(
            "newviso.first_scene.shadow_vertices",
            self.shadow_vertex_capacity() as u64 * VERTEX_STRIDE,
            "Vertex",
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
            DEFAULT_SHADOW_RESOLUTION,
            DEFAULT_SHADOW_RESOLUTION,
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
        ];

        let pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.first_scene.pipeline",
            vertex_shader,
            fragment_shader,
            vertex_stride: VERTEX_STRIDE,
            attributes: &attributes,
            topology: "TriangleList",
            bind_group_layouts: &[bind_group_layout],
            color_format: "Bgra8Unorm",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: true,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.scene.lit.v1",
        })?;

        let shadow_attributes = [VertexAttribute {
            location: 0,
            offset: 0,
            format: VertexFormat::Float32x4,
        }];
        let shadow_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.scene.shadow.pipeline",
            vertex_shader: shadow_vertex_shader,
            fragment_shader: shadow_fragment_shader,
            vertex_stride: VERTEX_STRIDE,
            attributes: &shadow_attributes,
            topology: "TriangleList",
            bind_group_layouts: &[shadow_bind_group_layout],
            color_format: "R32Float",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: true,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.scene.shadow.v1",
        })?;

        let flare_vertex_buffer = render.create_buffer(
            "newviso.scene.lens_flare.vertices",
            (MAX_LENS_FLARES * MAX_FLARE_ELEMENTS * 6) as u64 * FLARE_VERTEX_STRIDE,
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
            cube_capacity: self.cubes.len() + MAX_RUNTIME_CUBES,
            shadow_vertex_buffer,
            frame_uniform,
            bind_group_layout,
            bind_group,
            shadow_bind_group_layout,
            shadow_bind_group,
            shadow_render_target,
            shadow_sampler,
            vertex_shader,
            fragment_shader,
            pipeline,
            shadow_vertex_shader,
            shadow_fragment_shader,
            shadow_pipeline,
            shadow_resolution: DEFAULT_SHADOW_RESOLUTION,
            flare_vertex_buffer,
            flare_vertex_shader,
            flare_fragment_shader,
            flare_pipeline,
        });

        host_runtime::info(
            "newviso.scene",
            format!(
                "GPU scene ready buffer={} shadow_buffer={} pipeline={} shadow_pipeline={} flare_pipeline={} shadow_map={}x{} sky={}",
                vertex_buffer,
                shadow_vertex_buffer,
                pipeline,
                shadow_pipeline,
                flare_pipeline,
                DEFAULT_SHADOW_RESOLUTION,
                DEFAULT_SHADOW_RESOLUTION,
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
        render.destroy_pipeline(gpu.flare_pipeline);
        render.destroy_pipeline(gpu.shadow_pipeline);
        render.destroy_pipeline(gpu.pipeline);
        render.destroy_shader(gpu.flare_fragment_shader);
        render.destroy_shader(gpu.flare_vertex_shader);
        render.destroy_shader(gpu.shadow_fragment_shader);
        render.destroy_shader(gpu.shadow_vertex_shader);
        render.destroy_shader(gpu.fragment_shader);
        render.destroy_shader(gpu.vertex_shader);
        render.destroy_bind_group(gpu.shadow_bind_group);
        render.destroy_bind_group(gpu.bind_group);
        render.destroy_bind_group_layout(gpu.shadow_bind_group_layout);
        render.destroy_bind_group_layout(gpu.bind_group_layout);
        render.destroy_sampler(gpu.shadow_sampler);
        render.destroy_render_target(gpu.shadow_render_target);
        render.destroy_buffer(gpu.frame_uniform);
        render.destroy_buffer(gpu.flare_vertex_buffer);
        render.destroy_buffer(gpu.shadow_vertex_buffer);
        render.destroy_buffer(gpu.vertex_buffer);
    }
}
