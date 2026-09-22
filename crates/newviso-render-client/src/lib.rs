use newviso_host as host;
use serde_json::{json, Value};

const RENDER_SERVICE: &str = "engine.render";
const RENDER_INVOKE: &str = "invoke_json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShaderStage {
    Vertex,
    Fragment,
}

impl ShaderStage {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Vertex => "Vertex",
            Self::Fragment => "Fragment",
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

#[derive(Clone, Debug)]
pub struct GraphicsPipelineDesc<'a> {
    pub label: &'a str,
    pub vertex_shader: u32,
    pub fragment_shader: u32,
    pub vertex_stride: u64,
    pub attributes: &'a [VertexAttribute],
    pub topology: &'a str,
    pub color_format: &'a str,
    pub depth_format: &'a str,
    pub cull_mode: &'a str,
    pub blend_mode: &'a str,
    pub cache_key: &'a str,
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
            self.command(json!({
                "CreateBuffer": {
                    "label": label,
                    "size": size,
                    "usage": usage,
                    "memory": memory
                }
            }))?,
            "BufferId",
        )
    }

    pub fn create_shader(
        &self,
        label: &str,
        stage: ShaderStage,
        logical_path: &str,
        variant_id: &str,
    ) -> Result<u32, String> {
        command_id(
            self.command(json!({
                "CreateShader": {
                    "label": label,
                    "stage": stage.wire_name(),
                    "entry": "main",
                    "spirv": [],
                    "asset": {
                        "logical_path": logical_path,
                        "source_kind": "Glsl",
                        "entry": "main",
                        "variant_id": variant_id,
                        "defines": [],
                        "optional": false
                    }
                }
            }))?,
            "ShaderId",
        )
    }

    pub fn create_pipeline(&self, desc: GraphicsPipelineDesc<'_>) -> Result<u32, String> {
        let attributes = desc
            .attributes
            .iter()
            .map(|attribute| {
                json!({
                    "location": attribute.location,
                    "offset": attribute.offset,
                    "format": attribute.format.wire_name()
                })
            })
            .collect::<Vec<_>>();

        command_id(
            self.command(json!({
                "CreatePipeline": {
                    "label": desc.label,
                    "vs": desc.vertex_shader,
                    "fs": desc.fragment_shader,
                    "topology": desc.topology,
                    "vertex_layouts": [{
                        "stride": desc.vertex_stride,
                        "attributes": attributes,
                        "step_mode": "Vertex"
                    }],
                    "bind_group_layouts": [],
                    "color_format": desc.color_format,
                    "depth_format": desc.depth_format,
                    "cull_mode": desc.cull_mode,
                    "blend_mode": desc.blend_mode,
                    "cache_key": desc.cache_key
                }
            }))?,
            "PipelineId",
        )
    }

    pub fn write_buffer_f32(&self, id: u32, offset: u64, values: &[f32]) -> Result<(), String> {
        let mut data = Vec::with_capacity(std::mem::size_of_val(values));
        for value in values {
            data.extend_from_slice(&value.to_le_bytes());
        }
        self.unit(json!({
            "WriteBuffer": {
                "id": id,
                "offset": offset,
                "data": data
            }
        }))
    }

    pub fn begin_frame(&self, clear_color: [f32; 4], frame_index: u64) -> Result<(), String> {
        self.unit(json!({
            "BeginFrame": {
                "clear_color": clear_color,
                "frame_index": frame_index
            }
        }))
    }

    pub fn set_viewport(&self, width: u32, height: u32) -> Result<(), String> {
        self.unit(json!({
            "SetViewport": {
                "x": 0.0,
                "y": 0.0,
                "w": width as f32,
                "h": height as f32,
                "min_depth": 0.0,
                "max_depth": 1.0
            }
        }))
    }

    pub fn set_scissor(&self, width: u32, height: u32) -> Result<(), String> {
        self.unit(json!({
            "SetScissor": {
                "x": 0,
                "y": 0,
                "w": width,
                "h": height
            }
        }))
    }

    pub fn set_pipeline(&self, pipeline: u32) -> Result<(), String> {
        self.unit(json!({ "SetPipeline": { "pipeline": pipeline } }))
    }

    pub fn set_vertex_buffer(&self, slot: u32, buffer: u32, offset: u64) -> Result<(), String> {
        self.unit(json!({
            "SetVertexBuffer": {
                "slot": slot,
                "slice": {
                    "buffer": buffer,
                    "offset": offset
                }
            }
        }))
    }

    pub fn draw(&self, vertex_count: u32) -> Result<(), String> {
        self.unit(json!({
            "Draw": {
                "vertex_count": vertex_count,
                "instance_count": 1,
                "first_vertex": 0,
                "first_instance": 0
            }
        }))
    }

    pub fn end_frame(&self) -> Result<(), String> {
        self.unit(json!("EndFrame"))
    }

    pub fn abort_frame(&self) {
        let _ = self.request(json!("AbortFrame"));
    }

    pub fn destroy_pipeline(&self, id: u32) {
        let _ = self.unit(json!({ "DestroyPipeline": id }));
    }

    pub fn destroy_shader(&self, id: u32) {
        let _ = self.unit(json!({ "DestroyShader": id }));
    }

    pub fn destroy_buffer(&self, id: u32) {
        let _ = self.unit(json!({ "DestroyBuffer": id }));
    }

    fn request(&self, request: Value) -> Result<Value, String> {
        let response = host::call_json(RENDER_SERVICE, RENDER_INVOKE, &request)?;
        if let Some(problem) = response.get("Problem") {
            return Err(format!("render service problem: {problem}"));
        }
        Ok(response)
    }

    fn command(&self, command: Value) -> Result<Value, String> {
        let response = self.request(json!({ "Command": command }))?;
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

fn command_id(response: Value, field: &str) -> Result<u32, String> {
    let value = response
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("render response has no {field}: {response}"))?;
    u32::try_from(value).map_err(|_| format!("{field} out of range: {value}"))
}
