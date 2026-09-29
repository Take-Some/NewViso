use crate::{
    camera::{previous_hiz_camera_compatible, Camera, OrbitCamera},
    math::{transform_point, Vec3},
    world::{
        LightComponent, LightType, SceneBounds, SceneDestructible, SceneEntity, SceneEntityId,
        SceneEntityKind, SceneFocusSource, SceneFramePlan, SceneLifecycle, SceneLodPolicy,
        SceneMobility, SceneMutationSource, SceneProcessClaims, SceneProcessReasons,
        SceneResidency, SceneTransform, SceneView, SceneWorld, VisibilityMask,
    },
};
use newviso_host as host_runtime;
use newviso_input_client::InputSnapshot;
use newviso_render_client::{
    Extent2D, GraphicsPipelineDesc, RenderClient, RenderDrawListKind, RenderGraphDesc,
    RenderGraphPassDesc, RenderGraphPassDomain, RenderGraphPassId, RenderGraphPassKind,
    RenderGraphResourceDesc, RenderGraphResourceId, RenderGraphResourceSemantic,
    RenderGraphResourceUsage, RenderLight, RenderLightKind, ShaderStage,
    TextureFormat as RenderTextureFormat, TextureMipUpload, TextureResidencyState, VertexAttribute,
    VertexFormat, VertexLayoutDesc, VertexStepMode,
};
use newviso_textures::{TextureFormat, TextureResource};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[path = "geometry.rs"]
mod geometry;

mod animation_skinning;
mod asset_models;
mod atmospheric_clouds;
#[cfg(test)]
mod debris_stress;
mod gpu_instance_table;
mod mass_instances;
mod mesh_visibility;
mod parse_helpers;
mod portal_visibility;
mod render_math;
mod render_policy;
mod renderer_frame;
mod renderer_geometry;
mod renderer_init;
mod renderer_uniforms;
mod scene_entities;
mod scene_lighting;
mod scene_load;
mod scene_state;
mod volumetric_clouds;
mod weather_gpu_fx;
pub use asset_models::{
    prepare_model_geometry, SceneModelPartPose, ScenePreparedModelGeometry, SceneResolvedMaterial,
};
use asset_models::{AssetDrawRange, AssetTriangleVertex, CpuAssetMaterial, CpuAssetMesh};
use atmospheric_clouds::*;
pub use atmospheric_clouds::{
    AtmosphericCloudAnimMode, AtmosphericCloudLayerDesc, AtmosphericCloudLayerResources,
    AtmosphericCloudMeshResources, AtmosphericCloudResources, AtmosphericCloudTextureSet,
    AtmosphericCloudUvLayerDesc, AtmosphericCloudVertex,
};
pub use mass_instances::SceneMassInstanceDesc;
use portal_visibility::PortalVisibilityGraph;
use render_policy::RenderPolicy;
pub use volumetric_clouds::VolumetricCloudDesc;
use volumetric_clouds::*;
use weather_gpu_fx::*;
pub use weather_gpu_fx::{
    WeatherGpuFxEmitterDesc, WeatherGpuFxLayerDesc, WeatherGpuFxRenderDesc, WeatherGpuFxResources,
    WeatherGpuFxSystemType,
};
mod sky_gpu;

use parse_helpers::*;
use render_math::*;
use sky_gpu::*;

const SCENE_SERVICE: &str = "engine.scene";
const ASSET_SERVICE: &str = "engine.assets";
const ECS_SERVICE: &str = "engine.ecs";

const VERTEX_SHADER: &[u8] = include_bytes!("assets/scene.vert.spv");
const ASSET_INSTANCED_VERTEX_SHADER: &[u8] = include_bytes!("assets/scene_instanced.vert.spv");
const FRAGMENT_SHADER: &[u8] = include_bytes!("assets/scene.frag.spv");
const GBUFFER_FRAGMENT_SHADER: &[u8] = include_bytes!("assets/scene_gbuffer.frag.spv");
const SKY_VERTEX_SHADER: &[u8] = include_bytes!("assets/sky.vert.spv");
const SKY_FRAGMENT_SHADER: &[u8] = include_bytes!("assets/sky.frag.spv");
const SHADOW_VERTEX_SHADER: &[u8] = include_bytes!("assets/shadow.vert.spv");
const ASSET_INSTANCED_SHADOW_VERTEX_SHADER: &[u8] =
    include_bytes!("assets/shadow_instanced.vert.spv");
