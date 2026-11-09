use super::*;
fn quad(x: f32, z: f32, w: f32, d: f32, y: f32) -> Vec<[V; 3]> {
    let a = [x, y, z];
    let b = [x + w, y, z];
    let c = [x + w, y, z + d];
    let e = [x, y, z + d];
    vec![[a, b, c], [a, c, e]]
}
fn sole() -> [[f32; 2]; 4] {
    [[-0.05, -0.08], [0.05, -0.08], [0.05, 0.18], [-0.05, 0.18]]
}
#[test]
fn valid_foot_stays_off_centre_and_internal_diagonal_is_not_a_border() {
    let s = SupportSurfaceIndex::new(&quad(0.0, 0.0, 3.0, 0.4, 1.0));
    assert_eq!(s.surfaces.len(), 1);
    assert_eq!(s.surfaces[0].boundary.len(), 4);
    assert_eq!(
        s.surfaces[0].fit_footprint([0.25, 0.1], sole(), 0.4),
        Some([0.25, 1.0, 0.1])
    );
}
#[test]
fn nearest_correction_fits_toe_and_heel_without_centring() {
    let s = SupportSurfaceIndex::new(&quad(0.0, 0.0, 3.0, 0.4, 1.0));
    let s = &s.surfaces[0];
    let p = s.fit_footprint([0.25, 0.29], sole(), 0.3).unwrap();
    assert!((p[0] - 0.25).abs() < 1e-5 && (p[2] - 0.22).abs() < 1e-5);
    assert!(s.contains_footprint(sole().map(|o| [p[0] + o[0], p[2] + o[1]])));
}
#[test]
fn short_tread_and_unreachable_target_are_rejected() {
    let s = SupportSurfaceIndex::new(&quad(0.0, 0.0, 3.0, 0.2, 1.0));
    assert!(s.surfaces[0]
        .fit_footprint([0.25, 0.1], sole(), 0.3)
        .is_none());
    let s = SupportSurfaceIndex::new(&quad(0.0, 0.0, 3.0, 0.4, 1.0));
    assert!(s.surfaces[0]
        .fit_footprint([0.25, 0.6], sole(), 0.1)
        .is_none());
}
#[test]
fn separated_and_stacked_treads_do_not_merge() {
    let mut t = quad(0.0, 0.0, 3.0, 0.4, 1.0);
    t.extend(quad(0.0, 0.41, 3.0, 0.4, 1.0));
    t.extend(quad(0.0, 0.0, 3.0, 0.4, 2.0));
    assert_eq!(SupportSurfaceIndex::new(&t).surfaces.len(), 3);
}
#[test]
fn concavity_and_hole_are_not_filled_by_a_convex_hull() {
    let mut t = quad(0.0, 0.0, 1.0, 0.4, 0.0);
    t.extend(quad(0.0, 0.4, 0.3, 0.6, 0.0));
    let s = SupportSurfaceIndex::new(&t);
    assert!(s.surfaces.iter().all(|s| !s.contains_footprint([
        [0.2, 0.2],
        [0.8, 0.2],
        [0.8, 0.8],
        [0.2, 0.8]
    ])));
    // A ring with a complete hole strictly inside the sole.
    let mut t = quad(0.0, 0.0, 1.0, 0.4, 0.0);
    t.extend(quad(0.0, 0.6, 1.0, 0.4, 0.0));
    t.extend(quad(0.0, 0.4, 0.4, 0.2, 0.0));
    t.extend(quad(0.6, 0.4, 0.4, 0.2, 0.0));
    let s = SupportSurfaceIndex::new(&t);
    assert!(s.surfaces.iter().all(|s| !s.contains_footprint([
        [0.1, 0.1],
        [0.9, 0.1],
        [0.9, 0.9],
        [0.1, 0.9]
    ])));
}
#[test]
fn rotated_scaled_and_mixed_winding_surfaces_preserve_fit() {
    for yaw in [0.0_f32, 0.3, 1.2, 3.0] {
        for scale in [0.6_f32, 1.0, 1.7] {
            let rot = |v: V| {
                [
                    (v[0] * yaw.cos() + v[2] * yaw.sin()) * scale,
                    v[1] * scale,
                    (-v[0] * yaw.sin() + v[2] * yaw.cos()) * scale,
                ]
            };
            let mut t = quad(0.0, 0.0, 3.0, 0.4, 1.0);
            t[1].swap(0, 1);
            t = t.into_iter().map(|t| t.map(rot)).collect();
            let s = SupportSurfaceIndex::new(&t);
            assert_eq!(s.surfaces.len(), 1);
            let d = rot([0.25, 0.29, 0.29]);
            let offsets = sole().map(|o| {
                let r = rot([o[0], 0.0, o[1]]);
                [r[0], r[2]]
            });
            let q = s.surfaces[0]
                .fit_footprint([d[0], d[2]], offsets, 0.3 * scale)
                .unwrap();
            let want = rot([0.25, 1.0, 0.22]);
            assert!(
                (0..3).all(|i| (q[i] - want[i]).abs() < 1e-4),
                "{q:?} {want:?}"
            );
        }
    }
}
#[test]
fn mesh_index_refit_retires_old_surface_height() {
    let v = [
        [0.0, 1.0, 0.0],
        [3.0, 1.0, 0.0],
        [3.0, 1.0, 0.4],
        [0.0, 1.0, 0.4],
    ];
    let mut mesh = crate::SphereSweepMesh::new(&v, &[[0, 1, 2], [0, 2, 3]]).unwrap();
    assert_eq!(
        mesh.support_surfaces_near([0.0, 0.9, 0.0], [3.0, 1.1, 0.4])
            .len(),
        1
    );
    mesh.refit_vertices(&v.map(|p| [p[0], 2.0, p[2]])).unwrap();
    assert!(mesh
        .support_surfaces_near([0.0, 0.9, 0.0], [3.0, 1.1, 0.4])
        .is_empty());
    assert_eq!(
        mesh.support_surfaces_near([0.0, 1.9, 0.0], [3.0, 2.1, 0.4])[0].height(1.0, 0.2),
        2.0
    );
}

