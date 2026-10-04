use super::*;

impl NavigationRuntime {
    pub(super) fn index_polygon_edges(&mut self, polygon_id: u64) {
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

    pub(super) fn rebuild_off_mesh_links(&mut self) {
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

    pub(super) fn rebuild_connectivity(&mut self) {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct EdgeKey([i64; 3], [i64; 3]);

pub(super) fn edge_key(a: Vec3, b: Vec3, eps: f32) -> EdgeKey {
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
