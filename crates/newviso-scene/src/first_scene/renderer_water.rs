//! Dynamic water optics. Targets are sampled only after their render pass ends.
use super::*;

pub(super) fn water_optics_diagnostic() -> i32 {
    static MODE: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("NEWVISO_WATER_DIAG_OPTICS").as_deref() {
        Ok("reflection") => 1,
        Ok("refraction") => 2,
        Ok("depth") => 3,
        Ok("scene") => 4,
        _ => 0,
    })
}

const WATER_OPTICS_FLOATS: usize = 20;

#[derive(Debug)]
pub(super) struct WaterGpu {
    surface_pipeline: u32,
    shore_pipeline: u32,
    shore_shader: u32,
    surface_vertex_shader: u32,
    surface_vertices: u32,
    surface_vertex_count: u32,
    surface_indices: u32,
    surface_index_count: u32,
    field_buffers: [u32; SCENE_FRAME_SLOTS],
    field_uploaded_revision: [u64; SCENE_FRAME_SLOTS],
    field_upload_data: Vec<f32>,
    bumps: renderer_water_bump::WaterBumpGpu,
    underwater: renderer_water_underwater::UnderwaterGpu,
    fragment_shader: u32,
    depth_shader: u32,
    depth_pipeline: u32,
    depth_asset_pipeline: u32,
    optical_layout: u32,
    optical_uniforms: [u32; SCENE_FRAME_SLOTS],
    reflection_uniforms: [u32; SCENE_FRAME_SLOTS],
    reflection_groups: [u32; SCENE_FRAME_SLOTS],
    sky_uniforms: [u32; SCENE_FRAME_SLOTS],
    sky_groups: [u32; SCENE_FRAME_SLOTS],
    sampler: u32,
    targets: Option<WaterTargets>,
}

#[derive(Debug)]
pub(super) struct WaterTargets {
    width: u32,
    height: u32,
    reflection: u32,
    refraction: u32,
    depth: u32,
    textures: [u32; 3],
    groups: [[u32; SCENE_FRAME_SLOTS]; 2],
}

pub(super) fn reflect_camera(mut camera: Camera, level: f32) -> Camera {
    camera.position.y = 2.0 * level - camera.position.y;
    camera.target.y = 2.0 * level - camera.target.y;
    camera.up.y = -camera.up.y;
    camera
}

fn attributes() -> [VertexAttribute; 5] {
    [
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
    ]
}

impl Scene3dRuntime {
    pub fn water_simulation_mut(&mut self) -> &mut crate::water::WaterSimulation {
        self.update_water_tuning();
        let p = self.camera.position;
        self.water_simulation.track_focus([p.x, p.z]);
        &mut self.water_simulation
    }

    pub fn configure_water_surface(
        &mut self,
        surface: Option<crate::water::WaterSurface>,
    ) -> Result<(), String> {
        if let Some(surface) = surface {
            surface.validate().map_err(str::to_owned)?;
            self.water_simulation.surface = surface;
            self.water_simulation.tuning.ocean_amplitude = surface.amplitude;
            self.water_simulation.enabled = true;
        } else {
            self.water_simulation.enabled = false;
            self.water_simulation.driven_by_physics = false;
        }
        Ok(())
    }

    pub fn water_sample(&self, position: [f32; 2]) -> crate::water::WaterSample {
        self.water_simulation.sample(position)
    }

    pub fn water_state_json(&self) -> Value {
        let camera = self.camera.position;
        let sample = self.water_sample([camera.x, camera.z]);
        let shore = self.water_simulation.closest_shore([camera.x, camera.z])
            .map(|(point, direction, wave)| json!({
                "position":[point[0],self.water_simulation.surface.level,point[1]],
                "direction":[direction[0],0.0,direction[1]],
                "distance":(point[0]-camera.x).hypot(point[1]-camera.z),
                "height":wave.height,"vertical_velocity":wave.vertical_velocity,
                "foam":self.water_simulation.foam_at(point)
            }));
        let immersion = self.water_simulation.immersion;
        json!({"enabled":self.water_simulation.enabled,
            "camera":{"position":[camera.x,camera.y,camera.z],
                "depth":immersion.depth,"submerged":immersion.submerged,
                "blend":immersion.blend},
            "nearest_shore":shore,
            "solver":if self.water_simulation.source_loaded(){"dynamic_heightfield"}else{"analytic"},
            "seconds":self.water_simulation.seconds(),"grid_size":crate::water::GRID_SIZE,
            "cell_metres":crate::water::CELL_METRES,"origin":self.water_simulation.origin(),
            "surface_geometry":self.water_gpu.as_ref().map(|g|json!({
                "unique_vertices":g.surface_vertex_count,"indices":g.surface_index_count,
                "indexed":g.surface_indices!=0})) ,
            "wave_quad_count":self.water_simulation.region_counts()[0],
            "calming_quad_count":self.water_simulation.region_counts()[1],
            "level":self.water_simulation.surface.level,
            "focus_sample":{"height":sample.height,"normal":sample.normal,
                "vertical_velocity":sample.vertical_velocity},
            "planar_reflections_enabled":self.water_gpu.as_ref().and_then(|g|g.targets.as_ref()).is_some(),
            "refraction_enabled":self.water_gpu.as_ref().and_then(|g|g.targets.as_ref()).is_some()})
    }

