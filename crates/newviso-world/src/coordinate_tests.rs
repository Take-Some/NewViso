use super::*;

fn world() -> LivingWorldRuntime {
    let mut w = LivingWorldRuntime::default();
    w.configure_simulation(WorldSimulationPolicyDesc { background_interval_seconds: 0.05,
        reduced_interval_seconds: 0.05, full_interval_seconds: 0.05, ..WorldSimulationPolicyDesc::default() }).unwrap();
    w.upsert_actor(WorldActorDesc { id: "ped".into(), kind: "pedestrian".into(), position: [0.0; 3],
        group: None, channel: None, enabled: true, tags: vec![], parameters: BTreeMap::new(), state: Value::Null }).unwrap();
    w
}

#[test]
fn coordinate_travel_needs_no_graph_and_completes_once() {
    let mut w = world(); w.start_travel_to_position("ped", "goto", [5.0, 0.0, 0.0], 2.5, 0.18).unwrap();
    assert_eq!(w.actor_runtime_views()[0].travel_target_position, Some([5.0, 0.0, 0.0]));
    let mut count = 0;
    for _ in 0..60 { w.tick_frame(0.05, &[]); count += w.frame_travel_completions.len(); }
    assert_eq!(count, 1); assert!((w.actor_runtime_views()[0].position[0] - 5.0).abs() <= 0.18);
}

#[test]
fn coordinate_travel_roundtrips_checkpoint_and_keeps_physical_authority() {
    let mut w = world(); w.start_travel_to_position("ped", "goto", [5.0, 0.0, 0.0], 2.5, 0.18).unwrap();
    let text = w.checkpoint().unwrap();
    let mut restored = LivingWorldRuntime::default(); restored.restore_checkpoint(text).unwrap();
    assert_eq!(restored.actor_runtime_views()[0].travel_target_position, Some([5.0, 0.0, 0.0]));
    restored.set_actor_external_motion("ped", [1.0, 0.0, 0.0], [0.0; 3]).unwrap();
    restored.tick_frame(0.5, &[]); assert_eq!(restored.actor_runtime_views()[0].position, [1.0, 0.0, 0.0]);
    restored.release_actor_external_motion("ped"); restored.tick_frame(0.5, &[]);
    assert!(restored.actor_runtime_views()[0].position[0] > 1.0);
}
