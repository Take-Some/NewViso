use super::*;
use std::collections::BTreeSet;

fn instance_cell(position: Vec3) -> (i32, i32, i32) {
    let q = |value: f32| (value / ASSET_INSTANCE_CELL_SIZE).floor() as i32;
    (q(position.x), q(position.y), q(position.z))
}

fn instance_ids_sphere(world: &SceneWorld, stable_ids: &[u64]) -> Option<[f32; 4]> {
    let mut min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut any = false;
    for stable_id in stable_ids {
        let entity = world.entity(SceneEntityId(*stable_id))?;
        min.x = min.x.min(entity.bounds.min.x);
        min.y = min.y.min(entity.bounds.min.y);
        min.z = min.z.min(entity.bounds.min.z);
        max.x = max.x.max(entity.bounds.max.x);
        max.y = max.y.max(entity.bounds.max.y);
        max.z = max.z.max(entity.bounds.max.z);
        any = true;
    }
    if !any {
        return None;
    }
    let center = Vec3::new(
        (min.x + max.x) * 0.5,
        (min.y + max.y) * 0.5,
        (min.z + max.z) * 0.5,
    );
    let radius = max.sub(center).length().max(0.05);
    Some([center.x, center.y, center.z, radius])
}

fn asset_mesh_vertex_binding(
    mesh: &CpuAssetMesh,
    gpu: GpuScene,
    frame_slot: usize,
) -> Result<(u32, i32), String> {
    let Some(skinned_first_vertex) = mesh.skinned_first_vertex else {
        return Ok((gpu.asset_vertex_buffer, 0));
    };
    let delta = i64::from(skinned_first_vertex) - i64::from(mesh.first_vertex);
    let vertex_offset = i32::try_from(delta).map_err(|_| {
        format!(
            "skinned vertex offset exceeds i32 compact_first={} global_first={}",
            skinned_first_vertex, mesh.first_vertex
        )
    })?;
    Ok((gpu.skinned_vertex_buffers[frame_slot], vertex_offset))
}

#[derive(Clone, Copy, Debug)]
struct DirectAssetDraw {
    material_group: u32,
    vertex_buffer: u32,
    vertex_offset: i32,
    index_count: u32,
    first_index: u32,
    instance_count: u32,
    first_instance: u32,
}

#[derive(Clone, Copy, Debug)]
struct AlphaAssetDraw {
    distance_sq: f32,
    material_group: u32,
    vertex_buffer: u32,
    vertex_offset: i32,
    first_index: u32,
    index_count: u32,
    instance_index: u32,
}

#[derive(Clone, Copy, Debug)]
struct PendingHizAssetDraw {
    material_group: u32,
    sphere: [f32; 4],
    index_count: u32,
    first_index: u32,
    instance_count: u32,
    first_instance: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VisibleBatchRun {
    first_member: usize,
    member_count: usize,
}

fn visible_batch_runs(
    batch: &GpuResidentInstanceBatch,
    visible_slots: &BTreeMap<u64, u32>,
) -> Vec<VisibleBatchRun> {
    let mut runs = Vec::new();
    let mut run_start = None;

    for (member, stable_id) in batch.stable_ids.iter().copied().enumerate() {
        let visible = visible_slots.get(&stable_id).copied();
        if let Some(slot) = visible {
            debug_assert_eq!(
                slot,
                batch.first_instance.saturating_add(member as u32),
                "persistent instance batch slots must remain contiguous"
            );
            if run_start.is_none() {
                run_start = Some(member);
            }
        } else if let Some(first_member) = run_start.take() {
            runs.push(VisibleBatchRun {
                first_member,
                member_count: member - first_member,
            });
        }
    }

    if let Some(first_member) = run_start {
        runs.push(VisibleBatchRun {
            first_member,
            member_count: batch.stable_ids.len() - first_member,
        });
    }
    runs
}

/// Contiguous native indirect ranges with the same material. Offset gaps must
/// start a new run; every pass consumes exactly the same visible draw stream.
fn material_indirect_runs(draws: &[(u32, u64)]) -> impl Iterator<Item = (u32, u64, usize)> + '_ {
    let mut cursor = 0;
    std::iter::from_fn(move || {
        let &(material, offset) = draws.get(cursor)?;
        let start = cursor;
        cursor += 1;
        while cursor < draws.len()
            && draws[cursor].0 == material
            && draws[cursor].1 == offset + (cursor - start) as u64 * HIZ_INDIRECT_STRIDE
        {
            cursor += 1;
        }
        Some((material, offset, cursor - start))
    })
}

fn draw_material_indirect_runs(
    render: &RenderClient,
    buffer: u32,
    draws: &[(u32, u64)],
) -> Result<(), String> {
    for (material, offset, count) in material_indirect_runs(draws) {
        render.set_bind_group(1, material)?;
        render.draw_indexed_indirect(
            buffer,
            offset,
            u32::try_from(count).map_err(|_| "material indirect count exceeds u32".to_owned())?,
            HIZ_INDIRECT_STRIDE as u32,
        )?;
    }
    Ok(())
}

