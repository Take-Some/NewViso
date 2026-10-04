use super::*;
use peds::*;

fn agent() -> AgentDesc { AgentDesc { id: "ped.ai".into(), actor_id: "ped".into(), enabled: true,
    perception: AgentPerceptionPolicy::default(), thinking: AgentThinkingPolicy::default(), blackboard: BTreeMap::new() } }
fn actor(id: &str, position: [f32; 3], travel: Option<&str>) -> AgentActorView {
    AgentActorView { id: id.into(), position, enabled: true, simulation_tier: "full".into(), travel_destination: travel.map(str::to_owned) }
}
fn task(action: AgentTaskKind) -> AgentTaskDesc { AgentTaskDesc { id: "job".into(), lane: AgentTaskLane::Ambient, priority: 0, task: action } }
fn snapshot(time: f64, travel: Option<&str>) -> AgentWorldSnapshot {
    AgentWorldSnapshot { world_seconds: time, actors: vec![actor("ped", [0.0; 3], travel)], stimuli: vec![] }
}
fn runtime(action: AgentTaskKind) -> AgentRuntime {
    let mut r = AgentRuntime::default(); r.upsert_agent(agent()).unwrap(); r.set_task("ped.ai", task(action)).unwrap(); r
}

#[test]
fn coordinate_goal_emits_once_then_completes_on_physical_arrival() {
    let mut r = runtime(AgentTaskKind::GoToPosition { position: [10.0, 0.0, 0.0], speed: 1.4, stop_distance: 0.25 });
    assert!(matches!(&r.tick(0.1, &snapshot(0.1, None))[0], AgentCommand::MoveToPosition { .. }));
    assert!(r.tick(0.1, &snapshot(0.2, Some("job"))).is_empty());
    let mut s = snapshot(0.3, Some("job")); s.actors[0].position = [9.9, 0.0, 0.0];
    assert!(matches!(&r.tick(0.1, &s)[0], AgentCommand::CancelTravel { .. }));
    assert_eq!(r.runtime_state()["agents"][0]["tasks"][0]["status"], "completed");
}

#[test]
fn replacing_active_movement_with_idle_cancels_previous_motion() {
    let mut r = runtime(AgentTaskKind::GoToPosition { position: [10.0, 0.0, 0.0], speed: 1.4, stop_distance: 0.25 });
    r.tick(0.1, &snapshot(0.1, None));
    r.set_task("ped.ai", task(AgentTaskKind::Idle)).unwrap();
    assert!(matches!(&r.tick(0.1, &snapshot(0.2, Some("job")))[0], AgentCommand::CancelTravel { .. }));
}

#[test]
fn following_keeps_distance_and_stops_when_target_disappears() {
    let mut r = runtime(AgentTaskKind::FollowActor { target_actor: "friend".into(), speed: 2.0, distance: 2.0, repath_seconds: 0.5 });
    let mut s = snapshot(0.1, None); s.actors.push(actor("friend", [10.0, 0.0, 0.0], None));
    let c = r.tick(0.1, &s); let AgentCommand::MoveToPosition { position, .. } = c[0] else { panic!() };
    assert!((position[0] - 8.2).abs() < 0.001);
    s.world_seconds = 0.2; s.actors[0].travel_destination = Some("job".into()); s.actors[1].position = [1.0, 0.0, 0.0];
    assert!(matches!(&r.tick(0.1, &s)[0], AgentCommand::CancelTravel { .. }));
    s.world_seconds = 0.3; s.actors.pop(); r.tick(0.1, &s);
    assert_eq!(r.runtime_state()["agents"][0]["tasks"][0]["status"], "failed");
}

#[test]
fn flee_from_coincident_source_produces_finite_nonzero_goal() {
    let mut r = runtime(AgentTaskKind::Flee { threat_position: [0.0; 3], threat_actor: None,
        speed: 3.5, safe_distance: 30.0, duration_seconds: 12.0 });
    let c = r.tick(0.1, &snapshot(0.1, None)); let AgentCommand::MoveToPosition { position, .. } = c[0] else { panic!() };
    assert!(position.iter().all(|n| n.is_finite())); assert!(distance_between(position, [0.0; 3]) > 30.0);
}

