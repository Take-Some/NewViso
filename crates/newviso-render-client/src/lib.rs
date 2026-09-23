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

    pub fn write_buffer(&self, id: u32, offset: u64, data: &[u8]) -> Result<(), String> {
        self.unit(json!({"WriteBuffer":{"id":id,"offset":offset,"data":data}}))
    }

    pub fn write_buffer_f32(&self, id: u32, offset: u64, values: &[f32]) -> Result<(), String> {
        let mut data = Vec::with_capacity(std::mem::size_of_val(values));
        for value in values {
            data.extend_from_slice(&value.to_le_bytes());
        }
        self.write_buffer(id, offset, &data)
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
        let mip_data=mips.iter().map(|m|json!({
            "level":m.level,"width":m.width,"height":m.height,"offset":m.offset,"byte_len":m.byte_len
        })).collect::<Vec<_>>();
        command_id(
            self.command(json!({"CreateTexture":{
                "label":label,
                "extent":{"width":width,"height":height},
                "format":format,
                "usage":"Sampled",
                "mip_levels":mips.len().max(1) as u32,
                "data":data,
                "mip_data":mip_data,
                "data_policy":"Immediate"
            }}))?,
            "TextureId",
        )
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
        textures: [Option<u32>; 3],
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
                "texture3":null,"texture4":null,"texture5":null,
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
        let attributes = desc
            .attributes
            .iter()
            .map(|a| {
                json!({
                    "location":a.location,"offset":a.offset,"format":a.format.wire_name()
                })
            })
            .collect::<Vec<_>>();
        command_id(self.command(json!({"CreatePipeline":{
            "label":desc.label,"vs":desc.vertex_shader,"fs":desc.fragment_shader,
            "topology":desc.topology,
            "vertex_layouts":[{"stride":desc.vertex_stride,"attributes":attributes,"step_mode":"Vertex"}],
            "bind_group_layouts":desc.bind_group_layouts,
            "color_format":desc.color_format,"color_formats":[],
            "depth_format":desc.depth_format,
            "depth_mode":{"test":desc.depth_test,"write":desc.depth_write,"compare":desc.depth_compare},
            "cull_mode":desc.cull_mode,"blend_mode":desc.blend_mode,
            "depth_bias_constant":0.0,"depth_bias_slope":0.0,"depth_bias_clamp":0.0,
            "cache_key":desc.cache_key,
            "tessellation":{"mode":"Disabled","factor":4.0,"min_distance":8.0,"max_distance":96.0},
            "warmup":false
        }}))?,"PipelineId")
    }

    pub fn begin_frame(&self, clear_color: [f32; 4], frame_index: u64) -> Result<(), String> {
        self.unit(json!({"BeginFrame":{"clear_color":clear_color,"frame_index":frame_index}}))
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
        self.unit(json!({"Draw":{"vertex_count":vertex_count,"instance_count":1,"first_vertex":0,"first_instance":0}}))
    }
    pub fn draw_indexed(&self, index_count: u32) -> Result<(), String> {
        self.unit(json!({"DrawIndexed":{"index_count":index_count,"instance_count":1,"first_index":0,"vertex_offset":0,"first_instance":0}}))
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

fn command_id(response: Value, field: &str) -> Result<u32, String> {
    let value = response
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("render response has no {field}: {response}"))?;
    u32::try_from(value).map_err(|_| format!("{field} out of range: {value}"))
}
