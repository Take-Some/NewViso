//! Append aligned terrain without replacing the authoritative state of existing cells.
use super::*;

/// Project-authored seeded height profile; evaluated natively outside script budgets.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct SnowTerrainProfile {
    pub size_metres: f64, pub base_height: f64, pub octaves: Vec<[f64; 2]>, pub rim_height: f64,
    pub camp_flat_radius: f64, pub camp_blend_radius: f64, pub camp_snow_depth: f64,
    pub minimum_snow_depth: f64, pub maximum_snow_depth: f64, pub drift_scale: f64,
}
impl SnowTerrainProfile {
    pub fn generate(&self, id: String, seed: u64, origin: [f32; 2], cell_size: f32,
        columns: usize, rows: usize) -> Result<SnowFieldSpec, String> {
        if !(3..=1025).contains(&columns) || !(3..=1025).contains(&rows) || seed > u32::MAX as u64 ||
            ![self.size_metres,self.base_height,self.rim_height,self.camp_flat_radius,self.camp_blend_radius,
                self.camp_snow_depth,self.minimum_snow_depth,self.maximum_snow_depth,self.drift_scale]
                .iter().all(|n| n.is_finite()) || self.size_metres < 1.0 || self.camp_blend_radius <= 0.0 ||
            self.drift_scale <= 0.0 || self.rim_height < 0.0 || self.minimum_snow_depth < 0.0 ||
            self.maximum_snow_depth > 20.0 || self.maximum_snow_depth < self.minimum_snow_depth ||
            self.octaves.is_empty() || self.octaves.len() > 8 ||
            self.octaves.iter().any(|a| !a.iter().all(|v|v.is_finite()) || a[0] <= 0.0) {
            return Err("invalid procedural snow height profile".into());
        }
        let mut ground = Vec::with_capacity(columns * rows);
        let mut depth = Vec::with_capacity(columns * rows);
        let round = |n: f64| ((n * 100000.0).round() / 100000.0) as f32;
        for z in 0..rows { for x in 0..columns {
            let x = origin[0] as f64 + x as f64 * cell_size as f64;
            let z = origin[1] as f64 + z as f64 * cell_size as f64;
            let camp = ((x.hypot(z) - self.camp_flat_radius) / self.camp_blend_radius).clamp(0.0, 1.0);
            let camp = camp * camp * (3.0 - 2.0 * camp);
            let hills = self.octaves.iter().enumerate().map(|(i,a)|
                terrain_noise(x/a[0], z/a[0], seed + i as u64 * 101) * a[1]).sum::<f64>();
            let rim = self.rim_height * ((x.abs().max(z.abs()) - self.size_metres * 0.36) /
                (self.size_metres * 0.14)).clamp(0.0, 1.0).powi(2);
            let drift = (terrain_noise(x/self.drift_scale, z/self.drift_scale, seed+871)+1.0)*0.5;
            ground.push(round(self.base_height + camp * (hills + rim)));
            depth.push(round(self.camp_snow_depth * (1.0-camp) + camp *
                (self.minimum_snow_depth + drift * (self.maximum_snow_depth-self.minimum_snow_depth))));
        }}
        let spec = SnowFieldSpec {id, seed, origin, cell_size, columns, rows, ground, depth};
        spec.validate()?;
        Ok(spec)
    }
}
fn terrain_noise(x: f64, z: f64, seed: u64) -> f64 {
    let hash = |i: i64, j: i64| {
        let mut v = (i.wrapping_mul(374761393).wrapping_add(j.wrapping_mul(668265263))
            .wrapping_add((seed as i64).wrapping_mul(69069))) as u32;
        v = (v ^ (v >> 13)).wrapping_mul(1274126177);
        (v ^ (v >> 16)) as f64 / 4294967295.0 * 2.0 - 1.0
    };
    let i=x.floor() as i64;let j=z.floor() as i64;
    let u=x-i as f64;let v=z-j as f64;
    let u=u*u*(3.0-2.0*u);let v=v*v*(3.0-2.0*v);
    (hash(i,j)*(1.0-u)+hash(i+1,j)*u)*(1.0-v)+(hash(i,j+1)*(1.0-u)+hash(i+1,j+1)*u)*v
}

