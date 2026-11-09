use super::*;
use std::cmp::Ordering;

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

impl NavigationRuntime {
    pub fn request_path(
        &mut self,
        start: Vec3,
        end: Vec3,
        agent_radius: f32,
    ) -> Result<u64, String> {
        self.request_path_with_access(start, end, agent_radius, BTreeSet::new())
    }

    pub fn request_path_with_access(
        &mut self,
        start: Vec3,
        end: Vec3,
        agent_radius: f32,
        gate_access: BTreeSet<String>,
    ) -> Result<u64, String> {
        self.request_character_path(start, end, agent_radius, 2.0 * agent_radius, gate_access)
    }

    pub fn request_character_path(
        &mut self,
        start: Vec3,
        end: Vec3,
        agent_radius: f32,
        agent_height: f32,
        gate_access: BTreeSet<String>,
    ) -> Result<u64, String> {
        self.request_character_path_with_step(
            start,
            end,
            agent_radius,
            agent_height,
            gate_access,
            self.config.max_step_height,
        )
    }
    pub fn request_character_path_with_step(
        &mut self,
        start: Vec3,
        end: Vec3,
        agent_radius: f32,
        agent_height: f32,
        gate_access: BTreeSet<String>,
        max_step_height: f32,
    ) -> Result<u64, String> {
        if !max_step_height.is_finite() || !(0.0..=2.0).contains(&max_step_height) {
            return Err("invalid navigation step height".into());
        }
        if !agent_height.is_finite() || agent_height < 0.0 {
            return Err("invalid navigation agent height".into());
        }
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
            gate_access,
            agent_height,
            max_step_height,
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

    pub fn cancel_request(&mut self, id: u64) {
        self.requests.retain(|r| r.id != id);
        self.results.remove(&id);
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
            gate_access: BTreeSet::new(),
            agent_height: 2.0 * agent_radius,
            max_step_height: self.config.max_step_height,
        })
    }
    pub(super) fn solve(&self, request: &PathRequest) -> PathResult {
        let Some(start_id) = self.closest_polygon(request.start, request.agent_radius) else {
            return failed(request.id, PathStatus::NoStartPolygon);
        };
        let Some(end_id) = self.closest_polygon(request.end, request.agent_radius) else {
            return failed(request.id, PathStatus::NoEndPolygon);
        };

        if start_id == end_id {
            if self
                .gate_cost(request.start, request.end, request)
                .is_none()
            {
                return failed(request.id, PathStatus::NoPath);
            }
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
        let mut chosen = BTreeMap::<u64, Portal>::new();
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
                let portals = corridor
                    .iter()
                    .skip(1)
                    .map(|id| {
                        let p = &chosen[id];
                        let width = horizontal_distance(p.left, p.right);
                        let inset = if p.off_mesh || width <= f32::EPSILON {
                            0.0
                        } else {
                            ((request.agent_radius + self.config.weld_epsilon) / width).min(0.5)
                        };
                        (
                            add(p.left, mul(sub(p.right, p.left), inset)),
                            add(p.right, mul(sub(p.left, p.right), inset)),
                        )
                    })
                    .collect::<Vec<_>>();
                let mut waypoints = string_pull(request.start, request.end, &portals);
                if waypoints
                    .windows(2)
                    .any(|p| self.gate_cost(p[0], p[1], request).is_none())
                {
                    waypoints = vec![request.start];
                    for id in corridor.iter().skip(1) {
                        let p = &chosen[id];
                        waypoints.push(mul(add(p.left, p.right), 0.5));
                        waypoints.push(if *id == end_id {
                            request.end
                        } else {
                            self.polygons[id].centroid
                        });
                    }
                }
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
                if portal.step_height > request.max_step_height + self.config.weld_epsilon
                    || self.polygon_blocked(next, request.agent_radius)
                    || self.portal_blocked(portal, request.agent_radius)
                    || (!portal.off_mesh
                        && horizontal_distance(portal.left, portal.right)
                            + self.config.weld_epsilon
                            < request.agent_radius * 2.0)
                {
                    continue;
                }
                let a = if current.id == start_id {
                    request.start
                } else {
                    current_poly.centroid
                };
                let b = if next.id == end_id {
                    request.end
                } else {
                    next.centroid
                };
                let via = mul(add(portal.left, portal.right), 0.5);
                let (Some(ca), Some(cb)) = (
                    self.gate_cost(a, via, request),
                    self.gate_cost(via, b, request),
                ) else {
                    continue;
                };
                let tentative = g[&current.id]
                    + (distance(a, via) + distance(via, b)) * portal.cost_scale
                    + ca
                    + cb;
                if tentative + 1.0e-6 < g.get(&next.id).copied().unwrap_or(f32::INFINITY) {
                    came_from.insert(next.id, current.id);
                    chosen.insert(next.id, portal.clone());
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

    pub(super) fn closest_polygon(&self, point: Vec3, radius: f32) -> Option<u64> {
        match self
            .spatial_index
            .candidates(point, self.config.max_snap_distance)
        {
            Some(ids) => self.closest_polygon_from_ids(point, radius, ids),
            None => self.closest_polygon_from_ids(point, radius, self.polygons.keys().copied()),
        }
    }

    fn closest_polygon_from_ids(
        &self,
        point: Vec3,
        radius: f32,
        ids: impl Iterator<Item = u64>,
    ) -> Option<u64> {
        ids.filter_map(|id| self.polygons.get(&id))
            .filter(|poly| !self.polygon_blocked(poly, radius))
            .map(|poly| {
                (
                    poly.id,
                    distance(point, closest_point_on_triangle(point, poly.vertices)),
                )
            })
            .filter(|(_, distance)| *distance <= self.config.max_snap_distance)
            .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
            .map(|(id, _)| id)
    }

    fn polygon_blocked(&self, polygon: &Polygon, agent_radius: f32) -> bool {
        self.position_blocked(polygon.centroid, agent_radius)
    }

    fn portal_blocked(&self, portal: &Portal, agent_radius: f32) -> bool {
        if portal.off_mesh {
            return false;
        }
        let midpoint = mul(add(portal.left, portal.right), 0.5);
        self.position_blocked(midpoint, agent_radius)
    }

    fn position_blocked(&self, point: Vec3, agent_radius: f32) -> bool {
        self.obstacles.values().any(|obstacle| {
            obstacle.enabled
                && horizontal_distance(obstacle.center, point) < obstacle.radius + agent_radius
        })
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
