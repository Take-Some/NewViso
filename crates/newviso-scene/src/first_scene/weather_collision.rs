//! Bounded, world-anchored precipitation height map. CPU queries resident BVHs;
//! the GPU samples the completed map, never a partially updated generation.
use super::*;
pub(super) const WEATHER_HEIGHT_SIDE: usize = 33;
pub(super) const WEATHER_HEIGHT_FLOATS: usize = 1092; // 1089 samples, vec4 padding
const NO_SURFACE: f32 = -1_000_000.0;

#[derive(Debug)]
pub(super) struct WeatherCollisionMap {
    pub origin: [f32; 2],
    pub spacing: f32,
    pub heights: [f32; WEATHER_HEIGHT_FLOATS],
    pub ready: bool,
    pending_origin: [f32; 2],
    pending: [f32; WEATHER_HEIGHT_FLOATS],
    cursor: usize,
    pub camera_velocity: Vec3,
    previous_eye: Option<Vec3>,
}
impl Default for WeatherCollisionMap {
    fn default() -> Self {
        Self { origin: [0.0; 2], spacing: 4.0,
            heights: [NO_SURFACE; WEATHER_HEIGHT_FLOATS], ready: false,
            pending_origin: [0.0; 2], pending: [NO_SURFACE; WEATHER_HEIGHT_FLOATS],
            cursor: 0, camera_velocity: Vec3::ZERO, previous_eye: None }
    }
}
impl WeatherCollisionMap {
    pub fn track_camera(&mut self, eye: Vec3, dt: f32) {
        self.camera_velocity = self.previous_eye
            .filter(|old| eye.sub(*old).length() < 25.0)
            .map(|old| eye.sub(old).mul(1.0 / dt.max(0.001)))
            .unwrap_or(Vec3::ZERO);
        self.previous_eye = Some(eye);
    }
    fn begin(&mut self, eye: Vec3, spacing: f32) {
        if self.cursor == 0 {
            self.spacing = spacing;
            let radius = (WEATHER_HEIGHT_SIDE / 2) as f32 * spacing;
            self.pending_origin = [(eye.x / spacing).floor() * spacing - radius,
                                   (eye.z / spacing).floor() * spacing - radius];
        }
        let extent = (WEATHER_HEIGHT_SIDE - 1) as f32 * self.spacing;
        if self.ready && (eye.x < self.origin[0] || eye.z < self.origin[1]
            || eye.x > self.origin[0] + extent || eye.z > self.origin[1] + extent) {
            self.ready = false;
            self.cursor = 0;
            self.begin(eye, spacing);
        }
    }
    fn publish(&mut self) {
        if self.cursor == WEATHER_HEIGHT_SIDE * WEATHER_HEIGHT_SIDE {
            self.heights = self.pending;
            self.origin = self.pending_origin;
            self.ready = true;
            self.cursor = 0;
        }
    }
}
impl Scene3dRuntime {
    /// The runtime supplies exact resident triangle queries. Scene-owned
    /// heightfields, primitives and enabled water provide their own surfaces.
    pub fn refresh_weather_collision_map(
        &mut self, mut physical_height: impl FnMut(f32, f32, f32) -> Option<f32>,
    ) {
        if self.weather_gpu_fx.is_none() ||
            self.active_weather_world_layers().is_empty() {
            self.weather_collision.ready = false;
            self.weather_collision.cursor = 0;
            return;
        }
        let (spacing, budget) = self.weather_gpu_fx.as_ref()
            .map(|r| (r.collision_cell_metres, r.collision_probes_per_frame))
            .unwrap_or((4.0, 99));
        self.weather_collision.begin(self.camera.position, spacing);
        let end = (self.weather_collision.cursor + budget)
            .min(WEATHER_HEIGHT_SIDE * WEATHER_HEIGHT_SIDE);
        let top = self.camera.position.y + 256.0;
        for index in self.weather_collision.cursor..end {
            let x = self.weather_collision.pending_origin[0]
                + (index % WEATHER_HEIGHT_SIDE) as f32 * spacing;
            let z = self.weather_collision.pending_origin[1]
                + (index / WEATHER_HEIGHT_SIDE) as f32 * spacing;
            let mut height = physical_height(x, z, top);
            for entity in self.world.solid_entities() {
                // Imported model bounds are broad phase only; use the runtime
                // triangle query above. Only scene primitives own render slots.
                let Some(slot) = entity.render_slot else { continue; };
                let surface = if let Some(terrain) = self.terrain_meshes.get(&slot) {
                    terrain.surface_height(x, z)
                } else {
                    let b = self.world.collision_bounds(entity.id).unwrap_or(entity.bounds);
                    (x >= b.min.x && x <= b.max.x && z >= b.min.z && z <= b.max.z)
                        .then_some(b.max.y)
                };
                if let Some(y) = surface.filter(|y| *y <= top) {
                    height = Some(height.map_or(y, |old| old.max(y)));
                }
            }
            if self.water_simulation.enabled {
                let y = self.water_simulation.sample([x, z]).height;
                height = Some(height.map_or(y, |old| old.max(y)));
            }
            self.weather_collision.pending[index] = height.unwrap_or(NO_SURFACE);
        }
        self.weather_collision.cursor = end;
        self.weather_collision.publish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collision_map_publishes_only_complete_generations_and_invalidates_teleports() {
        let mut m = WeatherCollisionMap::default();
        m.begin(Vec3::ZERO, 4.0);
        m.pending[0] = 9.0;
        m.cursor = 99;
        m.publish();
        assert!(!m.ready);
        m.cursor = WEATHER_HEIGHT_SIDE * WEATHER_HEIGHT_SIDE;
        m.publish();
        assert!(m.ready);
        assert_eq!(m.heights[0], 9.0);
        m.begin(Vec3::new(500.0, 0.0, 0.0), 4.0);
        assert!(!m.ready);
        assert_eq!(m.cursor, 0);
    }
    #[test]
    fn camera_cuts_do_not_generate_extreme_rain_streaks() {
        let mut m = WeatherCollisionMap::default();
        m.track_camera(Vec3::ZERO, 0.016);
        m.track_camera(Vec3::new(1.0,0.0,0.0), 0.1);
        assert!((m.camera_velocity.x - 10.0).abs() < 0.001);
        m.track_camera(Vec3::new(500.0,0.0,0.0), 0.016);
        assert_eq!(m.camera_velocity.x, 0.0);
    }
}
