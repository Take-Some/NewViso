use crate::postfx::PostFxPassStats;
use crate::render_graph::{
    RecordedDrawListStats, RenderGraphDiagnosticsStats, RenderGraphSubmitReport,
};
use crate::shadows::ShadowPassStats;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderUploadQueuePolicy {
    Immediate,
    FrameBudgeted,
    BackgroundStaged,
}

impl Default for RenderUploadQueuePolicy {
    #[inline]
    fn default() -> Self {
        Self::FrameBudgeted
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RenderWorkBudget {
    pub max_upload_bytes_per_frame: u64,
    pub max_upload_jobs_per_frame: u32,
    pub max_pipeline_builds_per_frame: u32,
    pub max_blocking_ms_per_frame: f32,
    pub upload_policy: RenderUploadQueuePolicy,
}

impl Default for RenderWorkBudget {
    #[inline]
    fn default() -> Self {
        Self {
            max_upload_bytes_per_frame: 8 * 1024 * 1024,
            max_upload_jobs_per_frame: 4,
            max_pipeline_builds_per_frame: 1,
            max_blocking_ms_per_frame: 2.0,
            upload_policy: RenderUploadQueuePolicy::FrameBudgeted,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RenderFramePacingConfig {
    pub target_frame_ms: f32,
    pub warn_frame_ms: f32,
    pub warn_upload_ms: f32,
    pub warn_pipeline_ms: f32,
}

impl Default for RenderFramePacingConfig {
    #[inline]
    fn default() -> Self {
        Self {
            target_frame_ms: 16.6667,
            warn_frame_ms: 22.0,
            warn_upload_ms: 3.0,
            warn_pipeline_ms: 8.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RenderFrameTiming {
    pub frame_index: u64,
    pub last_begin_frame_ms: f32,
    pub last_frame_slot_wait_ms: f32,
    pub last_surface_acquire_ms: f32,
    pub last_image_wait_ms: f32,
    pub last_end_frame_ms: f32,
    pub last_gpu_submit_ms: f32,
    /// Diagnostic GPU timestamp queries are opt-in and backend-specific.
    pub gpu_timestamps_enabled: bool,
    /// Frame whose completed GPU query results populate the phase fields below.
    pub last_gpu_timing_frame_index: u64,
    pub last_gpu_shadow_ms: f32,
    pub last_gpu_opaque_ms: f32,
    pub last_gpu_postfx_ms: f32,
    pub last_gpu_ui_ms: f32,
    pub last_gpu_profiled_ms: f32,
    pub last_blocking_upload_ms: f32,
    pub last_pipeline_build_ms: f32,
    pub worst_blocking_upload_ms: f32,
    pub worst_pipeline_build_ms: f32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RenderQueueStats {
    pub queued_upload_jobs: u32,
    pub queued_upload_bytes: u64,
    pub queued_pipeline_builds: u32,
    pub queued_shader_bakes: u32,
    pub completed_upload_jobs: u64,
    pub completed_upload_bytes: u64,
    pub blocking_upload_jobs: u64,
    pub blocking_upload_bytes: u64,
    pub pipeline_builds: u64,
    pub pipeline_cache_hits: u64,
    pub pipeline_cache_misses: u64,
    pub shader_cache_hits: u64,
    pub shader_cache_misses: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RenderResourceStats {
    pub buffers: u32,
    pub textures: u32,
    pub samplers: u32,
    pub shaders: u32,
    pub pipelines: u32,
    pub bind_group_layouts: u32,
    pub bind_groups: u32,
    pub render_targets: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderFrameDebugSnapshot {
    pub frame_index: u64,
    pub surface_extent: [u32; 2],
    pub viewport_extent: [u32; 2],
    pub direct_surface_viewport: bool,
    pub graph_label: String,
    #[serde(default)]
    pub phase_order: Vec<String>,
    #[serde(default)]
    pub draw_list_stats: Vec<RecordedDrawListStats>,
    pub executed_passes: u32,
    pub skipped_passes: u32,
    pub cpu_record_ms: f32,
    pub gpu_submit_ms: f32,
    pub queued_upload_jobs: u32,
    pub queued_upload_bytes: u64,
    pub resource_buffers: u32,
    pub resource_textures: u32,
    pub resource_pipelines: u32,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl Default for RenderFrameDebugSnapshot {
    #[inline]
    fn default() -> Self {
        Self {
            frame_index: 0,
            surface_extent: [0, 0],
            viewport_extent: [0, 0],
            direct_surface_viewport: false,
            graph_label: String::new(),
            phase_order: Vec::new(),
            draw_list_stats: Vec::new(),
            executed_passes: 0,
            skipped_passes: 0,
            cpu_record_ms: 0.0,
            gpu_submit_ms: 0.0,
            queued_upload_jobs: 0,
            queued_upload_bytes: 0,
            resource_buffers: 0,
            resource_textures: 0,
            resource_pipelines: 0,
            notes: Vec::new(),
        }
    }
}

impl RenderFrameDebugSnapshot {
    #[inline]
    pub fn draw_calls(&self) -> u32 {
        self.draw_list_stats
            .iter()
            .map(|stats| stats.draw_calls.saturating_add(stats.indexed_draw_calls))
            .sum()
    }

    #[inline]
    pub fn recorded_commands(&self) -> u32 {
        self.draw_list_stats
            .iter()
            .map(|stats| stats.recorded_commands)
            .sum()
    }

    #[inline]
    pub fn skipped_commands(&self) -> u32 {
        self.draw_list_stats
            .iter()
            .map(|stats| stats.skipped_commands)
            .sum()
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RenderDebugChartSample {
    pub frame_index: u64,
    pub fps: f32,
    pub cpu_record_ms: f32,
    pub gpu_submit_ms: f32,
    pub draw_calls: u32,
    pub indexed_draw_calls: u32,
    pub triangle_count: u64,
    pub queued_upload_mb: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderDebugTelemetry {
    pub latest: Option<RenderFrameDebugSnapshot>,
    #[serde(default)]
    pub history: Vec<RenderDebugChartSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderDiagnosticsSnapshot {
    pub frame: RenderFrameTiming,
    pub queue: RenderQueueStats,
    pub resources: RenderResourceStats,
    pub shadows: ShadowPassStats,
    pub postfx: PostFxPassStats,
    #[serde(default)]
    pub graph: RenderGraphDiagnosticsStats,
    #[serde(default)]
    pub last_submit: Option<RenderGraphSubmitReport>,
    pub budget: RenderWorkBudget,
    pub pacing: RenderFramePacingConfig,
    pub notes: Vec<String>,
}

impl Default for RenderDiagnosticsSnapshot {
    #[inline]
    fn default() -> Self {
        Self {
            frame: RenderFrameTiming::default(),
            queue: RenderQueueStats::default(),
            resources: RenderResourceStats::default(),
            shadows: ShadowPassStats::default(),
            postfx: PostFxPassStats::default(),
            graph: RenderGraphDiagnosticsStats::default(),
            last_submit: None,
            budget: RenderWorkBudget::default(),
            pacing: RenderFramePacingConfig::default(),
            notes: Vec::new(),
        }
    }
}