#[test]
fn hole_inside_one_connected_triangulated_region_is_rejected() {
    let mut t = Vec::new();
    for x in 0..5 {
        for z in 0..5 {
            if x != 2 || z != 2 {
                t.extend(quad(x as f32, z as f32, 1.0, 1.0, 0.0));
            }
        }
    }
    let s = SupportSurfaceIndex::new(&t);
    assert_eq!(s.surfaces.len(), 1);
    assert!(!s.surfaces[0].contains_footprint([[0.5, 0.5], [4.5, 0.5], [4.5, 4.5], [0.5, 4.5]]));
    assert!(s.surfaces[0].contains_footprint([[0.2, 0.2], [1.8, 0.2], [1.8, 1.8], [0.2, 1.8]]));
}
#[test]
#[ignore = "CPU support-surface benchmark; opt in with --ignored --nocapture"]
fn benchmark_large_floor_support_queries() {
    let mut t = Vec::new();
    for x in 0..100 {
        for z in 0..100 {
            t.extend(quad(x as f32, z as f32, 1.0, 1.0, 0.0));
        }
    }
    let start = std::time::Instant::now();
    let s = SupportSurfaceIndex::new(&t);
    let build = start.elapsed();
    let start = std::time::Instant::now();
    for i in 0..10000 {
        let q = [0.3 + (i % 99) as f32, 0.2 + ((i / 99) % 99) as f32];
        assert!(s.surfaces[0].fit_footprint(q, sole(), 0.3).is_some());
    }
    eprintln!(
        "SUPPORT_BENCH triangles={} build_ms={} queries=10000 query_ms={}",
        t.len(),
        build.as_secs_f64() * 1000.0,
        start.elapsed().as_secs_f64() * 1000.0
    );
}
