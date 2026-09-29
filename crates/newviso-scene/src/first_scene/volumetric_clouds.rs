use super::*;

pub(super) const VOLUMETRIC_CLOUD_VERTEX_SHADER: &[u8] =
    include_bytes!("../assets/volumetric_cloud.vert.spv");
pub(super) const VOLUMETRIC_CLOUD_FRAGMENT_SHADER: &[u8] =
    include_bytes!("../assets/volumetric_cloud.frag.spv");
pub(super) const VOLUMETRIC_TEMPORAL_FRAGMENT_SHADER: &[u8] =
    include_bytes!("../assets/volumetric_cloud_temporal.frag.spv");
pub(super) const VOLUMETRIC_COMPOSITE_FRAGMENT_SHADER: &[u8] =
    include_bytes!("../assets/volumetric_cloud_composite.frag.spv");

pub(super) const VOLUMETRIC_CLOUD_UNIFORM_FLOATS: usize = 76;
pub(super) const VOLUMETRIC_FULLSCREEN_FLOATS_PER_VERTEX: usize = 2;
pub(super) const VOLUMETRIC_FULLSCREEN_VERTEX_STRIDE: u64 =
    (VOLUMETRIC_FULLSCREEN_FLOATS_PER_VERTEX * std::mem::size_of::<f32>()) as u64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumetricCloudDesc {
    pub enabled: bool,
    pub base_altitude: f32,
    pub top_altitude: f32,
    pub max_distance: f32,
    pub resolution_scale: f32,
    pub ray_steps: u32,
    pub light_steps: u32,
    pub coverage: f32,
    pub density: f32,
    pub shape_scale: f32,
    pub detail_scale: f32,
    pub detail_strength: f32,
    pub erosion_strength: f32,
    pub extinction: f32,
    pub scattering: f32,
    pub ambient: f32,
    pub phase_forward: f32,
    pub powder_strength: f32,
    pub temporal_blend: f32,
    pub jitter_strength: f32,
}

impl Default for VolumetricCloudDesc {
    fn default() -> Self {
        Self {
            enabled: false,
            base_altitude: 800.0,
            top_altitude: 2200.0,
            max_distance: 20_000.0,
            resolution_scale: 0.5,
            ray_steps: 48,
            light_steps: 6,
            coverage: 0.45,
            density: 1.0,
            shape_scale: 0.00032,
            detail_scale: 0.0018,
            detail_strength: 0.42,
            erosion_strength: 0.55,
            extinction: 0.012,
            scattering: 1.0,
            ambient: 0.28,
            phase_forward: 0.55,
            powder_strength: 0.45,
            temporal_blend: 0.88,
            jitter_strength: 1.0,
        }
    }
}

#[derive(Debug)]
pub(super) struct GpuVolumetricCloudTargets {
    pub width: u32,
    pub height: u32,
    pub scene_depth_target: u32,
    pub raw_target: u32,
    pub history_targets: [u32; 2],
    pub raymarch_bind_groups: [u32; SCENE_FRAME_SLOTS],
    /// First index is renderer frame slot; second is the history target written this frame.
    /// Each temporal set samples the opposite history target while binding that slot's UBO.
    pub temporal_bind_groups: [[u32; 2]; SCENE_FRAME_SLOTS],
    pub composite_bind_groups: [u32; 2],
}

#[derive(Debug)]
pub(super) struct GpuVolumetricClouds {
    pub fullscreen_vertex_buffer: u32,
    pub uniform_buffers: [u32; SCENE_FRAME_SLOTS],
    pub depth_uniform_buffers: [u32; SCENE_FRAME_SLOTS],
    pub sampler: u32,

    pub raymarch_bind_group_layout: u32,
    pub temporal_bind_group_layout: u32,
    pub composite_bind_group_layout: u32,
    pub depth_bind_group_layout: u32,
    pub depth_bind_groups: [u32; SCENE_FRAME_SLOTS],

    pub vertex_shader: u32,
    pub raymarch_fragment_shader: u32,
    pub temporal_fragment_shader: u32,
    pub composite_fragment_shader: u32,
    pub depth_vertex_shader: u32,
    pub depth_instanced_vertex_shader: u32,
    pub depth_fragment_shader: u32,
    pub depth_instanced_fragment_shader: u32,

