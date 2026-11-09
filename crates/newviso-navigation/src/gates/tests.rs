use super::*;
fn floor(id: &str, x: f32, z: f32) -> NavTileSource {
    NavTileSource {
        id: id.into(),
        vertices: vec![
            [x, 0.0, z],
            [x + 2.0, 0.0, z],
            [x + 2.0, 0.0, z + 2.0],
            [x, 0.0, z + 2.0],
        ],
        triangles: vec![[0, 2, 1], [0, 3, 2]],
    }
}
fn gate() -> TraversalGate {
    TraversalGate {
        id: "office".into(),
        center: [2.0, 1.0, 1.0],
        normal: [1.0, 0.0, 0.0],
        half_width: 1.0,
        half_height: 1.1,
        locked: true,
        free_egress_side: None,
        blocked: false,
        action_cost: 2.0,
    }
}
#[test]
fn locked_gate_uses_real_direction_access_and_rechecks_smoothed_paths() {
    let mut n = NavigationRuntime::default();
    n.upsert_tile(floor("a", 0.0, 0.0)).unwrap();
    n.upsert_tile(floor("b", 2.0, 0.0)).unwrap();
    n.set_traversal_gates(vec![gate()]).unwrap();
    assert_eq!(
        n.solve_now([0.25, 0.1, 0.5], [3.75, 0.1, 0.5], 0.2).status,
        PathStatus::NoPath
    );
    let id = n
        .request_path_with_access(
            [0.25, 0.1, 0.5],
            [3.75, 0.1, 0.5],
            0.2,
            BTreeSet::from(["office".into()]),
        )
        .unwrap();
    n.pump(1);
    assert_eq!(n.take_result(id).unwrap().status, PathStatus::Found);
    let mut g = gate();
    g.free_egress_side = Some(-1);
    n.set_traversal_gates(vec![g]).unwrap();
    assert_eq!(
        n.solve_now([0.25, 0.1, 0.5], [3.75, 0.1, 0.5], 0.2).status,
        PathStatus::Found
    );
    assert_eq!(
        n.solve_now([3.75, 0.1, 0.5], [0.25, 0.1, 0.5], 0.2).status,
        PathStatus::NoPath
    );
}
#[test]
fn a_locked_shortcut_selects_the_open_alternative_and_cannot_funnel_through_the_lock() {
    let mut n = NavigationRuntime::default();
    for (id, x, z) in [
        ("a", 0.0, 0.0),
        ("b", 2.0, 0.0),
        ("c", 0.0, 2.0),
        ("d", 2.0, 2.0),
    ] {
        n.upsert_tile(floor(id, x, z)).unwrap();
    }
    n.set_traversal_gates(vec![gate()]).unwrap();
    let path = n.solve_now([0.25, 0.1, 0.5], [3.75, 0.1, 0.5], 0.2);
    assert_eq!(path.status, PathStatus::Found);
    assert!(path.waypoints.iter().any(|p| p[2] > 2.2));
    assert!(path
        .waypoints
        .windows(2)
        .all(|p| gate().crossing(p[0], p[1], 0.2).is_none()));
}
#[test]
fn equivalent_gate_updates_do_not_invalidate_every_path_each_frame() {
    let mut n = NavigationRuntime::default();
    assert!(n.set_traversal_gates(vec![gate()]).unwrap());
    let revision = n.revision();
    for _ in 0..100 {
        assert!(!n.set_traversal_gates(vec![gate()]).unwrap());
    }
    assert_eq!(n.revision(), revision);
    assert!(n
        .set_traversal_gates(vec![TraversalGate {
            half_width: f32::NAN,
            ..gate()
        }])
        .is_err());
    assert_eq!(n.revision(), revision);
}
