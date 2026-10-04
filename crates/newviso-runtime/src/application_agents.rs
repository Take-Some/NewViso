use super::*;
use newviso_agent::{AgentActorView, AgentCommand, AgentStimulusView, AgentWorldSnapshot};

impl EngineApplication {
    pub(super) fn tick_agents(&mut self, dt: f32) -> Result<(), String> {
        let snapshot = AgentWorldSnapshot {
            world_seconds: self.living_world.world_seconds(),
            actors: self
                .living_world
                .actor_runtime_views()
                .into_iter()
                .map(|actor| {
                    let enabled = actor.enabled && !self.peds.is_dead(&actor.id);
                    AgentActorView {
                    id: actor.id,
                    position: actor.position,
                    enabled,
                    simulation_tier: actor.simulation_tier.to_owned(),
                    travel_destination: actor.travel_destination,
                }})
                .collect(),
            stimuli: self
                .living_world
                .stimulus_runtime_views()
                .into_iter()
                .map(|stimulus| AgentStimulusView {
                    id: stimulus.id,
                    kind: stimulus.kind,
                    source: stimulus.source,
                    position: stimulus.position,
                    radius: stimulus.radius,
                    intensity: stimulus.intensity,
                    remaining_seconds: stimulus.remaining_seconds,
                    tags: stimulus.tags,
                    payload: stimulus.payload,
                })
                .collect(),
        };

        self.peds.tick(&snapshot, &mut self.agents)?;
        let commands = self.agents.tick(dt, &snapshot);
        for command in commands {
            if let AgentCommand::Attack { actor_id, target_actor, damage, range } = command {
                self.ped_attack(&actor_id, &target_actor, damage, range)?;
            } else {
                let actor_id = match &command {
                    AgentCommand::StartTravel { actor_id, .. } | AgentCommand::CancelTravel { actor_id }
                    | AgentCommand::MoveToPosition { actor_id, .. } => actor_id.clone(),
                    AgentCommand::Attack { .. } => unreachable!(),
                };
                if let Err(error) = apply_agent_commands(&mut self.living_world, vec![command]) {
                    self.agents.fail_active(&actor_id);
                    host::warn("newviso.agents", format!("actor '{actor_id}' task failed: {error}"));
                }
            }
        }
        for event in self.agents.frame_events() {
            host::publish_event_json("agent.task.lifecycle", "newviso.agents", event.clone())?;
        }
        for event in self.peds.drain_events() {
            let kind = event.get("kind").and_then(Value::as_str).unwrap_or("ped.event");
            host::publish_event_json(kind, "newviso.peds", event.clone())?;
        }
        Ok(())
    }
}

pub(super) fn apply_agent_commands(
    world: &mut LivingWorldRuntime,
    commands: Vec<AgentCommand>,
) -> Result<(), String> {
    for command in commands {
        match command {
            AgentCommand::StartTravel {
                actor_id,
                start_node,
                destination_node,
                speed,
                mode,
                payload,
            } => {
                world.start_travel(WorldTravelRequestDesc {
                    actor_id,
                    start_node,
                    destination_node,
                    speed,
                    mode,
                    payload,
                })?;
            }
            AgentCommand::MoveToPosition { actor_id, request_id, position, speed, stop_distance } => {
                world.start_travel_to_position(&actor_id, &request_id, position, speed, stop_distance)?;
            }
            AgentCommand::Attack { .. } => {}
            AgentCommand::CancelTravel { actor_id } => {
                world.cancel_travel(&actor_id);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use newviso_agent::{
        AgentDesc, AgentPerceptionPolicy, AgentRuntime, AgentTaskDesc, AgentTaskKind, AgentTaskLane,
    };

    fn snapshot(world: &LivingWorldRuntime) -> AgentWorldSnapshot {
        AgentWorldSnapshot {
            world_seconds: world.world_seconds(),
            actors: world
                .actor_runtime_views()
                .into_iter()
                .map(|actor| AgentActorView {
                    id: actor.id,
                    position: actor.position,
                    enabled: actor.enabled,
                    simulation_tier: actor.simulation_tier.to_owned(),
                    travel_destination: actor.travel_destination,
                })
                .collect(),
            stimuli: Vec::new(),
        }
    }

    #[test]
    fn agent_travel_command_drives_living_world_actor() {
        let mut world = LivingWorldRuntime::default();
        world
            .configure_simulation(WorldSimulationPolicyDesc {
                full_interval_seconds: 0.05,
                reduced_interval_seconds: 0.05,
                background_interval_seconds: 0.05,
                ..WorldSimulationPolicyDesc::default()
            })
            .unwrap();
        world
            .upsert_actor(WorldActorDesc {
                id: "ped.1".to_owned(),
                kind: "pedestrian".to_owned(),
                position: [0.0, 0.0, 0.0],
                group: None,
                channel: None,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
                state: Value::Null,
            })
            .unwrap();
        for (id, position) in [("start", [0.0, 0.0, 0.0]), ("goal", [5.0, 0.0, 0.0])] {
            world
                .upsert_nav_node(WorldNavNodeDesc {
                    id: id.to_owned(),
                    position,
                    tags: Vec::new(),
                    parameters: BTreeMap::new(),
                })
                .unwrap();
        }
        world
            .upsert_nav_edge(WorldNavEdgeDesc {
                id: "start-goal".to_owned(),
                from: "start".to_owned(),
                to: "goal".to_owned(),
                bidirectional: true,
                distance: None,
                cost_scale: 1.0,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();

        let mut agents = AgentRuntime::default();
        let _ = agents
            .upsert_agent(AgentDesc {
                id: "ped.ai.1".to_owned(),
                actor_id: "ped.1".to_owned(),
                enabled: true,
                perception: AgentPerceptionPolicy::default(),
                thinking: newviso_agent::AgentThinkingPolicy {
                    full_interval_seconds: 0.05,
                    reduced_interval_seconds: 0.05,
                    background_interval_seconds: 0.05,
                },
                blackboard: BTreeMap::new(),
            })
            .unwrap();
        agents
            .set_task(
                "ped.ai.1",
                AgentTaskDesc {
                    id: "walk.goal".to_owned(),
                    lane: AgentTaskLane::Movement,
                    priority: 0,
                    task: AgentTaskKind::TravelToNode {
                        start_node: Some("start".to_owned()),
                        destination_node: "goal".to_owned(),
                        speed: 2.5,
                        mode: "walk".to_owned(),
                        payload: Value::Null,
                    },
                },
            )
            .unwrap();

        let commands = agents.tick(0.05, &snapshot(&world));
        apply_agent_commands(&mut world, commands).unwrap();

        for _ in 0..60 {
            world.tick_frame(0.05, &[]);
            let commands = agents.tick(0.05, &snapshot(&world));
            apply_agent_commands(&mut world, commands).unwrap();
        }

        let actor = world
            .actor_runtime_views()
            .into_iter()
            .find(|actor| actor.id == "ped.1")
            .unwrap();
        assert!((actor.position[0] - 5.0).abs() < 0.01);
        assert!(actor.travel_destination.is_none());
        assert_eq!(
            agents.runtime_state()["agents"][0]["tasks"][0]["status"],
            "completed"
        );
    }
}
