use super::*;

pub(super) const MASS_INSTANCE_FLOATS: usize = INSTANCE_FLOATS;
pub(super) const MASS_INSTANCE_STRIDE: u64 = INSTANCE_STRIDE;
pub(super) const MASS_INSTANCE_CHUNK_SIZE: usize = 4_096;
const MASS_INSTANCE_INITIAL_CAPACITY: usize = 16_384;
const MASS_INSTANCE_UPLOAD_BUDGET_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneMassInstanceDesc {
    pub position: [f32; 3],
    pub rotation_degrees: [f32; 3],
    pub scale: [f32; 3],
    pub base_color: [f32; 4],
    pub visible: bool,
}

impl Default for SceneMassInstanceDesc {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            rotation_degrees: [0.0; 3],
            scale: [1.0; 3],
            base_color: [1.0; 4],
            visible: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct MassInstanceChunk {
    min: Vec3,
    max: Vec3,
    first_instance: u32,
    instance_count: u32,
    revision: u64,
    occupied: bool,
}

impl MassInstanceChunk {
    fn new(first_instance: u32) -> Self {
        Self {
            min: Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
            max: Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
            first_instance,
            instance_count: 0,
            revision: 1,
            occupied: false,
        }
    }

    fn expand_sphere(&mut self, center: Vec3, radius: f32) {
        let r = radius.max(0.001);
        self.min.x = self.min.x.min(center.x - r);
        self.min.y = self.min.y.min(center.y - r);
        self.min.z = self.min.z.min(center.z - r);
        self.max.x = self.max.x.max(center.x + r);
        self.max.y = self.max.y.max(center.y + r);
        self.max.z = self.max.z.max(center.z + r);
        self.occupied = true;
    }

    fn sphere(&self) -> Option<[f32; 4]> {
        if !self.occupied {
            return None;
        }
        let center = Vec3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        );
        Some([
            center.x,
            center.y,
            center.z,
            self.max.sub(center).length().max(0.05),
        ])
    }
}

#[derive(Clone, Debug)]
struct MassInstanceLayer {
    base_color: [f32; 4],
    instance_data: Vec<f32>,
    chunks: Vec<MassInstanceChunk>,
    dirty_start_float: usize,
    dirty_end_float: usize,
    generation: u64,
}

impl MassInstanceLayer {
    fn new(base_color: [f32; 4]) -> Self {
        Self {
            base_color,
            instance_data: Vec::new(),
            chunks: Vec::new(),
            dirty_start_float: usize::MAX,
            dirty_end_float: 0,
            generation: 1,
        }
    }

    fn instance_count(&self) -> usize {
        self.instance_data.len() / MASS_INSTANCE_FLOATS
    }

    fn mark_dirty_slot(&mut self, slot: usize) {
        let start = slot.saturating_mul(MASS_INSTANCE_FLOATS);
        let end = start.saturating_add(MASS_INSTANCE_FLOATS);
        self.dirty_start_float = self.dirty_start_float.min(start);
        self.dirty_end_float = self.dirty_end_float.max(end);
        self.generation = self.generation.wrapping_add(1).max(1);
        let chunk_index = slot / MASS_INSTANCE_CHUNK_SIZE;
        if let Some(chunk) = self.chunks.get_mut(chunk_index) {
            chunk.revision = self.generation;
        }
    }

    fn dirty_range(&self) -> Option<(usize, usize)> {
        (self.dirty_start_float < self.dirty_end_float)
            .then_some((self.dirty_start_float, self.dirty_end_float))
    }

    fn clear_dirty(&mut self) {
        self.dirty_start_float = usize::MAX;
        self.dirty_end_float = 0;
    }

