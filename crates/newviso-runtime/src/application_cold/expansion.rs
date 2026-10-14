//! Append project-authored strips to the persistent snow grid and materialize terrain.
use super::*;

impl EngineApplication {
    pub(super) fn extend_snow_terrain(&mut self, command: &Value) -> Result<(), String> {
        let strip: SnowFieldSpec = if command.get("generator").is_some() {
            let profile: newviso_environment::snow::SnowTerrainProfile = decode(command, "generator")?;
            #[derive(serde::Deserialize)]
            struct Header {id: String, seed: u64, origin: [f32; 2], cell_size: f32, columns: usize, rows: usize}
            let h: Header = decode(command, "field")?;
            profile.generate(h.id, h.seed, h.origin, h.cell_size, h.columns, h.rows)?
        } else {decode(command, "field")?};
        strip.validate()?;
        let terrain = command.get("terrain").ok_or("terrain expansion material policy missing")?;
        let color = |key: &str| -> Result<[f32; 4], String> {
            let c: [f32; 4] = serde_json::from_value(terrain[key].clone()).map_err(|e| e.to_string())?;
            if c.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) { return Err("invalid terrain color".into()); }
            Ok(c)
        };
        let snow_color = color("snow_color")?;
        let soil_color = color("soil_color")?;
        let number = |key: &str, low: f32, high: f32| -> Result<f32, String> {
            let n = terrain[key].as_f64().ok_or("terrain material number missing")? as f32;
            if !n.is_finite() || !(low..=high).contains(&n) { return Err("invalid terrain material number".into()); }
            Ok(n)
        };
        let soil_offset = number("soil_offset", 0.001, 0.025)?;
        let relief = number("render_relief", 0.0, 0.03)?;
        let snow_id = terrain["snow_surface_id"].as_u64().and_then(|n| u32::try_from(n).ok()).ok_or("invalid snow surface id")?;
        let soil_id = terrain["soil_surface_id"].as_u64().and_then(|n| u32::try_from(n).ok()).ok_or("invalid soil surface id")?;
        let cells = command.get("patch_cells").and_then(Value::as_u64).unwrap_or(32) as usize;
        if !(2..=64).contains(&cells) || (strip.columns - 1) % cells != 0 || (strip.rows - 1) % cells != 0 {
            return Err("terrain expansion strip must contain whole 2..64-cell patches".into());
        }
        let field = self.cold.field.as_mut().ok_or("snow field not loaded")?;
        let changed = field.extend(strip.clone())?;
        // The base asset digest remains unchanged: expansion is persistent state,
        // not a replacement of the original map/seed.
        if changed {
            self.cold.wind_shelter.clear(); self.cold.shelter_direction = None;
            self.cold.precipitation_plan = None;
            self.cold.micro_dirty = true;
        }
        for row in (0..strip.rows - 1).step_by(cells) {
            for col in (0..strip.columns - 1).step_by(cells) {
                let mut ground = Vec::with_capacity((cells + 1).pow(2));
                let mut surface = Vec::with_capacity((cells + 1).pow(2));
                for z in row..=row + cells { for x in col..=col + cells {
                    let wx = strip.origin[0] + x as f32 * strip.cell_size;
                    let wz = strip.origin[1] + z as f32 * strip.cell_size;
                    let ix = ((wx - field.spec.origin[0]) / field.spec.cell_size).round() as usize;
                    let iz = ((wz - field.spec.origin[1]) / field.spec.cell_size).round() as usize;
                    ground.push(field.spec.ground[iz * field.spec.columns + ix]);
                    surface.push(field.sample(wx, wz).ok_or("expanded snow sample missing")?.surface_height);
                }}
                let center = [strip.origin[0] + (col as f32 + cells as f32 * 0.5) * strip.cell_size,
                    0.0, strip.origin[1] + (row as f32 + cells as f32 * 0.5) * strip.cell_size];
                let size = [cells as f32 * strip.cell_size, 1.0, cells as f32 * strip.cell_size];
                for snow in [true, false] {
                    let key = format!("terrain.{}.{}.{}.{}", strip.id,
                        strip.origin[0] + col as f32 * strip.cell_size,
                        strip.origin[1] + row as f32 * strip.cell_size,
                        if snow {"snow"} else {"soil"});
                    let color = if snow {snow_color} else {soil_color};
                    let heights = if snow {surface.clone()} else {ground.iter().map(|h| h + soil_offset).collect()};
                    let record = json!({"collider":{"solid":snow},
                        "material":{"surface_id":if snow {snow_id} else {soil_id}},
                        "mesh":{"heightfield":{"columns":cells+1,"rows":cells+1,
                            "heights":heights,"ground_heights":ground,
                            "surface_kind":if snow {"snow"} else {"ground_wet"},
                            "render_relief_metres":if snow {relief} else {0.0},
                            "render_subdivisions":1,"deformable":snow}}});
                    if let Some((entity, vertices, triangles, surface_id)) =
                        self.scene.append_runtime_heightfield(&key, &record, center, size, color)? {
                        if let Some(physics) = &mut self.physics {
                            physics.update_streamed_heightfield(entity, vertices, triangles, surface_id)?;
                        }
                    }
                }
            }
        }
        self.cold.sync_elapsed = self.cold.sync_interval;
        Ok(())
    }
}
