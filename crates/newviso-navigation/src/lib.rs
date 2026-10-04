mod connectivity;
mod geometry;
mod search;
mod spatial;
mod tiles;
mod types;

use connectivity::{edge_key, EdgeKey};
use geometry::*;
use serde_json::{json, Value};
use spatial::PolygonSpatialIndex;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};
pub use types::*;

#[derive(Clone, Debug)]
struct Portal {
    to: u64,
    left: Vec3,
    right: Vec3,
    cost_scale: f32,
    off_mesh: bool,
}

#[derive(Clone, Debug)]
struct Polygon {
    id: u64,
    vertices: [Vec3; 3],
    centroid: Vec3,
    portals: Vec<Portal>,
}

#[derive(Clone, Debug)]
struct Tile {
    polygon_ids: Vec<u64>,
}

#[derive(Clone, Debug)]
struct PendingTile {
    id: String,
    vertices: Vec<Vec3>,
    triangles: Vec<[u32; 3]>,
    next_triangle: usize,
    polygon_ids: Vec<u64>,
    min_up: f32,
}

#[derive(Clone, Debug)]
pub struct NavigationRuntime {
    config: NavBuildConfig,
    tiles: BTreeMap<String, Tile>,
    pending_tiles: VecDeque<PendingTile>,
    polygons: BTreeMap<u64, Polygon>,
    spatial_index: PolygonSpatialIndex,
    // Persistent edge index. Streaming one tile only touches the three edges
    // of each new/removed polygon instead of rebuilding the whole nav graph.
    edges: BTreeMap<EdgeKey, Vec<(u64, Vec3, Vec3)>>,
    links: BTreeMap<String, OffMeshLink>,
    obstacles: BTreeMap<String, DynamicObstacle>,
    requests: VecDeque<PathRequest>,
    results: BTreeMap<u64, PathResult>,
    next_polygon_id: u64,
    next_request_id: u64,
    revision: u64,
}

impl Default for NavigationRuntime {
    fn default() -> Self {
        Self::new(NavBuildConfig::default()).expect("default nav config")
    }
}

impl NavigationRuntime {
    pub fn new(config: NavBuildConfig) -> Result<Self, String> {
        config.validate()?;
        Ok(Self {
            config,
            tiles: BTreeMap::new(),
            pending_tiles: VecDeque::new(),
            polygons: BTreeMap::new(),
            spatial_index: PolygonSpatialIndex::default(),
            edges: BTreeMap::new(),
            links: BTreeMap::new(),
            obstacles: BTreeMap::new(),
            requests: VecDeque::new(),
            results: BTreeMap::new(),
            next_polygon_id: 1,
            next_request_id: 1,
            revision: 1,
        })
    }

    pub fn config(&self) -> NavBuildConfig {
        self.config
    }

    pub fn set_config(&mut self, config: NavBuildConfig) -> Result<(), String> {
        config.validate()?;
        self.config = config;
        self.revision = self.revision.wrapping_add(1).max(1);
        self.rebuild_connectivity();
        Ok(())
    }
    pub fn upsert_off_mesh_link(&mut self, link: OffMeshLink) -> Result<(), String> {
        link.validate()?;
        self.links.insert(link.id.clone(), link);
        self.revision = self.revision.wrapping_add(1).max(1);
        self.rebuild_off_mesh_links();
        Ok(())
    }

    pub fn remove_off_mesh_link(&mut self, id: &str) -> bool {
        let removed = self.links.remove(id.trim()).is_some();
        if removed {
            self.revision = self.revision.wrapping_add(1).max(1);
            self.rebuild_off_mesh_links();
        }
        removed
    }

    pub fn upsert_obstacle(&mut self, obstacle: DynamicObstacle) -> Result<(), String> {
        obstacle.validate()?;
        self.obstacles.insert(obstacle.id.clone(), obstacle);
        self.revision = self.revision.wrapping_add(1).max(1);
        Ok(())
    }

    pub fn remove_obstacle(&mut self, id: &str) -> bool {
        let removed = self.obstacles.remove(id.trim()).is_some();
        if removed {
            self.revision = self.revision.wrapping_add(1).max(1);
        }
        removed
    }
    pub fn active_obstacles(&self) -> Vec<DynamicObstacle> {
        self.obstacles
            .values()
            .filter(|obstacle| obstacle.enabled)
            .cloned()
            .collect()
    }

    pub fn runtime_state(&self) -> Value {
        json!({
            "revision": self.revision,
            "tiles": self.tiles.len(),
            "pending_tiles": self.pending_tiles.len(),
            "pending_triangles": self.pending_tiles.iter()
                .map(|tile| tile.triangles.len().saturating_sub(tile.next_triangle))
                .sum::<usize>(),
            "polygons": self.polygons.len(),
            "indexed_edges": self.edges.len(),
            "spatial_cells": self.spatial_index.cell_count(),
            "oversized_polygons": self.spatial_index.oversized_count(),
            "off_mesh_links": self.links.len(),
            "dynamic_obstacles": self.obstacles.len(),
            "queued_requests": self.requests.len(),
            "ready_results": self.results.len(),
            "config": self.config,
        })
    }
}

#[cfg(test)]
mod tests;