    fn ensure_chunk(&mut self, chunk_index: usize) -> Result<(), String> {
        while self.chunks.len() <= chunk_index {
            let first = self
                .chunks
                .len()
                .checked_mul(MASS_INSTANCE_CHUNK_SIZE)
                .ok_or_else(|| "mass-instance chunk offset overflow".to_owned())?;
            let first = u32::try_from(first)
                .map_err(|_| "mass-instance first slot exceeds u32".to_owned())?;
            self.chunks.push(MassInstanceChunk::new(first));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct MassInstanceStore {
    layers: BTreeMap<String, MassInstanceLayer>,
}

#[derive(Clone, Debug)]
pub(super) struct GpuMassInstanceLayer {
    vertex_buffer: u32,
    instance_buffer: u32,
    instance_capacity: u32,
    chunk_generations: Vec<u64>,
    chunk_instance_counts: Vec<u32>,
}

fn validate_layer_name(layer: &str) -> Result<&str, String> {
    let layer = layer.trim();
    if layer.is_empty() || layer.len() > 96 {
        return Err("mass-instance layer name must contain 1..=96 bytes".to_owned());
    }
    Ok(layer)
}

fn validate_desc(desc: SceneMassInstanceDesc) -> Result<(), String> {
    if desc
        .position
        .iter()
        .chain(desc.rotation_degrees.iter())
        .chain(desc.scale.iter())
        .chain(desc.base_color.iter())
        .any(|value| !value.is_finite())
    {
        return Err("mass-instance transform/color must be finite".to_owned());
    }
    if desc.visible && desc.scale.iter().any(|value| value.abs() <= 1.0e-6) {
        return Err("visible mass-instance scale must be non-zero".to_owned());
    }
    if desc
        .base_color
        .iter()
        .any(|value| *value < 0.0 || *value > 64.0)
    {
        return Err("mass-instance base_color must be in 0..=64".to_owned());
    }
    Ok(())
}

fn base_colors_compatible(a: [f32; 4], b: [f32; 4]) -> bool {
    a.into_iter().zip(b).all(|(a, b)| (a - b).abs() <= 1.0e-4)
}

fn chunk_visible(camera: &Camera, aspect: f32, sphere: [f32; 4]) -> bool {
    let center = Vec3::new(sphere[0], sphere[1], sphere[2]);
    let radius = sphere[3].max(0.0);
    let forward = camera.target.sub(camera.position).normalized();
    let right = forward.cross(camera.up).normalized();
    let up = right.cross(forward).normalized();
    let delta = center.sub(camera.position);
    let depth = delta.dot(forward);

    if depth + radius < camera.near || depth - radius > camera.far {
        return false;
    }
    if depth + radius <= 0.0 {
        return false;
    }

    let tan_y = (camera.fov_y_degrees.to_radians() * 0.5).tan().max(0.0001);
    let tan_x = tan_y * aspect.max(0.0001);
    let horizontal = delta.dot(right).abs();
    let vertical = delta.dot(up).abs();
    horizontal <= depth.max(0.0) * tan_x + radius * (1.0 + tan_x)
        && vertical <= depth.max(0.0) * tan_y + radius * (1.0 + tan_y)
}

impl Scene3dRuntime {
    pub fn populate_mass_instance_debug_grid(
        &mut self,
        layer_name: &str,
        count: u32,
        spacing: f32,
        base_color: [f32; 4],
    ) -> Result<(), String> {
        let layer_name = validate_layer_name(layer_name)?;
        if count == 0 || count > 8_000_000 {
            return Err("mass-instance debug grid count must be in 1..=8_000_000".to_owned());
        }
        if !spacing.is_finite() || !(0.01..=8.0).contains(&spacing) {
            return Err("mass-instance debug grid spacing must be in 0.01..=8".to_owned());
        }
        if base_color
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0 || *value > 64.0)
        {
            return Err("mass-instance debug grid color is invalid".to_owned());
        }

        let count_usize = count as usize;
        let mut layer = MassInstanceLayer::new(base_color);
        layer.instance_data = Vec::with_capacity(
            count_usize
                .checked_mul(MASS_INSTANCE_FLOATS)
                .ok_or_else(|| "mass-instance debug allocation overflow".to_owned())?,
        );
        let width = (count as f64).sqrt().ceil() as u32;
        let half_width = width as f32 * spacing * 0.5;
        let tile_side = (MASS_INSTANCE_CHUNK_SIZE as f64).sqrt() as u32;
        debug_assert_eq!(
            tile_side as usize * tile_side as usize,
            MASS_INSTANCE_CHUNK_SIZE
        );
        let tiles_x = width.div_ceil(tile_side);

        // Pack each 4096-record GPU chunk as one spatial 64x64 tile instead
        // of a long row-major strip. Chunk bounds remain local, so CPU/GPU
        // culling can reject whole tiles without touching individual records.
        for slot in 0..count_usize {
            let slot_u32 = slot as u32;
            let chunk_index = slot_u32 / MASS_INSTANCE_CHUNK_SIZE as u32;
            let local = slot_u32 % MASS_INSTANCE_CHUNK_SIZE as u32;
            let tile_x = chunk_index % tiles_x;
            let tile_z = chunk_index / tiles_x;
            let gx = tile_x * tile_side + local % tile_side;
            let gz = tile_z * tile_side + local / tile_side;
            let jitter = ((slot_u32.wrapping_mul(1_664_525).wrapping_add(1_013_904_223) >> 8)
                & 1023) as f32
                / 1023.0;
            let position = Vec3::new(
                gx as f32 * spacing - half_width + (jitter - 0.5) * spacing * 0.35,
                0.035 + (slot_u32 % 7) as f32 * 0.001,
                gz as f32 * spacing - half_width - (jitter - 0.5) * spacing * 0.35,
            );
            let rotation = Vec3::new(
                (slot_u32 % 37) as f32 * 3.7,
                (slot_u32 % 89) as f32 * 2.1,
                (slot_u32 % 53) as f32 * 4.3,
            );
            let scale = Vec3::new(
                0.025 + (slot_u32 % 11) as f32 * 0.002,
                0.010 + (slot_u32 % 5) as f32 * 0.001,
                0.008 + (slot_u32 % 3) as f32 * 0.001,
            );
            layer
                .instance_data
                .extend_from_slice(&geometry::instance_model_matrix(position, rotation, scale));

            let chunk_index = slot / MASS_INSTANCE_CHUNK_SIZE;
            layer.ensure_chunk(chunk_index)?;
            let local_member = slot % MASS_INSTANCE_CHUNK_SIZE;
            let chunk = &mut layer.chunks[chunk_index];
            chunk.instance_count = chunk.instance_count.max((local_member + 1) as u32);
            chunk.expand_sphere(position, scale.mul(0.5).length());
        }
        layer.dirty_start_float = 0;
        layer.dirty_end_float = layer.instance_data.len();
        layer.generation = layer.generation.wrapping_add(1).max(1);
        for chunk in &mut layer.chunks {
            chunk.revision = layer.generation;
        }
        self.mass_instances
            .layers
            .insert(layer_name.to_owned(), layer);
        Ok(())
    }

    pub fn upsert_mass_instance(
        &mut self,
        layer_name: &str,
        slot: u32,
        desc: SceneMassInstanceDesc,
    ) -> Result<(), String> {
        let layer_name = validate_layer_name(layer_name)?;
        validate_desc(desc)?;

        let layer = self
            .mass_instances
            .layers
            .entry(layer_name.to_owned())
            .or_insert_with(|| MassInstanceLayer::new(desc.base_color));
        if !base_colors_compatible(layer.base_color, desc.base_color) {
            return Err(format!(
                "mass-instance layer '{}' base_color is immutable after creation",
                layer_name
            ));
        }

        let slot = slot as usize;
        let required_floats = slot
            .checked_add(1)
            .and_then(|value| value.checked_mul(MASS_INSTANCE_FLOATS))
            .ok_or_else(|| "mass-instance slot allocation overflow".to_owned())?;
        if layer.instance_data.len() < required_floats {
            layer.instance_data.resize(required_floats, 0.0);
        }

        let start = slot * MASS_INSTANCE_FLOATS;
        let end = start + MASS_INSTANCE_FLOATS;
        if desc.visible {
            let matrix = geometry::instance_model_matrix(
                Vec3::new(desc.position[0], desc.position[1], desc.position[2]),
                Vec3::new(
                    desc.rotation_degrees[0],
                    desc.rotation_degrees[1],
                    desc.rotation_degrees[2],
                ),
                Vec3::new(desc.scale[0], desc.scale[1], desc.scale[2]),
            );
            layer.instance_data[start..end].copy_from_slice(&matrix);

            let chunk_index = slot / MASS_INSTANCE_CHUNK_SIZE;
            layer.ensure_chunk(chunk_index)?;
            let local_member = slot % MASS_INSTANCE_CHUNK_SIZE;
            let chunk = &mut layer.chunks[chunk_index];
            chunk.instance_count = chunk.instance_count.max(
                u32::try_from(local_member + 1)
                    .map_err(|_| "mass-instance chunk member exceeds u32".to_owned())?,
            );
            let half = Vec3::new(
                desc.scale[0].abs() * 0.5,
                desc.scale[1].abs() * 0.5,
                desc.scale[2].abs() * 0.5,
            );
            chunk.expand_sphere(
                Vec3::new(desc.position[0], desc.position[1], desc.position[2]),
                half.length(),
            );
        } else {
            layer.instance_data[start..end].fill(0.0);
        }
        layer.mark_dirty_slot(slot);
        Ok(())
    }

    pub fn set_mass_instance_visible(
        &mut self,
        layer_name: &str,
        slot: u32,
        visible: bool,
    ) -> Result<bool, String> {
        let layer_name = validate_layer_name(layer_name)?;
        let Some(layer) = self.mass_instances.layers.get_mut(layer_name) else {
            return Ok(false);
        };
        let slot = slot as usize;
        if slot >= layer.instance_count() {
            return Ok(false);
        }
        if visible {
            return Err(
                "mass-instance visibility restore requires scene.mass_instance.upsert with transform"
                    .to_owned(),
            );
        }
        let start = slot * MASS_INSTANCE_FLOATS;
        let end = start + MASS_INSTANCE_FLOATS;
        layer.instance_data[start..end].fill(0.0);
        layer.mark_dirty_slot(slot);
        Ok(true)
    }

    pub fn clear_mass_instance_layer(&mut self, layer_name: &str) -> Result<bool, String> {
        let layer_name = validate_layer_name(layer_name)?;
        Ok(self.mass_instances.layers.remove(layer_name).is_some())
    }

    pub fn mass_instance_count(&self) -> usize {
        self.mass_instances
            .layers
            .values()
            .map(MassInstanceLayer::instance_count)
            .sum()
    }

    pub(super) fn mass_instance_chunk_count(&self) -> usize {
        self.mass_instances
            .layers
            .values()
            .map(|layer| layer.chunks.len())
            .sum()
    }

    pub(super) fn sync_mass_instance_gpu(
        &mut self,
        render: &RenderClient,
        aspect: f32,
    ) -> Result<(), String> {
        let stale = self
            .gpu_mass_instances
            .keys()
            .filter(|name| !self.mass_instances.layers.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>();
        for name in stale {
            if let Some(gpu) = self.gpu_mass_instances.remove(&name) {
                render.destroy_buffer(gpu.instance_buffer);
                render.destroy_buffer(gpu.vertex_buffer);
            }
        }

        let names = self
            .mass_instances
            .layers
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for name in names {
            let (required, chunk_count) = self
                .mass_instances
                .layers
                .get(&name)
                .map(|layer| (layer.instance_count(), layer.chunks.len()))
                .unwrap_or((0, 0));
            if required == 0 {
                continue;
            }
            let required_u32 = u32::try_from(required)
                .map_err(|_| format!("mass-instance layer '{}' exceeds u32 slots", name))?;

            let needs_create = !self.gpu_mass_instances.contains_key(&name);
            let needs_grow = self
                .gpu_mass_instances
                .get(&name)
                .is_some_and(|gpu| required_u32 > gpu.instance_capacity);

            if needs_create || needs_grow {
                let base_color = self.mass_instances.layers[&name].base_color;
                let (vertex_buffer, old_instance_buffer) = if let Some(current) =
                    self.gpu_mass_instances.get(&name)
                {
                    (current.vertex_buffer, Some(current.instance_buffer))
                } else {
                    let unit_cube = Cube {
                        position: Vec3::ZERO,
                        rotation_degrees: Vec3::ZERO,
                        scale: Vec3::ONE,
                        base_color,
                    };
                    let mut vertices =
                        Vec::<f32>::with_capacity(CUBE_VERTEX_COUNT as usize * FLOATS_PER_VERTEX);
                    unit_cube.append_vertices(&mut vertices);
                    let vertex_buffer = render.create_buffer(
                        &format!("newviso.mass_instances.{}.mesh", name),
                        vertices.len() as u64 * std::mem::size_of::<f32>() as u64,
                        "Vertex",
                        "GpuOnly",
                    )?;
                    render.write_buffer_f32(vertex_buffer, 0, &vertices)?;
                    (vertex_buffer, None)
                };

                let capacity = required
                    .max(MASS_INSTANCE_INITIAL_CAPACITY)
                    .checked_next_power_of_two()
                    .ok_or_else(|| "mass-instance capacity overflow".to_owned())?;
                let capacity_u32 = u32::try_from(capacity)
                    .map_err(|_| "mass-instance capacity exceeds u32".to_owned())?;
                let instance_buffer = render.create_buffer(
                    &format!(
                        "newviso.mass_instances.{}.instances.capacity_{}",
                        name, capacity
                    ),
                    capacity as u64 * MASS_INSTANCE_STRIDE,
                    "Vertex",
                    "GpuOnly",
                )?;

                if let Some(old) = old_instance_buffer {
                    render.destroy_buffer(old);
                }
                self.gpu_mass_instances.insert(
                    name.clone(),
                    GpuMassInstanceLayer {
                        vertex_buffer,
                        instance_buffer,
                        instance_capacity: capacity_u32,
                        chunk_generations: vec![0; chunk_count],
                        chunk_instance_counts: vec![0; chunk_count],
                    },
                );
                // The new GPU buffer starts empty. CPU dirtiness is subsumed by
                // on-demand full-chunk residency below.
                self.mass_instances
                    .layers
                    .get_mut(&name)
                    .expect("layer exists")
                    .clear_dirty();
            } else {
                let gpu = self
                    .gpu_mass_instances
                    .get_mut(&name)
                    .expect("GPU mass layer exists");
                if gpu.chunk_generations.len() < chunk_count {
                    gpu.chunk_generations.resize(chunk_count, 0);
                    gpu.chunk_instance_counts.resize(chunk_count, 0);
                }
            }

            // Small edits to already resident chunks (wake/hide/move) should
            // remain 64-byte record updates, not 256 KiB page rewrites.
            if let Some((dirty_start, dirty_end)) = self.mass_instances.layers[&name].dirty_range()
            {
                let first_slot = dirty_start / MASS_INSTANCE_FLOATS;
                let last_slot_exclusive = dirty_end.div_ceil(MASS_INSTANCE_FLOATS);
                let first_chunk = first_slot / MASS_INSTANCE_CHUNK_SIZE;
                let last_chunk_exclusive = last_slot_exclusive
                    .div_ceil(MASS_INSTANCE_CHUNK_SIZE)
                    .min(chunk_count);

                let mut ranges = Vec::<(usize, usize)>::new();
                let mut updated_chunks = Vec::<usize>::new();
                {
                    let layer = &self.mass_instances.layers[&name];
                    let gpu = &self.gpu_mass_instances[&name];
                    for chunk_index in first_chunk..last_chunk_exclusive {
                        if gpu.chunk_generations[chunk_index] == 0 {
                            continue;
                        }
                        let chunk = &layer.chunks[chunk_index];
                        let chunk_start = chunk.first_instance as usize * MASS_INSTANCE_FLOATS;
                        let chunk_end = (chunk.first_instance as usize
                            + chunk.instance_count as usize)
                            * MASS_INSTANCE_FLOATS;
                        let start = dirty_start.max(chunk_start);
                        let end = dirty_end.min(chunk_end);
                        if start < end {
                            // If the chunk grew, upload the complete current
                            // chunk so newly exposed gaps are guaranteed zero.
                            if gpu.chunk_instance_counts[chunk_index] != chunk.instance_count {
                                ranges.push((chunk_start, chunk_end));
                            } else {
                                ranges.push((start, end));
                            }
                            updated_chunks.push(chunk_index);
                        }
                    }
                }
                if !ranges.is_empty() {
                    let layer = &self.mass_instances.layers[&name];
                    let buffer = self.gpu_mass_instances[&name].instance_buffer;
                    render.write_buffer_f32_ranges(buffer, &layer.instance_data, &ranges)?;
                    let gpu = self
                        .gpu_mass_instances
                        .get_mut(&name)
                        .expect("GPU mass layer exists");
                    let layer = &self.mass_instances.layers[&name];
                    for chunk_index in updated_chunks {
                        gpu.chunk_generations[chunk_index] = layer.chunks[chunk_index].revision;
                        gpu.chunk_instance_counts[chunk_index] =
                            layer.chunks[chunk_index].instance_count;
                    }
                }
                self.mass_instances
                    .layers
                    .get_mut(&name)
                    .expect("layer exists")
                    .clear_dirty();
            }

            // Stream only visible chunks that are absent/stale on the GPU.
            // 4096 instances * 64 B = 256 KiB per page; with an 8 MiB frame
            // budget this admits up to 32 spatial pages in one binary batch.
            let mut ranges = Vec::<(usize, usize)>::new();
            let mut admitted_chunks = Vec::<usize>::new();
            let mut admitted_bytes = 0usize;
            {
                let layer = &self.mass_instances.layers[&name];
                let gpu = &self.gpu_mass_instances[&name];
                for (chunk_index, chunk) in layer.chunks.iter().enumerate() {
                    if gpu.chunk_generations[chunk_index] == chunk.revision
                        && gpu.chunk_instance_counts[chunk_index] == chunk.instance_count
                    {
                        continue;
                    }
                    let Some(sphere) = chunk.sphere() else {
                        continue;
                    };
                    if !chunk_visible(&self.camera, aspect, sphere) || chunk.instance_count == 0 {
                        continue;
                    }

                    let start = chunk.first_instance as usize * MASS_INSTANCE_FLOATS;
                    let end = (chunk.first_instance as usize + chunk.instance_count as usize)
                        * MASS_INSTANCE_FLOATS;
                    let bytes = (end - start).saturating_mul(std::mem::size_of::<f32>());
                    if !ranges.is_empty()
                        && admitted_bytes.saturating_add(bytes) > MASS_INSTANCE_UPLOAD_BUDGET_BYTES
                    {
                        break;
                    }
                    ranges.push((start, end));
                    admitted_chunks.push(chunk_index);
                    admitted_bytes = admitted_bytes.saturating_add(bytes);
                }
            }

            if !ranges.is_empty() {
                let layer = &self.mass_instances.layers[&name];
                let buffer = self.gpu_mass_instances[&name].instance_buffer;
                render.write_buffer_f32_ranges(buffer, &layer.instance_data, &ranges)?;
                let gpu = self
                    .gpu_mass_instances
                    .get_mut(&name)
                    .expect("GPU mass layer exists");
                let layer = &self.mass_instances.layers[&name];
                for chunk_index in admitted_chunks {
                    gpu.chunk_generations[chunk_index] = layer.chunks[chunk_index].revision;
                    gpu.chunk_instance_counts[chunk_index] =
                        layer.chunks[chunk_index].instance_count;
                }
            }
        }
        Ok(())
    }

    pub(super) fn draw_mass_instances(
        &self,
        render: &RenderClient,
        gpu: GpuScene,
        frame_slot: usize,
        weather_material_group: u32,
        aspect: f32,
    ) -> Result<(usize, usize), String> {
        if self.mass_instances.layers.is_empty() {
            return Ok((0, 0));
        }

        render.set_pipeline(gpu.asset_gbuffer_pipeline)?;
        render.set_bind_group(0, gpu.bind_groups[frame_slot])?;
        render.set_bind_group(1, gpu.default_material_bind_group)?;
        render.set_bind_group(2, weather_material_group)?;

        let mut draws = 0usize;
        let mut instances = 0usize;
        for (name, layer) in &self.mass_instances.layers {
            let Some(gpu_layer) = self.gpu_mass_instances.get(name) else {
                continue;
            };
            render.set_vertex_buffer(0, gpu_layer.vertex_buffer, 0)?;
            render.set_vertex_buffer(1, gpu_layer.instance_buffer, 0)?;
            for chunk in &layer.chunks {
                let Some(sphere) = chunk.sphere() else {
                    continue;
                };
                if !chunk_visible(&self.camera, aspect, sphere) || chunk.instance_count == 0 {
                    continue;
                }
                let chunk_index = chunk.first_instance as usize / MASS_INSTANCE_CHUNK_SIZE;
                if gpu_layer
                    .chunk_generations
                    .get(chunk_index)
                    .copied()
                    .unwrap_or(0)
                    != chunk.revision
                    || gpu_layer
                        .chunk_instance_counts
                        .get(chunk_index)
                        .copied()
                        .unwrap_or(0)
                        != chunk.instance_count
                {
                    continue;
                }
                render.draw_range_instanced(
                    CUBE_VERTEX_COUNT,
                    0,
                    chunk.instance_count,
                    chunk.first_instance,
                )?;
                draws += 1;
                instances += chunk.instance_count as usize;
            }
        }
        Ok((draws, instances))
    }

    pub(super) fn shutdown_mass_instance_gpu(&mut self, render: &RenderClient) {
        for gpu in std::mem::take(&mut self.gpu_mass_instances).into_values() {
            render.destroy_buffer(gpu.instance_buffer);
            render.destroy_buffer(gpu.vertex_buffer);
        }
    }
}
