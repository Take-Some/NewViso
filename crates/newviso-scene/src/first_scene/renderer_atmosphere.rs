use super::*;

impl Scene3dRuntime {
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
            color_format: "R32Float",
            ..scene_pipeline_desc(
                "newviso.atmospheric_clouds.soft_depth.pipeline",
                "newviso.atmospheric_clouds.soft_depth.dynamic.v1",
                soft_depth_vertex_shader,
                soft_depth_fragment_shader,
                &soft_depth_attributes,
                &[soft_depth_bind_group_layout],
            )
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
                color_format: "R32Float",
                ..scene_pipeline_desc(
                    "newviso.atmospheric_clouds.soft_depth.instanced.pipeline",
                    "newviso.atmospheric_clouds.soft_depth.instanced.v2",
                    soft_depth_instanced_vertex_shader,
                    soft_depth_instanced_fragment_shader,
                    &soft_depth_asset_attributes,
                    &[soft_depth_bind_group_layout, material_bind_group_layout],
                )
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
            vertex_stride: ATMOSPHERIC_CLOUD_VERTEX_STRIDE,
            depth_write: false,
            blend_mode: "Alpha",
            ..scene_pipeline_desc(
                "newviso.atmospheric_clouds.pipeline",
                "newviso.atmospheric_clouds.cloudsps.v2",
                vertex_shader,
                fragment_shader,
                &attributes,
                &[bind_group_layout, soft_sample_bind_group_layout],
            )
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
}
