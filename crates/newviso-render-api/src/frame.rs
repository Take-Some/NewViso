use crate::Color4;
use crate::{RenderTargetId, TextureFormat};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderLightKind {
    Directional,
    Point,
    Spot,
    Area,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RenderLight {
    pub kind: RenderLightKind,
    pub position_ws: [f32; 3],
    pub direction_ws: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub spot_inner_cos: f32,
    pub spot_outer_cos: f32,
    pub casts_shadows: bool,
}

impl RenderLight {
    #[inline]
    pub const fn local(
        kind: RenderLightKind,
        position_ws: [f32; 3],
        direction_ws: [f32; 3],
        color: [f32; 3],
        intensity: f32,
        range: f32,
        spot_inner_cos: f32,
        spot_outer_cos: f32,
        casts_shadows: bool,
    ) -> Self {
        Self {
            kind,
            position_ws,
            direction_ws,
            color,
            intensity,
            range,
            spot_inner_cos,
            spot_outer_cos,
            casts_shadows,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeginFrameDesc {
    pub clear_color: Color4,
    /// Engine frame id supplied by the host/controller. Backends use this to
    /// publish precise frame-completion events for resource lifetime.
    #[serde(default)]
    pub frame_index: u64,
}

impl BeginFrameDesc {
    #[inline]
    pub const fn new(clear_color: Color4) -> Self {
        Self {
            clear_color,
            frame_index: 0,
        }
    }

    #[inline]
    pub const fn with_frame_index(mut self, frame_index: u64) -> Self {
        self.frame_index = frame_index;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderTargetDesc {
    pub extent: Extent2D,
    pub color: TextureFormat,
    pub depth: Option<TextureFormat>,
    pub label: Option<String>,
}

impl RenderTargetDesc {
    #[inline]
    pub fn new(extent: Extent2D, color: TextureFormat) -> Self {
        Self {
            extent,
            color,
            depth: None,
            label: None,
        }
    }

    #[inline]
    pub fn with_depth(mut self, depth: TextureFormat) -> Self {
        self.depth = Some(depth);
        self
    }

    #[inline]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeginRenderTargetDesc {
    pub target: RenderTargetId,
    pub clear_color: Option<Color4>,
    pub clear_depth: Option<f32>,
    pub clear_stencil: Option<u32>,
}

impl BeginRenderTargetDesc {
    #[inline]
    pub const fn new(target: RenderTargetId) -> Self {
        Self {
            target,
            clear_color: None,
            clear_depth: None,
            clear_stencil: None,
        }
    }

    #[inline]
    pub fn with_clear_color(mut self, color: Color4) -> Self {
        self.clear_color = Some(color);
        self
    }

    #[inline]
    pub fn with_clear_depth(mut self, depth: f32) -> Self {
        self.clear_depth = Some(depth);
        self
    }

    #[inline]
    pub fn with_clear_stencil(mut self, stencil: u32) -> Self {
        self.clear_stencil = Some(stencil);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extent2D {
    pub width: u32,
    pub height: u32,
}

impl Extent2D {
    #[inline]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    #[inline]
    pub const fn pixel_count(self) -> u64 {
        (self.width as u64) * (self.height as u64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

impl Viewport {
    #[inline]
    pub fn full(extent: Extent2D) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: extent.width as f32,
            h: extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RectI32 {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl RectI32 {
    #[inline]
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
}
