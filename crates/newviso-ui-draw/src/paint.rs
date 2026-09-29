use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::UiTexId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(default)]
pub struct TextureRef {
    pub uri: String,
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(default)]
pub struct VectorRef {
    pub uri: String,
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum UiImageRef {
    Texture(TextureRef),
    Vector(VectorRef),
}

/// Ordering phase for renderer-aware UI presentation commands.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiPaintPhase {
    /// Behind the provider's semantic/content mesh (backgrounds, control skins).
    #[default]
    Underlay,
    /// Normal content phase for provider-neutral paint primitives.
    Content,
    /// Above semantic/content mesh (scanlines, highlights, decorative overlays).
    Overlay,
    /// Final surface-level effect hook. Backends may execute this as a separate pass.
    Post,
}

/// Sampling/layout policy for `.ytd@entry` UI textures.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiTextureMode {
    #[default]
    Stretch,
    Fit,
    Cover,
    Tile,
}

/// Typed material parameter override carried by a UI paint command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum UiMaterialParamValue {
    Float(f32),
    Float2([f32; 2]),
    Float3([f32; 3]),
    Float4([f32; 4]),
    Int(i32),
    Bool(bool),
    Color([f32; 4]),
    TextureRef(String),
}

impl Default for UiMaterialParamValue {
    fn default() -> Self {
        Self::Float(0.0)
    }
}

/// Renderer-neutral paint stream emitted by UI providers before backend-specific batching.
///
/// This is the contract boundary between retained UI systems such as Aurelia and
/// GPU backends such as the Vulkan UI renderer. It intentionally contains only
/// generic primitives and resource references; it must never contain product
/// concepts such as Logger, Profiler, AssetBrowser, or EditorRegistry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiPaintList {
    pub version: u32,
    pub commands: Vec<UiPaintCommand>,
    pub diagnostics: Vec<String>,
}

