use super::*;

impl EngineApplication {
    pub(super) fn sync_vehicle_particle_interior(&mut self, entity: u64) -> Result<(), String> {
        if self.scene.entity_particle_interior_configured(entity) {
            return Ok(());
        }
        let Some(binding) = self.vehicle_presentations.get(&entity) else {
            return Ok(());
        };
        let seats = binding
            .parts
            .iter()
            .filter(|p| p.role == ModelFragmentPartRole::Seat)
            .collect::<Vec<_>>();
        let glass = binding
            .parts
            .iter()
            .filter(|p| {
                p.role == ModelFragmentPartRole::Glass
                    && (p.name_lower.contains("window")
                        || p.name_lower.contains("windscreen")
                        || p.name_lower.contains("windshield"))
            })
            .collect::<Vec<_>>();
        if seats.is_empty() || glass.is_empty() {
            return Ok(());
        }
        let names = glass
            .iter()
            .flat_map(|p| p.mesh_names.iter().cloned())
            .collect::<Vec<_>>();
        let Some((mut min, mut max)) = self.scene.entity_fragment_mesh_bounds(entity, &names)
        else {
            return Ok(());
        };
        // These are authored local glass and seat coordinates, not the whole
        // vehicle bounds (which would incorrectly erase effects at the wheels).
        min[0] += 0.01;
        max[0] -= 0.01;
        min[2] += 0.005;
        max[2] -= 0.005;
        min[1] = seats
            .iter()
            .map(|p| p.pivot[1] - 0.18)
            .fold(f32::INFINITY, f32::min);
        max[1] += 0.06;
        if (0..3).any(|i| max[i] - min[i] <= 0.05) {
            return Ok(());
        }
        let inside = std::array::from_fn(|i| {
            seats.iter().map(|p| p.pivot[i]).sum::<f32>() / seats.len() as f32
                + if i == 1 { 0.35 } else { 0.0 }
        });
        let mut planes = Vec::new();
        for rear in [false, true] {
            let names = glass
                .iter()
                .filter(|p| {
                    (p.name_lower.contains("windscreen") || p.name_lower.contains("windshield"))
                        && (p.name_lower.ends_with("_r") || p.name_lower.contains("rear")) == rear
                })
                .flat_map(|p| p.mesh_names.iter().cloned())
                .collect::<Vec<_>>();
            if let Some(plane) = self
                .scene
                .entity_fragment_interior_plane(entity, &names, inside)
            {
                planes.push(plane);
            }
        }
        for part in glass.iter().filter(|p| p.name_lower.contains("window")) {
            if let Some(plane) =
                self.scene
                    .entity_fragment_interior_plane(entity, &part.mesh_names, inside)
            {
                let duplicate = planes.iter().any(|p| {
                    (0..3).map(|i| p[i] * plane[i]).sum::<f32>() > 0.999
                        && (p[3] - plane[3]).abs() < 0.02
                });
                if !duplicate && planes.len() < 8 {
                    planes.push(plane);
                }
            }
        }
        self.scene
            .set_entity_particle_interior(entity, min, max, planes)
    }
}
