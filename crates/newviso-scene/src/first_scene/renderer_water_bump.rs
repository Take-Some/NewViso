//! Reference small-ripple simulation, integrated on the shared water clock.
use super::*;
const RIPPLE_SIZE: u32 = 256;
const MAX_RIPPLE_STEPS: usize = 8;
const STEP_STRIDE: u64 = 256;
const STEP_BYTES: u64 = 48;

#[derive(Debug)]
pub(super) struct WaterBumpGpu {
    pub(super) textures: [u32; 2],
    pub(super) current: usize,
    targets: [u32; 2],
    initialized: bool,
    seconds: f64,
    random: u32,
    uniform_buffers: [u32; SCENE_FRAME_SLOTS],
    groups: [[u32; SCENE_FRAME_SLOTS]; MAX_RIPPLE_STEPS * 2],
    layout: u32,
    sampler: u32,
    pipeline: u32,
    vertex_shader: u32,
    fragment_shader: u32,
    vertices: u32,
}

impl WaterBumpGpu {
    pub(super) fn new(render: &RenderClient, noise: u32) -> Result<Self, String> {
        let layout = render.create_bind_group_layout(
            "newviso.water.ripple.step",
            &["UniformBuffer", "Texture2D", "Texture2D", "Sampler"],
        )?;
        let uniform_buffers = create_frame_buffer_ring(
            render,
            "newviso.water.ripple.steps",
            STEP_STRIDE * MAX_RIPPLE_STEPS as u64,
            "Uniform",
        )?;
        let targets = [
            render.create_render_target(
                "newviso.water.ripple.ping",
                RIPPLE_SIZE,
                RIPPLE_SIZE,
                "Rgba16Float",
                None,
            )?,
            render.create_render_target(
                "newviso.water.ripple.pong",
                RIPPLE_SIZE,
                RIPPLE_SIZE,
                "Rgba16Float",
                None,
            )?,
        ];
        let textures = [
            render.render_target_color_texture(targets[0])?,
            render.render_target_color_texture(targets[1])?,
        ];
        let sampler = render.create_sampler_repeat_linear("newviso.water.ripple.repeat")?;
        let mut groups = [[0; SCENE_FRAME_SLOTS]; MAX_RIPPLE_STEPS * 2];
        for step in 0..MAX_RIPPLE_STEPS {
            for input in 0..2 {
                for slot in 0..SCENE_FRAME_SLOTS {
                    groups[step * 2 + input][slot] = render.create_bind_group(
                        "newviso.water.ripple.step",
                        layout,
                        [Some(textures[input]), Some(noise), None, None],
                        Some(sampler),
                        Some((uniform_buffers[slot], step as u64 * STEP_STRIDE, STEP_BYTES)),
                    )?;
                }
            }
        }
        let vertex_shader = render.create_shader_spirv(
            "newviso.water.ripple.vertex",
            ShaderStage::Vertex,
            include_bytes!("../assets/water_bump.vert.spv"),
        )?;
        let fragment_shader = render.create_shader_spirv(
            "newviso.water.ripple.fragment",
            ShaderStage::Fragment,
            include_bytes!("../assets/water_bump.frag.spv"),
        )?;
        let attributes = [VertexAttribute {
            location: 0,
            offset: 0,
            format: VertexFormat::Float32x2,
        }];
        let pipeline = render.create_pipeline(GraphicsPipelineDesc {
            vertex_stride: 8,
            depth_format: None,
            depth_test: false,
            depth_write: false,
            ..scene_pipeline_desc(
                "newviso.water.ripple.update",
                "newviso.water.ripple.reference.v1",
                vertex_shader,
                fragment_shader,
                &attributes,
                &[layout],
            )
        })?;
        let vertices =
            render.create_buffer("newviso.water.ripple.triangle", 24, "Vertex", "CpuToGpu")?;
        render.write_buffer_f32(vertices, 0, &[-1.0, -1.0, 3.0, -1.0, -1.0, 3.0])?;
        Ok(Self {
            textures,
            current: 0,
            targets,
            initialized: false,
            seconds: 0.0,
            random: 0x1dc2_7553,
            uniform_buffers,
            groups,
            layout,
            sampler,
            pipeline,
            vertex_shader,
            fragment_shader,
            vertices,
        })
    }

    pub(super) fn update(
        &mut self,
        render: &RenderClient,
        slot: usize,
        seconds: f64,
        weather: &WeatherEffectsState,
    ) -> Result<(), String> {
        render.set_render_phase(None)?;
        render.set_draw_list_kind(None)?;
        if !self.initialized {
            for target in self.targets {
                render.begin_render_target(target, Some([0.5; 4]), None)?;
                render.end_render_target()?;
            }
            self.initialized = true;
        }
        let steps = (((seconds - self.seconds) * 60.0 + 1e-6).floor().max(0.0) as usize)
            .min(MAX_RIPPLE_STEPS);
        let authored = weather.ocean_wave_max_amplitude > 0.0;
        let speed = if authored {
            weather.ripple_speed.max(0.0)
        } else {
            0.75
        };
        let transfer = if authored {
            weather.ripple_velocity_transfer.max(0.0)
        } else {
            0.45
        };
        let disturbance = if authored {
            weather.ripple_disturb.max(0.0)
        } else {
            0.16
        };
        for step in 0..steps {
            self.random ^= self.random << 13;
            self.random ^= self.random >> 17;
            self.random ^= self.random << 5;
            let offset = [
                (self.random & 0xffff) as f32 / 65536.0,
                (self.random >> 16) as f32 / 65536.0,
            ];
            let parameters = [
                1.0,
                0.18,
                0.01,
                1.0,
                transfer,
                disturbance,
                speed / 60.0,
                0.0,
                offset[0],
                offset[1],
                0.0,
                0.0,
            ];
            render.write_buffer_f32(
                self.uniform_buffers[slot],
                step as u64 * STEP_STRIDE,
                &parameters,
            )?;
            let output = 1 - self.current;
            render.begin_render_target(self.targets[output], None, None)?;
            render.set_viewport(RIPPLE_SIZE, RIPPLE_SIZE)?;
            render.set_scissor(RIPPLE_SIZE, RIPPLE_SIZE)?;
            render.set_pipeline(self.pipeline)?;
            render.set_bind_group(0, self.groups[step * 2 + self.current][slot])?;
            render.set_vertex_buffer(0, self.vertices, 0)?;
            render.draw(3)?;
            render.end_render_target()?;
            self.current = output;
            self.seconds += 1.0 / 60.0;
        }
        Ok(())
    }

    pub(super) fn shutdown(self, render: &RenderClient) {
        for ring in self.groups {
            for group in ring {
                render.destroy_bind_group(group);
            }
        }
        for buffer in self.uniform_buffers {
            render.destroy_buffer(buffer);
        }
        for target in self.targets {
            render.destroy_render_target(target);
        }
        render.destroy_buffer(self.vertices);
        render.destroy_pipeline(self.pipeline);
        render.destroy_shader(self.vertex_shader);
        render.destroy_shader(self.fragment_shader);
        render.destroy_sampler(self.sampler);
        render.destroy_bind_group_layout(self.layout);
    }
}
