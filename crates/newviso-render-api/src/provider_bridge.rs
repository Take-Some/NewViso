#![forbid(unsafe_op_in_unsafe_fn)]

use crate::{BufferSlice, Extent2D, IndexFormat, RenderDrawListKind, RenderGraphPassKind};
use serde::{Deserialize, Serialize};

/// Engine-facing gateway for profile/plugin draw-list extraction providers.
/// Runtime asks this gateway for draw-list data; the active provider is selected
/// by descriptor/capability metadata, never by a provider service id embedded in
/// scene/render code.
pub const ENGINE_RENDER_DRAW_LISTS_SERVICE_ID: &str = "engine.render.draw_lists";
/// Engine-facing gateway for profile/plugin light extraction providers.
pub const ENGINE_RENDER_LIGHT_EXTRACTION_SERVICE_ID: &str = "engine.render.light_extraction";

pub const RENDER_DRAW_LIST_PROVIDER_SERVICE_KIND: &str = "render.draw_lists";
pub const RENDER_LIGHT_EXTRACTION_PROVIDER_SERVICE_KIND: &str = "render.light_extraction";

pub const RENDER_DRAW_LIST_PROVIDER_CAPABILITY: &str = "render.draw_list_provider";
pub const RENDER_LIGHT_EXTRACTION_PROVIDER_CAPABILITY: &str = "render.light_extraction_provider";

