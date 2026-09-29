use newviso_agent::{
    AgentDesc, AgentPerceptionPolicy, AgentRuntime, AgentTaskDesc, AgentThinkingPolicy,
};
use newviso_assets_client::AssetClient;
use newviso_bugtrap as bugtrap;
use newviso_capabilities::{resolve_capabilities, CapabilityNeed, ResolvedCapability};
use newviso_collision::CollisionMeshResource;
use newviso_compat_abi::platform::{
    PlatformCursorGrabModeV1, PlatformCursorPollV1, PlatformCursorStateV1,
    PlatformSurfaceMetricsV1, PlatformWindowReadyV1,
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
    SceneRuntimeVisualKind, SceneTransientSphere, SkyAtmosphereDesc, SkyCloudDesc,
    SkyDomeResources, SkyIndexFormat, SkyMeshResources, SkyTextureResources, SkyVertex,
    SkyVisualDesc, SkyVisualKind, VolumetricCloudDesc, WeatherEffectsState,
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

mod application_agents;
mod application_animation;
mod application_characters;
mod application_content;
mod application_events;
mod application_particle_effects;
mod application_physics;
mod application_settings;
use application_physics::PhysicsRuntime;
mod application_platform;
mod application_script_commands;
mod application_streaming;
mod application_vehicles;
mod application_weather;
mod application_world;
mod world_persistence;
use world_persistence::{WorldPersistence, WorldStartup};
mod bootstrap;
mod bootstrap_support;
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

fn load_environment_sky(config: &ProjectSkyEnvironment) -> Result<SkyDomeResources, String> {
    let assets = AssetClient::new();
    let model_address = AssetAddress::parse(&config.model)
        .map_err(|error| format!("invalid environment sky model address: {error}"))?;
    if model_address.entry().is_none() {
        return Err("environment sky model requires @entry".to_owned());
    }

    let mut resources = builtin_resource_manager();
    let model = resources.load::<ModelResource>(&model_address)?;
    let mesh = model
        .meshes
        .first()
        .ok_or_else(|| format!("sky model '{}' contains no meshes", model.name))?;
    let position = mesh
        .vertex_streams
        .iter()
        .find(|stream| stream.semantic == VertexSemantic::Position)
        .ok_or_else(|| format!("sky mesh '{}' has no position stream", mesh.name))?;
    let uv = mesh
        .vertex_streams
        .iter()
        .find(|stream| stream.semantic == VertexSemantic::TexCoord(0))
        .ok_or_else(|| format!("sky mesh '{}' has no texcoord0 stream", mesh.name))?;
    if position.vertex_count != uv.vertex_count {
        return Err(format!(
            "sky mesh '{}' position vertex_count={} differs from texcoord0 vertex_count={}",
            mesh.name, position.vertex_count, uv.vertex_count
        ));
    }

    let vertices = (0..position.vertex_count as usize)
        .map(|index| {
            Ok(SkyVertex {
                position: read_sky_vec3(position, index)?,
                uv: read_sky_vec2(uv, index)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let (index_format, indices) = decode_sky_indices(&mesh.index_buffer)?;

    let base_noise_ref = config
        .base_noise_texture
        .as_deref()
        .unwrap_or("textures/skydome.ytd@baseperlinnoise3channel");
    let starfield_ref = config
        .starfield_texture
        .as_deref()
        .unwrap_or("textures/skydome.ytd@starfield");
    let detail_noise_ref = config
        .detail_noise_texture
        .as_deref()
        .unwrap_or("textures/skydome.ytd@noise16_p");

    let base_noise = load_sky_texture(&assets, base_noise_ref, false)?;
    let starfield = load_sky_texture(&assets, starfield_ref, true)?;
    let detail_noise = load_sky_texture(&assets, detail_noise_ref, false)?;
    let billboard_texture = config
        .billboard_texture
        .as_deref()
        .map(|reference| load_sky_texture(&assets, reference, true))
        .transpose()?;

    let model_name = model.name.clone();
    let mesh_name = mesh.name.clone();
    let material_name = "environment.sky".to_owned();
    host::info(
        "newviso.scene",
        format!(
            "sky semantic closure ready model='{}' mesh='{}' vertices={} indices={} textures=[{},{},{}] billboard={}",
            model_name,
            mesh_name,
            vertices.len(),
            indices.len(),
            base_noise.name,
            starfield.name,
            detail_noise.name,
            billboard_texture
                .as_ref()
                .map(|texture| texture.name.as_str())
                .unwrap_or("-")
        ),
    );

    Ok(SkyDomeResources {
        model_name,
        material_name,
        mesh: SkyMeshResources {
            name: mesh_name,
            bounds_min: model.bounds.min,
            bounds_max: model.bounds.max,
            vertices,
            indices,
            index_format,
        },
        base_noise,
        starfield,
        detail_noise,
        billboard_texture,
        clouds: SkyCloudDesc {
            enabled: config.clouds.enabled,
            coverage: config.clouds.coverage,
            density: config.clouds.density,
            softness: config.clouds.softness,
            scale: config.clouds.scale,
            detail_scale: config.clouds.detail_scale,
            speed: config.clouds.speed,
            horizon_fade: config.clouds.horizon_fade,
            macro_scale: config.clouds.macro_scale,
            macro_strength: config.clouds.macro_strength,
            detail_strength: config.clouds.detail_strength,
            micro_strength: config.clouds.micro_strength,
            erosion_strength: config.clouds.erosion_strength,
            warp_strength: config.clouds.warp_strength,
            shape_contrast: config.clouds.shape_contrast,
            shear_speed: config.clouds.shear_speed,
            seed_offset: config.clouds.seed_offset,
            large_speed: config.clouds.large_speed,
            small_speed: config.clouds.small_speed,
            overall_detail_speed: config.clouds.overall_detail_speed,
            edge_detail_speed: config.clouds.edge_detail_speed,
            noise_phase_scale: config.clouds.noise_phase_scale,
        },
        volumetric_clouds: VolumetricCloudDesc {
            enabled: config.volumetric_clouds.enabled,
            base_altitude: config.volumetric_clouds.base_altitude,
            top_altitude: config.volumetric_clouds.top_altitude,
            max_distance: config.volumetric_clouds.max_distance,
            resolution_scale: config.volumetric_clouds.resolution_scale,
            ray_steps: config.volumetric_clouds.ray_steps,
            light_steps: config.volumetric_clouds.light_steps,
            coverage: config.volumetric_clouds.coverage,
            density: config.volumetric_clouds.density,
            shape_scale: config.volumetric_clouds.shape_scale,
            detail_scale: config.volumetric_clouds.detail_scale,
            detail_strength: config.volumetric_clouds.detail_strength,
            erosion_strength: config.volumetric_clouds.erosion_strength,
            extinction: config.volumetric_clouds.extinction,
            scattering: config.volumetric_clouds.scattering,
            ambient: config.volumetric_clouds.ambient,
            phase_forward: config.volumetric_clouds.phase_forward,
            powder_strength: config.volumetric_clouds.powder_strength,
            temporal_blend: config.volumetric_clouds.temporal_blend,
            jitter_strength: config.volumetric_clouds.jitter_strength,
        },
        dome_scale: config.dome_scale,
        horizon_level: config.horizon_level,
    })
}

fn load_environment_atmospheric_clouds(
    config: &ProjectAtmosphericClouds,
) -> Result<AtmosphericCloudResources, String> {
    let assets = AssetClient::new();
    let mut resources = builtin_resource_manager();
    let mut layers = Vec::with_capacity(config.layers.len());
    let mut mesh_cache =
        BTreeMap::<(String, usize), (String, AtmosphericCloudMeshResources)>::new();
    let mut texture_cache = BTreeMap::<String, SkyTextureResources>::new();

    let neutral_normal = SkyTextureResources {
        name: "newviso.clouds.neutral_normal".to_owned(),
        width: 1,
        height: 1,
        srgb: false,
        rgba8: vec![128, 128, 255, 255],
    };

    for layer in &config.layers {
        let (model_name, mesh) = if let Some(reference) = layer.model.as_deref() {
            let cache_key = (reference.to_owned(), layer.model_mesh_index);
            if let Some(cached) = mesh_cache.get(&cache_key) {
                cached.clone()
            } else {
                let address = AssetAddress::parse(reference).map_err(|error| {
                    format!(
                        "invalid atmospheric cloud model address '{}' for layer '{}': {error}",
                        reference, layer.id
                    )
                })?;
                let model = resources.load::<ModelResource>(&address)?;
                let mesh = model.meshes.get(layer.model_mesh_index).ok_or_else(|| {
                    format!(
                        "atmospheric cloud model '{}' has no mesh index {} (mesh_count={})",
                        model.name,
                        layer.model_mesh_index,
                        model.meshes.len()
                    )
                })?;
                let stream = |semantic: VertexSemantic, label: &str| {
                    mesh.vertex_streams
                        .iter()
                        .find(|stream| stream.semantic == semantic)
                        .ok_or_else(|| format!("cloud mesh '{}' has no {label} stream", mesh.name))
                };
                let position = stream(VertexSemantic::Position, "position")?;
                let normal = stream(VertexSemantic::Normal, "normal")?;
                let tangent = stream(VertexSemantic::Tangent, "tangent")?;
                let color = stream(VertexSemantic::Color(0), "color0")?;
                let uv = stream(VertexSemantic::TexCoord(0), "texcoord0")?;
                let vertex_count = position.vertex_count;
                for candidate in [normal, tangent, color, uv] {
                    if candidate.vertex_count != vertex_count {
                        return Err(format!(
                            "cloud mesh '{}' vertex stream {:?} count={} differs from position count={}",
                            mesh.name, candidate.semantic, candidate.vertex_count, vertex_count
                        ));
                    }
                }
                let vertices = (0..vertex_count as usize)
                    .map(|index| {
                        Ok(AtmosphericCloudVertex {
                            position: read_sky_vec3(position, index)?,
                            normal: read_sky_vec3(normal, index)?,
                            tangent: read_sky_vec4(tangent, index)?,
                            color: read_sky_vec4(color, index)?,
                            uv: read_sky_vec2(uv, index)?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let (index_format, indices) = decode_sky_indices(&mesh.index_buffer)?;
                let resolved = (
                    model.name.clone(),
                    AtmosphericCloudMeshResources {
                        name: mesh.name.clone(),
                        bounds_min: model.bounds.min,
                        bounds_max: model.bounds.max,
                        vertices,
                        indices,
                        index_format,
                    },
                );
                mesh_cache.insert(cache_key, resolved.clone());
                resolved
            }
        } else {
            (
                "newviso.generated.atmospheric_cloud_sheet".to_owned(),
                generated_atmospheric_cloud_sheet(&layer.id),
            )
        };

        let density_ref = layer
            .density_texture
            .as_deref()
            .or(layer.texture.as_deref())
            .unwrap_or("textures/skydome.ytd@baseperlinnoise3channel");
        let density = load_cached_cloud_texture(&assets, &mut texture_cache, density_ref, false)?;

        let normal = if let Some(reference) = layer.normal_texture.as_deref() {
            load_cached_cloud_texture(&assets, &mut texture_cache, reference, false)?
        } else {
            neutral_normal.clone()
        };

        let detail1_present =
            layer.detail_density_texture.is_some() && layer.detail_normal_texture.is_some();
        let detail2_present =
            layer.detail_density2_texture.is_some() && layer.detail_normal2_texture.is_some();

        let detail_density = if let Some(reference) = layer.detail_density_texture.as_deref() {
            load_cached_cloud_texture(&assets, &mut texture_cache, reference, false)?
        } else {
            density.clone()
        };
        let detail_normal = if let Some(reference) = layer.detail_normal_texture.as_deref() {
            load_cached_cloud_texture(&assets, &mut texture_cache, reference, false)?
        } else {
            neutral_normal.clone()
        };
        let detail_density2 = if let Some(reference) = layer.detail_density2_texture.as_deref() {
            load_cached_cloud_texture(&assets, &mut texture_cache, reference, false)?
        } else {
            density.clone()
        };
        let detail_normal2 = if let Some(reference) = layer.detail_normal2_texture.as_deref() {
            load_cached_cloud_texture(&assets, &mut texture_cache, reference, false)?
        } else {
            neutral_normal.clone()
        };

        let uv_layers = layer.uv_layers.map(|uv| AtmosphericCloudUvLayerDesc {
            enabled: uv.enabled,
            mode: match uv.mode {
                ProjectAtmosphericCloudAnimMode::Combine => AtmosphericCloudAnimMode::Combine,
                ProjectAtmosphericCloudAnimMode::Sculpt => AtmosphericCloudAnimMode::Sculpt,
            },
            velocity: uv.velocity,
            scale: uv.scale,
            weight: uv.weight,
        });

        layers.push(AtmosphericCloudLayerResources {
            model_name,
            mesh,
            textures: AtmosphericCloudTextureSet {
                refs: [
                    density_ref.to_owned(),
                    layer
                        .normal_texture
                        .as_deref()
                        .unwrap_or("__neutral_normal")
                        .to_owned(),
                    layer
                        .detail_density_texture
                        .as_deref()
                        .unwrap_or(density_ref)
                        .to_owned(),
                    layer
                        .detail_normal_texture
                        .as_deref()
                        .unwrap_or("__neutral_normal")
                        .to_owned(),
                    layer
                        .detail_density2_texture
                        .as_deref()
                        .unwrap_or(density_ref)
                        .to_owned(),
                    layer
                        .detail_normal2_texture
                        .as_deref()
                        .unwrap_or("__neutral_normal")
                        .to_owned(),
                ],
                density,
                normal,
                detail_density,
                detail_normal,
                detail_density2,
                detail_normal2,
                detail_present: [detail1_present, detail2_present],
            },
            desc: AtmosphericCloudLayerDesc {
                id: layer.id.clone(),
                position: layer.position,
                rotation_degrees: layer.rotation_degrees,
                scale: layer.scale,
                angular_velocity_degrees: layer.angular_velocity_degrees,
                rotation_scale: layer.rotation_scale,
                camera_position_scale: layer.camera_position_scale,
                altitude_min: layer.altitude_min,
                altitude_min_fade: layer.altitude_min_fade,
                altitude_max: layer.altitude_max,
                altitude_max_fade: layer.altitude_max_fade,
                transition_seconds: layer.transition_seconds,
                transition_in_time_percent: layer.transition_in_time_percent,
                transition_out_time_percent: layer.transition_out_time_percent,
                transition_delay_percent: layer.transition_delay_percent,
                transition_midpoint: layer
                    .transition_midpoint
                    .unwrap_or(config.transition_midpoint),
                transition_alpha_range: layer
                    .transition_alpha_range
                    .unwrap_or(config.transition_alpha_range),
                cost_factor: layer.cost_factor,
                soft_intersection_distance: layer.soft_intersection_distance,
                density: layer.density,
                softness: layer.softness,
                opacity: layer.opacity,
                color: layer.color,
                density_shift_scale: layer.density_shift_scale,
                scatter: layer.scatter,
                piercing: layer.piercing,
                scale_diffuse_fill_ambient: layer.scale_diffuse_fill_ambient,
                wrap_lighting: layer.wrap_lighting,
                rescale_uv: layer.rescale_uv,
                layer_anim_scale: layer.layer_anim_scale,
                weather_weights: layer.weather_weights.clone(),
                uv_layers,
            },
        });
    }

    host::info(
        "newviso.scene",
        format!(
            "atmospheric cloud semantic closure ready layers={} unique_meshes={} unique_textures={} enabled={} cloud_hat_speed={:.3}",
            layers.len(),
            mesh_cache.len(),
            texture_cache.len(),
            config.enabled,
            config.cloud_hat_speed
        ),
    );

    Ok(AtmosphericCloudResources {
        enabled: config.enabled,
        cloud_hat_speed: config.cloud_hat_speed,
        wind_min_speed: config.wind_min_speed,
        wind_max_speed: config.wind_max_speed,
        altitude_scroll_scale: config.altitude_scroll_scale,
        global_alpha: config.global_alpha,
        transition_midpoint: config.transition_midpoint,
        transition_alpha_range: config.transition_alpha_range,
        streaming_budget: config.streaming_budget,
        soft_depth_resolution: config.soft_depth_resolution,
        layers,
    })
}

fn load_cached_cloud_texture(
    assets: &AssetClient,
    cache: &mut BTreeMap<String, SkyTextureResources>,
    reference: &str,
    srgb: bool,
) -> Result<SkyTextureResources, String> {
    if let Some(texture) = cache.get(reference) {
        return Ok(texture.clone());
    }
    let texture = load_sky_texture(assets, reference, srgb)?;
    cache.insert(reference.to_owned(), texture.clone());
    Ok(texture)
}

fn generated_atmospheric_cloud_sheet(id: &str) -> AtmosphericCloudMeshResources {
    // The fallback is deliberately a world-horizontal sheet. Cloud curvature
    // belongs to weather-scale density/lighting, not to a camera-centred dome:
    // bending the carrier mesh makes formations visibly climb out of the lower
    // hemisphere as the observer turns.
    const SEGMENTS: u32 = 16;
    let side = SEGMENTS + 1;
    let mut vertices = Vec::with_capacity((side * side) as usize);
    for z in 0..=SEGMENTS {
        let v = z as f32 / SEGMENTS as f32;
        let pz = v * 2.0 - 1.0;
        for x in 0..=SEGMENTS {
            let u = x as f32 / SEGMENTS as f32;
            let px = u * 2.0 - 1.0;
            vertices.push(AtmosphericCloudVertex {
                position: [px, 0.0, pz],
                normal: [0.0, 1.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
                uv: [u, v],
            });
        }
    }

    let mut indices = Vec::with_capacity((SEGMENTS * SEGMENTS * 6) as usize);
    for z in 0..SEGMENTS {
        for x in 0..SEGMENTS {
            let i0 = z * side + x;
            let i1 = i0 + 1;
            let i2 = i0 + side;
            let i3 = i2 + 1;
            indices.extend_from_slice(&[i0, i2, i3, i0, i3, i1]);
        }
    }

    AtmosphericCloudMeshResources {
        name: format!("generated_cloud_sheet_{id}"),
        bounds_min: [-1.0, 0.0, -1.0],
        bounds_max: [1.0, 0.0, 1.0],
        vertices,
        indices,
        index_format: SkyIndexFormat::U32,
    }
}

fn read_sky_vec3(stream: &ModelVertexStream, index: usize) -> Result<[f32; 3], String> {
    if stream.format != ModelVertexFormat::Float32x3 {
        return Err(format!(
            "sky vertex stream {:?} must be Float32x3, actual={:?}",
            stream.semantic, stream.format
        ));
    }
    let bytes = sky_stream_record(stream, index, 12)?;
    Ok([
        f32::from_le_bytes(bytes[0..4].try_into().expect("four bytes")),
        f32::from_le_bytes(bytes[4..8].try_into().expect("four bytes")),
        f32::from_le_bytes(bytes[8..12].try_into().expect("four bytes")),
    ])
}

fn read_sky_vec4(stream: &ModelVertexStream, index: usize) -> Result<[f32; 4], String> {
    if stream.format != ModelVertexFormat::Float32x4 {
        return Err(format!(
            "sky vertex stream {:?} must be Float32x4, actual={:?}",
            stream.semantic, stream.format
        ));
    }
    let bytes = sky_stream_record(stream, index, 16)?;
    Ok([
        f32::from_le_bytes(bytes[0..4].try_into().expect("four bytes")),
        f32::from_le_bytes(bytes[4..8].try_into().expect("four bytes")),
        f32::from_le_bytes(bytes[8..12].try_into().expect("four bytes")),
        f32::from_le_bytes(bytes[12..16].try_into().expect("four bytes")),
    ])
}

fn read_sky_vec2(stream: &ModelVertexStream, index: usize) -> Result<[f32; 2], String> {
    if stream.format != ModelVertexFormat::Float32x2 {
        return Err(format!(
            "sky vertex stream {:?} must be Float32x2, actual={:?}",
            stream.semantic, stream.format
        ));
    }
    let bytes = sky_stream_record(stream, index, 8)?;
    Ok([
        f32::from_le_bytes(bytes[0..4].try_into().expect("four bytes")),
        f32::from_le_bytes(bytes[4..8].try_into().expect("four bytes")),
    ])
}

fn sky_stream_record<'a>(
    stream: &'a ModelVertexStream,
    index: usize,
    record_bytes: usize,
) -> Result<&'a [u8], String> {
    if index >= stream.vertex_count as usize {
        return Err(format!(
            "sky vertex index {index} exceeds stream vertex_count={}",
            stream.vertex_count
        ));
    }
    let stride = usize::try_from(stream.stride)
        .map_err(|_| "sky vertex stream stride exceeds usize".to_owned())?;
    if stride < record_bytes {
        return Err(format!(
            "sky vertex stream {:?} stride={} is smaller than record bytes={record_bytes}",
            stream.semantic, stride
        ));
    }
    let offset = index
        .checked_mul(stride)
        .ok_or_else(|| "sky vertex stream offset overflow".to_owned())?;
    let end = offset
        .checked_add(record_bytes)
        .ok_or_else(|| "sky vertex stream range overflow".to_owned())?;
    stream.data.get(offset..end).ok_or_else(|| {
        format!(
            "sky vertex stream {:?} record[{index}] range={}..{} exceeds bytes={}",
            stream.semantic,
            offset,
            end,
            stream.data.len()
        )
    })
}

fn decode_sky_indices(buffer: &ModelIndexBuffer) -> Result<(SkyIndexFormat, Vec<u32>), String> {
    match buffer.format {
        ModelIndexFormat::U16 => {
            let expected = buffer.index_count as usize * 2;
            if buffer.data.len() < expected {
                return Err(format!(
                    "sky U16 index buffer bytes={} expected={expected}",
                    buffer.data.len()
                ));
            }
            Ok((
                SkyIndexFormat::U16,
                buffer.data[..expected]
                    .chunks_exact(2)
                    .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) as u32)
                    .collect(),
            ))
        }
        ModelIndexFormat::U32 => {
            let expected = buffer.index_count as usize * 4;
            if buffer.data.len() < expected {
                return Err(format!(
                    "sky U32 index buffer bytes={} expected={expected}",
                    buffer.data.len()
                ));
            }
            Ok((
                SkyIndexFormat::U32,
                buffer.data[..expected]
                    .chunks_exact(4)
                    .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four bytes")))
                    .collect(),
            ))
        }
    }
}

fn load_sky_texture(
    assets: &AssetClient,
    reference: &str,
    srgb: bool,
) -> Result<SkyTextureResources, String> {
    let address = AssetAddress::parse(reference)
        .map_err(|error| format!("invalid sky texture address '{reference}': {error}"))?;
    let entry = address
        .entry()
        .ok_or_else(|| format!("sky texture '{reference}' requires @entry"))?;
    let bytes = assets.decode(
        address.logical_path(),
        "texture.rgba8",
        json!({"texture_name": entry}),
    )?;
    if bytes.len() < 20 {
        return Err(format!(
            "sky texture '{reference}' returned short RGBA8 frame bytes={}",
            bytes.len()
        ));
    }
    if &bytes[0..4] != b"NTRT" {
        return Err(format!(
            "sky texture '{reference}' returned invalid RGBA8 magic"
        ));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != 1 {
        return Err(format!(
            "sky texture '{reference}' returned unsupported RGBA8 version {version}"
        ));
    }
    let width = u32::from_le_bytes(bytes[8..12].try_into().expect("four bytes"));
    let height = u32::from_le_bytes(bytes[12..16].try_into().expect("four bytes"));
    let payload_len = u32::from_le_bytes(bytes[16..20].try_into().expect("four bytes")) as usize;
    if bytes.len() != 20 + payload_len {
        return Err(format!(
            "sky texture '{reference}' RGBA8 frame size mismatch bytes={} expected={}",
            bytes.len(),
            20 + payload_len
        ));
    }
    let expected = width as usize * height as usize * 4;
    if payload_len != expected {
        return Err(format!(
            "sky texture '{reference}' RGBA8 payload={} expected={} for {}x{}",
            payload_len, expected, width, height
        ));
    }

    Ok(SkyTextureResources {
        name: entry.to_owned(),
        width,
        height,
        srgb,
        rgba8: bytes[20..].to_vec(),
    })
}

fn command_number(value: &Value, key: &str, index: usize) -> Result<f32, String> {
    let number = value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("script command item[{index}] '{key}' must be numeric"))?;
    let number = number as f32;
    if !number.is_finite() {
        return Err(format!(
            "script command item[{index}] '{key}' must be finite"
        ));
    }
    Ok(number)
}

fn command_f64(value: &Value, key: &str, index: usize) -> Result<f64, String> {
    let number = value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("script command item[{index}] '{key}' must be numeric"))?;
    if !number.is_finite() {
        return Err(format!(
            "script command item[{index}] '{key}' must be finite"
        ));
    }
    Ok(number)
}

fn command_vector<const N: usize>(
    value: &Value,
    key: &str,
    index: usize,
) -> Result<[f32; N], String> {
    let array = value.get(key).and_then(Value::as_array).ok_or_else(|| {
        format!("script command item[{index}] '{key}' must be an array of {N} numbers")
    })?;
    if array.len() != N {
        return Err(format!(
            "script command item[{index}] '{key}' must contain exactly {N} numbers"
        ));
    }
    let mut out = [0.0; N];
    for (slot, item) in out.iter_mut().zip(array) {
        let number = item
            .as_f64()
            .ok_or_else(|| format!("script command item[{index}] '{key}' contains a non-number"))?
            as f32;
        if !number.is_finite() {
            return Err(format!(
                "script command item[{index}] '{key}' contains a non-finite number"
            ));
        }
        *slot = number;
    }
    Ok(out)
}

fn command_vec3(value: &Value, key: &str, index: usize) -> Result<[f32; 3], String> {
    command_vector(value, key, index)
}

fn command_optional_vec3(
    value: &Value,
    key: &str,
    index: usize,
) -> Result<Option<[f32; 3]>, String> {
    if value.get(key).is_none() || value.get(key).is_some_and(Value::is_null) {
        return Ok(None);
    }
    command_vec3(value, key, index).map(Some)
}

fn command_vec4(value: &Value, key: &str, index: usize) -> Result<[f32; 4], String> {
    command_vector(value, key, index)
}

fn command_u32(value: &Value, key: &str, index: usize) -> Result<u32, String> {
    let number = value.get(key).and_then(Value::as_u64).ok_or_else(|| {
        format!("script command item[{index}] '{key}' must be an unsigned integer")
    })?;
    u32::try_from(number)
        .map_err(|_| format!("script command item[{index}] '{key}' is out of u32 range"))
}

fn command_u64(value: &Value, key: &str, index: usize) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("script command item[{index}] '{key}' must be an unsigned integer"))
}

fn command_i32(value: &Value, key: &str, index: usize) -> Result<i32, String> {
    let number = value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("script command item[{index}] '{key}' must be an integer"))?;
    i32::try_from(number)
        .map_err(|_| format!("script command item[{index}] '{key}' is out of i32 range"))
}

fn command_strings(value: &Value, key: &str, index: usize) -> Result<Vec<String>, String> {
    let values = value.get(key).and_then(Value::as_array).ok_or_else(|| {
        format!("script command item[{index}] '{key}' must be an array of strings")
    })?;
    values
        .iter()
        .enumerate()
        .map(|(item_index, item)| {
            item.as_str().map(str::to_owned).ok_or_else(|| {
                format!("script command item[{index}] '{key}[{item_index}]' must be a string")
            })
        })
        .collect()
}

fn command_float_map(
    value: &Value,
    key: &str,
    index: usize,
) -> Result<BTreeMap<String, f32>, String> {
    let Some(map) = value.get(key) else {
        return Ok(BTreeMap::new());
    };
    let map = map.as_object().ok_or_else(|| {
        format!("script command item[{index}] '{key}' must be an object of numeric values")
    })?;
    let mut out = BTreeMap::new();
    for (name, value) in map {
        let number = value
            .as_f64()
            .ok_or_else(|| format!("script command item[{index}] '{key}.{name}' must be numeric"))?
            as f32;
        if !number.is_finite() {
            return Err(format!(
                "script command item[{index}] '{key}.{name}' must be finite"
            ));
        }
        out.insert(name.clone(), number);
    }
    Ok(out)
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
}

struct EngineApplication {
    renderer_path: PathBuf,
    renderer: Option<RunningProvider>,
    scene: Scene3dRuntime,
    settings: ProjectRuntimeSettings,
    living_world: LivingWorldRuntime,
    agents: AgentRuntime,
    physical_characters: application_characters::PhysicalCharacterRuntime,
    vehicles: VehicleRuntime,
    vehicle_presentations: BTreeMap<u64, application_vehicles::VehiclePresentationBinding>,
    vehicle_auto_wheel_layout: BTreeSet<u64>,
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
    exit_requested: bool,
    cursor_captured: bool,
    window_focused: bool,
}

impl EngineApplication {
    fn new(
        renderer_path: PathBuf,
        scene: Scene3dRuntime,
        physics: Option<PhysicsRuntime>,
        scripts: Option<ScriptRuntime>,
        project_context: Value,
        ui_template: Option<Value>,
        content_manager: Option<ContentManager>,
        settings: ProjectRuntimeSettings,
        world_startup: WorldStartup,
    ) -> Result<Self, String> {
        settings.validate().map_err(|error| error.to_string())?;
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
            physical_characters: application_characters::PhysicalCharacterRuntime::default(),
            vehicles: VehicleRuntime::new(),
            vehicle_presentations: BTreeMap::new(),
            vehicle_auto_wheel_layout: BTreeSet::new(),
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
            exit_requested: false,
            cursor_captured: false,
            window_focused: true,
        })
    }

    fn runtime_state(&self) -> Value {
        self.compose_runtime_state(self.living_world.runtime_state())
    }

    fn script_frame_state(&mut self) -> Value {
        let mut runtime_state = self.scene.script_frame_state();
        let root = runtime_state
            .as_object_mut()
            .expect("scene script frame state must be a JSON object");

        let interval = f64::from(self.settings.scripting.world_snapshot_interval_seconds);
        let world_snapshot_fresh = interval <= 0.0
            || self.elapsed_seconds + 1.0e-9 >= self.next_script_world_snapshot_seconds;
        if world_snapshot_fresh {
            self.next_script_world_snapshot_seconds = if interval <= 0.0 {
                self.elapsed_seconds
            } else {
                self.elapsed_seconds + interval
            };
            root.insert("living_world".to_owned(), self.living_world.frame_state());
            root.insert("agents".to_owned(), self.agents.runtime_state());
            root.insert(
                "physical_characters".to_owned(),
                self.physical_characters.runtime_state(),
            );
        }
        // Inventory/pickup interaction is player-input critical and must not be
        // throttled with the large living-world snapshot.
        root.insert("items".to_owned(), self.items.runtime_state());
        root.insert("vehicles".to_owned(), self.vehicles.runtime_state());
        root.insert(
            "vehicle_presentation".to_owned(),
            self.vehicle_presentation_runtime_state(),
        );
        root.insert(
            "world_snapshot_fresh".to_owned(),
            Value::Bool(world_snapshot_fresh),
        );
        root.insert(
            "world_snapshot_seconds".to_owned(),
            json!(self.elapsed_seconds),
        );

        // Player/controller code needs current physics every frame.
        if let Some(physics) = self.physics.as_ref() {
            root.insert("physics".to_owned(), physics.runtime_state());
        }

        let streaming = self.asset_streamer.stats();
        root.insert(
            "asset_streaming".to_owned(),
            json!({
                "frame": streaming.frame,
                "queued": streaming.queued,
                "loading": streaming.loading,
                "waiting_dependencies": streaming.waiting_dependencies,
                "resident": streaming.resident,
                "failed": streaming.failed,
                "resident_bytes": streaming.resident_bytes
            }),
        );
        runtime_state
    }

    fn compose_runtime_state(&self, mut living_world_state: Value) -> Value {
        let actor_views = self.living_world.actor_runtime_views();
        let presentations = self
            .world_actor_presentations
            .iter()
            .map(|(actor_id, binding)| {
                let actor = actor_views.iter().find(|actor| actor.id == *actor_id);
                json!({
                    "actor_id": actor_id,
                    "scene_key": binding.scene_key,
                    "logical_position": actor.map(|actor| actor.position),
                    "representation": actor.map(|actor| actor.representation),
                    "enabled": actor.map(|actor| actor.enabled),
                    "materialized_representations": binding.materialized_representations,
                    "scene_entity": self.scene.runtime_entity_state(&binding.scene_key),
                })
            })
            .collect::<Vec<_>>();

        living_world_state
            .as_object_mut()
            .expect("living world runtime state must be a JSON object")
            .insert("presentations".to_owned(), Value::Array(presentations));

        let mut runtime_state = self.scene.runtime_state();
        let root = runtime_state
            .as_object_mut()
            .expect("scene runtime state must be a JSON object");
        living_world_state["persistence"] = self
            .world_persistence
            .as_ref()
            .map(WorldPersistence::runtime_state)
            .unwrap_or_else(|| json!({"enabled": false, "restored": false}));
        living_world_state["presentation_transitions"] = self.world_presentations_state();
        root.insert(
            "settings".to_owned(),
            serde_json::to_value(&self.settings).expect("validated runtime settings"),
        );
        root.insert("living_world".to_owned(), living_world_state);
        root.insert("agents".to_owned(), self.agents.runtime_state());
        root.insert(
            "physical_characters".to_owned(),
            self.physical_characters.runtime_state(),
        );
        root.insert("items".to_owned(), self.items.runtime_state());
        root.insert("vehicles".to_owned(), self.vehicles.runtime_state());
        root.insert(
            "vehicle_presentation".to_owned(),
            self.vehicle_presentation_runtime_state(),
        );
        if let Some(physics) = self.physics.as_ref() {
            root.insert("physics".to_owned(), physics.runtime_state());
        }
        let streaming = self.asset_streamer.stats();
        root.insert(
            "asset_streaming".to_owned(),
            json!({
                "frame": streaming.frame,
                "entries": streaming.entries,
                "queued": streaming.queued,
                "loading": streaming.loading,
                "waiting_dependencies": streaming.waiting_dependencies,
                "resident": streaming.resident,
                "failed": streaming.failed,
                "resident_sources": streaming.resident_sources,
                "resident_bytes": streaming.resident_bytes,
                "external_claims": streaming.external_claims,
                "dependency_claims": streaming.dependency_claims,
                "total_loads": streaming.total_loads,
                "total_evictions": streaming.total_evictions,
                "total_failures": streaming.total_failures,
                "over_budget": streaming.over_budget
            }),
        );

        let mut gta_character_addresses = self
            .world_actor_presentations
            .values()
            .filter_map(|binding| binding.asset_ref.as_deref())
            .filter(|reference| {
                reference
                    .to_ascii_lowercase()
                    .contains("models/characters/gta/")
            })
            .filter_map(|reference| AssetAddress::parse(reference).ok())
            .collect::<Vec<_>>();
        gta_character_addresses.sort_by(|a, b| a.canonical().cmp(&b.canonical()));
        gta_character_addresses.dedup_by(|a, b| a.canonical() == b.canonical());
        let gta_character_streaming = gta_character_addresses
            .iter()
            .filter_map(|address| {
                let snapshot = self.asset_streamer.snapshot(address)?;
                let dependencies = snapshot
                    .dependencies
                    .iter()
                    .map(|dependency| {
                        let dependency_snapshot = self.asset_streamer.snapshot(dependency);
                        json!({
                            "address": dependency.canonical(),
                            "state": dependency_snapshot
                                .as_ref()
                                .map(|snapshot| format!("{:?}", snapshot.state))
                                .unwrap_or_else(|| "Missing".to_owned()),
                            "priority": dependency_snapshot
                                .as_ref()
                                .map(|snapshot| snapshot.effective_priority),
                            "error": dependency_snapshot
                                .as_ref()
                                .and_then(|snapshot| snapshot.error.clone()),
                        })
                    })
                    .collect::<Vec<_>>();
                Some(json!({
                    "address": snapshot.address.canonical(),
                    "state": format!("{:?}", snapshot.state),
                    "priority": snapshot.effective_priority,
                    "error": snapshot.error,
                    "dependencies": dependencies,
                }))
            })
            .collect::<Vec<_>>();
        root.insert(
            "gta_character_streaming".to_owned(),
            Value::Array(gta_character_streaming),
        );

        let gta_character_aux_streaming = self
            .scene_stream_aux_claims
            .iter()
            .filter_map(|(stable_id, addresses)| {
                let entity = self.scene.entity_state(*stable_id)?;
                let asset_ref = entity
                    .get("asset_ref")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !asset_ref
                    .to_ascii_lowercase()
                    .contains("models/characters/gta/")
                {
                    return None;
                }
                let assets = addresses
                    .iter()
                    .map(|address| {
                        let snapshot = self.asset_streamer.snapshot(address);
                        json!({
                            "address": address.canonical(),
                            "state": snapshot
                                .as_ref()
                                .map(|snapshot| format!("{:?}", snapshot.state))
                                .unwrap_or_else(|| "Missing".to_owned()),
                            "is_texture": self
                                .asset_streamer
                                .get::<newviso_textures::TextureResource>(address)
                                .is_some(),
                            "error": snapshot
                                .as_ref()
                                .and_then(|snapshot| snapshot.error.clone()),
                        })
                    })
                    .collect::<Vec<_>>();
                Some(json!({
                    "entity": stable_id,
                    "scene_residency": entity.get("residency").cloned(),
                    "asset_ref": asset_ref,
                    "assets": assets,
                }))
            })
            .collect::<Vec<_>>();
        root.insert(
            "gta_character_aux_streaming".to_owned(),
            Value::Array(gta_character_aux_streaming),
        );

        let gta_character_scene_interests = self
            .scene
            .streaming_interests()
            .into_iter()
            .filter(|request| {
                request
                    .asset_ref
                    .to_ascii_lowercase()
                    .contains("models/characters/gta/")
            })
            .map(|request| {
                json!({
                    "entity": request.stable_id,
                    "asset_ref": request.asset_ref,
                    "priority": request.priority,
                })
            })
            .collect::<Vec<_>>();
        root.insert(
            "gta_character_scene_interests".to_owned(),
            Value::Array(gta_character_scene_interests),
        );
        runtime_state
    }
}

fn materialize_ui_template(template: &Value, bindings: &BTreeMap<String, Value>) -> Value {
    match template {
        Value::String(value) => Value::String(apply_string_bindings(value, bindings)),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| materialize_ui_template(value, bindings))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), materialize_ui_template(value, bindings)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn apply_string_bindings(source: &str, bindings: &BTreeMap<String, Value>) -> String {
    let mut output = source.to_owned();
    for (key, value) in bindings {
        let token = format!("{{{{{key}}}}}");
        if output.contains(&token) {
            output = output.replace(&token, &binding_text(value));
        }
    }
    output
}

fn binding_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
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
            ("camera.yaw".to_owned(), Value::String("32.10В°".to_owned())),
            ("camera.x".to_owned(), Value::String("4.200".to_owned())),
            ("camera.y".to_owned(), Value::String("3.000".to_owned())),
            ("camera.z".to_owned(), Value::String("6.000".to_owned())),
        ]);

        let materialized = materialize_ui_template(&template, &bindings);
        assert_eq!(materialized["body_lines"][0].as_str(), Some("Yaw: 32.10В°"));
        assert_eq!(
            materialized["body_lines"][1].as_str(),
            Some("Position: 4.200, 3.000, 6.000")
        );
    }
}
