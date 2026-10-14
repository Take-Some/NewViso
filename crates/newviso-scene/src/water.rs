//! Shared dynamic ocean height, normal and velocity queries in metres and seconds.
use std::f32::consts::TAU;

/// World-anchored dynamic water uses the PC reference 256 x 256, two-metre grid.
pub const GRID_SIZE: usize = 256;
pub const CELL_METRES: f32 = 2.0;
const FIXED_DT: f64 = 1.0 / 60.0;

// Blend the moving local field into the level far ocean over 32 metres.
// The solver retains its source forcing. Physics queries and GPU packing
// apply the same envelope to nodes before bilinear interpolation.
const EDGE_BLEND_CELLS: usize = 16;
const EDGE_ENVELOPE: [f32; GRID_SIZE] = {
    let mut values = [1.0; GRID_SIZE];
    let mut i = 0;
    while i < GRID_SIZE {
        let opposite = GRID_SIZE - 1 - i;
        let distance = if i < opposite { i } else { opposite };
        if distance < EDGE_BLEND_CELLS {
            let t = distance as f32 / EDGE_BLEND_CELLS as f32;
            values[i] = t * t * (3.0 - 2.0 * t);
        }
        i += 1;
    }
    values
};

#[derive(Clone, Debug)]
pub struct WaveTexture {
    size: usize,
    values: Vec<f32>,
}

#[derive(Clone, Copy)]
struct WaveAxis {
    first: usize,
    second: usize,
    blend: f32,
}

impl WaveTexture {
    /// The authored ocean/shore wave textures are linear L8, not normal maps.
    pub fn from_dds(bytes: &[u8]) -> Result<Self, String> {
        let word = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        if bytes.len() < 128 || &bytes[..4] != b"DDS " {
            return Err("water wave source is not a DDS".into());
        }
        let size = word(16) as usize;
        if size < 2
            || size > 1024
            || word(12) as usize != size
            || word(88) != 8
            || word(92) != 255
            || bytes[84..88] != [0; 4]
            || bytes.len() < 128 + size * size
        {
            return Err("water forcing requires a square linear L8 DDS".into());
        }
        Ok(Self {
            size,
            values: bytes[128..128 + size * size]
                .iter()
                .map(|v| f32::from(*v) / 256.0)
                .collect(),
        })
    }

    fn sample(&self, uv: [f32; 2]) -> f32 {
        // CCPUTexture uses width-1 coordinates, four wrapped fetches and
        // fixed-point L8 conversion. Preserve that convention for forcing.
        self.sample_axes(self.axis(uv[0]), self.axis(uv[1]))
    }

    fn axis(&self, uv: f32) -> WaveAxis {
        let maximum = (self.size - 1) as f32;
        let texel = 1.0 / maximum;
        WaveAxis {
            first: (uv.rem_euclid(1.0) * maximum) as usize,
            second: ((uv + texel).rem_euclid(1.0) * maximum) as usize,
            blend: (uv * maximum).fract(),
        }
    }

