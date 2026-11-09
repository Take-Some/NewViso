use super::*;
fn model() -> DirectorModel {
    serde_json::from_str(r#"{"schema":"newviso.combat-director.v1","max_active_melee":1,"max_active_ranged":2,"maximum_threat_probes":8,"sense_interval":0.15,"slot_duration_scale":2,"slot_separation_radians":0.65,"support_radius_scale":1.6,"cover_search_radius":9,"cover_clearance":0.08,"flank_angle_radians":1.05,"preferred_range_scale":0.8,"repath_distance":0.4,"switch_cost":0.2,"melee_request_interval":0.08,"engagement_seconds":60,"weights":{"distance":1,"readiness":1.2,"vitality":0.7,"stamina":0.6,"waiting":0.8,"threat":1,"counter":0.8,"guard":4,"retreat":1.8,"cover":2,"flank":0.9,"observe":0.1},"maximum_position_probes":24}"#).unwrap()
}
fn participant(id: &str, score: f32, position: [f32; 3]) -> Participant {
    Participant {
        id: id.into(),
        target: "player".into(),
        group: "enemy".into(),
        position,
        target_position: [0.0; 3],
        ranged: false,
        eligible: true,
        action_running: false,
        score,
        duration: 1.0,
        model: model(),
    }
}
#[test]
fn attack_leases_are_bounded_hold_during_action_and_release_on_death() {
    let mut d = DirectorRuntime::default();
    let mut actors = vec![
        participant("a", 2.0, [1.0, 0.0, 0.0]),
        participant("b", 1.0, [-1.0, 0.0, 0.0]),
    ];
    let roles = d.allocate(0.0, &actors);
    assert_eq!(roles["a"].0, "active");
    actors[1].score = 10.0;
    assert_eq!(
        d.allocate(0.5, &actors)["a"].0,
        "active",
        "permit must not flicker within its lease"
    );
    assert_eq!(
        d.allocate(2.1, &actors)["b"].0,
        "active",
        "waiting ready actor receives the next lease"
    );
    actors[1].action_running = true;
    assert_eq!(
        d.allocate(5.0, &actors)["b"].0,
        "active",
        "an action owns its slot through actual windup/contact"
    );
    actors[1].eligible = false;
    let roles = d.allocate(5.1, &actors);
    assert_eq!(roles["a"].0, "active");
    assert_ne!(roles["b"].0, "active");
    assert_eq!(roles.values().filter(|v| v.0 == "active").count(), 1);
}
#[test]
fn simultaneous_attack_slots_require_separated_approaches() {
    let mut actors = vec![
        participant("a", 2.0, [0.0, 0.0, 1.0]),
        participant("b", 1.0, [0.0, 0.0, 2.0]),
    ];
    for a in &mut actors {
        a.model.max_active_melee = 2;
    }
    let mut d = DirectorRuntime::default();
    assert_eq!(
        d.allocate(0.0, &actors)
            .values()
            .filter(|v| v.0 == "active")
            .count(),
        1
    );
    actors[1].position = [0.0, 0.0, -1.0];
    assert_eq!(
        d.allocate(0.1, &actors)
            .values()
            .filter(|v| v.0 == "active")
            .count(),
        2
    );
}
fn actor(id: &str, position: [f32; 3]) -> AgentActorView {
    AgentActorView {
        id: id.into(),
        position,
        enabled: true,
        simulation_tier: "full".into(),
        travel_destination: None,
    }
}
fn runtime() -> (PedRuntime, AgentRuntime, AgentWorldSnapshot) {
    let mut p = PedRuntime::default();
    let mut a = AgentRuntime::default();
    a.upsert_agent(AgentDesc {
        id: "enemy.ai".into(),
        actor_id: "enemy".into(),
        enabled: true,
        perception: AgentPerceptionPolicy::default(),
        thinking: AgentThinkingPolicy::default(),
        blackboard: BTreeMap::new(),
    })
    .unwrap();
    p.upsert(
        PedProfile {
            actor_id: "enemy".into(),
            agent_id: "enemy.ai".into(),
            group: "enemy".into(),
            combat: Some(combat::CombatProfile {
                target_actor: "player".into(),
                melee_only: true,
                director: Some(model()),
                ..Default::default()
            }),
            melee: Some(melee::MeleeProfile::default()),
            ..Default::default()
        },
        true,
    )
    .unwrap();
    p.upsert(
        PedProfile {
            actor_id: "player".into(),
            damage_entity: Some(1),
            group: "player".into(),
            melee: Some(melee::MeleeProfile::default()),
            ..Default::default()
        },
        true,
    )
    .unwrap();
    let c = p.records.get_mut("enemy").unwrap().combat.as_mut().unwrap();
    c.mode = "engage".into();
    c.visible = true;
    c.last_seen_position = Some([0.0, 0.0, 0.62]);
    c.ready_at = 0.0;
    let s = AgentWorldSnapshot {
        world_seconds: 1.0,
        actors: vec![actor("enemy", [0.0; 3]), actor("player", [0.0, 0.0, 0.62])],
        stimuli: Vec::new(),
    };
    (p, a, s)
}
#[test]
fn guard_and_counter_follow_observed_attack_phases_instead_of_wall_clock() {
    let (mut p, mut agents, mut snapshot) = runtime();
    p.records
        .get_mut("player")
        .unwrap()
        .melee
        .as_mut()
        .unwrap()
        .phase = "windup".into();
    p.tick_combat_director(&snapshot, &BTreeMap::new(), &BTreeMap::new(), &mut agents)
        .unwrap();
    let enemy = p.state("enemy").unwrap();
    assert_eq!(enemy.combat.as_ref().unwrap().tactics.action, "guard");
    assert!(enemy.melee.as_ref().unwrap().blocking);
    p.records
        .get_mut("player")
        .unwrap()
        .melee
        .as_mut()
        .unwrap()
        .phase = "recovery".into();
    snapshot.world_seconds = 1.2;
    p.tick_combat_director(&snapshot, &BTreeMap::new(), &BTreeMap::new(), &mut agents)
        .unwrap();
    assert_eq!(
        p.state("enemy")
            .unwrap()
            .combat
            .as_ref()
            .unwrap()
            .tactics
            .action,
        "counter"
    );
    assert!(!p.state("enemy").unwrap().melee.as_ref().unwrap().blocking);
    let c = p.records.get_mut("enemy").unwrap().combat.as_mut().unwrap();
    c.visible = false;
    c.mode = "search".into();
    snapshot.world_seconds = 1.4;
    p.tick_combat_director(&snapshot, &BTreeMap::new(), &BTreeMap::new(), &mut agents)
        .unwrap();
    assert!(
        !p.state("enemy")
            .unwrap()
            .combat
            .as_ref()
            .unwrap()
            .tactics
            .attack_allowed
    );
}
#[test]
fn retargeting_requires_visibility_hostility_and_a_new_reaction_window() {
    let (mut p, _, mut snapshot) = runtime();
    for (id, group, entity) in [("rival", "rival", 2), ("civilian", "civilian", 3)] {
        p.upsert(
            PedProfile {
                actor_id: id.into(),
                damage_entity: Some(entity),
                group: group.into(),
                ..Default::default()
            },
            true,
        )
        .unwrap();
    }
    p.set_relationship("enemy".into(), "rival".into(), PedRelationship::Hate)
        .unwrap();
    snapshot.actors.extend([
        actor("rival", [0.2, 0.0, 0.0]),
        actor("civilian", [0.1, 0.0, 0.0]),
    ]);
    let probes = p.tactical_probes(&snapshot);
    assert!(probes.iter().any(|v| v.1 == "rival"));
    assert!(!probes.iter().any(|v| v.1 == "civilian"));
    p.select_combat_targets(
        &snapshot,
        &BTreeMap::from([
            (("enemy".into(), "rival".into()), false),
            (("enemy".into(), "civilian".into()), true),
        ]),
    );
    assert_eq!(
        p.state("enemy")
            .unwrap()
            .combat
            .as_ref()
            .unwrap()
            .target_actor,
        "player"
    );
    p.select_combat_targets(
        &snapshot,
        &BTreeMap::from([(("enemy".into(), "rival".into()), true)]),
    );
    let c = p.state("enemy").unwrap().combat.as_ref().unwrap();
    assert_eq!(c.target_actor, "rival");
    assert!(c.ready_at >= 1.8);
}
#[test]
fn exact_authored_attack_reach_does_not_force_overlapping_capsules() {
    let (_, mut agents, snapshot) = runtime();
    agents
        .set_task(
            "enemy.ai",
            AgentTaskDesc {
                id: "test".into(),
                lane: AgentTaskLane::Reaction,
                priority: 90,
                task: AgentTaskKind::Combat {
                    target_actor: "player".into(),
                    speed: 2.0,
                    range: 0.65,
                    cooldown_seconds: 0.1,
                    damage: 1.0,
                    duration_seconds: 10.0,
                },
            },
        )
        .unwrap();
    let commands = agents.tick(0.05, &snapshot);
    assert!(commands
        .iter()
        .any(|c| matches!(c, AgentCommand::Attack { .. })));
    assert!(!commands
        .iter()
        .any(|c| matches!(c, AgentCommand::MoveToPosition { .. })));
}

#[test]
fn occluded_reload_retains_cover_then_peeks_and_memory_expiry_releases_task() {
    let (mut p, mut agents, mut snapshot) = runtime();
    let ped = p.records.get_mut("enemy").unwrap();
    ped.profile.combat.as_mut().unwrap().melee_only = false;
    let c = ped.combat.as_mut().unwrap();
    c.style = "ranged".into();
    c.magazine = 0;
    c.reserve = 8;
    c.reload_until = 3.0;
    snapshot.actors[1].position = [0.0, 0.0, 10.0];
    p.tick_combat(
        &snapshot,
        &BTreeMap::from([("enemy".into(), true)]),
        &mut agents,
    )
    .unwrap();
    let cover = [1.0, 0.0, 0.0];
    let peek = [1.0, 0.0, 1.0];
    let positions = BTreeMap::from([(
        "enemy".into(),
        vec![
            TacticalPosition {
                position: cover,
                kind: "cover".into(),
                quality: 1.0,
                travel_cost: 0.1,
            },
            TacticalPosition {
                position: peek,
                kind: "peek".into(),
                quality: 1.0,
                travel_cost: 0.1,
            },
        ],
    )]);
    p.tick_combat_director(&snapshot, &BTreeMap::new(), &positions, &mut agents)
        .unwrap();
    assert_eq!(
        p.state("enemy")
            .unwrap()
            .combat
            .as_ref()
            .unwrap()
            .tactics
            .action,
        "cover"
    );
    snapshot.world_seconds = 1.2;
    p.tick_combat(
        &snapshot,
        &BTreeMap::from([("enemy".into(), false)]),
        &mut agents,
    )
    .unwrap();
    p.tick_combat_director(&snapshot, &BTreeMap::new(), &positions, &mut agents)
        .unwrap();
    let c = p.state("enemy").unwrap().combat.as_ref().unwrap();
    assert_eq!(c.mode, "search");
    assert_eq!(c.tactics.action, "cover");
    assert_eq!(c.tactics.goal, Some(cover));
    assert!(!c.tactics.attack_allowed);
    snapshot.world_seconds = 3.2;
    p.tick_combat(
        &snapshot,
        &BTreeMap::from([("enemy".into(), false)]),
        &mut agents,
    )
    .unwrap();
    p.tick_combat_director(&snapshot, &BTreeMap::new(), &positions, &mut agents)
        .unwrap();
    assert_eq!(
        p.state("enemy")
            .unwrap()
            .combat
            .as_ref()
            .unwrap()
            .tactics
            .action,
        "peek"
    );
    snapshot.world_seconds = 20.0;
    p.tick_combat(
        &snapshot,
        &BTreeMap::from([("enemy".into(), false)]),
        &mut agents,
    )
    .unwrap();
    p.tick_combat_director(&snapshot, &BTreeMap::new(), &positions, &mut agents)
        .unwrap();
    let c = p.state("enemy").unwrap().combat.as_ref().unwrap();
    assert_eq!(c.tactics.action, "patrol");
    assert!(c.last_seen_position.is_none());
    assert!(!agents.agents["enemy.ai"]
        .tasks
        .contains_key("combat.engagement"));
}

#[test]
fn unused_attack_permission_does_not_erase_waiting_priority() {
    let (mut p, mut agents, mut snapshot) = runtime();
    snapshot.actors[0].position = [1.0, 0.0, 0.0];
    for now in [1.0, 1.2, 1.4] {
        snapshot.world_seconds = now;
        p.tick_combat_director(&snapshot, &BTreeMap::new(), &BTreeMap::new(), &mut agents)
            .unwrap();
    }
    let tactics = &p.state("enemy").unwrap().combat.as_ref().unwrap().tactics;
    assert_eq!(tactics.role, "active");
    assert!(
        tactics.waiting_seconds >= 0.39,
        "an unused lease erased waiting: {tactics:?}"
    );
    p.records
        .get_mut("enemy")
        .unwrap()
        .melee
        .as_mut()
        .unwrap()
        .attacks += 1;
    snapshot.world_seconds = 1.6;
    p.tick_combat_director(&snapshot, &BTreeMap::new(), &BTreeMap::new(), &mut agents)
        .unwrap();
    assert_eq!(
        p.state("enemy")
            .unwrap()
            .combat
            .as_ref()
            .unwrap()
            .tactics
            .waiting_seconds,
        0.0
    );
}
