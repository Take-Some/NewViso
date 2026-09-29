use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque},
};

pub type Vec3 = [f32; 3];

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NavBuildConfig {
    pub max_slope_degrees: f32,
    pub weld_epsilon: f32,
    pub max_snap_distance: f32,
}

impl Default for NavBuildConfig {
    fn default() -> Self {
        Self {
            max_slope_degrees: 50.0,
            weld_epsilon: 0.02,
            max_snap_distance: 4.0,
        }
    }
}

impl NavBuildConfig {
    pub fn validate(self) -> Result<(), String> {
        if !self.max_slope_degrees.is_finite()
            || !(0.0..89.0).contains(&self.max_slope_degrees)
            || !self.weld_epsilon.is_finite()
            || !(0.0001..=2.0).contains(&self.weld_epsilon)
            || !self.max_snap_distance.is_finite()
            || !(0.01..=1000.0).contains(&self.max_snap_distance)
        {
            return Err("invalid navmesh build config".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NavTileSource {
    pub id: String,
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
}

impl NavTileSource {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() || self.id.len() > 128 {
            return Err("navmesh tile id must be non-empty and <= 128 bytes".to_owned());
        }
        if self.vertices.is_empty() || self.triangles.is_empty() {
            return Err(format!("navmesh tile '{}' is empty", self.id));
        }
        if self.vertices.iter().flatten().any(|v| !v.is_finite()) {
            return Err(format!(
                "navmesh tile '{}' contains non-finite vertices",
                self.id
            ));
        }
        let count = self.vertices.len() as u64;
        if self
            .triangles
            .iter()
            .flatten()
            .any(|index| u64::from(*index) >= count)
        {
            return Err(format!(
                "navmesh tile '{}' has out-of-range triangle index",
                self.id
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OffMeshLink {
    pub id: String,
    pub start: Vec3,
    pub end: Vec3,
    #[serde(default = "default_true")]
    pub bidirectional: bool,
    #[serde(default = "default_cost")]
    pub cost_scale: f32,
    #[serde(default)]
    pub kind: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}
fn default_cost() -> f32 {
    1.0
}

impl OffMeshLink {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty()
            || self.id.len() > 128
            || self
                .start
                .iter()
                .chain(self.end.iter())
                .any(|v| !v.is_finite())
            || !self.cost_scale.is_finite()
            || !(0.001..=1000.0).contains(&self.cost_scale)
        {
            return Err("invalid off-mesh link".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DynamicObstacle {
    pub id: String,
    pub center: Vec3,
    pub radius: f32,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl DynamicObstacle {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty()
            || self.id.len() > 128
            || self.center.iter().any(|v| !v.is_finite())
            || !self.radius.is_finite()
            || !(0.01..=1000.0).contains(&self.radius)
        {
            return Err("invalid navigation obstacle".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PathRequest {
    pub id: u64,
    pub start: Vec3,
    pub end: Vec3,
    pub agent_radius: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathStatus {
    Found,
    NoStartPolygon,
    NoEndPolygon,
    NoPath,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PathResult {
    pub id: u64,
    pub status: PathStatus,
    pub corridor: Vec<u64>,
    pub waypoints: Vec<Vec3>,
    pub visited_polygons: usize,
}

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

#[derive(Clone, Copy, Debug)]
struct HeapNode {
    id: u64,
    f: f32,
}

impl Eq for HeapNode {}
impl PartialEq for HeapNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.f.to_bits() == other.f.to_bits()
    }
}
impl Ord for HeapNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f
            .total_cmp(&self.f)
            .then_with(|| other.id.cmp(&self.id))
    }
}
impl PartialOrd for HeapNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug)]
pub struct NavigationRuntime {
    config: NavBuildConfig,
    tiles: BTreeMap<String, Tile>,
    pending_tiles: VecDeque<PendingTile>,
    polygons: BTreeMap<u64, Polygon>,
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
        let time_budget = (max_micros != 0)
            .then(|| std::time::Duration::from_micros(max_micros));
        let mut processed = 0usize;
        let mut completed = 0usize;

        while processed < max_triangles
            && time_budget.is_none_or(|budget| started.elapsed() < budget)
        {
            let Some(front) = self.pending_tiles.front() else {
                break;
            };
            if front.next_triangle >= front.triangles.len() {
                let finished = self.pending_tiles.pop_front().expect("front exists");
                self.tiles.insert(
                    finished.id,
                    Tile {
                        polygon_ids: finished.polygon_ids,
                    },
                );
                self.revision = self.revision.wrapping_add(1).max(1);
                completed = completed.saturating_add(1);
                if !self.links.is_empty() {
                    self.rebuild_off_mesh_links();
                }
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
            let finished = self.pending_tiles.pop_front().expect("front exists");
            self.tiles.insert(
                finished.id,
                Tile {
                    polygon_ids: finished.polygon_ids,
                },
            );
            self.revision = self.revision.wrapping_add(1).max(1);
            completed = completed.saturating_add(1);
            if !self.links.is_empty() {
                self.rebuild_off_mesh_links();
            }
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
            let pending = self.pending_tiles.remove(index).expect("position came from queue");
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

    pub fn request_path(
        &mut self,
        start: Vec3,
        end: Vec3,
        agent_radius: f32,
    ) -> Result<u64, String> {
        if start
            .iter()
            .chain(end.iter())
            .any(|value| !value.is_finite())
            || !agent_radius.is_finite()
            || !(0.0..=20.0).contains(&agent_radius)
        {
            return Err("invalid navigation path request".to_owned());
        }
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        self.requests.push_back(PathRequest {
            id,
            start,
            end,
            agent_radius,
        });
        Ok(id)
    }

    pub fn pump(&mut self, max_requests: usize) -> usize {
        let mut processed = 0;
        for _ in 0..max_requests {
            let Some(request) = self.requests.pop_front() else {
                break;
            };
            let result = self.solve(&request);
            self.results.insert(request.id, result);
            processed += 1;
        }
        processed
    }

    pub fn take_result(&mut self, id: u64) -> Option<PathResult> {
        self.results.remove(&id)
    }

    pub fn solve_now(&self, start: Vec3, end: Vec3, agent_radius: f32) -> PathResult {
        self.solve(&PathRequest {
            id: 0,
            start,
            end,
            agent_radius,
        })
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
            "off_mesh_links": self.links.len(),
            "dynamic_obstacles": self.obstacles.len(),
            "queued_requests": self.requests.len(),
            "ready_results": self.results.len(),
            "config": self.config,
        })
    }

    fn solve(&self, request: &PathRequest) -> PathResult {
        let Some(start_id) = self.closest_polygon(request.start, request.agent_radius) else {
            return failed(request.id, PathStatus::NoStartPolygon);
        };
        let Some(end_id) = self.closest_polygon(request.end, request.agent_radius) else {
            return failed(request.id, PathStatus::NoEndPolygon);
        };

        if start_id == end_id {
            return PathResult {
                id: request.id,
                status: PathStatus::Found,
                corridor: vec![start_id],
                waypoints: vec![request.start, request.end],
                visited_polygons: 1,
            };
        }

        let mut open = BinaryHeap::new();
        let mut came_from = BTreeMap::<u64, u64>::new();
        let mut g = BTreeMap::<u64, f32>::from([(start_id, 0.0)]);
        let mut closed = BTreeSet::new();
        open.push(HeapNode {
            id: start_id,
            f: distance(
                self.polygons[&start_id].centroid,
                self.polygons[&end_id].centroid,
            ),
        });

        while let Some(current) = open.pop() {
            if !closed.insert(current.id) {
                continue;
            }
            if current.id == end_id {
                let corridor = reconstruct(start_id, end_id, &came_from);
                let portals = self.corridor_portals(&corridor);
                let waypoints = string_pull(request.start, request.end, &portals);
                return PathResult {
                    id: request.id,
                    status: PathStatus::Found,
                    corridor,
                    waypoints,
                    visited_polygons: closed.len(),
                };
            }

            let current_poly = &self.polygons[&current.id];
            for portal in &current_poly.portals {
                let Some(next) = self.polygons.get(&portal.to) else {
                    continue;
                };
                if self.polygon_blocked(next, request.agent_radius)
                    || self.portal_blocked(portal, request.agent_radius)
                    || (!portal.off_mesh
                        && horizontal_distance(portal.left, portal.right)
                            + self.config.weld_epsilon
                            < request.agent_radius * 2.0)
                {
                    continue;
                }
                let tentative = g[&current.id]
                    + distance(current_poly.centroid, next.centroid) * portal.cost_scale;
                if tentative + 1.0e-6 < g.get(&next.id).copied().unwrap_or(f32::INFINITY) {
                    came_from.insert(next.id, current.id);
                    g.insert(next.id, tentative);
                    let h = distance(next.centroid, self.polygons[&end_id].centroid);
                    open.push(HeapNode {
                        id: next.id,
                        f: tentative + h,
                    });
                }
            }
        }

        PathResult {
            id: request.id,
            status: PathStatus::NoPath,
            corridor: Vec::new(),
            waypoints: Vec::new(),
            visited_polygons: closed.len(),
        }
    }

    fn closest_polygon(&self, point: Vec3, radius: f32) -> Option<u64> {
        self.polygons
            .values()
            .filter(|poly| !self.polygon_blocked(poly, radius))
            .map(|poly| {
                let closest = closest_point_on_triangle(point, poly.vertices);
                (poly.id, distance(point, closest))
            })
            .filter(|(_, distance)| *distance <= self.config.max_snap_distance)
            .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
            .map(|(id, _)| id)
    }

    fn polygon_blocked(&self, polygon: &Polygon, agent_radius: f32) -> bool {
        self.obstacles.values().any(|obstacle| {
            obstacle.enabled
                && horizontal_distance(obstacle.center, polygon.centroid)
                    < obstacle.radius + agent_radius
        })
    }

    fn portal_blocked(&self, portal: &Portal, agent_radius: f32) -> bool {
        if portal.off_mesh {
            return false;
        }
        let midpoint = mul(add(portal.left, portal.right), 0.5);
        self.obstacles.values().any(|obstacle| {
            obstacle.enabled
                && horizontal_distance(obstacle.center, midpoint) < obstacle.radius + agent_radius
        })
    }

    fn corridor_portals(&self, corridor: &[u64]) -> Vec<(Vec3, Vec3)> {
        let mut out = Vec::new();
        for pair in corridor.windows(2) {
            let Some(poly) = self.polygons.get(&pair[0]) else {
                continue;
            };
            if let Some(portal) = poly.portals.iter().find(|portal| portal.to == pair[1]) {
                out.push((portal.left, portal.right));
            }
        }
        out
    }

    fn index_polygon_edges(&mut self, polygon_id: u64) {
        let Some(vertices) = self.polygons.get(&polygon_id).map(|poly| poly.vertices) else {
            return;
        };
        let eps = self.config.weld_epsilon;
        for i in 0..3 {
            let a = vertices[i];
            let b = vertices[(i + 1) % 3];
            let key = edge_key(a, b, eps);
            let neighbors = self.edges.get(&key).cloned().unwrap_or_default();
            for (neighbor_id, neighbor_a, neighbor_b) in neighbors {
                if neighbor_id == polygon_id {
                    continue;
                }
                self.connect_base_portal(polygon_id, neighbor_id, a, b);
                self.connect_base_portal(neighbor_id, polygon_id, neighbor_a, neighbor_b);
            }
            self.edges.entry(key).or_default().push((polygon_id, a, b));
        }
    }

    fn connect_base_portal(&mut self, from_id: u64, to_id: u64, a: Vec3, b: Vec3) {
        let Some(from) = self.polygons.get(&from_id).map(|poly| poly.centroid) else {
            return;
        };
        let Some(to) = self.polygons.get(&to_id).map(|poly| poly.centroid) else {
            return;
        };
        let (left, right) = orient_portal(a, b, from, to);
        if let Some(poly) = self.polygons.get_mut(&from_id) {
            if !poly
                .portals
                .iter()
                .any(|portal| !portal.off_mesh && portal.to == to_id)
            {
                poly.portals.push(Portal {
                    to: to_id,
                    left,
                    right,
                    cost_scale: 1.0,
                    off_mesh: false,
                });
            }
        }
    }

    fn rebuild_off_mesh_links(&mut self) {
        for polygon in self.polygons.values_mut() {
            polygon.portals.retain(|portal| !portal.off_mesh);
        }

        let links = self
            .links
            .values()
            .filter(|link| link.enabled)
            .cloned()
            .collect::<Vec<_>>();
        for link in links {
            let start = self.closest_polygon_unblocked(link.start);
            let end = self.closest_polygon_unblocked(link.end);
            let (Some(start), Some(end)) = (start, end) else {
                continue;
            };
            if let Some(poly) = self.polygons.get_mut(&start) {
                poly.portals.push(Portal {
                    to: end,
                    left: link.start,
                    right: link.start,
                    cost_scale: link.cost_scale,
                    off_mesh: true,
                });
            }
            if link.bidirectional {
                if let Some(poly) = self.polygons.get_mut(&end) {
                    poly.portals.push(Portal {
                        to: start,
                        left: link.end,
                        right: link.end,
                        cost_scale: link.cost_scale,
                        off_mesh: true,
                    });
                }
            }
        }
    }

    fn rebuild_connectivity(&mut self) {
        for polygon in self.polygons.values_mut() {
            polygon.portals.clear();
        }
        self.edges.clear();

        let ids = self.polygons.keys().copied().collect::<Vec<_>>();
        for id in ids {
            self.index_polygon_edges(id);
        }
        self.rebuild_off_mesh_links();
    }

    fn closest_polygon_unblocked(&self, point: Vec3) -> Option<u64> {
        self.polygons
            .values()
            .map(|poly| {
                (
                    poly.id,
                    distance(point, closest_point_on_triangle(point, poly.vertices)),
                )
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
            .map(|(id, _)| id)
    }
}

fn failed(id: u64, status: PathStatus) -> PathResult {
    PathResult {
        id,
        status,
        corridor: Vec::new(),
        waypoints: Vec::new(),
        visited_polygons: 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EdgeKey([i64; 3], [i64; 3]);

fn edge_key(a: Vec3, b: Vec3, eps: f32) -> EdgeKey {
    let qa = quantize(a, eps);
    let qb = quantize(b, eps);
    if qa <= qb {
        EdgeKey(qa, qb)
    } else {
        EdgeKey(qb, qa)
    }
}

fn quantize(v: Vec3, eps: f32) -> [i64; 3] {
    [
        (v[0] / eps).round() as i64,
        (v[1] / eps).round() as i64,
        (v[2] / eps).round() as i64,
    ]
}

fn reconstruct(start: u64, end: u64, came_from: &BTreeMap<u64, u64>) -> Vec<u64> {
    let mut out = vec![end];
    let mut current = end;
    while current != start {
        let Some(previous) = came_from.get(&current).copied() else {
            return Vec::new();
        };
        current = previous;
        out.push(current);
    }
    out.reverse();
    out
}

fn orient_portal(a: Vec3, b: Vec3, from: Vec3, to: Vec3) -> (Vec3, Vec3) {
    let dir = [to[0] - from[0], to[2] - from[2]];
    let edge = [b[0] - a[0], b[2] - a[2]];
    if cross2(dir, edge) >= 0.0 {
        (a, b)
    } else {
        (b, a)
    }
}

fn string_pull(start: Vec3, end: Vec3, portals: &[(Vec3, Vec3)]) -> Vec<Vec3> {
    if portals.is_empty() {
        return vec![start, end];
    }

    let mut all = Vec::with_capacity(portals.len() + 2);
    all.push((start, start));
    all.extend_from_slice(portals);
    all.push((end, end));

    let mut points = vec![start];
    let mut apex = start;
    let mut left = all[1].0;
    let mut right = all[1].1;
    let mut apex_index: usize;
    let mut left_index = 1usize;
    let mut right_index = 1usize;
    let mut i = 2usize;

    while i < all.len() {
        let (new_left, new_right) = all[i];

        if tri_area2(apex, right, new_right) <= 0.0 {
            if same_xz(apex, right) || tri_area2(apex, left, new_right) > 0.0 {
                right = new_right;
                right_index = i;
            } else {
                points.push(left);
                apex = left;
                apex_index = left_index;
                left = apex;
                right = apex;
                left_index = apex_index;
                right_index = apex_index;
                i = apex_index + 1;
                continue;
            }
        }

        if tri_area2(apex, left, new_left) >= 0.0 {
            if same_xz(apex, left) || tri_area2(apex, right, new_left) < 0.0 {
                left = new_left;
                left_index = i;
            } else {
                points.push(right);
                apex = right;
                apex_index = right_index;
                left = apex;
                right = apex;
                left_index = apex_index;
                right_index = apex_index;
                i = apex_index + 1;
                continue;
            }
        }

        i += 1;
    }

    if points
        .last()
        .is_none_or(|point| distance(*point, end) > 1.0e-4)
    {
        points.push(end);
    }
    points
}

fn tri_area2(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    (b[0] - a[0]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[0] - a[0])
}
fn cross2(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}
fn same_xz(a: Vec3, b: Vec3) -> bool {
    (a[0] - b[0]).abs() < 1.0e-5 && (a[2] - b[2]).abs() < 1.0e-5
}

fn triangle_normal(v: [Vec3; 3]) -> Vec3 {
    normalize(cross(sub(v[1], v[0]), sub(v[2], v[0])))
}
fn centroid(v: [Vec3; 3]) -> Vec3 {
    mul(add(add(v[0], v[1]), v[2]), 1.0 / 3.0)
}
fn closest_point_on_triangle(p: Vec3, tri: [Vec3; 3]) -> Vec3 {
    // Christer Ericson, Real-Time Collision Detection.
    let a = tri[0];
    let b = tri[1];
    let c = tri[2];
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add(a, mul(ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add(a, mul(ac, d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return add(b, mul(sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    add(a, add(mul(ab, v), mul(ac, w)))
}

fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    let dx = a[0] - b[0];
    let dz = a[2] - b[2];
    (dx * dx + dz * dz).sqrt()
}
fn distance(a: Vec3, b: Vec3) -> f32 {
    length(sub(a, b))
}
fn length(v: Vec3) -> f32 {
    dot(v, v).sqrt()
}
fn normalize(v: Vec3) -> Vec3 {
    let len = length(v);
    if len <= 1.0e-8 {
        [0.0, 1.0, 0.0]
    } else {
        mul(v, 1.0 / len)
    }
}
fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(v: Vec3, s: f32) -> Vec3 {
    [v[0] * s, v[1] * s, v[2] * s]
}
fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor_tile(id: &str, x: f32) -> NavTileSource {
        NavTileSource {
            id: id.to_owned(),
            vertices: vec![
                [x, 0.0, 0.0],
                [x + 2.0, 0.0, 0.0],
                [x + 2.0, 0.0, 2.0],
                [x, 0.0, 2.0],
            ],
            triangles: vec![[0, 2, 1], [0, 3, 2]],
        }
    }

    #[test]
    fn streamed_tiles_form_cross_tile_corridor() {
        let mut nav = NavigationRuntime::default();
        assert_eq!(nav.upsert_tile(floor_tile("a", 0.0)).unwrap(), 2);
        assert_eq!(nav.upsert_tile(floor_tile("b", 2.0)).unwrap(), 2);

        let result = nav.solve_now([0.25, 0.1, 1.0], [3.75, 0.1, 1.0], 0.25);
        assert_eq!(result.status, PathStatus::Found);
        assert!(result.corridor.len() >= 2);
        assert_eq!(result.waypoints.first().copied(), Some([0.25, 0.1, 1.0]));
        assert_eq!(result.waypoints.last().copied(), Some([3.75, 0.1, 1.0]));
    }

    #[test]
    fn streamed_tile_ingestion_is_incremental() {
        let mut nav = NavigationRuntime::default();
        let mut source = floor_tile("incremental", 0.0);
        source.triangles = vec![[0, 2, 1]; 32];
        nav.queue_tile(source).unwrap();

        assert_eq!(nav.pump_tile_build(4, 0), 0);
        assert_eq!(nav.runtime_state()["pending_tiles"], 1);
        assert!(nav.runtime_state()["pending_triangles"].as_u64().unwrap() <= 28);

        while nav.runtime_state()["pending_tiles"] != 0 {
            nav.pump_tile_build(4, 0);
        }
        assert_eq!(nav.runtime_state()["tiles"], 1);
    }

    #[test]
    fn streamed_tile_removal_updates_only_affected_connectivity() {
        let mut nav = NavigationRuntime::default();
        nav.upsert_tile(floor_tile("a", 0.0)).unwrap();
        nav.upsert_tile(floor_tile("b", 2.0)).unwrap();
        nav.upsert_tile(floor_tile("c", 4.0)).unwrap();
        assert!(nav.remove_tile("b"));

        let result = nav.solve_now([0.25, 0.1, 1.0], [5.75, 0.1, 1.0], 0.25);
        assert_eq!(result.status, PathStatus::NoPath);

        nav.upsert_tile(floor_tile("b", 2.0)).unwrap();
        let result = nav.solve_now([0.25, 0.1, 1.0], [5.75, 0.1, 1.0], 0.25);
        assert_eq!(result.status, PathStatus::Found);
    }

    #[test]
    fn downward_facing_horizontal_surface_is_not_walkable() {
        let mut nav = NavigationRuntime::default();
        let count = nav
            .upsert_tile(NavTileSource {
                id: "ceiling".to_owned(),
                vertices: vec![[0.0, 2.0, 0.0], [2.0, 2.0, 0.0], [0.0, 2.0, 2.0]],
                triangles: vec![[0, 1, 2]],
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn steep_geometry_is_not_walkable() {
        let mut nav = NavigationRuntime::default();
        let count = nav
            .upsert_tile(NavTileSource {
                id: "wall".to_owned(),
                vertices: vec![[0.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 2.0, 2.0]],
                triangles: vec![[0, 1, 2]],
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn async_queue_is_budgeted() {
        let mut nav = NavigationRuntime::default();
        nav.upsert_tile(floor_tile("a", 0.0)).unwrap();
        let a = nav
            .request_path([0.2, 0.1, 0.2], [1.8, 0.1, 1.8], 0.2)
            .unwrap();
        let b = nav
            .request_path([0.3, 0.1, 0.2], [1.7, 0.1, 1.8], 0.2)
            .unwrap();
        assert_eq!(nav.pump(1), 1);
        assert!(nav.take_result(a).is_some());
        assert!(nav.take_result(b).is_none());
        assert_eq!(nav.pump(1), 1);
        assert!(nav.take_result(b).is_some());
    }

    #[test]
    fn obstacle_can_block_small_nav_island() {
        let mut nav = NavigationRuntime::default();
        nav.upsert_tile(floor_tile("a", 0.0)).unwrap();
        nav.upsert_obstacle(DynamicObstacle {
            id: "crate".to_owned(),
            center: [1.0, 0.0, 1.0],
            radius: 2.0,
            enabled: true,
        })
        .unwrap();
        let result = nav.solve_now([0.2, 0.1, 0.2], [1.8, 0.1, 1.8], 0.2);
        assert!(matches!(
            result.status,
            PathStatus::NoStartPolygon | PathStatus::NoEndPolygon | PathStatus::NoPath
        ));
    }
}
