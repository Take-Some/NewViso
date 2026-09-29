#![forbid(unsafe_op_in_unsafe_fn)]
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use newviso_loading_api::{
    EngineTaskControlAction, EngineTaskControlEvent, EngineTaskEvent, EngineTaskPhase,
    ENGINE_TASK_CONTROL_TOPIC_V1, ENGINE_TASK_EVENT_TOPIC_V1,
};

pub const ENGINE_THREADING_SERVICE_ID: &str = "engine.threading";
pub const THREADING_SERVICE_ID: &str = "threading.api";
pub const THREADING_BACKEND_CAPABILITY_ID: &str = "threading.backend";
pub const THREADING_RUNTIME_CONTRACT: &str = "newengine.threading.runtime.v1";

/// Canonical topic for engine.task lifecycle envelopes.
///
/// `ENGINE_TASK_EVENT_TOPIC_V1` remains the UI/loading projection topic. This
/// topic carries the stricter task-authority envelope for diagnostics, profiler
/// and CI-visible work streams.
pub const ENGINE_TASK_ENVELOPE_TOPIC_V1: &str = "engine.threading.event.v1";

/// Canonical domain ids used by profiler-visible task passes.
///
/// These strings are not scheduler implementation details. They are the public
/// diagnostic language for the runtime spine: every heavy system should identify
/// which domain owns the work.
pub mod task_domain {
    pub const ENGINE_JOBS: &str = "engine.threading";
    pub const ENGINE_RENDER: &str = "engine.render";
    pub const ENGINE_RENDER_PREP: &str = "engine.render.prep";
    pub const ENGINE_ASSETS: &str = "engine.assets";
    pub const ENGINE_STREAMING: &str = "engine.streaming";
    pub const ENGINE_WORLD_STREAMING: &str = "engine.world.streaming";
    pub const ENGINE_SIMULATION: &str = "engine.simulation";
    pub const ENGINE_VISIBILITY: &str = "engine.visibility";
    pub const ENGINE_VEGETATION: &str = "engine.vegetation";
    pub const ENGINE_TERRAIN: &str = "engine.terrain";
    pub const ENGINE_ANIMATION: &str = "engine.animation";
    pub const ENGINE_DESTRUCTION: &str = "engine.destruction";
    pub const ENGINE_PARTICLES: &str = "engine.particles";
    pub const ENGINE_SHADER: &str = "engine.shader";
}

/// Canonical task-pass names for production-style domain task graph slices.
///
/// A pass name describes *what kind of work* is happening, independently from
/// which worker executes it. This keeps profiler reports stable when a pass
/// moves from a main-thread barrier to an engine.threading executor.
pub mod task_pass {
    pub const INPUT: &str = "input";
    pub const CONTROLLERS: &str = "controllers";
    pub const APPLY_INTENTS: &str = "apply-intents";
    pub const PHYSICS: &str = "physics";
    pub const DERIVED: &str = "derived";
    pub const SIM_READ_SNAPSHOT: &str = "sim-read-snapshot";
    pub const SIM_COMMAND_BATCH: &str = "sim-command-batch";
    pub const VISIBILITY: &str = "visibility";
    pub const SCENE_SNAPSHOT: &str = "scene-snapshot";
    pub const SCENE_RENDER_SNAPSHOT: &str = "scene-render-snapshot";
    pub const FEATURE_EXTRACT: &str = "feature-extract";
    pub const RENDER_SUBMIT: &str = "render-submit";
    pub const FRAME_ENVELOPE: &str = "frame-envelope";
    pub const TERRAIN_RENDER_PACKET: &str = "terrain-render-packet";
    pub const TERRAIN_GPU_RESIDENCY: &str = "terrain-gpu-residency";
    pub const WORLD_STREAMING_PLAN: &str = "world-streaming-plan";
    pub const ASSET_IO: &str = "asset-io";
    pub const LISTFILE_DECODE: &str = "listfile-decode";
    pub const SEMANTIC_DTO: &str = "semantic-dto";
    pub const TEXTURE_DECODE: &str = "texture-decode";
    pub const TEXTURE_UPLOAD: &str = "texture-upload";
    pub const SHADER_COMPILE: &str = "shader-compile";
    pub const VEGETATION_BUILD: &str = "vegetation-build";
    pub const ANIMATION_SAMPLING: &str = "animation-sampling";
    pub const DESTRUCTION_BUILD: &str = "destruction-build";
    pub const PARTICLE_SIMULATION: &str = "particle-simulation";
    pub const SERVICE_CALL: &str = "service-call";
    pub const INPUT_CAPTURE: &str = "input-capture";
    pub const PROCESS: &str = "process";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskExecutorKind {
    EngineWorker,
    MainThreadBarrier,
    PluginHostBridge,
    ToolRunner,
    SimulationInternalParallelism,
    RuntimeWatchdog,
    ExternalProvider,
}

impl TaskExecutorKind {
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EngineWorker => "engine-worker",
            Self::MainThreadBarrier => "main-thread-barrier",
            Self::PluginHostBridge => "plugin-host-bridge",
            Self::ToolRunner => "tool-runner",
            Self::SimulationInternalParallelism => "simulation-internal-parallelism",
            Self::RuntimeWatchdog => "runtime-watchdog",
            Self::ExternalProvider => "external-provider",
        }
    }
}