    pub raymarch_pipeline: u32,
    pub temporal_pipeline: u32,
    pub composite_pipeline: u32,
    pub depth_pipeline: u32,
    pub depth_instanced_pipeline: u32,

    pub targets: Option<GpuVolumetricCloudTargets>,
    pub previous_view_projection: [f32; 16],
    pub history_valid: bool,
}

impl Scene3dRuntime {
    /// Current authored/runtime volumetric-cloud descriptor.
    ///
    /// Weather scripts may update this descriptor at runtime. The GPU
    /// allocation itself stays persistent so weather transitions only change
    /// uniforms unless resolution changes.
    pub fn volumetric_clouds(&self) -> Option<VolumetricCloudDesc> {
        self.sky.as_ref().map(|sky| sky.volumetric_clouds)
    }

    /// Apply runtime volumetric-cloud weather parameters.
    ///
    /// Enabling the feature after renderer initialization is intentionally
    /// rejected when no volumetric GPU resources were created at startup;
    /// disabling/reconfiguring an already initialized cloud renderer is safe.
    pub fn set_volumetric_clouds(&mut self, desc: VolumetricCloudDesc) -> Result<(), String> {
        validate_volumetric_cloud_desc(&desc)?;

        let gpu_initialized = self.gpu.is_some();
        if gpu_initialized && desc.enabled && self.gpu_volumetric_clouds.is_none() {
            return Err(
                "cannot enable volumetric clouds after renderer initialization when the project started with them disabled"
                    .to_owned(),
            );
        }

        if self.sky.is_none() && !desc.enabled {
            return Ok(());
        }

        let sky = self
            .sky
            .as_mut()
            .ok_or_else(|| "volumetric clouds require configured sky resources".to_owned())?;
        let changed = sky.volumetric_clouds != desc;
        sky.volumetric_clouds = desc;

        if changed {
            if let Some(gpu) = self.gpu_volumetric_clouds.as_mut() {
                // A weather/profile discontinuity must not reproject stale
                // density into the newly selected cloud field.
                gpu.history_valid = false;
            }
        }
        Ok(())
    }

