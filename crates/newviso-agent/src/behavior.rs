use super::*;

#[derive(Clone, Debug, Default, Serialize)]
pub struct AgentPresentationIntent {
    pub heading_degrees: Option<f32>,
    pub clip_ref: Option<String>,
}

impl RuntimeTask {
    pub(super) fn new(desc: AgentTaskDesc) -> Self {
        Self { desc, status: AgentTaskStatus::Pending, elapsed_seconds: 0.0,
            goal: None, origin: None, next_action_seconds: 0.0, iteration: 0, child: None }
    }

    pub(super) fn presentation(&self, position: [f32; 3]) -> AgentPresentationIntent {
        if matches!(self.status, AgentTaskStatus::Completed | AgentTaskStatus::Failed) {
            return AgentPresentationIntent::default();
        }
        match &self.desc.task {
            AgentTaskKind::LookAt { position: target, .. } => AgentPresentationIntent {
                heading_degrees: heading(position, *target), clip_ref: None,
            },
            AgentTaskKind::PlayAnimation { clip_ref, .. } => AgentPresentationIntent {
                heading_degrees: None, clip_ref: Some(clip_ref.clone()),
            },
            AgentTaskKind::Combat { .. } => AgentPresentationIntent {
                heading_degrees: self.goal.and_then(|target| heading(position, target)), clip_ref: None,
            },
            AgentTaskKind::Sequence { .. } => self.child.as_deref()
                .map(|child| child.presentation(position)).unwrap_or_default(),
            _ => AgentPresentationIntent::default(),
        }
    }
}

