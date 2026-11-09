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
            let mut neighbors = Vec::new();
            // Bevelled stair edges can be within weld_epsilon while hashing
            // into adjacent buckets. Quantization must not become a crack.
            for da in [-1, 0, 1] {
                for dz in [-1, 0, 1] {
                    for db in [-1, 0, 1] {
                        for dw in [-1, 0, 1] {
                            let ka = [key.0[0] + da, 0, key.0[2] + dz];
                            let kb = [key.1[0] + db, 0, key.1[2] + dw];
                            let near = if ka <= kb {
                                EdgeKey(ka, kb)
                            } else {
                                EdgeKey(kb, ka)
                            };
                            if let Some(entries) = self.edges.get(&near) {
                                for e in entries {
                                    let close = |x: Vec3, y: Vec3| horizontal_distance(x, y) <= eps;
                                    if (close(a, e.1) && close(b, e.2))
                                        || (close(a, e.2) && close(b, e.1))
                                    {
                                        if !neighbors.iter().any(|x: &(u64, Vec3, Vec3)| x.0 == e.0)
                                        {
                                            neighbors.push(*e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let mut entries = neighbors.clone();
            for (neighbor_id, neighbor_a, neighbor_b) in neighbors {
                if neighbor_id == polygon_id {
                    continue;
                }
                let from = self.polygons[&polygon_id].centroid;
                let to = self.polygons[&neighbor_id].centroid;
                let (na, nb) =
                    if horizontal_distance(a, neighbor_a) <= horizontal_distance(a, neighbor_b) {
                        (neighbor_a, neighbor_b)
                    } else {
                        (neighbor_b, neighbor_a)
                    };
                let rise = (a[1] - na[1]).abs().max((b[1] - nb[1]).abs());
                if rise > self.config.max_step_height + eps
                    || (rise > eps && tri_area2(a, b, from) * tri_area2(a, b, to) >= 0.0)
                {
                    continue;
                }
                self.connect_base_portal(polygon_id, neighbor_id, a, b, rise);
                self.connect_base_portal(neighbor_id, polygon_id, neighbor_a, neighbor_b, rise);
            }
            self.edges.entry(key).or_default().push((polygon_id, a, b));
            // An internal triangulation diagonal is never a stair boundary.
            // Remove tentative step portals when the second coplanar face streams in.
            entries.push((polygon_id, a, b));
            let internal = entries
                .iter()
                .filter(|(id, x, y)| {
                    entries.iter().any(|(other, u, v)| {
                        id != other && ((x[1] + y[1]) - (u[1] + v[1])).abs() <= 2.0 * eps
                    })
                })
                .map(|e| e.0)
                .collect::<BTreeSet<_>>();
            for (id, _, _) in &entries {
                if let Some(poly) = self.polygons.get_mut(id) {
                    poly.portals.retain(|p| {
                        p.step_height <= eps
                            || edge_key(p.left, p.right, eps) != key
                            || !(internal.contains(id) && internal.contains(&p.to))
                    });
                }
            }
        }
    }

    fn connect_base_portal(
        &mut self,
        from_id: u64,
        to_id: u64,
        a: Vec3,
        b: Vec3,
        step_height: f32,
    ) {
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
                    step_height,
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
                    step_height: 0.0,
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
                        step_height: 0.0,
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
    [(v[0] / eps).round() as i64, 0, (v[2] / eps).round() as i64]
}

#[cfg(test)]
mod stair_tests {
    use super::*;
    fn tile(id: &str, z: f32, y: f32, run: f32) -> NavTileSource {
        NavTileSource {
            id: id.into(),
            vertices: vec![
                [-2.0, y, z],
                [2.0, y, z],
                [2.0, y, z + run],
                [-2.0, y, z + run],
            ],
            triangles: vec![[0, 2, 1], [0, 3, 2]],
        }
    }
    #[test]
    fn streamed_stair_edges_connect_both_ways_and_respect_agent_step_reach() {
        let mut nav = NavigationRuntime::default();
        nav.upsert_tile(tile("floor", -2.0, 0.0, 2.0)).unwrap();
        for i in 0..8 {
            nav.upsert_tile(tile(
                &format!("step{i}"),
                i as f32 * 0.32,
                (i + 1) as f32 * 0.18,
                0.32,
            ))
            .unwrap();
        }
        nav.upsert_tile(tile("landing", 2.56, 1.44, 2.0)).unwrap();
        let start = [0.0, 0.0, -1.0];
        let end = [0.0, 1.44, 4.0];
        assert_eq!(nav.solve_now(start, end, 0.3).status, PathStatus::Found);
        assert_eq!(nav.solve_now(end, start, 0.3).status, PathStatus::Found);
        let id = nav
            .request_character_path_with_step(start, end, 0.3, 1.8, BTreeSet::new(), 0.1)
            .unwrap();
        nav.pump(1);
        assert_eq!(nav.take_result(id).unwrap().status, PathStatus::NoPath);
        nav.remove_tile("step3");
        assert_eq!(nav.solve_now(start, end, 0.3).status, PathStatus::NoPath);
    }
    #[test]
    fn overlapping_floor_layers_do_not_gain_diagonal_stair_shortcuts() {
        let mut nav = NavigationRuntime::default();
        nav.upsert_tile(tile("lower", 0.0, 0.0, 4.0)).unwrap();
        nav.upsert_tile(tile("upper", 0.0, 0.25, 4.0)).unwrap();
        assert_eq!(
            nav.solve_now([0.0, 0.0, 2.0], [0.0, 0.25, 2.0], 0.2).status,
            PathStatus::NoPath
        );
    }
}

#[cfg(test)]
#[test]
fn bevel_gaps_weld_across_quantization_boundaries_and_negative_coordinates() {
    let mut nav = NavigationRuntime::default();
    for i in 0..10 {
        let z = -1.0 + i as f32 * 0.32;
        let y = i as f32 * 0.22;
        nav.upsert_tile(NavTileSource {
            id: format!("bevel{i}"),
            vertices: vec![
                [-1.0, y, z],
                [1.0, y, z],
                [1.0, y, z + 0.31],
                [-1.0, y, z + 0.31],
            ],
            triangles: vec![[0, 2, 1], [0, 3, 2]],
        })
        .unwrap();
    }
    assert_eq!(
        nav.solve_now([0.0, 0.0, -0.9], [0.0, 1.98, 1.95], 0.3)
            .status,
        PathStatus::Found
    );
}

#[cfg(test)]
#[test]
fn a_floor_continuing_beneath_a_stair_can_enter_its_boundary() {
    let mut nav = NavigationRuntime::default();
    for (id, y, z0, z1) in [
        ("approach", 0.0, -2.0, 0.0),
        ("under", 0.0, 0.0, 2.0),
        ("tread", 0.22, 0.0, 0.32),
    ] {
        nav.upsert_tile(NavTileSource {
            id: id.into(),
            vertices: vec![[-1.0, y, z0], [1.0, y, z0], [1.0, y, z1], [-1.0, y, z1]],
            triangles: vec![[0, 2, 1], [0, 3, 2]],
        })
        .unwrap();
    }
    assert_eq!(
        nav.solve_now([0.0, 0.0, -1.0], [0.0, 0.22, 0.16], 0.3)
            .status,
        PathStatus::Found
    );
}
