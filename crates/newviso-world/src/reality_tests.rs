use super::*;

fn actor(id: &str) -> WorldActorDesc {
    WorldActorDesc {
        id: id.into(),
        kind: "generic".into(),
        position: [10_000.0, 0.0, 0.0],
        group: None,
        channel: None,
        enabled: true,
        tags: vec![],
        parameters: BTreeMap::new(),
        state: Value::Null,
    }
}

fn event(id: &str, cause: Option<&str>) -> WorldScheduledEventDesc {
    WorldScheduledEventDesc {
        id: id.into(),
        kind: "generic.delivery".into(),
        source: "test".into(),
        cause: cause.map(str::to_owned),
        delay_seconds: 0.1,
        ttl_seconds: 10.0,
        priority: 0,
        position: None,
        tags: vec![],
        payload: json!({"units": 3}),
    }
}

fn reservation(id: &str, exclusive: bool) -> WorldScenarioReservationDesc {
    WorldScenarioReservationDesc {
        id: id.into(),
        scenario_point_id: "point".into(),
        actor_id: "actor".into(),
        delay_seconds: 0.0,
        duration_seconds: 10.0,
        priority: 0,
        exclusive,
        payload: Value::Null,
    }
}

fn reservation_world() -> LivingWorldRuntime {
    let mut world = LivingWorldRuntime::default();
    world.upsert_actor(actor("actor")).unwrap();
    world
        .upsert_scenario_point(ScenarioPointDesc {
            id: "point".into(),
            kind: "generic".into(),
            group: None,
            position: [0.0; 3],
            heading_degrees: 0.0,
            radius: 1.0,
            probability: 1.0,
            model_set: None,
            enabled: true,
            tags: vec![],
            parameters: BTreeMap::new(),
        })
        .unwrap();
    world
}

