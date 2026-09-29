use super::*;

pub(super) const WEATHER_WORLD_VERTEX_SHADER: &[u8] =
    include_bytes!("../assets/weather_world.vert.spv");
pub(super) const WEATHER_WORLD_FRAGMENT_SHADER: &[u8] =
    include_bytes!("../assets/weather_world.frag.spv");
pub(super) const WEATHER_LENS_VERTEX_SHADER: &[u8] =
    include_bytes!("../assets/weather_lens.vert.spv");
pub(super) const WEATHER_LENS_FRAGMENT_SHADER: &[u8] =
    include_bytes!("../assets/weather_lens.frag.spv");

const WEATHER_MATERIAL_UNIFORM_FLOATS: usize = 8;
const WEATHER_WORLD_UNIFORM_FLOATS: usize = 72;
const WEATHER_LENS_UNIFORM_FLOATS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeatherGpuFxSystemType {
    Drop,
    Mist,
    Ground,
    Other,
}

impl WeatherGpuFxSystemType {
    fn code(self) -> f32 {
        match self {
            Self::Drop => 0.0,
            Self::Mist => 1.0,
            Self::Ground => 2.0,
            Self::Other => 3.0,
        }
    }

    fn base_particle_count(self) -> u32 {
        match self {
            Self::Drop => 1_200,
            Self::Mist => 96,
            Self::Ground => 320,
            Self::Other => 128,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeatherGpuFxEmitterDesc {
    pub box_centre_offset: [f32; 3],
    pub box_size: [f32; 3],
    pub life_min_max: [f32; 2],
    pub velocity_min: [f32; 3],
    pub velocity_max: [f32; 3],
    pub clamp_to_ground: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeatherGpuFxRenderDesc {
    pub texture_rows_cols_start_end: [f32; 4],
    pub texture_anim_rate_scale_over_life: [f32; 4],
    pub size_min_max: [f32; 4],
    pub colour: [f32; 4],
    pub fade_in_out: [f32; 2],
    pub fade_near_far: [f32; 2],
    pub fade_ground_offset: [f32; 4],
    pub rot_speed_min_max: [f32; 2],
    pub directional_z_offset_min_max: [f32; 3],
    pub camera_speed_add: [f32; 3],
    pub edge_softness: f32,
    pub particle_color_percentage: f32,
    pub background_distortion_visibility: f32,
    pub background_distortion_alpha_booster: f32,
    pub background_distortion_amount: f32,
    pub background_distortion_blur_level: f32,
    pub local_lights_multiplier: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeatherGpuFxLayerDesc {
    pub name: String,
    pub system_type: WeatherGpuFxSystemType,
    pub drive_type: String,
    pub wind_influence: f32,
    pub gravity: f32,
    pub diffuse_texture: String,
    pub distortion_texture: Option<String>,
    pub splash_texture: Option<String>,
    pub emitter: WeatherGpuFxEmitterDesc,
    pub render: WeatherGpuFxRenderDesc,
}

#[derive(Clone, Debug)]
pub struct WeatherGpuFxResources {
    pub layers: BTreeMap<String, WeatherGpuFxLayerDesc>,
    /// Canonical logical texture reference -> decoded RGBA8 resource.
    pub textures: BTreeMap<String, SkyTextureResources>,
    pub puddle_layout_texture: String,
    pub puddle_normal_textures: Vec<String>,
    pub lightning_texture: String,
    pub lens_drop_texture: String,
    pub lens_drop_normal_texture: String,
    pub lens_running_normal_texture: String,
}

#[derive(Debug)]
struct GpuWeatherLayer {
    uniform_buffers: [u32; SCENE_FRAME_SLOTS],
    bind_groups: [u32; SCENE_FRAME_SLOTS],
}

#[derive(Debug)]
pub(super) struct GpuWeatherFx {
    source_ready: bool,
    owned_texture_ids: Vec<u32>,
    sampler: u32,

    material_uniform_buffers: [u32; SCENE_FRAME_SLOTS],
    material_bind_group_layout: u32,
    /// Puddle frame -> frame-slot bind groups.
    material_bind_groups: Vec<[u32; SCENE_FRAME_SLOTS]>,

    world_uniform_layout: u32,
    world_layers: BTreeMap<String, GpuWeatherLayer>,
    world_vertex_shader: u32,
    world_fragment_shader: u32,
    world_pipeline: u32,

    lens_uniform_buffers: [u32; SCENE_FRAME_SLOTS],
    lens_bind_group_layout: u32,
    lens_bind_groups: [u32; SCENE_FRAME_SLOTS],
    lens_vertex_shader: u32,
    lens_fragment_shader: u32,
    lens_pipeline: u32,
}

impl Scene3dRuntime {
    pub fn set_weather_gpu_fx_resources(
        &mut self,
        resources: WeatherGpuFxResources,
    ) -> Result<(), String> {
        if self.gpu.is_some() {
            return Err(
                "weather GPU FX resources must be assigned before renderer initialization"
                    .to_owned(),
            );
        }
        validate_weather_resources(&resources)?;
        self.weather_gpu_fx = Some(resources);
        Ok(())
    }

    pub(super) fn update_weather_gpu_fx(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.weather_fx_time_seconds = (self.weather_fx_time_seconds + dt).rem_euclid(86_400.0);

        // One conservative world query per frame. Rain does not render through
        // a resident solid directly above the camera; lens water persists and
        // dries naturally after entering cover.
        self.weather_outdoor_exposure = if self.direction_is_occluded(Vec3::Y) {
            0.0
        } else {
            1.0
        };

        let rain_target = self.weather_effects.rain.clamp(0.0, 1.0) * self.weather_outdoor_exposure;
        if rain_target > self.weather_wetness {
            self.weather_wetness =
                (self.weather_wetness + dt * (0.65 + rain_target * 0.85)).min(rain_target);
        } else {
            // Wet surfaces and lens water outlive the precipitation itself.
            self.weather_wetness = (self.weather_wetness - dt * 0.018).max(rain_target);
        }
    }

    pub(super) fn weather_material_bind_group_layout(&self) -> Result<u32, String> {
        self.gpu_weather
            .as_ref()
            .map(|gpu| gpu.material_bind_group_layout)
            .ok_or_else(|| "weather material GPU resources are not initialized".to_owned())
    }

    pub(super) fn weather_material_bind_group(&self, frame_slot: usize) -> Result<u32, String> {
        let gpu = self
            .gpu_weather
            .as_ref()
            .ok_or_else(|| "weather material GPU resources are not initialized".to_owned())?;
        let frame = self.weather_puddle_frame();
        let groups = gpu
            .material_bind_groups
            .get(frame % gpu.material_bind_groups.len().max(1))
            .ok_or_else(|| "weather material bind-group ring is empty".to_owned())?;
        groups
            .get(frame_slot)
            .copied()
            .ok_or_else(|| "weather material frame slot is invalid".to_owned())
    }

    pub(super) fn weather_material_uniform(&self) -> [f32; WEATHER_MATERIAL_UNIFORM_FLOATS] {
        [
            self.weather_effects.rain.clamp(0.0, 1.0),
            self.weather_wetness.clamp(0.0, 1.0),
            self.weather_lightning_flash(),
            self.weather_fx_time_seconds,
            self.weather_effects.ripple_scale.max(0.0001),
            self.weather_effects.ripple_bumpiness.max(0.0),
            self.weather_effects.wind_speed.max(0.0),
            self.weather_puddle_frame() as f32,
        ]
    }

    pub(super) fn weather_lens_uniform(&self) -> [f32; WEATHER_LENS_UNIFORM_FLOATS] {
        let mut out = [0.0; WEATHER_LENS_UNIFORM_FLOATS];
        out[0..4].copy_from_slice(&[
            self.weather_effects.rain.clamp(0.0, 1.0),
            self.weather_wetness.clamp(0.0, 1.0),
            self.weather_lightning_flash(),
            self.weather_fx_time_seconds,
        ]);
        out[4..8].copy_from_slice(&[
            self.weather_effects.wind_direction[0],
            self.weather_effects.wind_direction[1],
            self.weather_effects.wind_speed.max(0.0),
            self.weather_outdoor_exposure,
        ]);
        out[8] = self.weather_backend.blend.clamp(0.0, 1.0);
        out
    }

    fn weather_puddle_frame(&self) -> usize {
        let count = self
            .weather_gpu_fx
            .as_ref()
            .map(|resources| resources.puddle_normal_textures.len())
            .unwrap_or(1)
            .max(1);
        let fps = self.weather_effects.ripple_speed.abs().max(0.01);
        ((self.weather_fx_time_seconds * fps).floor() as usize) % count
    }

    fn thunder_weight(&self) -> f32 {
        let blend = self.weather_backend.blend.clamp(0.0, 1.0);
        let current = if self
            .weather_backend
            .current
            .to_ascii_uppercase()
            .contains("THUNDER")
        {
            1.0 - blend
        } else {
            0.0
        };
        let next = if self
            .weather_backend
            .next
            .to_ascii_uppercase()
            .contains("THUNDER")
        {
            blend
        } else {
            0.0
        };
        (current + next)
            .max(self.weather_effects.lightning.clamp(0.0, 1.0))
            .clamp(0.0, 1.0)
    }

    fn weather_lightning_flash(&self) -> f32 {
        let thunder = self.thunder_weight();
        if thunder <= 0.0001 {
            return 0.0;
        }

        // Deterministic irregular lightning windows. The authored THUNDER
        // state is the driver; this only supplies the flash cadence that GTA's
        // weather state does not serialize as a scalar in weather.xml.
        let window = (self.weather_fx_time_seconds / 5.75).floor();
        let selector = hash01(window * 17.17 + 3.91);
        if selector < 0.56 {
            return 0.0;
        }
        let phase = self.weather_fx_time_seconds.rem_euclid(5.75);
        let first = (-phase * 22.0).exp();
        let second = (-(phase - 0.115).abs() * 34.0).exp() * 0.62;
        (first.max(second) * thunder).clamp(0.0, 1.0)
    }

    fn weather_driver(&self, layer: &WeatherGpuFxLayerDesc) -> f32 {
        let drive = layer.drive_type.to_ascii_uppercase();
        if drive.contains("RAIN") {
            self.weather_effects.rain
        } else if drive.contains("SNOW_MIST") {
            self.weather_effects.snow_mist
        } else if drive.contains("SNOW") {
            self.weather_effects.snow
        } else if drive.contains("FOG") {
            self.weather_effects.fog
        } else {
            1.0
        }
        .clamp(0.0, 1.0)
    }

    pub(super) fn active_weather_world_layers(&self) -> Vec<(String, f32)> {
        let Some(resources) = self.weather_gpu_fx.as_ref() else {
            return Vec::new();
        };
        if self.weather_outdoor_exposure <= 0.001 {
            return Vec::new();
        }

        let blend = self.weather_backend.blend.clamp(0.0, 1.0);
        let mut weights = BTreeMap::<String, f32>::new();
        let mut add = |name: &str, side_weight: f32| {
            if name.trim().is_empty() || side_weight <= 0.0001 {
                return;
            }
            let Some(layer) = resources.layers.get(name) else {
                return;
            };
            let weight = side_weight * self.weather_driver(layer) * self.weather_outdoor_exposure;
            if weight > 0.0001 {
                *weights.entry(name.to_owned()).or_insert(0.0) += weight;
            }
        };

        for (current, next) in [
            (
                self.weather_effects.current_drop_setting.as_str(),
                self.weather_effects.next_drop_setting.as_str(),
            ),
            (
                self.weather_effects.current_mist_setting.as_str(),
                self.weather_effects.next_mist_setting.as_str(),
            ),
            (
                self.weather_effects.current_ground_setting.as_str(),
                self.weather_effects.next_ground_setting.as_str(),
            ),
        ] {
            add(current, 1.0 - blend);
            add(next, blend);
        }

        weights
            .into_iter()
            .map(|(name, weight)| (name, weight.clamp(0.0, 1.0)))
            .collect()
    }

    fn weather_ground_level(&self) -> f32 {
        let camera = self.camera.position;
        let mut ground = camera.y - 1.65;
        for bounds in self.world.solid_bounds() {
            if camera.x >= bounds.min.x
                && camera.x <= bounds.max.x
                && camera.z >= bounds.min.z
                && camera.z <= bounds.max.z
                && bounds.max.y <= camera.y + 0.35
                && bounds.max.y > ground
            {
                ground = bounds.max.y;
            }
        }
        ground
    }

    pub(super) fn weather_world_uniform(
        &self,
        layer: &WeatherGpuFxLayerDesc,
        intensity: f32,
        aspect: f32,
    ) -> [f32; WEATHER_WORLD_UNIFORM_FLOATS] {
        let mut out = [0.0; WEATHER_WORLD_UNIFORM_FLOATS];
        out[0..16].copy_from_slice(&camera_view_projection(&self.camera, aspect));

        let forward = self.camera.target.sub(self.camera.position).normalized();
        let (right, up, forward) = view_basis(forward, self.camera.up);
        let tan_half_y = (self.camera.fov_y_degrees.to_radians() * 0.5).tan();
        let tan_half_x = tan_half_y * aspect.max(0.0001);

        out[16..20].copy_from_slice(&[
            self.camera.position.x,
            self.camera.position.y,
            self.camera.position.z,
            self.weather_fx_time_seconds,
        ]);
        out[20..24].copy_from_slice(&[right.x, right.y, right.z, tan_half_x]);
        out[24..28].copy_from_slice(&[up.x, up.y, up.z, tan_half_y]);
        out[28..32].copy_from_slice(&[
            forward.x,
            forward.y,
            forward.z,
            self.weather_outdoor_exposure,
        ]);
        out[32..36].copy_from_slice(&[
            layer.emitter.box_centre_offset[0],
            layer.emitter.box_centre_offset[1],
            layer.emitter.box_centre_offset[2],
            layer.system_type.code(),
        ]);
        out[36..40].copy_from_slice(&[
            layer.emitter.box_size[0],
            layer.emitter.box_size[1],
            layer.emitter.box_size[2],
            intensity.clamp(0.0, 1.0),
        ]);
        out[40..44].copy_from_slice(&[
            layer.emitter.life_min_max[0],
            layer.emitter.life_min_max[1],
            layer.gravity,
            layer.wind_influence,
        ]);
        out[44..48].copy_from_slice(&[
            layer.emitter.velocity_min[0],
            layer.emitter.velocity_min[1],
            layer.emitter.velocity_min[2],
            0.0,
        ]);
        out[48..52].copy_from_slice(&[
            layer.emitter.velocity_max[0],
            layer.emitter.velocity_max[1],
            layer.emitter.velocity_max[2],
            0.0,
        ]);
        out[52..56].copy_from_slice(&layer.render.texture_rows_cols_start_end);
        out[56..60].copy_from_slice(&layer.render.size_min_max);
        out[60..64].copy_from_slice(&layer.render.colour);
        out[64..68].copy_from_slice(&[
            layer.render.fade_near_far[0],
            layer.render.fade_near_far[1],
            layer.render.fade_in_out[0],
            layer.render.fade_in_out[1],
        ]);
        out[68..72].copy_from_slice(&[
            self.weather_effects.wind_direction[0],
            self.weather_effects.wind_direction[1],
            self.weather_effects.wind_speed,
            self.weather_ground_level(),
        ]);
        out
    }

    pub(super) fn initialize_weather_gpu_fx_renderer(
        &mut self,
        render: &RenderClient,
    ) -> Result<(), String> {
        if self.gpu_weather.is_some() {
            return Ok(());
        }

        let source_ready = self.weather_gpu_fx.is_some();
        let sampler = render.create_sampler_repeat_linear("newviso.weather.sampler")?;
        let mut texture_ids = BTreeMap::<String, u32>::new();
        let mut owned_texture_ids = Vec::<u32>::new();

        if let Some(resources) = self.weather_gpu_fx.as_ref() {
            for (reference, texture) in &resources.textures {
                let id = upload_sky_texture(
                    render,
                    &format!("newviso.weather.texture.{}", texture.name),
                    texture,
                )?;
                texture_ids.insert(reference.clone(), id);
                owned_texture_ids.push(id);
            }
        }

        let one_pixel = [TextureMipUpload {
            level: 0,
            width: 1,
            height: 1,
            offset: 0,
            byte_len: 4,
        }];
        let white = render.create_texture(
            "newviso.weather.fallback.white",
            1,
            1,
            "Rgba8Srgb",
            &one_pixel,
            &[255, 255, 255, 255],
        )?;
        let neutral_normal = render.create_texture(
            "newviso.weather.fallback.normal",
            1,
            1,
            "Rgba8Unorm",
            &one_pixel,
            &[128, 128, 255, 255],
        )?;
        let transparent = render.create_texture(
            "newviso.weather.fallback.transparent",
            1,
            1,
            "Rgba8Srgb",
            &one_pixel,
            &[0, 0, 0, 0],
        )?;
        owned_texture_ids.extend([white, neutral_normal, transparent]);

        let material_bind_group_layout = render.create_bind_group_layout(
            "newviso.weather.material.bindings",
            &["Texture2D", "Texture2D", "Sampler", "UniformBuffer"],
        )?;
        let mut material_uniform_buffers = [0u32; SCENE_FRAME_SLOTS];
        for slot in 0..SCENE_FRAME_SLOTS {
            material_uniform_buffers[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.weather.material.uniform.slot.{slot}"),
                (WEATHER_MATERIAL_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                "Uniform",
                "CpuToGpu",
            )?;
        }

        let (puddle_layout, puddle_normals) = if let Some(resources) = self.weather_gpu_fx.as_ref()
        {
            let layout = texture_ids
                .get(&resources.puddle_layout_texture)
                .copied()
                .unwrap_or(white);
            let normals = resources
                .puddle_normal_textures
                .iter()
                .filter_map(|reference| texture_ids.get(reference).copied())
                .collect::<Vec<_>>();
            (
                layout,
                if normals.is_empty() {
                    vec![neutral_normal]
                } else {
                    normals
                },
            )
        } else {
            (white, vec![neutral_normal])
        };

        let mut material_bind_groups = Vec::with_capacity(puddle_normals.len());
        for (frame, normal) in puddle_normals.iter().copied().enumerate() {
            let mut groups = [0u32; SCENE_FRAME_SLOTS];
            for slot in 0..SCENE_FRAME_SLOTS {
                groups[slot] = render.create_bind_group(
                    &format!("newviso.weather.material.frame.{frame}.slot.{slot}"),
                    material_bind_group_layout,
                    [Some(puddle_layout), Some(normal), None, None],
                    Some(sampler),
                    Some((
                        material_uniform_buffers[slot],
                        0,
                        (WEATHER_MATERIAL_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                    )),
                )?;
            }
            material_bind_groups.push(groups);
        }

        let world_uniform_layout = render.create_bind_group_layout(
            "newviso.weather.world.bindings",
            &[
                "UniformBuffer",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Sampler",
            ],
        )?;
        let world_vertex_shader = render.create_shader_spirv(
            "newviso.weather.world.vertex",
            ShaderStage::Vertex,
            WEATHER_WORLD_VERTEX_SHADER,
        )?;
        let world_fragment_shader = render.create_shader_spirv(
            "newviso.weather.world.fragment",
            ShaderStage::Fragment,
            WEATHER_WORLD_FRAGMENT_SHADER,
        )?;
        let world_pipeline = render.create_pipeline_with_layouts(
            GraphicsPipelineDesc {
                label: "newviso.weather.world.pipeline",
                vertex_shader: world_vertex_shader,
                fragment_shader: world_fragment_shader,
                vertex_stride: 0,
                attributes: &[],
                topology: "TriangleList",
                bind_group_layouts: &[world_uniform_layout],
                color_format: "Rgba16Float",
                depth_format: Some("Depth32Float"),
                depth_test: true,
                depth_write: false,
                depth_compare: "LessOrEqual",
                cull_mode: "None",
                blend_mode: "Alpha",
                cache_key: "newviso.weather.world.gta_gpu_fx.v1",
            },
            &[],
        )?;

        let mut world_layers = BTreeMap::<String, GpuWeatherLayer>::new();
        if let Some(resources) = self.weather_gpu_fx.as_ref() {
            for (name, layer) in &resources.layers {
                let mut uniform_buffers = [0u32; SCENE_FRAME_SLOTS];
                let mut bind_groups = [0u32; SCENE_FRAME_SLOTS];
                let diffuse = texture_ids
                    .get(&layer.diffuse_texture)
                    .copied()
                    .unwrap_or(white);
                let distortion = layer
                    .distortion_texture
                    .as_ref()
                    .and_then(|reference| texture_ids.get(reference).copied())
                    .unwrap_or(neutral_normal);
                let splash = layer
                    .splash_texture
                    .as_ref()
                    .and_then(|reference| texture_ids.get(reference).copied())
                    .unwrap_or(diffuse);

                for slot in 0..SCENE_FRAME_SLOTS {
                    uniform_buffers[slot] = render.create_frame_buffer(
                        slot,
                        &format!("newviso.weather.world.{name}.uniform.slot.{slot}"),
                        (WEATHER_WORLD_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                        "Uniform",
                        "CpuToGpu",
                    )?;
                    bind_groups[slot] = render.create_bind_group(
                        &format!("newviso.weather.world.{name}.bind.slot.{slot}"),
                        world_uniform_layout,
                        [Some(diffuse), Some(distortion), Some(splash), None],
                        Some(sampler),
                        Some((
                            uniform_buffers[slot],
                            0,
                            (WEATHER_WORLD_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                        )),
                    )?;
                }
                world_layers.insert(
                    name.clone(),
                    GpuWeatherLayer {
                        uniform_buffers,
                        bind_groups,
                    },
                );
            }
        }

        let lens_bind_group_layout = render.create_bind_group_layout(
            "newviso.weather.lens.bindings",
            &[
                "UniformBuffer",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Sampler",
            ],
        )?;
        let mut lens_uniform_buffers = [0u32; SCENE_FRAME_SLOTS];
        let mut lens_bind_groups = [0u32; SCENE_FRAME_SLOTS];
        let (lens_drop, lens_normal, lens_running, lightning) =
            if let Some(resources) = self.weather_gpu_fx.as_ref() {
                (
                    texture_ids
                        .get(&resources.lens_drop_texture)
                        .copied()
                        .unwrap_or(transparent),
                    texture_ids
                        .get(&resources.lens_drop_normal_texture)
                        .copied()
                        .unwrap_or(neutral_normal),
                    texture_ids
                        .get(&resources.lens_running_normal_texture)
                        .copied()
                        .unwrap_or(neutral_normal),
                    texture_ids
                        .get(&resources.lightning_texture)
                        .copied()
                        .unwrap_or(white),
                )
            } else {
                (transparent, neutral_normal, neutral_normal, white)
            };
        for slot in 0..SCENE_FRAME_SLOTS {
            lens_uniform_buffers[slot] = render.create_frame_buffer(
                slot,
                &format!("newviso.weather.lens.uniform.slot.{slot}"),
                (WEATHER_LENS_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                "Uniform",
                "CpuToGpu",
            )?;
            lens_bind_groups[slot] = render.create_bind_group(
                &format!("newviso.weather.lens.bind.slot.{slot}"),
                lens_bind_group_layout,
                [
                    Some(lens_drop),
                    Some(lens_normal),
                    Some(lens_running),
                    Some(lightning),
                ],
                Some(sampler),
                Some((
                    lens_uniform_buffers[slot],
                    0,
                    (WEATHER_LENS_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
                )),
            )?;
        }

        let lens_vertex_shader = render.create_shader_spirv(
            "newviso.weather.lens.vertex",
            ShaderStage::Vertex,
            WEATHER_LENS_VERTEX_SHADER,
        )?;
        let lens_fragment_shader = render.create_shader_spirv(
            "newviso.weather.lens.fragment",
            ShaderStage::Fragment,
            WEATHER_LENS_FRAGMENT_SHADER,
        )?;
        let lens_pipeline = render.create_pipeline_with_layouts(
            GraphicsPipelineDesc {
                label: "newviso.weather.lens.pipeline",
                vertex_shader: lens_vertex_shader,
                fragment_shader: lens_fragment_shader,
                vertex_stride: 0,
                attributes: &[],
                topology: "TriangleList",
                bind_group_layouts: &[lens_bind_group_layout],
                color_format: "Rgba16Float",
                depth_format: Some("Depth32Float"),
                depth_test: false,
                depth_write: false,
                depth_compare: "Always",
                cull_mode: "None",
                blend_mode: "Alpha",
                cache_key: "newviso.weather.lens.gta_ptfx.v1",
            },
            &[],
        )?;

        self.gpu_weather = Some(GpuWeatherFx {
            source_ready,
            owned_texture_ids,
            sampler,
            material_uniform_buffers,
            material_bind_group_layout,
            material_bind_groups,
            world_uniform_layout,
            world_layers,
            world_vertex_shader,
            world_fragment_shader,
            world_pipeline,
            lens_uniform_buffers,
            lens_bind_group_layout,
            lens_bind_groups,
            lens_vertex_shader,
            lens_fragment_shader,
            lens_pipeline,
        });

        host_runtime::info(
            "newviso.scene",
            format!(
                "weather GPU FX ready source={} layers={} puddle_frames={} textures={}",
                source_ready,
                self.weather_gpu_fx.as_ref().map_or(0, |r| r.layers.len()),
                self.weather_gpu_fx
                    .as_ref()
                    .map_or(0, |r| r.puddle_normal_textures.len()),
                self.weather_gpu_fx.as_ref().map_or(0, |r| r.textures.len()),
            ),
        );
        Ok(())
    }

    pub(super) fn upload_weather_uniforms(
        &self,
        render: &RenderClient,
        frame_slot: usize,
        aspect: f32,
    ) -> Result<(), String> {
        let Some(gpu) = self.gpu_weather.as_ref() else {
            return Ok(());
        };

        let material = self.weather_material_uniform();
        render.write_buffer_f32(gpu.material_uniform_buffers[frame_slot], 0, &material)?;
        let lens = self.weather_lens_uniform();
        render.write_buffer_f32(gpu.lens_uniform_buffers[frame_slot], 0, &lens)?;

        let Some(resources) = self.weather_gpu_fx.as_ref() else {
            return Ok(());
        };
        for (name, intensity) in self.active_weather_world_layers() {
            let Some(layer) = resources.layers.get(&name) else {
                continue;
            };
            let Some(gpu_layer) = gpu.world_layers.get(&name) else {
                continue;
            };
            let uniform = self.weather_world_uniform(layer, intensity, aspect);
            render.write_buffer_f32(gpu_layer.uniform_buffers[frame_slot], 0, &uniform)?;
        }
        Ok(())
    }

    pub(super) fn draw_weather_world_fx(
        &self,
        render: &RenderClient,
        frame_slot: usize,
    ) -> Result<(), String> {
        let Some(gpu) = self.gpu_weather.as_ref() else {
            return Ok(());
        };
        let Some(resources) = self.weather_gpu_fx.as_ref() else {
            return Ok(());
        };

        let active = self.active_weather_world_layers();
        if active.is_empty() {
            return Ok(());
        }

        render.set_pipeline(gpu.world_pipeline)?;
        for (name, intensity) in active {
            let Some(layer) = resources.layers.get(&name) else {
                continue;
            };
            let Some(gpu_layer) = gpu.world_layers.get(&name) else {
                continue;
            };
            let count = ((layer.system_type.base_particle_count() as f32)
                * intensity.clamp(0.0, 1.0))
            .ceil()
            .max(1.0) as u32;
            render.set_bind_group(0, gpu_layer.bind_groups[frame_slot])?;
            render.draw(count.saturating_mul(6))?;
        }
        Ok(())
    }

    pub(super) fn draw_weather_lens_fx(
        &self,
        render: &RenderClient,
        frame_slot: usize,
    ) -> Result<(), String> {
        let Some(gpu) = self.gpu_weather.as_ref() else {
            return Ok(());
        };
        if !gpu.source_ready {
            return Ok(());
        }
        let lightning = self.weather_lightning_flash();
        if self.weather_wetness <= 0.002 && lightning <= 0.002 {
            return Ok(());
        }

        render.set_pipeline(gpu.lens_pipeline)?;
        render.set_bind_group(0, gpu.lens_bind_groups[frame_slot])?;
        render.draw(3)
    }

    pub(super) fn weather_runtime_state(&self) -> Value {
        json!({
            "resources": self.weather_gpu_fx.is_some(),
            "gpu_ready": self.gpu_weather.is_some(),
            "source_ready": self.gpu_weather.as_ref().is_some_and(|gpu| gpu.source_ready),
            "time_seconds": self.weather_fx_time_seconds,
            "outdoor_exposure": self.weather_outdoor_exposure,
            "wetness": self.weather_wetness,
            "puddle_frame": self.weather_puddle_frame(),
            "lightning_flash": self.weather_lightning_flash(),
            "active_world_layers": self.active_weather_world_layers()
                .into_iter()
                .map(|(name, weight)| json!({"name": name, "weight": weight}))
                .collect::<Vec<_>>(),
        })
    }

    pub(super) fn shutdown_weather_gpu_fx_renderer(&mut self, render: &RenderClient) {
        let Some(gpu) = self.gpu_weather.take() else {
            return;
        };

        render.destroy_pipeline(gpu.lens_pipeline);
        render.destroy_shader(gpu.lens_fragment_shader);
        render.destroy_shader(gpu.lens_vertex_shader);
        for group in gpu.lens_bind_groups {
            render.destroy_bind_group(group);
        }
        for buffer in gpu.lens_uniform_buffers {
            render.destroy_buffer(buffer);
        }
        render.destroy_bind_group_layout(gpu.lens_bind_group_layout);

        render.destroy_pipeline(gpu.world_pipeline);
        render.destroy_shader(gpu.world_fragment_shader);
        render.destroy_shader(gpu.world_vertex_shader);
        for layer in gpu.world_layers.into_values() {
            for group in layer.bind_groups {
                render.destroy_bind_group(group);
            }
            for buffer in layer.uniform_buffers {
                render.destroy_buffer(buffer);
            }
        }
        render.destroy_bind_group_layout(gpu.world_uniform_layout);

        for groups in gpu.material_bind_groups {
            for group in groups {
                render.destroy_bind_group(group);
            }
        }
        for buffer in gpu.material_uniform_buffers {
            render.destroy_buffer(buffer);
        }
        render.destroy_bind_group_layout(gpu.material_bind_group_layout);

        render.destroy_sampler(gpu.sampler);
        for texture in gpu.owned_texture_ids {
            render.destroy_texture(texture);
        }
    }
}

fn validate_weather_resources(resources: &WeatherGpuFxResources) -> Result<(), String> {
    if resources.puddle_normal_textures.is_empty() {
        return Err("weather GPU FX require at least one puddle normal texture".to_owned());
    }

    let required = [
        resources.puddle_layout_texture.as_str(),
        resources.lightning_texture.as_str(),
        resources.lens_drop_texture.as_str(),
        resources.lens_drop_normal_texture.as_str(),
        resources.lens_running_normal_texture.as_str(),
    ];
    for reference in required
        .into_iter()
        .chain(resources.puddle_normal_textures.iter().map(String::as_str))
    {
        if !resources.textures.contains_key(reference) {
            return Err(format!(
                "weather GPU FX texture resource is missing decoded payload '{reference}'"
            ));
        }
    }

    for (name, layer) in &resources.layers {
        if name.trim().is_empty() || layer.name.trim().is_empty() {
            return Err("weather GPU FX layer has an empty name".to_owned());
        }
        if !layer.wind_influence.is_finite()
            || !layer.gravity.is_finite()
            || layer
                .emitter
                .box_centre_offset
                .iter()
                .chain(layer.emitter.box_size.iter())
                .chain(layer.emitter.life_min_max.iter())
                .chain(layer.emitter.velocity_min.iter())
                .chain(layer.emitter.velocity_max.iter())
                .chain(layer.render.texture_rows_cols_start_end.iter())
                .chain(layer.render.size_min_max.iter())
                .chain(layer.render.colour.iter())
                .chain(layer.render.fade_near_far.iter())
                .any(|value| !value.is_finite())
        {
            return Err(format!(
                "weather GPU FX layer '{name}' contains non-finite data"
            ));
        }
        for reference in [
            Some(layer.diffuse_texture.as_str()),
            layer.distortion_texture.as_deref(),
            layer.splash_texture.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if !resources.textures.contains_key(reference) {
                return Err(format!(
                    "weather GPU FX layer '{name}' references missing texture '{reference}'"
                ));
            }
        }
    }
    Ok(())
}

fn hash01(value: f32) -> f32 {
    let x = (value.sin() * 43_758.547).fract();
    if x < 0.0 {
        x + 1.0
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_system_type_codes_match_shader_contract() {
        assert_eq!(WeatherGpuFxSystemType::Drop.code(), 0.0);
        assert_eq!(WeatherGpuFxSystemType::Mist.code(), 1.0);
        assert_eq!(WeatherGpuFxSystemType::Ground.code(), 2.0);
    }

    #[test]
    fn weather_hash_is_normalized() {
        for value in [-100.0, -1.0, 0.0, 1.0, 12345.0] {
            let h = hash01(value);
            assert!((0.0..1.0).contains(&h));
        }
    }
}
