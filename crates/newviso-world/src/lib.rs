#[cfg(test)]
mod coordinate_tests;
mod clock;
mod data;
mod navigation;
mod persistence;
pub use persistence::MAX_CHECKPOINT_BYTES;
mod reality;
mod scheduler;
mod simulation;

pub use clock::WorldClockPolicyDesc;
pub use data::{
    AmbientModelSetDesc, LivingWorldZoneDesc, PopulationChannelDesc, PopulationStreamingPolicyDesc,
    RelationshipRuleDesc, ScenarioPointDesc, WorldStimulusDesc,
};
pub use navigation::{WorldNavEdgeDesc, WorldNavNodeDesc, WorldTravelRequestDesc};
pub use reality::{WorldRealityEventDesc, WorldScenarioReservationDesc};
pub use scheduler::{WorldProcessDesc, WorldScheduledEventDesc};
pub use simulation::{WorldActorDesc, WorldObserverDesc, WorldSimulationPolicyDesc};

use clock::{WorldClockRuntime, WorldStep};
use data::{
    point_in_aabb, valid_json_payload, valid_label, validate_id, zone_volume, ActiveStimulus,
};
use navigation::{
    advance_toward, nearest_node, shortest_route, WorldTravelCompletion, WorldTravelRecord,
};
use reality::{
    WorldRealityEventRecord, WorldRepresentationChange, WorldScenarioReservationRecord,
    MAX_REALITY_HISTORY,
};
use scheduler::{
    first_due_at_or_after, ScheduledWorldEvent, WorldEventDelivery, WorldFactRecord,
    WorldProcessActivation, WorldProcessRecord,
};
use serde_json::{json, Value};
use simulation::{actor_tier, ActorRecord, ActorUpdateTicket, SimulationTier};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct WorldActorRuntimeView {
    pub id: String,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub travel_mode: Option<String>,
    pub travel_destination: Option<String>,
    pub travel_target_position: Option<[f32; 3]>,
    pub travel_speed: Option<f32>,
    pub external_motion_authority: bool,
    pub simulation_tier: &'static str,
    pub representation: &'static str,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldStimulusRuntimeView {
    pub id: String,
    pub kind: String,
    pub source: String,
    pub position: [f32; 3],
    pub radius: f32,
    pub intensity: f32,
    pub remaining_seconds: f32,
    pub tags: Vec<String>,
    pub payload: Value,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LivingWorldRuntime {
    clock: WorldClockRuntime,
    simulation_policy: WorldSimulationPolicyDesc,
    observers: BTreeMap<String, WorldObserverDesc>,
    actors: BTreeMap<String, ActorRecord>,
    processes: BTreeMap<String, WorldProcessRecord>,
    scheduled_events: BTreeMap<String, ScheduledWorldEvent>,
    #[serde(skip)]
    scheduled_event_deadlines: BTreeSet<runtime_events::EventDeadline>,
    #[serde(skip)]
    scheduled_event_index_ready: bool,
    facts: BTreeMap<String, WorldFactRecord>,
    population_channels: BTreeMap<String, PopulationChannelDesc>,
    population_streaming: PopulationStreamingPolicyDesc,
    zones: BTreeMap<String, LivingWorldZoneDesc>,
    model_sets: BTreeMap<String, AmbientModelSetDesc>,
    scenario_points: BTreeMap<String, ScenarioPointDesc>,
    #[serde(with = "persistence::relationships")]
    relationships: BTreeMap<(String, String), RelationshipRuleDesc>,
    stimuli: BTreeMap<String, ActiveStimulus>,
    #[serde(skip)]
    frame_actor_updates: Vec<ActorUpdateTicket>,
    #[serde(skip)]
    frame_process_activations: Vec<WorldProcessActivation>,
    #[serde(skip)]
    frame_events: Vec<WorldEventDelivery>,
    next_stimulus_id: u64,
    next_event_id: u64,
    dropped_expired_events: u64,
    reality_events: Vec<WorldRealityEventRecord>,
    #[serde(skip)]
    frame_reality_events: Vec<WorldRealityEventRecord>,
    pending_reality_events: Vec<WorldRealityEventRecord>,
    scenario_reservations: BTreeMap<String, WorldScenarioReservationRecord>,
    nav_nodes: BTreeMap<String, WorldNavNodeDesc>,
    nav_edges: BTreeMap<String, WorldNavEdgeDesc>,
    travel: BTreeMap<String, WorldTravelRecord>,
    #[serde(skip)]
    external_motion_velocities: BTreeMap<String, [f32; 3]>,
    #[serde(skip)]
    frame_travel_completions: Vec<WorldTravelCompletion>,
    #[serde(skip)]
    frame_representation_changes: Vec<WorldRepresentationChange>,
    next_reality_event_id: u64,
    next_reality_sequence: u64,
}

impl Default for LivingWorldRuntime {
    fn default() -> Self {
        Self {
            clock: WorldClockRuntime::default(),
            simulation_policy: WorldSimulationPolicyDesc::default(),
            observers: BTreeMap::new(),
            actors: BTreeMap::new(),
            processes: BTreeMap::new(),
            scheduled_events: BTreeMap::new(),
            scheduled_event_deadlines: BTreeSet::new(),
            scheduled_event_index_ready: true,
            facts: BTreeMap::new(),
            population_channels: BTreeMap::new(),
            population_streaming: PopulationStreamingPolicyDesc::default(),
            zones: BTreeMap::new(),
            model_sets: BTreeMap::new(),
            scenario_points: BTreeMap::new(),
            relationships: BTreeMap::new(),
            stimuli: BTreeMap::new(),
            frame_actor_updates: Vec::new(),
            frame_process_activations: Vec::new(),
            frame_events: Vec::new(),
            next_stimulus_id: 1,
            next_event_id: 1,
            dropped_expired_events: 0,
            reality_events: Vec::new(),
            frame_reality_events: Vec::new(),
            pending_reality_events: Vec::new(),
            scenario_reservations: BTreeMap::new(),
            nav_nodes: BTreeMap::new(),
            nav_edges: BTreeMap::new(),
            travel: BTreeMap::new(),
            external_motion_velocities: BTreeMap::new(),
            frame_travel_completions: Vec::new(),
            frame_representation_changes: Vec::new(),
            next_reality_event_id: 1,
            next_reality_sequence: 1,
        }
    }
}

mod runtime_actors;
mod runtime_clock;
mod runtime_events;
mod runtime_navigation;
mod runtime_population;
mod runtime_processes;
mod runtime_reality;
mod runtime_snapshot;
mod runtime_tick;

type ActorUpdateCandidate<'a> = (&'a str, SimulationTier, f64, f64);