    pub(super) fn initialize_volumetric_cloud_renderer(
        &mut self,
        render: &RenderClient,
    ) -> Result<(), String> {
        let desc = self
            .sky
            .as_ref()
            .map(|sky| sky.volumetric_clouds)
            .ok_or_else(|| "volumetric clouds require sky resources".to_owned())?;
        if !desc.enabled {
            return Ok(());
        }
        if self.gpu_sky.is_none() {
            return Err("volumetric clouds require initialized sky GPU textures".to_owned());
        }

        let fullscreen_vertex_buffer = render.create_buffer(
            "newviso.volumetric_cloud.fullscreen",
            (6 * std::mem::size_of::<f32>()) as u64,
            "Vertex",
            "CpuToGpu",
        )?;
        render.write_buffer_f32(
            fullscreen_vertex_buffer,
            0,
            &[-1.0, -1.0, 3.0, -1.0, -1.0, 3.0],
        )?;

        let mut uniform_buffers = [0u32; SCENE_FRAME_SLOTS];
        let mut depth_uniform_buffers = [0u32; SCENE_FRAME_SLOTS];
        for slot in 0..SCENE_FRAME_SLOTS {
            uniform_buffers[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.volumetric_cloud.uniform.slot.{slot}"),
                (VOLUMETRIC_CLOUD_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                "Uniform",
                "CpuToGpu",
            )?;
            depth_uniform_buffers[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.volumetric_cloud.depth.uniform.slot.{slot}"),
                (ATMOSPHERIC_SOFT_DEPTH_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                "Uniform",
                "CpuToGpu",
            )?;
        }

        let sampler = render.create_sampler_repeat_linear("newviso.volumetric_cloud.sampler")?;

        let raymarch_bind_group_layout = render.create_bind_group_layout(
            "newviso.volumetric_cloud.raymarch.bindings",
            &[
                "UniformBuffer",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Sampler",
            ],
        )?;
        let temporal_bind_group_layout = render.create_bind_group_layout(
            "newviso.volumetric_cloud.temporal.bindings",
            &["UniformBuffer", "Texture2D", "Texture2D", "Sampler"],
        )?;
        let composite_bind_group_layout = render.create_bind_group_layout(
            "newviso.volumetric_cloud.composite.bindings",
            &["Texture2D", "Sampler"],
        )?;
        let depth_bind_group_layout = render.create_bind_group_layout(
            "newviso.volumetric_cloud.depth.bindings",
            &["UniformBuffer"],
        )?;
        let mut depth_bind_groups = [0u32; SCENE_FRAME_SLOTS];
        for slot in 0..SCENE_FRAME_SLOTS {
            depth_bind_groups[slot] = render.create_bind_group(
                &format!("newviso.volumetric_cloud.depth.bind_group.slot.{slot}"),
                depth_bind_group_layout,
                [None, None, None, None],
                None,
                Some((
                    depth_uniform_buffers[slot],
                    0,
                    (ATMOSPHERIC_SOFT_DEPTH_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                )),
            )?;
        }

        let vertex_shader = render.create_shader_spirv(
            "newviso.volumetric_cloud.vertex",
            ShaderStage::Vertex,
            VOLUMETRIC_CLOUD_VERTEX_SHADER,
        )?;
        let raymarch_fragment_shader = render.create_shader_spirv(
            "newviso.volumetric_cloud.raymarch.fragment",
            ShaderStage::Fragment,
            VOLUMETRIC_CLOUD_FRAGMENT_SHADER,
        )?;
        let temporal_fragment_shader = render.create_shader_spirv(
            "newviso.volumetric_cloud.temporal.fragment",
            ShaderStage::Fragment,
            VOLUMETRIC_TEMPORAL_FRAGMENT_SHADER,
        )?;
        let composite_fragment_shader = render.create_shader_spirv(
            "newviso.volumetric_cloud.composite.fragment",
            ShaderStage::Fragment,
            VOLUMETRIC_COMPOSITE_FRAGMENT_SHADER,
        )?;
        let depth_vertex_shader = render.create_shader_spirv(
            "newviso.volumetric_cloud.depth.vertex",
            ShaderStage::Vertex,
            ATMOSPHERIC_DEPTH_VERTEX_SHADER,
        )?;
        let depth_instanced_vertex_shader = render.create_shader_spirv(
            "newviso.volumetric_cloud.depth.instanced.vertex",
            ShaderStage::Vertex,
            ATMOSPHERIC_DEPTH_INSTANCED_VERTEX_SHADER,
        )?;
        let depth_fragment_shader = render.create_shader_spirv(
            "newviso.volumetric_cloud.depth.fragment",
            ShaderStage::Fragment,
            ATMOSPHERIC_DEPTH_FRAGMENT_SHADER,
        )?;

        let fullscreen_attributes = [VertexAttribute {
            location: 0,
            offset: 0,
            format: VertexFormat::Float32x2,
        }];
        let make_fullscreen_pipeline = |label: &'static str,
                                        fs: u32,
                                        layout: u32,
                                        color_format: &'static str,
                                        blend: &'static str,
                                        cache_key: &'static str|
         -> Result<u32, String> {
            render.create_pipeline(GraphicsPipelineDesc {
                label,
                vertex_shader,
                fragment_shader: fs,
                vertex_stride: VOLUMETRIC_FULLSCREEN_VERTEX_STRIDE,
                attributes: &fullscreen_attributes,
                topology: "TriangleList",
                bind_group_layouts: &[layout],
                color_format,
                depth_format: None,
                depth_test: false,
                depth_write: false,
                depth_compare: "Always",
                cull_mode: "None",
                blend_mode: blend,
                cache_key,
            })
        };

        let raymarch_pipeline = make_fullscreen_pipeline(
            "newviso.volumetric_cloud.raymarch.pipeline",
            raymarch_fragment_shader,
            raymarch_bind_group_layout,
            "Rgba16Float",
            "Opaque",
            "newviso.volumetric_cloud.raymarch.v1",
        )?;
        let temporal_pipeline = make_fullscreen_pipeline(
            "newviso.volumetric_cloud.temporal.pipeline",
            temporal_fragment_shader,
            temporal_bind_group_layout,
            "Rgba16Float",
            "Opaque",
            "newviso.volumetric_cloud.temporal.v1",
        )?;
        let composite_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.volumetric_cloud.composite.pipeline",
            vertex_shader,
            fragment_shader: composite_fragment_shader,
            vertex_stride: VOLUMETRIC_FULLSCREEN_VERTEX_STRIDE,
            attributes: &fullscreen_attributes,
            topology: "TriangleList",
            bind_group_layouts: &[composite_bind_group_layout],
            color_format: "Rgba16Float",
            depth_format: Some("Depth32Float"),
            depth_test: false,
            depth_write: false,
            depth_compare: "Always",
            cull_mode: "None",
            blend_mode: "Alpha",
            cache_key: "newviso.volumetric_cloud.composite.v1",
        })?;

        let depth_instanced_fragment_shader = render.create_shader_spirv(
            "newviso.volumetric_cloud.depth.instanced.fragment",
            ShaderStage::Fragment,
            ATMOSPHERIC_DEPTH_INSTANCED_FRAGMENT_SHADER,
        )?;
        let material_bind_group_layout = self
            .gpu
            .as_ref()
            .ok_or("volumetric depth requires scene material bindings")?
            .material_bind_group_layout;
        let depth_attributes = [VertexAttribute {
            location: 0,
            offset: 0,
            format: VertexFormat::Float32x4,
        }];
        let depth_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.volumetric_cloud.depth.pipeline",
            vertex_shader: depth_vertex_shader,
            fragment_shader: depth_fragment_shader,
            vertex_stride: VERTEX_STRIDE,
            attributes: &depth_attributes,
            topology: "TriangleList",
            bind_group_layouts: &[depth_bind_group_layout],
            color_format: "R32Float",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: true,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.volumetric_cloud.depth.dynamic.v1",
        })?;

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
        let depth_asset_attributes = [
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
        let depth_layouts = [
            VertexLayoutDesc {
                stride: VERTEX_STRIDE,
                attributes: &depth_asset_attributes,
                step_mode: VertexStepMode::Vertex,
            },
            VertexLayoutDesc {
                stride: INSTANCE_STRIDE,
                attributes: &instance_attributes,
                step_mode: VertexStepMode::Instance,
            },
        ];
        let depth_instanced_pipeline = render.create_pipeline_with_layouts(
            GraphicsPipelineDesc {
                label: "newviso.volumetric_cloud.depth.instanced.pipeline",
                vertex_shader: depth_instanced_vertex_shader,
                fragment_shader: depth_instanced_fragment_shader,
                vertex_stride: VERTEX_STRIDE,
                attributes: &depth_asset_attributes,
                topology: "TriangleList",
                bind_group_layouts: &[depth_bind_group_layout, material_bind_group_layout],
                color_format: "R32Float",
                depth_format: Some("Depth32Float"),
                depth_test: true,
                depth_write: true,
                depth_compare: "LessOrEqual",
                cull_mode: "None",
                blend_mode: "Opaque",
                cache_key: "newviso.volumetric_cloud.depth.instanced.v2",
            },
            &depth_layouts,
        )?;

        let aspect = 16.0 / 9.0;
        let previous_view_projection = camera_view_projection(&self.camera, aspect);
        self.gpu_volumetric_clouds = Some(GpuVolumetricClouds {
            fullscreen_vertex_buffer,
            uniform_buffers,
            depth_uniform_buffers,
            sampler,
            raymarch_bind_group_layout,
            temporal_bind_group_layout,
            composite_bind_group_layout,
            depth_bind_group_layout,
            depth_bind_groups,
            vertex_shader,
            raymarch_fragment_shader,
            temporal_fragment_shader,
            composite_fragment_shader,
            depth_vertex_shader,
            depth_instanced_vertex_shader,
            depth_fragment_shader,
            depth_instanced_fragment_shader,
            raymarch_pipeline,
            temporal_pipeline,
            composite_pipeline,
            depth_pipeline,
            depth_instanced_pipeline,
            targets: None,
            previous_view_projection,
            history_valid: false,
        });

        host_runtime::info(
            "newviso.scene",
            format!(
                "volumetric cloud GPU ready ray_steps={} light_steps={} height={:.0}..{:.0}m resolution_scale={:.3}",
                desc.ray_steps,
                desc.light_steps,
                desc.base_altitude,
                desc.top_altitude,
                desc.resolution_scale
            ),
        );
        Ok(())
    }

