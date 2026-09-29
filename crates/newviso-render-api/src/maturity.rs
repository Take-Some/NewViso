use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderDebugOverlayKind {
    FrameGraph,
    GBuffer,
    ShadowCascades,
    LightTiles,
    ReflectionProbes,
    Occlusion,
    LodRanges,
    TerrainCells,
    FoliageDensity,
    ParticleBounds,
    MaterialGraph,
    ShaderVariants,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderDebugOverlayRequestDto {
    pub schema: String,
    pub overlay: RenderDebugOverlayKind,
    #[serde(default)]
    pub target_view: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LightingStackDescriptorDto {
    pub schema: String,
    #[serde(default)]
    pub clustered_lighting: bool,
    #[serde(default)]
    pub tiled_lighting: bool,
    #[serde(default)]
    pub direct_lights: u32,
    #[serde(default)]
    pub probe_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShadowSystemDescriptorDto {
    pub schema: String,
    #[serde(default)]
    pub cascades: u32,
    #[serde(default)]
    pub atlas_size: u32,
    #[serde(default)]
    pub supports_pcss: bool,
    #[serde(default)]
    pub supports_contact_shadows: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeGridDescriptorDto {
    pub schema: String,
    pub grid_id: String,
    #[serde(default)]
    pub reflection_probe_count: u32,
    #[serde(default)]
    pub irradiance_probe_count: u32,
    #[serde(default)]
    pub update_policy: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxSystemDescriptorDto {
    pub schema: String,
    #[serde(default)]
    pub gpu_particles: bool,
    #[serde(default)]
    pub max_emitters: u32,
    #[serde(default)]
    pub max_particles: u32,
    #[serde(default)]
    pub max_transient_lights: u32,
    #[serde(default)]
    pub max_decals: u32,
    #[serde(default)]
    pub max_trails: u32,
    #[serde(default)]
    pub semantic_effect_library: bool,
    #[serde(default)]
    pub surface_impact_response: bool,
    #[serde(default)]
    pub staged_runtime: bool,
    #[serde(default)]
    pub simulation_stage: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainFoliageLodDescriptorDto {
    pub schema: String,
    #[serde(default)]
    pub terrain_cells: u32,
    #[serde(default)]
    pub foliage_batches: u32,
    #[serde(default)]
    pub lod_levels: u32,
    #[serde(default)]
    pub occlusion_policy: String,
}