impl Default for UiPaintList {
    fn default() -> Self {
        Self {
            version: 1,
            commands: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

impl UiPaintList {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.commands.clear();
        self.diagnostics.clear();
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    #[inline]
    pub fn push(&mut self, command: UiPaintCommand) {
        self.commands.push(command);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum UiPaintCommand {
    Rect(UiRectPaintCommand),
    RoundedRect(UiRoundedRectPaintCommand),
    Border(UiBorderPaintCommand),
    Text(UiTextPaintCommand),
    Image(UiImagePaintCommand),
    MaterialQuad(UiMaterialQuadPaintCommand),
    SurfaceEffect(UiSurfaceEffectPaintCommand),
    Vector(UiVectorPaintCommand),
    Icon(UiIconPaintCommand),
    ClipBegin(UiClipPaintCommand),
    ClipEnd(UiScopePaintCommand),
    LayerBegin(UiLayerPaintCommand),
    LayerEnd(UiScopePaintCommand),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiPaintNodeRef {
    pub surface_id: String,
    pub node_id: String,
    pub component_id: String,
    pub role: String,
    pub state: String,
    pub state_tags: Vec<String>,
    pub z_index: i32,
}

impl Default for UiPaintNodeRef {
    fn default() -> Self {
        Self {
            surface_id: String::new(),
            node_id: String::new(),
            component_id: String::new(),
            role: String::new(),
            state: "normal".to_owned(),
            state_tags: Vec::new(),
            z_index: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiRectPaintCommand {
    pub node: UiPaintNodeRef,
    pub rect: [f32; 4],
    pub color: u32,
    pub clip_rect: Option<[f32; 4]>,
}

impl Default for UiRectPaintCommand {
    fn default() -> Self {
        Self {
            node: UiPaintNodeRef::default(),
            rect: [0.0, 0.0, 0.0, 0.0],
            color: 0,
            clip_rect: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiRoundedRectPaintCommand {
    pub node: UiPaintNodeRef,
    pub rect: [f32; 4],
    pub radius_px: f32,
    pub color: u32,
    pub clip_rect: Option<[f32; 4]>,
}

impl Default for UiRoundedRectPaintCommand {
    fn default() -> Self {
        Self {
            node: UiPaintNodeRef::default(),
            rect: [0.0, 0.0, 0.0, 0.0],
            radius_px: 0.0,
            color: 0,
            clip_rect: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiBorderPaintCommand {
    pub node: UiPaintNodeRef,
    pub rect: [f32; 4],
    pub radius_px: f32,
    pub width_px: f32,
    pub color: u32,
    pub clip_rect: Option<[f32; 4]>,
}

impl Default for UiBorderPaintCommand {
    fn default() -> Self {
        Self {
            node: UiPaintNodeRef::default(),
            rect: [0.0, 0.0, 0.0, 0.0],
            radius_px: 0.0,
            width_px: 1.0,
            color: 0,
            clip_rect: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiTextPaintCommand {
    pub node: UiPaintNodeRef,
    pub rect: [f32; 4],
    pub text: String,
    pub font_ref: String,
    pub font_px: f32,
    /// Additional authored tracking applied after the font advance, in pixels.
    /// Zero preserves the font's nominal spacing.
    pub letter_spacing_px: f32,
    pub color: u32,
    pub max_width_px: f32,
    pub clip_rect: Option<[f32; 4]>,
}

impl Default for UiTextPaintCommand {
    fn default() -> Self {
        Self {
            node: UiPaintNodeRef::default(),
            rect: [0.0, 0.0, 0.0, 0.0],
            text: String::new(),
            font_ref: String::new(),
            font_px: 14.0,
            letter_spacing_px: 0.0,
            color: 0,
            max_width_px: 0.0,
            clip_rect: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiImagePaintCommand {
    pub node: UiPaintNodeRef,
    pub rect: [f32; 4],
    pub texture_id: Option<UiTexId>,
    pub texture_ref: Option<String>,
    pub uv_rect: [f32; 4],
    pub tint_rgba: u32,
    /// Clockwise visual rotation around the image rect center, in radians.
    /// Zero keeps the legacy axis-aligned quad path.
    pub rotation_radians: f32,
    pub clip_rect: Option<[f32; 4]>,
}

impl Default for UiImagePaintCommand {
    fn default() -> Self {
        Self {
            node: UiPaintNodeRef::default(),
            rect: [0.0, 0.0, 0.0, 0.0],
            texture_id: None,
            texture_ref: None,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint_rgba: 0xffff_ffff,
            rotation_radians: 0.0,
            clip_rect: None,
        }
    }
}

/// Renderer-aware quad used for project-owned UI skins and material-backed panels.
/// `texture_ref` is always a canonical `.ytd@entry` reference when present.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiMaterialQuadPaintCommand {
    pub node: UiPaintNodeRef,
    pub phase: UiPaintPhase,
    pub rect: [f32; 4],
    pub texture_id: Option<UiTexId>,
    pub texture_ref: Option<String>,
    pub texture_mode: UiTextureMode,
    pub uv_rect: [f32; 4],
    pub tint_rgba: u32,
    /// Selected `.nemat@entry` material. Empty means standard textured/solid UI pipeline.
    pub material_ref: Option<String>,
    pub material_params: BTreeMap<String, UiMaterialParamValue>,
    pub clip_rect: Option<[f32; 4]>,
}

impl Default for UiMaterialQuadPaintCommand {
    fn default() -> Self {
        Self {
            node: UiPaintNodeRef::default(),
            phase: UiPaintPhase::Underlay,
            rect: [0.0, 0.0, 0.0, 0.0],
            texture_id: None,
            texture_ref: None,
            texture_mode: UiTextureMode::Stretch,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint_rgba: 0xffff_ffff,
            material_ref: None,
            material_params: BTreeMap::new(),
            clip_rect: None,
        }
    }
}

/// Surface-level renderer effect hook (background/overlay/post).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiSurfaceEffectPaintCommand {
    pub node: UiPaintNodeRef,
    pub phase: UiPaintPhase,
    pub rect: [f32; 4],
    pub material_ref: String,
    pub material_params: BTreeMap<String, UiMaterialParamValue>,
    pub clip_rect: Option<[f32; 4]>,
}

impl Default for UiSurfaceEffectPaintCommand {
    fn default() -> Self {
        Self {
            node: UiPaintNodeRef::default(),
            phase: UiPaintPhase::Overlay,
            rect: [0.0, 0.0, 0.0, 0.0],
            material_ref: String::new(),
            material_params: BTreeMap::new(),
            clip_rect: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiVectorPaintCommand {
    pub node: UiPaintNodeRef,
    pub rect: [f32; 4],
    pub vector: VectorRef,
    pub tint_rgba: u32,
    pub opacity: f32,
    /// True when the referenced SVG/vector should be sampled as an animated asset.
    /// Static renderers may safely ignore this and draw the rest pose.
    pub animated: bool,
    /// Provider-supplied animation clock for SVG SMIL/CSS/keyframe evaluation.
    pub animation_time_ms: f32,
    /// Normalized convenience progress for simple control transitions.
    pub animation_progress_01: f32,
    pub clip_rect: Option<[f32; 4]>,
}

impl Default for UiVectorPaintCommand {
    fn default() -> Self {
        Self {
            node: UiPaintNodeRef::default(),
            rect: [0.0, 0.0, 0.0, 0.0],
            vector: VectorRef::default(),
            tint_rgba: 0xffff_ffff,
            opacity: 1.0,
            animated: false,
            animation_time_ms: 0.0,
            animation_progress_01: 1.0,
            clip_rect: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiIconPaintCommand {
    pub node: UiPaintNodeRef,
    pub rect: [f32; 4],
    pub icon: String,
    pub texture_ref: Option<String>,
    pub tint_rgba: u32,
    pub clip_rect: Option<[f32; 4]>,
}

impl Default for UiIconPaintCommand {
    fn default() -> Self {
        Self {
            node: UiPaintNodeRef::default(),
            rect: [0.0, 0.0, 0.0, 0.0],
            icon: String::new(),
            texture_ref: None,
            tint_rgba: 0xffff_ffff,
            clip_rect: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiClipPaintCommand {
    pub node: UiPaintNodeRef,
    pub rect: [f32; 4],
}

impl Default for UiClipPaintCommand {
    fn default() -> Self {
        Self {
            node: UiPaintNodeRef::default(),
            rect: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiLayerPaintCommand {
    pub node: UiPaintNodeRef,
    pub name: String,
    pub opacity: f32,
    pub transform: [f32; 6],
}

impl Default for UiLayerPaintCommand {
    fn default() -> Self {
        Self {
            node: UiPaintNodeRef::default(),
            name: String::new(),
            opacity: 1.0,
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct UiScopePaintCommand {
    pub node: UiPaintNodeRef,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_list_serializes_generic_commands() {
        let mut list = UiPaintList::new();
        list.push(UiPaintCommand::RoundedRect(UiRoundedRectPaintCommand {
            node: UiPaintNodeRef {
                surface_id: "surface".to_owned(),
                node_id: "button.primary".to_owned(),
                component_id: "button".to_owned(),
                role: "button".to_owned(),
                state: "hovered".to_owned(),
                state_tags: vec!["hover".to_owned(), "hovered".to_owned()],
                z_index: 1,
            },
            rect: [10.0, 20.0, 100.0, 28.0],
            radius_px: 6.0,
            color: 0xff00_0000,
            clip_rect: Some([0.0, 0.0, 1280.0, 720.0]),
        }));

        let bytes = serde_json::to_vec(&list).unwrap();
        let decoded: UiPaintList = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.commands.len(), 1);
        match &decoded.commands[0] {
            UiPaintCommand::RoundedRect(command) => {
                assert_eq!(command.node.component_id, "button");
                assert_eq!(command.node.state, "hovered");
            }
            other => panic!("expected rounded_rect command, got {other:?}"),
        }
    }

    #[test]
    fn paint_list_serializes_vector_refs_without_svg_xml_payload() {
        let mut list = UiPaintList::new();
        list.push(UiPaintCommand::Vector(UiVectorPaintCommand {
            node: UiPaintNodeRef {
                surface_id: "surface".to_owned(),
                node_id: "input.checkbox".to_owned(),
                component_id: "checkbox".to_owned(),
                state: "checked".to_owned(),
                state_tags: vec!["checked".to_owned(), "active".to_owned()],
                ..Default::default()
            },
            rect: [12.0, 24.0, 16.0, 16.0],
            vector: VectorRef {
                uri: "engine.ui.icons/control.checkbox_check".to_owned(),
                variant: None,
            },
            tint_rgba: 0xffff_ffff,
            opacity: 1.0,
            animated: true,
            animation_time_ms: 120.0,
            animation_progress_01: 0.75,
            clip_rect: None,
        }));

        let json = serde_json::to_string(&list).unwrap();
        assert!(json.contains("engine.ui.icons/control.checkbox_check"));
        assert!(json.contains("animation_time_ms"));
        assert!(!json.contains("<svg"));
        let decoded: UiPaintList = serde_json::from_str(&json).unwrap();
        match decoded.commands.first() {
            Some(UiPaintCommand::Vector(vector)) => {
                assert!(vector.animated);
                assert_eq!(vector.animation_progress_01, 0.75);
            }
            other => panic!("expected vector command, got {other:?}"),
        }
    }
}
