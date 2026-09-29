use newviso_host as host;
use newviso_render_api as render_api;
pub use render_api::{
    Extent2D, RenderDrawListKind, RenderGraphDesc, RenderGraphPassDesc, RenderGraphPassDomain,
    RenderGraphPassId, RenderGraphPassKind, RenderGraphResourceDesc, RenderGraphResourceId,
    RenderGraphResourceSemantic, RenderGraphResourceUsage, RenderLight, RenderLightKind,
    TextureFormat,
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};

const RENDER_SERVICE: &str = "engine.render";
const RENDER_INVOKE: &str = "invoke_json";
const RENDER_COMMAND_BATCH_BIN_V2: &str = "command_batch_bin_v2";
const RENDER_FRAME_STATE_V1: &str = "frame_state_v1";
const RENDER_BEGIN_FRAME_ACQUIRE_V1: &str = "begin_frame_acquire_v1";
const FRAME_OWNED_BUFFER_PREFIX: &str = "frame_owned/";
static TRY_BINARY_WRITE_BUFFER: AtomicBool = AtomicBool::new(true);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderFrameState {
    pub active: bool,
    pub frame_slot: usize,
    pub frames_in_flight: usize,
    pub engine_frame_index: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}
impl ShaderStage {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Vertex => "Vertex",
            Self::Fragment => "Fragment",
            Self::Compute => "Compute",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VertexFormat {
    Float32x2,
    Float32x3,
    Float32x4,
}
impl VertexFormat {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Float32x2 => "Float32x2",
            Self::Float32x3 => "Float32x3",
            Self::Float32x4 => "Float32x4",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VertexAttribute {
    pub location: u32,
    pub offset: u64,
    pub format: VertexFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VertexStepMode {
    Vertex,
    Instance,
}

impl VertexStepMode {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Vertex => "Vertex",
            Self::Instance => "Instance",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VertexLayoutDesc<'a> {
    pub stride: u64,
    pub attributes: &'a [VertexAttribute],
    pub step_mode: VertexStepMode,
}

#[derive(Clone, Debug)]
pub struct GraphicsPipelineDesc<'a> {
    pub label: &'a str,
    pub vertex_shader: u32,
    pub fragment_shader: u32,
    pub vertex_stride: u64,
    pub attributes: &'a [VertexAttribute],
    pub topology: &'a str,
    pub bind_group_layouts: &'a [u32],
    pub color_format: &'a str,
    pub depth_format: Option<&'a str>,
    pub depth_test: bool,
    pub depth_write: bool,
    pub depth_compare: &'a str,
    pub cull_mode: &'a str,
    pub blend_mode: &'a str,
    pub cache_key: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct TextureMipUpload {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub offset: u64,
    pub byte_len: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureResidencyState {
    Missing,
    Queued,
    Uploading,
    Ready,
    Failed,
}

#[derive(Clone, Debug)]
pub struct TextureResidencyInfo {
    pub state: TextureResidencyState,
    pub queued_bytes: u64,
    pub uploaded_bytes: u64,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UploadPumpInfo {
    pub processed_jobs: u32,
    pub processed_bytes: u64,
    pub remaining_jobs: u32,
    pub remaining_bytes: u64,
    pub blocked_by_budget: bool,
    pub failed_jobs: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderClient;

impl RenderClient {
    pub const fn new() -> Self {
        Self
    }

    pub fn create_buffer(
        &self,
        label: &str,
        size: u64,
        usage: &str,
        memory: &str,
    ) -> Result<u32, String> {
        command_id(
            self.command(
                json!({"CreateBuffer":{"label":label,"size":size,"usage":usage,"memory":memory}}),
            )?,
            "BufferId",
        )
    }

    /// Creates a buffer whose writes are owned by one acquired renderer frame slot.
    ///
    /// The Vulkan provider recognizes the reserved label prefix and may skip the
    /// compatibility all-frame fence barrier. Callers must only write a slot after
    /// BeginFrame has acquired that exact slot.
    pub fn create_frame_buffer(
        &self,
        frame_slot: usize,
        label: &str,
        size: u64,
        usage: &str,
        memory: &str,
    ) -> Result<u32, String> {
        self.create_buffer(
            &format!("{FRAME_OWNED_BUFFER_PREFIX}{frame_slot}/{label}"),
            size,
            usage,
            memory,
        )
    }

    pub fn write_buffer(&self, id: u32, offset: u64, data: &[u8]) -> Result<(), String> {
        // WriteBuffer is a frame hot-path command. Sending byte payloads through
        // invoke_json expands every byte into a JSON integer and makes animated
        // vertex uploads catastrophically expensive. The renderer ABI already
        // supports WriteBuffer as command_batch_bin_v2 tag 1, so use the raw
        // binary service path and retain JSON only as an old-provider fallback.
        if TRY_BINARY_WRITE_BUFFER.load(Ordering::Relaxed) {
            let packet = encode_write_buffer_bin_packet(id, offset, data)?;
            match host::call_service(RENDER_SERVICE, RENDER_COMMAND_BATCH_BIN_V2, &packet) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    let detail = error.to_string();
                    let unsupported = detail.contains("unsupported")
                        || detail.contains("unknown method")
                        || detail.contains("not found")
                        || detail.contains("unknown render command batch binary tag");
                    if unsupported {
                        TRY_BINARY_WRITE_BUFFER.store(false, Ordering::Relaxed);
                    } else {
                        return Err(detail);
                    }
                }
            }
        }

        self.unit(json!({"WriteBuffer":{"id":id,"offset":offset,"data":data}}))
    }

    pub fn write_buffer_f32(&self, id: u32, offset: u64, values: &[f32]) -> Result<(), String> {
        #[cfg(target_endian = "little")]
        {
            let data = unsafe {
                std::slice::from_raw_parts(
                    values.as_ptr().cast::<u8>(),
                    std::mem::size_of_val(values),
                )
            };
            return self.write_buffer(id, offset, data);
        }

        #[cfg(not(target_endian = "little"))]
        {
            let mut data = Vec::with_capacity(std::mem::size_of_val(values));
            for value in values {
                data.extend_from_slice(&value.to_le_bytes());
            }
            self.write_buffer(id, offset, &data)
        }
    }

    /// Upload several disjoint f32 ranges from one source buffer through one
    /// render-service call. This is the hot path for CPU-skinned characters:
    /// the Vulkan buffer is CpuToGpu/host-visible, so the backend can memcpy
    /// each range while holding the render API once instead of crossing the
    /// service ABI once per character.
    pub fn write_buffer_f32_ranges(
        &self,
        id: u32,
        values: &[f32],
        ranges: &[(usize, usize)],
    ) -> Result<(), String> {
        if ranges.is_empty() {
            return Ok(());
        }
        if ranges.len() == 1 {
            let (start, end) = ranges[0];
            if start > end || end > values.len() {
                return Err("write_buffer_f32_ranges contains invalid range".to_owned());
            }
            return self.write_buffer_f32(
                id,
                start as u64 * std::mem::size_of::<f32>() as u64,
                &values[start..end],
            );
        }

        #[cfg(target_endian = "little")]
        if TRY_BINARY_WRITE_BUFFER.load(Ordering::Relaxed) {
            let packet = encode_write_buffer_f32_ranges_bin_packet(id, values, ranges)?;
            match host::call_service(RENDER_SERVICE, RENDER_COMMAND_BATCH_BIN_V2, &packet) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    let detail = error.to_string();
                    let unsupported = detail.contains("unsupported")
                        || detail.contains("unknown method")
                        || detail.contains("not found")
                        || detail.contains("unknown render command batch binary tag");
                    if unsupported {
                        TRY_BINARY_WRITE_BUFFER.store(false, Ordering::Relaxed);
                    } else {
                        return Err(detail);
                    }
                }
            }
        }

        for &(start, end) in ranges {
            if start > end || end > values.len() {
                return Err("write_buffer_f32_ranges contains invalid range".to_owned());
            }
            self.write_buffer_f32(
                id,
                start as u64 * std::mem::size_of::<f32>() as u64,
                &values[start..end],
            )?;
        }
        Ok(())
    }

    pub fn create_texture(
        &self,
        label: &str,
        width: u32,
        height: u32,
        format: &str,
        mips: &[TextureMipUpload],
        data: &[u8],
    ) -> Result<u32, String> {
        self.create_texture_with_policy(label, width, height, format, mips, data, "Immediate")
    }

    pub fn create_texture_deferred(
        &self,
        label: &str,
        width: u32,
        height: u32,
        format: &str,
        mips: &[TextureMipUpload],
        data: &[u8],
    ) -> Result<u32, String> {
        self.create_texture_with_policy(label, width, height, format, mips, data, "Deferred")
    }

    fn create_texture_with_policy(
        &self,
        label: &str,
        width: u32,
        height: u32,
        format: &str,
        mips: &[TextureMipUpload],
        data: &[u8],
        data_policy: &str,
    ) -> Result<u32, String> {
        let packet = encode_create_texture_bin_packet(
            label,
            width,
            height,
            format,
            mips,
            data,
            data_policy,
        )?;
        let response = host::call_service(RENDER_SERVICE, "create_texture_bin_v1", &packet)?;
        decode_texture_id_bin_packet(&response)
    }

    /// Sets the backend-owned per-frame GPU/transfer work budget.
    ///
    /// Upload execution belongs to the render backend. Scene/resource systems
    /// should queue deferred work and let BeginFrame/RenderGraph consume it under
    /// this single budget rather than running additional pumps themselves.
    pub fn set_work_budget(
        &self,
        max_upload_bytes_per_frame: u64,
        max_upload_jobs_per_frame: u32,
        max_pipeline_builds_per_frame: u32,
        max_blocking_ms_per_frame: f32,
    ) -> Result<(), String> {
        let budget = render_api::RenderWorkBudget {
            max_upload_bytes_per_frame,
            max_upload_jobs_per_frame,
            max_pipeline_builds_per_frame,
            max_blocking_ms_per_frame,
            upload_policy: render_api::RenderUploadQueuePolicy::FrameBudgeted,
        };
        match self.typed_request(render_api::RenderServiceRequest::SetWorkBudget(budget))? {
            render_api::RenderServiceResponse::Unit => Ok(()),
            other => Err(format!(
                "render service expected Unit for SetWorkBudget, got {other:?}"
            )),
        }
    }

    pub fn validate_render_graph(
        &self,
        graph: render_api::RenderGraphDesc,
    ) -> Result<render_api::RenderGraphValidationReport, String> {
        match self.typed_request(render_api::RenderServiceRequest::ValidateRenderGraph(graph))? {
            render_api::RenderServiceResponse::GraphValidationReport(report) => Ok(report),
            other => Err(format!(
                "render service expected GraphValidationReport, got {other:?}"
            )),
        }
    }

    pub fn compile_render_graph(
        &self,
        graph: render_api::RenderGraphDesc,
    ) -> Result<render_api::RenderGraphCompileReport, String> {
        match self.typed_request(render_api::RenderServiceRequest::CompileRenderGraph(graph))? {
            render_api::RenderServiceResponse::GraphCompileReport(report) => Ok(report),
            other => Err(format!(
                "render service expected GraphCompileReport, got {other:?}"
            )),
        }
    }

    pub fn submit_render_graph(
        &self,
        graph: render_api::RenderGraphDesc,
    ) -> Result<render_api::RenderGraphSubmitReport, String> {
        match self.typed_request(render_api::RenderServiceRequest::SubmitRenderGraph(graph))? {
            render_api::RenderServiceResponse::GraphSubmitReport(report) => Ok(report),
            other => Err(format!(
                "render service expected GraphSubmitReport, got {other:?}"
            )),
        }
    }

    pub fn set_draw_list_kind(
        &self,
        kind: Option<render_api::RenderDrawListKind>,
    ) -> Result<(), String> {
        match self.typed_request(render_api::RenderServiceRequest::SetDrawListKind { kind })? {
            render_api::RenderServiceResponse::Unit => Ok(()),
            other => Err(format!(
                "render service expected Unit for SetDrawListKind, got {other:?}"
            )),
        }
    }

    pub fn set_frame_lights(&self, lights: &[RenderLight]) -> Result<(), String> {
        self.unit(json!({"SetFrameLights":lights}))
    }

    pub fn pump_uploads(
        &self,
        max_bytes: u64,
        max_jobs: u32,
        max_blocking_ms: f32,
    ) -> Result<UploadPumpInfo, String> {
        let response = self.command(json!({"PumpUploads":{
            "reason":"Explicit",
            "budget":{
                "max_upload_bytes_per_frame":max_bytes,
                "max_upload_jobs_per_frame":max_jobs,
                "max_pipeline_builds_per_frame":0,
                "max_blocking_ms_per_frame":max_blocking_ms,
                "upload_policy":"FrameBudgeted"
            }
        }}))?;
        let value = response
            .get("UploadPumpReport")
            .ok_or_else(|| format!("render service expected UploadPumpReport, got {response}"))?;
        Ok(UploadPumpInfo {
            processed_jobs: value
                .get("processed_jobs")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32,
            processed_bytes: value
                .get("processed_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            remaining_jobs: value
                .get("remaining_jobs")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32,
            remaining_bytes: value
                .get("remaining_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            blocked_by_budget: value
                .get("blocked_by_budget")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            failed_jobs: value
                .get("failed_jobs")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32,
        })
    }

    pub fn texture_residency(&self, id: u32) -> Result<TextureResidencyInfo, String> {
        let response = self.command(json!({"TextureResidency":{"id":id}}))?;
        let value = response
            .get("TextureResidency")
            .ok_or_else(|| format!("render service expected TextureResidency, got {response}"))?;
        let state = match value
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("Missing")
        {
            "Missing" => TextureResidencyState::Missing,
            "Queued" => TextureResidencyState::Queued,
            "Uploading" => TextureResidencyState::Uploading,
            "Ready" => TextureResidencyState::Ready,
            "Failed" => TextureResidencyState::Failed,
            other => return Err(format!("unknown texture residency state '{other}'")),
        };
        Ok(TextureResidencyInfo {
            state,
            queued_bytes: value
                .get("queued_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            uploaded_bytes: value
                .get("uploaded_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            message: value
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }

    pub fn create_sampler_repeat_linear(&self, label: &str) -> Result<u32, String> {
        command_id(
            self.command(json!({"CreateSampler":{
                "label":label,
                "min_filter":"Linear","mag_filter":"Linear","mip_filter":"Linear",
                "address_u":"Repeat","address_v":"Repeat","address_w":"Repeat"
            }}))?,
            "SamplerId",
        )
    }

    pub fn create_sampler_clamp_linear(&self, label: &str) -> Result<u32, String> {
        command_id(
            self.command(json!({"CreateSampler":{
                "label":label,
                "min_filter":"Linear","mag_filter":"Linear","mip_filter":"Linear",
                "address_u":"ClampToEdge","address_v":"ClampToEdge","address_w":"ClampToEdge"
            }}))?,
            "SamplerId",
        )
    }

    pub fn create_render_target(
        &self,
        label: &str,
        width: u32,
        height: u32,
        color_format: &str,
        depth_format: Option<&str>,
    ) -> Result<u32, String> {
        command_id(
            self.command(json!({"CreateRenderTarget":{
                "extent":{"width":width,"height":height},
                "color":color_format,
                "depth":depth_format,
                "label":label
            }}))?,
            "RenderTargetId",
        )
    }

    pub fn render_target_color_texture(&self, id: u32) -> Result<u32, String> {
        command_id(
            self.command(json!({"RenderTargetColorTextureId":{"id":id}}))?,
            "TextureId",
        )
    }

    pub fn begin_render_target(
        &self,
        id: u32,
        clear_color: Option<[f32; 4]>,
        clear_depth: Option<f32>,
    ) -> Result<(), String> {
        self.unit(json!({"BeginRenderTarget":{
            "target":id,
            "clear_color":clear_color,
            "clear_depth":clear_depth,
            "clear_stencil":null
        }}))
    }

    pub fn end_render_target(&self) -> Result<(), String> {
        self.unit(json!("EndRenderTarget"))
    }

    pub fn create_bind_group_layout(&self, label: &str, bindings: &[&str]) -> Result<u32, String> {
        command_id(
            self.command(json!({"CreateBindGroupLayout":{
                "label":label,"bindings":bindings
            }}))?,
            "BindGroupLayoutId",
        )
    }

    pub fn create_bind_group(
        &self,
        label: &str,
        layout: u32,
        textures: [Option<u32>; 4],
        sampler: Option<u32>,
        uniform: Option<(u32, u64, u64)>,
    ) -> Result<u32, String> {
        self.create_bind_group6(
            label,
            layout,
            [
                textures[0],
                textures[1],
                textures[2],
                textures[3],
                None,
                None,
            ],
            sampler,
            uniform,
        )
    }

    pub fn create_bind_group6(
        &self,
        label: &str,
        layout: u32,
        textures: [Option<u32>; 6],
        sampler: Option<u32>,
        uniform: Option<(u32, u64, u64)>,
    ) -> Result<u32, String> {
        let uniform0 = uniform.map(
            |(buffer, offset, size)| json!({"buffer": buffer, "offset": offset, "size": size}),
        );
        command_id(
            self.command(json!({"CreateBindGroup":{
                "label":label,"layout":layout,
                "texture0":textures[0],"texture1":textures[1],"texture2":textures[2],
                "texture3":textures[3],"texture4":textures[4],"texture5":textures[5],
                "graph_texture_fallback":null,
                "sampler0":sampler,
                "uniform0":uniform0,"storage0":null,"storage1":null,"storage2":null
            }}))?,
            "BindGroupId",
        )
    }

    pub fn create_shader(
        &self,
        label: &str,
        stage: ShaderStage,
        logical_path: &str,
        variant_id: &str,
    ) -> Result<u32, String> {
        command_id(self.command(json!({"CreateShader":{
            "label":label,"stage":stage.wire_name(),"entry":"main","spirv":[],
            "asset":{"logical_path":logical_path,"source_kind":"Glsl","entry":"main","variant_id":variant_id,"defines":[],"optional":false}
        }}))?,"ShaderId")
    }

    pub fn create_shader_spirv(
        &self,
        label: &str,
        stage: ShaderStage,
        bytes: &[u8],
    ) -> Result<u32, String> {
        if bytes.len() < 20 || bytes.len() % 4 != 0 {
            return Err("SPIR-V byte length must contain an aligned header".to_owned());
        }
        let words: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        if words[0] != 0x0723_0203 {
            return Err("invalid SPIR-V magic".to_owned());
        }
        command_id(
            self.command(json!({"CreateShader":{
                "label":label,"stage":stage.wire_name(),"entry":"main","spirv":words,"asset":null
            }}))?,
            "ShaderId",
        )
    }

    pub fn create_pipeline(&self, desc: GraphicsPipelineDesc<'_>) -> Result<u32, String> {
        let layout = [VertexLayoutDesc {
            stride: desc.vertex_stride,
            attributes: desc.attributes,
            step_mode: VertexStepMode::Vertex,
        }];
        self.create_pipeline_with_layouts(desc, &layout)
    }

    pub fn create_pipeline_with_layouts(
        &self,
        desc: GraphicsPipelineDesc<'_>,
        layouts: &[VertexLayoutDesc<'_>],
    ) -> Result<u32, String> {
        self.create_pipeline_with_layouts_and_color_formats(desc, layouts, &[])
    }

    /// Creates a graphics pipeline with an explicit MRT color-attachment contract.
    ///
    /// Color formats follow fragment output locations. Passing an empty slice is
    /// reserved for the compatibility single-attachment path above.
    pub fn create_pipeline_mrt_with_layouts(
        &self,
        desc: GraphicsPipelineDesc<'_>,
        layouts: &[VertexLayoutDesc<'_>],
        color_formats: &[&str],
    ) -> Result<u32, String> {
        if color_formats.len() < 2 {
            return Err("MRT pipeline requires at least two color formats".to_owned());
        }
        self.create_pipeline_with_layouts_and_color_formats(desc, layouts, color_formats)
    }

    fn create_pipeline_with_layouts_and_color_formats(
        &self,
        desc: GraphicsPipelineDesc<'_>,
        layouts: &[VertexLayoutDesc<'_>],
        color_formats: &[&str],
    ) -> Result<u32, String> {
        let vertex_layouts = layouts
            .iter()
            .map(|layout| {
                let attributes = layout
                    .attributes
                    .iter()
                    .map(|a| {
                        json!({
                            "location":a.location,"offset":a.offset,"format":a.format.wire_name()
                        })
                    })
                    .collect::<Vec<_>>();
                json!({
                    "stride": layout.stride,
                    "attributes": attributes,
                    "step_mode": layout.step_mode.wire_name()
                })
            })
            .collect::<Vec<_>>();
        command_id(self.command(json!({"CreatePipeline":{
            "label":desc.label,"vs":desc.vertex_shader,"fs":desc.fragment_shader,
            "topology":desc.topology,
            "vertex_layouts":vertex_layouts,
            "bind_group_layouts":desc.bind_group_layouts,
            "color_format":desc.color_format,"color_formats":color_formats,
            "depth_format":desc.depth_format,
            "depth_mode":{"test":desc.depth_test,"write":desc.depth_write,"compare":desc.depth_compare},
            "cull_mode":desc.cull_mode,"blend_mode":desc.blend_mode,
            "depth_bias_constant":0.0,"depth_bias_slope":0.0,"depth_bias_clamp":0.0,
            "cache_key":desc.cache_key,
            "tessellation":{"mode":"Disabled","factor":4.0,"min_distance":8.0,"max_distance":96.0},
            "warmup":false
        }}))?,"PipelineId")
    }

    pub fn create_compute_pipeline(
        &self,
        label: &str,
        compute_shader: u32,
        bind_group_layouts: &[u32],
        cache_key: &str,
    ) -> Result<u32, String> {
        command_id(
            self.command(json!({"CreateComputePipeline":{
                "label":label,
                "cs":compute_shader,
                "bind_group_layouts":bind_group_layouts,
                "cache_key":cache_key
            }}))?,
            "PipelineId",
        )
    }

    pub fn dispatch(&self, groups_x: u32, groups_y: u32, groups_z: u32) -> Result<(), String> {
        self.unit(json!({"Dispatch":{
            "groups_x":groups_x,
            "groups_y":groups_y,
            "groups_z":groups_z
        }}))
    }

    pub fn begin_frame(&self, clear_color: [f32; 4], frame_index: u64) -> Result<(), String> {
        self.unit(json!({"BeginFrame":{"clear_color":clear_color,"frame_index":frame_index}}))
    }
    pub fn frame_state(&self) -> Result<RenderFrameState, String> {
        let value = host::call_json(RENDER_SERVICE, RENDER_FRAME_STATE_V1, &json!({}))?;
        Ok(RenderFrameState {
            active: value
                .get("active")
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("render frame state has no active flag: {value}"))?,
            frame_slot: value
                .get("frame_slot")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("render frame state has no frame_slot: {value}"))?
                as usize,
            frames_in_flight: value
                .get("frames_in_flight")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("render frame state has no frames_in_flight: {value}"))?
                as usize,
            engine_frame_index: value
                .get("engine_frame_index")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("render frame state has no engine_frame_index: {value}"))?,
        })
    }

    pub fn begin_frame_acquire(
        &self,
        clear_color: [f32; 4],
        frame_index: u64,
    ) -> Result<RenderFrameState, String> {
        let value = host::call_json(
            RENDER_SERVICE,
            RENDER_BEGIN_FRAME_ACQUIRE_V1,
            &json!({"BeginFrame":{"clear_color":clear_color,"frame_index":frame_index}}),
        )?;
        Ok(RenderFrameState {
            active: value
                .get("active")
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("render begin-frame state has no active flag: {value}"))?,
            frame_slot: value
                .get("frame_slot")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("render begin-frame state has no frame_slot: {value}"))?
                as usize,
            frames_in_flight: value
                .get("frames_in_flight")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    format!("render begin-frame state has no frames_in_flight: {value}")
                })? as usize,
            engine_frame_index: value
                .get("engine_frame_index")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    format!("render begin-frame state has no engine_frame_index: {value}")
                })?,
        })
    }
    pub fn set_viewport(&self, width: u32, height: u32) -> Result<(), String> {
        self.unit(json!({"SetViewport":{"x":0.0,"y":0.0,"w":width as f32,"h":height as f32,"min_depth":0.0,"max_depth":1.0}}))
    }
    pub fn set_scissor(&self, width: u32, height: u32) -> Result<(), String> {
        self.unit(json!({"SetScissor":{"x":0,"y":0,"w":width,"h":height}}))
    }
    pub fn set_ui_draw_list(&self, draw_list: Value) -> Result<(), String> {
        self.unit(json!({"SetUiDrawList":draw_list}))
    }
    pub fn set_render_phase(&self, phase: Option<&str>) -> Result<(), String> {
        self.unit(json!({"SetRenderPhase":{"phase":phase}}))
    }

