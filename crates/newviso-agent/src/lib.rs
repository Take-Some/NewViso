use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[cfg(test)]
mod ped_tests;
mod tasks;
mod behavior;
pub mod peds;
pub use tasks::AgentTaskKind;
pub use behavior::AgentPresentationIntent;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentPerceptionPolicy {
    pub radius: f32,
    pub memory_seconds: f32,
    pub max_memories: usize,
}

impl Default for AgentPerceptionPolicy {
    fn default() -> Self {
        Self {
            radius: 35.0,
            memory_seconds: 8.0,
            max_memories: 32,
        }
    }
}

impl AgentPerceptionPolicy {
    fn validate(&self) -> Result<(), String> {
        if !self.radius.is_finite()
            || !(0.0..=100_000.0).contains(&self.radius)
            || !self.memory_seconds.is_finite()
            || !(0.0..=86_400.0).contains(&self.memory_seconds)
            || !(1..=4096).contains(&self.max_memories)
        {
            return Err("invalid agent perception policy".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentThinkingPolicy {
    pub full_interval_seconds: f32,
    pub reduced_interval_seconds: f32,
    pub background_interval_seconds: f32,
}

impl Default for AgentThinkingPolicy {
    fn default() -> Self {
        Self {
            full_interval_seconds: 0.05,
            reduced_interval_seconds: 0.25,
            background_interval_seconds: 2.0,
        }
    }
}

impl AgentThinkingPolicy {
    fn validate(&self) -> Result<(), String> {
        if !self.full_interval_seconds.is_finite()
            || !(0.001..=86_400.0).contains(&self.full_interval_seconds)
            || !self.reduced_interval_seconds.is_finite()
            || !(self.full_interval_seconds..=86_400.0).contains(&self.reduced_interval_seconds)
            || !self.background_interval_seconds.is_finite()
            || !(self.reduced_interval_seconds..=86_400.0)
                .contains(&self.background_interval_seconds)
        {
            return Err("invalid agent thinking policy".to_owned());
        }
        Ok(())
    }

    fn interval_for_tier(&self, tier: &str) -> f32 {
        match tier {
            "full" => self.full_interval_seconds,
            "reduced" => self.reduced_interval_seconds,
            _ => self.background_interval_seconds,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentDesc {
    pub id: String,
    pub actor_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub perception: AgentPerceptionPolicy,
    #[serde(default)]
    pub thinking: AgentThinkingPolicy,
    #[serde(default)]
    pub blackboard: BTreeMap<String, Value>,
}

impl AgentDesc {
    pub fn validate(&self) -> Result<(), String> {
        validate_id("agent", &self.id)?;
        validate_id("agent actor", &self.actor_id)?;
        self.perception.validate()?;
        self.thinking.validate()?;
        for (key, value) in &self.blackboard {
            validate_id("agent blackboard key", key)?;
            validate_payload(value)?;
        }
        Ok(())
    }
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskLane {
    Ambient,
    Movement,
    Primary,
    Reaction,
}

impl AgentTaskLane {
    fn rank(self) -> u8 {
        match self {
            Self::Ambient => 0,
            Self::Movement => 1,
            Self::Primary => 2,
            Self::Reaction => 3,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentTaskDesc {
    pub id: String,
    pub lane: AgentTaskLane,
    #[serde(default)]
    pub priority: i32,
    pub task: AgentTaskKind,
}

impl AgentTaskDesc {
    pub fn validate(&self) -> Result<(), String> {
        validate_id("agent task", &self.id)?;
        self.task.validate_at_depth(0)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentActorView {
    pub id: String,
    pub position: [f32; 3],
    pub enabled: bool,
    pub simulation_tier: String,
    pub travel_destination: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentStimulusView {
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentWorldSnapshot {
    pub world_seconds: f64,
    pub actors: Vec<AgentActorView>,
    pub stimuli: Vec<AgentStimulusView>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentMemory {
    pub stimulus_id: String,
    pub kind: String,
    pub source: String,
    pub position: [f32; 3],
    pub intensity: f32,
    pub first_seen_world_seconds: f64,
    pub last_seen_world_seconds: f64,
    pub expires_world_seconds: f64,
    pub tags: Vec<String>,
    pub payload: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentCommand {
    StartTravel {
        actor_id: String,
        start_node: Option<String>,
        destination_node: String,
        speed: f32,
        mode: String,
        payload: Value,
    },
    CancelTravel { actor_id: String },
    MoveToPosition { actor_id: String, request_id: String, position: [f32; 3], speed: f32, stop_distance: f32 },
    Attack { actor_id: String, target_actor: String, damage: f32, range: f32 },
}

#[derive(Clone, Debug)]
struct RuntimeTask {
    desc: AgentTaskDesc,
    status: AgentTaskStatus,
    elapsed_seconds: f32,
    goal: Option<[f32; 3]>,
    origin: Option<[f32; 3]>,
    next_action_seconds: f32,
    iteration: u64,
    child: Option<Box<RuntimeTask>>,
}

#[derive(Clone, Debug)]
struct AgentRecord {
    desc: AgentDesc,
    tasks: BTreeMap<String, RuntimeTask>,
    memories: BTreeMap<String, AgentMemory>,
    active_task: Option<String>,
    next_think_world_seconds: f64,
    last_think_world_seconds: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct AgentRuntime {
    agents: BTreeMap<String, AgentRecord>,
    frame_commands: Vec<AgentCommand>,
    pending_commands: Vec<AgentCommand>,
    frame_events: Vec<Value>,
}

impl AgentRuntime {
    pub fn upsert_agent(&mut self, desc: AgentDesc) -> Result<Option<AgentCommand>, String> {
        desc.validate()?;
        if let Some(record) = self.agents.get_mut(&desc.id) {
            let actor_changed = record.desc.actor_id != desc.actor_id;
            let disabling = record.desc.enabled && !desc.enabled;
            let cancel = if actor_changed || disabling {
                suspend_active_travel(record)
            } else {
                None
            };

            if actor_changed {
                record.tasks.clear();
                record.memories.clear();
                record.active_task = None;
            }

            record.desc = desc;
            record.next_think_world_seconds = 0.0;
            record.last_think_world_seconds = None;
            Ok(cancel)
        } else {
            self.agents.insert(
                desc.id.clone(),
                AgentRecord {
                    desc,
                    tasks: BTreeMap::new(),
                    memories: BTreeMap::new(),
                    active_task: None,
                    next_think_world_seconds: 0.0,
                    last_think_world_seconds: None,
                },
            );
            Ok(None)
        }
    }

    pub fn remove_agent(&mut self, id: &str) -> Option<AgentCommand> {
        let record = self.agents.remove(id.trim())?;
        active_travel_cancel(&record)
    }

    pub fn set_task(&mut self, agent_id: &str, desc: AgentTaskDesc) -> Result<(), String> {
        desc.validate()?;
        let record = self
            .agents
            .get_mut(agent_id.trim())
            .ok_or_else(|| format!("agent '{agent_id}' does not exist"))?;
        if record.active_task.as_deref() == Some(desc.id.as_str()) {
            if let Some(cancel) = suspend_active_travel(record) { self.pending_commands.push(cancel); }
        }
        record.tasks.insert(desc.id.clone(), RuntimeTask::new(desc));
        record.next_think_world_seconds = 0.0;
        record.last_think_world_seconds = None;
        Ok(())
    }

    pub fn clear_task(
        &mut self,
        agent_id: &str,
        task_id: &str,
    ) -> Result<Option<AgentCommand>, String> {
        let record = self
            .agents
            .get_mut(agent_id.trim())
            .ok_or_else(|| format!("agent '{agent_id}' does not exist"))?;
        let was_active = record.active_task.as_deref() == Some(task_id.trim());
        let removed = record.tasks.remove(task_id.trim());
        let cancel = if was_active {
            removed
                .as_ref()
                .and_then(|task| task_travel_cancel(&record.desc.actor_id, task))
        } else {
            None
        };
        if was_active {
            record.active_task = None;
        }
        record.next_think_world_seconds = 0.0;
        record.last_think_world_seconds = None;
        Ok(cancel)
    }

    pub fn set_blackboard(
        &mut self,
        agent_id: &str,
        key: &str,
        value: Value,
    ) -> Result<(), String> {
        validate_id("agent blackboard key", key)?;
        validate_payload(&value)?;
        let record = self
            .agents
            .get_mut(agent_id.trim())
            .ok_or_else(|| format!("agent '{agent_id}' does not exist"))?;
        record.desc.blackboard.insert(key.to_owned(), value);
        Ok(())
    }

    pub fn remove_blackboard(&mut self, agent_id: &str, key: &str) -> Result<(), String> {
        let record = self
            .agents
            .get_mut(agent_id.trim())
            .ok_or_else(|| format!("agent '{agent_id}' does not exist"))?;
        record.desc.blackboard.remove(key.trim());
        Ok(())
    }

    pub fn tick(&mut self, dt: f32, snapshot: &AgentWorldSnapshot) -> Vec<AgentCommand> {
        self.frame_commands = std::mem::take(&mut self.pending_commands);
        self.frame_events.clear();
        if !dt.is_finite() || dt < 0.0 || !snapshot.world_seconds.is_finite() {
            return Vec::new();
        }

        let actors = snapshot
            .actors
            .iter()
            .map(|actor| (actor.id.as_str(), actor))
            .collect::<BTreeMap<_, _>>();

        for record in self.agents.values_mut() {
            if !record.desc.enabled {
                if let Some(cancel) = suspend_active_travel(record) { self.frame_commands.push(cancel); }
                continue;
            }
            let Some(actor) = actors.get(record.desc.actor_id.as_str()).copied() else {
                if let Some(cancel) = suspend_active_travel(record) { self.frame_commands.push(cancel); }
                continue;
            };
            if !actor.enabled {
                if let Some(cancel) = suspend_active_travel(record) { self.frame_commands.push(cancel); }
                continue;
            }

            if snapshot.world_seconds + 1.0e-9 < record.next_think_world_seconds {
                continue;
            }
            let interval = record
                .desc
                .thinking
                .interval_for_tier(&actor.simulation_tier);
            let elapsed = record
                .last_think_world_seconds
                .map(|last| (snapshot.world_seconds - last).max(0.0) as f32)
                .unwrap_or(dt);
            record.last_think_world_seconds = Some(snapshot.world_seconds);
            record.next_think_world_seconds =
                snapshot.world_seconds + f64::from(interval.max(0.001));

            update_perception(record, actor, snapshot);
            let selected = select_task(record);
            if selected != record.active_task {
                if let Some(previous_id) = record.active_task.as_deref() {
                    if let Some(previous) = record.tasks.get_mut(previous_id) {
                        if let Some(cancel) = task_travel_cancel(&record.desc.actor_id, previous) {
                            self.frame_commands.push(cancel);
                            if previous.status == AgentTaskStatus::Running {
                                previous.status = AgentTaskStatus::Pending;
                            }
                        }
                    }
                }
                record.active_task = selected.clone();
            }

            let Some(task_id) = selected else {
                continue;
            };
            let Some(task) = record.tasks.get_mut(&task_id) else {
                continue;
            };
            let old_status = task.status;
            behavior::advance_task(task, actor, &actors, elapsed, &mut self.frame_commands);
            if task.status != old_status {
                self.frame_events.push(json!({"actor_id": actor.id, "agent_id": record.desc.id,
                    "task_id": task.desc.id, "status": task.status}));
            }
        }

        self.frame_commands.clone()
    }

    pub fn is_bound(&self, agent_id: &str, actor_id: &str) -> bool {
        self.agents.get(agent_id).is_some_and(|r| r.desc.actor_id == actor_id)
    }

    pub fn clear_tasks(&mut self, agent_id: &str) -> Result<Option<AgentCommand>, String> {
        let record = self.agents.get_mut(agent_id)
            .ok_or_else(|| format!("agent '{agent_id}' does not exist"))?;
        let cancel = suspend_active_travel(record);
        record.tasks.clear(); record.next_think_world_seconds = 0.0;
        record.last_think_world_seconds = None;
        Ok(cancel)
    }

    pub fn presentation(&self, actor_id: &str, position: [f32; 3]) -> AgentPresentationIntent {
        self.agents.values().find(|r| r.desc.actor_id == actor_id && r.desc.enabled)
            .and_then(|r| r.active_task.as_ref().and_then(|id| r.tasks.get(id)))
            .map(|task| task.presentation(position)).unwrap_or_default()
    }

    pub fn fail_active(&mut self, actor_id: &str) {
        for record in self.agents.values_mut().filter(|r| r.desc.actor_id == actor_id) {
            if let Some(task) = record.active_task.as_ref().and_then(|id| record.tasks.get_mut(id)) {
                task.status = AgentTaskStatus::Failed;
            }
        }
    }

    pub fn frame_events(&self) -> &[Value] { &self.frame_events }

    pub fn runtime_state(&self) -> Value {
        let agents = self
            .agents
            .values()
            .map(|record| {
                let tasks = record
                    .tasks
                    .values()
                    .map(|task| {
                        json!({
                            "id": task.desc.id,
                            "lane": task.desc.lane,
                            "priority": task.desc.priority,
                            "task": task.desc.task,
                            "status": task.status,
                            "elapsed_seconds": task.elapsed_seconds,
                        })
                    })
                    .collect::<Vec<_>>();
                let memories = record.memories.values().cloned().collect::<Vec<_>>();
                json!({
                    "id": record.desc.id,
                    "actor_id": record.desc.actor_id,
                    "enabled": record.desc.enabled,
                    "perception": record.desc.perception,
                    "thinking": record.desc.thinking,
                    "blackboard": record.desc.blackboard,
                    "active_task": record.active_task,
                    "next_think_world_seconds": record.next_think_world_seconds,
                    "tasks": tasks,
                    "memories": memories,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "agents": agents,
            "frame_commands": self.frame_commands,
            "frame_events": self.frame_events,
        })
    }
}

fn suspend_active_travel(record: &mut AgentRecord) -> Option<AgentCommand> {
    let task_id = record.active_task.clone()?;
    let task = record.tasks.get_mut(&task_id)?;
    let cancel = task_travel_cancel(&record.desc.actor_id, task);
    if cancel.is_some() && task.status == AgentTaskStatus::Running {
        task.status = AgentTaskStatus::Pending;
    }
    record.active_task = None;
    cancel
}

fn task_travel_cancel(actor_id: &str, task: &RuntimeTask) -> Option<AgentCommand> {
    matches!(
        (&task.desc.task, task.status),
        (
            _,
            AgentTaskStatus::Pending | AgentTaskStatus::Running
        )
    )
    .then_some(())
    .filter(|_| task.desc.task.owns_movement())
    .map(|_| AgentCommand::CancelTravel {
        actor_id: actor_id.to_owned(),
    })
}

fn active_travel_cancel(record: &AgentRecord) -> Option<AgentCommand> {
    let task_id = record.active_task.as_deref()?;
    let task = record.tasks.get(task_id)?;
    task_travel_cancel(&record.desc.actor_id, task)
}

fn update_perception(
    record: &mut AgentRecord,
    actor: &AgentActorView,
    snapshot: &AgentWorldSnapshot,
) {
    let now = snapshot.world_seconds;
    record
        .memories
        .retain(|_, memory| memory.expires_world_seconds > now);

    for stimulus in &snapshot.stimuli {
        if stimulus.remaining_seconds <= 0.0 || stimulus.radius <= 0.0 {
            continue;
        }
        let distance = distance_between(actor.position, stimulus.position);
        if distance > record.desc.perception.radius || distance > stimulus.radius {
            continue;
        }

        let expires_world_seconds = now + f64::from(record.desc.perception.memory_seconds);
        let memory = record
            .memories
            .entry(stimulus.id.clone())
            .or_insert_with(|| AgentMemory {
                stimulus_id: stimulus.id.clone(),
                kind: stimulus.kind.clone(),
                source: stimulus.source.clone(),
                position: stimulus.position,
                intensity: stimulus.intensity,
                first_seen_world_seconds: now,
                last_seen_world_seconds: now,
                expires_world_seconds,
                tags: stimulus.tags.clone(),
                payload: stimulus.payload.clone(),
            });
        memory.kind = stimulus.kind.clone();
        memory.source = stimulus.source.clone();
        memory.position = stimulus.position;
        memory.intensity = stimulus.intensity;
        memory.last_seen_world_seconds = now;
        memory.expires_world_seconds = expires_world_seconds;
        memory.tags.clone_from(&stimulus.tags);
        memory.payload.clone_from(&stimulus.payload);
    }

    if record.memories.len() > record.desc.perception.max_memories {
        let mut oldest = record
            .memories
            .iter()
            .map(|(id, memory)| (id.clone(), memory.last_seen_world_seconds))
            .collect::<Vec<_>>();
        oldest.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        let remove_count = record.memories.len() - record.desc.perception.max_memories;
        for (id, _) in oldest.into_iter().take(remove_count) {
            record.memories.remove(&id);
        }
    }
}

fn select_task(record: &AgentRecord) -> Option<String> {
    record
        .tasks
        .values()
        .filter(|task| {
            !matches!(
                task.status,
                AgentTaskStatus::Completed | AgentTaskStatus::Failed
            )
        })
        .max_by(|a, b| {
            a.desc
                .lane
                .rank()
                .cmp(&b.desc.lane.rank())
                .then_with(|| a.desc.priority.cmp(&b.desc.priority))
                .then_with(|| b.desc.id.cmp(&a.desc.id))
        })
        .map(|task| task.desc.id.clone())
}

fn validate_id(kind: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 {
        Err(format!("{kind} id must be non-empty and at most 128 bytes"))
    } else {
        Ok(())
    }
}

fn validate_payload(value: &Value) -> Result<(), String> {
    let len = serde_json::to_vec(value)
        .map_err(|error| format!("agent JSON payload encode failed: {error}"))?
        .len();
    if len > 1_048_576 {
        Err("agent JSON payload exceeds 1 MiB".to_owned())
    } else {
        Ok(())
    }
}

fn distance_between(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(destination: Option<&str>) -> AgentActorView {
        AgentActorView {
            id: "actor.1".to_owned(),
            position: [0.0, 0.0, 0.0],
            enabled: true,
            simulation_tier: "full".to_owned(),
            travel_destination: destination.map(str::to_owned),
        }
    }

    fn agent() -> AgentDesc {
        AgentDesc {
            id: "agent.1".to_owned(),
            actor_id: "actor.1".to_owned(),
            enabled: true,
            perception: AgentPerceptionPolicy::default(),
            thinking: AgentThinkingPolicy::default(),
            blackboard: BTreeMap::new(),
        }
    }

    #[test]
    fn reaction_lane_preempts_primary_task() {
        let mut runtime = AgentRuntime::default();
        let _ = runtime.upsert_agent(agent()).unwrap();
        runtime
            .set_task(
                "agent.1",
                AgentTaskDesc {
                    id: "primary".to_owned(),
                    lane: AgentTaskLane::Primary,
                    priority: 100,
                    task: AgentTaskKind::Idle,
                },
            )
            .unwrap();
        runtime
            .set_task(
                "agent.1",
                AgentTaskDesc {
                    id: "reaction".to_owned(),
                    lane: AgentTaskLane::Reaction,
                    priority: -100,
                    task: AgentTaskKind::Idle,
                },
            )
            .unwrap();

        runtime.tick(
            0.1,
            &AgentWorldSnapshot {
                world_seconds: 1.0,
                actors: vec![actor(None)],
                stimuli: Vec::new(),
            },
        );

        assert_eq!(
            runtime.runtime_state()["agents"][0]["active_task"],
            "reaction"
        );
    }

    #[test]
    fn travel_task_emits_once_and_completes_when_world_travel_finishes() {
        let mut runtime = AgentRuntime::default();
        let _ = runtime.upsert_agent(agent()).unwrap();
        runtime
            .set_task(
                "agent.1",
                AgentTaskDesc {
                    id: "go".to_owned(),
                    lane: AgentTaskLane::Movement,
                    priority: 0,
                    task: AgentTaskKind::TravelToNode {
                        start_node: None,
                        destination_node: "door".to_owned(),
                        speed: 1.5,
                        mode: "walk".to_owned(),
                        payload: Value::Null,
                    },
                },
            )
            .unwrap();

        let first = runtime.tick(
            0.1,
            &AgentWorldSnapshot {
                world_seconds: 1.0,
                actors: vec![actor(None)],
                stimuli: Vec::new(),
            },
        );
        assert_eq!(first.len(), 1);
        assert!(matches!(first[0], AgentCommand::StartTravel { .. }));

        let second = runtime.tick(
            0.1,
            &AgentWorldSnapshot {
                world_seconds: 1.1,
                actors: vec![actor(Some("door"))],
                stimuli: Vec::new(),
            },
        );
        assert!(second.is_empty());

        let third = runtime.tick(
            0.1,
            &AgentWorldSnapshot {
                world_seconds: 1.2,
                actors: vec![actor(None)],
                stimuli: Vec::new(),
            },
        );
        assert!(third.is_empty());
        assert_eq!(
            runtime.runtime_state()["agents"][0]["tasks"][0]["status"],
            "completed"
        );
    }

    #[test]
    fn reaction_preemption_cancels_running_travel_and_allows_resume() {
        let mut runtime = AgentRuntime::default();
        let _ = runtime.upsert_agent(agent()).unwrap();
        runtime
            .set_task(
                "agent.1",
                AgentTaskDesc {
                    id: "walk".to_owned(),
                    lane: AgentTaskLane::Movement,
                    priority: 0,
                    task: AgentTaskKind::TravelToNode {
                        start_node: None,
                        destination_node: "door".to_owned(),
                        speed: 1.5,
                        mode: "walk".to_owned(),
                        payload: Value::Null,
                    },
                },
            )
            .unwrap();

        let first = runtime.tick(
            0.1,
            &AgentWorldSnapshot {
                world_seconds: 1.0,
                actors: vec![actor(None)],
                stimuli: Vec::new(),
            },
        );
        assert!(matches!(
            first.as_slice(),
            [AgentCommand::StartTravel { .. }]
        ));

        runtime
            .set_task(
                "agent.1",
                AgentTaskDesc {
                    id: "react".to_owned(),
                    lane: AgentTaskLane::Reaction,
                    priority: 0,
                    task: AgentTaskKind::Wait {
                        duration_seconds: 0.05,
                    },
                },
            )
            .unwrap();

        let preempt = runtime.tick(
            0.1,
            &AgentWorldSnapshot {
                world_seconds: 1.1,
                actors: vec![actor(Some("door"))],
                stimuli: Vec::new(),
            },
        );
        assert!(matches!(
            preempt.as_slice(),
            [AgentCommand::CancelTravel { .. }]
        ));

        let resume = runtime.tick(
            0.1,
            &AgentWorldSnapshot {
                world_seconds: 1.2,
                actors: vec![actor(None)],
                stimuli: Vec::new(),
            },
        );
        assert!(matches!(
            resume.as_slice(),
            [AgentCommand::StartTravel { .. }]
        ));
    }

    #[test]
    fn background_agents_are_timesliced() {
        let mut desc = agent();
        desc.thinking = AgentThinkingPolicy {
            full_interval_seconds: 0.05,
            reduced_interval_seconds: 0.25,
            background_interval_seconds: 2.0,
        };
        let mut runtime = AgentRuntime::default();
        let _ = runtime.upsert_agent(desc).unwrap();
        runtime
            .set_task(
                "agent.1",
                AgentTaskDesc {
                    id: "wait".to_owned(),
                    lane: AgentTaskLane::Ambient,
                    priority: 0,
                    task: AgentTaskKind::Wait {
                        duration_seconds: 10.0,
                    },
                },
            )
            .unwrap();

        let mut background = actor(None);
        background.simulation_tier = "background".to_owned();
        runtime.tick(
            0.01,
            &AgentWorldSnapshot {
                world_seconds: 1.0,
                actors: vec![background.clone()],
                stimuli: Vec::new(),
            },
        );
        let elapsed = runtime.runtime_state()["agents"][0]["tasks"][0]["elapsed_seconds"]
            .as_f64()
            .unwrap();

        runtime.tick(
            0.01,
            &AgentWorldSnapshot {
                world_seconds: 1.5,
                actors: vec![background],
                stimuli: Vec::new(),
            },
        );
        let unchanged = runtime.runtime_state()["agents"][0]["tasks"][0]["elapsed_seconds"]
            .as_f64()
            .unwrap();
        assert_eq!(elapsed, unchanged);
    }

    #[test]
    fn clearing_or_removing_active_travel_returns_cancel_intent() {
        let mut runtime = AgentRuntime::default();
        let _ = runtime.upsert_agent(agent()).unwrap();
        runtime
            .set_task(
                "agent.1",
                AgentTaskDesc {
                    id: "walk".to_owned(),
                    lane: AgentTaskLane::Movement,
                    priority: 0,
                    task: AgentTaskKind::TravelToNode {
                        start_node: None,
                        destination_node: "door".to_owned(),
                        speed: 1.5,
                        mode: "walk".to_owned(),
                        payload: Value::Null,
                    },
                },
            )
            .unwrap();
        runtime.tick(
            0.1,
            &AgentWorldSnapshot {
                world_seconds: 1.0,
                actors: vec![actor(None)],
                stimuli: Vec::new(),
            },
        );

        let cancel = runtime.clear_task("agent.1", "walk").unwrap();
        assert!(matches!(cancel, Some(AgentCommand::CancelTravel { .. })));

        runtime
            .set_task(
                "agent.1",
                AgentTaskDesc {
                    id: "walk-again".to_owned(),
                    lane: AgentTaskLane::Movement,
                    priority: 0,
                    task: AgentTaskKind::TravelToNode {
                        start_node: None,
                        destination_node: "door".to_owned(),
                        speed: 1.5,
                        mode: "walk".to_owned(),
                        payload: Value::Null,
                    },
                },
            )
            .unwrap();
        runtime.tick(
            0.1,
            &AgentWorldSnapshot {
                world_seconds: 1.1,
                actors: vec![actor(None)],
                stimuli: Vec::new(),
            },
        );
        assert!(matches!(
            runtime.remove_agent("agent.1"),
            Some(AgentCommand::CancelTravel { .. })
        ));
    }

    #[test]
    fn disabling_or_rebinding_agent_cancels_owned_travel() {
        let mut runtime = AgentRuntime::default();
        let _ = runtime.upsert_agent(agent()).unwrap();
        runtime
            .set_task(
                "agent.1",
                AgentTaskDesc {
                    id: "walk".to_owned(),
                    lane: AgentTaskLane::Movement,
                    priority: 0,
                    task: AgentTaskKind::TravelToNode {
                        start_node: None,
                        destination_node: "door".to_owned(),
                        speed: 1.5,
                        mode: "walk".to_owned(),
                        payload: Value::Null,
                    },
                },
            )
            .unwrap();
        runtime.tick(
            0.1,
            &AgentWorldSnapshot {
                world_seconds: 1.0,
                actors: vec![actor(None)],
                stimuli: Vec::new(),
            },
        );

        let mut disabled = agent();
        disabled.enabled = false;
        assert!(matches!(
            runtime.upsert_agent(disabled).unwrap(),
            Some(AgentCommand::CancelTravel { .. })
        ));

        let mut rebound = agent();
        rebound.actor_id = "actor.2".to_owned();
        let _ = runtime.upsert_agent(rebound).unwrap();
        let state = runtime.runtime_state();
        assert_eq!(state["agents"][0]["actor_id"], "actor.2");
        assert!(state["agents"][0]["tasks"].as_array().unwrap().is_empty());
    }

    #[test]
    fn perception_records_only_stimuli_inside_agent_and_stimulus_radius() {
        let mut runtime = AgentRuntime::default();
        let _ = runtime.upsert_agent(agent()).unwrap();
        let snapshot = AgentWorldSnapshot {
            world_seconds: 10.0,
            actors: vec![actor(None)],
            stimuli: vec![
                AgentStimulusView {
                    id: "near".to_owned(),
                    kind: "sound".to_owned(),
                    source: "test".to_owned(),
                    position: [5.0, 0.0, 0.0],
                    radius: 20.0,
                    intensity: 1.0,
                    remaining_seconds: 1.0,
                    tags: Vec::new(),
                    payload: Value::Null,
                },
                AgentStimulusView {
                    id: "outside-source-radius".to_owned(),
                    kind: "sound".to_owned(),
                    source: "test".to_owned(),
                    position: [10.0, 0.0, 0.0],
                    radius: 2.0,
                    intensity: 1.0,
                    remaining_seconds: 1.0,
                    tags: Vec::new(),
                    payload: Value::Null,
                },
            ],
        };

        runtime.tick(0.1, &snapshot);
        let state = runtime.runtime_state();
        assert_eq!(state["agents"][0]["memories"].as_array().unwrap().len(), 1);
        assert_eq!(state["agents"][0]["memories"][0]["stimulus_id"], "near");
    }
}
