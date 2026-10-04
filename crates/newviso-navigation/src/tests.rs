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

fn brute_force_closest(nav: &NavigationRuntime, point: Vec3, radius: f32) -> Option<u64> {
    nav.polygons
        .values()
        .filter(|poly| {
            !nav.obstacles.values().any(|obstacle| {
                obstacle.enabled
                    && horizontal_distance(obstacle.center, poly.centroid)
                        < obstacle.radius + radius
            })
        })
        .map(|poly| {
            (
                poly.id,
                distance(point, closest_point_on_triangle(point, poly.vertices)),
            )
        })
        .filter(|(_, distance)| *distance <= nav.config.max_snap_distance)
        .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
        .map(|(id, _)| id)
}

#[test]
fn spatial_snap_matches_full_scan_with_negative_coordinates_floors_and_obstacles() {
    let mut nav = NavigationRuntime::default();
    for i in 0..64 {
        let mut tile = floor_tile(&format!("tile-{i}"), (i % 8) as f32 * 16.0 - 64.0);
        for vertex in &mut tile.vertices {
            vertex[1] = (i % 3) as f32 * 2.0;
            vertex[2] += (i / 8) as f32 * 16.0 - 64.0;
        }
        nav.upsert_tile(tile).unwrap();
    }
    nav.upsert_tile(floor_tile("tie", 0.0)).unwrap();
    nav.upsert_tile(floor_tile("tie-copy", 0.0)).unwrap();
    nav.upsert_obstacle(DynamicObstacle {
        id: "blocker".into(),
        center: [1.0, 0.0, 1.0],
        radius: 0.8,
        enabled: true,
    })
    .unwrap();
    for snap in [0.01, 4.0, 20.0, 1000.0] {
        nav.set_config(NavBuildConfig {
            max_snap_distance: snap,
            ..nav.config()
        })
        .unwrap();
        for i in 0..300 {
            let point = [
                ((i * 47) % 151) as f32 - 70.0,
                (i % 7) as f32 - 1.0,
                ((i * 31) % 151) as f32 - 70.0,
            ];
            assert_eq!(
                nav.closest_polygon(point, 0.25),
                brute_force_closest(&nav, point, 0.25),
                "point={point:?} snap={snap}"
            );
        }
        assert_eq!(
            nav.closest_polygon([0.5, 0.0, 0.5], 0.0),
            brute_force_closest(&nav, [0.5, 0.0, 0.5], 0.0)
        );
    }
}

#[test]
fn oversized_polygon_snaps_near_edge_and_removal_clears_index() {
    let mut nav = NavigationRuntime::default();
    nav.upsert_tile(NavTileSource {
        id: "large".into(),
        vertices: vec![
            [-4096.0, 0.0, -4096.0],
            [4096.0, 0.0, -4096.0],
            [4096.0, 0.0, 4096.0],
        ],
        triangles: vec![[0, 2, 1]],
    })
    .unwrap();
    let point = [4095.0, 0.1, 4094.0];
    assert_eq!(nav.solve_now(point, point, 0.25).status, PathStatus::Found);
    assert_eq!(nav.runtime_state()["oversized_polygons"], 1);
    assert!(nav.remove_tile("large"));
    assert_eq!(
        nav.solve_now(point, point, 0.25).status,
        PathStatus::NoStartPolygon
    );
    assert_eq!(nav.runtime_state()["oversized_polygons"], 0);
}

#[test]
fn partial_tile_replacement_does_not_leave_stale_spatial_entries() {
    let mut nav = NavigationRuntime::default();
    let mut tile = floor_tile("streamed", -16.0);
    tile.triangles = vec![[0, 2, 1]; 4];
    nav.queue_tile(tile).unwrap();
    nav.pump_tile_build(1, 0);
    assert_eq!(
        nav.solve_now([-15.0, 0.0, 0.2], [-15.0, 0.0, 0.2], 0.0)
            .status,
        PathStatus::Found
    );
    nav.queue_tile(floor_tile("streamed", 100.0)).unwrap();
    assert_eq!(nav.runtime_state()["spatial_cells"], 0);
    assert_eq!(
        nav.solve_now([-15.0, 0.0, 0.2], [-15.0, 0.0, 0.2], 0.0)
            .status,
        PathStatus::NoStartPolygon
    );
    nav.pump_tile_build(10, 0);
    assert_eq!(
        nav.solve_now([101.0, 0.0, 0.2], [101.0, 0.0, 0.2], 0.0)
            .status,
        PathStatus::Found
    );
}

#[test]
fn nearby_snap_candidates_do_not_grow_with_distant_tiles() {
    let mut nav = NavigationRuntime::default();
    for i in 0..1024 {
        nav.upsert_tile(floor_tile(&format!("tile-{i}"), i as f32 * 32.0))
            .unwrap();
    }
    let point = [1.0, 0.1, 1.0];
    assert!(
        nav.spatial_index
            .candidates(point, nav.config.max_snap_distance)
            .unwrap()
            .count()
            <= 4
    );
    assert_eq!(
        nav.closest_polygon(point, 0.25),
        brute_force_closest(&nav, point, 0.25)
    );
    assert!(nav.remove_tile("tile-0"));
    assert_eq!(nav.closest_polygon(point, 0.25), None);
}