fn append_hiz_candidate(out: &mut Vec<u8>, sphere: [f32; 4]) {
    for value in sphere {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in [1u32, 0, 0, 0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn append_indexed_indirect_command(
    out: &mut Vec<u8>,
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    first_instance: u32,
) {
    out.extend_from_slice(&index_count.to_le_bytes());
    out.extend_from_slice(&instance_count.to_le_bytes());
    out.extend_from_slice(&first_index.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&first_instance.to_le_bytes());
}

fn main_deferred_hdr_render_graph(frame_index: u64, width: u32, height: u32) -> RenderGraphDesc {
    const SURFACE: RenderGraphResourceId = RenderGraphResourceId(1);
    const GBUFFER_ALBEDO: RenderGraphResourceId = RenderGraphResourceId(2);
    const GBUFFER_NORMAL: RenderGraphResourceId = RenderGraphResourceId(3);
    const GBUFFER_MATERIAL: RenderGraphResourceId = RenderGraphResourceId(4);
    const GBUFFER_DEPTH: RenderGraphResourceId = RenderGraphResourceId(5);
    const SCENE_HDR: RenderGraphResourceId = RenderGraphResourceId(6);
    const SSR_SIGNAL: RenderGraphResourceId = RenderGraphResourceId(7);
    const BLOOM_SIGNAL: RenderGraphResourceId = RenderGraphResourceId(8);

    const BACKGROUND: RenderGraphPassId = RenderGraphPassId(1);
    const GBUFFER: RenderGraphPassId = RenderGraphPassId(2);
    const DEFERRED_LIGHTING: RenderGraphPassId = RenderGraphPassId(3);
    const TRANSPARENT: RenderGraphPassId = RenderGraphPassId(4);
    const SSR: RenderGraphPassId = RenderGraphPassId(5);
    const BLOOM: RenderGraphPassId = RenderGraphPassId(6);
    const POSTFX: RenderGraphPassId = RenderGraphPassId(7);

    let extent = Extent2D::new(width.max(1), height.max(1));
    let surface = RenderGraphResourceDesc::external_swapchain(
        SURFACE,
        "newviso.main.surface",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Bgra8Srgb,
    )
    .with_semantic(RenderGraphResourceSemantic::SurfaceColor);

    let gbuffer_albedo = RenderGraphResourceDesc::transient_texture(
        GBUFFER_ALBEDO,
        "newviso.main.gbuffer.albedo",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Rgba8Unorm,
    )
    .with_semantic(RenderGraphResourceSemantic::GBufferAlbedo);
    let gbuffer_normal = RenderGraphResourceDesc::transient_texture(
        GBUFFER_NORMAL,
        "newviso.main.gbuffer.normal",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Rgba16Float,
    )
    .with_semantic(RenderGraphResourceSemantic::GBufferNormal);
    let gbuffer_material = RenderGraphResourceDesc::transient_texture(
        GBUFFER_MATERIAL,
        "newviso.main.gbuffer.material",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Rgba8Unorm,
    )
    .with_semantic(RenderGraphResourceSemantic::GBufferMaterial);
    let gbuffer_depth = RenderGraphResourceDesc::transient_texture(
        GBUFFER_DEPTH,
        "newviso.main.gbuffer.depth",
        RenderGraphResourceUsage::DepthAttachmentSampled,
        extent,
        RenderTextureFormat::Depth32Float,
    )
    .with_semantic(RenderGraphResourceSemantic::GBufferDepth);
    let scene_hdr = RenderGraphResourceDesc::transient_texture(
        SCENE_HDR,
        "newviso.main.scene_hdr",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Rgba16Float,
    )
    .with_semantic(RenderGraphResourceSemantic::LitColor);
    let ssr_signal = RenderGraphResourceDesc::transient_texture(
        SSR_SIGNAL,
        "newviso.main.ssr_reflection",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Rgba16Float,
    )
    .with_semantic(RenderGraphResourceSemantic::ScreenSpaceReflection);
    let bloom_signal = RenderGraphResourceDesc::transient_texture(
        BLOOM_SIGNAL,
        "newviso.main.bloom_composite",
        RenderGraphResourceUsage::ColorAttachment,
        extent,
        RenderTextureFormat::Rgba16Float,
    )
    .with_semantic(RenderGraphResourceSemantic::BloomComposite);

    // Background is produced first. Deferred resolve LOAD-preserves this HDR target
    // and discards pixels without GBuffer coverage, so the sky survives untouched.
    let background = RenderGraphPassDesc::new(
        BACKGROUND,
        "newviso.main.background",
        RenderGraphPassKind::ForwardOpaque,
    )
    .with_domain(RenderGraphPassDomain::Render3d)
    .writes(SCENE_HDR, RenderGraphResourceUsage::ColorAttachment)
    .draw_list(RenderDrawListKind::OpaqueForward);

    let gbuffer = RenderGraphPassDesc::new(
        GBUFFER,
        "newviso.main.gbuffer",
        RenderGraphPassKind::GBuffer,
    )
    .with_domain(RenderGraphPassDomain::Render3d)
    .writes(GBUFFER_ALBEDO, RenderGraphResourceUsage::ColorAttachment)
    .writes(GBUFFER_NORMAL, RenderGraphResourceUsage::ColorAttachment)
    .writes(GBUFFER_MATERIAL, RenderGraphResourceUsage::ColorAttachment)
    .writes(
        GBUFFER_DEPTH,
        RenderGraphResourceUsage::DepthAttachmentSampled,
    )
    .draw_list(RenderDrawListKind::OpaqueForward);

    let deferred = RenderGraphPassDesc::new(
        DEFERRED_LIGHTING,
        "newviso.main.deferred_lighting",
        RenderGraphPassKind::DeferredLighting,
    )
    .with_domain(RenderGraphPassDomain::Render3d)
    .reads(GBUFFER_ALBEDO, RenderGraphResourceUsage::SampledTexture)
    .reads(GBUFFER_NORMAL, RenderGraphResourceUsage::SampledTexture)
    .reads(GBUFFER_MATERIAL, RenderGraphResourceUsage::SampledTexture)
    .reads(GBUFFER_DEPTH, RenderGraphResourceUsage::SampledTexture)
    .writes(SCENE_HDR, RenderGraphResourceUsage::ColorAttachment);

    let transparent = RenderGraphPassDesc::new(
        TRANSPARENT,
        "newviso.main.transparent",
        RenderGraphPassKind::Transparent,
    )
    .with_domain(RenderGraphPassDomain::Render3d)
    .reads(GBUFFER_DEPTH, RenderGraphResourceUsage::DepthAttachment)
    .writes(SCENE_HDR, RenderGraphResourceUsage::ColorAttachment)
    .draw_list(RenderDrawListKind::Transparent);

    let ssr = RenderGraphPassDesc::new(
        SSR,
        "newviso.main.ssr",
        RenderGraphPassKind::ScreenSpaceReflections,
    )
    .with_domain(RenderGraphPassDomain::PostProcess)
    .reads(SCENE_HDR, RenderGraphResourceUsage::SampledTexture)
    .reads(GBUFFER_DEPTH, RenderGraphResourceUsage::SampledTexture)
    .reads(GBUFFER_NORMAL, RenderGraphResourceUsage::SampledTexture)
    .reads(GBUFFER_MATERIAL, RenderGraphResourceUsage::SampledTexture)
    .writes(SSR_SIGNAL, RenderGraphResourceUsage::ColorAttachment);

    // Bloom is an explicit graph side-signal. The provider owns prefilter,
    // downsample and upsample targets; the root graph owns only the final HDR
    // bloom composite consumed by the tonemap pass.
    let bloom = RenderGraphPassDesc::new(
        BLOOM,
        "newviso.main.bloom",
        RenderGraphPassKind::BloomExtract,
    )
    .with_domain(RenderGraphPassDomain::PostProcess)
    .reads(SCENE_HDR, RenderGraphResourceUsage::SampledTexture)
    .writes(BLOOM_SIGNAL, RenderGraphResourceUsage::ColorAttachment);

    let postfx =
        RenderGraphPassDesc::new(POSTFX, "newviso.main.postfx", RenderGraphPassKind::PostFx)
            .with_domain(RenderGraphPassDomain::PostProcess)
            .reads(SCENE_HDR, RenderGraphResourceUsage::SampledTexture)
            .reads(GBUFFER_DEPTH, RenderGraphResourceUsage::SampledTexture)
            .reads(SSR_SIGNAL, RenderGraphResourceUsage::SampledTexture)
            .reads(BLOOM_SIGNAL, RenderGraphResourceUsage::SampledTexture)
            .writes(SURFACE, RenderGraphResourceUsage::ColorAttachment);

    let mut graph = RenderGraphDesc::new("newviso.main.deferred_hdr.v2")
        .add_resource(surface)
        .add_resource(gbuffer_albedo)
        .add_resource(gbuffer_normal)
        .add_resource(gbuffer_material)
        .add_resource(gbuffer_depth)
        .add_resource(scene_hdr)
        .add_resource(ssr_signal)
        .add_resource(bloom_signal)
        .add_pass(background)
        .add_pass(gbuffer)
        .add_pass(deferred)
        .add_pass(transparent)
        .add_pass(ssr)
        .add_pass(bloom)
        .add_pass(postfx);
    graph.frame_index = frame_index;
    graph
}

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
            let rotating = input.mouse_button_down(self.orbit.rotate_button);
            if ((dx != 0.0 || dy != 0.0) && rotating) || wheel_y != 0.0 {
                self.orbit.apply_mouse(dx, dy, wheel_y, rotating);
                self.camera.position = self.orbit.position(self.camera.target);
                self.sync_runtime_camera_to_flecs()?;
            }
        }
        self.tick(dt)
    }
    pub fn tick(&mut self, dt: f32) -> Result<(), String> {
        // SceneWorld owns process cadence. Advance it before process consumers
        // (animation, scripting, movers) inspect the current frame's tickets.
        self.update_scene_world(dt)?;

        if dt.is_finite() && dt > 0.0 {
            let cycle_delta_seconds = dt * self.sky_time_scale;
            self.sky_time_seconds = (self.sky_time_seconds + cycle_delta_seconds)
                .rem_euclid(self.timecycle_backend.duration_seconds);

            // GTA's large sky-cloud base noise is not driven by time-of-day.
            // Its phase integrates real frame time along normalized global-air
            // wind direction. Wind magnitude does not alter phase speed; the
            // dedicated large_speed scalar does.
            let volumetric_clouds_enabled = self
                .sky
                .as_ref()
                .is_some_and(|sky| sky.volumetric_clouds.enabled);
            if self.sky_clouds.enabled || volumetric_clouds_enabled {
                let wind = self.sky_clouds.speed;
                let wind_len = (wind[0] * wind[0] + wind[1] * wind[1]).sqrt();
                let direction = if wind_len > 1.0e-6 {
                    [wind[0] / wind_len, wind[1] / wind_len]
                } else {
                    [1.0, 0.0]
                };
                let phase_scale = self.sky_clouds.noise_phase_scale;
                let phase_step = self.sky_clouds.large_speed * phase_scale * phase_scale * dt;
                self.sky_cloud_noise_phase[0] =
                    (self.sky_cloud_noise_phase[0] + direction[0] * phase_step).rem_euclid(1.0);
                self.sky_cloud_noise_phase[1] =
                    (self.sky_cloud_noise_phase[1] + direction[1] * phase_step).rem_euclid(1.0);
            }

            // GTA's small/overall/edge cloud phases use continuous time-cycle
            // time measured in days (daysBetween + dayRatio), not elapsed seconds.
            self.sky_cloud_cycle_time_days = (self.sky_cloud_cycle_time_days
                + cycle_delta_seconds / self.timecycle_backend.duration_seconds)
                .rem_euclid(4096.0);

            self.update_atmospheric_clouds(dt);
            self.update_weather_gpu_fx(dt);
            self.update_particles(dt);
            self.update_skinned_animations(dt)?;
        }
        Ok(())
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
        if self.gpu.is_none() {
            return Err("3D scene GPU resources are not initialized".to_owned());
        }

        let render_perf_start = std::time::Instant::now();
        let mut render_perf_mark = render_perf_start;

        let width = width.max(1);
        let height = height.max(1);
        let aspect = width as f32 / height as f32;
        let forward = self.camera.target.sub(self.camera.position).normalized();
        let view = SceneView {
            position: self.camera.position,
            forward,
            up: self.camera.up,
            near: self.camera.near,
            far: self.camera.far,
            fov_y_radians: self.camera.fov_y_degrees.to_radians(),
            aspect,
        };
        let mut visibility_candidates = self.world.visibility_candidates(view);
        if let Some(graph) = self.portal_visibility.as_mut() {
            graph.filter_candidates(&self.camera, aspect, &mut visibility_candidates);
        }
        self.frame_plan = self
            .world
            .scan_visibility_candidates(view, visibility_candidates);
        let perf_visibility_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        render_perf_mark = std::time::Instant::now();

        self.sync_static_asset_gpu()?;
        let perf_geometry_sync_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        render_perf_mark = std::time::Instant::now();
        self.sync_static_asset_material_gpu()?;
        let perf_material_sync_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        let perf_gpu_sync_ms = perf_geometry_sync_ms + perf_material_sync_ms;
        render_perf_mark = std::time::Instant::now();
        let required_vertices = self.vertex_count();
        let required_shadow_vertices = self.shadow_vertex_count();
        self.ensure_geometry_buffer_capacity(required_vertices, required_shadow_vertices)?;
        let mut gpu = self
            .gpu
            .ok_or_else(|| "3D scene GPU resources disappeared".to_owned())?;
        let vertex_data = self.build_cube_vertices(aspect);
        let shadow_vertex_data = self.build_shadow_vertices();
        let flare_vertex_data = self.build_lens_flare_vertices(aspect);
        let flare_vertex_count =
            u32::try_from(flare_vertex_data.len() / FLARE_FLOATS_PER_VERTEX).unwrap_or(0);
        let (particle_vertex_data, particle_alpha_vertices, particle_additive_vertices) =
            self.build_particle_vertices();
        let (frame_uniform, shadow_enabled) =
            self.scene_frame_uniform(aspect, gpu.shadow_resolution);
        let perf_geometry_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        render_perf_mark = std::time::Instant::now();

        let render = RenderClient::new();
        self.sync_mass_instance_gpu(&render, aspect)?;
        let renderer_local_lights = self.renderer_local_lights();
        render.set_frame_lights(&renderer_local_lights)?;
        self.sync_atmospheric_cloud_gpu_residency(&render)?;
        self.ensure_volumetric_cloud_targets(&render, width, height)?;

        let frame_index = self.frame_index;
        let frame_state = render.begin_frame_acquire(self.clear_color, frame_index)?;
        if frame_state.frames_in_flight != SCENE_FRAME_SLOTS {
            if frame_state.active {
                render.abort_frame();
            }
            return Err(format!(
                "renderer frame-ring mismatch scene={} renderer={}",
                SCENE_FRAME_SLOTS, frame_state.frames_in_flight
            ));
        }
        if frame_state.frame_slot >= SCENE_FRAME_SLOTS {
            if frame_state.active {
                render.abort_frame();
            }
            return Err(format!(
                "renderer returned invalid frame slot {} for {} slots",
                frame_state.frame_slot, SCENE_FRAME_SLOTS
            ));
        }
        if !frame_state.active {
            // WSI/back-pressure can defer a presentation attempt. No frame-owned
            // resource is writable until the backend has actually acquired its slot.
            self.frame_index = self.frame_index.wrapping_add(1);
            return Ok(());
        }
        let frame_slot = frame_state.frame_slot;
        gpu = self.sync_skinned_vertex_gpu(&render, frame_slot)?;
        self.sync_gpu_instance_table(&render, gpu, frame_slot)?;

        let perf_residency_sync_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        render_perf_mark = std::time::Instant::now();

        if !vertex_data.is_empty() {
            render.write_buffer_f32(gpu.vertex_buffers[frame_slot], 0, &vertex_data)?;
        }
        if !shadow_vertex_data.is_empty() {
            render.write_buffer_f32(
                gpu.shadow_vertex_buffers[frame_slot],
                0,
                &shadow_vertex_data,
            )?;
        }
        if flare_vertex_count > 0 {
            render.write_buffer_f32(gpu.flare_vertex_buffers[frame_slot], 0, &flare_vertex_data)?;
        }
        if !particle_vertex_data.is_empty() {
            render.write_buffer_f32(
                gpu.particle_vertex_buffers[frame_slot],
                0,
                &particle_vertex_data,
            )?;
        }
        render.write_buffer_f32(gpu.frame_uniforms[frame_slot], 0, &frame_uniform)?;
        self.upload_weather_uniforms(&render, frame_slot, aspect)?;
        if let Some(gpu_sky) = self.gpu_sky {
            let sky_frame = self.sky_frame_uniform(width as f32 / height as f32);
            render.write_buffer_f32(gpu_sky.camera_uniforms[frame_slot], 0, &sky_frame)?;
        }
        if let Some(gpu_clouds) = self.gpu_atmospheric_clouds.as_ref() {
            let cloud_frames = self.atmospheric_cloud_uniforms(width, height);
            if cloud_frames.len() != gpu_clouds.layers.len() {
                return Err(format!(
                    "atmospheric cloud CPU/GPU layer count mismatch cpu={} gpu={}",
                    cloud_frames.len(),
                    gpu_clouds.layers.len()
                ));
            }
            for (frame, layer) in cloud_frames.iter().zip(gpu_clouds.layers.iter()) {
                if let Some(layer) = layer {
                    render.write_buffer_f32(layer.uniform_buffers[frame_slot], 0, frame)?;
                }
            }
            let depth_frame = self.atmospheric_soft_depth_uniform(aspect);
            render.write_buffer_f32(gpu_clouds.soft_depth_uniforms[frame_slot], 0, &depth_frame)?;
        }

        if let Some(volume) = self.gpu_volumetric_clouds.as_ref() {
            if let Some(targets) = volume.targets.as_ref() {
                let frame = self.volumetric_cloud_uniform(
                    width,
                    height,
                    targets.width,
                    targets.height,
                    volume.previous_view_projection,
                    volume.history_valid,
                )?;
                render.write_buffer_f32(volume.uniform_buffers[frame_slot], 0, &frame)?;
                let depth_frame = self.atmospheric_soft_depth_uniform(aspect);
                render.write_buffer_f32(
                    volume.depth_uniform_buffers[frame_slot],
                    0,
                    &depth_frame,
                )?;
            }
        }

        let perf_uploads_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        render_perf_mark = std::time::Instant::now();

        let previous_hiz_camera_compatible = self
            .last_render_camera
            .as_ref()
            .is_some_and(|previous| previous_hiz_camera_compatible(previous, &self.camera));
        if let Err(error) = self.render_frame_inner(
            &render,
            gpu,
            width,
            height,
            frame_index,
            frame_slot,
            shadow_enabled,
            flare_vertex_count,
            particle_alpha_vertices,
            particle_additive_vertices,
            previous_hiz_camera_compatible,
            overlay,
        ) {
            render.abort_frame();
            return Err(error);
        }

        let perf_inner_ms = render_perf_mark.elapsed().as_secs_f64() * 1000.0;
        let perf_total_ms = render_perf_start.elapsed().as_secs_f64() * 1000.0;
        if perf_total_ms >= 25.0 || frame_index % 60 == 0 {
            host_runtime::info(
                "newviso.perf.render",
                format!(
                    "frame={} total_ms={:.2} visibility_ms={:.2} gpu_sync_ms={:.2} geometry_sync_ms={:.2} material_sync_ms={:.2} geometry_ms={:.2} residency_sync_ms={:.2} uploads_ms={:.2} inner_ms={:.2} visible={} resident={} spatial_candidates={} instance_batches={} instances={} hiz_draws={} opaque_groups={} direct_opaque={} alpha_draws={} mass_draws={} mass_instances={} mass_chunks={} graph_passes={} graph_skipped={} graph_cpu_ms={:.3}",
                    frame_index,
                    perf_total_ms,
                    perf_visibility_ms,
                    perf_gpu_sync_ms,
                    perf_geometry_sync_ms,
                    perf_material_sync_ms,
                    perf_geometry_ms,
                    perf_residency_sync_ms,
                    perf_uploads_ms,
                    perf_inner_ms,
                    self.frame_plan.visible_count,
                    self.frame_plan.resident_count,
                    self.frame_plan.spatial_candidate_count,
                    self.last_submission_stats.instance_batches,
                    self.last_submission_stats.instance_count,
                    self.last_submission_stats.hiz_draws,
                    self.last_submission_stats.opaque_indirect_groups,
                    self.last_submission_stats.direct_opaque_draws,
                    self.last_submission_stats.alpha_draws,
                    self.last_submission_stats.mass_draws,
                    self.last_submission_stats.mass_instances,
                    self.mass_instance_chunk_count(),
                    self.last_submission_stats.graph_executed_passes,
                    self.last_submission_stats.graph_skipped_passes,
                    self.last_submission_stats.graph_cpu_record_ms
                ),
            );
        }

        self.last_render_camera = Some(self.camera.clone());
        self.frame_index = self.frame_index.wrapping_add(1);
        Ok(())
    }
    pub(super) fn render_frame_inner<F>(
        &mut self,
        render: &RenderClient,
        gpu: GpuScene,
        width: u32,
        height: u32,
        frame_index: u64,
        frame_slot: usize,
        shadow_enabled: bool,
        flare_vertex_count: u32,
        particle_alpha_vertices: u32,
        particle_additive_vertices: u32,
        previous_hiz_camera_compatible: bool,
        overlay: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        let mut instance_batches = Vec::<GpuResidentInstanceBatch>::new();
        let mut selected_static_batches = BTreeSet::<GpuInstanceBatchKey>::new();
        let mut dynamic_groups = BTreeMap::<GpuInstanceBatchKey, Vec<u64>>::new();
        let mut visible_instance_slots = BTreeMap::<u64, u32>::new();

        for id in &self.frame_plan.visible_entities {
            let Some(mesh) = self.asset_meshes.get(&id.0) else {
                continue;
            };
            let Some(entity) = self.world.entity(*id) else {
                continue;
            };
            if entity.mobility == SceneMobility::Static {
                if let Some(key) = self.gpu_instance_table.entity_batches.get(&id.0) {
                    selected_static_batches.insert(*key);
                }
                if let Some(slot) = self.gpu_instance_table.entity_slots.get(&id.0) {
                    visible_instance_slots.insert(id.0, *slot);
                }
            } else {
                let (cx, cy, cz) = instance_cell(entity.transform.position);
                dynamic_groups
                    .entry((mesh.model_id.0, cx, cy, cz, 1))
                    .or_default()
                    .push(id.0);
            }
        }

        for key in selected_static_batches {
            let Some(batch) = self.gpu_instance_table.batches.get(&key) else {
                continue;
            };

            // RSC7-style instanced submission: visibility may cut holes in a
            // resident spatial batch, but it must not automatically degrade the
            // entire batch to one draw per entity. Preserve each contiguous run
            // of resident instance slots as one instanced submission.
            for run in visible_batch_runs(batch, &visible_instance_slots) {
                let end = run.first_member + run.member_count;
                let stable_ids = batch.stable_ids[run.first_member..end].to_vec();
                let full_batch =
                    run.first_member == 0 && run.member_count == batch.stable_ids.len();
                let sphere = if full_batch {
                    batch.sphere
                } else {
                    instance_ids_sphere(&self.world, &stable_ids)
                        .ok_or_else(|| "visible static instance run lost scene entity".to_owned())?
                };
                instance_batches.push(GpuResidentInstanceBatch {
                    model_id: batch.model_id,
                    first_instance: batch.first_instance.saturating_add(run.first_member as u32),
                    instance_count: u32::try_from(run.member_count)
                        .map_err(|_| "visible static run count exceeds u32".to_owned())?,
                    stable_ids,
                    sphere,
                });
            }
        }

        let static_instance_count = self.gpu_instance_table.instance_data.len() / INSTANCE_FLOATS;
        let dynamic_instance_count = dynamic_groups.values().map(Vec::len).sum::<usize>();
        let total_instance_count = static_instance_count.saturating_add(dynamic_instance_count);
        if total_instance_count > gpu.asset_instance_capacity as usize {
            return Err(format!(
                "resident static + visible dynamic asset instances {} exceed GPU instance capacity {}",
                total_instance_count, gpu.asset_instance_capacity
            ));
        }

        let mut dynamic_instance_data =
            Vec::<f32>::with_capacity(dynamic_instance_count.saturating_mul(INSTANCE_FLOATS));
        for (key, stable_ids) in dynamic_groups {
            let first_instance = u32::try_from(
                static_instance_count + dynamic_instance_data.len() / INSTANCE_FLOATS,
            )
            .map_err(|_| "dynamic asset instance offset exceeds u32".to_owned())?;
            for stable_id in &stable_ids {
                let entity = self
                    .world
                    .entity(SceneEntityId(*stable_id))
                    .ok_or_else(|| format!("dynamic asset entity {} disappeared", stable_id))?;
                let slot = u32::try_from(
                    static_instance_count + dynamic_instance_data.len() / INSTANCE_FLOATS,
                )
                .map_err(|_| "dynamic asset instance index exceeds u32".to_owned())?;
                dynamic_instance_data.extend_from_slice(&geometry::instance_model_matrix(
                    entity.transform.position,
                    entity.transform.rotation_degrees,
                    entity.transform.scale,
                ));
                visible_instance_slots.insert(*stable_id, slot);
            }
            let count = u32::try_from(stable_ids.len())
                .map_err(|_| "dynamic asset instance count exceeds u32".to_owned())?;
            let sphere = instance_ids_sphere(&self.world, &stable_ids)
                .ok_or_else(|| "dynamic asset batch lost scene entities".to_owned())?;
            instance_batches.push(GpuResidentInstanceBatch {
                model_id: key.0,
                first_instance,
                instance_count: count,
                stable_ids,
                sphere,
            });
        }
        if !dynamic_instance_data.is_empty() {
            render.write_buffer_f32(
                gpu.asset_instance_buffers[frame_slot],
                static_instance_count as u64 * INSTANCE_STRIDE,
                &dynamic_instance_data,
            )?;
        }

        // Build one fail-open indirect command per opaque/cutout primitive
        // range and spatial instance batch. The Vulkan visibility provider may
        // zero instance_count using previous-frame Hi-Z; if it cannot, these
        // CPU-initialized commands remain fully drawable.
        let mut pending_hiz_draws = Vec::<PendingHizAssetDraw>::new();
        let mut direct_opaque_fallback = Vec::<DirectAssetDraw>::new();

        // Split only main-view submissions that contain an instance override.
        // Shared geometry and the full shadow batches remain unchanged.
        for (batch, member) in instance_batches.iter().flat_map(|batch| {
            let split = batch.instance_count > 1
                && batch
                    .stable_ids
                    .iter()
                    .any(|id| self.main_view_mesh_visibility.has_override(*id));
            let count = if split { batch.stable_ids.len() } else { 1 };
            (0..count).map(move |member| (batch, split.then_some(member)))
        }) {
            let Some(stable_id) = batch.stable_ids.get(member.unwrap_or(0)) else {
                continue;
            };
            let mesh = self
                .asset_meshes
                .get(stable_id)
                .ok_or_else(|| format!("asset instance {} disappeared", stable_id))?;
            debug_assert_eq!(mesh.model_id.0, batch.model_id);
            let sphere = batch.sphere;
            let instance_count = if member.is_some() {
                1
            } else {
                batch.instance_count
            };
            let first_instance = batch.first_instance + member.unwrap_or(0) as u32;

            let (vertex_buffer, vertex_offset) = asset_mesh_vertex_binding(mesh, gpu, frame_slot)?;
            let mut queue_range =
                |material_group: u32, first_index: u32, index_count: u32| -> Result<(), String> {
                    let candidate_index = pending_hiz_draws.len();
                    let static_hiz_compatible =
                        vertex_buffer == gpu.asset_vertex_buffer && vertex_offset == 0;
                    if static_hiz_compatible
                        && ENABLE_ASSET_HIZ_OCCLUSION
                        && candidate_index < MAX_HIZ_DRAW_CANDIDATES as usize
                    {
                        pending_hiz_draws.push(PendingHizAssetDraw {
                            material_group,
                            sphere,
                            index_count,
                            first_index,
                            instance_count,
                            first_instance,
                        });
                    } else {
                        direct_opaque_fallback.push(DirectAssetDraw {
                            material_group,
                            vertex_buffer,
                            vertex_offset,
                            index_count,
                            first_index,
                            instance_count,
                            first_instance,
                        });
                    }
                    Ok(())
                };

            if mesh.local_draw_ranges.is_empty() {
                queue_range(
                    gpu.default_material_bind_group,
                    mesh.first_vertex,
                    mesh.vertex_count,
                )?;
                continue;
            }

            for range_index in mesh.opaque_draw_range_indices.iter().copied() {
                let range = &mesh.local_draw_ranges[range_index as usize];
                if !self
                    .main_view_mesh_visibility
                    .visible(*stable_id, &range.mesh_name)
                {
                    continue;
                }
                let material_group = range
                    .material_slot
                    .and_then(|slot| self.asset_gpu_materials.get(&(mesh.model_id.0, slot)))
                    .map(|material| material.bind_group)
                    .unwrap_or(gpu.default_material_bind_group);
                queue_range(
                    material_group,
                    mesh.first_vertex.saturating_add(range.first_vertex),
                    range.vertex_count,
                )?;
            }
        }

        // Opaque/cutout geometry is order-independent. Build a compact draw
        // list sorted by material/state before uploading it, like RSC7 draw
        // lists do, so one material can execute as one contiguous indirect
        // multi-draw instead of many alternating service calls.
        pending_hiz_draws.sort_unstable_by_key(|draw| {
            (
                draw.material_group,
                draw.first_index,
                draw.first_instance,
                draw.instance_count,
            )
        });
        direct_opaque_fallback.sort_unstable_by_key(|draw| {
            (
                draw.material_group,
                draw.vertex_buffer,
                draw.vertex_offset,
                draw.first_index,
                draw.first_instance,
            )
        });

        let mut hiz_candidate_bytes =
            Vec::<u8>::with_capacity(pending_hiz_draws.len() * HIZ_CANDIDATE_STRIDE as usize);
        let mut hiz_indirect_bytes =
            Vec::<u8>::with_capacity(pending_hiz_draws.len() * HIZ_INDIRECT_STRIDE as usize);
        let mut hiz_draws = Vec::<(u32, u64)>::with_capacity(pending_hiz_draws.len());
        for (candidate_index, draw) in pending_hiz_draws.iter().enumerate() {
            append_hiz_candidate(&mut hiz_candidate_bytes, draw.sphere);
            append_indexed_indirect_command(
                &mut hiz_indirect_bytes,
                draw.index_count,
                draw.instance_count,
                draw.first_index,
                draw.first_instance,
            );
            hiz_draws.push((
                draw.material_group,
                candidate_index as u64 * HIZ_INDIRECT_STRIDE,
            ));
        }

        if !hiz_candidate_bytes.is_empty() {
            render.write_buffer(
                gpu.visibility_candidate_buffers[frame_slot],
                0,
                &hiz_candidate_bytes,
            )?;
            render.write_buffer(
                gpu.visibility_indirect_buffers[frame_slot],
                0,
                &hiz_indirect_bytes,
            )?;
        }

        let mut volumetric_composite: Option<(u32, u32, u32)> = None;
        if previous_hiz_camera_compatible && !hiz_draws.is_empty() {
            let forward = self.camera.target.sub(self.camera.position).normalized();
            render.set_render_phase(Some("VisibilityCull"))?;
            render.dispatch_visibility_indirect_cull(
                gpu.visibility_candidate_buffers[frame_slot],
                gpu.visibility_indirect_buffers[frame_slot],
                u32::try_from(hiz_draws.len())
                    .map_err(|_| "Hi-Z draw count exceeds u32".to_owned())?,
                [width, height],
                [
                    self.camera.position.x,
                    self.camera.position.y,
                    self.camera.position.z,
                ],
                [forward.x, forward.y, forward.z],
                [self.camera.up.x, self.camera.up.y, self.camera.up.z],
                self.camera.fov_y_degrees.to_radians(),
                self.camera.near,
                self.camera.far,
            )?;
            render.set_render_phase(None)?;
        }

        if shadow_enabled && (self.shadow_vertex_count() > 0 || !instance_batches.is_empty()) {
            render.begin_render_target(
                gpu.shadow_render_target,
                Some([1.0, 1.0, 1.0, 1.0]),
                Some(1.0),
            )?;
            render.set_viewport(gpu.shadow_resolution, gpu.shadow_resolution)?;
            render.set_scissor(gpu.shadow_resolution, gpu.shadow_resolution)?;
            render.set_pipeline(gpu.shadow_pipeline)?;
            render.set_bind_group(0, gpu.shadow_bind_groups[frame_slot])?;
            render.set_bind_group(1, gpu.default_material_bind_group)?;

            let dynamic_shadow_vertices = self.shadow_vertex_count();
            if dynamic_shadow_vertices > 0 {
                render.set_vertex_buffer(0, gpu.shadow_vertex_buffers[frame_slot], 0)?;
                render.draw(dynamic_shadow_vertices)?;
            }

            if !instance_batches.is_empty() {
                render.set_pipeline(gpu.asset_shadow_pipeline)?;
                render.set_bind_group(0, gpu.shadow_bind_groups[frame_slot])?;
                render.set_vertex_buffer(1, gpu.asset_instance_buffers[frame_slot], 0)?;
                render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;

                let mut bound_material = None;
                let mut bound_vertex_buffer = None;
                for batch in &instance_batches {
                    let Some(stable_id) = batch.stable_ids.first() else {
                        continue;
                    };
                    let mesh = self
                        .asset_meshes
                        .get(stable_id)
                        .ok_or_else(|| format!("asset instance {} disappeared", stable_id))?;
                    debug_assert_eq!(mesh.model_id.0, batch.model_id);
                    let (vertex_buffer, vertex_offset) =
                        asset_mesh_vertex_binding(mesh, gpu, frame_slot)?;
                    if bound_vertex_buffer != Some(vertex_buffer) {
                        render.set_vertex_buffer(0, vertex_buffer, 0)?;
                        bound_vertex_buffer = Some(vertex_buffer);
                    }

                    if mesh.local_draw_ranges.is_empty() {
                        if bound_material != Some(gpu.default_material_bind_group) {
                            render.set_bind_group(1, gpu.default_material_bind_group)?;
                            bound_material = Some(gpu.default_material_bind_group);
                        }
                        render.draw_indexed_range_instanced(
                            mesh.vertex_count,
                            mesh.first_vertex,
                            vertex_offset,
                            batch.instance_count,
                            batch.first_instance,
                        )?;
                        continue;
                    }

                    for range in mesh.local_draw_ranges.iter() {
                        let material_group = range
                            .material_slot
                            .and_then(|slot| self.asset_gpu_materials.get(&(mesh.model_id.0, slot)))
                            .map(|material| material.bind_group)
                            .unwrap_or(gpu.default_material_bind_group);
                        if bound_material != Some(material_group) {
                            render.set_bind_group(1, material_group)?;
                            bound_material = Some(material_group);
                        }
                        render.draw_indexed_range_instanced(
                            range.vertex_count,
                            mesh.first_vertex.saturating_add(range.first_vertex),
                            vertex_offset,
                            batch.instance_count,
                            batch.first_instance,
                        )?;
                    }
                }
            }
            render.end_render_target()?;
        }

        // Build a low-resolution linear scene-depth proxy from the current
        // camera before drawing CloudHat geometry. This is provider-neutral and
        // gives the cloud shader a sampled opaque-scene distance for soft
        // intersection, while the main framebuffer keeps normal hardware depth.
        if let Some(clouds) = self.gpu_atmospheric_clouds.as_ref() {
            let has_resident_cloud = self
                .atmospheric_cloud_runtime
                .iter()
                .any(|runtime| runtime.resident);
            if has_resident_cloud {
                let clear_linear_depth = self.camera.far.max(1.0) * 2.0;
                render.begin_render_target(
                    clouds.soft_depth_render_target,
                    Some([clear_linear_depth, 0.0, 0.0, 0.0]),
                    Some(1.0),
                )?;
                render.set_viewport(clouds.soft_depth_resolution, clouds.soft_depth_resolution)?;
                render.set_scissor(clouds.soft_depth_resolution, clouds.soft_depth_resolution)?;
                render.set_pipeline(clouds.soft_depth_pipeline)?;
                render.set_bind_group(0, clouds.soft_depth_bind_groups[frame_slot])?;

                let dynamic_depth_vertices = self.shadow_vertex_count();
                if dynamic_depth_vertices > 0 {
                    render.set_vertex_buffer(0, gpu.shadow_vertex_buffers[frame_slot], 0)?;
                    render.draw(dynamic_depth_vertices)?;
                }

                if !hiz_draws.is_empty() || !direct_opaque_fallback.is_empty() {
                    render.set_pipeline(clouds.soft_depth_instanced_pipeline)?;
                    render.set_bind_group(0, clouds.soft_depth_bind_groups[frame_slot])?;
                    render.set_vertex_buffer(0, gpu.asset_vertex_buffer, 0)?;
                    render.set_vertex_buffer(1, gpu.asset_instance_buffers[frame_slot], 0)?;
                    render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;

                    // Preserve alpha-test coverage while retaining material multi-draw batches.
                    draw_material_indirect_runs(
                        render,
                        gpu.visibility_indirect_buffers[frame_slot],
                        &hiz_draws,
                    )?;
                    let mut bound_material = None;
                    let mut bound_vertex_buffer = Some(gpu.asset_vertex_buffer);
                    for draw in &direct_opaque_fallback {
                        if bound_material != Some(draw.material_group) {
                            render.set_bind_group(1, draw.material_group)?;
                            bound_material = Some(draw.material_group);
                        }
                        if bound_vertex_buffer != Some(draw.vertex_buffer) {
                            render.set_vertex_buffer(0, draw.vertex_buffer, 0)?;
                            bound_vertex_buffer = Some(draw.vertex_buffer);
                        }
                        render.draw_indexed_range_instanced(
                            draw.index_count,
                            draw.first_index,
                            draw.vertex_offset,
                            draw.instance_count,
                            draw.first_instance,
                        )?;
                    }
                }
                render.end_render_target()?;
            }
        }

        if let Some(pass) = self.gpu_volumetric_clouds.as_ref().and_then(|volume| {
            volume.targets.as_ref().map(|targets| {
                let write_index = (frame_index as usize) & 1;
                (
                    targets.width,
                    targets.height,
                    targets.scene_depth_target,
                    targets.raw_target,
                    targets.history_targets[write_index],
                    volume.depth_pipeline,
                    volume.depth_instanced_pipeline,
                    volume.depth_bind_groups[frame_slot],
                    volume.raymarch_pipeline,
                    targets.raymarch_bind_groups[frame_slot],
                    volume.temporal_pipeline,
                    targets.temporal_bind_groups[frame_slot][write_index],
                    volume.composite_pipeline,
                    targets.composite_bind_groups[write_index],
                    volume.fullscreen_vertex_buffer,
                )
            })
        }) {
            let (
                cloud_width,
                cloud_height,
                depth_target,
                raw_target,
                history_target,
                depth_pipeline,
                depth_instanced_pipeline,
                depth_bind_group,
                raymarch_pipeline,
                raymarch_bind_group,
                temporal_pipeline,
                temporal_bind_group,
                composite_pipeline,
                composite_bind_group,
                fullscreen_vertex_buffer,
            ) = pass;

            let clear_linear_depth = self
                .sky
                .as_ref()
                .map(|sky| sky.volumetric_clouds.max_distance)
                .unwrap_or(self.camera.far)
                .max(self.camera.far)
                * 2.0;

            // Low-resolution opaque scene distance. The raymarch terminates at
            // this distance, so mountains/buildings cut the volume correctly.
            render.begin_render_target(
                depth_target,
                Some([clear_linear_depth, 0.0, 0.0, 0.0]),
                Some(1.0),
            )?;
            render.set_viewport(cloud_width, cloud_height)?;
            render.set_scissor(cloud_width, cloud_height)?;
            render.set_pipeline(depth_pipeline)?;
            render.set_bind_group(0, depth_bind_group)?;

            let dynamic_depth_vertices = self.shadow_vertex_count();
            if dynamic_depth_vertices > 0 {
                render.set_vertex_buffer(0, gpu.shadow_vertex_buffers[frame_slot], 0)?;
                render.draw(dynamic_depth_vertices)?;
            }

            if !hiz_draws.is_empty() || !direct_opaque_fallback.is_empty() {
                render.set_pipeline(depth_instanced_pipeline)?;
                render.set_bind_group(0, depth_bind_group)?;
                render.set_vertex_buffer(0, gpu.asset_vertex_buffer, 0)?;
                render.set_vertex_buffer(1, gpu.asset_instance_buffers[frame_slot], 0)?;
                render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;

                draw_material_indirect_runs(
                    render,
                    gpu.visibility_indirect_buffers[frame_slot],
                    &hiz_draws,
                )?;
                let mut bound_material = None;
                let mut bound_vertex_buffer = Some(gpu.asset_vertex_buffer);
                for draw in &direct_opaque_fallback {
                    if bound_material != Some(draw.material_group) {
                        render.set_bind_group(1, draw.material_group)?;
                        bound_material = Some(draw.material_group);
                    }
                    if bound_vertex_buffer != Some(draw.vertex_buffer) {
                        render.set_vertex_buffer(0, draw.vertex_buffer, 0)?;
                        bound_vertex_buffer = Some(draw.vertex_buffer);
                    }
                    render.draw_indexed_range_instanced(
                        draw.index_count,
                        draw.first_index,
                        draw.vertex_offset,
                        draw.instance_count,
                        draw.first_instance,
                    )?;
                }
            }
            render.end_render_target()?;

            // True participating-medium integration through the world-space
            // altitude slab.
            render.begin_render_target(raw_target, Some([0.0, 0.0, 0.0, 0.0]), None)?;
            render.set_viewport(cloud_width, cloud_height)?;
            render.set_scissor(cloud_width, cloud_height)?;
            render.set_pipeline(raymarch_pipeline)?;
            render.set_bind_group(0, raymarch_bind_group)?;
            render.set_vertex_buffer(0, fullscreen_vertex_buffer, 0)?;
            render.draw(3)?;
            render.end_render_target()?;

            // Reproject previous history into the current camera and clamp it
            // to the current-frame neighborhood before accumulation.
            render.begin_render_target(history_target, Some([0.0, 0.0, 0.0, 0.0]), None)?;
            render.set_viewport(cloud_width, cloud_height)?;
            render.set_scissor(cloud_width, cloud_height)?;
            render.set_pipeline(temporal_pipeline)?;
            render.set_bind_group(0, temporal_bind_group)?;
            render.set_vertex_buffer(0, fullscreen_vertex_buffer, 0)?;
            render.draw(3)?;
            render.end_render_target()?;

            volumetric_composite = Some((
                composite_pipeline,
                composite_bind_group,
                fullscreen_vertex_buffer,
            ));

            let current_view_projection =
                camera_view_projection(&self.camera, width as f32 / height.max(1) as f32);
            if let Some(volume) = self.gpu_volumetric_clouds.as_mut() {
                volume.previous_view_projection = current_view_projection;
                volume.history_valid = true;
            }
        }

        // From this point onward, swapchain work is recorded into the backend
        // draw-list arena and executed by the real RenderGraph. Offscreen shadow
        // and cloud preparation above intentionally remains imperative for this
        // migration step.
        render.set_render_phase(Some("ForwardOpaque"))?;
        render.set_draw_list_kind(Some(RenderDrawListKind::OpaqueForward))?;
        render.set_viewport(width, height)?;
        render.set_scissor(width, height)?;

        if let Some(sky) = self.gpu_sky {
            render.set_pipeline(sky.pipeline)?;
            render.set_bind_group(0, sky.bind_groups[frame_slot])?;
            render.set_vertex_buffer(0, sky.vertex_buffer, 0)?;
            render.set_index_buffer(sky.index_buffer, 0, sky.index_format)?;
            render.draw_indexed(sky.index_count)?;
        }

        // Opaque world geometry is a true MRT GBuffer producer. Background stays
        // isolated in ForwardOpaque so sky/cloud atmosphere cannot masquerade as
        // material geometry during the deferred resolve.
        render.set_render_phase(Some("GBuffer"))?;
        render.set_draw_list_kind(Some(RenderDrawListKind::OpaqueForward))?;

        let weather_material_group = self.weather_material_bind_group(frame_slot)?;
        render.set_pipeline(gpu.gbuffer_pipeline)?;
        render.set_bind_group(0, gpu.bind_groups[frame_slot])?;
        render.set_bind_group(1, gpu.default_material_bind_group)?;
        render.set_bind_group(2, weather_material_group)?;
        let dynamic_vertices = self.vertex_count();
        if dynamic_vertices > 0 {
            render.set_vertex_buffer(0, gpu.vertex_buffers[frame_slot], 0)?;
            render.draw(dynamic_vertices)?;
        }

        let (mass_draws, mass_instance_count) = self.draw_mass_instances(
            render,
            gpu,
            frame_slot,
            weather_material_group,
            width as f32 / height.max(1) as f32,
        )?;

        let mut alpha_draws = Vec::<AlphaAssetDraw>::new();
        for id in &self.frame_plan.visible_entities {
            let Some(instance_index) = visible_instance_slots.get(&id.0).copied() else {
                continue;
            };
            let Some(mesh) = self.asset_meshes.get(&id.0) else {
                continue;
            };
            let distance_sq = self
                .world
                .entity(*id)
                .map(|entity| {
                    let delta = entity.transform.position.sub(self.camera.position);
                    delta.dot(delta)
                })
                .unwrap_or(0.0);

            for range_index in mesh.alpha_draw_range_indices.iter().copied() {
                let range = &mesh.local_draw_ranges[range_index as usize];
                if !self
                    .main_view_mesh_visibility
                    .visible(id.0, &range.mesh_name)
                {
                    continue;
                }
                let material_group = range
                    .material_slot
                    .and_then(|slot| self.asset_gpu_materials.get(&(mesh.model_id.0, slot)))
                    .map(|material| material.bind_group)
                    .unwrap_or(gpu.default_material_bind_group);
                let (vertex_buffer, vertex_offset) =
                    asset_mesh_vertex_binding(mesh, gpu, frame_slot)?;
                alpha_draws.push(AlphaAssetDraw {
                    distance_sq,
                    material_group,
                    vertex_buffer,
                    vertex_offset,
                    first_index: mesh.first_vertex.saturating_add(range.first_vertex),
                    index_count: range.vertex_count,
                    instance_index,
                });
            }
        }

        let opaque_indirect_groups = material_indirect_runs(&hiz_draws).count();
        self.last_submission_stats = RenderSubmissionStats {
            instance_batches: instance_batches.len(),
            instance_count: instance_batches
                .iter()
                .map(|batch| batch.instance_count as usize)
                .sum(),
            hiz_draws: hiz_draws.len(),
            opaque_indirect_groups,
            direct_opaque_draws: direct_opaque_fallback.len(),
            alpha_draws: alpha_draws.len(),
            mass_draws,
            mass_instances: mass_instance_count,
            graph_executed_passes: 0,
            graph_skipped_passes: 0,
            graph_cpu_record_ms: 0.0,
        };

        if !hiz_draws.is_empty() || !direct_opaque_fallback.is_empty() {
            render.set_pipeline(gpu.asset_gbuffer_pipeline)?;
            render.set_bind_group(0, gpu.bind_groups[frame_slot])?;
            render.set_bind_group(2, weather_material_group)?;
            render.set_vertex_buffer(0, gpu.asset_vertex_buffer, 0)?;
            render.set_vertex_buffer(1, gpu.asset_instance_buffers[frame_slot], 0)?;
            render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;
            draw_material_indirect_runs(
                render,
                gpu.visibility_indirect_buffers[frame_slot],
                &hiz_draws,
            )?;
            // The helper left the last material bound.
            let mut bound_material = hiz_draws.last().map(|draw| draw.0);

            let mut bound_vertex_buffer = Some(gpu.asset_vertex_buffer);
            for draw in direct_opaque_fallback {
                if bound_material != Some(draw.material_group) {
                    render.set_bind_group(1, draw.material_group)?;
                    bound_material = Some(draw.material_group);
                }
                if bound_vertex_buffer != Some(draw.vertex_buffer) {
                    render.set_vertex_buffer(0, draw.vertex_buffer, 0)?;
                    bound_vertex_buffer = Some(draw.vertex_buffer);
                }
                render.draw_indexed_range_instanced(
                    draw.index_count,
                    draw.first_index,
                    draw.vertex_offset,
                    draw.instance_count,
                    draw.first_instance,
                )?;
            }
        }

        render.set_render_phase(Some("Transparent"))?;
        render.set_draw_list_kind(Some(RenderDrawListKind::Transparent))?;

        if let Some((pipeline, bind_group, fullscreen_vertex_buffer)) = volumetric_composite {
            render.set_viewport(width, height)?;
            render.set_scissor(width, height)?;
            render.set_pipeline(pipeline)?;
            render.set_bind_group(0, bind_group)?;
            render.set_vertex_buffer(0, fullscreen_vertex_buffer, 0)?;
            render.draw(3)?;
        }

        // Authored atmospheric cloud geometry is deliberately separate from the
        // procedural sky dome. It depth-tests against opaque world geometry,
        // never writes depth, and alpha-blends before ordinary transparent
        // scene surfaces so windows/particles can still composite in front.
        if let Some(clouds) = self.gpu_atmospheric_clouds.as_ref() {
            if !clouds.layers.is_empty() {
                render.set_pipeline(clouds.pipeline)?;
                render.set_bind_group(1, clouds.soft_sample_bind_group)?;
                for (layer, runtime) in clouds
                    .layers
                    .iter()
                    .zip(self.atmospheric_cloud_runtime.iter())
                {
                    if !runtime.resident {
                        continue;
                    }
                    let Some(layer) = layer else {
                        return Err("resident atmospheric cloud has no GPU resources".to_owned());
                    };
                    render.set_bind_group(0, layer.bind_groups[frame_slot])?;
                    render.set_vertex_buffer(0, layer.vertex_buffer, 0)?;
                    render.set_index_buffer(layer.index_buffer, 0, layer.index_format)?;
                    render.draw_indexed(layer.index_count)?;
                }
            }
        }

        // Execute authored GTA WeatherGpuFx after cloud volumes and before
        // ordinary transparent scene materials. The pass depth-tests against
        // opaque geometry while never modifying scene depth.
        self.draw_weather_world_fx(render, frame_slot)?;

        if !alpha_draws.is_empty() {
            alpha_draws.sort_by(|a, b| {
                b.distance_sq
                    .partial_cmp(&a.distance_sq)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            render.set_pipeline(gpu.asset_alpha_pipeline)?;
            render.set_bind_group(0, gpu.bind_groups[frame_slot])?;
            render.set_bind_group(2, weather_material_group)?;
            render.set_vertex_buffer(1, gpu.asset_instance_buffers[frame_slot], 0)?;
            render.set_index_buffer(gpu.asset_index_buffer, 0, "U32")?;
            let mut bound_material = None;
            let mut bound_vertex_buffer = None;
            for draw in alpha_draws {
                if bound_material != Some(draw.material_group) {
                    render.set_bind_group(1, draw.material_group)?;
                    bound_material = Some(draw.material_group);
                }
                if bound_vertex_buffer != Some(draw.vertex_buffer) {
                    render.set_vertex_buffer(0, draw.vertex_buffer, 0)?;
                    bound_vertex_buffer = Some(draw.vertex_buffer);
                }
                render.draw_indexed_range_instanced(
                    draw.index_count,
                    draw.first_index,
                    draw.vertex_offset,
                    1,
                    draw.instance_index,
                )?;
            }
        }

        if particle_alpha_vertices > 0 {
            render.set_pipeline(gpu.alpha_pipeline)?;
            render.set_bind_group(0, gpu.bind_groups[frame_slot])?;
            render.set_bind_group(1, gpu.default_material_bind_group)?;
            render.set_bind_group(2, weather_material_group)?;
            render.set_vertex_buffer(0, gpu.particle_vertex_buffers[frame_slot], 0)?;
            render.draw(particle_alpha_vertices)?;
        }
        if particle_additive_vertices > 0 {
            render.set_pipeline(gpu.particle_additive_pipeline)?;
            render.set_bind_group(0, gpu.bind_groups[frame_slot])?;
            render.set_bind_group(1, gpu.default_material_bind_group)?;
            render.set_bind_group(2, weather_material_group)?;
            render.set_vertex_buffer(
                0,
                gpu.particle_vertex_buffers[frame_slot],
                particle_alpha_vertices as u64 * VERTEX_STRIDE,
            )?;
            render.draw(particle_additive_vertices)?;
        }

        if flare_vertex_count > 0 {
            render.set_pipeline(gpu.flare_pipeline)?;
            render.set_vertex_buffer(0, gpu.flare_vertex_buffers[frame_slot], 0)?;
            render.draw(flare_vertex_count)?;
        }

        // Lens rain/water is deliberately the final 3D transparent layer.
        // UI is submitted afterwards and therefore remains optically clean.
        self.draw_weather_lens_fx(render, frame_slot)?;

        render.set_draw_list_kind(None)?;
        render.set_render_phase(None)?;
        let graph_report = render.submit_render_graph(main_deferred_hdr_render_graph(
            frame_index,
            width,
            height,
        ))?;
        self.last_submission_stats.graph_executed_passes = graph_report.executed_passes;
        self.last_submission_stats.graph_skipped_passes = graph_report.skipped_passes;
        self.last_submission_stats.graph_cpu_record_ms = graph_report.cpu_record_ms;

        overlay()?;
        render.end_frame()
    }
}

#[cfg(test)]
mod submission_tests {
    use super::*;

    #[test]
    fn deferred_graph_exposes_reference_postfx_side_signals() {
        let graph = main_deferred_hdr_render_graph(7, 1920, 1080);
        let kinds = graph
            .passes
            .iter()
            .map(|pass| pass.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                RenderGraphPassKind::ForwardOpaque,
                RenderGraphPassKind::GBuffer,
                RenderGraphPassKind::DeferredLighting,
                RenderGraphPassKind::Transparent,
                RenderGraphPassKind::ScreenSpaceReflections,
                RenderGraphPassKind::BloomExtract,
                RenderGraphPassKind::PostFx,
            ]
        );
        assert!(graph.resources.iter().any(
            |resource| resource.semantic == RenderGraphResourceSemantic::ScreenSpaceReflection
        ));
        assert!(graph
            .resources
            .iter()
            .any(|resource| resource.semantic == RenderGraphResourceSemantic::BloomComposite));
        assert!(graph.resources.iter().any(|resource| resource.semantic
            == RenderGraphResourceSemantic::LitColor
            && resource.format == Some(RenderTextureFormat::Rgba16Float)));
    }

    #[test]
    fn material_runs_preserve_material_and_offset_boundaries() {
        let stride = HIZ_INDIRECT_STRIDE;
        let draws = [
            (7, 0),
            (7, stride),
            (8, 2 * stride),
            (8, 4 * stride),
            (7, 5 * stride),
        ];
        assert_eq!(
            material_indirect_runs(&draws).collect::<Vec<_>>(),
            vec![
                (7, 0, 2),
                (8, 2 * stride, 1),
                (8, 4 * stride, 1),
                (7, 5 * stride, 1)
            ]
        );
        assert_eq!(material_indirect_runs(&[]).count(), 0);
    }

    fn test_batch(ids: &[u64]) -> GpuResidentInstanceBatch {
        GpuResidentInstanceBatch {
            model_id: 7,
            first_instance: 100,
            instance_count: ids.len() as u32,
            stable_ids: ids.to_vec(),
            sphere: [0.0, 0.0, 0.0, 1.0],
        }
    }

    #[test]
    fn visible_batch_runs_coalesce_contiguous_members() {
        let batch = test_batch(&[10, 11, 12, 13, 14, 15, 16]);
        let visible_slots = BTreeMap::from([(10, 100), (11, 101), (13, 103), (14, 104), (15, 105)]);
        assert_eq!(
            visible_batch_runs(&batch, &visible_slots),
            vec![
                VisibleBatchRun {
                    first_member: 0,
                    member_count: 2,
                },
                VisibleBatchRun {
                    first_member: 3,
                    member_count: 3,
                },
            ]
        );
    }

    #[test]
    fn visible_batch_runs_keep_full_batch_single() {
        let batch = test_batch(&[20, 21, 22]);
        let visible_slots = BTreeMap::from([(20, 100), (21, 101), (22, 102)]);
        assert_eq!(
            visible_batch_runs(&batch, &visible_slots),
            vec![VisibleBatchRun {
                first_member: 0,
                member_count: 3,
            }]
        );
    }
}