impl Default for TaskExecutorKind {
    #[inline]
    fn default() -> Self {
        Self::EngineWorker
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskAuthorityV1 {
    pub gateway: String,
    pub provider: String,
    pub contract: String,
    pub event_topic: String,
    pub control_topic: String,
    pub task_event_topic: String,
    pub identity_required: bool,
    pub invisible_work_allowed: bool,
}

impl Default for TaskAuthorityV1 {
    fn default() -> Self {
        Self {
            gateway: ENGINE_THREADING_SERVICE_ID.to_owned(),
            provider: THREADING_SERVICE_ID.to_owned(),
            contract: THREADING_RUNTIME_CONTRACT.to_owned(),
            event_topic: ENGINE_TASK_EVENT_TOPIC_V1.to_owned(),
            control_topic: ENGINE_TASK_CONTROL_TOPIC_V1.to_owned(),
            task_event_topic: ENGINE_TASK_ENVELOPE_TOPIC_V1.to_owned(),
            identity_required: true,
            invisible_work_allowed: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineTaskEnvelopeV1 {
    pub schema: String,
    pub authority: TaskAuthorityV1,
    pub executor: TaskExecutorKind,
    pub semantic_owner: String,
    pub event: EngineTaskEvent,
}

impl Default for EngineTaskEnvelopeV1 {
    fn default() -> Self {
        Self {
            schema: "newengine.threading.event.v1".to_owned(),
            authority: TaskAuthorityV1::default(),
            executor: TaskExecutorKind::EngineWorker,
            semantic_owner: "runtime-work".to_owned(),
            event: EngineTaskEvent::new(
                "task.unknown",
                ENGINE_THREADING_SERVICE_ID,
                ENGINE_THREADING_SERVICE_ID,
                "runtime",
                "unknown-task",
                "runtime",
                EngineTaskPhase::Scheduled,
                "Task scheduled",
                "Task event was created without a concrete producer.",
            ),
        }
    }
}

impl EngineTaskEnvelopeV1 {
    #[inline]
    pub fn new(
        mut event: EngineTaskEvent,
        executor: TaskExecutorKind,
        semantic_owner: impl Into<String>,
    ) -> Self {
        if event.executor.is_none() {
            event = event.with_executor(executor.as_str());
        }
        Self {
            executor,
            semantic_owner: semantic_owner.into(),
            event,
            ..Default::default()
        }
    }

    #[inline]
    pub fn into_task_event(self) -> EngineTaskEvent {
        self.event
    }
}

pub mod task_method {
    pub const INFO_JSON: &str = newviso_service_api::SERVICE_METHOD_INFO_JSON;
    pub const INVOKE_JSON: &str = newviso_service_api::SERVICE_METHOD_INVOKE_JSON;
    pub const SHUTDOWN_V1: &str = newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1;
    pub const START_V1: &str = "task.start_v1";
    pub const RUN_PROCESS_START_V1: &str = "task.run_process_start_v1";
    pub const RESULT_BIN_V1: &str = "task.result_bin_v1";
    pub const INVOKE_SERVICE_V1: &str = "task.invoke_service_v1";
    pub const CANCEL_V1: &str = "task.cancel_v1";
    pub const PAUSE_V1: &str = "task.pause_v1";
    pub const RESUME_V1: &str = "task.resume_v1";
    pub const STATUS_JSON_V1: &str = "task.status_json_v1";
    pub const PROGRESS_EVENT_V1: &str = "task.progress_event_v1";
    pub const TRACE_JSON_V1: &str = "task.trace_json_v1";
    pub const SNAPSHOT_JSON_V1: &str = "task.snapshot_json_v1";
}

pub const TASK_SERVICE_METHODS: &[&str] = &[
    task_method::INFO_JSON,
    task_method::INVOKE_JSON,
    task_method::SHUTDOWN_V1,
    task_method::START_V1,
    task_method::RUN_PROCESS_START_V1,
    task_method::RESULT_BIN_V1,
    task_method::INVOKE_SERVICE_V1,
    task_method::CANCEL_V1,
    task_method::PAUSE_V1,
    task_method::RESUME_V1,
    task_method::STATUS_JSON_V1,
    task_method::PROGRESS_EVENT_V1,
    task_method::TRACE_JSON_V1,
    task_method::SNAPSHOT_JSON_V1,
];

pub const THREADING_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "threading",
        ENGINE_THREADING_SERVICE_ID,
        THREADING_SERVICE_ID,
        THREADING_BACKEND_CAPABILITY_ID,
    );

pub const THREADING_RUNTIME_CONTRACT_SPEC: newviso_service_api::RuntimeServiceContractSpec =
    newviso_service_api::RuntimeServiceContractSpec::new(
        ENGINE_THREADING_SERVICE_ID,
        THREADING_RUNTIME_CONTRACT,
        TASK_SERVICE_METHODS,
    );

pub const THREADING_RUNTIME_REQUIREMENT_SPEC: newviso_service_api::RuntimeServiceRequirementSpec =
    newviso_service_api::RuntimeServiceRequirementSpec::new(
        THREADING_RUNTIME_CONTRACT_SPEC,
        Some(THREADING_BACKEND_CAPABILITY_ID),
        Some("NEWENGINE_REQUIRE_JOBS_BACKEND"),
    );

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskServiceInfoV1 {
    pub service_id: String,
    pub gateway: String,
    pub provider: String,
    pub contract: String,
    pub methods: Vec<String>,
    pub event_topic: String,
    pub control_topic: String,
    pub task_event_topic: String,
    pub authority: TaskAuthorityV1,
    pub cooperative_control: bool,
}

impl Default for TaskServiceInfoV1 {
    fn default() -> Self {
        Self {
            service_id: THREADING_SERVICE_ID.to_owned(),
            gateway: ENGINE_THREADING_SERVICE_ID.to_owned(),
            provider: "ThreadPoolTaskProvider".to_owned(),
            contract: THREADING_RUNTIME_CONTRACT.to_owned(),
            methods: TASK_SERVICE_METHODS
                .iter()
                .map(|m| (*m).to_owned())
                .collect(),
            event_topic: ENGINE_TASK_EVENT_TOPIC_V1.to_owned(),
            control_topic: ENGINE_TASK_CONTROL_TOPIC_V1.to_owned(),
            task_event_topic: ENGINE_TASK_ENVELOPE_TOPIC_V1.to_owned(),
            authority: TaskAuthorityV1::default(),
            cooperative_control: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskIdRequestV1 {
    #[serde(alias = "job_id")]
    pub task_id: String,
    pub reason: String,
    pub source: String,
}

impl Default for TaskIdRequestV1 {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            reason: String::new(),
            source: "engine.threading".to_owned(),
        }
    }
}

impl TaskIdRequestV1 {
    pub fn control_event(&self, action: EngineTaskControlAction) -> EngineTaskControlEvent {
        EngineTaskControlEvent::new(self.task_id.clone(), action)
            .with_reason(self.reason.clone())
            .with_source(self.source.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskStartRequestV1 {
    #[serde(alias = "job_id")]
    pub task_id: String,
    pub name: String,
    pub owner: String,
    pub category: String,
    pub lane: String,
    pub priority: String,
    pub frame_id: Option<u64>,
    pub dependency_group: String,
    pub task_pass: String,
    pub task_domain: String,
    pub can_pause: bool,
    pub can_cancel: bool,
}

impl Default for TaskStartRequestV1 {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            name: "external-task".to_owned(),
            owner: "engine.threading".to_owned(),
            category: "external".to_owned(),
            lane: "external".to_owned(),
            priority: "normal".to_owned(),
            frame_id: None,
            dependency_group: String::new(),
            task_pass: "external".to_owned(),
            task_domain: "engine.threading".to_owned(),
            can_pause: false,
            can_cancel: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskRunProcessStartRequestV1 {
    #[serde(alias = "job_id")]
    pub task_id: String,
    pub name: String,
    pub owner: String,
    pub category: String,
    pub lane: String,
    pub priority: String,
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub result_path: String,
    pub frame_id: Option<u64>,
    pub dependency_group: String,
    pub task_pass: String,
    pub task_domain: String,
    pub can_cancel: bool,
}

impl Default for TaskRunProcessStartRequestV1 {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            name: "external-process".to_owned(),
            owner: "engine.threading".to_owned(),
            category: "tool.process".to_owned(),
            lane: "render-prep".to_owned(),
            priority: "background".to_owned(),
            executable: String::new(),
            args: Vec::new(),
            cwd: String::new(),
            env: BTreeMap::new(),
            result_path: String::new(),
            frame_id: None,
            dependency_group: String::new(),
            task_pass: "process".to_owned(),
            task_domain: "engine.threading".to_owned(),
            can_cancel: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskRunProcessStartedV1 {
    pub task_id: String,
    pub job_id: String,
    pub accepted: bool,
    pub status: String,
    pub detail: String,
    pub result_path: String,
}

impl Default for TaskRunProcessStartedV1 {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            job_id: String::new(),
            accepted: false,
            status: "rejected".to_owned(),
            detail: String::new(),
            result_path: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskServiceCallTargetV1 {
    pub gateway: String,
    pub method: String,
    pub payload_json: serde_json::Value,
}

impl Default for TaskServiceCallTargetV1 {
    fn default() -> Self {
        Self {
            gateway: String::new(),
            method: String::new(),
            payload_json: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskServiceCallRequestV1 {
    #[serde(alias = "job_id")]
    pub task_id: String,
    pub name: String,
    pub owner: String,
    pub category: String,
    pub lane: String,
    pub priority: String,
    pub can_pause: bool,
    pub frame_id: Option<u64>,
    pub dependency_group: String,
    pub task_pass: String,
    pub task_domain: String,
    pub can_cancel: bool,
    pub target: TaskServiceCallTargetV1,
}

impl Default for TaskServiceCallRequestV1 {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            name: "service-call-task".to_owned(),
            owner: "engine.threading".to_owned(),
            category: "service-call".to_owned(),
            lane: "plugin".to_owned(),
            priority: "background".to_owned(),
            can_pause: false,
            frame_id: None,
            dependency_group: String::new(),
            task_pass: "service-call".to_owned(),
            task_domain: "engine.threading".to_owned(),
            can_cancel: true,
            target: TaskServiceCallTargetV1::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TaskServiceCallAcceptedV1 {
    pub task_id: String,
    pub job_id: String,
    pub accepted: bool,
    pub gateway: String,
    pub method: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskProgressEventV1 {
    pub task_id: String,
    pub owner: String,
    pub category: String,
    pub name: String,
    pub lane: String,
    pub status: String,
    pub detail: String,
    pub progress_01: f32,
    pub phase: EngineTaskPhase,
    pub frame_id: Option<u64>,
    pub dependency_group: String,
    pub task_pass: String,
    pub task_domain: String,
    pub priority: String,
    pub executor: String,
    pub can_pause: bool,
    pub can_cancel: bool,
}

impl Default for TaskProgressEventV1 {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            owner: "engine.threading".to_owned(),
            category: "runtime".to_owned(),
            name: "runtime-task".to_owned(),
            lane: "runtime".to_owned(),
            status: "Task progress".to_owned(),
            detail: String::new(),
            progress_01: 0.0,
            phase: EngineTaskPhase::Running,
            frame_id: None,
            dependency_group: String::new(),
            task_pass: "runtime".to_owned(),
            task_domain: "engine.threading".to_owned(),
            priority: "normal".to_owned(),
            executor: "external-provider".to_owned(),
            can_pause: false,
            can_cancel: true,
        }
    }
}

impl TaskProgressEventV1 {
    pub fn into_task_event(self) -> EngineTaskEvent {
        let mut event = EngineTaskEvent::new(
            self.task_id,
            "engine.threading",
            self.owner,
            self.category,
            self.name,
            self.lane,
            self.phase,
            self.status,
            self.detail,
        )
        .with_progress(self.progress_01)
        .with_controls(self.can_pause, self.can_cancel)
        .with_task_domain(self.task_domain)
        .with_task_pass(self.task_pass)
        .with_priority(self.priority)
        .with_executor(self.executor);
        if let Some(frame_id) = self.frame_id {
            event = event.with_frame_id(frame_id);
        }
        if !self.dependency_group.trim().is_empty() {
            event = event.with_dependency_group(self.dependency_group);
        }
        event
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskStatusJsonV1 {
    pub task_id: String,
    pub job_id: String,
    pub name: String,
    pub lane: String,
    pub priority: String,
    pub frame_id: Option<u64>,
    pub dependency_group: String,
    pub task_pass: String,
    pub task_domain: String,
    pub phase: EngineTaskPhase,
    pub can_pause: bool,
    pub can_cancel: bool,
    pub cancel_requested: bool,
    pub pause_requested: bool,
    pub found: bool,
}

impl Default for TaskStatusJsonV1 {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            job_id: String::new(),
            name: String::new(),
            lane: String::new(),
            priority: String::new(),
            frame_id: None,
            dependency_group: String::new(),
            task_pass: String::new(),
            task_domain: String::new(),
            phase: EngineTaskPhase::Scheduled,
            can_pause: false,
            can_cancel: false,
            cancel_requested: false,
            pause_requested: false,
            found: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TaskControlResponseV1 {
    pub task_id: String,
    pub job_id: String,
    pub action: String,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TaskQueueSnapshotJsonV1 {
    pub worker_threads: usize,
    pub pending_threading: usize,
    pub running_threading: usize,
    pub paused_threading: usize,
    pub submitted_threading: u64,
    pub completed_threading: u64,
    pub cancelled_threading: u64,
    pub panicked_threading: u64,
    pub total_cpu_time_ns: u64,
    pub frame_cpu_budget_ns: u64,
    pub frame_cpu_used_ns: u64,
    pub frame_over_budget: bool,
    pub overbudget_frames: u64,
    pub budget_deferred_polls: u64,
    #[serde(default)]
    pub lanes: BTreeMap<String, TaskQueueLaneSnapshotJsonV1>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TaskQueueLaneSnapshotJsonV1 {
    pub pending_threading: usize,
    pub running_threading: usize,
    pub completed_threading: u64,
    pub cpu_time_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TaskTraceJsonV1 {
    pub task_id: String,
    pub job_id: String,
    pub status: TaskStatusJsonV1,
    pub note: String,
}
