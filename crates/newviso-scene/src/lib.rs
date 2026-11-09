#![recursion_limit = "256"]

mod camera;
mod first_scene;
mod math;
pub mod water;
mod world;
pub use newviso_scene_document::{CharacterUpdatePhase, SceneCharacterBinding};

pub use first_scene::{
    prepare_model_geometry, AtmosphericCloudAnimMode, AtmosphericCloudLayerDesc,
    AtmosphericCloudLayerResources, AtmosphericCloudMeshResources, AtmosphericCloudResources,
    AtmosphericCloudTextureSet, AtmosphericCloudUvLayerDesc, AtmosphericCloudVertex,
    CloudHatKeyframeState, KeyframedSkyCloudDesc, KeyframedSkyDesc, LensFlareDesc,
    LensFlareElementDesc, LensFlareElementKind, Scene3dLoadReport, Scene3dRuntime,
    SceneArmIkConstraint, SceneAuthoringCamera, SceneDestructionActivation, SceneEnvironmentDesc,
    SceneFootSupport, SceneGpuWarmupStatus, SceneLegIkConstraint, SceneLightDesc, SceneLightType,
    SceneMassInstanceDesc, SceneModelDent, SceneModelPartPose, SceneNavigationGeometry,
    SceneOverlayQuad, SceneParticleBlend, SceneParticleMesh, SceneParticleSpawnDesc,
    SceneParticleStyle, ScenePhysicalParticleSpawn, ScenePhysicalRecovery, ScenePhysicsSource,
    ScenePreparedModelGeometry, SceneResolvedMaterial, SceneRuntimeEntityDesc,
    SceneRuntimeVisualKind, SceneSnowCap, SceneStreamRequest, SceneSurfaceMark,
    SceneTransientSphere, SceneVehicleDashboard, SkyAtmosphereDesc, SkyCloudDesc, SkyDomeResources,
    SkyIndexFormat, SkyMeshResources, SkyTextureResources, SkyVertex, SkyVisualDesc, SkyVisualKind,
    TimeCycleBackendState, VolumetricCloudDesc, WeatherBackendState, WeatherEffectsState,
    WeatherGpuFxEmitterDesc, WeatherGpuFxLayerDesc, WeatherGpuFxRenderDesc, WeatherGpuFxResources,
    WeatherGpuFxSystemType,
};