    fn sample_axes(&self, x: WaveAxis, y: WaveAxis) -> f32 {
        let a = self.values[y.first * self.size + x.first] * (1.0 - x.blend)
            + self.values[y.first * self.size + x.second] * x.blend;
        let b = self.values[y.second * self.size + x.first] * (1.0 - x.blend)
            + self.values[y.second * self.size + x.second] * x.blend;
        a * (1.0 - y.blend) + b * y.blend
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WaterTuning {
    pub ocean_amplitude: f32,
    pub shore_wave_amplitude: f32,
    pub noise_amplitude: f32,
    pub ripple_scale: f32,
    pub ripple_bumpiness: f32,
    pub ocean_bumpiness: f32,
    pub foam_intensity: f32,
    pub foam_scale: f32,
}

impl Default for WaterTuning {
    fn default() -> Self {
        Self {
            ocean_amplitude: 0.30,
            shore_wave_amplitude: 1.0,
            noise_amplitude: 0.10,
            ripple_scale: 0.10,
            ripple_bumpiness: 0.12,
            ocean_bumpiness: 0.10,
            foam_intensity: 0.20,
            foam_scale: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct WaveQuad {
    min: [f32; 2],
    max: [f32; 2],
    direction: [f32; 2],
    amplitude: f32,
}

#[derive(Clone, Copy, Debug)]
struct CalmingQuad {
    min: [f32; 2],
    max: [f32; 2],
    damping: f32,
}

#[derive(Debug, Default)]
pub struct WaterSimulation {
    pub surface: WaterSurface,
    pub tuning: WaterTuning,
    pub driven_by_physics: bool,
    pub enabled: bool,
    time_seconds: f64,
    accumulator: f64,
    origin: [f32; 2],
    heights: Vec<f32>,
    previous: Vec<f32>,
    velocities: Vec<f32>,
    foam: Vec<f32>,
    wet_heights: Vec<f32>,
    wet_scratch: Vec<f32>,
    foam_scratch: Vec<f32>,
    pub optics: WaterOpticsSettings,
    pub immersion: WaterImmersion,
    ocean: Option<WaveTexture>,
    shore: Option<WaveTexture>,
    wave_quads: Vec<WaveQuad>,
    calming_quads: Vec<CalmingQuad>,
    random: u32,
    field_revision: u64,
}

impl WaterSimulation {
    /// Authored rectangular wave and calming regions, equivalent to the
    /// reference's wave-quads and calming-quads rather than an island formula.
    pub fn configure_regions(&mut self, config: &serde_json::Value) -> Result<(), String> {
        if config.is_null() {
            return Ok(());
        }
        if !config.is_object() {
            return Err("scene water must be an object".into());
        }
        let vector = |record: &serde_json::Value, key: &str| -> Result<[f32; 2], String> {
            let a = record
                .get(key)
                .and_then(serde_json::Value::as_array)
                .filter(|a| a.len() == 2)
                .ok_or_else(|| format!("water region requires '{key}' [x,z]"))?;
            let mut result = [0.0; 2];
            for (i, value) in a.iter().enumerate() {
                result[i] = value
                    .as_f64()
                    .ok_or_else(|| format!("water region '{key}' must be numeric"))?
                    as f32;
                if !result[i].is_finite() {
                    return Err("water region must be finite".into());
                }
            }
            Ok(result)
        };
        let bounds = |record: &serde_json::Value| -> Result<([f32; 2], [f32; 2]), String> {
            let min = vector(record, "min")?;
            let max = vector(record, "max")?;
            if min[0] >= max[0] || min[1] >= max[1] {
                return Err("water region min must precede max".into());
            }
            Ok((min, max))
        };
        let mut waves = Vec::new();
        let mut calming = Vec::new();
        for (key, output) in [("wave_quads", true), ("calming_quads", false)] {
            let Some(records) = config.get(key) else {
                continue;
            };
            let records = records
                .as_array()
                .filter(|a| a.len() <= 2048)
                .ok_or_else(|| {
                    format!("water '{key}' must be an array with at most 2048 regions")
                })?;
            for record in records {
                let (min, max) = bounds(record)?;
                if output {
                    let direction = vector(record, "direction")?;
                    let length = direction[0].hypot(direction[1]);
                    let amplitude = record
                        .get("amplitude")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(1.0) as f32;
                    if length <= 1e-5
                        || !amplitude.is_finite()
                        || !(0.0..=20.0).contains(&amplitude)
                    {
                        return Err("water wave direction/amplitude invalid".into());
                    }
                    waves.push(WaveQuad {
                        min,
                        max,
                        direction: direction.map(|v| v / length),
                        amplitude,
                    });
                } else {
                    let damping = record
                        .get("damping")
                        .and_then(serde_json::Value::as_f64)
                        .ok_or("water calming region requires damping")?
                        as f32;
                    if !damping.is_finite() || !(0.0..=1.0).contains(&damping) {
                        return Err("water calming damping must be between zero and one".into());
                    }
                    calming.push(CalmingQuad { min, max, damping });
                }
            }
        }
        self.wave_quads = waves;
        self.calming_quads = calming;
        Ok(())
    }

    pub fn region_counts(&self) -> [usize; 2] {
        [self.wave_quads.len(), self.calming_quads.len()]
    }

    pub fn load_sources(&mut self, ocean: &[u8], shore: &[u8]) -> Result<(), String> {
        self.ocean = Some(WaveTexture::from_dds(ocean)?);
        self.shore = Some(WaveTexture::from_dds(shore)?);
        self.heights = vec![0.0; GRID_SIZE * GRID_SIZE];
        self.previous = self.heights.clone();
        self.velocities = self.heights.clone();
        self.foam = self.heights.clone();
        self.wet_heights = self.heights.clone();
        self.wet_scratch = self.heights.clone();
        self.foam_scratch = self.heights.clone();
        self.origin = [-(GRID_SIZE as f32) * CELL_METRES * 0.5; 2];
        self.random = 0x81e5_49ad;
        self.field_revision = self.field_revision.wrapping_add(1).max(1);
        self.enabled = true;
        Ok(())
    }

    pub fn seconds(&self) -> f64 {
        self.time_seconds
    }
    pub fn origin(&self) -> [f32; 2] {
        self.origin
    }
    pub fn field_revision(&self) -> u64 {
        self.field_revision
    }
    pub fn source_loaded(&self) -> bool {
        self.ocean.is_some()
    }

    /// Move only whole cells and preserve the world positions of retained waves.
    pub fn track_focus(&mut self, focus: [f32; 2]) {
        if self.heights.is_empty() || !focus.iter().all(|v| v.is_finite()) {
            return;
        }
        let next = focus.map(|v| {
            (v / CELL_METRES).floor() * CELL_METRES - (GRID_SIZE as f32) * CELL_METRES * 0.5
        });
        let delta = [
            ((next[0] - self.origin[0]) / CELL_METRES) as i32,
            ((next[1] - self.origin[1]) / CELL_METRES) as i32,
        ];
        if delta == [0; 2] {
            return;
        }
        for field in [
            &mut self.heights,
            &mut self.previous,
            &mut self.velocities,
            &mut self.foam,
            &mut self.wet_heights,
        ] {
            shift_water_field(field, delta);
        }
        self.origin = next;
        self.field_revision = self.field_revision.wrapping_add(1).max(1);
    }

    pub fn advance(&mut self, dt: f32) {
        if !self.enabled || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        // A bounded backlog is integrated in stable substeps; a hitch cannot
        // inject one giant Euler update or advance only the visual wave phase.
        self.accumulator = (self.accumulator + f64::from(dt)).min(FIXED_DT * 8.0);
        while self.accumulator + 1e-8 >= FIXED_DT {
            self.previous.copy_from_slice(&self.heights);
            self.integrate(FIXED_DT as f32 * 0.75);
            self.time_seconds += FIXED_DT;
            self.accumulator = (self.accumulator - FIXED_DT).max(0.0);
        }
    }

    fn integrate(&mut self, dt: f32) {
        let Some(ocean) = self.ocean.as_ref() else {
            return;
        };
        let n = GRID_SIZE;
        let damping = 0.7_f32.powf(dt);
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        let noise = [
            (self.random & 0xffff) as f32 / 65536.0,
            (self.random >> 16) as f32 / 65536.0,
        ];
        let seconds = self.time_seconds as f32;
        let phase = (self.time_seconds * 1000.0).rem_euclid(4000.0) as f32 * (TAU / 4000.0) / 64.0;
        let gain = self.tuning.ocean_amplitude.max(0.0);
        let disturb = (gain * 3.0).max(self.tuning.noise_amplitude.max(0.0));
        // Forcing UVs are separable in x/z. Cache the 256 axis coordinates
        // instead of repeating wrapped texture addressing for all 65,536 cells.
        // Arithmetic and wrapped L8 interpolation remain identical.
        let wave_x: [WaveAxis; GRID_SIZE] = std::array::from_fn(|x|
            ocean.axis((self.origin[0] + x as f32 * CELL_METRES) / 512.0 + seconds / 400.0));
        let wave_y: [WaveAxis; GRID_SIZE] = std::array::from_fn(|y|
            ocean.axis((self.origin[1] + y as f32 * CELL_METRES) / 512.0 + phase));
        let noise_x: [WaveAxis; GRID_SIZE] = std::array::from_fn(|y|
            ocean.axis(noise[0] + y as f32 / 32.0));
        let noise_y: [WaveAxis; GRID_SIZE] = std::array::from_fn(|x|
            ocean.axis(noise[1] + x as f32 / 32.0));
        let sample = |x: usize, y: usize| self.heights[y * n + x];
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                let xm = x.saturating_sub(1);
                let xp = (x + 1).min(n - 1);
                let ym = y.saturating_sub(1);
                let yp = (y + 1).min(n - 1);
                // Exact reference stencil, spring, equilibrium and exponential drag.
                let average = 0.17
                    * (sample(xm, y) + sample(xp, y) + sample(x, ym) + sample(x, yp))
                    + 0.08 * (sample(xm, ym) + sample(xp, ym) + sample(xm, yp) + sample(xp, yp));
                let wave = ocean.sample_axes(wave_x[x], wave_y[y]) - 0.5 + 2.0 / 255.0;
                let noise_value =
                    ocean.sample_axes(noise_x[y], noise_y[x]) - 0.5 + 2.0 / 255.0;
                let velocity = self.velocities[i] + (gain * wave + disturb * noise_value) * dt;
                self.velocities[i] = (velocity
                    - ((self.heights[i] - average) * 40.0 + self.heights[i] * 0.5) * dt)
                    * damping;
            }
        }
        // Shore forcing follows the spring/drag update; calming regions damp
        // velocity before height integration, in the reference's order.
        self.apply_regions(dt);
        for i in 0..self.heights.len() {
            self.heights[i] += self.velocities[i] * dt;
        }
        self.update_wet_map(dt);
        self.field_revision = self.field_revision.wrapping_add(1).max(1);
    }

    /// Reference PS_WaterUpdateWet: diffuse height/tension, retain receding wash.
    fn update_wet_map(&mut self, dt: f32) {
        // Ping-pong reusable storage: the stencil reads only the previous
        // generation, while the next one is written without per-step allocation.
        std::mem::swap(&mut self.wet_heights, &mut self.wet_scratch);
        std::mem::swap(&mut self.foam, &mut self.foam_scratch);
        let old_height = &self.wet_scratch;
        let old_foam = &self.foam_scratch;
        let n = GRID_SIZE;
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                let neighbours = [
                    y * n + x.saturating_sub(1),
                    y * n + (x + 1).min(n - 1),
                    y.saturating_sub(1) * n + x,
                    (y + 1).min(n - 1) * n + x,
                ];
                let avg_height = neighbours.iter().map(|j| old_height[*j]).sum::<f32>() * 0.25;
                let avg_foam = neighbours.iter().map(|j| old_foam[*j]).sum::<f32>() * 0.25;
                let history = old_height[i] + (avg_height - old_height[i]) * (2.5 * dt).min(1.0);
                let tension = (old_foam[i] + (avg_foam - old_foam[i]) * (5.0 * dt).min(1.0))
                    * (1.0 - 0.15 * dt).max(0.0);
                let new_height = self.heights[i] * 0.5;
                let new_tension =
                    (self.heights[i] - 0.1).clamp(0.0, 2.0) * self.tuning.foam_scale.max(0.0);
                let height_blend = if new_height >= history {
                    dt.min(1.0)
                } else {
                    0.01
                };
                let foam_blend = if new_tension >= tension {
                    (3.0 * dt).min(1.0)
                } else {
                    0.001
                };
                self.wet_heights[i] = history + (new_height - history) * height_blend;
                self.foam[i] = (tension + (new_tension - tension) * foam_blend).clamp(0.0, 2.0);
            }
        }
    }

    fn apply_regions(&mut self, dt: f32) {
        let clipped = |min: [f32; 2], max: [f32; 2], origin: [f32; 2]| {
            let first = [
                ((min[0] - origin[0]) / CELL_METRES)
                    .ceil()
                    .clamp(0.0, GRID_SIZE as f32) as usize,
                ((min[1] - origin[1]) / CELL_METRES)
                    .ceil()
                    .clamp(0.0, GRID_SIZE as f32) as usize,
            ];
            let last = [
                ((max[0] - origin[0]) / CELL_METRES)
                    .ceil()
                    .clamp(0.0, GRID_SIZE as f32) as usize,
                ((max[1] - origin[1]) / CELL_METRES)
                    .ceil()
                    .clamp(0.0, GRID_SIZE as f32) as usize,
            ];
            (first, last)
        };
        if let Some(shore) = self.shore.as_ref() {
            for quad in &self.wave_quads {
                let (first, last) = clipped(quad.min, quad.max, self.origin);
                let n = quad.direction.map(|v| -v);
                let tangent = [n[1], -n[0]];
                let gain = self.tuning.shore_wave_amplitude.max(0.0) * quad.amplitude * dt;
                for y in first[1]..last[1] {
                    for x in first[0]..last[0] {
                        let world = [
                            self.origin[0] + x as f32 * CELL_METRES,
                            self.origin[1] + y as f32 * CELL_METRES,
                        ];
                        let uv = [
                            (world[0] * n[0] + world[1] * n[1]) / 256.0
                                + self.time_seconds as f32 / 50.0,
                            (world[0] * tangent[0] + world[1] * tangent[1]) / 256.0,
                        ];
                        self.velocities[y * GRID_SIZE + x] +=
                            (1.723 * shore.sample(uv) - 0.723) * gain;
                    }
                }
            }
        }
        for quad in &self.calming_quads {
            let (first, last) = clipped(quad.min, quad.max, self.origin);
            let gain = quad.damping.powf(dt);
            for y in first[1]..last[1] {
                for x in first[0]..last[0] {
                    self.velocities[y * GRID_SIZE + x] *= gain;
                }
            }
        }
    }

    fn field(&self, values: &[f32], position: [f32; 2]) -> f32 {
        if values.is_empty() {
            return 0.0;
        }
        let p = [
            (position[0] - self.origin[0]) / CELL_METRES,
            (position[1] - self.origin[1]) / CELL_METRES,
        ];
        if p.iter()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > (GRID_SIZE - 1) as f32)
        {
            return 0.0;
        }
        let x = p[0].floor() as usize;
        let y = p[1].floor() as usize;
        let xp = (x + 1).min(GRID_SIZE - 1);
        let yp = (y + 1).min(GRID_SIZE - 1);
        let at = |col: usize, row: usize| {
            values[row * GRID_SIZE + col] * (EDGE_ENVELOPE[col] * EDGE_ENVELOPE[row])
        };
        let a = at(x, y) * (1.0 - p[0].fract()) + at(xp, y) * p[0].fract();
        let b = at(x, yp) * (1.0 - p[0].fract()) + at(xp, yp) * p[0].fract();
        a * (1.0 - p[1].fract()) + b * p[1].fract()
    }

