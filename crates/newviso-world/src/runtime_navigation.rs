use super::*;

impl LivingWorldRuntime {
    pub fn upsert_nav_node(&mut self, desc: WorldNavNodeDesc) -> Result<(), String> {
        desc.validate()?;
        self.nav_nodes.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_nav_node(&mut self, id: &str) -> Result<(), String> {
        let id = id.trim();
        if self
            .nav_edges
            .values()
            .any(|edge| edge.from == id || edge.to == id)
        {
            return Err(format!(
                "world navigation node '{id}' is still referenced by an edge"
            ));
        }
        if self
            .travel
            .values()
            .any(|travel| travel.route.iter().any(|node| node == id))
        {
            return Err(format!(
                "world navigation node '{id}' is still referenced by active travel"
            ));
        }
        self.nav_nodes.remove(id);
        Ok(())
    }

    pub fn upsert_nav_edge(&mut self, desc: WorldNavEdgeDesc) -> Result<(), String> {
        desc.validate()?;
        if !self.nav_nodes.contains_key(&desc.from) {
            return Err(format!(
                "world navigation edge '{}' references missing from node '{}'",
                desc.id, desc.from
            ));
        }
        if !self.nav_nodes.contains_key(&desc.to) {
            return Err(format!(
                "world navigation edge '{}' references missing to node '{}'",
                desc.id, desc.to
            ));
        }
        self.nav_edges.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_nav_edge(&mut self, id: &str) {
        self.nav_edges.remove(id.trim());
    }

    pub fn start_travel(&mut self, desc: WorldTravelRequestDesc) -> Result<(), String> {
        desc.validate()?;
        let actor_position = self
            .actors
            .get(&desc.actor_id)
            .ok_or_else(|| format!("world travel actor '{}' does not exist", desc.actor_id))?
            .desc
            .position;
        let start_node = match desc.start_node.as_deref() {
            Some(id) => {
                if !self.nav_nodes.contains_key(id) {
                    return Err(format!("world travel start node '{id}' does not exist"));
                }
                id.to_owned()
            }
            None => nearest_node(&self.nav_nodes, actor_position)
                .ok_or_else(|| "world navigation graph has no nodes".to_owned())?,
        };
        let route = shortest_route(
            &self.nav_nodes,
            &self.nav_edges,
            &start_node,
            &desc.destination_node,
        )?;
        let next_waypoint_index = if desc.start_node.is_some() {
            // An explicit start node is a routing assertion by the caller, not a
            // waypoint that the actor must physically revisit. This matters for
            // materialized characters whose collision-resolved feet position may
            // differ slightly in Y (or sit on another overlapping nav layer) from
            // the coarse graph anchor used to select the route.
            usize::from(route.len() > 1)
        } else {
            route
                .iter()
                .take_while(|node_id| {
                    self.nav_nodes.get(*node_id).is_some_and(|node| {
                        let dx = node.position[0] - actor_position[0];
                        let dy = node.position[1] - actor_position[1];
                        let dz = node.position[2] - actor_position[2];
                        dx * dx + dy * dy + dz * dz <= 1.0e-10
                    })
                })
                .count()
        };
        let actor_id = desc.actor_id.clone();
        self.travel.insert(
            actor_id.clone(),
            WorldTravelRecord {
                actor_id: actor_id.clone(),
                route,
                target_position: None,
                arrival_radius: 0.18,
                next_waypoint_index,
                destination_node: desc.destination_node.clone(),
                speed: desc.speed,
                mode: desc.mode.clone(),
                payload: desc.payload.clone(),
                started_world_seconds: self.clock.world_seconds,
                last_update_world_seconds: self.clock.world_seconds,
                distance_travelled: 0.0,
            },
        );
        self.push_reality_event(
            WorldRealityEventDesc {
                id: String::new(),
                kind: "world.travel.started".to_owned(),
                source: "world.navigation".to_owned(),
                cause: None,
                participants: vec![actor_id],
                position: Some(actor_position),
                importance: 1.0,
                tags: vec!["navigation".to_owned(), "travel".to_owned()],
                payload: json!({
                    "start_node": start_node,
                    "destination_node": desc.destination_node,
                    "mode": desc.mode,
                    "payload": desc.payload,
                }),
            },
            self.clock.world_seconds,
        );
        Ok(())
    }

    /// Coordinate goals do not allocate synthetic graph nodes. Nearby characters
    /// refine this segment through resident navmesh; distant actors use coarse travel.
    pub fn start_travel_to_position(
        &mut self,
        actor_id: &str,
        request_id: &str,
        position: [f32; 3],
        speed: f32,
        arrival_radius: f32,
    ) -> Result<(), String> {
        validate_id("world travel actor", actor_id)?;
        validate_id("world position request", request_id)?;
        if !self.actors.contains_key(actor_id) {
            return Err(format!("world actor '{actor_id}' does not exist"));
        }
        if position.iter().any(|n| !n.is_finite())
            || !speed.is_finite()
            || !(0.001..=10_000.0).contains(&speed)
            || !arrival_radius.is_finite()
            || !(0.01..=1000.0).contains(&arrival_radius)
        {
            return Err("invalid world coordinate travel request".into());
        }
        self.travel.insert(
            actor_id.to_owned(),
            WorldTravelRecord {
                actor_id: actor_id.to_owned(),
                route: Vec::new(),
                target_position: Some(position),
                arrival_radius,
                next_waypoint_index: 0,
                destination_node: request_id.to_owned(),
                speed,
                mode: if speed > 2.5 { "run" } else { "walk" }.to_owned(),
                payload: Value::Null,
                started_world_seconds: self.clock.world_seconds,
                last_update_world_seconds: self.clock.world_seconds,
                distance_travelled: 0.0,
            },
        );
        Ok(())
    }

    pub fn cancel_travel(&mut self, actor_id: &str) {
        let actor_id = actor_id.trim();
        self.travel.remove(actor_id);
        self.external_motion_velocities.remove(actor_id);
    }

    pub(super) fn advance_actor_travel(&mut self, actor_id: &str, step: WorldStep) {
        let Some(mut travel) = self.travel.remove(actor_id) else {
            return;
        };
        let Some(actor) = self.actors.get_mut(actor_id) else {
            return;
        };

        if let Some(target) = travel.target_position {
            let physical = self.external_motion_velocities.contains_key(actor_id);
            let delta_seconds = (step.world_seconds - travel.last_update_world_seconds).max(0.0);
            travel.last_update_world_seconds = step.world_seconds;
            if !physical {
                let (position, consumed, _) = advance_toward(
                    actor.desc.position,
                    target,
                    (delta_seconds * f64::from(travel.speed)) as f32,
                );
                actor.desc.position = position;
                travel.distance_travelled += f64::from(consumed);
            }
            let dx = target[0] - actor.desc.position[0];
            let dz = target[2] - actor.desc.position[2];
            let reached = (dx * dx + dz * dz).sqrt() <= travel.arrival_radius
                && (target[1] - actor.desc.position[1]).abs()
                    <= if physical { 1.0 } else { travel.arrival_radius };
            if !reached {
                self.travel.insert(actor_id.to_owned(), travel);
                return;
            }
            let position = actor.desc.position;
            let completion = WorldTravelCompletion {
                actor_id: actor_id.to_owned(),
                destination_node: travel.destination_node,
                mode: travel.mode,
                payload: travel.payload,
                world_seconds: step.world_seconds,
                distance_travelled: travel.distance_travelled,
            };
            self.frame_travel_completions.push(completion.clone());
            self.push_reality_event(WorldRealityEventDesc { id: String::new(),
                kind: "world.travel.completed".into(), source: "world.navigation".into(), cause: None,
                participants: vec![actor_id.to_owned()], position: Some(position), importance: 1.0,
                tags: vec!["navigation".into(), "travel".into()],
                payload: json!({"destination_node": completion.destination_node, "target_position": target,
                    "mode": completion.mode, "distance_travelled": completion.distance_travelled}),
            }, step.world_seconds);
            return;
        }

        if self.external_motion_velocities.contains_key(actor_id) {
            travel.last_update_world_seconds = step.world_seconds;
            while travel.next_waypoint_index < travel.route.len() {
                let node_id = &travel.route[travel.next_waypoint_index];
                let Some(node) = self.nav_nodes.get(node_id) else {
                    break;
                };
                let dx = node.position[0] - actor.desc.position[0];
                let dy = node.position[1] - actor.desc.position[1];
                let dz = node.position[2] - actor.desc.position[2];
                let distance = (dx * dx + dy * dy + dz * dz).sqrt();
                if distance > 0.18 {
                    break;
                }
                travel.distance_travelled += f64::from(distance);
                actor.desc.position = node.position;
                travel.next_waypoint_index += 1;
            }

            if travel.next_waypoint_index < travel.route.len() {
                self.travel.insert(actor_id.to_owned(), travel);
                return;
            }

            let position = actor.desc.position;
            self.complete_actor_travel(actor_id, travel, position, step.world_seconds);
            return;
        }

        // A new route must not consume time from before its own start.
        let delta_seconds = (step.world_seconds - travel.last_update_world_seconds).max(0.0);
        travel.last_update_world_seconds = step.world_seconds;
        let mut budget = (delta_seconds * f64::from(travel.speed)) as f32;
        let mut completed = false;

        while budget > 0.0 && travel.next_waypoint_index < travel.route.len() {
            let node_id = &travel.route[travel.next_waypoint_index];
            let Some(node) = self.nav_nodes.get(node_id) else {
                break;
            };
            let (next_position, consumed, reached) =
                advance_toward(actor.desc.position, node.position, budget);
            actor.desc.position = next_position;
            travel.distance_travelled += f64::from(consumed);
            budget = (budget - consumed).max(0.0);
            if reached {
                travel.next_waypoint_index += 1;
            } else {
                break;
            }
        }

        if travel.next_waypoint_index >= travel.route.len() {
            completed = true;
        }

        if completed {
            let position = actor.desc.position;
            self.complete_actor_travel(actor_id, travel, position, step.world_seconds);
        } else {
            self.travel.insert(actor_id.to_owned(), travel);
        }
    }
    fn complete_actor_travel(
        &mut self,
        actor_id: &str,
        travel: WorldTravelRecord,
        position: [f32; 3],
        world_seconds: f64,
    ) {
        self.external_motion_velocities.remove(actor_id);
        let completion = WorldTravelCompletion {
            actor_id: actor_id.to_owned(),
            destination_node: travel.destination_node.clone(),
            mode: travel.mode.clone(),
            payload: travel.payload.clone(),
            world_seconds,
            distance_travelled: travel.distance_travelled,
        };
        self.frame_travel_completions.push(completion.clone());
        self.push_reality_event(
            WorldRealityEventDesc {
                id: String::new(),
                kind: "world.travel.completed".to_owned(),
                source: "world.navigation".to_owned(),
                cause: None,
                participants: vec![actor_id.to_owned()],
                position: Some(position),
                importance: 1.0,
                tags: vec!["navigation".to_owned(), "travel".to_owned()],
                payload: json!({
                    "destination_node": completion.destination_node,
                    "mode": completion.mode,
                    "distance_travelled": completion.distance_travelled,
                    "started_world_seconds": travel.started_world_seconds,
                    "payload": completion.payload,
                }),
            },
            world_seconds,
        );
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    #[test]
    fn coarse_and_physical_travel_commit_completion_once_with_payload() {
        for physical in [false, true] {
            let mut world = LivingWorldRuntime::default();
            world
                .upsert_actor(WorldActorDesc {
                    id: "walker".into(),
                    kind: "generic".into(),
                    position: [0.0; 3],
                    group: None,
                    channel: None,
                    enabled: true,
                    tags: Vec::new(),
                    parameters: BTreeMap::new(),
                    state: Value::Null,
                })
                .unwrap();
            for (id, position) in [("start", [0.0; 3]), ("end", [1.0, 0.0, 0.0])] {
                world
                    .upsert_nav_node(WorldNavNodeDesc {
                        id: id.into(),
                        position,
                        tags: Vec::new(),
                        parameters: BTreeMap::new(),
                    })
                    .unwrap();
            }
            world
                .upsert_nav_edge(WorldNavEdgeDesc {
                    id: "route".into(),
                    from: "start".into(),
                    to: "end".into(),
                    bidirectional: true,
                    distance: None,
                    cost_scale: 1.0,
                    enabled: true,
                    tags: Vec::new(),
                    parameters: BTreeMap::new(),
                })
                .unwrap();
            world
                .start_travel(WorldTravelRequestDesc {
                    actor_id: "walker".into(),
                    start_node: Some("start".into()),
                    destination_node: "end".into(),
                    speed: 1.0,
                    mode: "walk".into(),
                    payload: json!({"order": 7}),
                })
                .unwrap();
            if physical {
                world
                    .set_actor_external_motion("walker", [1.0, 0.0, 0.0], [1.0, 0.0, 0.0])
                    .unwrap();
            }
            world.advance_actor_travel(
                "walker",
                WorldStep {
                    world_seconds: 1.0,
                    fixed_tick: 20,
                },
            );
            world.advance_actor_travel(
                "walker",
                WorldStep {
                    world_seconds: 1.05,
                    fixed_tick: 21,
                },
            );
            assert!(world.travel.is_empty());
            assert!(world.external_motion_velocities.is_empty());
            assert_eq!(world.frame_travel_completions.len(), 1);
            let completion = &world.frame_travel_completions[0];
            assert_eq!(completion.destination_node, "end");
            assert_eq!(completion.world_seconds, 1.0);
            assert_eq!(completion.payload, json!({"order": 7}));
            assert_eq!(
                world
                    .reality_events
                    .iter()
                    .filter(|event| event.desc.kind == "world.travel.completed")
                    .count(),
                1
            );
        }
    }
}
