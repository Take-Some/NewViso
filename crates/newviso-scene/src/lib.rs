#![recursion_limit = "256"]

mod camera;
mod first_scene;
mod math;
mod world;

pub use first_scene::{
    prepare_model_geometry, AtmosphericCloudAnimMode, AtmosphericCloudLayerDesc,
    AtmosphericCloudLayerResources, AtmosphericCloudMeshResources, AtmosphericCloudResources,
    AtmosphericCloudTextureSet, AtmosphericCloudUvLayerDesc, AtmosphericCloudVertex,
    CloudHatKeyframeState, LensFlareDesc, LensFlareElementDesc, LensFlareElementKind,
    Scene3dLoadReport, Scene3dRuntime, SceneDestructionActivation, SceneEnvironmentDesc,
    SceneLightDesc, SceneLightType, SceneMassInstanceDesc, SceneModelPartPose, SceneOverlayQuad,
    SceneParticleBlend, SceneParticleSpawnDesc, ScenePreparedModelGeometry, SceneResolvedMaterial,
    SceneRuntimeEntityDesc, SceneRuntimeVisualKind, SceneStreamRequest, SceneTransientSphere,
    SkyAtmosphereDesc, SkyCloudDesc, SkyDomeResources, SkyIndexFormat, SkyMeshResources,
    SkyTextureResources, SkyVertex, SkyVisualDesc, SkyVisualKind, TimeCycleBackendState,
    VolumetricCloudDesc, WeatherBackendState, WeatherEffectsState, WeatherGpuFxEmitterDesc,
    WeatherGpuFxLayerDesc, WeatherGpuFxRenderDesc, WeatherGpuFxResources, WeatherGpuFxSystemType,
};
