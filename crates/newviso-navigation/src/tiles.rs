use super::*;

impl NavigationRuntime {
    pub fn queue_tile(&mut self, source: NavTileSource) -> Result<(), String> {
        source.validate()?;
        let id = source.id.trim().to_owned();
        self.remove_tile(&id);
        self.pending_tiles.push_back(PendingTile {
            id,
            vertices: source.vertices,
            triangles: source.triangles,
            next_triangle: 0,
            polygon_ids: Vec::new(),
            min_up: self.config.max_slope_degrees.to_radians().cos(),
        });
        Ok(())
    }

    /// Incrementally ingest streamed navigation geometry.
    ///
    /// A zero microsecond limit disables the wall-clock limit and is intended
    /// only for tests/tools. Runtime callers use a small budget so a large
    /// streamed collision mesh never monopolizes the gameplay thread.
    pub fn pump_tile_build(&mut self, max_triangles: usize, max_micros: u64) -> usize {
        if max_triangles == 0 {
            return 0;
        }
        let started = std::time::Instant::now();
        let time_budget = (max_micros != 0).then(|| std::time::Duration::from_micros(max_micros));
        let mut processed = 0usize;
        let mut completed = 0usize;

        while processed < max_triangles
            && time_budget.is_none_or(|budget| started.elapsed() < budget)
        {
            let Some(front) = self.pending_tiles.front() else {
                break;
            };
            if front.next_triangle >= front.triangles.len() {
                self.complete_front_tile();
                completed = completed.saturating_add(1);
                continue;
            }

            let vertices = {
                let pending = self.pending_tiles.front_mut().expect("front exists");
                let triangle = pending.triangles[pending.next_triangle];
                pending.next_triangle += 1;
                processed = processed.saturating_add(1);
                let vertices = [
                    pending.vertices[triangle[0] as usize],
                    pending.vertices[triangle[1] as usize],
                    pending.vertices[triangle[2] as usize],
                ];
                (triangle_normal(vertices)[1] >= pending.min_up).then_some(vertices)
            };

            let Some(vertices) = vertices else {
                continue;
            };
            let polygon_id = self.next_polygon_id;
            self.next_polygon_id = self.next_polygon_id.wrapping_add(1).max(1);
            self.polygons.insert(
                polygon_id,
                Polygon {
                    id: polygon_id,
                    centroid: centroid(vertices),
                    vertices,
                    portals: Vec::new(),
                },
            );
            self.spatial_index.insert(polygon_id, vertices);
            self.index_polygon_edges(polygon_id);
            self.pending_tiles
                .front_mut()
                .expect("front exists")
                .polygon_ids
                .push(polygon_id);
        }

        if self
            .pending_tiles
            .front()
            .is_some_and(|pending| pending.next_triangle >= pending.triangles.len())
            && time_budget.is_none_or(|budget| started.elapsed() < budget)
        {
            self.complete_front_tile();
            completed = completed.saturating_add(1);
        }

        completed
    }

    pub fn upsert_tile(&mut self, source: NavTileSource) -> Result<usize, String> {
        let id = source.id.trim().to_owned();
        self.queue_tile(source)?;
        while self.pending_tiles.iter().any(|pending| pending.id == id) {
            self.pump_tile_build(usize::MAX, 0);
        }
        Ok(self.tiles.get(&id).map_or(0, |tile| tile.polygon_ids.len()))
    }

    pub fn remove_tile(&mut self, id: &str) -> bool {
        let id = id.trim();
        let mut removed_any = false;
        let mut polygon_ids = Vec::new();

        if let Some(index) = self.pending_tiles.iter().position(|tile| tile.id == id) {
            let pending = self
                .pending_tiles
                .remove(index)
                .expect("position came from queue");
            polygon_ids.extend(pending.polygon_ids);
            removed_any = true;
        }
        if let Some(tile) = self.tiles.remove(id) {
            polygon_ids.extend(tile.polygon_ids);
            removed_any = true;
        }
        if !removed_any {
            return false;
        }

        self.remove_polygon_ids(polygon_ids);
        self.revision = self.revision.wrapping_add(1).max(1);
        if !self.links.is_empty() {
            self.rebuild_off_mesh_links();
        }
        true
    }

    fn remove_polygon_ids(&mut self, polygon_ids: Vec<u64>) {
        if polygon_ids.is_empty() {
            return;
        }
        let removed = polygon_ids.into_iter().collect::<BTreeSet<_>>();
        let affected_neighbors = removed
            .iter()
            .filter_map(|polygon_id| self.polygons.get(polygon_id))
            .flat_map(|polygon| polygon.portals.iter())
            .filter(|portal| !portal.off_mesh && !removed.contains(&portal.to))
            .map(|portal| portal.to)
            .collect::<BTreeSet<_>>();

        for polygon_id in &removed {
            let Some(vertices) = self.polygons.get(polygon_id).map(|poly| poly.vertices) else {
                continue;
            };
            self.spatial_index.remove(*polygon_id, vertices);
            for i in 0..3 {
                let key = edge_key(vertices[i], vertices[(i + 1) % 3], self.config.weld_epsilon);
                let mut erase_key = false;
                if let Some(entries) = self.edges.get_mut(&key) {
                    entries.retain(|entry| entry.0 != *polygon_id);
                    erase_key = entries.is_empty();
                }
                if erase_key {
                    self.edges.remove(&key);
                }
            }
            self.polygons.remove(polygon_id);
        }

        for neighbor_id in affected_neighbors {
            if let Some(polygon) = self.polygons.get_mut(&neighbor_id) {
                polygon
                    .portals
                    .retain(|portal| portal.off_mesh || !removed.contains(&portal.to));
            }
        }
    }

    fn complete_front_tile(&mut self) {
        let finished = self.pending_tiles.pop_front().expect("front exists");
        self.tiles.insert(
            finished.id,
            Tile {
                polygon_ids: finished.polygon_ids,
            },
        );
        self.revision = self.revision.wrapping_add(1).max(1);
        if !self.links.is_empty() {
            self.rebuild_off_mesh_links();
        }
    }
}
