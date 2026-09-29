use serde::{Deserialize, Serialize};

/// Stable capability ids for render features that sit above the backend route.
///
/// These are not renderer names. They are profile/capability contracts that a
/// backend or feature provider may implement, shadow, or decline explicitly.
pub mod render_feature_capability {
    pub const MATERIAL_SHADER_GRAPH: &str = "render.material_shader_graph";
    pub const SHADER_VARIANT_REGISTRY: &str = "render.shader_variant_registry";
    pub const LIGHTING_STACK: &str = "render.lighting_stack";
    pub const SHADOW_SYSTEM: &str = "render.shadow_system";
    pub const POSTFX_STACK: &str = "render.postfx_stack";
    pub const REFLECTION_PROBES: &str = "render.probes.reflection";
    pub const PARTICLES_VFX: &str = "render.vfx.particles";
    pub const HAIR_STRANDS: &str = "render.hair.strands";
    pub const HAIR_GPU_SIMULATION: &str = "render.hair.gpu_simulation";
    pub const HAIR_SKINNING: &str = "render.hair.skinning";
    pub const HAIR_COLLISION_CAPSULES: &str = "render.hair.collision.capsules";
    pub const HAIR_COLLISION_SDF: &str = "render.hair.collision.sdf";
    pub const HAIR_SHADOWS: &str = "render.hair.shadows";
    pub const HAIR_LOD: &str = "render.hair.lod";
    pub const TERRAIN_RENDERING: &str = "render.terrain";
    pub const FOLIAGE_RENDERING: &str = "render.foliage";
    pub const LOD_SYSTEM: &str = "render.lod";
    pub const OCCLUSION_CULLING: &str = "render.occlusion";
    pub const DEBUG_OVERLAYS: &str = "render.debug.overlays";
}

/// Engine-facing render feature gateway ids.
///
/// Consumers still call `engine.render` for frame submission. These child
/// gateways describe optional feature providers and diagnostics routes.
pub mod render_feature_gateway {
    pub const MATERIAL_SHADER_GRAPH: &str = "engine.render.material_shader_graph";
    pub const SHADER_VARIANTS: &str = "engine.render.shader_variants";
    pub const LIGHTING: &str = "engine.render.lighting";
    pub const SHADOWS: &str = "engine.render.shadows";
    pub const POSTFX: &str = "engine.render.postfx";
    pub const PROBES: &str = "engine.render.probes";
    pub const VFX: &str = "engine.render.vfx";
    pub const HAIR: &str = "engine.render.hair";
    pub const TERRAIN: &str = "engine.render.terrain";
    pub const FOLIAGE: &str = "engine.render.foliage";
    pub const LOD: &str = "engine.render.lod";
    pub const OCCLUSION: &str = "engine.render.occlusion";
    pub const DEBUG: &str = "engine.render.debug";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderFeatureSystemKind {
    MaterialShaderGraph,
    ShaderVariantRegistry,
    Lighting,
    Shadows,
    PostFx,
    Probes,
    ParticlesVfx,
    Hair,
    Terrain,
    Foliage,
    Lod,
    Occlusion,
    DebugOverlays,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderFeatureExecutionStage {
    ImportResolve,
    RenderPrep,
    FrameGraphBuild,
    GpuExecution,
    DebugExtract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderFeatureCapabilityDescriptor {
    pub feature: RenderFeatureSystemKind,
    pub capability_id: String,
    pub engine_gateway: String,
    pub owner_service: String,
    pub contract: String,
    pub quality_tier: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub execution_stages: Vec<RenderFeatureExecutionStage>,
    #[serde(default)]
    pub debug_overlays: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderFeatureCapabilityCatalog {
    pub schema: String,
    #[serde(default)]
    pub descriptors: Vec<RenderFeatureCapabilityDescriptor>,
}

impl RenderFeatureCapabilityCatalog {
    #[inline]
    pub fn capabilities(&self) -> impl Iterator<Item = &str> {
        self.descriptors.iter().map(|d| d.capability_id.as_str())
    }
}