#[test]
fn script_mutation_reaches_next_frame_exactly_once_even_without_fixed_step() {
    let mut world = LivingWorldRuntime::default();
    world.tick_frame(0.1, &[]);
    world.set_fact("value", json!(7)).unwrap();
    assert_eq!(world.runtime_state()["reality"]["frame_events"], json!([]));
    world.tick_frame(0.0, &[]);
    let state = world.runtime_state();
    assert_eq!(state["clock"]["last_frame_steps"], 0);
    assert_eq!(
        state["reality"]["frame_events"].as_array().unwrap().len(),
        1
    );
    assert_eq!(state["reality"]["frame_events"][0]["payload"]["value"], 7);
    world.tick_frame(0.0, &[]);
    assert_eq!(world.runtime_state()["reality"]["frame_events"], json!([]));
    assert_eq!(
        world.runtime_state()["reality"]["history"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn automatic_reality_ids_are_nonempty_unique_and_do_not_collide_with_explicit_ids() {
    let mut world = LivingWorldRuntime::default();
    world
        .record_reality_event(WorldRealityEventDesc {
            id: "reality.event.0000000000000001".into(),
            kind: "generic".into(),
            source: "test".into(),
            cause: None,
            participants: vec![],
            position: None,
            importance: 1.0,
            tags: vec![],
            payload: Value::Null,
        })
        .unwrap();
    world.set_fact("value", json!(1)).unwrap();
    world.schedule_event(event("delivery", None)).unwrap();
    world.tick_frame(0.2, &[]);
    let state = world.runtime_state();
    let history = state["reality"]["history"].as_array().unwrap();
    let ids = history
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), history.len());
    assert!(ids.iter().all(|id| !id.is_empty()));
    assert!(history
        .windows(2)
        .all(|pair| pair[0]["sequence"].as_u64() < pair[1]["sequence"].as_u64()));
}

#[test]
fn scheduled_cause_and_fact_consequence_form_a_chain_without_observers() {
    let mut world = LivingWorldRuntime::default();
    world.set_fact("stock", json!(0)).unwrap();
    let production = world
        .record_reality_event(WorldRealityEventDesc {
            id: "production".into(),
            kind: "generic.production".into(),
            source: "test".into(),
            cause: None,
            participants: vec![],
            position: None,
            importance: 1.0,
            tags: vec![],
            payload: json!({"units": 3}),
        })
        .unwrap();
    world
        .schedule_event(event("delivery", Some(&production)))
        .unwrap();
    world.tick_frame(0.2, &[]);
    let snapshot = world.runtime_state();
    let receipt_id = snapshot["events"]["frame_due"][0]["reality_event_id"]
        .as_str()
        .unwrap();
    let receipt = snapshot["reality"]["history"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == receipt_id)
        .unwrap();
    assert_eq!(receipt["cause"], production);
    world
        .set_fact_with_cause("stock", json!(3), Some(receipt_id.into()))
        .unwrap();
    world.tick_frame(0.01, &[]);
    let next = world.runtime_state();
    let consequence = &next["reality"]["frame_events"][0];
    assert_eq!(consequence["cause"], receipt_id);
    assert_eq!(consequence["payload"]["previous_value"], 0);
    assert_eq!(consequence["payload"]["value"], 3);
    assert_eq!(next["facts"][0]["revision"], 2);
    assert_eq!(next["simulation"]["observers"], json!([]));
    assert_eq!(next["events"]["frame_due"], json!([]));
}

#[test]
fn fact_removal_is_a_recorded_change_and_noop_removal_is_not() {
    let mut world = LivingWorldRuntime::default();
    world.set_fact("value", json!(7)).unwrap();
    world.tick_frame(0.0, &[]);
    world.remove_fact("value");
    world.remove_fact("value");
    world.tick_frame(0.0, &[]);
    let state = world.runtime_state();
    assert_eq!(state["facts"], json!([]));
    assert_eq!(
        state["reality"]["frame_events"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        state["reality"]["frame_events"][0]["kind"],
        "world.fact.removed"
    );
    assert_eq!(
        state["reality"]["frame_events"][0]["payload"]["previous_value"],
        7
    );
}

#[test]
fn exclusive_reservation_blocks_both_shared_and_exclusive_requests() {
    for first_exclusive in [true, false] {
        let mut world = reservation_world();
        world
            .reserve_scenario(reservation("first", first_exclusive))
            .unwrap();
        assert!(world
            .reserve_scenario(reservation("second", !first_exclusive))
            .is_err());
        assert_eq!(
            world.runtime_state()["scenario_reservations"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn updating_own_reservation_and_removing_owners_preserve_consistency() {
    let mut world = reservation_world();
    world.reserve_scenario(reservation("first", true)).unwrap();
    let mut replacement = reservation("first", true);
    replacement.duration_seconds = 20.0;
    world.reserve_scenario(replacement).unwrap();
    assert_eq!(
        world.runtime_state()["scenario_reservations"][0]["ends_world_seconds"],
        20.0
    );
    world.remove_actor("actor");
    assert_eq!(world.runtime_state()["scenario_reservations"], json!([]));
    world.upsert_actor(actor("actor")).unwrap();
    world.reserve_scenario(reservation("second", true)).unwrap();
    world.remove_scenario_point("point");
    assert_eq!(world.runtime_state()["scenario_reservations"], json!([]));
}

#[test]
fn backlog_can_be_drained_without_adding_more_elapsed_time() {
    let mut world = LivingWorldRuntime::default();
    world
        .configure_clock(WorldClockPolicyDesc {
            fixed_hz: 20.0,
            time_scale: 1.0,
            max_steps_per_frame: 2,
        })
        .unwrap();
    world.tick_frame(1.0, &[]);
    for _ in 0..12 {
        world.tick_frame(0.0, &[]);
    }
    let state = world.runtime_state();
    assert!((state["clock"]["world_seconds"].as_f64().unwrap() - 1.0).abs() < 1e-9);
    assert!(state["clock"]["backlog_seconds"].as_f64().unwrap().abs() < 1e-9);
}

#[test]
fn actor_budget_does_not_starve_background_world() {
    let mut world = LivingWorldRuntime::default();
    world
        .configure_simulation(WorldSimulationPolicyDesc {
            max_actor_updates_per_step: 1,
            ..WorldSimulationPolicyDesc::default()
        })
        .unwrap();
    for id in ["a", "b", "c", "d"] {
        world.upsert_actor(actor(id)).unwrap();
    }
    for _ in 0..10 {
        world.tick_frame(0.1, &[]);
    }
    let snapshot = world.runtime_state();
    for actor in snapshot["simulation"]["actors"].as_array().unwrap() {
        assert_eq!(actor["simulation_tier"], "background");
        assert!(actor["last_update_world_seconds"].as_f64().unwrap() > 0.0);
    }
}

#[test]
fn checkpoint_roundtrip_preserves_pending_work_and_reservations() {
    let mut world = reservation_world();
    world
        .reserve_scenario(reservation("reserved", true))
        .unwrap();
    world
        .upsert_relationship(RelationshipRuleDesc {
            source_group: "a".into(),
            target_group: "b".into(),
            relation: "neutral".into(),
            weight: 1.0,
            tags: vec![],
        })
        .unwrap();
    world
        .configure_clock(WorldClockPolicyDesc {
            max_steps_per_frame: 1,
            ..Default::default()
        })
        .unwrap();
    world
        .upsert_process(WorldProcessDesc {
            id: "production".into(),
            interval_seconds: 0.2,
            phase_seconds: 0.0,
            enabled: true,
            priority: 0,
            tags: vec![],
            payload: json!({"batch": 3}),
        })
        .unwrap();
    world.schedule_event(event("delivery", None)).unwrap();
    world.tick_frame(0.5, &[]);
    world.set_fact("counter", json!(3)).unwrap();
    world.set_fact("counter", json!(4)).unwrap();
    let checkpoint = world.checkpoint().unwrap();
    let mut restored = LivingWorldRuntime::from_checkpoint(checkpoint.clone()).unwrap();
    assert_eq!(restored.checkpoint().unwrap(), checkpoint);
    assert_eq!(restored.facts["counter"].revision, 2);
    assert_eq!(restored.scenario_reservations.len(), 1);
    assert_eq!(restored.relationships.len(), 1);
    restored.tick_frame(0.0, &[]);
    assert_eq!(restored.frame_events.len(), 1);
    assert!(restored
        .frame_reality_events
        .iter()
        .any(|e| e.desc.kind == "world.fact.changed"));
    let after_delivery = restored.checkpoint().unwrap();
    let mut restarted = LivingWorldRuntime::from_checkpoint(after_delivery).unwrap();
    for _ in 0..12 {
        restarted.tick_frame(0.0, &[]);
        assert!(restarted.frame_events.is_empty());
    }
    assert!((restarted.clock.world_seconds - 0.5).abs() < 1e-7);
}

#[test]
fn invalid_checkpoint_does_not_replace_live_world() {
    let mut world = reservation_world();
    world.set_fact("keep", json!(42)).unwrap();
    let before = world.checkpoint().unwrap();
    let mut broken = before.clone();
    broken["state"]["clock"]["policy"]["fixed_hz"] = json!(0);
    assert!(world.restore_checkpoint(broken).is_err());
    assert_eq!(world.checkpoint().unwrap(), before);
}

fn moving_world() -> LivingWorldRuntime {
    let mut world = LivingWorldRuntime::default();
    let mut a = actor("walker");
    a.position = [0.0; 3];
    world.upsert_actor(a).unwrap();
    for (id, x) in [("a", 0.0), ("b", 100.0)] {
        world
            .upsert_nav_node(WorldNavNodeDesc {
                id: id.into(),
                position: [x, 0.0, 0.0],
                tags: vec![],
                parameters: BTreeMap::new(),
            })
            .unwrap();
    }
    world
        .upsert_nav_edge(WorldNavEdgeDesc {
            id: "ab".into(),
            from: "a".into(),
            to: "b".into(),
            bidirectional: true,
            distance: None,
            cost_scale: 1.0,
            enabled: true,
            tags: vec![],
            parameters: BTreeMap::new(),
        })
        .unwrap();
    world
}
fn begin_travel(world: &mut LivingWorldRuntime) {
    world
        .start_travel(WorldTravelRequestDesc {
            actor_id: "walker".into(),
            start_node: Some("a".into()),
            destination_node: "b".into(),
            speed: 5.0,
            mode: "walk".into(),
            payload: json!({"job": 7}),
        })
        .unwrap();
}

#[test]
fn route_continues_identically_after_restart_and_observer_promotes_immediately() {
    let mut world = moving_world();
    begin_travel(&mut world);
    for _ in 0..25 {
        world.tick_frame(0.1, &[]);
    }
    let mut restored = LivingWorldRuntime::from_checkpoint(world.checkpoint().unwrap()).unwrap();
    for _ in 0..30 {
        world.tick_frame(0.1, &[]);
        restored.tick_frame(0.1, &[]);
    }
    assert_eq!(world.checkpoint().unwrap(), restored.checkpoint().unwrap());
    assert!(restored.actors["walker"].desc.position[0] > 10.0);
    let position = restored.actors["walker"].desc.position;
    restored.tick_frame(0.05, &[position]);
    assert_eq!(restored.actors["walker"].last_tier, SimulationTier::Full);
    restored.tick_frame(0.05, &[]);
    assert_eq!(
        restored.actors["walker"].last_tier,
        SimulationTier::Background
    );
}

#[test]
fn new_route_does_not_consume_time_from_before_departure() {
    let mut world = moving_world();
    world.tick_frame(0.1, &[]);
    for _ in 0..9 {
        world.tick_frame(0.1, &[]);
    }
    begin_travel(&mut world);
    world.tick_frame(0.05, &[[0.0; 3]]);
    assert!(world.actors["walker"].desc.position[0] <= 0.251);
}