    pub fn sample(&self, position: [f32; 2]) -> WaterSample {
        if !self.source_loaded() {
            return self.surface.sample(position, self.time_seconds as f32);
        }
        let dx = (self.field(&self.heights, [position[0] + 1.0, position[1]])
            - self.field(&self.heights, [position[0] - 1.0, position[1]]))
            * 0.5;
        let dz = (self.field(&self.heights, [position[0], position[1] + 1.0])
            - self.field(&self.heights, [position[0], position[1] - 1.0]))
            * 0.5;
        let length = (1.0 + dx * dx + dz * dz).sqrt();
        WaterSample {
            height: self.surface.level + self.field(&self.heights, position),
            normal: [-dx / length, 1.0 / length, -dz / length],
            vertical_velocity: self.field(&self.velocities, position) * 0.75,
        }
    }

    /// One compact shared field is uploaded to the vertex shader; the static
    /// ocean mesh is not rebuilt and uploaded for every animated wave.
    pub fn gpu_field(&self) -> Vec<f32> {
        let mut output = Vec::new();
        self.fill_gpu_field(&mut output);
        output
    }

    /// Reuse the renderer's packing allocation instead of allocating 1 MB each frame.
    pub fn fill_gpu_field(&self, output: &mut Vec<f32>) {
        output.clear();
        output.reserve(self.heights.len() * 4);
        for i in 0..self.heights.len() {
            let envelope = EDGE_ENVELOPE[i % GRID_SIZE] * EDGE_ENVELOPE[i / GRID_SIZE];
            output.extend_from_slice(&[
                self.heights[i] * envelope,
                self.velocities[i] * 0.75 * envelope,
                self.foam[i] * envelope,
                self.wet_heights[i] * 2.0 * envelope,
            ]);
        }
    }