    pub(super) fn ensure_volumetric_cloud_targets(
        &mut self,
        render: &RenderClient,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        // Projects without authored sky/volumetric-cloud resources are valid.
        // Renderer initialization only creates GpuVolumetricClouds when the
        // feature is enabled, so an absent GPU cloud renderer means there is
        // no per-frame target work to perform.
        if self.gpu_volumetric_clouds.is_none() {
            return Ok(());
        }

        let desc = self
            .sky
            .as_ref()
            .map(|sky| sky.volumetric_clouds)
            .ok_or_else(|| {
                "volumetric cloud GPU resources exist without configured sky resources".to_owned()
            })?;
        if !desc.enabled {
            return Ok(());
        }

        let target_width = ((width.max(1) as f32 * desc.resolution_scale).round() as u32).max(64);
        let target_height = ((height.max(1) as f32 * desc.resolution_scale).round() as u32).max(36);

        let (base_noise, detail_noise) = self
            .gpu_sky
            .map(|sky| (sky.base_noise, sky.detail_noise))
            .ok_or_else(|| "volumetric clouds require sky noise textures".to_owned())?;

        let Some(gpu) = self.gpu_volumetric_clouds.as_mut() else {
            return Ok(());
        };
        if gpu
            .targets
            .as_ref()
            .is_some_and(|targets| targets.width == target_width && targets.height == target_height)
        {
            return Ok(());
        }

        if let Some(old) = gpu.targets.take() {
            destroy_volumetric_targets(render, old);
        }

        let scene_depth_target = render.create_render_target(
            "newviso.volumetric_cloud.scene_depth",
            target_width,
            target_height,
            "R32Float",
            Some("Depth32Float"),
        )?;
        let scene_depth_texture = render.render_target_color_texture(scene_depth_target)?;
        let raw_target = render.create_render_target(
            "newviso.volumetric_cloud.raw",
            target_width,
            target_height,
            "Rgba16Float",
            None,
        )?;
        let raw_texture = render.render_target_color_texture(raw_target)?;

        let history_a = render.create_render_target(
            "newviso.volumetric_cloud.history.a",
            target_width,
            target_height,
            "Rgba16Float",
            None,
        )?;
        let history_b = render.create_render_target(
            "newviso.volumetric_cloud.history.b",
            target_width,
            target_height,
            "Rgba16Float",
            None,
        )?;
        let history_a_texture = render.render_target_color_texture(history_a)?;
        let history_b_texture = render.render_target_color_texture(history_b)?;

        let mut raymarch_bind_groups = [0u32; SCENE_FRAME_SLOTS];
        let mut temporal_bind_groups = [[0u32; 2]; SCENE_FRAME_SLOTS];
        for slot in 0..SCENE_FRAME_SLOTS {
            raymarch_bind_groups[slot] = render.create_bind_group(
                &format!("newviso.volumetric_cloud.raymarch.bind_group.slot.{slot}"),
                gpu.raymarch_bind_group_layout,
                [
                    Some(base_noise),
                    Some(detail_noise),
                    Some(scene_depth_texture),
                    None,
                ],
                Some(gpu.sampler),
                Some((
                    gpu.uniform_buffers[slot],
                    0,
                    (VOLUMETRIC_CLOUD_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                )),
            )?;

            temporal_bind_groups[slot][0] = render.create_bind_group(
                &format!("newviso.volumetric_cloud.temporal.a.slot.{slot}"),
                gpu.temporal_bind_group_layout,
                [Some(raw_texture), Some(history_b_texture), None, None],
                Some(gpu.sampler),
                Some((
                    gpu.uniform_buffers[slot],
                    0,
                    (VOLUMETRIC_CLOUD_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                )),
            )?;
            temporal_bind_groups[slot][1] = render.create_bind_group(
                &format!("newviso.volumetric_cloud.temporal.b.slot.{slot}"),
                gpu.temporal_bind_group_layout,
                [Some(raw_texture), Some(history_a_texture), None, None],
                Some(gpu.sampler),
                Some((
                    gpu.uniform_buffers[slot],
                    0,
                    (VOLUMETRIC_CLOUD_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                )),
            )?;
        }
        let composite_a = render.create_bind_group(
            "newviso.volumetric_cloud.composite.a",
            gpu.composite_bind_group_layout,
            [Some(history_a_texture), None, None, None],
            Some(gpu.sampler),
            None,
        )?;
        let composite_b = render.create_bind_group(
            "newviso.volumetric_cloud.composite.b",
            gpu.composite_bind_group_layout,
            [Some(history_b_texture), None, None, None],
            Some(gpu.sampler),
            None,
        )?;

        gpu.targets = Some(GpuVolumetricCloudTargets {
            width: target_width,
            height: target_height,
            scene_depth_target,
            raw_target,
            history_targets: [history_a, history_b],
            raymarch_bind_groups,
            temporal_bind_groups,
            composite_bind_groups: [composite_a, composite_b],
        });
        gpu.history_valid = false;

        host_runtime::info(
            "newviso.scene",
            format!(
                "volumetric cloud targets ready {}x{} from viewport {}x{}",
                target_width, target_height, width, height
            ),
        );
        Ok(())
    }

    pub(super) fn volumetric_cloud_uniform(
        &self,
        full_width: u32,
        full_height: u32,
        low_width: u32,
        low_height: u32,
        previous_view_projection: [f32; 16],
        history_valid: bool,
    ) -> Result<[f32; VOLUMETRIC_CLOUD_UNIFORM_FLOATS], String> {
        let desc = self
            .sky
            .as_ref()
            .map(|sky| sky.volumetric_clouds)
            .ok_or_else(|| "volumetric clouds require sky resources".to_owned())?;
        let aspect = full_width.max(1) as f32 / full_height.max(1) as f32;
        let forward = self.camera.target.sub(self.camera.position).normalized();
        let (right, up, forward) = view_basis(forward, self.camera.up);
        let tan_half_y = (self.camera.fov_y_degrees.to_radians() * 0.5).tan();
        let tan_half_x = tan_half_y * aspect;
        let (sun_direction, sun_intensity) = self.atmospheric_sun();

        let mut out = [0.0_f32; VOLUMETRIC_CLOUD_UNIFORM_FLOATS];
        out[0..4].copy_from_slice(&[
            self.camera.position.x,
            self.camera.position.y,
            self.camera.position.z,
            self.sky_time_seconds,
        ]);
        out[4..8].copy_from_slice(&[right.x, right.y, right.z, tan_half_x]);
        out[8..12].copy_from_slice(&[up.x, up.y, up.z, tan_half_y]);
        out[12..16].copy_from_slice(&[forward.x, forward.y, forward.z, self.frame_index as f32]);
        out[16..20].copy_from_slice(&[
            desc.base_altitude,
            desc.top_altitude,
            desc.max_distance,
            desc.ray_steps as f32,
        ]);
        out[20..24].copy_from_slice(&[
            desc.coverage,
            desc.density,
            desc.shape_scale,
            desc.detail_scale,
        ]);
        out[24..28].copy_from_slice(&[
            desc.detail_strength,
            desc.erosion_strength,
            desc.extinction,
            desc.scattering,
        ]);
        out[28..32].copy_from_slice(&[
            desc.ambient,
            desc.phase_forward,
            desc.powder_strength,
            desc.light_steps as f32,
        ]);
        out[32..36].copy_from_slice(&[
            sun_direction[0],
            sun_direction[1],
            sun_direction[2],
            sun_intensity,
        ]);
        out[36..40].copy_from_slice(&[
            self.sky_atmosphere.cloud_day_shadow[0],
            self.sky_atmosphere.cloud_day_shadow[1],
            self.sky_atmosphere.cloud_day_shadow[2],
            sun_direction[1],
        ]);
        out[40..44].copy_from_slice(&[
            self.sky_atmosphere.cloud_day_light[0],
            self.sky_atmosphere.cloud_day_light[1],
            self.sky_atmosphere.cloud_day_light[2],
            self.sky_atmosphere.silver_lining_strength,
        ]);
        out[44..48].copy_from_slice(&[
            self.sky_atmosphere.cloud_night[0],
            self.sky_atmosphere.cloud_night[1],
            self.sky_atmosphere.cloud_night[2],
            self.sky_atmosphere.tonemap_shoulder,
        ]);
        out[48..52].copy_from_slice(&[
            self.sky_cloud_noise_phase[0],
            self.sky_cloud_noise_phase[1],
            desc.jitter_strength,
            desc.resolution_scale,
        ]);
        out[52..56].copy_from_slice(&[
            low_width.max(1) as f32,
            low_height.max(1) as f32,
            full_width.max(1) as f32,
            full_height.max(1) as f32,
        ]);
        out[56..60].copy_from_slice(&[
            desc.temporal_blend,
            if history_valid { 1.0 } else { 0.0 },
            (desc.base_altitude + desc.top_altitude) * 0.5,
            self.camera.far,
        ]);
        out[60..76].copy_from_slice(&previous_view_projection);
        Ok(out)
    }

    pub(super) fn volumetric_cloud_runtime_state(&self) -> Value {
        let Some(sky) = self.sky.as_ref() else {
            return json!({"configured": false});
        };
        let desc = sky.volumetric_clouds;
        let gpu = self.gpu_volumetric_clouds.as_ref();
        let targets = gpu.and_then(|gpu| gpu.targets.as_ref());
        json!({
            "configured": true,
            "enabled": desc.enabled,
            "base_altitude": desc.base_altitude,
            "top_altitude": desc.top_altitude,
            "thickness": desc.top_altitude - desc.base_altitude,
            "max_distance": desc.max_distance,
            "resolution_scale": desc.resolution_scale,
            "ray_steps": desc.ray_steps,
            "light_steps": desc.light_steps,
            "coverage": desc.coverage,
            "density": desc.density,
            "shape_scale": desc.shape_scale,
            "detail_scale": desc.detail_scale,
            "detail_strength": desc.detail_strength,
            "erosion_strength": desc.erosion_strength,
            "extinction": desc.extinction,
            "scattering": desc.scattering,
            "ambient": desc.ambient,
            "phase_forward": desc.phase_forward,
            "powder_strength": desc.powder_strength,
            "temporal_blend": desc.temporal_blend,
            "jitter_strength": desc.jitter_strength,
            "gpu_ready": gpu.is_some(),
            "history_valid": gpu.is_some_and(|gpu| gpu.history_valid),
            "target_width": targets.map(|targets| targets.width),
            "target_height": targets.map(|targets| targets.height),
        })
    }

    pub(super) fn shutdown_volumetric_cloud_renderer(&mut self, render: &RenderClient) {
        let Some(mut gpu) = self.gpu_volumetric_clouds.take() else {
            return;
        };
        if let Some(targets) = gpu.targets.take() {
            destroy_volumetric_targets(render, targets);
        }
        render.destroy_pipeline(gpu.depth_instanced_pipeline);
        render.destroy_pipeline(gpu.depth_pipeline);
        render.destroy_pipeline(gpu.composite_pipeline);
        render.destroy_pipeline(gpu.temporal_pipeline);
        render.destroy_pipeline(gpu.raymarch_pipeline);

        render.destroy_shader(gpu.depth_instanced_fragment_shader);
        render.destroy_shader(gpu.depth_fragment_shader);
        render.destroy_shader(gpu.depth_instanced_vertex_shader);
        render.destroy_shader(gpu.depth_vertex_shader);
        render.destroy_shader(gpu.composite_fragment_shader);
        render.destroy_shader(gpu.temporal_fragment_shader);
        render.destroy_shader(gpu.raymarch_fragment_shader);
        render.destroy_shader(gpu.vertex_shader);

        for group in gpu.depth_bind_groups {
            render.destroy_bind_group(group);
        }
        render.destroy_bind_group_layout(gpu.depth_bind_group_layout);
        render.destroy_bind_group_layout(gpu.composite_bind_group_layout);
        render.destroy_bind_group_layout(gpu.temporal_bind_group_layout);
        render.destroy_bind_group_layout(gpu.raymarch_bind_group_layout);

        render.destroy_sampler(gpu.sampler);
        for buffer in gpu.depth_uniform_buffers {
            render.destroy_buffer(buffer);
        }
        for buffer in gpu.uniform_buffers {
            render.destroy_buffer(buffer);
        }
        render.destroy_buffer(gpu.fullscreen_vertex_buffer);
    }
}

fn validate_volumetric_cloud_desc(desc: &VolumetricCloudDesc) -> Result<(), String> {
    let invalid = !desc.base_altitude.is_finite()
        || !desc.top_altitude.is_finite()
        || desc.top_altitude <= desc.base_altitude
        || !desc.max_distance.is_finite()
        || !(100.0..=200_000.0).contains(&desc.max_distance)
        || !desc.resolution_scale.is_finite()
        || !(0.125..=1.0).contains(&desc.resolution_scale)
        || !(8..=128).contains(&desc.ray_steps)
        || !(1..=24).contains(&desc.light_steps)
        || !desc.coverage.is_finite()
        || !(0.0..=1.0).contains(&desc.coverage)
        || !desc.density.is_finite()
        || !(0.0..=8.0).contains(&desc.density)
        || !desc.shape_scale.is_finite()
        || !(1.0e-6..=1.0).contains(&desc.shape_scale)
        || !desc.detail_scale.is_finite()
        || !(1.0e-6..=4.0).contains(&desc.detail_scale)
        || !desc.detail_strength.is_finite()
        || !(0.0..=2.0).contains(&desc.detail_strength)
        || !desc.erosion_strength.is_finite()
        || !(0.0..=2.0).contains(&desc.erosion_strength)
        || !desc.extinction.is_finite()
        || !(1.0e-5..=2.0).contains(&desc.extinction)
        || !desc.scattering.is_finite()
        || !(0.0..=8.0).contains(&desc.scattering)
        || !desc.ambient.is_finite()
        || !(0.0..=4.0).contains(&desc.ambient)
        || !desc.phase_forward.is_finite()
        || !(-0.95..=0.95).contains(&desc.phase_forward)
        || !desc.powder_strength.is_finite()
        || !(0.0..=4.0).contains(&desc.powder_strength)
        || !desc.temporal_blend.is_finite()
        || !(0.0..=0.98).contains(&desc.temporal_blend)
        || !desc.jitter_strength.is_finite()
        || !(0.0..=2.0).contains(&desc.jitter_strength);
    if invalid {
        return Err("invalid generic VolumetricCloudDesc parameters".to_owned());
    }
    Ok(())
}

fn destroy_volumetric_targets(render: &RenderClient, targets: GpuVolumetricCloudTargets) {
    render.destroy_bind_group(targets.composite_bind_groups[1]);
    render.destroy_bind_group(targets.composite_bind_groups[0]);
    for groups in targets.temporal_bind_groups {
        render.destroy_bind_group(groups[1]);
        render.destroy_bind_group(groups[0]);
    }
    for group in targets.raymarch_bind_groups {
        render.destroy_bind_group(group);
    }

    render.destroy_render_target(targets.history_targets[1]);
    render.destroy_render_target(targets.history_targets[0]);
    render.destroy_render_target(targets.raw_target);
    render.destroy_render_target(targets.scene_depth_target);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_volume_is_a_real_positive_thickness_layer() {
        let desc = VolumetricCloudDesc::default();
        assert!(desc.top_altitude > desc.base_altitude);
        assert!(desc.ray_steps >= 8);
        assert!(desc.light_steps >= 1);
        assert!(desc.extinction > 0.0);
        assert!(desc.resolution_scale > 0.0 && desc.resolution_scale <= 1.0);
    }
}
