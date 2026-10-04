use super::*;

impl Scene3dRuntime {
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
            vertex_stride: SKY_VERTEX_STRIDE,
            depth_test: false,
            depth_write: false,
            depth_compare: "Always",
            ..scene_pipeline_desc(
                "newviso.sky.pipeline",
                "newviso.sky.semantic.v3",
                vertex_shader,
                fragment_shader,
                &attributes,
                &[bind_group_layout],
            )
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
}