    pub fn update_camera(&mut self, position: [f32; 3]) {
        let depth = self.sample([position[0], position[2]]).height - position[1];
        self.immersion
            .update(depth, self.enabled, self.optics.transition_metres);
    }
    pub fn closest_shore(&self, position: [f32; 2]) -> Option<([f32; 2], [f32; 2], WaterSample)> {
        self.wave_quads
            .iter()
            .map(|q| {
                let point = [
                    position[0].clamp(q.min[0], q.max[0]),
                    position[1].clamp(q.min[1], q.max[1]),
                ];
                let distance = (point[0] - position[0]).hypot(point[1] - position[1]);
                (distance, point, q.direction)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, point, direction)| (point, direction, self.sample(point)))
    }
    pub fn configure_optics(&mut self, config: &serde_json::Value) -> Result<(), String> {
        if let Some(optics) = config.get("optics") {
            let mut value = serde_json::to_value(self.optics).map_err(|e| e.to_string())?;
            let object = optics.as_object().ok_or("water optics must be an object")?;
            for (key, setting) in object {
                value[key] = setting.clone();
            }
            let settings: WaterOpticsSettings =
                serde_json::from_value(value).map_err(|e| e.to_string())?;
            settings.validate()?;
            self.optics = settings;
        }
        Ok(())
    }

