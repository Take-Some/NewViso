use serde::{Deserialize, Serialize};

use crate::{LoadingStatusEvent, LoadingStatusPhase, LoadingSubsystemSnapshot};

/// Host/event-bus topic for every engine task lifecycle update.
///
/// This is deliberately generic: plugin lifecycle calls, service calls, CPU jobs,
/// asset worker passes and renderer bootstrap phases can all publish the same
/// shape. Loading/UI/debug tools can then build a live task view without knowing
/// which subsystem performs the work.
pub const ENGINE_TASK_EVENT_TOPIC_V1: &str = "engine.task.event.v1";

/// Host/event-bus topic for cooperative task control requests.
///
/// A task may ignore a control action only when it explicitly advertised that the
/// action is unsupported. Otherwise cancellation and pause/resume are cooperative
/// and must be observed at task-owned checkpoints.
pub const ENGINE_TASK_CONTROL_TOPIC_V1: &str = "engine.task.control.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineTaskPhase {
    Scheduled,
    Running,
    Blocked,
    PauseRequested,
    Paused,
    ResumeRequested,
    CancelRequested,
    Cancelled,
    Completed,
    Failed,
}

impl EngineTaskPhase {
    #[inline]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Completed | Self::Failed)
    }

    #[inline]
    pub const fn loading_phase(self) -> LoadingStatusPhase {
        match self {
            Self::Scheduled | Self::PauseRequested | Self::ResumeRequested => {
                LoadingStatusPhase::Waiting
            }
            Self::Running | Self::Blocked | Self::Paused | Self::CancelRequested => {
                LoadingStatusPhase::Running
            }
            Self::Completed | Self::Cancelled => LoadingStatusPhase::Ready,
            Self::Failed => LoadingStatusPhase::Failed,
        }
    }

    #[inline]
    pub const fn state_label(self) -> &'static str {
        match self {
            Self::Scheduled => "QUEUED",
            Self::Running => "RUNNING",
            Self::Blocked => "BLOCKED",
            Self::PauseRequested => "PAUSE",
            Self::Paused => "PAUSED",
            Self::ResumeRequested => "RESUME",
            Self::CancelRequested => "CANCEL",
            Self::Cancelled => "CANCELLED",
            Self::Completed => "DONE",
            Self::Failed => "ERR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineTaskControlAction {
    Pause,
    Resume,
    Cancel,
}

impl EngineTaskControlAction {
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Cancel => "cancel",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineTaskControlEvent {
    pub task_id: String,
    pub action: EngineTaskControlAction,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub source: String,
}

impl EngineTaskControlEvent {
    #[inline]
    pub fn new(task_id: impl Into<String>, action: EngineTaskControlAction) -> Self {
        Self {
            task_id: task_id.into(),
            action,
            reason: String::new(),
            source: "engine.task.control".to_owned(),
        }
    }

    #[inline]
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = reason.into();
        self
    }

    #[inline]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineTaskEvent {
    pub task_id: String,
    #[serde(default)]
    pub parent_task_id: Option<String>,
    pub source: String,
    pub owner: String,
    pub category: String,
    pub name: String,
    pub lane: String,
    /// Optional frame id for frame-bound engine.threading work.
    ///
    /// This is deliberately generic task metadata so profiler/loading/debug tools
    /// can correlate CPU work without knowing the producer crate. Long-running
    /// background jobs may leave it empty; render/simulation/streaming passes
    /// should fill it.
    #[serde(default)]
    pub frame_id: Option<u64>,
    /// Deterministic dependency group / barrier name for task-graph diagnostics.
    #[serde(default)]
    pub dependency_group: Option<String>,
    /// Domain-level job pass, e.g. `visibility`, `streaming`, `terrain`,
    /// `texture-decode`, `simulation`, `shader-compile`.
    #[serde(default)]
    pub task_pass: Option<String>,
    /// Owner domain for the pass, e.g. `engine.render`, `engine.assets`,
    /// `engine.simulation`.
    #[serde(default)]
    pub task_domain: Option<String>,
    /// Stable scheduler priority as text.
    #[serde(default)]
    pub priority: Option<String>,
    /// Executor identity such as `engine-worker`, `main-thread-barrier` or
    /// `external-provider`. This makes dark work visible even when it is not
    /// worker-pool backed yet.
    #[serde(default)]
    pub executor: Option<String>,
    pub phase: EngineTaskPhase,
    pub state_label: String,
    pub status: String,
    pub detail: String,
    #[serde(default)]
    pub progress_01: Option<f32>,
    pub can_pause: bool,
    pub can_cancel: bool,
    pub terminal: bool,
}

