use serde::{Deserialize, Serialize};

use super::{
    FrameCameraContext, RenderGraphPassDesc, RenderGraphResourceDesc, RendererParitySettings,
    VisibilitySettings,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderGraphDesc {
    pub label: Option<String>,
    #[serde(default)]
    pub frame_index: u64,
    #[serde(default)]
    pub camera: FrameCameraContext,
    #[serde(default)]
    pub visibility: VisibilitySettings,
    #[serde(default)]
    pub parity: RendererParitySettings,
    #[serde(default)]
    pub resources: Vec<RenderGraphResourceDesc>,
    #[serde(default)]
    pub passes: Vec<RenderGraphPassDesc>,
}

impl RenderGraphDesc {
    #[inline]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            frame_index: 0,
            camera: FrameCameraContext::default(),
            visibility: VisibilitySettings::default(),
            parity: RendererParitySettings::default(),
            resources: Vec::new(),
            passes: Vec::new(),
        }
    }

    #[inline]
    pub fn add_resource(mut self, resource: RenderGraphResourceDesc) -> Self {
        self.resources.push(resource);
        self
    }

    #[inline]
    pub fn add_pass(mut self, pass: RenderGraphPassDesc) -> Self {
        self.passes.push(pass);
        self
    }
}
