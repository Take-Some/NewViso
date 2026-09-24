use super::*;

impl Scene3dRuntime {
    pub fn update_native_input_from_snapshot(
        &mut self,
        input: &InputSnapshot,
        dt: f32,
        camera_navigation_enabled: bool,
    ) -> Result<(), String> {
        if camera_navigation_enabled {
            let [dx, dy] = input.mouse_delta();
            let wheel_y = input.mouse_wheel_y();
            let rotating = input.mouse_button_down(PRIMARY_MOUSE_BUTTON);
            if ((dx != 0.0 || dy != 0.0) && rotating) || wheel_y != 0.0 {
                self.orbit.apply_mouse(dx, dy, wheel_y, rotating);
                self.camera.position = self.orbit.position(self.camera.target);
                self.sync_runtime_camera_to_flecs()?;
            }
        }
        self.tick(dt)
    }
    pub fn tick(&mut self, dt: f32) -> Result<(), String> {
        if dt.is_finite() && dt > 0.0 {
            self.sky_time_seconds =
                (self.sky_time_seconds + dt * self.sky_time_scale).rem_euclid(86_400.0);
        }
        self.update_scene_world(dt)
    }
    pub fn render_frame(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.render_frame_with_overlay(width, height, || Ok(()))
    }
    pub fn render_frame_with_overlay<F>(
        &mut self,
        width: u32,
        height: u32,
        overlay: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        let gpu = self
            .gpu
            .ok_or_else(|| "3D scene GPU resources are not initialized".to_owned())?;

        let width = width.max(1);
        let height = height.max(1);
        let aspect = width as f32 / height as f32;
        let forward = self.camera.target.sub(self.camera.position).normalized();
        self.frame_plan = self.world.scan_visibility(SceneView {
            position: self.camera.position,
            forward,
            up: self.camera.up,
            near: self.camera.near,
            far: self.camera.far,
            fov_y_radians: self.camera.fov_y_degrees.to_radians(),
            aspect,
        });
        let vertex_data = self.build_cube_vertices(aspect);
        let shadow_vertex_data = self.build_shadow_vertices();
        let flare_vertex_data = self.build_lens_flare_vertices(aspect);
        let flare_vertex_count =
            u32::try_from(flare_vertex_data.len() / FLARE_FLOATS_PER_VERTEX).unwrap_or(0);
        let (frame_uniform, shadow_enabled) =
            self.scene_frame_uniform(aspect, gpu.shadow_resolution);
        let render = RenderClient::new();

        render.write_buffer_f32(gpu.vertex_buffer, 0, &vertex_data)?;
        render.write_buffer_f32(gpu.shadow_vertex_buffer, 0, &shadow_vertex_data)?;
        if flare_vertex_count > 0 {
            render.write_buffer_f32(gpu.flare_vertex_buffer, 0, &flare_vertex_data)?;
        }
        render.write_buffer_f32(gpu.frame_uniform, 0, &frame_uniform)?;
        if let Some(gpu_sky) = self.gpu_sky {
            let sky_frame = self.sky_frame_uniform(width as f32 / height as f32);
            render.write_buffer_f32(gpu_sky.camera_uniform, 0, &sky_frame)?;
        }

        let frame_index = self.frame_index;
        if let Err(error) = self.render_frame_inner(
            &render,
            gpu,
            width,
            height,
            frame_index,
            shadow_enabled,
            flare_vertex_count,
            overlay,
        ) {
            render.abort_frame();
            return Err(error);
        }

        self.frame_index = self.frame_index.wrapping_add(1);
        Ok(())
    }
    pub(super) fn render_frame_inner<F>(
        &self,
        render: &RenderClient,
        gpu: GpuScene,
        width: u32,
        height: u32,
        frame_index: u64,
        shadow_enabled: bool,
        flare_vertex_count: u32,
        overlay: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        render.begin_frame(self.clear_color, frame_index)?;

        if shadow_enabled && self.shadow_vertex_count() > 0 {
            render.begin_render_target(
                gpu.shadow_render_target,
                Some([1.0, 1.0, 1.0, 1.0]),
                Some(1.0),
            )?;
            render.set_viewport(gpu.shadow_resolution, gpu.shadow_resolution)?;
            render.set_scissor(gpu.shadow_resolution, gpu.shadow_resolution)?;
            render.set_pipeline(gpu.shadow_pipeline)?;
            render.set_bind_group(0, gpu.shadow_bind_group)?;
            render.set_vertex_buffer(0, gpu.shadow_vertex_buffer, 0)?;
            render.draw(self.shadow_vertex_count())?;
            render.end_render_target()?;
        }

        render.set_viewport(width, height)?;
        render.set_scissor(width, height)?;

        if let Some(sky) = self.gpu_sky {
            render.set_pipeline(sky.pipeline)?;
            render.set_bind_group(0, sky.bind_group)?;
            render.set_vertex_buffer(0, sky.vertex_buffer, 0)?;
            render.set_index_buffer(sky.index_buffer, 0, sky.index_format)?;
            render.draw_indexed(sky.index_count)?;
        }

        render.set_pipeline(gpu.pipeline)?;
        render.set_bind_group(0, gpu.bind_group)?;
        render.set_vertex_buffer(0, gpu.vertex_buffer, 0)?;
        render.draw(self.vertex_count())?;

        if flare_vertex_count > 0 {
            render.set_pipeline(gpu.flare_pipeline)?;
            render.set_vertex_buffer(0, gpu.flare_vertex_buffer, 0)?;
            render.draw(flare_vertex_count)?;
        }

        overlay()?;
        render.end_frame()
    }
}
