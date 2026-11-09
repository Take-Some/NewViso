use super::*;
#[derive(Clone, Copy, Debug)]
pub(crate) struct DoorPassage {
    pub entity: u64,
    pub target: [f32; 3],
    pub wait: bool,
    pub denied: bool,
    pub phase: &'static str,
    pub facing: Option<[f32; 3]>,
}
impl PhysicsRuntime {
    pub(crate) fn door_navigation_gates(&self) -> Vec<TraversalGate> {
        let now = self.fixed_seconds;
        self.doors
            .entries
            .iter()
            .filter_map(|(id, b)| {
                if !b.definition.navigation {
                    return None;
                }
                let g = b.geometry.as_ref()?;
                if g.normal[1].abs() > 0.001 || g.tangent[1].abs() > 0.001 {
                    return None;
                }
                Some(TraversalGate {
                    id: id.to_string(),
                    center: g.center,
                    normal: g.normal,
                    half_width: (g.half[0] - b.mechanism.config.passage_margin).max(f32::EPSILON),
                    half_height: g.half[1],
                    locked: b.mechanism.state.locked && !b.mechanism.state.bolt_broken,
                    free_egress_side: b.mechanism.config.free_egress_side,
                    blocked: now < b.mechanism.state.jammed_until,
                    action_cost: if b.mechanism.state.latch_engaged || b.mechanism.state.locked {
                        b.mechanism.config.hold_seconds
                    } else {
                        0.0
                    },
                })
            })
            .collect()
    }
    pub(crate) fn plan_door_passage(
        &mut self,
        actor: DoorActor,
        feet: [f32; 3],
        waypoint: [f32; 3],
    ) -> Option<DoorPassage> {
        let now = self.fixed_seconds;
        let id = self
            .doors
            .entries
            .iter()
            .filter_map(|(id, b)| {
                let g = b.geometry.as_ref()?;
                if !b.definition.navigation {
                    return None;
                }
                let owned = b.queue.iter().any(|r| r.actor == actor.id);
                let gate = TraversalGate {
                    id: id.to_string(),
                    center: g.center,
                    normal: g.normal,
                    half_width: g.half[0],
                    half_height: g.half[1],
                    locked: false,
                    free_egress_side: None,
                    blocked: false,
                    action_cost: 0.0,
                };
                if owned || gate.crossing(feet, waypoint, actor.radius).is_some() {
                    let d = (feet[0] - g.center[0]).hypot(feet[2] - g.center[2]);
                    (d <= g.width() + b.mechanism.config.interaction_reach).then_some((*id, d))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|x| x.0)?;
        let mut b = self.doors.entries.remove(&id)?;
        let decision = (|| {
            let g = b.geometry.as_ref()?;
            let physical = self.bodies.get(&actor.body)?;
            let leaf = self.bodies.get(&id)?;
            let existing = b.queue.iter().find(|r| r.actor == actor.id).cloned();
            let side = existing
                .as_ref()
                .map_or(g.side(physical.position), |r| r.side);
            let point = g.crossing_point(feet[1]);
            let stand = actor.radius + g.half[2] + b.mechanism.config.passage_margin;
            let approach = dm::add(point, dm::mul(g.normal, side * stand));
            let exit = dm::add(point, dm::mul(g.normal, -side * (stand + actor.radius)));
            let signed = dm::dot(dm::sub(physical.position, g.center), g.normal) * side;
            let authorized = b.mechanism.config.access(side, actor.access.contains(&id));
            let narrow = g.width() < 2.0 * (actor.radius + b.mechanism.config.passage_margin)
                || 2.0 * g.half[1] < actor.height + b.mechanism.config.passage_margin;
            if existing.is_some() && signed < -stand {
                b.queue.retain(|r| r.actor != actor.id);
                self.doors.event(
                    id,
                    Some(&actor.id),
                    "passed",
                    Some(dm::DoorResult::Accepted),
                    point,
                );
                return None;
            }
            if narrow
                || b.mechanism.state.locked && !authorized
                || now < b.mechanism.state.jammed_until
            {
                return Some(DoorPassage {
                    entity: id,
                    target: approach,
                    wait: true,
                    denied: true,
                    phase: "unavailable",
                    facing: None,
                });
            }
            if existing.is_none() {
                self.doors.next_ticket = self.doors.next_ticket.wrapping_add(1).max(1);
                b.queue.push_back(PassageRequest::new(
                    &actor,
                    side,
                    self.doors.next_ticket,
                    now,
                    physical.position,
                ));
                self.doors.event(id, Some(&actor.id), "queued", None, point);
            }
            if let Some(r) = b.queue.iter_mut().find(|r| r.actor == actor.id) {
                r.seen = now;
            }
            let distance = (feet[0] - approach[0]).hypot(feet[2] - approach[2]);
            if b.queue.front().is_none_or(|r| r.actor != actor.id) {
                return Some(DoorPassage {
                    entity: id,
                    target: approach,
                    wait: distance <= stand,
                    denied: false,
                    phase: "queued",
                    facing: None,
                });
            }
            if b.mechanism.state.goal.is_none()
                && dm::sub(physical.position, g.handle(leaf.position, leaf.rotation))
                    .iter()
                    .map(|v| v * v)
                    .sum::<f32>()
                    .sqrt()
                    > b.mechanism.config.interaction_reach
            {
                return Some(DoorPassage {
                    entity: id,
                    target: approach,
                    wait: false,
                    denied: false,
                    phase: "approach",
                    facing: None,
                });
            }
            if let Some(angle) = g.target(side, b.mechanism.config.open_angle_radians) {
                if b.mechanism
                    .state
                    .goal
                    .is_some_and(|goal| (goal.angle - angle).abs() < 0.001)
                {
                    b.mechanism.state.handle_until = now + b.mechanism.config.hold_seconds as f64;
                    let goal = b.mechanism.state.goal.as_mut().unwrap();
                    goal.until = if goal.reached {
                        b.mechanism.state.handle_until
                    } else {
                        goal.started + b.mechanism.config.operation_timeout_seconds as f64
                    };
                } else {
                    let o = dm::Observation {
                        seconds: now,
                        angle: b.angle,
                        angular_speed: b.angular_speed,
                        edge_radius: g.width(),
                        occupied: b.occupied,
                        ..Default::default()
                    };
                    let result = b.mechanism.operate(
                        dm::Operation::Open {
                            target_angle: angle,
                        },
                        o,
                        authorized,
                    );
                    self.doors.action(id, &actor.id, "open", result, point);
                }
            }
            let clear = g.passage_clear(leaf, actor.radius, b.mechanism.config.passage_margin);
            let desired = -g.pivot[0].signum() * side;
            let pulling = g
                .target(side, b.mechanism.config.open_angle_radians)
                .is_some_and(|a| a.signum() != desired);
            let retreat = dm::add(
                point,
                dm::mul(
                    g.normal,
                    side * (g.width() + actor.radius + b.mechanism.config.passage_margin),
                ),
            );
            Some(DoorPassage {
                entity: id,
                target: if clear {
                    exit
                } else if pulling {
                    retreat
                } else {
                    approach
                },
                wait: !clear && !pulling && distance <= actor.radius,
                denied: false,
                phase: if clear {
                    "crossing"
                } else if pulling {
                    "pulling"
                } else {
                    "opening"
                },
                facing: (!clear).then(|| g.handle(leaf.position, leaf.rotation)),
            })
        })();
        self.doors.entries.insert(id, b);
        decision
    }
}