    pub(super) fn update_water_tuning(&mut self) {
        let state = &self.weather_effects;
        if state.ocean_wave_max_amplitude > 0.0 {
            // Reference Lerp uses (factor, start, end).
            let wind = state.wind_speed.clamp(0.0, 1.0);
            let wind_scale = state.ocean_wave_wind_scale.clamp(0.0, 1.0);
            self.water_simulation.tuning.ocean_amplitude =
                (state.ocean_wave_amplitude * (1.0 - wind_scale + wind_scale * wind)).clamp(
                    state.ocean_wave_min_amplitude.max(0.0),
                    state
                        .ocean_wave_max_amplitude
                        .max(state.ocean_wave_min_amplitude),
                );
            let shore_wind_scale = state.shore_wave_wind_scale.clamp(0.0, 1.0);
            self.water_simulation.tuning.shore_wave_amplitude = (state.shore_wave_amplitude
                * (1.0 - shore_wind_scale + shore_wind_scale * wind))
                .clamp(
                    state.shore_wave_min_amplitude.max(0.0),
                    state
                        .shore_wave_max_amplitude
                        .max(state.shore_wave_min_amplitude)
                        .max(0.0),
                );
            self.water_simulation.tuning.noise_amplitude = state.ocean_noise_min_amplitude.max(0.0);
            self.water_simulation.tuning.ripple_scale = state.ripple_scale.max(0.001);
            self.water_simulation.tuning.ripple_bumpiness = (state.ripple_bumpiness
                * (1.0 - state.ripple_bumpiness_wind_scale.clamp(0.0, 1.0)
                    + state.ripple_bumpiness_wind_scale.clamp(0.0, 1.0) * wind))
                .clamp(
                    state.ripple_min_bumpiness.max(0.0),
                    state.ripple_max_bumpiness.max(state.ripple_min_bumpiness),
                );
            self.water_simulation.tuning.ocean_bumpiness = state.ocean_bumpiness.max(0.0);
            self.water_simulation.tuning.foam_intensity = state.ocean_foam_intensity;
            self.water_simulation.tuning.foam_scale = state.ocean_foam_scale.max(0.001);
        }
    }