pub const RENDER_DRAW_LIST_PROVIDER_METHOD_EXTRACT: &str = "extract_draw_lists";
pub const RENDER_LIGHT_EXTRACTION_PROVIDER_METHOD_EXTRACT: &str = "extract_light_plan";

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RenderBoundsSnapshot {
    pub center: [f32; 3],
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RenderViewSnapshot {
    pub view_projection_cols: [[f32; 4]; 4],
    pub position_ws: [f32; 3],
}

impl Default for RenderViewSnapshot {
    #[inline]
    fn default() -> Self {
        Self {
            view_projection_cols: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            position_ws: [0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneExtractionSnapshot {
    pub frame_index: u64,
    pub viewport_extent: Extent2D,
    pub surface_extent: Extent2D,
    pub runtime: bool,
    pub debug_overlays: bool,
    pub bounds: RenderBoundsSnapshot,
    #[serde(default)]
    pub view: RenderViewSnapshot,
    #[serde(default)]
    pub active_draw_lists: Vec<RenderDrawListKind>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct VisibilityMask {
    pub shadow_casters: bool,
    pub local_shadow_casters: bool,
    pub opaque_forward: bool,
    #[serde(default)]
    pub particle_gbuffer: bool,
    pub transparent: bool,
    #[serde(default)]
    pub scene_2d: bool,
    pub debug: bool,
}

impl VisibilityMask {
    #[inline]
    pub const fn allows(self, kind: RenderDrawListKind) -> bool {
        match kind {
            RenderDrawListKind::ShadowCasters => self.shadow_casters,
            RenderDrawListKind::LocalShadowCasters => self.local_shadow_casters,
            RenderDrawListKind::OpaqueForward => self.opaque_forward,
            RenderDrawListKind::ParticleGBuffer => self.particle_gbuffer,
            RenderDrawListKind::Transparent => self.transparent,
            RenderDrawListKind::Scene2D => self.scene_2d,
            RenderDrawListKind::Debug => self.debug,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameGraphRoute {
    pub pass: RenderGraphPassKind,
    #[serde(default)]
    pub draw_lists: Vec<RenderDrawListKind>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrameGraphRoutes {
    #[serde(default)]
    pub routes: Vec<FrameGraphRoute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawListProviderExtractRequest {
    pub scene: SceneExtractionSnapshot,
    pub visibility: VisibilityMask,
    pub routes: FrameGraphRoutes,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DrawListContributionStats {
    pub draw_calls: u32,
    pub indexed_draw_calls: u32,
    pub triangle_count: u64,
    pub instance_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StableRenderHandle {
    pub namespace: String,
    pub key: String,
    #[serde(default)]
    pub generation: u32,
}

impl StableRenderHandle {
    #[inline]
    pub fn new(namespace: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            key: key.into(),
            generation: 0,
        }
    }

    #[inline]
    pub fn with_generation(mut self, generation: u32) -> Self {
        self.generation = generation;
        self
    }

    #[inline]
    pub fn stable_label(&self) -> String {
        if self.generation == 0 {
            format!("{}:{}", self.namespace, self.key)
        } else {
            format!("{}:{}#{}", self.namespace, self.key, self.generation)
        }
    }
}

pub type RenderMeshHandle = StableRenderHandle;
pub type RenderMaterialHandle = StableRenderHandle;
pub type RenderInstanceBufferHandle = StableRenderHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderPipelineClass {
    LitForward,
    ShadowDepth,
    TransparentForward,
    DebugLines,
}

impl Default for RenderPipelineClass {
    #[inline]
    fn default() -> Self {
        Self::LitForward
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderMaterialBinding {
    #[serde(default = "default_material_base_color")]
    pub base_color: [f32; 4],
    #[serde(default)]
    pub emissive_radiance: [f32; 3],
    #[serde(default = "default_uv_transform")]
    pub uv_transform: [f32; 4],
    #[serde(default = "default_material_params")]
    pub material_params: [f32; 4],
    #[serde(default)]
    pub base_color_texture: Option<String>,
    #[serde(default)]
    pub normal_texture: Option<String>,
    #[serde(default)]
    pub roughness_texture: Option<String>,
    #[serde(default)]
    pub double_sided: bool,
    #[serde(default = "default_true")]
    pub cast_shadows: bool,
    #[serde(default = "default_true")]
    pub receive_shadows: bool,
}

impl Default for RenderMaterialBinding {
    #[inline]
    fn default() -> Self {
        Self {
            base_color: default_material_base_color(),
            emissive_radiance: [0.0, 0.0, 0.0],
            uv_transform: default_uv_transform(),
            material_params: default_material_params(),
            base_color_texture: None,
            normal_texture: None,
            roughness_texture: None,
            double_sided: false,
            cast_shadows: true,
            receive_shadows: true,
        }
    }
}

#[inline]
fn default_true() -> bool {
    true
}

#[inline]
fn default_material_base_color() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}

#[inline]
fn default_uv_transform() -> [f32; 4] {
    [1.0, 1.0, 0.0, 0.0]
}

#[inline]
fn default_material_params() -> [f32; 4] {
    [0.5, 0.0, 1.0, 0.0]
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RenderMeshGpuBinding {
    pub vertex: BufferSlice,
    #[serde(default)]
    pub index: Option<BufferSlice>,
    #[serde(default = "default_index_format")]
    pub index_format: IndexFormat,
    pub vertex_count: u32,
    #[serde(default)]
    pub index_count: u32,
    #[serde(default)]
    pub first_vertex: u32,
    #[serde(default)]
    pub first_index: u32,
    #[serde(default)]
    pub vertex_offset: i32,
}

#[inline]
fn default_index_format() -> IndexFormat {
    IndexFormat::U32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderInstanceData {
    pub transform_cols: [[f32; 4]; 4],
    #[serde(default)]
    pub base_color_override: Option<[f32; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderInstanceBufferBinding {
    pub handle: RenderInstanceBufferHandle,
    pub buffer: BufferSlice,
    pub stride: u32,
    pub instance_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderInstanceSource {
    Inline(Vec<RenderInstanceData>),
    Buffer(RenderInstanceBufferBinding),
}

impl Default for RenderInstanceSource {
    #[inline]
    fn default() -> Self {
        Self::Inline(Vec::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DrawListContributionCommand {
    /// Executable geometry contribution. The mesh/material/instance handles are
    /// stable provider-owned identities; the GPU binding is a frame/session-resident
    /// binding that the host can lower into RenderApi draw calls immediately.
    GpuMesh {
        mesh: RenderMeshHandle,
        #[serde(default)]
        material: Option<RenderMaterialHandle>,
        #[serde(default)]
        material_binding: Box<RenderMaterialBinding>,
        gpu: RenderMeshGpuBinding,
        #[serde(default)]
        instances: Box<RenderInstanceSource>,
        #[serde(default)]
        pipeline: RenderPipelineClass,
    },
    DebugLineList {
        vertices: Vec<[f32; 3]>,
        color: [f32; 4],
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawListContribution {
    pub draw_list: RenderDrawListKind,
    pub label: String,
    #[serde(default)]
    pub stats: DrawListContributionStats,
    #[serde(default)]
    pub commands: Vec<DrawListContributionCommand>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DrawListProviderExtractResponse {
    #[serde(default)]
    pub contributions: Vec<DrawListContribution>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendShadowCapabilities {
    #[serde(default)]
    pub directional_depth_map: bool,
    #[serde(default)]
    pub cascaded_shadow_maps: bool,
    #[serde(default)]
    pub point_cube_map: bool,
    #[serde(default)]
    pub spot_depth_map: bool,
    #[serde(default)]
    pub max_shadow_resolution: u32,
    #[serde(default = "default_max_directional_cascades")]
    pub max_directional_cascades: u32,
    #[serde(default)]
    pub shadow_atlas: bool,
}

#[inline]
fn default_max_directional_cascades() -> u32 {
    1
}

impl Default for BackendShadowCapabilities {
    #[inline]
    fn default() -> Self {
        Self {
            directional_depth_map: true,
            cascaded_shadow_maps: false,
            point_cube_map: false,
            spot_depth_map: false,
            max_shadow_resolution: 2048,
            max_directional_cascades: 1,
            shadow_atlas: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightExtractionSnapshot {
    pub frame_index: u64,
    pub viewport_extent: Extent2D,
    pub surface_extent: Extent2D,
    pub bounds: RenderBoundsSnapshot,
    #[serde(default)]
    pub view: RenderViewSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowSettingsSnapshot {
    pub enabled: bool,
    pub method: String,
    #[serde(default = "default_shadow_filter")]
    pub filter: String,
    pub resolution: u32,
    pub max_distance: f32,
    pub bias: f32,
    pub softness: f32,
    pub contact_strength: f32,
    #[serde(default)]
    pub normal_bias: f32,
    #[serde(default = "default_cascade_count")]
    pub cascade_count: u32,
    #[serde(default)]
    pub pcss: ShadowPcssSettingsSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowPcssSettingsSnapshot {
    #[serde(default = "default_pcss_light_angular_radius_degrees")]
    pub light_angular_radius_degrees: f32,
    #[serde(default = "default_pcss_blocker_search_radius_texels")]
    pub blocker_search_radius_texels: f32,
    #[serde(default = "default_pcss_max_filter_radius_texels")]
    pub max_filter_radius_texels: f32,
    #[serde(default = "default_pcss_blocker_samples")]
    pub blocker_samples: u32,
    #[serde(default = "default_pcss_filter_samples")]
    pub filter_samples: u32,
    #[serde(default = "default_pcss_min_filter_radius_texels")]
    pub min_filter_radius_texels: f32,
    #[serde(default = "default_pcss_stable_kernel_cell_texels")]
    pub stable_kernel_cell_texels: f32,
}

impl Default for ShadowPcssSettingsSnapshot {
    fn default() -> Self {
        Self {
            light_angular_radius_degrees: default_pcss_light_angular_radius_degrees(),
            blocker_search_radius_texels: default_pcss_blocker_search_radius_texels(),
            max_filter_radius_texels: default_pcss_max_filter_radius_texels(),
            blocker_samples: default_pcss_blocker_samples(),
            filter_samples: default_pcss_filter_samples(),
            min_filter_radius_texels: default_pcss_min_filter_radius_texels(),
            stable_kernel_cell_texels: default_pcss_stable_kernel_cell_texels(),
        }
    }
}

fn default_shadow_filter() -> String {
    "pcf".to_owned()
}
fn default_pcss_light_angular_radius_degrees() -> f32 {
    0.266
}
fn default_pcss_blocker_search_radius_texels() -> f32 {
    3.0
}
fn default_pcss_max_filter_radius_texels() -> f32 {
    5.0
}
fn default_pcss_blocker_samples() -> u32 {
    10
}
fn default_pcss_filter_samples() -> u32 {
    12
}
fn default_pcss_min_filter_radius_texels() -> f32 {
    0.18
}
fn default_pcss_stable_kernel_cell_texels() -> f32 {
    8.0
}

#[inline]
fn default_cascade_count() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightExtractionProviderRequest {
    pub scene: LightExtractionSnapshot,
    pub settings: ShadowSettingsSnapshot,
    pub backend: BackendShadowCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightPlanContributionKind {
    Directional,
    Point,
    Spot,
    AmbientOcclusion,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightPlanContribution {
    pub handled: bool,
    pub kind: LightPlanContributionKind,
    pub supported: bool,
    pub resolution: u32,
    pub render_target: Option<u32>,
    pub shadow_texture: Option<u32>,
    pub light_mvp_cols: [[f32; 4]; 4],
    pub params: [f32; 4],
    #[serde(default)]
    pub extra: [f32; 4],
    #[serde(default)]
    pub pcss0: [f32; 4],
    #[serde(default)]
    pub pcss1: [f32; 4],
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl LightPlanContribution {
    #[inline]
    pub fn unhandled() -> Self {
        Self {
            handled: false,
            kind: LightPlanContributionKind::None,
            supported: false,
            resolution: 1,
            render_target: None,
            shadow_texture: None,
            light_mvp_cols: [[0.0; 4]; 4],
            params: [0.0; 4],
            extra: [0.0; 4],
            pcss0: [0.0; 4],
            pcss1: [0.0; 4],
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LightExtractionProviderResponse {
    pub contribution: Option<LightPlanContribution>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_capabilities_json_use_defaults_for_missing_fields() {
        let minimal = r#"{
            "directional_depth_map": true,
            "cascaded_shadow_maps": false,
            "point_cube_map": false,
            "spot_depth_map": false,
            "max_shadow_resolution": 1024
        }"#;
        let caps: BackendShadowCapabilities = serde_json::from_str(minimal).expect("caps json");
        assert!(caps.directional_depth_map);
        assert_eq!(caps.max_shadow_resolution, 1024);
        assert_eq!(caps.max_directional_cascades, 1);
        assert!(!caps.shadow_atlas);
    }

    #[test]
    fn light_plan_contribution_extra_defaults_for_old_providers() {
        let minimal = r#"{
            "handled": true,
            "kind": "Directional",
            "supported": true,
            "resolution": 1024,
            "render_target": 7,
            "shadow_texture": 9,
            "light_mvp_cols": [[1.0,0.0,0.0,0.0],[0.0,1.0,0.0,0.0],[0.0,0.0,1.0,0.0],[0.0,0.0,0.0,1.0]],
            "params": [1.0, 0.0015, 0.35, 0.4]
        }"#;
        let contribution: LightPlanContribution =
            serde_json::from_str(minimal).expect("minimal contribution json");
        assert_eq!(contribution.extra, [0.0; 4]);
    }
}