    pub fn set_pipeline(&self, pipeline: u32) -> Result<(), String> {
        self.unit(json!({"SetPipeline":{"pipeline":pipeline}}))
    }
    pub fn set_bind_group(&self, index: u32, group: u32) -> Result<(), String> {
        self.unit(json!({"SetBindGroup":{"index":index,"group":group}}))
    }
    pub fn set_vertex_buffer(&self, slot: u32, buffer: u32, offset: u64) -> Result<(), String> {
        self.unit(
            json!({"SetVertexBuffer":{"slot":slot,"slice":{"buffer":buffer,"offset":offset}}}),
        )
    }
    pub fn set_index_buffer(&self, buffer: u32, offset: u64, format: &str) -> Result<(), String> {
        self.unit(
            json!({"SetIndexBuffer":{"slice":{"buffer":buffer,"offset":offset},"format":format}}),
        )
    }
    pub fn draw(&self, vertex_count: u32) -> Result<(), String> {
        self.draw_range(vertex_count, 0)
    }
    pub fn draw_range(&self, vertex_count: u32, first_vertex: u32) -> Result<(), String> {
        self.draw_range_instanced(vertex_count, first_vertex, 1, 0)
    }

    pub fn draw_range_instanced(
        &self,
        vertex_count: u32,
        first_vertex: u32,
        instance_count: u32,
        first_instance: u32,
    ) -> Result<(), String> {
        self.unit(json!({"Draw":{
            "vertex_count":vertex_count,
            "instance_count":instance_count,
            "first_vertex":first_vertex,
            "first_instance":first_instance
        }}))
    }
    pub fn draw_indexed(&self, index_count: u32) -> Result<(), String> {
        self.draw_indexed_range_instanced(index_count, 0, 0, 1, 0)
    }