impl SnowField {
    pub fn extend(&mut self, patch: SnowFieldSpec) -> Result<bool, String> {
        patch.validate()?;
        self.validate()?;
        let old = &self.spec;
        if patch.id != old.id || patch.seed != old.seed || patch.cell_size != old.cell_size {
            return Err("snow expansion identity or resolution mismatch".into());
        }
        let offset = |axis: usize| -> Result<i64, String> {
            let value = (patch.origin[axis] - old.origin[axis]) / old.cell_size;
            if (value - value.round()).abs() > 0.0001 {
                return Err("snow expansion must align with the existing grid".into());
            }
            Ok(value.round() as i64)
        };
        let px = offset(0)?;
        let pz = offset(1)?;
        if px >= 0 && pz >= 0 && px + patch.columns as i64 <= old.columns as i64 &&
            pz + patch.rows as i64 <= old.rows as i64 {
            // A loaded expedition already owns these cells. Check only the
            // requested strip instead of allocating another whole-world grid.
            for z in 0..patch.rows { for x in 0..patch.columns {
                let a = (z + pz as usize) * old.columns + x + px as usize;
                let b = z * patch.columns + x;
                if (old.ground[a] - patch.ground[b]).abs() > 0.0002 {
                    return Err("snow expansion changes existing ground".into());
                }
            }}
            return Ok(false);
        }
        let min_x = px.min(0);
        let min_z = pz.min(0);
        let max_x = (px + patch.columns as i64 - 1).max(old.columns as i64 - 1);
        let max_z = (pz + patch.rows as i64 - 1).max(old.rows as i64 - 1);
        let columns = (max_x - min_x + 1) as usize;
        let rows = (max_z - min_z + 1) as usize;
        if columns > 1025 || rows > 1025 {
            return Err("snow expansion exceeds the bounded grid budget".into());
        }
        let mut ground = Vec::with_capacity(columns * rows);
        let mut depth = Vec::with_capacity(columns * rows);
        let mut mapping = Vec::with_capacity(columns * rows);
        let mut overlap = false;
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                let a = (x >= 0 && z >= 0 && x < old.columns as i64 && z < old.rows as i64)
                    .then(|| z as usize * old.columns + x as usize);
                let b = (x >= px && z >= pz && x < px + patch.columns as i64 && z < pz + patch.rows as i64)
                    .then(|| (z - pz) as usize * patch.columns + (x - px) as usize);
                if let (Some(a), Some(b)) = (a, b) {
                    overlap = true;
                    if (old.ground[a] - patch.ground[b]).abs() > 0.0002 {
                        return Err("snow expansion changes existing ground".into());
                    }
                }
                let (g, d) = if let Some(a) = a {
                    (old.ground[a], old.depth[a])
                } else if let Some(b) = b {
                    (patch.ground[b], patch.depth[b])
                } else {
                    return Err("snow expansion leaves a gap in the rectangular grid".into());
                };
                ground.push(g);
                depth.push(d);
                mapping.push(a);
            }
        }
        if !overlap { return Err("snow expansion must share an existing border".into()); }
        if columns == old.columns && rows == old.rows && min_x == 0 && min_z == 0 {
            return Ok(false); // replay after loading a saved expanded field
        }
        let spec = SnowFieldSpec {
            id: old.id.clone(), seed: old.seed, cell_size: old.cell_size, columns, rows,
            origin: [old.origin[0] + min_x as f32 * old.cell_size,
                old.origin[1] + min_z as f32 * old.cell_size], ground, depth,
        };
        let mut expanded = Self::new(spec, self.policy.clone())?;
        self.ensure_strata();
        self.ensure_liquids();
        if let Some(policy) = self.layer_policy { expanded.configure_layers(policy)?; }
        for (to, from) in mapping.into_iter().enumerate() {
            if let Some(from) = from {
                expanded.compaction[to] = self.compaction[from];
                expanded.strata[to] = self.strata[from];
                expanded.surface_water[to] = self.surface_water[from];
                expanded.surface_ice[to] = self.surface_ice[from];
                if self.layer_policy.is_some() {
                    expanded.layer_columns[to] = self.layer_columns[from].clone();
                }
            }
            expanded.dirty.insert(to);
            expanded.liquid_dirty.insert(to);
        }
        expanded.microprints = self.microprints.clone();
        *self = expanded;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec(origin: [f32; 2], columns: usize, rows: usize) -> SnowFieldSpec {
        SnowFieldSpec { id: "test".into(), seed: 42, origin, cell_size: 1.0,
            columns, rows, ground: vec![4.0; columns * rows], depth: vec![0.7; columns * rows] }
    }
    #[test]
    fn expansion_preserves_every_layer_and_replays_without_resetting() {
        let policy = SnowPolicy { compacted_fraction: 0.47, contact_compaction: 0.32,
            max_contact_radius: 2.0, ridge_displacement_fraction: 0.28 };
        let mut field = SnowField::new(spec([0.0, 0.0], 3, 3), policy).unwrap();
        let layers = serde_json::from_value(serde_json::json!({
            "contact_depth_metres":0.12,"surface_blend_metres":0.03
        })).unwrap();
        field.configure_layers(layers).unwrap();
        field.contact(1.0, 1.0, 0.8, 0.9).unwrap();
        field.surface_water[4] = 0.1;
        field.surface_ice[4] = 0.2;
        let before = field.sample(1.0, 1.0).unwrap();
        let before_layers = field.layer_profile(1.0, 1.0).unwrap();
        let strips = [spec([2.0, 0.0], 3, 3), spec([-2.0, 0.0], 3, 3),
            spec([-2.0, 2.0], 7, 3), spec([-2.0, -2.0], 7, 3)];
        for strip in strips {
            assert!(field.extend(strip.clone()).unwrap());
            assert!(!field.extend(strip).unwrap());
            let now = field.sample(1.0, 1.0).unwrap();
            assert_eq!(before.surface_height, now.surface_height);
            assert_eq!(before.compaction, now.compaction);
            assert_eq!(before.standing_water_depth, now.standing_water_depth);
            assert_eq!(before.ground_ice_depth, now.ground_ice_depth);
            assert_eq!(before_layers, field.layer_profile(1.0, 1.0).unwrap());
            field.validate().unwrap();
        }
        let checkpoint = serde_json::to_value(&field).unwrap();
        let restored: SnowField = serde_json::from_value(checkpoint).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.spec.origin, [-2.0, -2.0]);
        assert_eq!(restored.spec.columns, 7);
        let before = serde_json::to_value(&field).unwrap();
        assert!(field.extend(spec([9.0, 0.0], 3, 3)).is_err());
        let mut bad = spec([4.0, -2.0], 3, 7); bad.ground[0] = 99.0;
        assert!(field.extend(bad).is_err());
        assert_eq!(before, serde_json::to_value(&field).unwrap());
    }
}
