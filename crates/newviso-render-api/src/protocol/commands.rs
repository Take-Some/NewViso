use super::RenderApiVersion;
use crate::{
    BeginFrameDesc, BeginRenderTargetDesc, BindGroupDesc, BindGroupId, BindGroupLayoutDesc,
    BindGroupLayoutId, BufferDesc, BufferId, BufferSlice, Color4, ComputePipelineDesc,
    DispatchArgs, DrawArgs, DrawIndexedArgs, DrawIndexedIndirectArgs, DrawIndexedIndirectCountArgs,
    GpuVisibilityIndirectCompactArgsV2, GpuVisibilityIndirectCullArgs, IndexFormat, PipelineDesc,
    PipelineId, PipelineWarmupDesc, PipelineWarmupReport, RectI32, RenderBackendCapabilities,
    RenderDiagnosticsSnapshot, RenderDrawListKind, RenderGraphPassKind, RenderLight,
    RenderLightingEnvironment, RenderTargetDesc, RenderTargetId, RenderWorkBudget, SamplerDesc,
    SamplerId, ShaderDesc, ShaderId, ShaderRuntimeCacheStats, TextureDesc, TextureId,
    TextureResidencySnapshot, UiTexId, UploadPumpDesc, UploadPumpReport, Viewport,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderDeviceInfo {
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub driver_version: u32,
    pub api_version: String,
    pub dedicated_vram_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderBackendInfo {
    pub backend_id: String,
    pub backend_name: String,
    pub backend_version: String,
    #[serde(default)]
    pub device: RenderDeviceInfo,
    pub debug_text: String,
    pub clear_color: Color4,
    #[serde(default)]
    pub capabilities: RenderBackendCapabilities,
    #[serde(default)]
    pub work_budget: RenderWorkBudget,
    #[serde(default)]
    pub protocol_version: RenderApiVersion,
}

impl RenderBackendInfo {
    #[inline]
    pub fn with_capabilities(mut self, capabilities: RenderBackendCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    #[inline]
    pub fn with_work_budget(mut self, budget: RenderWorkBudget) -> Self {
        self.work_budget = budget;
        self
    }
}

/// Imperative render device command used inside the stable service protocol.
/// This is the resource/draw command vocabulary shared by runtime, render graph
/// replay and backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderCommand {
    BeginFrame(BeginFrameDesc),
    SetDebugText(String),
    SetRenderPhase {
        phase: Option<RenderGraphPassKind>,
    },
    SetDrawListKind {
        kind: Option<RenderDrawListKind>,
    },
    /// Authoritative scene-local lights for the current frame. The renderer
    /// owns GPU classification/tile/cluster list construction from this packet.
    SetFrameLights(Vec<RenderLight>),
    /// Authoritative scene ambient + primary directional lighting for deferred resolve.
    SetFrameLighting(RenderLightingEnvironment),
    DiscardRecordedCommands,
    EndFrame,
    Resize {
        width: u32,
        height: u32,
    },
    CreateRenderTarget(RenderTargetDesc),
    DestroyRenderTarget {
        id: RenderTargetId,
    },
    RenderTargetUiTexId {
        id: RenderTargetId,
    },
    RenderTargetColorTextureId {
        id: RenderTargetId,
    },
    BeginRenderTarget(BeginRenderTargetDesc),
    EndRenderTarget,
    CreateBuffer(BufferDesc),
    DestroyBuffer {
        id: BufferId,
    },
    WriteBuffer {
        id: BufferId,
        offset: u64,
        data: Vec<u8>,
    },
    CreateTexture(TextureDesc),
    DestroyTexture {
        id: TextureId,
    },
    CreateSampler(SamplerDesc),
    DestroySampler {
        id: SamplerId,
    },
    CreateShader(ShaderDesc),
    DestroyShader {
        id: ShaderId,
    },
    CreatePipeline(PipelineDesc),
    CreateComputePipeline(ComputePipelineDesc),
    DestroyPipeline {
        id: PipelineId,
    },
    CreateBindGroupLayout(BindGroupLayoutDesc),
    DestroyBindGroupLayout {
        id: BindGroupLayoutId,
    },
    CreateBindGroup(BindGroupDesc),
    DestroyBindGroup {
        id: BindGroupId,
    },
    SetViewport(Viewport),
    SetScissor(RectI32),
    SetPipeline {
        pipeline: PipelineId,
    },
    SetBindGroup {
        index: u32,
        group: BindGroupId,
    },
    SetVertexBuffer {
        slot: u32,
        slice: BufferSlice,
    },
    SetIndexBuffer {
        slice: BufferSlice,
        format: IndexFormat,
    },
    Draw(DrawArgs),
    DrawIndexed(DrawIndexedArgs),
    DrawIndexedIndirect(DrawIndexedIndirectArgs),
    DrawIndexedIndirectCount(DrawIndexedIndirectCountArgs),
    DispatchVisibilityIndirectCull(GpuVisibilityIndirectCullArgs),
    DispatchVisibilityIndirectCompactV2(GpuVisibilityIndirectCompactArgsV2),
    Dispatch(DispatchArgs),
    SetWorkBudget(RenderWorkBudget),
    PumpUploads(UploadPumpDesc),
    TextureResidency {
        id: TextureId,
    },
    WarmupPipelines(PipelineWarmupDesc),
    ShaderCacheStats,
    DiagnosticsSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderCommandResponse {
    Unit,
    RenderTargetId(RenderTargetId),
    UiTexId(UiTexId),
    BufferId(BufferId),
    TextureId(TextureId),
    SamplerId(SamplerId),
    ShaderId(ShaderId),
    PipelineId(PipelineId),
    BindGroupLayoutId(BindGroupLayoutId),
    BindGroupId(BindGroupId),
    UploadPumpReport(UploadPumpReport),
    TextureResidency(TextureResidencySnapshot),
    PipelineWarmupReport(PipelineWarmupReport),
    ShaderCacheStats(ShaderRuntimeCacheStats),
    DiagnosticsSnapshot(Box<RenderDiagnosticsSnapshot>),
}

#[inline]
pub fn encode_json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|e| e.to_string())
}

#[inline]
pub fn decode_json<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod backend_info_tests {
    use super::*;

    #[test]
    fn backend_info_without_device_remains_compatible() {
        let value = serde_json::json!({
            "backend_id": "engine.render.test",
            "backend_name": "Test Renderer",
            "backend_version": "1.0.0",
            "debug_text": "",
            "clear_color": [0.0, 0.0, 0.0, 1.0]
        });

        let info: RenderBackendInfo = serde_json::from_value(value).unwrap();
        assert!(info.device.name.is_empty());
        assert_eq!(info.device.dedicated_vram_mb, 0);
    }

    #[test]
    fn backend_info_roundtrips_device_metadata() {
        let value = serde_json::json!({
            "backend_id": "engine.render.vulkan",
            "backend_name": "Vulkan Renderer",
            "backend_version": "0.33.8",
            "device": {
                "name": "GPU",
                "vendor_id": 1,
                "device_id": 2,
                "driver_version": 3,
                "api_version": "1.3.0",
                "dedicated_vram_mb": 4096
            },
            "debug_text": "",
            "clear_color": [0.0, 0.0, 0.0, 1.0]
        });

        let info: RenderBackendInfo = serde_json::from_value(value).unwrap();
        assert_eq!(info.device.name, "GPU");
        assert_eq!(info.device.driver_version, 3);
        assert_eq!(info.device.api_version, "1.3.0");
    }
}