const SHADOW_FRAGMENT_SHADER: &[u8] = include_bytes!("assets/shadow.frag.spv");
const FLARE_VERTEX_SHADER: &[u8] = include_bytes!("assets/flare.vert.spv");
const FLARE_FRAGMENT_SHADER: &[u8] = include_bytes!("assets/flare.frag.spv");
const SKY_FLOATS_PER_VERTEX: usize = 5;
const SKY_VERTEX_STRIDE: u64 = (SKY_FLOATS_PER_VERTEX * std::mem::size_of::<f32>()) as u64;
const SKY_UNIFORM_FLOATS: usize = 188;
const MAX_SKY_VISUALS: usize = 4;
const FLARE_FLOATS_PER_VERTEX: usize = 12;
const FLARE_VERTEX_STRIDE: u64 = (FLARE_FLOATS_PER_VERTEX * std::mem::size_of::<f32>()) as u64;
const CUBE_VERTEX_COUNT: u32 = 36;
const FLOATS_PER_VERTEX: usize = 17;
const MAX_LIGHTS: usize = 16;
const SCENE_FRAME_UNIFORM_FLOATS: usize = 68 + MAX_LIGHTS * 20;
const DEFAULT_SHADOW_RESOLUTION: u32 = 2048;
const VERTEX_STRIDE: u64 = (FLOATS_PER_VERTEX * std::mem::size_of::<f32>()) as u64;
const INSTANCE_FLOATS: usize = 16;
const INSTANCE_STRIDE: u64 = (INSTANCE_FLOATS * std::mem::size_of::<f32>()) as u64;
const DEFAULT_ASSET_INSTANCE_CAPACITY: u32 = 16_384;
const DEFAULT_ASSET_VERTEX_CAPACITY: u32 = 2_097_152;
const DEFAULT_SKINNED_VERTEX_CAPACITY: u32 = 131_072;
const ASSET_INSTANCE_CELL_SIZE: f32 = 16.0;
const ENABLE_ASSET_HIZ_OCCLUSION: bool = true;
const MAX_HIZ_DRAW_CANDIDATES: u32 = 4_096;
const MAX_STREAMED_TEXTURE_UPLOADS_PER_FRAME: usize = 1;
const MAX_STREAMED_MATERIAL_CREATIONS_PER_FRAME: usize = 8;
const MAX_STREAMED_TEXTURE_DIMENSION: u32 = 2_048;
const HIZ_CANDIDATE_STRIDE: u64 = 32;
const HIZ_INDIRECT_STRIDE: u64 = 20;
const SCENE_FRAME_SLOTS: usize = 3;