#[test]
fn sequence_resumes_interrupted_child_and_then_advances_to_wait() {
    let mut r = runtime(AgentTaskKind::Sequence { repeat: false, steps: vec![
        AgentTaskKind::GoToPosition { position: [5.0, 0.0, 0.0], speed: 1.4, stop_distance: 0.25 },
        AgentTaskKind::Wait { duration_seconds: 0.2 }] });
    r.tick(0.1, &snapshot(0.1, None));
    r.set_task("ped.ai", AgentTaskDesc { id: "reaction".into(), lane: AgentTaskLane::Reaction,
        priority: 0, task: AgentTaskKind::Wait { duration_seconds: 0.1 } }).unwrap();
    assert!(matches!(&r.tick(0.1, &snapshot(0.2, Some("job")))[0], AgentCommand::CancelTravel { .. }));
    assert!(matches!(&r.tick(0.1, &snapshot(0.3, None))[0], AgentCommand::MoveToPosition { .. }));
    let mut s = snapshot(0.4, Some("job")); s.actors[0].position = [5.0, 0.0, 0.0]; r.tick(0.1, &s);
    for t in [0.5, 0.6, 0.7] { s.world_seconds = t; s.actors[0].travel_destination = None; r.tick(0.1, &s); }
    assert_eq!(r.runtime_state()["agents"][0]["tasks"].as_array().unwrap().iter().find(|t| t["id"] == "job").unwrap()["status"], "completed");
}

#[test]
fn armour_absorbs_damage_and_death_cancels_tasks_once() {
    let mut r = runtime(AgentTaskKind::Wander { center: None, radius: 5.0, speed: 1.0, pause_seconds: 1.0 });
    r.tick(0.1, &snapshot(0.1, None));
    let mut p = PedRuntime::default(); p.upsert(PedProfile { actor_id: "ped".into(), agent_id: "ped.ai".into(), max_health: 100.0, armour: 25.0, ..PedProfile::default() }, false).unwrap();
    p.damage("ped", "attacker".into(), 40.0, [0.0; 3], &mut r).unwrap();
    assert_eq!(p.state("ped").unwrap().health, 85.0); assert_eq!(p.state("ped").unwrap().armour, 0.0);
    assert!(matches!(p.damage("ped", "attacker".into(), 90.0, [0.0; 3], &mut r).unwrap(), Some(AgentCommand::CancelTravel { .. })));
    assert!(p.is_dead("ped")); assert!(r.runtime_state()["agents"][0]["tasks"].as_array().unwrap().is_empty());
    let count = p.drain_events().len(); assert_eq!(count, 3);
    p.damage("ped", "attacker".into(), 90.0, [0.0; 3], &mut r).unwrap(); assert!(p.drain_events().is_empty());
}

#[test]
fn gunshots_trigger_one_reaction_and_respect_relationships() {
    let mut r = runtime(AgentTaskKind::Idle);
    let mut p = PedRuntime::default(); p.upsert(PedProfile { actor_id: "ped".into(), agent_id: "ped.ai".into(), ..PedProfile::default() }, false).unwrap();
    let mut s = snapshot(0.1, None); s.stimuli.push(AgentStimulusView { id: "shot".into(), kind: "gunshot".into(), source: "gunman".into(),
        position: [1.0, 0.0, 0.0], radius: 50.0, intensity: 1.0, remaining_seconds: 0.4, tags: vec![], payload: Value::Null });
    p.tick(&s, &mut r).unwrap(); r.tick(0.1, &s); assert_eq!(p.drain_events().len(), 1);
    s.world_seconds = 0.2; p.tick(&s, &mut r).unwrap(); assert!(p.drain_events().is_empty());
    let mut p = PedRuntime::default(); p.upsert(PedProfile { actor_id: "ped".into(), agent_id: "ped.ai".into(), ..PedProfile::default() }, false).unwrap();
    s.stimuli[0].source = "ped".into(); p.tick(&s, &mut r).unwrap(); assert!(p.drain_events().is_empty());
}

#[test]
fn nested_invalid_tasks_are_rejected_before_execution() {
    let mut action = AgentTaskKind::Wait { duration_seconds: 1.0 };
    for _ in 0..10 { action = AgentTaskKind::Sequence { steps: vec![action], repeat: false }; }
    assert!(task(action).validate().is_err());
    assert!(task(AgentTaskKind::GoToPosition { position: [f32::NAN, 0.0, 0.0], speed: 1.0, stop_distance: 0.25 }).validate().is_err());
}