    pub fn draw_indexed_range_instanced(
        &self,
        index_count: u32,
        first_index: u32,
        vertex_offset: i32,
        instance_count: u32,
        first_instance: u32,
    ) -> Result<(), String> {
        self.unit(json!({"DrawIndexed":{
            "index_count":index_count,
            "instance_count":instance_count,
            "first_index":first_index,
            "vertex_offset":vertex_offset,
            "first_instance":first_instance
        }}))
    }
    pub fn dispatch_visibility_indirect_cull(
        &self,
        candidate_buffer: u32,
        indirect_buffer: u32,
        candidate_count: u32,
        viewport_extent: [u32; 2],
        camera_position: [f32; 3],
        camera_forward: [f32; 3],
        camera_up: [f32; 3],
        fov_y_radians: f32,
        near: f32,
        far: f32,
    ) -> Result<(), String> {
        self.unit(json!({"DispatchVisibilityIndirectCull":{
            "candidate_buffer":candidate_buffer,
            "candidate_offset":0,
            "indirect_buffer":indirect_buffer,
            "indirect_offset":0,
            "candidate_count":candidate_count,
            "candidate_stride":32,
            "command_stride":20,
            "viewport_extent":viewport_extent,
            "camera":{
                "position_ws":camera_position,
                "forward_ws":camera_forward,
                "up_ws":camera_up,
                "fov_y":fov_y_radians,
                "near":near,
                "far":far
            }
        }}))
    }