    pub fn foam_at(&self, position: [f32; 2]) -> f32 {
        self.field(&self.foam, position)
    }

    pub fn impact(&mut self, position: [f32; 2], strength: f32) {
        if !self.enabled || self.heights.is_empty() || !strength.is_finite()
            || !position.iter().all(|value| value.is_finite()) {
            return;
        }
        self.field_revision = self.field_revision.wrapping_add(1).max(1);
        let radius = (strength * 0.30 + 1.0).clamp(1.0, 3.5);
        let cell = [
            (position[0] - self.origin[0]) / CELL_METRES,
            (position[1] - self.origin[1]) / CELL_METRES,
        ];
        let extent = (radius / CELL_METRES).ceil() as i32 + 1;
        for y in cell[1] as i32 - extent..=cell[1] as i32 + extent {
            for x in cell[0] as i32 - extent..=cell[0] as i32 + extent {
                if !(0..GRID_SIZE as i32).contains(&x) || !(0..GRID_SIZE as i32).contains(&y) {
                    continue;
                }
                let distance = ((cell[0] - x as f32).powi(2) + (cell[1] - y as f32).powi(2)).sqrt()
                    * CELL_METRES;
                let envelope = (-distance * distance / (radius * radius)).exp();
                let i = y as usize * GRID_SIZE + x as usize;
                self.velocities[i] -= strength.clamp(0.0, 8.0) * envelope * 0.24;
                self.foam[i] = (self.foam[i] + envelope * strength * 0.15).min(1.0);
            }
        }
    }
}

/// Shift retained world cells with one overlapping memmove. Clearing exposed
/// rows/columns prevents linear-buffer row wrapping, including teleports.
fn shift_water_field(field: &mut [f32], delta: [i32; 2]) {
    let n = GRID_SIZE as i32;
    if delta.iter().any(|value| value.unsigned_abs() >= GRID_SIZE as u32) {
        field.fill(0.0);
        return;
    }
    let shift = delta[1] * n + delta[0];
    if shift > 0 {
        field.copy_within(shift as usize.., 0);
    } else if shift < 0 {
        field.copy_within(..(field.len() as i32 + shift) as usize, (-shift) as usize);
    }
    let first_row = (-delta[1]).max(0) as usize;
    let last_row = (n - delta[1]).min(n) as usize;
    field[..first_row * GRID_SIZE].fill(0.0);
    field[last_row * GRID_SIZE..].fill(0.0);
    let first_column = (-delta[0]).max(0) as usize;
    let last_column = (n - delta[0]).min(n) as usize;
    for row in field[first_row * GRID_SIZE..last_row * GRID_SIZE].chunks_mut(GRID_SIZE) {
        row[..first_column].fill(0.0);
        row[last_column..].fill(0.0);
    }
}