impl EngineTaskEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        task_id: impl Into<String>,
        source: impl Into<String>,
        owner: impl Into<String>,
        category: impl Into<String>,
        name: impl Into<String>,
        lane: impl Into<String>,
        phase: EngineTaskPhase,
        status: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        let phase_label = phase.state_label();
        Self {
            task_id: task_id.into(),
            parent_task_id: None,
            source: source.into(),
            owner: owner.into(),
            category: category.into(),
            name: name.into(),
            lane: lane.into(),
            frame_id: None,
            dependency_group: None,
            task_pass: None,
            task_domain: None,
            priority: None,
            executor: None,
            phase,
            state_label: phase_label.to_owned(),
            status: status.into(),
            detail: detail.into(),
            progress_01: None,
            can_pause: false,
            can_cancel: false,
            terminal: phase.is_terminal(),
        }
    }

    #[inline]
    pub fn with_frame_id(mut self, frame_id: u64) -> Self {
        self.frame_id = Some(frame_id);
        self
    }

    #[inline]
    pub fn with_dependency_group(mut self, dependency_group: impl Into<String>) -> Self {
        self.dependency_group = Some(dependency_group.into());
        self
    }

    #[inline]
    pub fn with_task_pass(mut self, task_pass: impl Into<String>) -> Self {
        self.task_pass = Some(task_pass.into());
        self
    }

    #[inline]
    pub fn with_task_domain(mut self, task_domain: impl Into<String>) -> Self {
        self.task_domain = Some(task_domain.into());
        self
    }

    #[inline]
    pub fn with_priority(mut self, priority: impl Into<String>) -> Self {
        self.priority = Some(priority.into());
        self
    }

    #[inline]
    pub fn with_executor(mut self, executor: impl Into<String>) -> Self {
        self.executor = Some(executor.into());
        self
    }

    #[inline]
    pub fn with_parent_task_id(mut self, parent_task_id: impl Into<String>) -> Self {
        self.parent_task_id = Some(parent_task_id.into());
        self
    }

    #[inline]
    pub fn with_progress(mut self, progress_01: f32) -> Self {
        self.progress_01 = Some(progress_01.clamp(0.0, 1.0));
        self
    }

    #[inline]
    pub fn with_controls(mut self, can_pause: bool, can_cancel: bool) -> Self {
        self.can_pause = can_pause;
        self.can_cancel = can_cancel;
        self
    }

    #[inline]
    pub fn with_state_label(mut self, state_label: impl Into<String>) -> Self {
        self.state_label = state_label.into();
        self
    }

    #[inline]
    pub fn with_terminal(mut self, terminal: bool) -> Self {
        self.terminal = terminal;
        self
    }

    pub fn to_loading_status_event(&self) -> LoadingStatusEvent {
        let progress =
            self.progress_01
                .unwrap_or_else(|| if self.phase.is_terminal() { 1.0 } else { 0.0 });
        LoadingStatusEvent::new(
            self.source.as_str(),
            self.category.as_str(),
            self.category.as_str(),
            self.category.to_ascii_uppercase(),
            self.phase.loading_phase(),
            "NORTH STAR ENGINE // TASKS",
            self.status.as_str(),
            self.detail.as_str(),
            progress,
        )
        .with_state_label(self.state_label.as_str())
        .with_terminal(false)
    }

    pub fn to_subsystem_snapshot(&self) -> LoadingSubsystemSnapshot {
        LoadingSubsystemSnapshot::new(
            self.category.as_str(),
            self.category.to_ascii_uppercase(),
            self.phase.loading_phase().as_subsystem_phase(),
            self.state_label.as_str(),
            self.detail.as_str(),
            self.progress_01,
        )
    }
}