    pub fn draw_indexed_indirect(
        &self,
        buffer: u32,
        offset: u64,
        draw_count: u32,
        stride: u32,
    ) -> Result<(), String> {
        self.unit(json!({"DrawIndexedIndirect":{
            "buffer":buffer,
            "offset":offset,
            "draw_count":draw_count,
            "stride":stride
        }}))
    }

    pub fn end_frame(&self) -> Result<(), String> {
        self.unit(json!("EndFrame"))
    }
    pub fn abort_frame(&self) {
        let _ = self.request(json!("AbortFrame"));
    }

    pub fn destroy_render_target(&self, id: u32) {
        let _ = self.unit(json!({"DestroyRenderTarget":{"id":id}}));
    }

    pub fn destroy_pipeline(&self, id: u32) {
        let _ = self.unit(json!({"DestroyPipeline":{"id":id}}));
    }

    pub fn destroy_shader(&self, id: u32) {
        let _ = self.unit(json!({"DestroyShader":{"id":id}}));
    }
    pub fn destroy_buffer(&self, id: u32) {
        let _ = self.unit(json!({"DestroyBuffer":{"id":id}}));
    }
    pub fn destroy_texture(&self, id: u32) {
        let _ = self.unit(json!({"DestroyTexture":{"id":id}}));
    }
    pub fn destroy_sampler(&self, id: u32) {
        let _ = self.unit(json!({"DestroySampler":{"id":id}}));
    }
    pub fn destroy_bind_group(&self, id: u32) {
        let _ = self.unit(json!({"DestroyBindGroup":{"id":id}}));
    }
    pub fn destroy_bind_group_layout(&self, id: u32) {
        let _ = self.unit(json!({"DestroyBindGroupLayout":{"id":id}}));
    }

