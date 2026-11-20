//! Opt-in deterministic CPU benchmark of scene indexing, process scheduling and visibility.
//! This is not a Vulkan/GPU, AI decision-making or vehicle dynamics benchmark.
use super::*;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn actor(id: u64, slot: usize, position: Vec3) -> SceneEntity {
    SceneEntity {
        id: SceneEntityId(id),
        name: format!("load-{id}"),
        kind: SceneEntityKind::DynamicMesh,
        mobility: SceneMobility::Dynamic,
        lifecycle: SceneLifecycle::Constructed,
        transform: SceneTransform {
            position,
            rotation_degrees: Vec3::ZERO,
            scale: Vec3::ONE,
        },
        light: None,
        bounds: SceneBounds::from_center_half_extent(position, Vec3::new(0.8, 1.0, 1.6)),
        parent: None,
        children: Vec::new(),
        visibility: VisibilityMask::default(),
        lod: SceneLodPolicy::default(),
        solid: false,
        collision_local_bounds: None,
        destructible: None,
        asset_ref: None,
        resident_geometry: false,
        render_slot: Some(slot),
        residency: SceneResidency::Resident,
        priority_score: 0.0,
        lod_alpha: 1.0,
        last_visible_frame: None,
        revision: 0,
        last_mutation_frame: 0,
        process_claims: SceneProcessClaims::default(),
        last_process_frame: None,
    }
}

fn percentile(samples: &[Duration], numerator: usize, denominator: usize) -> f64 {
    let mut ordered = samples.to_vec();
    ordered.sort_unstable();
    let index = ((ordered.len() - 1) * numerator) / denominator;
    ordered[index].as_secs_f64() * 1000.0
}

fn profile(npcs: usize, vehicles: usize, static_props: usize) {
    let mut world = SceneWorld::new(Vec3::ZERO);
    let mut mobile = Vec::with_capacity(npcs + vehicles);
    let total = npcs + vehicles + static_props;
    for slot in 0..total {
        let position = Vec3::new(
            (slot % 80) as f32 * 6.0 - 240.0,
            0.0,
            -((slot / 80) as f32 * 6.0 + 15.0),
        );
        let mut e = actor(10_000 + slot as u64, slot, position);
        if slot >= npcs + vehicles {
            e.mobility = SceneMobility::Static;
            e.kind = SceneEntityKind::StaticMesh;
        } else {
            mobile.push((e.id, position));
        }
        world.add_entity(e).expect("unique scene entity");
    }
    world.activate_all();
    for (i, (id, _)) in mobile.iter().enumerate() {
        let (reason, owner, cadence) = if i < npcs {
            ("intelligence", "benchmark.npc", 10.0)
        } else {
            ("mover", "benchmark.vehicle", 20.0)
        };
        world
            .set_process_claim(*id, owner, reason, true, SceneMutationSource::Engine)
            .unwrap();
        world.set_process_rate_hz(*id, reason, cadence).unwrap();
    }
    world.seal_initial_state();

    let view = SceneView {
        position: Vec3::ZERO,
        forward: Vec3::new(0.0, 0.0, -1.0),
        up: Vec3::Y,
        near: 0.1,
        far: 500.0,
        fov_y_radians: 70.0_f32.to_radians(),
        aspect: 16.0 / 9.0,
    };
    const FRAMES: usize = 150;
    let mut movement = Vec::with_capacity(FRAMES);
    let mut process = Vec::with_capacity(FRAMES);
    let mut visibility = Vec::with_capacity(FRAMES);
    let mut visible_peak = 0;
    let mut due_peak = 0;
    let mut candidate_peak = 0;

    for frame in 0..FRAMES {
        let start = Instant::now();
        for (id, origin) in mobile.iter().skip(frame % 12).step_by(12) {
            let position = Vec3::new(origin.x + (frame as f32 * 0.005), origin.y, origin.z);
            world
                .update_spatial_from(
                    *id,
                    SceneTransform {
                        position,
                        rotation_degrees: Vec3::ZERO,
                        scale: Vec3::ONE,
                    },
                    SceneBounds::from_center_half_extent(position, Vec3::new(0.8, 1.0, 1.6)),
                    SceneMutationSource::Physics,
                )
                .unwrap();
        }
        black_box(world.drain_mutations());
        movement.push(start.elapsed());

        let start = Instant::now();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        world.update();
        due_peak = due_peak.max(world.process_due_count());
        process.push(start.elapsed());

        let start = Instant::now();
        let plan = world.scan_visibility(view);
        visible_peak = visible_peak.max(plan.visible_count);
        candidate_peak = candidate_peak.max(plan.spatial_candidate_count);
        black_box(plan);
        visibility.push(start.elapsed());
    }
    assert!(visible_peak > 0);
    assert!(due_peak > 0);
    println!(
        "scene_cpu_load npc={npcs} vehicles={vehicles} static={static_props} frames={FRAMES}          move_p50_ms={:.4} move_p95_ms={:.4}          process_p50_ms={:.4} process_p95_ms={:.4}          visibility_p50_ms={:.4} visibility_p95_ms={:.4}          due_peak={due_peak} visible_peak={visible_peak} candidates_peak={candidate_peak}",
        percentile(&movement, 50, 100), percentile(&movement, 95, 100),
        percentile(&process, 50, 100), percentile(&process, 95, 100),
        percentile(&visibility, 50, 100), percentile(&visibility, 95, 100)
    );
}

#[test]
#[ignore = "manual scene CPU load benchmark; cargo test -p newviso-scene scene_cpu_load -- --ignored --nocapture"]
fn scene_cpu_load() {
    profile(300, 100, 1_000);
    profile(1_500, 400, 3_000);
    profile(3_000, 1_000, 8_000);
}
