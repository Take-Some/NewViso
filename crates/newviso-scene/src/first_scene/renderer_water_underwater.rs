//! Water fog composites the complete HDR scene before bloom/tonemapping.
use super::*;
pub(super) const FLOATS: usize = 36;
#[derive(Debug)]
pub(super) struct UnderwaterGpu {
    pipeline: u32,
    vertex_shader: u32,
    fragment_shader: u32,
    layout: u32,
    uniforms: [u32; SCENE_FRAME_SLOTS],
    groups: [u32; SCENE_FRAME_SLOTS],
    vertices: u32,
    caustic: u32,
    fog_texture: u32,
    bump_texture: u32,
    sampler: u32,
    geometry_depth: Option<u32>,
}
impl UnderwaterGpu {
    pub(super) fn new(
        render: &RenderClient,
        material: &renderer_terrain_material::TerrainGpuMaterial,
        sampler: u32,
    ) -> Result<Self, String> {
        use newviso_render_client::{BindingKind, GraphTextureSemantic};
        let data = host_runtime::call_service(
            ASSET_SERVICE,
            "asset.raw_bytes_v1",
            b"textures/environment/water/caustic.dds",
        )
        .map_err(|e| format!("water caustic: {e}"))?;
        let (w, h, _, mips) = renderer_terrain_material::load_dds_mips(&data, "water caustic")?;
        let caustic = render.create_texture(
            "newviso.water.caustic",
            w,
            h,
            "Bc1RgbaUnorm",
            &mips,
            &data[128..],
        )?;
        let layout = render.create_semantic_bind_group_layout(
            "newviso.water.underwater",
            &[
                BindingKind::UniformBuffer,
                BindingKind::GraphTexture2D(GraphTextureSemantic::LitColor),
                BindingKind::GraphTexture2D(GraphTextureSemantic::SceneDepth),
                BindingKind::Texture2D,
                BindingKind::Texture2D,
                BindingKind::Texture2D,
                BindingKind::Sampler,
                BindingKind::Texture2D,
            ],
        )?;
        let uniforms = create_frame_buffer_ring(
            render,
            "newviso.water.underwater",
            (FLOATS * 4) as u64,
            "Uniform",
        )?;
        let mut groups = [0; SCENE_FRAME_SLOTS];
        for slot in 0..SCENE_FRAME_SLOTS {
            groups[slot] = render.create_graph_bind_group(
                "newviso.water.underwater",
                layout,
                [
                    Some(material.textures[3]),
                    Some(caustic),
                    Some(material.textures[0]),
                    Some(caustic),
                ],
                sampler,
                (uniforms[slot], 0, (FLOATS * 4) as u64),
                caustic,
            )?;
        }
        let vertex_shader = render.create_shader_spirv(
            "newviso.water.underwater.vertex",
            ShaderStage::Vertex,
            include_bytes!("../assets/water_underwater.vert.spv"),
        )?;
        let fragment_shader = render.create_shader_spirv(
            "newviso.water.underwater.fragment",
            ShaderStage::Fragment,
            include_bytes!("../assets/water_underwater.frag.spv"),
        )?;
        let attrs = [VertexAttribute {
            location: 0,
            offset: 0,
            format: VertexFormat::Float32x3,
        }];
        let pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.water.underwater",
            vertex_shader,
            fragment_shader,
            vertex_stride: 12,
            attributes: &attrs,
            topology: "TriangleList",
            bind_group_layouts: &[layout],
            color_format: "Rgba16Float",
            depth_format: None,
            depth_test: false,
            depth_write: false,
            depth_compare: "Always",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.water.underwater.v2",
        })?;
        let vertices = render.create_buffer(
            "newviso.water.underwater.triangle",
            36,
            "Vertex",
            "CpuToGpu",
        )?;
        render.write_buffer_f32(vertices, 0, &[-1., -1., 0., 3., -1., 0., -1., 3., 0.])?;
        Ok(Self {
            pipeline,
            vertex_shader,
            fragment_shader,
            layout,
            uniforms,
            groups,
            vertices,
            caustic,
            fog_texture: material.textures[3],
            bump_texture: material.textures[0],
            sampler,
            geometry_depth: None,
        })
    }
    /// Keep original opaque geometry depth separate from the water surface
    /// written into the main depth attachment. Caustics illuminate geometry,
    /// never the occluding surface or sky.
    pub(super) fn set_geometry_depth(
        &mut self,
        render: &RenderClient,
        texture: u32,
    ) -> Result<(), String> {
        if self.geometry_depth == Some(texture) {
            return Ok(());
        }
        let mut groups = [0; SCENE_FRAME_SLOTS];
        for slot in 0..SCENE_FRAME_SLOTS {
            match render.create_graph_bind_group(
                "newviso.water.underwater",
                self.layout,
                [
                    Some(self.fog_texture),
                    Some(self.caustic),
                    Some(self.bump_texture),
                    Some(texture),
                ],
                self.sampler,
                (self.uniforms[slot], 0, (FLOATS * 4) as u64),
                self.caustic,
            ) {
                Ok(group) => groups[slot] = group,
                Err(error) => {
                    for group in groups.into_iter().filter(|group| *group != 0) {
                        render.destroy_bind_group(group);
                    }
                    return Err(error);
                }
            }
        }
        for group in std::mem::replace(&mut self.groups, groups) {
            render.destroy_bind_group(group);
        }
        self.geometry_depth = Some(texture);
        Ok(())
    }
    pub(super) fn upload(
        &self,
        scene: &Scene3dRuntime,
        render: &RenderClient,
        slot: usize,
        aspect: f32,
    ) -> Result<(), String> {
        let camera = &scene.camera;
        let forward = camera.target.sub(camera.position).normalized();
        let right = forward.cross(camera.up).normalized();
        let up = right.cross(forward).normalized();
        let tan = (camera.fov_y_degrees.to_radians() * 0.5).tan();
        let surface = scene
            .water_sample([camera.position.x, camera.position.z])
            .height;
        let settings = scene.water_simulation.optics;
        let mut block = [0.0; FLOATS];
        block[0..4].copy_from_slice(&[
            camera.position.x,
            camera.position.y,
            camera.position.z,
            surface,
        ]);
        block[4..8].copy_from_slice(&[forward.x, forward.y, forward.z, camera.near]);
        block[8..12].copy_from_slice(&[
            right.x * tan * aspect,
            right.y * tan * aspect,
            right.z * tan * aspect,
            camera.far,
        ]);
        block[12..16].copy_from_slice(&[
            up.x * tan,
            up.y * tan,
            up.z * tan,
            scene.water_simulation.seconds() as f32,
        ]);
        block[16..20].copy_from_slice(&[
            if super::renderer_water::water_optics_diagnostic() == 0 {
                scene.water_simulation.immersion.blend
            } else {
                0.0
            },
            settings.opacity,
            settings.refraction_index,
            settings.caustics_strength,
        ]);
        let environment = &scene.scene_environment;
        let ambient = environment
            .ambient_color
            .map(|v| v * environment.ambient_intensity.max(0.0));
        block[20..23].copy_from_slice(&ambient);
        for (_, transform, light) in scene.world.active_lights() {
            if light.light_type == LightType::Directional {
                block[24..27].copy_from_slice(&light.color.map(|v| v * light.intensity.max(0.0)));
                let sun = light_direction(transform.rotation_degrees);
                block[28..31].copy_from_slice(&[sun.x, sun.y, sun.z]);
                break;
            }
        }
        block[32..36].copy_from_slice(&[
            settings.caustics_time_scale,
            settings.caustics_depth,
            settings.fog_pierce_intensity,
            settings.fog_light_intensity,
        ]);
        render.write_buffer_f32(self.uniforms[slot], 0, &block)
    }
    pub(super) fn draw(&self, render: &RenderClient, slot: usize) -> Result<(), String> {
        render.set_render_phase(Some("Custom"))?;
        render.set_draw_list_kind(Some(RenderDrawListKind::Transparent))?;
        render.set_pipeline(self.pipeline)?;
        render.set_bind_group(0, self.groups[slot])?;
        render.set_vertex_buffer(0, self.vertices, 0)?;
        render.draw(3)
    }
    pub(super) fn attach(graph: &mut RenderGraphDesc) {
        let extent = graph
            .resources
            .iter()
            .find(|r| r.id == RenderGraphResourceId(6))
            .and_then(|r| r.extent)
            .unwrap_or(Extent2D::new(1, 1));
        graph.resources.push(
            RenderGraphResourceDesc::transient_texture(
                RenderGraphResourceId(24),
                "newviso.water.underwater_hdr",
                RenderGraphResourceUsage::ColorAttachment,
                extent,
                RenderTextureFormat::Rgba16Float,
            )
            .with_semantic(RenderGraphResourceSemantic::LitColor),
        );
        let index = graph
            .passes
            .iter()
            .position(|p| p.kind == RenderGraphPassKind::ScreenSpaceReflections)
            .unwrap_or(graph.passes.len());
        for pass in &mut graph.passes[index..] {
            for read in &mut pass.reads {
                if read.resource == RenderGraphResourceId(6) {
                    read.resource = RenderGraphResourceId(24);
                }
            }
        }
        let pass = RenderGraphPassDesc::new(
            RenderGraphPassId(24),
            "newviso.water.underwater",
            RenderGraphPassKind::Custom,
        )
        .with_domain(RenderGraphPassDomain::PostProcess)
        .reads(
            RenderGraphResourceId(6),
            RenderGraphResourceUsage::SampledTexture,
        )
        .reads(
            RenderGraphResourceId(5),
            RenderGraphResourceUsage::SampledTexture,
        )
        .reads(
            RenderGraphResourceId(22),
            RenderGraphResourceUsage::SampledTexture,
        )
        .writes(
            RenderGraphResourceId(24),
            RenderGraphResourceUsage::ColorAttachment,
        )
        .draw_list(RenderDrawListKind::Transparent);
        graph.passes.insert(index, pass);
    }
    pub(super) fn shutdown(self, render: &RenderClient) {
        for group in self.groups {
            render.destroy_bind_group(group);
        }
        for buffer in self.uniforms {
            render.destroy_buffer(buffer);
        }
        render.destroy_buffer(self.vertices);
        render.destroy_pipeline(self.pipeline);
        render.destroy_shader(self.vertex_shader);
        render.destroy_shader(self.fragment_shader);
        render.destroy_texture(self.caustic);
        render.destroy_bind_group_layout(self.layout);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underwater_graph_uses_hdr_extent_and_original_geometry_depth() {
        let mut graph = main_deferred_hdr_render_graph(
            1, 1440, 900, FrameCameraContext::default(),
        );
        // A shadow target can precede the viewport resources.
        graph.resources.insert(0, RenderGraphResourceDesc::transient_texture(
            RenderGraphResourceId(99), "shadow",
            RenderGraphResourceUsage::DepthAttachment,
            Extent2D::new(2048, 2048), RenderTextureFormat::Depth32Float,
        ));
        UnderwaterGpu::attach(&mut graph);
        let hdr = graph.resources.iter()
            .find(|r| r.id == RenderGraphResourceId(24)).unwrap();
        assert_eq!(hdr.extent, Some(Extent2D::new(1440, 900)));
        let underwater = graph.passes.iter()
            .position(|p| p.id == RenderGraphPassId(24)).unwrap();
        assert!(graph.passes[underwater].reads.iter()
            .any(|r| r.resource == RenderGraphResourceId(22)));
        for pass in &graph.passes[underwater + 1..] {
            assert!(!pass.reads.iter().any(|r| r.resource == RenderGraphResourceId(6)));
        }
    }
}