/// Renderer-independent settings loaded from the common Shared profile.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WaterOpticsSettings {
    pub opacity: f32,
    pub refraction_index: f32,
    pub transition_metres: f32,
    pub caustics_strength: f32,
    pub caustics_depth: f32,
    pub caustics_time_scale: f32,
    pub fog_light_intensity: f32,
    pub fog_pierce_intensity: f32,
}
impl Default for WaterOpticsSettings {
    fn default() -> Self {
        Self {
            opacity: (40.0_f32 / 255.0).powi(2),
            refraction_index: 1.22,
            transition_metres: 0.12,
            caustics_strength: 10.0,
            caustics_depth: 7.0,
            caustics_time_scale: 0.025, // EndFogPrepass receives elapsed seconds / 40.
            fog_light_intensity: 1.0,
            fog_pierce_intensity: 1.0,
        }
    }
}
impl WaterOpticsSettings {
    fn validate(self) -> Result<(), String> {
        let values = [
            self.opacity,
            self.refraction_index,
            self.transition_metres,
            self.caustics_strength,
            self.caustics_depth,
            self.caustics_time_scale,
            self.fog_light_intensity,
            self.fog_pierce_intensity,
        ];
        if !values.iter().all(|x| x.is_finite())
            || !(0.0..=1.0).contains(&self.opacity)
            || !(1.0..=2.0).contains(&self.refraction_index)
            || !(0.01..=2.0).contains(&self.transition_metres)
            || !(0.0..=100.0).contains(&self.caustics_strength)
            || !(0.1..=100.0).contains(&self.caustics_depth)
            || !(0.0..=1.0).contains(&self.caustics_time_scale)
            || !(0.0..=10.0).contains(&self.fog_light_intensity)
            || !(0.0..=10.0).contains(&self.fog_pierce_intensity)
        {
            return Err("invalid water optics settings".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct WaterImmersion {
    pub submerged: bool,
    pub depth: f32,
    pub blend: f32,
}
impl WaterImmersion {
    fn update(&mut self, depth: f32, enabled: bool, transition: f32) {
        if !enabled || !depth.is_finite() {
            *self = Self::default();
            return;
        }
        self.depth = depth;
        if self.submerged {
            self.submerged = depth > -0.06;
        } else {
            self.submerged = depth > 0.06;
        }
        let x = (depth / transition.max(0.01)).clamp(0.0, 1.0);
        self.blend = x * x * (3.0 - 2.0 * x);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterSurface {
    pub level: f32,
    pub amplitude: f32,
    pub wavelength: f32,
    pub speed: f32,
    pub wind_direction: [f32; 2],
}

impl Default for WaterSurface {
    fn default() -> Self {
        Self {
            level: 0.0,
            amplitude: 0.30,
            wavelength: 18.0,
            speed: 2.0,
            wind_direction: [0.72, 0.69],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterSample {
    pub height: f32,
    pub normal: [f32; 3],
    pub vertical_velocity: f32,
}

impl WaterSurface {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.level.is_finite()
            || !self.amplitude.is_finite()
            || !(0.0..=20.0).contains(&self.amplitude)
            || !self.wavelength.is_finite()
            || self.wavelength < 0.25
            || !self.speed.is_finite()
            || !(0.0..=100.0).contains(&self.speed)
            || !self.wind_direction.iter().all(|v| v.is_finite())
            || self.wind_direction[0].hypot(self.wind_direction[1]) < 1e-5
        {
            return Err("invalid water surface parameters");
        }
        Ok(())
    }

    pub fn sample(&self, position: [f32; 2], seconds: f32) -> WaterSample {
        let time = if seconds.is_finite() { seconds } else { 0.0 };
        let d = self.wind_direction;
        let inv = 1.0 / d[0].hypot(d[1]).max(1e-6);
        let wind = [d[0] * inv, d[1] * inv];
        let waves = [
            (wind, self.amplitude * 0.64, self.wavelength, self.speed),
            (
                [-wind[1], wind[0]],
                self.amplitude * 0.26,
                self.wavelength * 0.48,
                self.speed * 1.35,
            ),
            (
                [wind[0] * 0.7 - wind[1] * 0.7, wind[0] * 0.7 + wind[1] * 0.7],
                self.amplitude * 0.10,
                self.wavelength * 0.22,
                self.speed * 1.8,
            ),
        ];
        let mut height = self.level;
        let mut gradient = [0.0_f32; 2];
        let mut velocity = 0.0_f32;
        for (dir, amp, wavelength, speed) in waves {
            let k = TAU / wavelength;
            let phase = (position[0] * dir[0] + position[1] * dir[1]) * k - time * speed * k;
            let (sine, cosine) = phase.sin_cos();
            height += amp * sine;
            gradient[0] += amp * k * dir[0] * cosine;
            gradient[1] += amp * k * dir[1] * cosine;
            velocity -= amp * speed * k * cosine;
        }
        let normal = [-gradient[0], 1.0, -gradient[1]];
        let inv_normal =
            1.0 / (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        WaterSample {
            height,
            normal: [
                normal[0] * inv_normal,
                normal[1] * inv_normal,
                normal[2] * inv_normal,
            ],
            vertical_velocity: velocity,
        }
    }

    pub fn submersion(&self, bottom: f32, top: f32, position: [f32; 2], seconds: f32) -> f32 {
        let extent = top - bottom;
        if extent <= 1e-5 {
            return 0.0;
        }
        ((self.sample(position, seconds).height - bottom) / extent).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dynamic() -> WaterSimulation {
        let mut dds = vec![0u8; 128 + 128 * 128];
        dds[..4].copy_from_slice(b"DDS ");
        for (offset, value) in [(12, 128u32), (16, 128), (88, 8), (92, 255)] {
            dds[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        for (i, value) in dds[128..].iter_mut().enumerate() {
            *value = (i % 256) as u8;
        }
        let mut sim = WaterSimulation::default();
        sim.load_sources(&dds, &dds).unwrap();
        sim
    }

    #[test]
    fn cached_wave_axes_preserve_reference_sampling_including_negative_uvs() {
        let sim = dynamic();
        let texture = sim.ocean.as_ref().unwrap();
        let reference = |uv: [f32; 2]| {
            let maximum = (texture.size - 1) as f32;
            let fetch = |u: f32, v: f32| {
                let x = (u.rem_euclid(1.0) * maximum) as usize;
                let y = (v.rem_euclid(1.0) * maximum) as usize;
                texture.values[y * texture.size + x]
            };
            let dx = (uv[0] * maximum).fract();
            let dy = (uv[1] * maximum).fract();
            let texel = 1.0 / maximum;
            let a = fetch(uv[0], uv[1]) * (1.0 - dx) + fetch(uv[0] + texel, uv[1]) * dx;
            let b = fetch(uv[0], uv[1] + texel) * (1.0 - dx)
                + fetch(uv[0] + texel, uv[1] + texel) * dx;
            a * (1.0 - dy) + b * dy
        };
        for i in -1000..1000 {
            let uv = [i as f32 * 0.00371, i as f32 * -0.00913];
            assert_eq!(texture.sample(uv).to_bits(), reference(uv).to_bits());
        }
    }

    #[test]
    fn fixed_water_steps_are_independent_of_frame_partition() {
        let mut a = dynamic();
        let mut b = dynamic();
        for _ in 0..60 {
            a.advance(1.0 / 60.0);
        }
        for _ in 0..30 {
            b.advance(1.0 / 30.0);
        }
        assert_eq!(a.heights, b.heights);
        assert_eq!(a.velocities, b.velocities);
        assert!((a.seconds() - 1.0).abs() < 1e-7);
        assert!(a.heights.iter().all(|v| v.is_finite() && v.abs() < 2.0));
    }

    #[test]
    fn field_revision_changes_only_when_gpu_sample_data_changes() {
        let mut sim = dynamic();
        let initial = sim.field_revision();
        sim.advance(0.0);
        sim.advance(f32::NAN);
        sim.advance(1.0 / 240.0);
        assert_eq!(sim.field_revision(), initial);
        sim.advance(1.0 / 60.0);
        assert!(sim.field_revision() > initial);
        let stepped = sim.field_revision();
        sim.impact([0.0, 0.0], 1.0);
        assert!(sim.field_revision() > stepped);
        let impacted = sim.field_revision();
        sim.track_focus([0.0, 0.0]);
        assert_eq!(sim.field_revision(), impacted);
        sim.track_focus([2.0, -2.0]);
        assert!(sim.field_revision() > impacted);
        let mut reusable = Vec::new();
        sim.fill_gpu_field(&mut reusable);
        let pointer = reusable.as_ptr();
        assert_eq!(reusable, sim.gpu_field());
        sim.fill_gpu_field(&mut reusable);
        assert_eq!(pointer, reusable.as_ptr());
    }

    #[test]
    fn in_place_grid_shift_preserves_retained_cells_and_clears_every_exposed_edge() {
        let original: Vec<f32> = (0..GRID_SIZE * GRID_SIZE).map(|i| i as f32 + 1.0).collect();
        for delta in [[1, -2], [-3, 1], [0, 4], [2, 0], [-255, 255], [256, -3], [-500, 0]] {
            let mut shifted = original.clone();
            shift_water_field(&mut shifted, delta);
            for y in 0..GRID_SIZE {
                for x in 0..GRID_SIZE {
                    let source = [x as i32 + delta[0], y as i32 + delta[1]];
                    let expected = if source.iter().all(|value| (0..GRID_SIZE as i32).contains(value)) {
                        original[source[1] as usize * GRID_SIZE + source[0] as usize]
                    } else {0.0};
                    assert_eq!(shifted[y * GRID_SIZE + x], expected, "delta={delta:?} x={x} y={y}");
                }
            }
        }
    }

    #[test]
    fn local_field_meets_level_ocean_at_each_edge_without_reducing_interior_waves() {
        let mut sim = dynamic();
        sim.heights.fill(0.6);
        sim.velocities.fill(0.4);
        sim.foam.fill(0.8);
        sim.wet_heights.fill(0.3);
        let origin = sim.origin();
        let world = |x: f32, z: f32| [origin[0] + x * CELL_METRES, origin[1] + z * CELL_METRES];
        for (point, expected) in [
            ([0.0, 128.0], 0.0), ([255.0, 128.0], 0.0),
            ([128.0, 0.0], 0.0), ([128.0, 255.0], 0.0),
            ([8.0, 128.0], 0.3), ([247.0, 128.0], 0.3),
            ([8.0, 8.0], 0.15), ([16.0, 128.0], 0.6), ([128.0, 128.0], 0.6),
        ] {
            assert!((sim.sample(world(point[0], point[1])).height - sim.surface.level - expected).abs() < 1e-6);
        }
        assert_eq!(sim.sample(world(-0.1, 128.0)).height, sim.surface.level);
        assert!(sim.sample(world(0.1, 128.0)).height.abs() < 0.001);
        assert_eq!(sim.heights[128 * GRID_SIZE + 128], 0.6);
        let packed = sim.gpu_field();
        for i in [0, GRID_SIZE - 1, (GRID_SIZE - 1) * GRID_SIZE, GRID_SIZE * GRID_SIZE - 1] {
            assert_eq!(&packed[i * 4..i * 4 + 4], &[0.0; 4]);
        }
        assert_eq!(&packed[(128 * GRID_SIZE + 128) * 4..(128 * GRID_SIZE + 128) * 4 + 4],
                   &[0.6, 0.4 * 0.75, 0.8, 0.3 * 2.0]);
    }

    #[test]
    fn gpu_node_interpolation_matches_buoyancy_queries_through_the_transition() {
        let mut sim = dynamic();
        for i in 0..GRID_SIZE * GRID_SIZE {
            sim.heights[i] = ((i % 71) as f32 - 35.0) * 0.01;
            sim.velocities[i] = ((i % 29) as f32 - 14.0) * 0.02;
        }
        let packed = sim.gpu_field();
        let gpu_at = |x: f32, z: f32, channel: usize| {
            if x < 0.0 || z < 0.0 || x > 255.0 || z > 255.0 { return 0.0; }
            let i = x.floor() as usize;
            let j = z.floor() as usize;
            let next_x = (i + 1).min(255);
            let next_z = (j + 1).min(255);
            let u = x.fract();
            let v = z.fract();
            let at = |a: usize, b: usize| packed[(b * GRID_SIZE + a) * 4 + channel];
            (at(i,j)*(1.0-u)+at(next_x,j)*u)*(1.0-v)
                +(at(i,next_z)*(1.0-u)+at(next_x,next_z)*u)*v
        };
        for x in [-0.25, 0.0, 0.2, 7.3, 15.9, 16.1, 128.5, 247.4, 254.8, 255.0, 255.25] {
            for z in [0.2, 8.7, 128.5, 247.1, 255.0] {
                let origin=sim.origin();
                let query=sim.sample([origin[0]+x*CELL_METRES,origin[1]+z*CELL_METRES]);
                assert!((query.height-sim.surface.level-gpu_at(x,z,0)).abs()<2e-6);
                assert!((query.vertical_velocity-gpu_at(x,z,1)).abs()<2e-6);
            }
        }
    }

    #[test]
    fn moving_grid_retains_world_wave_height_and_velocity() {
        let mut sim = dynamic();
        for _ in 0..10 {
            sim.advance(1.0 / 60.0);
        }
        let before = sim.sample([10.0, 12.0]);
        sim.track_focus([4.0, -6.0]);
        assert_eq!(before, sim.sample([10.0, 12.0]));
    }

    #[test]
    fn impact_changes_velocity_without_teleporting_the_surface() {
        let mut sim = dynamic();
        let height = sim.sample([0.0, 0.0]).height;
        sim.impact([0.0, 0.0], 3.0);
        assert_eq!(sim.sample([0.0, 0.0]).height, height);
        assert!(sim.sample([0.0, 0.0]).vertical_velocity < 0.0);
        sim.advance(1.0 / 60.0);
        assert!(sim.sample([0.0, 0.0]).height < height);
    }

    #[test]
    fn invalid_delta_and_large_hitch_do_not_destabilize_water() {
        let mut sim = dynamic();
        sim.advance(f32::NAN);
        sim.advance(-1.0);
        assert_eq!(sim.seconds(), 0.0);
        sim.advance(50.0);
        assert!(sim.seconds() <= 8.0 / 60.0 + 1e-7);
        assert!(sim.heights.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn shore_waves_use_authored_bounds_and_calming_precedes_height_integration() {
        let mut sim = dynamic();
        sim.tuning.ocean_amplitude = 0.0;
        sim.tuning.noise_amplitude = 0.0;
        sim.configure_regions(&serde_json::json!({
            "wave_quads":[{"min":[-8.0,-8.0],"max":[8.0,8.0],"direction":[1.0,0.0],"amplitude":2.0}],
            "calming_quads":[{"min":[-4.0,-4.0],"max":[0.0,4.0],"damping":0.0}]
        })).unwrap();
        sim.advance(1.0 / 60.0);
        assert_eq!(sim.sample([20.0, 0.0]).height, 0.0);
        assert_eq!(sim.sample([-2.0, 0.0]).height, 0.0);
        assert!(sim.sample([2.0, 0.0]).height.abs() > 1e-6);
        assert!(sim
            .configure_regions(
                &serde_json::json!({"wave_quads":[{"min":[0,0],"max":[1,1],"direction":[0,0]}]})
            )
            .is_err());
    }

    #[test]
    fn water_queries_are_deterministic_and_normalized() {
        let ocean = WaterSurface::default();
        ocean.validate().unwrap();
        let a = ocean.sample([8.0, -4.0], 1.25);
        assert_eq!(a, ocean.sample([8.0, -4.0], 1.25));
        assert!((a.height - ocean.level).abs() <= ocean.amplitude + 1e-5);
        assert!((a.normal.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn submerged_volume_fraction_clamps() {
        let ocean = WaterSurface {
            amplitude: 0.0,
            ..Default::default()
        };
        assert_eq!(ocean.submersion(1.0, 2.0, [0.0; 2], 0.0), 0.0);
        assert_eq!(ocean.submersion(-2.0, -1.0, [0.0; 2], 0.0), 1.0);
        assert!((ocean.submersion(-0.5, 0.5, [0.0; 2], 0.0) - 0.5).abs() < 1e-5);
    }
    #[test]
    fn zero_wavelength_rejected() {
        let mut ocean = WaterSurface::default();
        ocean.wavelength = 0.0;
        assert!(ocean.validate().is_err());
    }
    #[test]
    fn crest_foam_and_wash_survive_recession_and_grid_motion() {
        let mut sim = dynamic();
        sim.heights.fill(0.6);
        for _ in 0..80 {
            sim.update_wet_map(0.0125);
        }
        let foam = sim.foam_at([0.0, 0.0]);
        let wet = sim.field(&sim.wet_heights, [0.0, 0.0]);
        assert!(foam > 0.4 && wet > 0.1);
        sim.heights.fill(-0.1);
        sim.update_wet_map(0.0125);
        assert!(sim.foam_at([0.0, 0.0]) > foam * 0.95);
        assert!(sim.field(&sim.wet_heights, [0.0, 0.0]) > wet * 0.9);
        let before = sim.foam_at([0.0, 0.0]);
        sim.track_focus([8.0, -6.0]);
        assert_eq!(before, sim.foam_at([0.0, 0.0]));
        for _ in 0..500 {
            sim.update_wet_map(0.0125);
        }
        assert!(sim.foam_at([0.0, 0.0]) < foam * 0.5);
    }
    #[test]
    fn camera_water_contact_has_hysteresis_and_continuous_optical_blend() {
        let mut immersion = WaterImmersion::default();
        immersion.update(0.07, true, 0.12);
        assert!(immersion.submerged);
        assert!(immersion.blend > 0.0 && immersion.blend < 1.0);
        for depth in [-0.02, 0.02, -0.04, 0.01] {
            immersion.update(depth, true, 0.12);
            assert!(immersion.submerged);
        }
        immersion.update(-0.07, true, 0.12);
        assert!(!immersion.submerged);
        immersion.update(2.0, true, 0.12);
        assert_eq!(immersion.blend, 1.0);
        immersion.update(2.0, false, 0.12);
        assert_eq!(immersion.blend, 0.0);
        assert!(!immersion.submerged);
    }
    #[test]
    fn optics_overrides_validate_without_replacing_other_shared_settings() {
        let mut sim = dynamic();
        sim.configure_optics(&serde_json::json!({"optics":{"opacity":0.01}}))
            .unwrap();
        assert_eq!(sim.optics.opacity, 0.01);
        assert_eq!(sim.optics.refraction_index, 1.22);
        assert!(sim
            .configure_optics(&serde_json::json!({"optics":{"transition_metres":0}}))
            .is_err());
        assert_eq!(sim.optics.opacity, 0.01);
        assert!(sim
            .configure_optics(&serde_json::json!({"optics":{"misspelled_setting":1}}))
            .is_err());
    }
}
