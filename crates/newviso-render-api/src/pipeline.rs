use crate::{BindGroupLayoutId, ShaderId, TextureFormat};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimitiveTopology {
    TriangleList,
    TriangleStrip,
    LineList,
    LineStrip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RasterCullMode {
    None,
    Front,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineBlendMode {
    Opaque,
    Alpha,
    PremultipliedAlpha,
    /// Additive accumulation used by weighted particle GBuffer MRTs.
    Additive,
    Multiply,
    Screen,
}

impl Default for PipelineBlendMode {
    #[inline]
    fn default() -> Self {
        Self::Opaque
    }
}

impl Default for RasterCullMode {
    #[inline]
    fn default() -> Self {
        Self::Back
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineDepthCompare {
    Always,
    LessOrEqual,
}

impl Default for PipelineDepthCompare {
    #[inline]
    fn default() -> Self {
        Self::LessOrEqual
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineDepthMode {
    pub test: bool,
    pub write: bool,
    pub compare: PipelineDepthCompare,
}

impl PipelineDepthMode {
    #[inline]
    pub const fn new(test: bool, write: bool, compare: PipelineDepthCompare) -> Self {
        Self {
            test,
            write,
            compare,
        }
    }

    #[inline]
    pub const fn read_write_less_equal() -> Self {
        Self::new(true, true, PipelineDepthCompare::LessOrEqual)
    }

    #[inline]
    pub const fn no_write_always() -> Self {
        Self::new(false, false, PipelineDepthCompare::Always)
    }
}

impl Default for PipelineDepthMode {
    #[inline]
    fn default() -> Self {
        Self::read_write_less_equal()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexFormat {
    U16,
    U32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VertexFormat {
    Float32x2,
    Float32x3,
    Float32x4,
    /// Four unsigned 16-bit integer components. Used for skeletal joint indices.
    Uint16x4,
    Unorm8x4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VertexStepMode {
    Vertex,
    Instance,
}

#[inline]
fn default_vertex_step_mode() -> VertexStepMode {
    VertexStepMode::Vertex
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VertexAttribute {
    pub location: u32,
    pub offset: u32,
    pub format: VertexFormat,
}

impl VertexAttribute {
    #[inline]
    pub const fn new(location: u32, offset: u32, format: VertexFormat) -> Self {
        Self {
            location,
            offset,
            format,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VertexLayout {
    pub stride: u32,
    pub attributes: Vec<VertexAttribute>,
    #[serde(default = "default_vertex_step_mode")]
    pub step_mode: VertexStepMode,
}

impl VertexLayout {
    #[inline]
    pub fn new(stride: u32, attributes: Vec<VertexAttribute>) -> Self {
        Self {
            stride,
            attributes,
            step_mode: VertexStepMode::Vertex,
        }
    }

    #[inline]
    pub fn per_instance(mut self) -> Self {
        self.step_mode = VertexStepMode::Instance;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TessellationMode {
    Disabled,
    Fixed,
    DistanceAdaptive,
}

impl Default for TessellationMode {
    #[inline]
    fn default() -> Self {
        Self::Disabled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TessellationDesc {
    #[serde(default)]
    pub mode: TessellationMode,
    #[serde(default = "default_tess_factor")]
    pub factor: f32,
    #[serde(default = "default_tess_min_distance")]
    pub min_distance: f32,
    #[serde(default = "default_tess_max_distance")]
    pub max_distance: f32,
}

impl Default for TessellationDesc {
    #[inline]
    fn default() -> Self {
        Self {
            mode: TessellationMode::Disabled,
            factor: default_tess_factor(),
            min_distance: default_tess_min_distance(),
            max_distance: default_tess_max_distance(),
        }
    }
}

#[inline]
fn default_tess_factor() -> f32 {
    4.0
}
#[inline]
fn default_tess_min_distance() -> f32 {
    8.0
}
#[inline]
fn default_tess_max_distance() -> f32 {
    96.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDesc {
    pub label: Option<String>,
    pub vs: ShaderId,
    pub fs: ShaderId,
    pub topology: PrimitiveTopology,
    pub vertex_layouts: Vec<VertexLayout>,
    pub bind_group_layouts: Vec<BindGroupLayoutId>,
    pub color_format: TextureFormat,
    #[serde(default)]
    pub color_formats: Vec<TextureFormat>,
    pub depth_format: Option<TextureFormat>,
    #[serde(default)]
    pub depth_mode: PipelineDepthMode,
    #[serde(default)]
    pub cull_mode: RasterCullMode,
    #[serde(default)]
    pub blend_mode: PipelineBlendMode,
    #[serde(default)]
    pub depth_bias_constant: f32,
    #[serde(default)]
    pub depth_bias_slope: f32,
    #[serde(default)]
    pub depth_bias_clamp: f32,
    #[serde(default)]
    pub cache_key: Option<String>,
    #[serde(default)]
    pub tessellation: TessellationDesc,
    #[serde(default)]
    pub warmup: bool,
}

impl PipelineDesc {
    #[inline]
    pub fn new(vs: ShaderId, fs: ShaderId, color_format: TextureFormat) -> Self {
        Self {
            label: None,
            vs,
            fs,
            topology: PrimitiveTopology::TriangleList,
            vertex_layouts: Vec::new(),
            bind_group_layouts: Vec::new(),
            color_format,
            color_formats: Vec::new(),
            depth_format: None,
            depth_mode: PipelineDepthMode::default(),
            cull_mode: RasterCullMode::Back,
            blend_mode: PipelineBlendMode::Opaque,
            depth_bias_constant: 0.0,
            depth_bias_slope: 0.0,
            depth_bias_clamp: 0.0,
            cache_key: None,
            tessellation: TessellationDesc::default(),
            warmup: false,
        }
    }

    #[inline]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[inline]
    pub fn with_topology(mut self, topology: PrimitiveTopology) -> Self {
        self.topology = topology;
        self
    }

    #[inline]
    pub fn with_vertex_layouts(mut self, layouts: Vec<VertexLayout>) -> Self {
        self.vertex_layouts = layouts;
        self
    }

    #[inline]
    pub fn with_bind_group_layouts(mut self, layouts: Vec<BindGroupLayoutId>) -> Self {
        self.bind_group_layouts = layouts;
        self
    }

    #[inline]
    pub fn push_bind_group_layout(mut self, layout: BindGroupLayoutId) -> Self {
        self.bind_group_layouts.push(layout);
        self
    }

    #[inline]
    pub fn with_color_formats(mut self, formats: Vec<TextureFormat>) -> Self {
        self.color_formats = formats;
        self
    }

    #[inline]
    pub fn mrt_color_formats(&self) -> &[TextureFormat] {
        if self.color_formats.is_empty() {
            std::slice::from_ref(&self.color_format)
        } else {
            &self.color_formats
        }
    }

    #[inline]
    pub fn with_depth(mut self, depth_format: TextureFormat) -> Self {
        self.depth_format = Some(depth_format);
        self.depth_mode = PipelineDepthMode::read_write_less_equal();
        self
    }

    #[inline]
    pub fn with_depth_state(
        mut self,
        depth_format: TextureFormat,
        depth_mode: PipelineDepthMode,
    ) -> Self {
        self.depth_format = Some(depth_format);
        self.depth_mode = depth_mode;
        self
    }

    #[inline]
    pub fn with_cull_mode(mut self, cull_mode: RasterCullMode) -> Self {
        self.cull_mode = cull_mode;
        self
    }

    #[inline]
    pub fn with_blend_mode(mut self, blend_mode: PipelineBlendMode) -> Self {
        self.blend_mode = blend_mode;
        self
    }

    #[inline]
    pub fn with_depth_bias(mut self, constant: f32, slope: f32, clamp: f32) -> Self {
        self.depth_bias_constant = constant;
        self.depth_bias_slope = slope;
        self.depth_bias_clamp = clamp;
        self
    }

    #[inline]
    pub fn with_tessellation(mut self, tessellation: TessellationDesc) -> Self {
        self.tessellation = tessellation;
        self
    }

    #[inline]
    pub fn with_cache_key(mut self, key: impl Into<String>) -> Self {
        let key = key.into();
        self.cache_key = if key.trim().is_empty() {
            None
        } else {
            Some(key)
        };
        self
    }

    #[inline]
    pub fn as_warmup(mut self) -> Self {
        self.warmup = true;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputePipelineDesc {
    pub label: Option<String>,
    pub cs: ShaderId,
    pub bind_group_layouts: Vec<BindGroupLayoutId>,
    #[serde(default)]
    pub cache_key: Option<String>,
    #[serde(default)]
    pub warmup: bool,
}

impl ComputePipelineDesc {
    #[inline]
    pub fn new(cs: ShaderId) -> Self {
        Self {
            label: None,
            cs,
            bind_group_layouts: Vec::new(),
            cache_key: None,
            warmup: false,
        }
    }

    #[inline]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[inline]
    pub fn with_bind_group_layouts(mut self, layouts: Vec<BindGroupLayoutId>) -> Self {
        self.bind_group_layouts = layouts;
        self
    }

    #[inline]
    pub fn with_cache_key(mut self, key: impl Into<String>) -> Self {
        let key = key.into();
        self.cache_key = (!key.trim().is_empty()).then_some(key);
        self
    }

    #[inline]
    pub fn as_warmup(mut self) -> Self {
        self.warmup = true;
        self
    }
}