    pub(super) fn initialize_water_renderer(
        &mut self,
        render: &RenderClient,
    ) -> Result<(), String> {
        if self.water_render_slots.is_empty() {
            return Ok(());
        }
        for slot in &self.water_render_slots {
            if let Some(mesh) = self.terrain_meshes.get_mut(slot) {
                mesh.collides = false;
            }
        }
        let ocean = host_runtime::call_service(
            ASSET_SERVICE,
            "asset.raw_bytes_v1",
            b"textures/environment/water/ocean_forcing.dds",
        )
        .map_err(|e| format!("ocean forcing: {e}"))?;
        let shore = host_runtime::call_service(
            ASSET_SERVICE,
            "asset.raw_bytes_v1",
            b"textures/environment/water/shore_forcing.dds",
        )
        .map_err(|e| format!("shore forcing: {e}"))?;
        self.water_simulation.load_sources(&ocean, &shore)?;
        let gpu = self.gpu.ok_or("water requires scene GPU")?;
        let optical_layout = render.create_bind_group_layout(
            "newviso.water.optics",
            &[
                "UniformBuffer",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Sampler",
                "StorageBuffer",
            ],
        )?;
        let optical_uniforms = create_frame_buffer_ring(
            render,
            "newviso.water.optics",
            (WATER_OPTICS_FLOATS * 4) as u64,
            "Uniform",
        )?;
        let reflection_uniforms = create_frame_buffer_ring(
            render,
            "newviso.water.reflected_frame",
            (SCENE_FRAME_UNIFORM_FLOATS * 4) as u64,
            "Uniform",
        )?;
        let mut reflection_groups = [0; SCENE_FRAME_SLOTS];
        let shadow = render.render_target_color_texture(gpu.shadow_render_target)?;
        for slot in 0..SCENE_FRAME_SLOTS {
            reflection_groups[slot] = render.create_bind_group(
                "newviso.water.reflected_frame",
                gpu.bind_group_layout,
                [Some(shadow), None, None, None],
                Some(gpu.shadow_sampler),
                Some((
                    reflection_uniforms[slot],
                    0,
                    (SCENE_FRAME_UNIFORM_FLOATS * 4) as u64,
                )),
            )?;
        }
        let mut sky_uniforms = [0; SCENE_FRAME_SLOTS];
        let mut sky_groups = [0; SCENE_FRAME_SLOTS];
        if let Some(sky) = self.gpu_sky {
            sky_uniforms = create_frame_buffer_ring(
                render,
                "newviso.water.reflected_sky",
                (SKY_UNIFORM_FLOATS * 4) as u64,
                "Uniform",
            )?;
            for slot in 0..SCENE_FRAME_SLOTS {
                sky_groups[slot] = render.create_bind_group(
                    "newviso.water.reflected_sky",
                    sky.bind_group_layout,
                    [
                        Some(sky.base_noise),
                        Some(sky.starfield),
                        Some(sky.detail_noise),
                        Some(sky.billboard_texture.unwrap_or(sky.detail_noise)),
                    ],
                    Some(sky.sampler),
                    Some((sky_uniforms[slot], 0, (SKY_UNIFORM_FLOATS * 4) as u64)),
                )?;
            }
        }
        let surface_vertex_shader = render.create_shader_spirv(
            "newviso.water.vertex",
            ShaderStage::Vertex,
            include_bytes!("../assets/water.vert.spv"),
        )?;
        let field_bytes = (crate::water::GRID_SIZE * crate::water::GRID_SIZE * 16) as u64;
        let field_buffers =
            create_frame_buffer_ring(render, "newviso.water.heightfield", field_bytes, "Storage")?;
        let material = self
            .water_gpu_material
            .as_ref()
            .ok_or("water requires original textures")?;
        let bumps = renderer_water_bump::WaterBumpGpu::new(render, material.textures[4])?;
        let underwater =
            renderer_water_underwater::UnderwaterGpu::new(render, material, gpu.material_sampler)?;
        let mut mesh_vertices = Vec::new();
        for slot in &self.water_render_slots {
            let cube = &self.cubes[*slot];
            let start = mesh_vertices.len();
            if let Some(mesh) = self.terrain_meshes.get(slot) {
                mesh_vertices.extend_from_slice(&mesh.vertices);
                for vertex in mesh_vertices[start..].chunks_exact_mut(FLOATS_PER_VERTEX) {
                    vertex[0] -= cube.position.x;
                    vertex[2] -= cube.position.z;
                    vertex[3] = -1.0;
                }
            } else {
                cube.append_vertices(&mut mesh_vertices);
            }
        }
        let expanded_vertex_count = mesh_vertices.len() / FLOATS_PER_VERTEX;
        let (mesh_vertices, indices) = if std::env::var("NEWVISO_LINEAR_WATER").as_deref() == Ok("1") {
            (mesh_vertices, Vec::new())
        } else {
            renderer_water_geometry::index_water_vertices(&mesh_vertices)?
        };
        let surface_vertex_count = u32::try_from(mesh_vertices.len() / FLOATS_PER_VERTEX)
            .map_err(|_| "water vertex count exceeds u32".to_owned())?;
        let surface_index_count = u32::try_from(indices.len())
            .map_err(|_| "water index count exceeds u32".to_owned())?;
        let surface_vertices = render.create_buffer(
            "newviso.water.static_grid",
            (mesh_vertices.len() * 4) as u64,
            "Vertex",
            "CpuToGpu",
        )?;
        let surface_indices = if indices.is_empty() { 0 } else {
            match render.create_buffer("newviso.water.static_grid.indices",
                (indices.len() * 4) as u64, "Index", "CpuToGpu") {
                Ok(buffer) => buffer,
                Err(error) => { render.destroy_buffer(surface_vertices); return Err(error); }
            }
        };
        let upload = (|| {
            render.write_buffer_f32(surface_vertices, 0, &mesh_vertices)?;
            if surface_indices != 0 {
                let mut bytes = Vec::with_capacity(indices.len() * 4);
                for index in &indices { bytes.extend_from_slice(&index.to_le_bytes()); }
                render.write_buffer(surface_indices, 0, &bytes)?;
            }
            Ok::<(), String>(())
        })();
        if let Err(error) = upload {
            render.destroy_buffer(surface_vertices);
            if surface_indices != 0 { render.destroy_buffer(surface_indices); }
            return Err(error);
        }
        host_runtime::debug("newviso.render.water", format!(
            "expanded_vertices={expanded_vertex_count} unique_vertices={surface_vertex_count} indices={surface_index_count}"));
        let fragment_shader = render.create_shader_spirv(
            "newviso.water.surface",
            ShaderStage::Fragment,
            include_bytes!("../assets/water.frag.spv"),
        )?;
        let depth_shader = render.create_shader_spirv(
            "newviso.water.depth",
            ShaderStage::Fragment,
            include_bytes!("../assets/water_depth.frag.spv"),
        )?;
        let attrs = attributes();
        let weather_layout = self.weather_material_bind_group_layout()?;
        let shore_shader = render.create_shader_spirv(
            "newviso.water.shore",
            ShaderStage::Fragment,
            include_bytes!("../assets/water_shore.frag.spv"),
        )?;
        let shore_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            depth_write: false,
            blend_mode: "Alpha",
            ..scene_pipeline_desc(
                "newviso.water.shore",
                "newviso.water.shore.wet.v1",
                gpu.vertex_shader,
                shore_shader,
                &attrs,
                &[
                    gpu.bind_group_layout,
                    gpu.material_bind_group_layout,
                    weather_layout,
                    optical_layout,
                ],
            )
        })?;
        let surface_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            // The surface occludes above-water alpha geometry from below,
            // and submerged alpha geometry from above. Optical targets retain
            // the original scene (including particles) on the other side.
            depth_write: true,
            ..scene_pipeline_desc(
                "newviso.water.surface",
                "newviso.water.surface.dynamic.optics.v2",
                surface_vertex_shader,
                fragment_shader,
                &attrs,
                &[
                    gpu.bind_group_layout,
                    gpu.material_bind_group_layout,
                    weather_layout,
                    optical_layout,
                ],
            )
        })?;
        let depth_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            color_format: "R32Float",
            ..scene_pipeline_desc(
                "newviso.water.scene_depth",
                "newviso.water.scene_depth.v1",
                gpu.vertex_shader,
                depth_shader,
                &attrs,
                &[gpu.bind_group_layout, gpu.material_bind_group_layout],
            )
        })?;
        let instance_attrs = [
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
        let layouts = [
            VertexLayoutDesc {
                stride: VERTEX_STRIDE,
                attributes: &attrs,
                step_mode: VertexStepMode::Vertex,
            },
            VertexLayoutDesc {
                stride: INSTANCE_STRIDE,
                attributes: &instance_attrs,
                step_mode: VertexStepMode::Instance,
            },
        ];
        let depth_asset_pipeline = render.create_pipeline_with_layouts(
            GraphicsPipelineDesc {
                color_format: "R32Float",
                ..scene_pipeline_desc(
                    "newviso.water.scene_depth.instanced",
                    "newviso.water.scene_depth.instanced.v1",
                    gpu.asset_vertex_shader,
                    depth_shader,
                    &attrs,
                    &[gpu.bind_group_layout, gpu.material_bind_group_layout],
                )
            },
            &layouts,
        )?;
        self.water_gpu = Some(WaterGpu {
            surface_pipeline,
            shore_pipeline,
            shore_shader,
            surface_vertex_shader,
            surface_vertices,
            surface_vertex_count,
            surface_indices,
            surface_index_count,
            field_buffers,
            field_uploaded_revision: [u64::MAX; SCENE_FRAME_SLOTS],
            field_upload_data: Vec::new(),
            bumps,
            underwater,
            fragment_shader,
            depth_shader,
            depth_pipeline,
            depth_asset_pipeline,
            optical_layout,
            optical_uniforms,
            reflection_uniforms,
            reflection_groups,
            sky_uniforms,
            sky_groups,
            sampler: render.create_sampler_clamp_linear("newviso.water.screen_sampler")?,
            targets: None,
        });
        Ok(())
    }

    pub(super) fn ensure_water_targets(
        &mut self,
        render: &RenderClient,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let Some(gpu) = self.water_gpu.as_mut() else {
            return Ok(());
        };
        let w = (width / 2).max(1);
        let h = (height / 2).max(1);
        if gpu
            .targets
            .as_ref()
            .is_some_and(|t| t.width == w && t.height == h)
        {
            return Ok(());
        }
        let reflection = render.create_render_target(
            "newviso.water.planar_reflection",
            w,
            h,
            "Rgba16Float",
            Some("Depth32Float"),
        )?;
        let refraction = render.create_render_target(
            "newviso.water.refraction",
            w,
            h,
            "Rgba16Float",
            Some("Depth32Float"),
        )?;
        let depth = render.create_render_target(
            "newviso.water.refraction_depth",
            w,
            h,
            "R32Float",
            Some("Depth32Float"),
        )?;
        let textures = [
            render.render_target_color_texture(reflection)?,
            render.render_target_color_texture(refraction)?,
            render.render_target_color_texture(depth)?,
        ];
        let mut groups = [[0; SCENE_FRAME_SLOTS]; 2];
        for bump in 0..2 {
            for slot in 0..SCENE_FRAME_SLOTS {
                groups[bump][slot] = render.create_bind_group_with_storage(
                    "newviso.water.optics",
                    gpu.optical_layout,
                    [
                        Some(textures[0]),
                        Some(textures[1]),
                        Some(textures[2]),
                        Some(gpu.bumps.textures[bump]),
                    ],
                    Some(gpu.sampler),
                    (
                        gpu.optical_uniforms[slot],
                        0,
                        (WATER_OPTICS_FLOATS * 4) as u64,
                    ),
                    (
                        gpu.field_buffers[slot],
                        0,
                        (crate::water::GRID_SIZE * crate::water::GRID_SIZE * 16) as u64,
                    ),
                )?;
            }
        }
        gpu.underwater.set_geometry_depth(render, textures[2])?;
        if let Some(old) = gpu.targets.replace(WaterTargets {
            width: w,
            height: h,
            reflection,
            refraction,
            depth,
            textures,
            groups,
        }) {
            for ring in old.groups {
                for group in ring {
                    render.destroy_bind_group(group);
                }
            }
            for target in [old.reflection, old.refraction, old.depth] {
                render.destroy_render_target(target);
            }
        }
        Ok(())
    }

    pub(super) fn upload_water_uniforms(
        &mut self,
        render: &RenderClient,
        slot: usize,
        aspect: f32,
    ) -> Result<(), String> {
        let Some(gpu) = self.water_gpu.as_ref() else {
            return Ok(());
        };
        let uniforms = gpu.reflection_uniforms[slot];
        let optics = gpu.optical_uniforms[slot];
        let sky_uniform = gpu.sky_uniforms[slot];
        let reflected = reflect_camera(self.camera.clone(), self.water_simulation.surface.level);
        let matrix = camera_view_projection(&reflected, aspect);
        // The shadow texture was rendered with the main camera's light selection
        // and focus. Keep that contract while replacing only the viewing camera.
        let (mut frame, _) = self.scene_frame_uniform(aspect, self.render_policy.shadow_resolution);
        frame[..16].copy_from_slice(&matrix);
        let camera_base = 40 + MAX_LIGHTS * 4 * 5;
        frame[camera_base..camera_base + 3].copy_from_slice(&[
            reflected.position.x,
            reflected.position.y,
            reflected.position.z,
        ]);
        let camera = std::mem::replace(&mut self.camera, reflected);
        frame[SCENE_FRAME_UNIFORM_FLOATS - 4] = if self.water_simulation.immersion.depth > 0.0 {
            -1.0
        } else {
            1.0
        };
        let sky_frame = if sky_uniform != 0 {
            Some(self.sky_frame_uniform(aspect))
        } else {
            None
        };
        self.camera = camera;
        render.write_buffer_f32(uniforms, 0, &frame)?;
        if let Some(sky_frame) = sky_frame {
            render.write_buffer_f32(sky_uniform, 0, &sky_frame)?;
        }
        let mut block = [0.0; WATER_OPTICS_FLOATS];
        block[..16].copy_from_slice(&matrix);
        // Main viewport dimensions; half-resolution textures use normalized UV.
        block[16] = if water_optics_diagnostic() == 0 {
            self.camera.near
        } else {
            -(water_optics_diagnostic() as f32)
        };
        block[17] = self.camera.far;
        block[18] = self.water_simulation.immersion.depth;
        block[19] = self.water_simulation.optics.refraction_index;
        gpu.underwater.upload(self, render, slot, aspect)?;
        render.write_buffer_f32(optics, 0, &block)?;
        let revision = self.water_simulation.field_revision();
        if let Some(gpu) = self.water_gpu.as_mut() {
            if gpu.field_uploaded_revision[slot] != revision {
                self.water_simulation.fill_gpu_field(&mut gpu.field_upload_data);
                render.write_buffer_f32(gpu.field_buffers[slot], 0, &gpu.field_upload_data)?;
                gpu.field_uploaded_revision[slot] = revision;
            }
        }
        Ok(())
    }

    /// These passes use uncullled resident batches. Main-camera Hi-Z is invalid
    /// for a mirrored camera and must never remove reflection geometry.
    pub(super) fn prepare_water_optics(
        &mut self,
        render: &RenderClient,
        gpu: GpuScene,
        slot: usize,
        batches: &[GpuResidentInstanceBatch],
        particle_batches: &[ParticleDrawBatch],
        weather: u32,
    ) -> Result<(), String> {
        if self.water_simulation.enabled {
            if let Some(water) = self.water_gpu.as_mut() {
                water.bumps.update(
                    render,
                    slot,
                    self.water_simulation.seconds(),
                    &self.weather_effects,
                )?;
            }
        }
        render.record_unit_batch(|render|
            self.record_water_optics(render, gpu, slot, batches, particle_batches, weather))
    }

    fn record_water_optics(
        &self,
        render: &RenderClient,
        gpu: GpuScene,
        slot: usize,
        batches: &[GpuResidentInstanceBatch],
        particle_batches: &[ParticleDrawBatch],
        weather: u32,
    ) -> Result<(), String> {
        let Some(water) = self.water_gpu.as_ref() else {
            return Ok(());
        };
        let Some(targets) = water.targets.as_ref() else {
            return Ok(());
        };
        if !self.water_simulation.enabled {
            return Ok(());
        }
        render.set_render_phase(None)?;
        render.set_draw_list_kind(None)?;
        for mode in 0..3 {
            let target = [targets.reflection, targets.refraction, targets.depth][mode];
            let group = if mode == 0 {
                water.reflection_groups[slot]
            } else {
                gpu.bind_groups[slot]
            };
            let clear = if mode == 2 {
                [self.camera.far * 2.0, 0.0, 0.0, 0.0]
            } else {
                self.clear_color
            };
            render.begin_render_target(target, Some(clear), Some(1.0))?;
            render.set_viewport(targets.width, targets.height)?;
            render.set_scissor(targets.width, targets.height)?;
            if mode != 2 {
                if let Some(sky) = self.gpu_sky {
                    render.set_pipeline(sky.pipeline)?;
                    render.set_bind_group(
                        0,
                        if mode == 0 {
                            water.sky_groups[slot]
                        } else {
                            sky.bind_groups[slot]
                        },
                    )?;
                    render.set_vertex_buffer(0, sky.vertex_buffer, 0)?;
                    render.set_index_buffer(sky.index_buffer, 0, sky.index_format)?;
                    render.draw_indexed(sky.index_count)?;
                }
            }
            render.set_pipeline(if mode == 2 {
                water.depth_pipeline
            } else {
                gpu.pipeline
            })?;
            render.set_bind_group(0, group)?;
            render.set_bind_group(1, gpu.default_material_bind_group)?;
            if mode != 2 {
                render.set_bind_group(2, weather)?;
            }
            render.set_vertex_buffer(0, gpu.vertex_buffers[slot], 0)?;
            let mut cursor = 0;
            for render_slot in &self.primitive_main_render_slots() {
                if self.water_render_slots.contains(render_slot) {
                    continue;
                }
                let count = self
                    .terrain_meshes
                    .get(render_slot)
                    .map(|m| (m.vertices.len() / FLOATS_PER_VERTEX) as u32)
                    .unwrap_or(CUBE_VERTEX_COUNT);
                if !self.water_render_slots.contains(render_slot) {
                    let material = if self.terrain_meshes.contains_key(render_slot) {
                        self.terrain_gpu_material
                            .as_ref()
                            .map(|m| m.bind_group)
                            .unwrap_or(gpu.default_material_bind_group)
                    } else {
                        gpu.default_material_bind_group
                    };
                    render.set_bind_group(1, material)?;
                    render.draw_range(count, cursor)?;
                }
                cursor += count;
            }
            render.set_pipeline(if mode == 2 {
                water.depth_asset_pipeline
            } else {
                gpu.asset_pipeline
            })?;
            render.set_bind_group(0, group)?;
            if mode != 2 {
                render.set_bind_group(2, weather)?;
            }
            render.set_vertex_buffer(1, gpu.asset_instance_buffers[slot], 0)?;
            render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;
            let mut bindings = MaterialVertexBindings::new(None, None);
            let mut alpha_draws = Vec::<AlphaAssetDraw>::new();
            let optical_camera = if mode == 0 {
                reflect_camera(self.camera.clone(), self.water_simulation.surface.level)
            } else {
                self.camera.clone()
            };
            // A player and an NPC may share a static batch. Split only batches
            // containing camera-local instances, preserving all world instances.
            for (batch, draw_member) in batches.iter().flat_map(|batch| {
                let split = batch.instance_count > 1 && batch.stable_ids.iter()
                    .any(|id| !self.asset_reflections_visible(*id));
                let count = if split { batch.stable_ids.len() } else { 1 };
                (0..count).map(move |member| (batch, split.then_some(member)))
            }) {
                let Some(id) = batch.stable_ids.get(draw_member.unwrap_or(0)) else {
                    continue;
                };
                let Some(mesh) = self.asset_meshes.get(id) else {
                    continue;
                };
                if !self.asset_reflections_visible(*id) {
                    continue;
                }
                let instance_count = if draw_member.is_some() { 1 } else { batch.instance_count };
                let first_instance = batch.first_instance + draw_member.unwrap_or(0) as u32;
                let (buffer, offset) = asset_mesh_vertex_binding(mesh, gpu, slot)?;
                bindings.bind_vertex(render, buffer)?;
                if mesh.local_draw_ranges.is_empty() {
                    bindings.bind_material(render, gpu.default_material_bind_group)?;
                    render.draw_indexed_range_instanced(
                        mesh.vertex_count,
                        mesh.first_vertex,
                        offset,
                        instance_count,
                        first_instance,
                    )?;
                } else {
                    for index in mesh.opaque_draw_range_indices.iter().copied() {
                        let range = &mesh.local_draw_ranges[index as usize];
                        let material = match range.material_slot {
                            Some(s) => match self.asset_gpu_materials.get(&(mesh.model_id.0, s)) {
                                Some(m) => m.bind_group,
                                None => continue,
                            },
                            None => gpu.default_material_bind_group,
                        };
                        bindings.bind_material(render, material)?;
                        render.draw_indexed_range_instanced(
                            range.vertex_count,
                            mesh.first_vertex + range.first_vertex,
                            offset,
                            instance_count,
                            first_instance,
                        )?;
                    }
                    if mode != 2 {
                        for (member, id) in batch.stable_ids.iter().enumerate()
                            .skip(draw_member.unwrap_or(0))
                            .take(if draw_member.is_some() { 1 } else { batch.stable_ids.len() })
                        {
                            let Some(entity) = self.world.entity(SceneEntityId(*id)) else {
                                continue;
                            };
                            for index in mesh.alpha_draw_range_indices.iter().copied() {
                                let range = &mesh.local_draw_ranges[index as usize];
                                let material = match range.material_slot {
                                    Some(s) => {
                                        match self.asset_gpu_materials.get(&(mesh.model_id.0, s)) {
                                            Some(m) => m.bind_group,
                                            None => continue,
                                        }
                                    }
                                    None => gpu.default_material_bind_group,
                                };
                                let t = entity.transform;
                                let centre = transform_point(
                                    range.local_center,
                                    t.scale,
                                    t.rotation_degrees,
                                    t.position,
                                );
                                let delta = centre.sub(optical_camera.position);
                                alpha_draws.push(AlphaAssetDraw {
                                    distance_sq: delta.dot(delta),
                                    material_group: material,
                                    vertex_buffer: buffer,
                                    vertex_offset: offset,
                                    first_index: mesh.first_vertex + range.first_vertex,
                                    index_count: range.vertex_count,
                                    instance_index: batch.first_instance + member as u32,
                                });
                            }
                        }
                    }
                }
            }
            if mode != 2 && !alpha_draws.is_empty() {
                alpha_draws.sort_by(|a, b| b.distance_sq.total_cmp(&a.distance_sq));
                render.set_pipeline(gpu.asset_alpha_pipeline)?;
                render.set_bind_group(0, group)?;
                render.set_bind_group(2, weather)?;
                // A pipeline switch clears descriptor sets; invalidate the cache too.
                bindings = MaterialVertexBindings::new(None, None);
                for draw in alpha_draws {
                    bindings.bind_vertex(render, draw.vertex_buffer)?;
                    bindings.bind_material(render, draw.material_group)?;
                    render.draw_indexed_range_instanced(
                        draw.index_count,
                        draw.first_index,
                        draw.vertex_offset,
                        1,
                        draw.instance_index,
                    )?;
                }
            }
            if mode != 2 {
                // The depth-writing surface must still transmit/reflect source
                // bubbles and spray rendered on the opposite side of it.
                for batch in particle_batches {
                    let material = batch.material.as_ref()
                        .and_then(|key| self.particle_gpu_materials.get(key))
                        .map_or(gpu.default_material_bind_group, |m| m.bind_group);
                    render.set_pipeline(if batch.blend == SceneParticleBlend::Additive {
                        gpu.particle_additive_pipeline
                    } else {
                        gpu.alpha_pipeline
                    })?;
                    render.set_bind_group(0, group)?;
                    render.set_bind_group(1, material)?;
                    render.set_bind_group(2, weather)?;
                    render.set_vertex_buffer(
                        0, gpu.particle_vertex_buffers[slot],
                        batch.first_vertex as u64 * VERTEX_STRIDE,
                    )?;
                    render.draw(batch.vertex_count)?;
                }
            }
            render.end_render_target()?;
        }
        Ok(())
    }

    pub(super) fn draw_water_surface(
        &self,
        render: &RenderClient,
        gpu: GpuScene,
        slot: usize,
        weather: u32,
    ) -> Result<(), String> {
        let Some(water) = self.water_gpu.as_ref() else {
            return Ok(());
        };
        let Some(targets) = water.targets.as_ref() else {
            return Ok(());
        };
        let Some(material) = self.water_gpu_material.as_ref() else {
            return Ok(());
        };
        if !self.water_simulation.enabled || water_optics_diagnostic() == 4 {
            return Ok(());
        }
        render.set_render_phase(Some("Water"))?;
        render.set_draw_list_kind(Some(RenderDrawListKind::Transparent))?;
        render.set_pipeline(water.surface_pipeline)?;
        render.set_bind_group(0, gpu.bind_groups[slot])?;
        render.set_bind_group(1, material.bind_group)?;
        render.set_bind_group(2, weather)?;
        render.set_bind_group(3, targets.groups[water.bumps.current][slot])?;
        render.set_vertex_buffer(0, water.surface_vertices, 0)?;
        if water.surface_indices != 0 {
            render.set_index_buffer(water.surface_indices, 0, "U32")?;
            render.draw_indexed(water.surface_index_count)?;
        } else {
            render.draw(water.surface_vertex_count)?;
        }
        render.set_pipeline(water.shore_pipeline)?;
        render.set_bind_group(0, gpu.bind_groups[slot])?;
        render.set_bind_group(1, material.bind_group)?;
        render.set_bind_group(2, weather)?;
        render.set_bind_group(3, targets.groups[water.bumps.current][slot])?;
        render.set_vertex_buffer(0, gpu.vertex_buffers[slot], 0)?;
        let mut cursor = 0;
        for terrain_slot in &self.primitive_main_render_slots() {
            if self.water_render_slots.contains(terrain_slot) {
                continue;
            }
            let count = self
                .terrain_meshes
                .get(terrain_slot)
                .map(|mesh| (mesh.vertices.len() / FLOATS_PER_VERTEX) as u32)
                .unwrap_or(CUBE_VERTEX_COUNT);
            if self.terrain_meshes.contains_key(terrain_slot) {
                render.draw_range(count, cursor)?;
            }
            cursor += count;
        }
        Ok(())
    }

    pub(super) fn draw_underwater(&self, render: &RenderClient, slot: usize) -> Result<(), String> {
        if self.water_simulation.enabled && self.water_simulation.immersion.blend > 0.0 {
            if let Some(water) = self.water_gpu.as_ref() {
                water.underwater.draw(render, slot)?;
            }
        }
        Ok(())
    }

    pub(super) fn attach_water_graph(&self, graph: &mut RenderGraphDesc) {
        if !self.water_simulation.enabled {
            return;
        }
        let Some(targets) = self.water_gpu.as_ref().and_then(|g| g.targets.as_ref()) else {
            return;
        };
        for (i, texture) in targets.textures.iter().enumerate() {
            graph
                .resources
                .push(RenderGraphResourceDesc::external_texture(
                    RenderGraphResourceId(20 + i as u64),
                    [
                        "newviso.water.reflection",
                        "newviso.water.refraction",
                        "newviso.water.refraction_depth",
                    ][i],
                    newviso_render_client::TextureId::new(*texture),
                    RenderGraphResourceUsage::SampledTexture,
                ));
        }
        graph
            .resources
            .push(RenderGraphResourceDesc::external_texture(
                RenderGraphResourceId(23),
                "newviso.water.ripples",
                newviso_render_client::TextureId::new(
                    self.water_gpu.as_ref().unwrap().bumps.textures
                        [self.water_gpu.as_ref().unwrap().bumps.current],
                ),
                RenderGraphResourceUsage::SampledTexture,
            ));
        let pass = RenderGraphPassDesc::new(
            RenderGraphPassId(20),
            "newviso.main.water",
            RenderGraphPassKind::Water,
        )
        .with_domain(RenderGraphPassDomain::Render3d)
        .reads(
            RenderGraphResourceId(20),
            RenderGraphResourceUsage::SampledTexture,
        )
        .reads(
            RenderGraphResourceId(21),
            RenderGraphResourceUsage::SampledTexture,
        )
        .reads(
            RenderGraphResourceId(22),
            RenderGraphResourceUsage::SampledTexture,
        )
        .reads(
            RenderGraphResourceId(23),
            RenderGraphResourceUsage::SampledTexture,
        )
        .reads(
            RenderGraphResourceId(5),
            RenderGraphResourceUsage::DepthAttachment,
        )
        .writes(
            RenderGraphResourceId(5),
            RenderGraphResourceUsage::DepthAttachment,
        )
        .writes(
            RenderGraphResourceId(6),
            RenderGraphResourceUsage::ColorAttachment,
        )
        .draw_list(RenderDrawListKind::Transparent);
        let index = graph
            .passes
            .iter()
            .position(|p| p.kind == RenderGraphPassKind::Transparent)
            .unwrap_or(graph.passes.len());
        graph.passes.insert(index, pass);
        if self.water_simulation.immersion.blend > 0.0 {
            renderer_water_underwater::UnderwaterGpu::attach(graph);
        }
    }

    pub(super) fn shutdown_water_renderer(&mut self, render: &RenderClient) {
        if let Some(water) = self.water_gpu.take() {
            if let Some(targets) = water.targets {
                for ring in targets.groups {
                    for group in ring {
                        render.destroy_bind_group(group);
                    }
                }
                for target in [targets.reflection, targets.refraction, targets.depth] {
                    render.destroy_render_target(target);
                }
            }
            for pipeline in [
                water.surface_pipeline,
                water.shore_pipeline,
                water.depth_pipeline,
                water.depth_asset_pipeline,
            ] {
                render.destroy_pipeline(pipeline);
            }
            for shader in [
                water.surface_vertex_shader,
                water.shore_shader,
                water.fragment_shader,
                water.depth_shader,
            ] {
                render.destroy_shader(shader);
            }
            for groups in [water.reflection_groups, water.sky_groups] {
                for group in groups {
                    if group != 0 {
                        render.destroy_bind_group(group);
                    }
                }
            }
            for uniforms in [
                water.optical_uniforms,
                water.reflection_uniforms,
                water.sky_uniforms,
            ] {
                for uniform in uniforms {
                    if uniform != 0 {
                        render.destroy_buffer(uniform);
                    }
                }
            }
            for buffer in water.field_buffers {
                render.destroy_buffer(buffer);
            }
            render.destroy_buffer(water.surface_vertices);
            if water.surface_indices != 0 { render.destroy_buffer(water.surface_indices); }
            water.bumps.shutdown(render);
            water.underwater.shutdown(render);
            render.destroy_sampler(water.sampler);
            render.destroy_bind_group_layout(water.optical_layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reflected_camera_preserves_projection_and_mirrors_about_authored_level() {
        let camera = Camera {
            position: Vec3::new(1.0, 7.0, 2.0),
            target: Vec3::new(4.0, 3.0, 9.0),
            up: Vec3::new(0.0, 1.0, 0.0),
            fov_y_degrees: 60.0,
            near: 0.1,
            far: 500.0,
        };
        let mirrored = reflect_camera(camera.clone(), 2.0);
        assert_eq!(mirrored.position.y, -3.0);
        assert_eq!(mirrored.target.y, 1.0);
        assert_eq!(mirrored.up.y, -1.0);
        assert_eq!(mirrored.fov_y_degrees, camera.fov_y_degrees);
        let original = reflect_camera(mirrored, 2.0);
        assert_eq!(original.position.y, camera.position.y);
    }
}