fn compare_actor_update_candidates(
    a: &ActorUpdateCandidate<'_>,
    b: &ActorUpdateCandidate<'_>,
) -> Ordering {
    b.3.partial_cmp(&a.3)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.1.rank().cmp(&b.1.rank()))
        .then_with(|| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal))
        .then_with(|| a.0.cmp(&b.0))
}

fn representation_for_tier(tier: SimulationTier) -> &'static str {
    match tier {
        SimulationTier::Full => "physical",
        SimulationTier::Reduced => "proxy",
        SimulationTier::Background => "abstract",
    }
}

fn intervals_overlap(a_start: f64, a_end: f64, b_start: f64, b_end: f64) -> bool {
    a_start < b_end && b_start < a_end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(id: &str, position: [f32; 3]) -> WorldActorDesc {
        WorldActorDesc {
            id: id.to_owned(),
            kind: "generic".to_owned(),
            position,
            group: None,
            channel: None,
            enabled: true,
            tags: Vec::new(),
            parameters: BTreeMap::new(),
            state: Value::Null,
        }
    }

    #[test]
    fn explicit_travel_start_overrides_nearest_disconnected_node() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .upsert_actor(actor("walker.explicit", [0.01, 0.0, 0.0]))
            .unwrap();

        for (id, position) in [
            ("wrong.nearest", [0.0, 0.0, 0.0]),
            ("route.start", [1.0, 0.0, 0.0]),
            ("route.goal", [5.0, 0.0, 0.0]),
        ] {
            runtime
                .upsert_nav_node(WorldNavNodeDesc {
                    id: id.to_owned(),
                    position,
                    tags: Vec::new(),
                    parameters: BTreeMap::new(),
                })
                .unwrap();
        }
        runtime
            .upsert_nav_edge(WorldNavEdgeDesc {
                id: "route.edge".to_owned(),
                from: "route.start".to_owned(),
                to: "route.goal".to_owned(),
                bidirectional: true,
                distance: None,
                cost_scale: 1.0,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();

        runtime
            .start_travel(WorldTravelRequestDesc {
                actor_id: "walker.explicit".to_owned(),
                start_node: Some("route.start".to_owned()),
                destination_node: "route.goal".to_owned(),
                speed: 1.0,
                mode: "walk".to_owned(),
                payload: Value::Null,
            })
            .unwrap();

        let state = runtime.runtime_state();
        let travel = state["navigation"]["active_travel"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["actor_id"] == "walker.explicit")
            .unwrap();
        assert_eq!(travel["route"], json!(["route.start", "route.goal"]));
        assert_eq!(travel["next_waypoint_index"], json!(1));
    }

    #[test]
    fn frame_state_omits_static_navigation_topology_but_keeps_runtime_navigation() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .upsert_actor(actor("walker.frame", [0.0, 0.0, 0.0]))
            .unwrap();
        for (id, position) in [("a.frame", [0.0, 0.0, 0.0]), ("b.frame", [5.0, 0.0, 0.0])] {
            runtime
                .upsert_nav_node(WorldNavNodeDesc {
                    id: id.to_owned(),
                    position,
                    tags: Vec::new(),
                    parameters: BTreeMap::new(),
                })
                .unwrap();
        }
        runtime
            .upsert_nav_edge(WorldNavEdgeDesc {
                id: "ab.frame".to_owned(),
                from: "a.frame".to_owned(),
                to: "b.frame".to_owned(),
                bidirectional: true,
                distance: None,
                cost_scale: 1.0,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .start_travel(WorldTravelRequestDesc {
                actor_id: "walker.frame".to_owned(),
                start_node: Some("a.frame".to_owned()),
                destination_node: "b.frame".to_owned(),
                speed: 1.0,
                mode: "walk".to_owned(),
                payload: Value::Null,
            })
            .unwrap();

        let frame = runtime.frame_state();
        assert_eq!(frame["navigation"]["node_count"], json!(2));
        assert_eq!(frame["navigation"]["edge_count"], json!(1));
        assert_eq!(frame["navigation"]["nodes"], json!([]));
        assert_eq!(frame["navigation"]["edges"], json!([]));
        assert_eq!(
            frame["navigation"]["active_travel"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );

        let full = runtime.runtime_state();
        assert_eq!(
            full["navigation"]["nodes"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(
            full["navigation"]["edges"].as_array().map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn background_world_advances_without_observer() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .upsert_actor(actor("far", [10_000.0, 0.0, 0.0]))
            .unwrap();
        runtime
            .upsert_process(WorldProcessDesc {
                id: "economy".to_owned(),
                interval_seconds: 1.0,
                phase_seconds: 0.0,
                enabled: true,
                priority: 0,
                tags: Vec::new(),
                payload: json!({"system": "generic"}),
            })
            .unwrap();

        for _ in 0..50 {
            runtime.tick_frame(0.1, &[]);
        }

        let state = runtime.runtime_state();
        assert!(state["clock"]["world_seconds"].as_f64().unwrap() >= 4.9);
        assert!(
            state["simulation"]["actors"][0]["last_update_world_seconds"]
                .as_f64()
                .unwrap()
                > 0.0
        );
        assert!(
            state["processes"]["registered"][0]["next_due_world_seconds"]
                .as_f64()
                .unwrap()
                > 4.0
        );
    }

    #[test]
    fn observer_changes_fidelity_not_world_existence() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .upsert_actor(actor("near", [0.0, 0.0, 0.0]))
            .unwrap();
        runtime
            .upsert_actor(actor("far", [5000.0, 0.0, 0.0]))
            .unwrap();

        runtime.tick_frame(0.1, &[[0.0, 0.0, 0.0]]);
        let state = runtime.runtime_state();
        let actors = state["simulation"]["actors"].as_array().unwrap();
        let near = actors.iter().find(|item| item["id"] == "near").unwrap();
        let far = actors.iter().find(|item| item["id"] == "far").unwrap();
        assert_eq!(near["simulation_tier"].as_str(), Some("full"));
        assert_eq!(far["simulation_tier"].as_str(), Some("background"));
        assert!(far["enabled"].as_bool().unwrap());
    }

    #[test]
    fn fixed_clock_preserves_backlog_instead_of_dropping_world_time() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .configure_clock(WorldClockPolicyDesc {
                fixed_hz: 20.0,
                time_scale: 1.0,
                max_steps_per_frame: 2,
            })
            .unwrap();
        runtime.tick_frame(1.0, &[]);
        let state = runtime.runtime_state();
        assert_eq!(state["clock"]["last_frame_steps"].as_u64(), Some(2));
        assert!(state["clock"]["backlog_seconds"].as_f64().unwrap() > 0.8);
    }

    #[test]
    fn scheduled_events_fire_without_player_input() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .schedule_event(WorldScheduledEventDesc {
                id: "event.1".to_owned(),
                kind: "generic.change".to_owned(),
                source: "world.process".to_owned(),
                cause: None,
                delay_seconds: 0.2,
                ttl_seconds: 2.0,
                priority: 3,
                position: None,
                tags: Vec::new(),
                payload: json!({"value": 7}),
            })
            .unwrap();

        runtime.tick_frame(0.25, &[]);
        let state = runtime.runtime_state();
        assert_eq!(
            state["events"]["frame_due"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(state["events"]["frame_due"][0]["kind"], "generic.change");
    }

    #[test]
    fn world_facts_keep_revision_and_time() {
        let mut runtime = LivingWorldRuntime::default();
        runtime.set_fact("economy.index", json!(12.0)).unwrap();
        runtime.tick_frame(0.1, &[]);
        runtime.set_fact("economy.index", json!(13.0)).unwrap();
        let state = runtime.runtime_state();
        assert_eq!(state["facts"][0]["revision"].as_u64(), Some(2));
        assert_eq!(state["facts"][0]["value"], json!(13.0));
    }

    #[test]
    fn zones_are_world_memberships_not_single_player_zone() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .upsert_zone(LivingWorldZoneDesc {
                id: "outer".to_owned(),
                min: [-10.0, -10.0, -10.0],
                max: [10.0, 10.0, 10.0],
                priority: 1,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .upsert_zone(LivingWorldZoneDesc {
                id: "inner".to_owned(),
                min: [-2.0, -2.0, -2.0],
                max: [2.0, 2.0, 2.0],
                priority: 2,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .upsert_actor(actor("actor.1", [0.0, 0.0, 0.0]))
            .unwrap();

        let state = runtime.runtime_state();
        assert_eq!(
            state["simulation"]["actors"][0]["zones"],
            json!(["inner", "outer"])
        );
    }

    #[test]
    fn expired_stimuli_follow_world_clock() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .emit_stimulus(WorldStimulusDesc {
                id: "test".to_owned(),
                kind: "noise".to_owned(),
                source: "test".to_owned(),
                position: [0.0; 3],
                radius: 1.0,
                intensity: 1.0,
                lifetime_seconds: 0.25,
                tags: Vec::new(),
                payload: Value::Null,
            })
            .unwrap();
        runtime.tick_frame(0.3, &[]);
        assert_eq!(
            runtime.runtime_state()["stimuli"].as_array().map(Vec::len),
            Some(0)
        );
    }
    #[test]
    fn scheduled_event_is_persisted_as_reality_history() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .schedule_event(WorldScheduledEventDesc {
                id: "delivery.1".to_owned(),
                kind: "factory.delivery.arrival".to_owned(),
                source: "factory.17".to_owned(),
                cause: None,
                delay_seconds: 0.1,
                ttl_seconds: 5.0,
                priority: 2,
                position: Some([100.0, 20.0, 0.0]),
                tags: vec!["logistics".to_owned()],
                payload: json!({"cargo": 3}),
            })
            .unwrap();

        runtime.tick_frame(0.2, &[]);
        let state = runtime.runtime_state();
        assert_eq!(
            state["reality"]["history"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(
            state["reality"]["history"][0]["kind"],
            "factory.delivery.arrival"
        );
        assert_eq!(state["reality"]["history"][0]["cause"], "delivery.1");
    }

    #[test]
    fn exclusive_scenario_reservation_prevents_double_booking() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .upsert_scenario_point(ScenarioPointDesc {
                id: "bench.1".to_owned(),
                kind: "sit".to_owned(),
                group: None,
                position: [0.0; 3],
                heading_degrees: 0.0,
                radius: 1.0,
                probability: 1.0,
                model_set: None,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime.upsert_actor(actor("a", [0.0; 3])).unwrap();
        runtime.upsert_actor(actor("b", [0.0; 3])).unwrap();

        runtime
            .reserve_scenario(WorldScenarioReservationDesc {
                id: "r1".to_owned(),
                scenario_point_id: "bench.1".to_owned(),
                actor_id: "a".to_owned(),
                delay_seconds: 0.0,
                duration_seconds: 10.0,
                priority: 0,
                exclusive: true,
                payload: Value::Null,
            })
            .unwrap();

        let conflict = runtime.reserve_scenario(WorldScenarioReservationDesc {
            id: "r2".to_owned(),
            scenario_point_id: "bench.1".to_owned(),
            actor_id: "b".to_owned(),
            delay_seconds: 5.0,
            duration_seconds: 10.0,
            priority: 0,
            exclusive: true,
            payload: Value::Null,
        });
        assert!(conflict.is_err());
    }

    #[test]
    fn observer_transition_requests_physical_representation_without_creating_world() {
        let mut runtime = LivingWorldRuntime::default();
        runtime.upsert_actor(actor("actor.1", [0.0; 3])).unwrap();
        runtime.tick_frame(0.1, &[[0.0, 0.0, 0.0]]);
        let state = runtime.runtime_state();
        assert_eq!(
            state["simulation"]["actors"][0]["representation"],
            "physical"
        );
        assert_eq!(
            state["simulation"]["frame_representation_changes"][0]["previous"],
            "abstract"
        );
        assert_eq!(
            state["simulation"]["frame_representation_changes"][0]["current"],
            "physical"
        );
    }
    #[test]
    fn actor_runtime_view_exposes_travel_velocity_and_mode() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .upsert_actor(actor("walker-view", [0.0, 0.0, 0.0]))
            .unwrap();
        for (id, position) in [("a", [0.0, 0.0, 0.0]), ("b", [0.0, 0.0, 10.0])] {
            runtime
                .upsert_nav_node(WorldNavNodeDesc {
                    id: id.to_owned(),
                    position,
                    tags: Vec::new(),
                    parameters: BTreeMap::new(),
                })
                .unwrap();
        }
        runtime
            .upsert_nav_edge(WorldNavEdgeDesc {
                id: "ab-view".to_owned(),
                from: "a".to_owned(),
                to: "b".to_owned(),
                bidirectional: true,
                distance: None,
                cost_scale: 1.0,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .start_travel(WorldTravelRequestDesc {
                actor_id: "walker-view".to_owned(),
                start_node: Some("a".to_owned()),
                destination_node: "b".to_owned(),
                speed: 2.5,
                mode: "walk".to_owned(),
                payload: Value::Null,
            })
            .unwrap();

        let view = runtime
            .actor_runtime_views()
            .into_iter()
            .find(|view| view.id == "walker-view")
            .unwrap();
        assert_eq!(view.travel_mode.as_deref(), Some("walk"));
        assert!(view.velocity[0].abs() < 1.0e-6);
        assert!(view.velocity[1].abs() < 1.0e-6);
        assert!((view.velocity[2] - 2.5).abs() < 1.0e-6);
    }

    #[test]
    fn external_motion_authority_holds_coarse_travel_and_resumes_cleanly() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .configure_simulation(WorldSimulationPolicyDesc {
                full_interval_seconds: 0.05,
                reduced_interval_seconds: 0.05,
                background_interval_seconds: 0.05,
                ..WorldSimulationPolicyDesc::default()
            })
            .unwrap();
        runtime
            .upsert_actor(actor("physical", [0.0, 0.0, 0.0]))
            .unwrap();
        for (id, position) in [("a", [0.0, 0.0, 0.0]), ("b", [2.0, 0.0, 0.0])] {
            runtime
                .upsert_nav_node(WorldNavNodeDesc {
                    id: id.to_owned(),
                    position,
                    tags: Vec::new(),
                    parameters: BTreeMap::new(),
                })
                .unwrap();
        }
        runtime
            .upsert_nav_edge(WorldNavEdgeDesc {
                id: "ab-physical".to_owned(),
                from: "a".to_owned(),
                to: "b".to_owned(),
                bidirectional: true,
                distance: None,
                cost_scale: 1.0,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .start_travel(WorldTravelRequestDesc {
                actor_id: "physical".to_owned(),
                start_node: Some("a".to_owned()),
                destination_node: "b".to_owned(),
                speed: 1.0,
                mode: "walk".to_owned(),
                payload: Value::Null,
            })
            .unwrap();

        runtime
            .set_actor_external_motion("physical", [0.4, 0.0, 0.0], [1.0, 0.0, 0.0])
            .unwrap();
        runtime.tick_frame(1.0, &[]);
        let held = runtime
            .actor_runtime_views()
            .into_iter()
            .find(|view| view.id == "physical")
            .unwrap();
        assert!((held.position[0] - 0.4).abs() < 1.0e-6);
        assert!(held.external_motion_authority);

        runtime.release_actor_external_motion("physical");
        runtime.tick_frame(0.5, &[]);
        let resumed = runtime
            .actor_runtime_views()
            .into_iter()
            .find(|view| view.id == "physical")
            .unwrap();
        assert!(resumed.position[0] > 0.4);
    }

    #[test]
    fn background_actor_moves_over_route_without_observer() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .configure_simulation(WorldSimulationPolicyDesc {
                reduced_interval_seconds: 0.1,
                background_interval_seconds: 0.1,
                ..WorldSimulationPolicyDesc::default()
            })
            .unwrap();
        runtime
            .upsert_actor(actor("walker", [0.0, 0.0, 0.0]))
            .unwrap();
        runtime
            .upsert_nav_node(WorldNavNodeDesc {
                id: "a".to_owned(),
                position: [0.0, 0.0, 0.0],
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .upsert_nav_node(WorldNavNodeDesc {
                id: "b".to_owned(),
                position: [10.0, 0.0, 0.0],
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .upsert_nav_edge(WorldNavEdgeDesc {
                id: "ab".to_owned(),
                from: "a".to_owned(),
                to: "b".to_owned(),
                bidirectional: true,
                distance: None,
                cost_scale: 1.0,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        runtime
            .start_travel(WorldTravelRequestDesc {
                actor_id: "walker".to_owned(),
                start_node: Some("a".to_owned()),
                destination_node: "b".to_owned(),
                speed: 5.0,
                mode: "generic".to_owned(),
                payload: Value::Null,
            })
            .unwrap();

        for _ in 0..30 {
            runtime.tick_frame(0.1, &[]);
        }

        let state = runtime.runtime_state();
        let walker = state["simulation"]["actors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == "walker")
            .unwrap();
        assert!((walker["position"][0].as_f64().unwrap() - 10.0).abs() < 0.01);
        assert!(state["navigation"]["active_travel"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(state["reality"]["history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "world.travel.completed"));
    }
    #[test]
    fn observer_promotion_does_not_wait_for_background_actor_cadence() {
        let mut runtime = LivingWorldRuntime::default();
        runtime
            .configure_simulation(WorldSimulationPolicyDesc {
                background_interval_seconds: 30.0,
                ..WorldSimulationPolicyDesc::default()
            })
            .unwrap();
        runtime
            .upsert_actor(actor("actor.fast-promote", [1000.0, 0.0, 0.0]))
            .unwrap();

        runtime.tick_frame(0.1, &[]);
        let before = runtime.runtime_state();
        let next_background_update = before["simulation"]["actors"][0]["next_update_world_seconds"]
            .as_f64()
            .unwrap();
        assert!(next_background_update > 20.0);

        runtime.tick_frame(0.05, &[[1000.0, 0.0, 0.0]]);
        let after = runtime.runtime_state();
        let actor = &after["simulation"]["actors"][0];
        assert_eq!(actor["representation"], "physical");
        assert!(after["simulation"]["frame_actor_updates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|update| update["actor_id"] == "actor.fast-promote"));
    }
}

#[cfg(test)]
mod reality_tests;
