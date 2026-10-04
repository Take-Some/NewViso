use newviso_agent::{
    AgentDesc, AgentPerceptionPolicy, AgentRuntime, AgentTaskDesc, AgentThinkingPolicy,
};
use newviso_assets_client::AssetClient;
use newviso_bugtrap as bugtrap;
use newviso_capabilities::{resolve_capabilities, CapabilityNeed, ResolvedCapability};
use newviso_collision::CollisionMeshResource;
use newviso_compat_abi::platform::{
    PlatformCursorGrabModeV1, PlatformCursorPollV1, PlatformCursorStateV1,
    PlatformLoadingOverlayV1, PlatformSurfaceMetricsV1, PlatformWindowReadyV1,
};
use newviso_config::ResolvedBootstrapConfig;
use newviso_content_manager::{ContentEffect, ContentManager};
use newviso_core::{EngineState, RuntimePhase};
use newviso_events::{topic as event_topic, EventPhase};
use newviso_host as host;
use newviso_input_client::InputSnapshot;
use newviso_items::{ItemDefinition, ItemsRuntime, WorldPickup};
use newviso_model::{
    IndexBuffer as ModelIndexBuffer, IndexFormat as ModelIndexFormat, ModelAnimationClip,
    ModelResource, VertexFormat as ModelVertexFormat, VertexSemantic,
    VertexStream as ModelVertexStream,
};
use newviso_physics_client::{
    CollisionShape, MeshCollider, PhysicsBodyActivityUpdate, PhysicsBodyFlags, PhysicsBodyKind,
    PhysicsBodyPoseUpdate, PhysicsBodySnapshot, PhysicsClient, PhysicsCollider, PhysicsCommand,
    PhysicsCommandKind, PhysicsEvent, PhysicsFeature, PhysicsFrameColliderSnapshot,
    PhysicsFrameInput, PhysicsFrameOutput, PhysicsMaterial, PhysicsQuery, PhysicsQueryKind,
};
use newviso_platform::{run_platform, PlatformApplication, PlatformRunConfig, PlatformRunReport};
use newviso_project::{
    ProjectAtmosphericCloudAnimMode, ProjectAtmosphericClouds, ProjectEnvironment,
    ProjectRuntimeSettings, ProjectScripts, ProjectSkyEnvironment, ProjectStreamingSettings,
    ResolvedProject,
};
use newviso_provider_runtime::{
    probe_directory, probe_lifecycle, BootstrapPhase, ProviderInfo, RootSymbol, RunningProvider,
};
use newviso_render_client::RenderClient;
use newviso_resource_runtime::{
    AssetAddress, AssetClientSource, AssetStreamer, ResourceManager, StreamingClaim,
    StreamingOwnerId, StreamingPolicy,
};
use newviso_scene::{
    AtmosphericCloudAnimMode, AtmosphericCloudLayerDesc, AtmosphericCloudLayerResources,
    AtmosphericCloudMeshResources, AtmosphericCloudResources, AtmosphericCloudTextureSet,
    AtmosphericCloudUvLayerDesc, AtmosphericCloudVertex, CloudHatKeyframeState, LensFlareDesc,
    LensFlareElementDesc, LensFlareElementKind, Scene3dLoadReport, Scene3dRuntime,
    SceneDestructionActivation, SceneLightDesc, SceneLightType, SceneMassInstanceDesc,
    SceneOverlayQuad, SceneParticleBlend, SceneParticleSpawnDesc, SceneRuntimeEntityDesc,
    SceneRuntimeVisualKind, SceneSurfaceMark, SceneTransientSphere, SkyAtmosphereDesc,
    SkyCloudDesc, SkyDomeResources, SkyIndexFormat, SkyMeshResources, SkyTextureResources,
    SkyVertex, SkyVisualDesc, SkyVisualKind, VolumetricCloudDesc, WeatherEffectsState,
    WeatherGpuFxEmitterDesc, WeatherGpuFxLayerDesc, WeatherGpuFxRenderDesc, WeatherGpuFxResources,
    WeatherGpuFxSystemType,
};
use newviso_scripting::{ScriptModuleSpec, ScriptPermission, ScriptRuntime};
use newviso_semantic_assets::{
    load_model_animation_clip, SemanticCollisionDecoder, SemanticMaterialDecoder,
    SemanticModelDecoder, SemanticTextureDecoder,
};
use newviso_ui_client::UiClient;
use newviso_vehicle::{
    VehicleBodyState, VehicleDefinition, VehicleInput, VehicleProbeHit, VehicleRuntime,
};
use newviso_world::{
    AmbientModelSetDesc, LivingWorldRuntime, LivingWorldZoneDesc, PopulationChannelDesc,
    PopulationStreamingPolicyDesc, RelationshipRuleDesc, ScenarioPointDesc, WorldActorDesc,
    WorldClockPolicyDesc, WorldNavEdgeDesc, WorldNavNodeDesc, WorldObserverDesc, WorldProcessDesc,
    WorldRealityEventDesc, WorldScenarioReservationDesc, WorldScheduledEventDesc,
    WorldSimulationPolicyDesc, WorldStimulusDesc, WorldTravelRequestDesc,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

mod application_snapshot;
mod environment_sky;
use environment_sky::*;
mod environment_clouds;
use environment_clouds::*;
mod environment_assets;
use environment_assets::*;
mod command_decode;
use command_decode::*;
mod ui_bindings;
use ui_bindings::*;

mod application_agents;
mod application_peds;
mod application_animation;
mod application_characters;
mod application_content;
mod application_events;
mod application_particle_debris;
mod application_particle_effects;
mod application_physics;
mod application_settings;
mod application_startup_loading;
use application_physics::PhysicsRuntime;
mod application_platform;
mod application_script_commands;
mod application_streaming;
mod application_vehicle_tracks;
mod application_vehicles;
mod application_weather;
mod application_world;
mod world_persistence;
use world_persistence::{WorldPersistence, WorldStartup};
mod bootstrap;
mod bootstrap_support;
mod engine_scripts;
pub use bootstrap::run;

pub fn builtin_resource_manager() -> ResourceManager<AssetClientSource> {
    let mut resources = ResourceManager::new(AssetClientSource::default());
    resources.register_decoder(SemanticModelDecoder);
    resources.register_decoder(SemanticMaterialDecoder);
    resources.register_decoder(SemanticTextureDecoder);
    resources.register_decoder(SemanticCollisionDecoder);
    resources
}

pub fn builtin_asset_streamer(
    policy: StreamingPolicy,
) -> Result<AssetStreamer<AssetClientSource>, String> {
    AssetStreamer::new(builtin_resource_manager(), policy)
}

fn streaming_policy_from_project(settings: &ProjectStreamingSettings) -> StreamingPolicy {
    const MIB: u64 = 1024 * 1024;
    StreamingPolicy {
        max_resident_bytes: settings.max_resident_mb.saturating_mul(MIB),
        max_loads_per_tick: settings.max_loads_per_tick,
        parallel_loads: settings.parallel_loads,
        max_source_bytes_per_tick: settings.max_source_mb_per_tick.saturating_mul(MIB),
        eviction_grace_frames: settings.eviction_grace_frames,
        failed_retry_frames: settings.failed_retry_frames,
        dependency_priority_scale: settings.dependency_priority_scale,
    }
}

pub struct RuntimeReport {
    pub provider_count: usize,
    pub scene: Scene3dLoadReport,
    pub platform: Option<PlatformRunReport>,
}

#[derive(Clone, Debug, Deserialize)]
struct VfsMountLayer {
    mount: String,
    priority: i32,
}

#[derive(Clone, Debug, Deserialize)]
struct VfsMountPolicy {
    schema: String,
    shared_assets: VfsMountLayer,
    project_root: VfsMountLayer,
    project_assets: VfsMountLayer,
}

#[derive(Clone, Debug)]
struct ProviderRoles {
    logging: String,
    input: String,
    assets: String,
    ecs: String,
    platform: String,
    renderer: String,
}

impl ProviderRoles {
    fn resolve(bootstrap: &ResolvedBootstrapConfig, project: Option<&ResolvedProject>) -> Self {
        let overrides = project.map(|project| &project.manifest.providers);
        Self {
            logging: overrides
                .and_then(|providers| providers.logging.clone())
                .unwrap_or_else(|| bootstrap.logging_provider.clone()),
            input: overrides
                .and_then(|providers| providers.input.clone())
                .unwrap_or_else(|| bootstrap.input_provider.clone()),
            assets: overrides
                .and_then(|providers| providers.assets.clone())
                .unwrap_or_else(|| bootstrap.assets_provider.clone()),
            ecs: overrides
                .and_then(|providers| providers.ecs.clone())
                .unwrap_or_else(|| bootstrap.ecs_provider.clone()),
            platform: overrides
                .and_then(|providers| providers.platform.clone())
                .unwrap_or_else(|| bootstrap.platform_provider.clone()),
            renderer: overrides
                .and_then(|providers| providers.renderer.clone())
                .unwrap_or_else(|| bootstrap.renderer_provider.clone()),
        }
    }
}

#[derive(Clone, Debug)]
struct LoadedProjectFiles {
    runtime: ProjectRuntimeSettings,
    environment: ProjectEnvironment,
    render_defaults: Value,
    scripts: Option<ProjectScripts>,
    ui_surface: Option<Value>,
    context: Value,
}

fn default_npc_walk_threshold() -> f32 {
    0.15
}

fn default_npc_run_threshold() -> f32 {
    3.5
}

fn default_npc_animation_rate_hz() -> f32 {
    30.0
}

fn default_npc_face_velocity() -> bool {
    true
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
struct WorldActorLocomotionBinding {
    #[serde(default)]
    idle_clip: Option<String>,
    #[serde(default)]
    walk_clip: Option<String>,
    #[serde(default)]
    run_clip: Option<String>,
    #[serde(default)]
    jump_clip: Option<String>,
    #[serde(default)]
    fall_clip: Option<String>,
    #[serde(default)]
    land_clip: Option<String>,
    #[serde(default = "default_npc_walk_threshold")]
    walk_speed_threshold: f32,
    #[serde(default = "default_npc_run_threshold")]
    run_speed_threshold: f32,
    #[serde(default = "default_npc_animation_rate_hz")]
    animation_rate_hz: f32,
    #[serde(default = "default_npc_face_velocity")]
    face_velocity: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct WorldActorPresentationBinding {
    scene_key: String,
    visual: SceneRuntimeVisualKind,
    asset_ref: Option<String>,
    position_offset: [f32; 3],
    rotation_degrees: [f32; 3],
    scale: [f32; 3],
    bounds_half_extent: [f32; 3],
    base_color: [f32; 4],
    solid: bool,
    #[serde(with = "application_world::distance_serde")]
    visible_distance: f32,
    #[serde(with = "application_world::distance_serde")]
    stream_distance: f32,
    fade_range: f32,
    materialized_representations: Vec<String>,
    #[serde(default)]
    locomotion: Option<WorldActorLocomotionBinding>,
}

#[derive(Clone, Debug)]
struct SceneAnimationBinding {
    clip_ref: String,
    playback_rate: f32,
    restart_if_same: bool,
    /// Authored local clip time to start from. Cutscene playback uses this to
    /// stay locked to the master CUT timeline after hitches/late section entry.
    start_time_seconds: f32,
    /// Cross-fade duration when replacing the current clip. Gameplay keeps the
    /// soft default; authored CUT sections may request a hard transition.
    blend_seconds: f32,
    /// Engine elapsed time at which this binding was requested. This is
    /// stamped by bind_scene_entity_animation and lets deferred streaming
    /// materialization recover the correct authored phase instead of starting
    /// late from the original section offset.
    bound_elapsed_seconds: f64,
    /// Apply the animation clip's optional authored entity mover tracks.
    /// Generic locomotion leaves this false; mission/cutscene playback opts in.
    apply_mover: bool,
}

struct EngineApplication {
    renderer_path: PathBuf,
    renderer: Option<RunningProvider>,
    scene: Scene3dRuntime,
    settings: ProjectRuntimeSettings,
    living_world: LivingWorldRuntime,
    agents: AgentRuntime,
    peds: newviso_agent::peds::PedRuntime,
    physical_characters: application_characters::PhysicalCharacterRuntime,
    vehicles: VehicleRuntime,
    vehicle_tracks: application_vehicle_tracks::VehicleTracks,
    vehicle_presentations: BTreeMap<u64, application_vehicles::VehiclePresentationBinding>,
    vehicle_debris: BTreeMap<u64, application_vehicles::VehicleDebrisState>,
    next_vehicle_debris_serial: u64,
    vehicle_auto_wheel_layout: BTreeSet<u64>,
    vehicle_specifications: Option<BTreeMap<String, newviso_vehicle::VehicleSpecification>>,
    vehicle_explicit_specifications: BTreeSet<u64>,
    items: ItemsRuntime,
    world_actor_presentations: BTreeMap<String, WorldActorPresentationBinding>,
    world_presentation_states: BTreeMap<String, application_world::PresentationState>,
    world_persistence: Option<WorldPersistence>,
    world_save_allowed: bool,
    physics: Option<PhysicsRuntime>,
    scripts: Option<ScriptRuntime>,
    project_context: Value,
    ui_template: Option<Value>,
    ui_bindings: BTreeMap<String, Value>,
    last_ui_surface: Option<Value>,
    content_manager: Option<ContentManager>,
    asset_streamer: AssetStreamer<AssetClientSource>,
    scene_stream_claims: BTreeMap<u64, AssetAddress>,
    scene_stream_priorities: BTreeMap<u64, f32>,
    scene_stream_aux_claims: BTreeMap<u64, BTreeSet<AssetAddress>>,
    scene_materialization_queue: VecDeque<u64>,
    scene_materialization_pending: BTreeSet<u64>,
    scene_prepare_pool: application_streaming::ScenePreparePool,
    scene_model_prepare_in_flight: BTreeSet<u64>,
    scene_model_prepare_waiters: BTreeMap<u64, BTreeSet<u64>>,
    scene_prepared_models:
        BTreeMap<u64, Result<Arc<newviso_scene::ScenePreparedModelGeometry>, String>>,
    scene_collision_prepare_in_flight: BTreeSet<(u64, u64)>,
    scene_prepared_collisions:
        BTreeMap<(u64, u64), Result<application_physics::PreparedStreamedCollision, String>>,
    scene_animation_bindings: BTreeMap<u64, SceneAnimationBinding>,
    animation_clip_cache: BTreeMap<(u64, String), Arc<ModelAnimationClip>>,
    last_content_surfaces: Vec<Value>,
    ui_frame_index: u64,
    next_script_world_snapshot_seconds: f64,
    elapsed_seconds: f64,
    ready: bool,
    startup_map_ready: bool,
    startup_map_progress: f32,
    startup_map_spinner_phase: u32,
    startup_map_ready_streak: u8,
    startup_map_interest_peak: usize,
    startup_gpu_upload_peak_jobs: u32,
    startup_map_status: String,
    startup_map_detail: String,
    exit_requested: bool,
    cursor_captured: bool,
    window_focused: bool,
}

impl EngineApplication {
    fn new(
        renderer_path: PathBuf,
        mut scene: Scene3dRuntime,
        physics: Option<PhysicsRuntime>,
        scripts: Option<ScriptRuntime>,
        project_context: Value,
        ui_template: Option<Value>,
        content_manager: Option<ContentManager>,
        settings: ProjectRuntimeSettings,
        world_startup: WorldStartup,
    ) -> Result<Self, String> {
        settings.validate().map_err(|error| error.to_string())?;
        scene.enable_physical_particle_debris(physics.is_some());
        let streaming_policy = streaming_policy_from_project(&settings.streaming);
        let scene_prepare_pool =
            application_streaming::ScenePreparePool::new(settings.streaming.parallel_loads)?;
        Ok(Self {
            settings,
            renderer_path,
            renderer: None,
            scene,
            living_world: world_startup.world,
            agents: AgentRuntime::default(),
            peds: newviso_agent::peds::PedRuntime::default(),
            physical_characters: application_characters::PhysicalCharacterRuntime::default(),
            vehicles: VehicleRuntime::new(),
            vehicle_tracks: Default::default(),
            vehicle_presentations: BTreeMap::new(),
            vehicle_debris: BTreeMap::new(),
            next_vehicle_debris_serial: 0,
            vehicle_auto_wheel_layout: BTreeSet::new(),
            vehicle_specifications: None,
            vehicle_explicit_specifications: BTreeSet::new(),
            items: world_startup.items,
            world_actor_presentations: world_startup.presentations,
            world_presentation_states: BTreeMap::new(),
            world_persistence: world_startup.persistence,
            world_save_allowed: false,
            physics,
            scripts,
            project_context,
            ui_template,
            ui_bindings: BTreeMap::new(),
            last_ui_surface: None,
            content_manager,
            asset_streamer: builtin_asset_streamer(streaming_policy)?,
            scene_stream_claims: BTreeMap::new(),
            scene_stream_priorities: BTreeMap::new(),
            scene_stream_aux_claims: BTreeMap::new(),
            scene_materialization_queue: VecDeque::new(),
            scene_materialization_pending: BTreeSet::new(),
            scene_prepare_pool,
            scene_model_prepare_in_flight: BTreeSet::new(),
            scene_model_prepare_waiters: BTreeMap::new(),
            scene_prepared_models: BTreeMap::new(),
            scene_collision_prepare_in_flight: BTreeSet::new(),
            scene_prepared_collisions: BTreeMap::new(),
            scene_animation_bindings: BTreeMap::new(),
            animation_clip_cache: BTreeMap::new(),
            last_content_surfaces: Vec::new(),
            ui_frame_index: 0,
            next_script_world_snapshot_seconds: 0.0,
            elapsed_seconds: 0.0,
            ready: false,
            startup_map_ready: false,
            startup_map_progress: 0.02,
            startup_map_spinner_phase: 0,
            startup_map_ready_streak: 0,
            startup_map_interest_peak: 0,
            startup_gpu_upload_peak_jobs: 0,
            startup_map_status: "Discovering map assets...".to_owned(),
            startup_map_detail: "Preparing initial model and texture residency.".to_owned(),
            exit_requested: false,
            cursor_captured: false,
            window_focused: true,
        })
    }
}

#[cfg(test)]
mod binding_tests {
    use super::*;

    #[test]
    fn generated_atmospheric_cloud_fallback_is_world_horizontal() {
        let mesh = generated_atmospheric_cloud_sheet("test");
        assert!(!mesh.vertices.is_empty());
        assert!(mesh
            .vertices
            .iter()
            .all(|vertex| vertex.position[1].abs() <= f32::EPSILON));
        assert_eq!(mesh.bounds_min[1], 0.0);
        assert_eq!(mesh.bounds_max[1], 0.0);
        assert!(mesh.name.contains("cloud_sheet"));
    }

    #[test]
    fn ui_template_materializes_script_bindings() {
        let template = json!({
            "body_lines": [
                "Yaw: {{camera.yaw}}",
                "Position: {{camera.x}}, {{camera.y}}, {{camera.z}}"
            ]
        });
        let bindings = BTreeMap::from([
            (
                "camera.yaw".to_owned(),
                Value::String("32.10Р’В°".to_owned()),
            ),
            ("camera.x".to_owned(), Value::String("4.200".to_owned())),
            ("camera.y".to_owned(), Value::String("3.000".to_owned())),
            ("camera.z".to_owned(), Value::String("6.000".to_owned())),
        ]);

        let materialized = materialize_ui_template(&template, &bindings);
        assert_eq!(
            materialized["body_lines"][0].as_str(),
            Some("Yaw: 32.10Р’В°")
        );
        assert_eq!(
            materialized["body_lines"][1].as_str(),
            Some("Position: 4.200, 3.000, 6.000")
        );
    }
}