    fn typed_request(
        &self,
        request: render_api::RenderServiceRequest,
    ) -> Result<render_api::RenderServiceResponse, String> {
        let request = serde_json::to_value(request)
            .map_err(|error| format!("render request serialization failed: {error}"))?;
        let response = host::call_json(RENDER_SERVICE, RENDER_INVOKE, &request)?;
        let response: render_api::RenderServiceResponse = serde_json::from_value(response)
            .map_err(|error| format!("render response decode failed: {error}"))?;
        match response {
            render_api::RenderServiceResponse::Problem(problem) => {
                Err(format!("render service problem: {problem:?}"))
            }
            other => Ok(other),
        }
    }

    fn request(&self, request: Value) -> Result<Value, String> {
        let response = host::call_json(RENDER_SERVICE, RENDER_INVOKE, &request)?;
        if let Some(problem) = response.get("Problem") {
            return Err(format!("render service problem: {problem}"));
        }
        Ok(response)
    }
    fn command(&self, command: Value) -> Result<Value, String> {
        let response = self.request(json!({"Command":command}))?;
        response
            .get("Command")
            .cloned()
            .ok_or_else(|| format!("render service expected Command response, got {response}"))
    }
    fn unit(&self, command: Value) -> Result<(), String> {
        let response = self.command(command)?;
        if response == Value::String("Unit".to_owned()) || response.get("Unit").is_some() {
            return Ok(());
        }
        Err(format!(
            "render service expected unit command response, got {response}"
        ))
    }
}

#[cfg(target_endian = "little")]
fn encode_write_buffer_f32_ranges_bin_packet(
    id: u32,
    values: &[f32],
    ranges: &[(usize, usize)],
) -> Result<Vec<u8>, String> {
    let command_count = u32::try_from(ranges.len())
        .map_err(|_| "write-buffer range count exceeds u32".to_owned())?;
    let payload_bytes = ranges.iter().try_fold(0usize, |total, &(start, end)| {
        if start > end || end > values.len() {
            return Err("write-buffer range exceeds source f32 slice".to_owned());
        }
        total
            .checked_add((end - start).saturating_mul(std::mem::size_of::<f32>()))
            .ok_or_else(|| "write-buffer batch payload size overflow".to_owned())
    })?;
    let mut out =
        Vec::with_capacity(payload_bytes.saturating_add(12 + ranges.len().saturating_mul(17)));
    out.extend_from_slice(b"NECB\x02\0\0\0");
    put_u32(&mut out, command_count);
    for &(start, end) in ranges {
        let data = unsafe {
            std::slice::from_raw_parts(
                values[start..end].as_ptr().cast::<u8>(),
                (end - start).saturating_mul(std::mem::size_of::<f32>()),
            )
        };
        let len = u32::try_from(data.len())
            .map_err(|_| "write-buffer range payload exceeds u32".to_owned())?;
        out.push(1); // RenderCommand::WriteBuffer
        put_u32(&mut out, id);
        put_u64(&mut out, start as u64 * std::mem::size_of::<f32>() as u64);
        put_u32(&mut out, len);
        out.extend_from_slice(data);
    }
    Ok(out)
}

fn encode_write_buffer_bin_packet(id: u32, offset: u64, data: &[u8]) -> Result<Vec<u8>, String> {
    let len = u32::try_from(data.len())
        .map_err(|_| "write-buffer payload is too large for binary render packet".to_owned())?;
    let mut out = Vec::with_capacity(data.len().saturating_add(25));
    out.extend_from_slice(b"NECB\x02\0\0\0");
    put_u32(&mut out, 1); // command count
    out.push(1); // RenderCommand::WriteBuffer
    put_u32(&mut out, id);
    put_u64(&mut out, offset);
    put_u32(&mut out, len);
    out.extend_from_slice(data);
    Ok(out)
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_bytes(out: &mut Vec<u8>, value: &[u8], what: &str) -> Result<(), String> {
    let len = u32::try_from(value.len())
        .map_err(|_| format!("{what} is too large for binary render packet"))?;
    put_u32(out, len);
    out.extend_from_slice(value);
    Ok(())
}

fn texture_format_bin_tag(value: &str) -> Result<u8, String> {
    match value {
        "Rgba8Unorm" => Ok(1),
        "Rgba8Srgb" => Ok(2),
        "Bgra8Unorm" => Ok(3),
        "Bgra8Srgb" => Ok(4),
        "Rgba16Float" => Ok(5),
        "R32Float" => Ok(6),
        "Bc1RgbaUnorm" => Ok(7),
        "Bc1RgbaSrgb" => Ok(8),
        "Bc3RgbaUnorm" => Ok(9),
        "Bc3RgbaSrgb" => Ok(10),
        "Bc5RgUnorm" => Ok(11),
        "Bc7RgbaUnorm" => Ok(12),
        "Bc7RgbaSrgb" => Ok(13),
        "Depth24Stencil8" => Ok(14),
        "Depth32Float" => Ok(15),
        other => Err(format!(
            "unsupported binary render texture format '{other}'"
        )),
    }
}

fn encode_create_texture_bin_packet(
    label: &str,
    width: u32,
    height: u32,
    format: &str,
    mips: &[TextureMipUpload],
    data: &[u8],
    data_policy: &str,
) -> Result<Vec<u8>, String> {
    let mip_count =
        u32::try_from(mips.len()).map_err(|_| "texture mip count exceeds u32".to_owned())?;
    let mip_levels =
        u32::try_from(mips.len().max(1)).map_err(|_| "texture mip count exceeds u32".to_owned())?;
    let policy_tag = match data_policy {
        "Immediate" => 1u8,
        "Deferred" => 2u8,
        other => return Err(format!("unsupported texture data policy '{other}'")),
    };

    let mut out = Vec::with_capacity(data.len().saturating_add(128));
    out.extend_from_slice(b"NECT\x01\0\0\0");
    out.push(1); // label present
    put_bytes(&mut out, label.as_bytes(), "texture label")?;
    put_u32(&mut out, width);
    put_u32(&mut out, height);
    out.push(texture_format_bin_tag(format)?);
    out.push(1); // TextureUsage::Sampled
    put_u32(&mut out, mip_levels);
    out.push(policy_tag);
    put_u32(&mut out, mip_count);
    for mip in mips {
        put_u32(&mut out, mip.level);
        put_u32(&mut out, mip.width);
        put_u32(&mut out, mip.height);
        put_u64(&mut out, mip.offset);
        put_u64(&mut out, mip.byte_len);
    }
    out.push(1); // payload present
    put_bytes(&mut out, data, "texture payload")?;
    Ok(out)
}

fn decode_texture_id_bin_packet(bytes: &[u8]) -> Result<u32, String> {
    if bytes.len() != 12 || &bytes[..8] != b"NETR\x01\0\0\0" {
        return Err("create-texture binary response has invalid packet".to_owned());
    }
    Ok(u32::from_le_bytes(bytes[8..12].try_into().map_err(
        |_| "create-texture binary response is truncated".to_owned(),
    )?))
}

fn command_id(response: Value, field: &str) -> Result<u32, String> {
    let value = response
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("render response has no {field}: {response}"))?;
    u32::try_from(value).map_err(|_| format!("{field} out of range: {value}"))
}
