use super::*;
use newviso_collision::sweep_sphere_aabb_hit;

struct World(Vec<(u64, Vec3, Vec3)>);
impl CharacterCollisionWorld for World {
    fn sweep_sphere(&self, origin: Vec3, delta: Vec3, radius: f32) -> Option<SweepHit> {
        self.0
            .iter()
            .filter_map(|(id, min, max)| {
                sweep_sphere_aabb_hit(origin, delta, radius, *min, *max).map(|h| SweepHit {
                    fraction: h.fraction,
                    normal: h.normal,
                    entity: Some(*id),
                })
            })
            .min_by(|a, b| a.fraction.total_cmp(&b.fraction))
    }
}
fn staircase(rise: f32, run: f32, count: usize) -> World {
    let mut boxes = vec![(1, [-4.0, -1.0, -2.0], [8.0, 0.0, 2.0])];
    for n in 0..count {
        boxes.push((
            n as u64 + 2,
            [1.0 + n as f32 * run, 0.0, -2.0],
            [1.0 + (n + 1) as f32 * run, (n + 1) as f32 * rise, 2.0],
        ));
    }
    boxes.push((
        99,
        [1.0 + count as f32 * run, 0.0, -2.0],
        [8.0, count as f32 * rise, 2.0],
    ));
    World(boxes)
}
fn walk(
    world: &impl CharacterCollisionWorld,
    cfg: CharacterConfig,
    start: Vec3,
    speed: f32,
    dt: f32,
    seconds: f32,
) -> (Vec3, f32) {
    let mut stairs = StairController::default();
    let controller = CharacterController::new(cfg).unwrap();
    let mut p = start;
    let mut grounded = true;
    let mut largest: f32 = 0.0;
    for _ in 0..(seconds / dt) as usize {
        let before = p[1];
        if let Some(a) = stairs.update(
            world,
            cfg,
            p,
            [speed, 0.0, 0.0],
            [speed, 0.0, 0.0],
            grounded,
            dt,
        ) {
            assert!(
                a.normal[1] > 0.99,
                "support must use a horizontal tread plane, not a rounded contact: {:?}",
                a.normal
            );
            p[1] = a.y;
            grounded = true;
        } else {
            grounded = false;
        }
        let d = [speed * dt, 0.0, 0.0];
        let h = controller.sweep_capsule(world, p, d);
        let f = h.map_or(1.0, |h| {
            if dot(d, h.normal) < -1e-6 {
                h.fraction
            } else {
                1.0
            }
        });
        p = add(p, mul(d, f));
        largest = largest.max((p[1] - before).abs());
    }
    (p, largest)
}
#[test]
fn climbs_each_tread_at_multiple_rates_and_capsule_sizes() {
    for dt in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 120.0] {
        for (r, hh) in [(0.32, 0.58), (0.23, 0.45)] {
            let cfg = CharacterConfig {
                radius: r,
                half_height: hh,
                ..CharacterConfig::default()
            };
            let (p, change) = walk(
                &staircase(0.18, 0.32, 8),
                cfg,
                [0.0, r + hh - 0.00001, 0.0],
                1.5,
                dt,
                9.0,
            );
            assert!(p[0] > 4.0, "stalled: {dt} {r} {p:?}");
            assert!((p[1] - (r + hh + 8.0 * 0.18)).abs() < 0.04, "height {p:?}");
            assert!(
                change <= cfg.step_speed * dt + 1e-5,
                "unbounded lift {change}"
            );
        }
    }
}
#[test]
fn descends_without_airborne_launch_or_full_height_snaps() {
    let cfg = CharacterConfig::default();
    let (p, change) = walk(
        &staircase(0.18, 0.32, 8),
        cfg,
        [4.5, 0.9 + 8.0 * 0.18 + 0.002, 0.0],
        -1.0,
        1.0 / 120.0,
        6.0,
    );
    assert!(p[0] < 0.5, "{p:?}");
    assert!((p[1] - 0.9).abs() < 0.04, "{p:?}");
    assert!(change <= cfg.ground_snap_speed / 120.0 + 1e-5, "{change}");
}
#[test]
fn rejects_high_wall_and_low_headroom_without_levitating() {
    let cfg = CharacterConfig::default();
    let wall = World(vec![
        (1, [-4.0, -1.0, -2.0], [8.0, 0.0, 2.0]),
        (2, [1.0, 0.0, -2.0], [2.0, 2.0, 2.0]),
    ]);
    let (p, _) = walk(&wall, cfg, [0.0, 0.902, 0.0], 1.5, 1.0 / 120.0, 5.0);
    assert!(p[0] < 0.7 && p[1] < 0.94, "wall climb {p:?}");
    let mut low = staircase(0.22, 0.4, 1);
    low.0.push((300, [-1.0, 1.88, -2.0], [2.5, 2.4, 2.0]));
    let (p, _) = walk(&low, cfg, [0.0, 0.902, 0.0], 1.5, 1.0 / 120.0, 5.0);
    assert!(p[0] < 0.7 && p[1] < 1.0, "ceiling penetrated {p:?}");
}
#[test]
fn jump_freefall_and_a_missing_tread_cancel_assistance() {
    let cfg = CharacterConfig::default();
    let world = staircase(0.18, 0.32, 2);
    let mut stairs = StairController::default();
    assert!(stairs
        .update(
            &world,
            cfg,
            [0.6, 0.902, 0.0],
            [1.0, 5.0, 0.0],
            [1.0, 0.0, 0.0],
            true,
            1.0 / 120.0
        )
        .is_none());
    assert!(stairs
        .update(
            &world,
            cfg,
            [0.6, 1.1, 0.0],
            [1.0, -3.0, 0.0],
            [1.0, 0.0, 0.0],
            false,
            1.0 / 120.0
        )
        .is_none());
    let mut active = StairController::default();
    assert!(active
        .update(
            &world,
            cfg,
            [0.6, 0.902, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            true,
            1.0 / 120.0
        )
        .is_some());
    assert!(active
        .update(
            &World(vec![]),
            cfg,
            [0.6, 0.91, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            false,
            1.0 / 120.0
        )
        .is_none());
}
#[test]
fn native_contact_lift_within_the_step_budget_keeps_support() {
    let cfg = CharacterConfig::default();
    let world = staircase(0.18, 0.32, 2);
    let mut stairs = StairController::default();
    assert!(stairs
        .update(
            &world,
            cfg,
            [0.0, 0.902, 0.0],
            [1.0, 0.37, 0.0],
            [1.0, 0.0, 0.0],
            true,
            1.0 / 120.0
        )
        .is_some());
    assert!(stairs
        .update(
            &world,
            cfg,
            [0.0, 0.902, 0.0],
            [1.0, cfg.step_speed + 0.01, 0.0],
            [1.0, 0.0, 0.0],
            true,
            1.0 / 120.0
        )
        .is_none());
}
#[test]
fn a_new_airborne_body_does_not_snap_to_a_lower_landing() {
    let cfg = CharacterConfig::default();
    let world = staircase(0.18, 0.32, 2);
    let mut stairs = StairController::default();
    assert!(stairs
        .update(
            &world,
            cfg,
            [0.0, 1.04, 0.0],
            [1.0, -0.4, 0.0],
            [1.0, 0.0, 0.0],
            false,
            1.0 / 120.0
        )
        .is_none());
}

#[test]
fn lateral_external_displacement_releases_the_old_tread() {
    let cfg = CharacterConfig::default();
    let world = staircase(0.18, 0.32, 2);
    let mut stairs = StairController::default();
    assert!(stairs
        .update(
            &world,
            cfg,
            [0.6, 0.902, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            true,
            1.0 / 120.0
        )
        .is_some());
    assert!(stairs
        .update(
            &world,
            cfg,
            [20.0, 1.0, 0.0],
            [8.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            false,
            1.0 / 120.0
        )
        .is_none());
}

#[test]
fn triangle_nosings_descend_to_each_tread_without_losing_the_lower_support() {
    struct Triangles(newviso_collision::SphereSweepMesh);
    impl CharacterCollisionWorld for Triangles {
        fn sweep_sphere(&self, o: Vec3, d: Vec3, r: f32) -> Option<SweepHit> {
            self.0.sweep_hit(o, d, r).map(|h| SweepHit {
                fraction: h.fraction,
                normal: h.normal,
                entity: Some(1),
            })
        }
    }
    let world = staircase(0.22, 0.32, 8);
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for (_, min, max) in world.0 {
        let base = vertices.len() as u32;
        for x in [min[0], max[0]] {
            for y in [min[1], max[1]] {
                for z in [min[2], max[2]] {
                    vertices.push([x, y, z]);
                }
            }
        }
        for [a, b, c] in [
            [0, 1, 3],
            [0, 3, 2],
            [4, 6, 7],
            [4, 7, 5],
            [0, 4, 5],
            [0, 5, 1],
            [2, 3, 7],
            [2, 7, 6],
            [0, 2, 6],
            [0, 6, 4],
            [1, 5, 7],
            [1, 7, 3],
        ] {
            triangles.push([base + a, base + b, base + c]);
        }
    }
    let world = Triangles(newviso_collision::SphereSweepMesh::new(&vertices, &triangles).unwrap());
    let cfg = CharacterConfig::default();
    let (p, change) = walk(
        &world,
        cfg,
        [4.5, 0.9 + 8.0 * 0.22 + 0.002, 0.0],
        -1.0,
        1.0 / 120.0,
        6.0,
    );
    assert!(
        p[0] < 0.5 && (p[1] - 0.9).abs() < 0.04,
        "triangle stair descent stalled/floated: {p:?}"
    );
    assert!(
        change <= cfg.ground_snap_speed / 120.0 + 1e-5,
        "unbounded descent {change}"
    );
}
