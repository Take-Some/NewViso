use crate::UploadPumpReport;
use serde::{Deserialize, Serialize};

use super::{
    RenderDrawListKind, RenderGraphPassId, RenderGraphResourceId, RenderGraphResourceUsage,
    TransientResourceAllocationPlan,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderGraphLifetimeStats {
    pub persistent: u32,
    pub transient: u32,
    pub retired: u32,
    pub destroyed: u32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RenderGraphBarrierStats {
    pub read_after_write: u32,
    pub write_after_read: u32,
    pub write_after_write: u32,
    pub external_imports: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderGraphDiagnosticsStats {
    pub compiled_graphs: u64,
    pub submitted_graphs: u64,
    pub skipped_passes: u64,
    pub lifetime: RenderGraphLifetimeStats,
    pub barriers: RenderGraphBarrierStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderGraphValidationIssue {
    pub code: String,
    pub message: String,
    pub pass: Option<RenderGraphPassId>,
    pub resource: Option<RenderGraphResourceId>,
}

impl RenderGraphValidationIssue {
    #[inline]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            pass: None,
            resource: None,
        }
    }

    #[inline]
    pub fn with_pass(mut self, pass: RenderGraphPassId) -> Self {
        self.pass = Some(pass);
        self
    }

    #[inline]
    pub fn with_resource(mut self, resource: RenderGraphResourceId) -> Self {
        self.resource = Some(resource);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RenderGraphDependencyKind {
    ReadAfterWrite,
    WriteAfterRead,
    WriteAfterWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphDependencyEdge {
    pub producer: RenderGraphPassId,
    pub consumer: RenderGraphPassId,
    pub resource: RenderGraphResourceId,
    pub kind: RenderGraphDependencyKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledRenderGraphPass {
    pub id: RenderGraphPassId,
    pub declaration_index: u32,
    pub producers: Vec<RenderGraphPassId>,
    pub consumers: Vec<RenderGraphPassId>,
    #[serde(default)]
    pub culled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphCompiledDag {
    /// Raw pass nodes are retained for diagnostics. Culled nodes are marked on
    /// `CompiledRenderGraphPass`; execution_order contains live passes only.
    pub passes: Vec<CompiledRenderGraphPass>,
    pub edges: Vec<RenderGraphDependencyEdge>,
    pub execution_order: Vec<RenderGraphPassId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphCullingReport {
    pub roots: Vec<RenderGraphPassId>,
    pub live_passes: Vec<RenderGraphPassId>,
    pub culled_passes: Vec<RenderGraphPassId>,
}

impl RenderGraphCullingReport {
    #[inline]
    pub fn live_count(&self) -> usize {
        self.live_passes.len()
    }

    #[inline]
    pub fn culled_count(&self) -> usize {
        self.culled_passes.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderGraphResourceUseKind {
    Create,
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphResourceUse {
    pub execution_index: u32,
    pub pass: RenderGraphPassId,
    pub kind: RenderGraphResourceUseKind,
    #[serde(default)]
    pub usage: Option<RenderGraphResourceUsage>,
}

impl RenderGraphResourceUse {
    #[inline]
    pub const fn new(
        execution_index: u32,
        pass: RenderGraphPassId,
        kind: RenderGraphResourceUseKind,
        usage: Option<RenderGraphResourceUsage>,
    ) -> Self {
        Self {
            execution_index,
            pass,
            kind,
            usage,
        }
    }
}

/// Authoritative lifetime interval for one resource in the compiled live DAG.
///
/// Indices are positions in `RenderGraphCompiledDag::execution_order` after pass
/// culling. `history` contains every create/read/write event observed while walking
/// that live order and is therefore suitable input for later transient allocation,
/// aliasing and barrier-planning passes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLifetimeInterval {
    pub resource: RenderGraphResourceId,
    pub first_pass: RenderGraphPassId,
    pub last_pass: RenderGraphPassId,
    pub first_execution_index: u32,
    pub last_execution_index: u32,
    pub create_count: u32,
    pub read_count: u32,
    pub write_count: u32,
    #[serde(default)]
    pub history: Vec<RenderGraphResourceUse>,
}

impl ResourceLifetimeInterval {
    #[inline]
    pub const fn execution_span(&self) -> u32 {
        self.last_execution_index
            .saturating_sub(self.first_execution_index)
            .saturating_add(1)
    }

    #[inline]
    pub const fn access_count(&self) -> u32 {
        self.create_count
            .saturating_add(self.read_count)
            .saturating_add(self.write_count)
    }

    #[inline]
    pub fn first_live_use(&self) -> Option<&RenderGraphResourceUse> {
        self.history.first()
    }

    #[inline]
    pub fn last_live_use(&self) -> Option<&RenderGraphResourceUse> {
        self.history.last()
    }

    #[inline]
    pub const fn overlaps(&self, other: &Self) -> bool {
        self.first_execution_index <= other.last_execution_index
            && other.first_execution_index <= self.last_execution_index
    }
}

/// Compatibility name kept while downstream providers migrate to the Phase 3 DTO.
pub type CompiledResourceLifetime = ResourceLifetimeInterval;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphResourceLifetimeReport {
    /// Live resource intervals in graph declaration order.
    pub resources: Vec<CompiledResourceLifetime>,
    /// Declared resources with no create/read/write event in the live execution order.
    pub unused_resources: Vec<RenderGraphResourceId>,
}

impl RenderGraphResourceLifetimeReport {
    #[inline]
    pub fn get(&self, resource: RenderGraphResourceId) -> Option<&CompiledResourceLifetime> {
        self.resources
            .iter()
            .find(|entry| entry.resource == resource)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderGraphCompilation {
    pub dag: RenderGraphCompiledDag,
    pub report: RenderGraphCompileReport,
    #[serde(default)]
    pub culling: RenderGraphCullingReport,
    #[serde(default)]
    pub resource_lifetimes: RenderGraphResourceLifetimeReport,
    #[serde(default)]
    pub transient_allocation_plan: TransientResourceAllocationPlan,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderGraphCompileReport {
    pub pass_count: u32,
    pub resource_count: u32,
    pub execution_order: Vec<RenderGraphPassId>,
    pub lifetime: RenderGraphLifetimeStats,
    pub barriers: RenderGraphBarrierStats,
    pub warnings: Vec<RenderGraphValidationIssue>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RecordedDrawListStats {
    pub draw_list: RenderDrawListKind,
    pub recorded_commands: u32,
    pub draw_calls: u32,
    pub indexed_draw_calls: u32,
    pub skipped_commands: u32,
    #[serde(default)]
    pub viewport_sets: u32,
    #[serde(default)]
    pub scissor_sets: u32,
    #[serde(default)]
    pub pipeline_binds: u32,
    #[serde(default)]
    pub vertex_buffer_binds: u32,
    #[serde(default)]
    pub index_buffer_binds: u32,
    #[serde(default)]
    pub bind_group_binds: u32,
    #[serde(default)]
    pub invalid_draw_calls: u32,
}

impl RecordedDrawListStats {
    #[inline]
    pub fn total_draw_calls(self) -> u32 {
        self.draw_calls.saturating_add(self.indexed_draw_calls)
    }

    #[inline]
    pub fn submitted_commands(self) -> u32 {
        self.recorded_commands.saturating_sub(self.skipped_commands)
    }
}

impl Default for RecordedDrawListStats {
    #[inline]
    fn default() -> Self {
        Self {
            draw_list: RenderDrawListKind::OpaqueForward,
            recorded_commands: 0,
            draw_calls: 0,
            indexed_draw_calls: 0,
            skipped_commands: 0,
            viewport_sets: 0,
            scissor_sets: 0,
            pipeline_binds: 0,
            vertex_buffer_binds: 0,
            index_buffer_binds: 0,
            bind_group_binds: 0,
            invalid_draw_calls: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderGraphValidationReport {
    pub ok: bool,
    pub errors: Vec<RenderGraphValidationIssue>,
    pub warnings: Vec<RenderGraphValidationIssue>,
    pub compile: Option<RenderGraphCompileReport>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderGraphSubmitReport {
    pub cpu_record_ms: f32,
    pub gpu_submit_ms: f32,
    pub executed_passes: u32,
    pub skipped_passes: u32,
    /// Backend intentionally deferred native presentation before graph execution.
    /// This is normal bounded WSI back-pressure, not a render-graph failure.
    #[serde(default)]
    pub backend_deferred: bool,
    pub uploads: UploadPumpReport,
    pub compile: RenderGraphCompileReport,
    #[serde(default)]
    pub draw_list_stats: Vec<RecordedDrawListStats>,
}

#[cfg(test)]
mod submit_report_compat_tests {
    use super::*;

    #[test]
    fn default_submit_report_is_not_backend_deferred() {
        assert!(!RenderGraphSubmitReport::default().backend_deferred);
    }
}