fn heading(a: [f32; 3], b: [f32; 3]) -> Option<f32> {
    let dx = b[0] - a[0]; let dz = b[2] - a[2];
    (dx * dx + dz * dz > 1.0e-8).then(|| dx.atan2(dz).to_degrees())
}
fn distance_xz(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
fn cancel(task: &mut RuntimeTask, actor: &AgentActorView, commands: &mut Vec<AgentCommand>) {
    if task.goal.take().is_some() || actor.travel_destination.is_some() {
        commands.push(AgentCommand::CancelTravel { actor_id: actor.id.clone() });
    }
}
fn move_to(task: &mut RuntimeTask, actor: &AgentActorView, goal: [f32; 3], speed: f32,
           stop_distance: f32, commands: &mut Vec<AgentCommand>) {
    commands.push(AgentCommand::MoveToPosition { actor_id: actor.id.clone(),
        request_id: task.desc.id.clone(), position: goal, speed, stop_distance });
    task.goal = Some(goal);
}

pub(super) fn advance_task(task: &mut RuntimeTask, actor: &AgentActorView,
    actors: &BTreeMap<&str, &AgentActorView>, dt: f32, commands: &mut Vec<AgentCommand>) {
    let starting = task.status == AgentTaskStatus::Pending;
    task.status = AgentTaskStatus::Running;
    task.elapsed_seconds += dt;
    // Clone the small descriptor to permit mutation of execution state.
    match task.desc.task.clone() {
        AgentTaskKind::Idle => {}
        AgentTaskKind::Wait { duration_seconds } | AgentTaskKind::LookAt { duration_seconds, .. }
        | AgentTaskKind::PlayAnimation { duration_seconds, .. } => {
            if task.elapsed_seconds >= duration_seconds { task.status = AgentTaskStatus::Completed; }
        }
        AgentTaskKind::TravelToNode { start_node, destination_node, speed, mode, payload } => {
            if starting {
                if actor.travel_destination.as_deref() != Some(destination_node.as_str()) {
                    commands.push(AgentCommand::StartTravel { actor_id: actor.id.clone(),
                        start_node, destination_node, speed, mode, payload });
                }
            } else if actor.travel_destination.is_none() { task.status = AgentTaskStatus::Completed; }
            else if actor.travel_destination.as_deref() != Some(destination_node.as_str()) {
                task.status = AgentTaskStatus::Failed;
            }
        }
        AgentTaskKind::GoToPosition { position, speed, stop_distance } => {
            if distance_xz(actor.position, position) <= stop_distance
                && (actor.position[1] - position[1]).abs() <= 1.0 {
                cancel(task, actor, commands); task.status = AgentTaskStatus::Completed;
            } else if starting || actor.travel_destination.is_none() {
                move_to(task, actor, position, speed, stop_distance, commands);
            }
        }
        AgentTaskKind::FollowActor { target_actor, speed, distance, repath_seconds } => {
            let Some(target) = actors.get(target_actor.as_str()).filter(|target| target.enabled) else {
                cancel(task, actor, commands); task.status = AgentTaskStatus::Failed; return;
            };
            let separation = distance_xz(actor.position, target.position);
            if separation <= distance {
                cancel(task, actor, commands);
            } else if starting || task.elapsed_seconds >= task.next_action_seconds {
                let ratio = (separation - distance * 0.9) / separation;
                let goal = [actor.position[0] + (target.position[0] - actor.position[0]) * ratio,
                    target.position[1], actor.position[2] + (target.position[2] - actor.position[2]) * ratio];
                if actor.travel_destination.is_none() || task.goal.is_none_or(|p| distance_xz(p, goal) > 0.35) {
                    move_to(task, actor, goal, speed, 0.2, commands);
                }
                task.next_action_seconds = task.elapsed_seconds + repath_seconds;
            }
        }
        AgentTaskKind::Flee { threat_position, threat_actor, speed, safe_distance, duration_seconds } => {
            let threat = threat_actor.as_deref().and_then(|id| actors.get(id))
                .map(|target| target.position).unwrap_or(threat_position);
            if task.elapsed_seconds >= duration_seconds || distance_xz(actor.position, threat) >= safe_distance {
                cancel(task, actor, commands); task.status = AgentTaskStatus::Completed;
            } else if starting || task.elapsed_seconds >= task.next_action_seconds {
                let dx = actor.position[0] - threat[0]; let dz = actor.position[2] - threat[2];
                let length = (dx * dx + dz * dz).sqrt();
                let (x, z) = if length > 0.001 { (dx / length, dz / length) }
                    else { let a = stable_seed(&actor.id) as f32 * 0.0001; (a.sin(), a.cos()) };
                move_to(task, actor, [threat[0] + x * (safe_distance + 1.0), actor.position[1],
                    threat[2] + z * (safe_distance + 1.0)], speed, 0.25, commands);
                task.next_action_seconds = task.elapsed_seconds + 0.5;
            }
        }
        AgentTaskKind::Wander { center, radius, speed, pause_seconds } => {
            let origin = *task.origin.get_or_insert(center.unwrap_or(actor.position));
            if starting { task.goal = None; }
            if task.goal.is_some_and(|p| distance_xz(p, actor.position) <= 0.3) {
                cancel(task, actor, commands);
                task.next_action_seconds = task.elapsed_seconds + pause_seconds;
            }
            if task.goal.is_none() && task.elapsed_seconds >= task.next_action_seconds {
                task.iteration += 1;
                let seed = stable_seed(&actor.id).wrapping_add(task.iteration.wrapping_mul(0x9e3779b97f4a7c15));
                let unit = (seed % 100_003) as f32 / 100_003.0;
                let angle = unit * std::f32::consts::TAU;
                let r = radius * (0.3 + 0.7 * ((seed >> 20) % 1009) as f32 / 1009.0).sqrt();
                move_to(task, actor, [origin[0] + angle.sin() * r, origin[1], origin[2] + angle.cos() * r],
                    speed, 0.25, commands);
            } else if task.goal.is_some() && actor.travel_destination.is_none() {
                move_to(task, actor, task.goal.unwrap(), speed, 0.25, commands);
            }
        }
        AgentTaskKind::Sequence { steps, repeat } => {
            let index = task.iteration as usize;
            if index >= steps.len() {
                if repeat { task.iteration = 0; } else { task.status = AgentTaskStatus::Completed; return; }
            }
            if task.child.is_none() {
                task.child = Some(Box::new(RuntimeTask::new(AgentTaskDesc {
                    id: task.desc.id.clone(), lane: task.desc.lane, priority: task.desc.priority,
                    task: steps[task.iteration as usize].clone(),
                })));
            }
            let child = task.child.as_mut().unwrap();
            if starting && child.desc.task.owns_movement() { child.status = AgentTaskStatus::Pending; }
            advance_task(child, actor, actors, dt, commands);
            if child.status == AgentTaskStatus::Failed { task.status = AgentTaskStatus::Failed; }
            else if child.status == AgentTaskStatus::Completed {
                task.child = None; task.iteration += 1;
                if task.iteration as usize == steps.len() && !repeat { task.status = AgentTaskStatus::Completed; }
            }
        }
        AgentTaskKind::Combat { target_actor, speed, range, cooldown_seconds, damage, duration_seconds } => {
            let Some(target) = actors.get(target_actor.as_str()).filter(|target| target.enabled) else {
                cancel(task, actor, commands); task.status = AgentTaskStatus::Completed; return;
            };
            if task.elapsed_seconds >= duration_seconds {
                cancel(task, actor, commands); task.status = AgentTaskStatus::Completed; return;
            }
            if distance_xz(actor.position, target.position) > range * 0.9 {
                if starting || task.elapsed_seconds >= task.next_action_seconds {
                    move_to(task, actor, target.position, speed, 0.5, commands);
                    task.next_action_seconds = task.elapsed_seconds + 0.5;
                }
            } else {
                cancel(task, actor, commands);
                if starting || task.elapsed_seconds >= task.next_action_seconds {
                    commands.push(AgentCommand::Attack { actor_id: actor.id.clone(),
                        target_actor: target_actor.clone(), damage, range });
                    task.next_action_seconds = task.elapsed_seconds + cooldown_seconds;
                }
                task.goal = Some(target.position);
            }
        }
    }
}

pub(super) fn stable_seed(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3))
}