#[derive(Clone, Debug)]
pub(super) struct Cube {
    position: Vec3,
    rotation_degrees: Vec3,
    scale: Vec3,
    base_color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct SceneTransientSphere {
    pub position: [f32; 3],
    pub rotation_degrees: [f32; 3],
    pub radius: f32,
    pub color: [f32; 4],
    pub marker_color: Option<[f32; 4]>,
    pub marker_direction: [f32; 3],
    pub marker_threshold: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SceneOverlayQuad {
    pub rect: [f32; 4],
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneParticleBlend {
    Alpha,
    Additive,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneParticleSpawnDesc {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub acceleration: [f32; 3],
    pub size: [f32; 2],
    pub end_size: [f32; 2],
    pub color: [f32; 4],
    pub end_color: [f32; 4],
    pub lifetime_seconds: f32,
    pub rotation_degrees: f32,
    pub angular_velocity_degrees: f32,
    pub blend: SceneParticleBlend,
}

#[derive(Clone, Debug)]
struct SceneRuntimeParticle {
    desc: SceneParticleSpawnDesc,
    age_seconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SceneRuntimeVisualKind {
    None,
    Cube,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneRuntimeEntityDesc {
    pub visual: SceneRuntimeVisualKind,
    pub asset_ref: Option<String>,
    /// Optional texture dictionary used to resolve texture bindings that carry
    /// a semantic texture name but no fully-qualified AssetRef.
    pub texture_dictionary: Option<String>,
    pub position: [f32; 3],
    pub rotation_degrees: [f32; 3],
    pub scale: [f32; 3],
    pub bounds_half_extent: [f32; 3],
    pub base_color: [f32; 4],
    pub solid: bool,
    pub visible_distance: f32,
    pub stream_distance: f32,
    pub fade_range: f32,
}

impl Default for SceneRuntimeEntityDesc {
    fn default() -> Self {
        Self {
            visual: SceneRuntimeVisualKind::None,
            asset_ref: None,
            texture_dictionary: None,
            position: [0.0; 3],
            rotation_degrees: [0.0; 3],
            scale: [1.0; 3],
            bounds_half_extent: [0.5; 3],
            base_color: [1.0; 4],
            solid: false,
            visible_distance: f32::INFINITY,
            stream_distance: f32::INFINITY,
            fade_range: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneLightType {
    Directional,
    Point,
    Spot,
    Area,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneLightDesc {
    pub light_type: SceneLightType,
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub cone_inner_degrees: f32,
    pub cone_outer_degrees: f32,
    pub casts_shadows: bool,
    pub shadow_bias: f32,
    pub shadow_normal_bias: f32,
    pub shadow_resolution: u32,
    pub shadow_distance: f32,
}

impl Default for SceneLightDesc {
    fn default() -> Self {
        Self {
            light_type: SceneLightType::Point,
            color: [1.0, 1.0, 1.0],
            intensity: 0.0,
            range: 0.0,
            cone_inner_degrees: 25.0,
            cone_outer_degrees: 35.0,
            casts_shadows: false,
            shadow_bias: 0.0015,
            shadow_normal_bias: 0.02,
            shadow_resolution: DEFAULT_SHADOW_RESOLUTION,
            shadow_distance: 96.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkyVisualKind {
    Disc,
    Billboard,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyVisualDesc {
    pub kind: SkyVisualKind,
    pub color: [f32; 3],
    pub intensity: f32,
    pub angular_size_degrees: f32,
    pub halo_size_degrees: f32,
    pub halo_intensity: f32,
    pub atmosphere_driver: bool,
}

impl Default for SkyVisualDesc {
    fn default() -> Self {
        Self {
            kind: SkyVisualKind::Disc,
            color: [1.0, 1.0, 1.0],
            intensity: 0.0,
            angular_size_degrees: 1.0,
            halo_size_degrees: 1.0,
            halo_intensity: 0.0,
            atmosphere_driver: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LensFlareElementKind {
    Halo,
    Ghost,
    Streak,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensFlareElementDesc {
    pub kind: LensFlareElementKind,
    pub offset: f32,
    pub size: f32,
    pub color: [f32; 3],
    pub alpha: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LensFlareDesc {
    pub source: String,
    pub enabled: bool,
    pub intensity: f32,
    pub scale: f32,
    pub occlusion_test: bool,
    pub elements: Vec<LensFlareElementDesc>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyCloudDesc {
    pub enabled: bool,
    pub coverage: f32,
    pub density: f32,
    pub softness: f32,
    pub scale: f32,
    pub detail_scale: f32,
    pub speed: [f32; 2],
    pub horizon_fade: f32,
    pub macro_scale: f32,
    pub macro_strength: f32,
    pub detail_strength: f32,
    pub micro_strength: f32,
    pub erosion_strength: f32,
    pub warp_strength: f32,
    pub shape_contrast: f32,
    pub shear_speed: [f32; 2],
    pub seed_offset: [f32; 2],
    /// Large-cloud base phase speed. Mirrors the dedicated large cloud speed
    /// used by GTA's procedural sky rather than reusing detail-layer motion.
    pub large_speed: f32,
    /// Independent small/filler cloud phase speed.
    pub small_speed: f32,
    /// Independent overall-detail phase speed.
    pub overall_detail_speed: f32,
    /// Independent edge-detail phase speed.
    pub edge_detail_speed: f32,
    /// Scalar applied twice to the integrated wind-driven base noise phase.
    /// GTA's sky uses 0.01, yielding speed * 0.0001 UV units per second.
    pub noise_phase_scale: f32,
}

impl Default for SkyCloudDesc {
    fn default() -> Self {
        Self {
            enabled: false,
            coverage: 0.0,
            density: 0.0,
            softness: 0.2,
            scale: 1.0,
            detail_scale: 1.0,
            speed: [0.0, 0.0],
            horizon_fade: 0.0,
            macro_scale: 0.35,
            macro_strength: 0.55,
            detail_strength: 0.28,
            micro_strength: 0.12,
            erosion_strength: 0.72,
            warp_strength: 0.10,
            shape_contrast: 1.0,
            shear_speed: [0.0, 0.0],
            seed_offset: [0.0, 0.0],
            large_speed: 5.0,
            small_speed: 1.0,
            overall_detail_speed: 1.0,
            edge_detail_speed: 1.0,
            noise_phase_scale: 0.01,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyAtmosphereDesc {
    pub twilight_altitudes: [f32; 4],
    pub daylight_altitudes: [f32; 2],
    pub horizon_power: f32,
    pub tonemap_shoulder: f32,
    pub night_zenith: [f32; 3],
    pub night_horizon: [f32; 3],
    pub astronomical_zenith: [f32; 3],
    pub astronomical_horizon: [f32; 3],
    pub nautical_zenith: [f32; 3],
    pub nautical_horizon: [f32; 3],
    pub civil_zenith: [f32; 3],
    pub civil_horizon: [f32; 3],
    pub day_zenith: [f32; 3],
    pub day_horizon: [f32; 3],
    pub sunset_tint: [f32; 3],
    pub sunset_strength: f32,
    pub cloud_night: [f32; 3],
    pub cloud_twilight_shadow: [f32; 3],
    pub cloud_twilight_light: [f32; 3],
    pub cloud_day_shadow: [f32; 3],
    pub cloud_day_light: [f32; 3],
    pub star_tint: [f32; 3],
    pub star_intensity: f32,
    pub star_visibility_altitudes: [f32; 2],
    pub cloud_occlusion: f32,
    pub silver_lining_tint: [f32; 3],
    pub silver_lining_strength: f32,
    pub cloud_alpha_range: [f32; 2],
}

impl Default for SkyAtmosphereDesc {
    fn default() -> Self {
        Self {
            twilight_altitudes: [-18.0, -12.0, -6.0, 4.0],
            daylight_altitudes: [-2.0, 12.0],
            horizon_power: 2.0,
            tonemap_shoulder: 0.0,
            night_zenith: [0.0; 3],
            night_horizon: [0.0; 3],
            astronomical_zenith: [0.0; 3],
            astronomical_horizon: [0.0; 3],
            nautical_zenith: [0.0; 3],
            nautical_horizon: [0.0; 3],
            civil_zenith: [0.0; 3],
            civil_horizon: [0.0; 3],
            day_zenith: [0.0; 3],
            day_horizon: [0.0; 3],
            sunset_tint: [0.0; 3],
            sunset_strength: 0.0,
            cloud_night: [0.0; 3],
            cloud_twilight_shadow: [0.0; 3],
            cloud_twilight_light: [0.0; 3],
            cloud_day_shadow: [0.0; 3],
            cloud_day_light: [0.0; 3],
            star_tint: [0.0; 3],
            star_intensity: 0.0,
            star_visibility_altitudes: [-16.0, -2.0],
            cloud_occlusion: 1.0,
            silver_lining_tint: [0.0; 3],
            silver_lining_strength: 0.0,
            cloud_alpha_range: [0.0, 0.0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneEnvironmentDesc {
    pub ambient_color: [f32; 3],
    pub ambient_intensity: f32,
    pub fog_enabled: bool,
    pub fog_color: [f32; 3],
    pub fog_density: f32,
    pub fog_start_distance: f32,
    pub fog_height_falloff: f32,
    pub fog_base_height: f32,
    pub fog_max_opacity: f32,
    pub haze_color: [f32; 3],
    pub haze_density: f32,
    pub haze_start_distance: f32,
}

impl Default for SceneEnvironmentDesc {
    fn default() -> Self {
        Self {
            ambient_color: [1.0, 1.0, 1.0],
            ambient_intensity: 0.0,
            fog_enabled: false,
            fog_color: [0.0, 0.0, 0.0],
            fog_density: 0.0,
            fog_start_distance: 0.0,
            fog_height_falloff: 0.0,
            fog_base_height: 0.0,
            fog_max_opacity: 0.0,
            haze_color: [0.0, 0.0, 0.0],
            haze_density: 0.0,
            haze_start_distance: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct GpuAssetMaterial {
    uniform_buffer: u32,
    bind_group: u32,
}

type GpuInstanceBatchKey = (u64, i32, i32, i32, u8);

#[derive(Clone, Debug)]
struct GpuResidentInstanceBatch {
    model_id: u64,
    first_instance: u32,
    instance_count: u32,
    stable_ids: Vec<u64>,
    sphere: [f32; 4],
}

#[derive(Clone, Debug, Default)]
struct GpuInstanceTable {
    source_render_epoch: u64,
    source_asset_epoch: u64,
    instance_data: Vec<f32>,
    batches: BTreeMap<GpuInstanceBatchKey, GpuResidentInstanceBatch>,
    entity_slots: BTreeMap<u64, u32>,
    entity_batches: BTreeMap<u64, GpuInstanceBatchKey>,
    generation: u64,
    uploaded_generation: [u64; SCENE_FRAME_SLOTS],
    rebuild_count: u64,
    upload_count: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct RenderSubmissionStats {
    instance_batches: usize,
    instance_count: usize,
    hiz_draws: usize,
    opaque_indirect_groups: usize,
    direct_opaque_draws: usize,
    alpha_draws: usize,
    mass_draws: usize,
    mass_instances: usize,
    graph_executed_passes: u32,
    graph_skipped_passes: u32,
    graph_cpu_record_ms: f32,
}

#[derive(Clone, Copy, Debug)]
struct GpuScene {
    vertex_buffers: [u32; SCENE_FRAME_SLOTS],
    vertex_capacity: u32,
    cube_capacity: usize,
    shadow_vertex_buffers: [u32; SCENE_FRAME_SLOTS],
    shadow_vertex_capacity: u32,
    asset_vertex_buffer: u32,
    asset_vertex_capacity: u32,
    asset_vertex_count: u32,
    skinned_vertex_buffers: [u32; SCENE_FRAME_SLOTS],
    skinned_vertex_capacity: u32,
    asset_index_buffer: u32,
    asset_index_capacity: u32,
    asset_instance_buffers: [u32; SCENE_FRAME_SLOTS],
    asset_instance_capacity: u32,
    visibility_candidate_buffers: [u32; SCENE_FRAME_SLOTS],
    visibility_indirect_buffers: [u32; SCENE_FRAME_SLOTS],
    frame_uniforms: [u32; SCENE_FRAME_SLOTS],
    bind_group_layout: u32,
    bind_groups: [u32; SCENE_FRAME_SLOTS],
    material_bind_group_layout: u32,
    default_base_color_texture: u32,
    default_normal_texture: u32,
    default_specular_texture: u32,
    default_emissive_texture: u32,
    default_environment_texture: u32,
    default_material_uniform: u32,
    material_sampler: u32,
    default_material_bind_group: u32,
    shadow_bind_group_layout: u32,
    shadow_bind_groups: [u32; SCENE_FRAME_SLOTS],
    shadow_render_target: u32,
    shadow_sampler: u32,
    vertex_shader: u32,
    fragment_shader: u32,
    gbuffer_fragment_shader: u32,
    pipeline: u32,
    gbuffer_pipeline: u32,
    alpha_pipeline: u32,
    asset_pipeline: u32,
    asset_gbuffer_pipeline: u32,
    asset_alpha_pipeline: u32,
    shadow_vertex_shader: u32,
    shadow_fragment_shader: u32,
    shadow_pipeline: u32,
    asset_shadow_pipeline: u32,
    asset_vertex_shader: u32,
    asset_shadow_vertex_shader: u32,
    shadow_resolution: u32,
    flare_vertex_buffers: [u32; SCENE_FRAME_SLOTS],
    flare_vertex_shader: u32,
    flare_fragment_shader: u32,
    flare_pipeline: u32,
    particle_vertex_buffers: [u32; SCENE_FRAME_SLOTS],
    particle_additive_pipeline: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkyIndexFormat {
    U16,
    U32,
}

#[derive(Clone, Copy, Debug)]
pub struct SkyVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Debug)]
pub struct SkyMeshResources {
    pub name: String,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub vertices: Vec<SkyVertex>,
    pub indices: Vec<u32>,
    pub index_format: SkyIndexFormat,
}

#[derive(Clone, Debug)]
pub struct SkyTextureResources {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub srgb: bool,
    pub rgba8: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct SkyDomeResources {
    pub model_name: String,
    pub material_name: String,
    pub mesh: SkyMeshResources,
    pub base_noise: SkyTextureResources,
    pub starfield: SkyTextureResources,
    pub detail_noise: SkyTextureResources,
    pub billboard_texture: Option<SkyTextureResources>,
    pub clouds: SkyCloudDesc,
    pub volumetric_clouds: VolumetricCloudDesc,
    /// Radius/scale of the camera-relative sky dome. GTA uses ~20 km.
    pub dome_scale: f32,
    /// Fixed vertical horizon anchor. Horizontal axes follow the camera while
    /// this level remains world-anchored, matching GTA's water/horizon plane.
    pub horizon_level: f32,
}

#[derive(Clone, Copy, Debug)]
struct GpuSky {
    vertex_buffer: u32,
    index_buffer: u32,
    base_noise: u32,
    starfield: u32,
    detail_noise: u32,
    billboard_texture: Option<u32>,
    sampler: u32,
    bind_group_layout: u32,
    bind_groups: [u32; SCENE_FRAME_SLOTS],
    camera_uniforms: [u32; SCENE_FRAME_SLOTS],
    vertex_shader: u32,
    fragment_shader: u32,
    pipeline: u32,
    index_count: u32,
    index_format: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimeCycleBackendState {
    pub cycle_seconds: f32,
    pub phase: f32,
    pub rate: f32,
    pub duration_seconds: f32,
}

impl Default for TimeCycleBackendState {
    fn default() -> Self {
        Self {
            cycle_seconds: 0.0,
            phase: 0.0,
            rate: 1.0,
            duration_seconds: 86_400.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeatherBackendState {
    pub current: String,
    pub next: String,
    pub blend: f32,
}

impl Default for WeatherBackendState {
    fn default() -> Self {
        Self {
            current: String::new(),
            next: String::new(),
            blend: 0.0,
        }
    }
}

/// Resolved physical/output state of the active weather blend.
///
/// The fields mirror the data-driven channels used by GTA V weather.xml so
/// downstream systems (clouds, precipitation, wind and water) consume one
/// authoritative state rather than reimplementing weather logic independently.
#[derive(Clone, Debug, PartialEq)]
pub struct WeatherEffectsState {
    pub current_cloud_settings: String,
    pub next_cloud_settings: String,
    pub current_timecycle: String,
    pub next_timecycle: String,
    pub current_drop_setting: String,
    pub next_drop_setting: String,
    pub current_mist_setting: String,
    pub next_mist_setting: String,
    pub current_ground_setting: String,
    pub next_ground_setting: String,
    pub current_cloud_variant: String,
    pub next_cloud_variant: String,
    pub sun: f32,
    pub cloud: f32,
    pub wind_min: f32,
    pub wind_max: f32,
    pub wind_speed: f32,
    pub wind_direction: [f32; 2],
    pub rain: f32,
    pub snow: f32,
    pub snow_mist: f32,
    pub fog: f32,
    pub ripple_bumpiness: f32,
    pub ripple_min_bumpiness: f32,
    pub ripple_max_bumpiness: f32,
    pub ripple_bumpiness_wind_scale: f32,
    pub ripple_scale: f32,
    pub ripple_speed: f32,
    pub ripple_velocity_transfer: f32,
    pub ocean_bumpiness: f32,
    pub deep_ocean_scale: f32,
    pub ocean_noise_min_amplitude: f32,
    pub ocean_wave_amplitude: f32,
    pub shore_wave_amplitude: f32,
    pub ocean_wave_wind_scale: f32,
    pub shore_wave_wind_scale: f32,
    pub ocean_wave_min_amplitude: f32,
    pub shore_wave_min_amplitude: f32,
    pub ocean_wave_max_amplitude: f32,
    pub shore_wave_max_amplitude: f32,
    pub ocean_foam_intensity: f32,
    pub ocean_foam_scale: f32,
    pub ripple_disturb: f32,
    pub lightning: f32,
    pub sandstorm: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudHatKeyframeState {
    pub enabled: bool,
    pub cloud_color: [f32; 4],
    pub cloud_light_color: [f32; 4],
    pub cloud_ambient_color: [f32; 4],
    pub cloud_sky_color: [f32; 4],
    pub cloud_bounce_color: [f32; 4],
    pub cloud_east_color: [f32; 4],
    pub cloud_west_color: [f32; 4],
    pub scale_fill_colors: [f32; 4],
    pub density_shift_scale_scattering: [f32; 4],
    pub piercing_light: [f32; 4],
    pub scale_diffuse_fill_ambient_wrap: [f32; 4],
}

impl Default for CloudHatKeyframeState {
    fn default() -> Self {
        Self {
            enabled: false,
            cloud_color: [1.0; 4],
            cloud_light_color: [1.0; 4],
            cloud_ambient_color: [0.0, 0.0, 0.0, 1.0],
            cloud_sky_color: [0.0, 0.0, 0.0, 1.0],
            cloud_bounce_color: [0.0, 0.0, 0.0, 1.0],
            cloud_east_color: [0.0, 0.0, 0.0, 1.0],
            cloud_west_color: [0.0, 0.0, 0.0, 1.0],
            scale_fill_colors: [1.0; 4],
            density_shift_scale_scattering: [0.0, 1.0, 1.0, 1.0],
            piercing_light: [1.0; 4],
            scale_diffuse_fill_ambient_wrap: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

impl Default for WeatherEffectsState {
    fn default() -> Self {
        Self {
            current_cloud_settings: String::new(),
            next_cloud_settings: String::new(),
            current_timecycle: String::new(),
            next_timecycle: String::new(),
            current_drop_setting: String::new(),
            next_drop_setting: String::new(),
            current_mist_setting: String::new(),
            next_mist_setting: String::new(),
            current_ground_setting: String::new(),
            next_ground_setting: String::new(),
            current_cloud_variant: String::new(),
            next_cloud_variant: String::new(),
            sun: 1.0,
            cloud: 0.0,
            wind_min: 0.0,
            wind_max: 0.0,
            wind_speed: 0.0,
            wind_direction: [1.0, 0.0],
            rain: 0.0,
            snow: 0.0,
            snow_mist: 0.0,
            fog: 0.0,
            ripple_bumpiness: 0.0,
            ripple_min_bumpiness: 0.0,
            ripple_max_bumpiness: 0.0,
            ripple_bumpiness_wind_scale: 0.0,
            ripple_scale: 0.0,
            ripple_speed: 0.0,
            ripple_velocity_transfer: 0.0,
            ocean_bumpiness: 0.0,
            deep_ocean_scale: 0.0,
            ocean_noise_min_amplitude: 0.0,
            ocean_wave_amplitude: 0.0,
            shore_wave_amplitude: 0.0,
            ocean_wave_wind_scale: 0.0,
            shore_wave_wind_scale: 0.0,
            ocean_wave_min_amplitude: 0.0,
            shore_wave_min_amplitude: 0.0,
            ocean_wave_max_amplitude: 0.0,
            shore_wave_max_amplitude: 0.0,
            ocean_foam_intensity: 0.0,
            ocean_foam_scale: 0.0,
            ripple_disturb: 0.0,
            lightning: 0.0,
            sandstorm: 0.0,
        }
    }
}

#[derive(Debug)]
pub struct Scene3dRuntime {
    title: String,
    camera_entity_id: u64,
    camera: Camera,
    orbit: OrbitCamera,
    render_policy: RenderPolicy,
    cubes: Vec<Cube>,
    asset_meshes: BTreeMap<u64, CpuAssetMesh>,
    skinned_entities: BTreeMap<u64, animation_skinning::SkinnedEntityAnimationState>,
    animation_skinning_pool: animation_skinning::SkinningWorkerPool,
    animation_skinning_in_flight: BTreeSet<u64>,
    main_view_mesh_visibility: mesh_visibility::MainViewMeshVisibility,
    static_asset_instance_epoch: u64,
    asset_model_cache: BTreeMap<u64, std::sync::Arc<[AssetTriangleVertex]>>,
    asset_draw_range_cache: BTreeMap<u64, std::sync::Arc<[AssetDrawRange]>>,
    asset_model_gpu_ranges: BTreeMap<u64, (u32, u32)>,
    asset_gpu_textures: BTreeMap<u64, u32>,
    asset_gpu_pending_textures: BTreeMap<u64, u32>,
    asset_gpu_materials: BTreeMap<(u64, u32), GpuAssetMaterial>,
    asset_binding_contexts: BTreeMap<u64, SceneAssetBindingContext>,
    asset_vertex_data: Vec<f32>,
    asset_upload_from_float: Option<usize>,
    skinned_vertex_data: Vec<f32>,
    skinned_dirty_ranges: [Vec<(usize, usize)>; SCENE_FRAME_SLOTS],
    asset_geometry_full_rebuild: bool,
    retired_asset_vertex_buffers: Vec<u32>,
    transient_spheres: Vec<SceneTransientSphere>,
    overlay_quads: Vec<SceneOverlayQuad>,
    particles: Vec<SceneRuntimeParticle>,
    sky_visuals: BTreeMap<String, SkyVisualDesc>,
    lens_flares: BTreeMap<String, LensFlareDesc>,
    sky_clouds: SkyCloudDesc,
    sky_atmosphere: SkyAtmosphereDesc,
    scene_environment: SceneEnvironmentDesc,
    timecycle_backend: TimeCycleBackendState,
    weather_backend: WeatherBackendState,
    weather_effects: WeatherEffectsState,
    cloudhat_keyframe: CloudHatKeyframeState,
    weather_gpu_fx: Option<WeatherGpuFxResources>,
    weather_fx_time_seconds: f32,
    weather_wetness: f32,
    weather_outdoor_exposure: f32,
    sky_time_seconds: f32,
    sky_time_scale: f32,
    /// Wind-driven procedural base-noise phase. Kept independent from the
    /// time-of-day clock, matching GTA's sky cloud phase integration.
    sky_cloud_noise_phase: [f32; 2],
    /// Continuous time-cycle position measured in days. Detail layers use this
    /// instead of wrapped time-of-day seconds so their phase never pops at midnight.
    sky_cloud_cycle_time_days: f32,
    atmospheric_clouds: Option<AtmosphericCloudResources>,
    atmospheric_cloud_runtime: Vec<AtmosphericCloudLayerRuntime>,
    runtime_entity_ids: BTreeMap<String, u64>,
    next_runtime_entity_id: u64,
    world: SceneWorld,
    frame_plan: SceneFramePlan,
    portal_visibility: Option<PortalVisibilityGraph>,
    gpu_instance_table: GpuInstanceTable,
    mass_instances: mass_instances::MassInstanceStore,
    gpu_mass_instances: BTreeMap<String, mass_instances::GpuMassInstanceLayer>,
    last_submission_stats: RenderSubmissionStats,
    clear_color: [f32; 4],
    gpu: Option<GpuScene>,
    sky: Option<SkyDomeResources>,
    gpu_sky: Option<GpuSky>,
    gpu_atmospheric_clouds: Option<GpuAtmosphericClouds>,
    gpu_volumetric_clouds: Option<GpuVolumetricClouds>,
    gpu_weather: Option<GpuWeatherFx>,
    frame_index: u64,
    last_render_camera: Option<Camera>,
}

#[derive(Clone, Debug)]
pub struct Scene3dLoadReport {
    pub title: String,
    pub entity_count: usize,
    pub camera_name: String,
    pub mesh_name: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneAssetBindingContext {
    pub texture_dictionary: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneDestructionActivation {
    pub entity: u64,
    pub scene_position: [f32; 3],
    pub rotation_degrees: [f32; 3],
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub density: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub impulse_transfer: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneStreamRequest {
    pub stable_id: u64,
    pub asset_ref: String,
    pub priority: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_view_projection_maps_origin_in_front_of_camera() {
        let camera = Camera {
            position: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y_degrees: 60.0,
            near: 0.1,
            far: 100.0,
        };

        let matrix = camera_view_projection(&camera, 16.0 / 9.0);
        assert!(matrix.iter().all(|value| value.is_finite()));

        let clip = mul_mat4_vec4(matrix, [0.0, 0.0, 0.0, 1.0]);
        assert!(clip[0].abs() < 0.001);
        assert!(clip[1].abs() < 0.001);
        assert!(clip[3] > 0.0);
        assert!(clip[2] > 0.0 && clip[2] < clip[3]);

        let behind = mul_mat4_vec4(matrix, [0.0, 0.0, 6.0, 1.0]);
        assert!(behind[3] < 0.0);
    }

    fn mul_mat4_vec4(matrix: [f32; 16], vector: [f32; 4]) -> [f32; 4] {
        [
            matrix[0] * vector[0]
                + matrix[4] * vector[1]
                + matrix[8] * vector[2]
                + matrix[12] * vector[3],
            matrix[1] * vector[0]
                + matrix[5] * vector[1]
                + matrix[9] * vector[2]
                + matrix[13] * vector[3],
            matrix[2] * vector[0]
                + matrix[6] * vector[1]
                + matrix[10] * vector[2]
                + matrix[14] * vector[3],
            matrix[3] * vector[0]
                + matrix[7] * vector[1]
                + matrix[11] * vector[2]
                + matrix[15] * vector[3],
        ]
    }

    #[test]
    fn directional_shadow_projection_is_stable_within_one_texel() {
        let direction = Vec3::new(-0.35, -1.0, -0.2).normalized();
        let shadow_distance = 96.0;
        let shadow_resolution = 2048;
        let half_extent = shadow_distance * 0.55;
        let world_units_per_texel = (half_extent * 2.0) / shadow_resolution as f32;
        let (right, _, _) = view_basis(direction, Vec3::Y);

        let base = directional_shadow_view_projection(
            direction,
            Vec3::ZERO,
            shadow_distance,
            shadow_resolution,
        );
        let sub_texel = directional_shadow_view_projection(
            direction,
            right.mul(world_units_per_texel * 0.25),
            shadow_distance,
            shadow_resolution,
        );

        for (a, b) in base.iter().zip(sub_texel.iter()) {
            assert!(
                (a - b).abs() < 1e-6,
                "shadow projection moved within one texel"
            );
        }
    }

    #[test]
    fn orbit_camera_rotates_only_while_dragging() {
        let camera = Camera {
            position: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y_degrees: 60.0,
            near: 0.1,
            far: 100.0,
        };
        let mut orbit = OrbitCamera::from_camera(&camera);
        orbit.rotate_sensitivity = 0.005;
        orbit.zoom_sensitivity = 0.0015;
        orbit.min_distance = 2.0;
        orbit.max_distance = 40.0;
        orbit.min_pitch_degrees = -85.0;
        orbit.max_pitch_degrees = 85.0;
        orbit.configured = true;
        let yaw = orbit.yaw;
        orbit.apply_mouse(100.0, 0.0, 0.0, false);
        assert!((orbit.yaw - yaw).abs() < f32::EPSILON);
        orbit.apply_mouse(100.0, 0.0, 0.0, true);
        assert!((orbit.yaw - yaw).abs() > 0.1);
    }

    #[test]
    fn orbit_camera_wheel_changes_distance_with_clamp() {
        let camera = Camera {
            position: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y_degrees: 60.0,
            near: 0.1,
            far: 100.0,
        };
        let mut orbit = OrbitCamera::from_camera(&camera);
        orbit.rotate_sensitivity = 0.005;
        orbit.zoom_sensitivity = 0.0015;
        orbit.min_distance = 2.0;
        orbit.max_distance = 40.0;
        orbit.min_pitch_degrees = -85.0;
        orbit.max_pitch_degrees = 85.0;
        orbit.configured = true;
        let distance = orbit.distance;
        orbit.apply_mouse(0.0, 0.0, 120.0, false);
        assert!(orbit.distance < distance);
        orbit.apply_mouse(0.0, 0.0, 100000.0, false);
        assert_eq!(orbit.distance, orbit.min_distance);
    }

    #[test]
    fn instance_matrix_matches_cpu_transform_order() {
        let position = Vec3::new(7.0, -2.0, 11.0);
        let rotation = Vec3::new(21.0, -37.0, 13.0);
        let scale = Vec3::new(1.5, 0.75, -2.0);
        let point = Vec3::new(3.0, -4.0, 2.0);
        let matrix = geometry::instance_model_matrix(position, rotation, scale);
        let gpu = mul_mat4_vec4(matrix, [point.x, point.y, point.z, 1.0]);
        let cpu = transform_point(point, scale, rotation, position);
        assert!((gpu[0] - cpu.x).abs() < 1.0e-5);
        assert!((gpu[1] - cpu.y).abs() < 1.0e-5);
        assert!((gpu[2] - cpu.z).abs() < 1.0e-5);
        assert!((gpu[3] - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn sky_visual_direction_is_opposite_directional_light_ray_for_same_rotation() {
        let rotation = Vec3::new(-35.0, 115.0, 0.0);
        let light_ray = light_direction(rotation);
        let sky_direction = sky_visual_direction(rotation);
        assert!((light_ray.dot(sky_direction) + 1.0).abs() < 1.0e-5);
        assert!(sky_direction.y > 0.0);
    }

    #[test]
    fn flare_quad_uses_six_vertices_with_procedural_payload() {
        let mut out = Vec::new();
        append_flare_quad(
            &mut out,
            [0.25, -0.1],
            [0.05, 0.08],
            [1.0, 0.8, 0.4],
            0.5,
            2.0,
        );
        assert_eq!(out.len(), 6 * FLARE_FLOATS_PER_VERTEX);
        assert_eq!(out[4..8], [1.0, 0.8, 0.4, 0.5]);
        assert_eq!(out[8], 2.0);
    }

    #[test]
    fn scene_ray_occlusion_hits_forward_aabb_and_rejects_side_aabb() {
        let origin = Vec3::ZERO;
        let direction = Vec3::new(0.0, 0.0, -1.0);
        let forward = SceneBounds::from_center_half_extent(
            Vec3::new(0.0, 0.0, -5.0),
            Vec3::new(1.0, 1.0, 1.0),
        );
        let side = SceneBounds::from_center_half_extent(
            Vec3::new(5.0, 0.0, -5.0),
            Vec3::new(1.0, 1.0, 1.0),
        );
        assert!(ray_hits_aabb(origin, direction, forward, 0.05));
        assert!(!ray_hits_aabb(origin, direction, side, 0.05));
    }

    #[test]
    fn cube_output_matches_world_vertex_layout_and_collision_bounds() {
        let cube = Cube {
            position: Vec3::new(2.0, 1.0, -3.0),
            rotation_degrees: Vec3::ZERO,
            scale: Vec3::new(4.0, 2.0, 6.0),
            base_color: [1.0; 4],
        };
        let mut out = Vec::new();
        cube.append_vertices(&mut out);
        assert_eq!(out.len(), CUBE_VERTEX_COUNT as usize * FLOATS_PER_VERTEX);
        assert_eq!(VERTEX_STRIDE, 68);
        assert_eq!(cube.bounds().min, [0.0, 0.0, -6.0]);
        assert_eq!(cube.bounds().max, [4.0, 2.0, 0.0]);
        assert_eq!(out[3], 0.0);
        assert_eq!(&out[4..7], &[0.0, 0.0, 1.0]);
        assert_eq!(&out[7..11], &[1.0, 1.0, 1.0, 1.0]);
        assert_eq!(&out[11..13], &[0.0, 0.0]);
        assert_eq!(&out[13..17], &[1.0, 0.0, 0.0, 1.0]);
    }
}
