use newviso_assets_client::AssetClient;
use newviso_bugtrap as bugtrap;
use newviso_capabilities::{resolve_capabilities, CapabilityNeed, ResolvedCapability};
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
use newviso_physics_client::{
    CollisionShape, PhysicsBodyFlags, PhysicsBodyKind, PhysicsBodySnapshot, PhysicsClient,
    PhysicsCommand, PhysicsCommandKind, PhysicsFeature, PhysicsFrameInput, PhysicsFrameOutput,
    PhysicsMaterial,
};
use newviso_platform::{run_platform, PlatformApplication, PlatformRunConfig, PlatformRunReport};
use newviso_project::{
    ProjectEnvironment, ProjectRuntimeSettings, ProjectScripts, ProjectSkyEnvironment,
    ProjectStreamingSettings, ResolvedProject,
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
    LensFlareDesc, LensFlareElementDesc, LensFlareElementKind, Scene3dLoadReport, Scene3dRuntime,
    SceneLightDesc, SceneLightType, SceneOverlayQuad, SceneTransientSphere, SkyDomeResources,
    SkyIndexFormat, SkyMeshResources, SkyTextureResources, SkyVertex, SkyVisualDesc, SkyVisualKind,
};
use newviso_scripting::{ScriptModuleSpec, ScriptPermission, ScriptRuntime};
use newviso_ui_client::UiClient;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

mod application_content;
mod application_events;
mod application_physics;
use application_physics::PhysicsRuntime;
mod application_platform;
mod application_script_commands;
mod application_streaming;
mod bootstrap;
mod bootstrap_support;
pub use bootstrap::run;

pub fn builtin_resource_manager() -> ResourceManager<AssetClientSource> {
    ResourceManager::new(AssetClientSource)
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
    let model_entry = model_address
        .entry()
        .ok_or_else(|| "environment sky model requires @entry".to_owned())?;
    let model = assets.decode_json(
        model_address.logical_path(),
        "model.runtime_json_v1",
        json!({"entry": model_entry}),
    )?;
    if model.get("schema").and_then(Value::as_str) != Some("engine.model.runtime_json.v1") {
        return Err(format!(
            "sky model semantic output has unexpected schema: {model}"
        ));
    }

    let model_name = required_string(&model, "name", "sky model")?;
    let bounds = model
        .get("bounds")
        .ok_or_else(|| "sky model semantic output has no bounds".to_owned())?;
    let bounds_min = required_vec3(bounds, "aabb_min", "sky model bounds")?;
    let bounds_max = required_vec3(bounds, "aabb_max", "sky model bounds")?;

    let mesh = model
        .get("meshes")
        .and_then(Value::as_array)
        .and_then(|meshes| meshes.first())
        .ok_or_else(|| "sky model semantic output has no meshes".to_owned())?;
    let mesh_name = required_string(mesh, "name", "sky mesh")?;
    let material_slot = required_string(mesh, "material_slot", "sky mesh")?;
    let index_format = match required_string(mesh, "index_format", "sky mesh")?.as_str() {
        "u16" | "U16" => SkyIndexFormat::U16,
        "u32" | "U32" => SkyIndexFormat::U32,
        other => return Err(format!("sky mesh unsupported index format '{other}'")),
    };

    let vertices = mesh
        .get("vertices")
        .and_then(Value::as_array)
        .ok_or_else(|| "sky mesh has no vertices".to_owned())?
        .iter()
        .map(|vertex| {
            Ok(SkyVertex {
                position: required_vec3(vertex, "pos", "sky vertex")?,
                uv: required_vec2(vertex, "uv", "sky vertex")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let indices = mesh
        .get("indices")
        .and_then(Value::as_array)
        .ok_or_else(|| "sky mesh has no indices".to_owned())?
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| format!("sky index[{index}] is not a valid u32"))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let material_ref = model
        .get("material_slots")
        .and_then(Value::as_array)
        .and_then(|slots| {
            slots.iter().find(|slot| {
                slot.get("slot_name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name == material_slot)
            })
        })
        .or_else(|| {
            model
                .get("material_slots")
                .and_then(Value::as_array)
                .and_then(|slots| slots.first())
        })
        .and_then(|slot| slot.get("material_ref"))
        .and_then(Value::as_str)
        .filter(|reference| !reference.trim().is_empty())
        .ok_or_else(|| format!("sky mesh material slot '{material_slot}' has no material_ref"))?;

    let material_address = AssetAddress::parse(material_ref)
        .map_err(|error| format!("invalid sky material address '{material_ref}': {error}"))?;
    let material_entry = material_address
        .entry()
        .ok_or_else(|| format!("sky material '{material_ref}' requires @entry"))?;
    let material = assets.decode_json(
        material_address.logical_path(),
        "material.runtime_json_v1",
        json!({"material": material_entry}),
    )?;
    if material.get("schema").and_then(Value::as_str) != Some("engine.material.runtime_json.v1") {
        return Err(format!(
            "sky material semantic output has unexpected schema: {material}"
        ));
    }
    let material_name = required_string(&material, "name", "sky material")?;
    let texture_ref = |slot: &str| -> Result<String, String> {
        material
            .get("textures")
            .and_then(Value::as_array)
            .and_then(|textures| {
                textures.iter().find(|binding| {
                    binding
                        .get("slot")
                        .and_then(Value::as_str)
                        .is_some_and(|name| name == slot)
                })
            })
            .and_then(|binding| binding.get("ref"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("sky material '{material_name}' has no texture slot '{slot}'"))
    };

    let base_noise = load_sky_texture(&assets, &texture_ref("base_color")?, false)?;
    let starfield = load_sky_texture(&assets, &texture_ref("emissive")?, true)?;
    let detail_noise = load_sky_texture(&assets, &texture_ref("normal")?, false)?;
    let billboard_texture = config
        .billboard_texture
        .as_deref()
        .map(|reference| load_sky_texture(&assets, reference, true))
        .transpose()?;

    host::info(
        "newviso.scene",
        format!(
            "sky semantic closure ready model='{}' material='{}' mesh='{}' vertices={} indices={} textures=[{},{},{}] billboard={}",
            model_name,
            material_name,
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
            bounds_min,
            bounds_max,
            vertices,
            indices,
            index_format,
        },
        base_noise,
        starfield,
        detail_noise,
        billboard_texture,
    })
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

fn command_vec4(value: &Value, key: &str, index: usize) -> Result<[f32; 4], String> {
    command_vector(value, key, index)
}

fn required_string(value: &Value, key: &str, context: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{context} missing string '{key}'"))
}

fn required_vector<const N: usize>(
    value: &Value,
    key: &str,
    context: &str,
) -> Result<[f32; N], String> {
    let array = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{context} missing vec{N} '{key}'"))?;
    if array.len() != N {
        return Err(format!("{context} '{key}' must have {N} values"));
    }

    let mut out = [0.0; N];
    for (index, item) in array.iter().enumerate() {
        out[index] = item
            .as_f64()
            .ok_or_else(|| format!("{context} '{key}[{index}]' must be numeric"))?
            as f32;
        if !out[index].is_finite() {
            return Err(format!("{context} '{key}[{index}]' must be finite"));
        }
    }
    Ok(out)
}

fn required_vec3(value: &Value, key: &str, context: &str) -> Result<[f32; 3], String> {
    required_vector(value, key, context)
}

fn required_vec2(value: &Value, key: &str, context: &str) -> Result<[f32; 2], String> {
    required_vector(value, key, context)
}

pub struct RuntimeReport {
    pub provider_count: usize,
    pub scene: Scene3dLoadReport,
    pub platform: Option<PlatformRunReport>,
}

const BUILTIN_VFS_MOUNTS_JSON: &str = include_str!("assets/vfs_mounts.json");

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
    scripts: Option<ProjectScripts>,
    ui_surface: Option<Value>,
    context: Value,
}

struct EngineApplication {
    renderer_path: PathBuf,
    renderer: Option<RunningProvider>,
    scene: Scene3dRuntime,
    physics: Option<PhysicsRuntime>,
    scripts: Option<ScriptRuntime>,
    project_context: Value,
    ui_template: Option<Value>,
    ui_bindings: BTreeMap<String, Value>,
    last_ui_surface: Option<Value>,
    content_manager: Option<ContentManager>,
    asset_streamer: AssetStreamer<AssetClientSource>,
    scene_stream_claims: BTreeMap<u64, AssetAddress>,
    last_content_surfaces: Vec<Value>,
    ui_frame_index: u64,
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
        streaming_policy: StreamingPolicy,
    ) -> Result<Self, String> {
        Ok(Self {
            renderer_path,
            renderer: None,
            scene,
            physics,
            scripts,
            project_context,
            ui_template,
            ui_bindings: BTreeMap::new(),
            last_ui_surface: None,
            content_manager,
            asset_streamer: builtin_asset_streamer(streaming_policy)?,
            scene_stream_claims: BTreeMap::new(),
            last_content_surfaces: Vec::new(),
            ui_frame_index: 0,
            elapsed_seconds: 0.0,
            ready: false,
            exit_requested: false,
            cursor_captured: false,
            window_focused: true,
        })
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
    fn ui_template_materializes_script_bindings() {
        let template = json!({
            "body_lines": [
                "Yaw: {{camera.yaw}}",
                "Position: {{camera.x}}, {{camera.y}}, {{camera.z}}"
            ]
        });
        let bindings = BTreeMap::from([
            ("camera.yaw".to_owned(), Value::String("32.10°".to_owned())),
            ("camera.x".to_owned(), Value::String("4.200".to_owned())),
            ("camera.y".to_owned(), Value::String("3.000".to_owned())),
            ("camera.z".to_owned(), Value::String("6.000".to_owned())),
        ]);

        let materialized = materialize_ui_template(&template, &bindings);
        assert_eq!(materialized["body_lines"][0].as_str(), Some("Yaw: 32.10°"));
        assert_eq!(
            materialized["body_lines"][1].as_str(),
            Some("Position: 4.200, 3.000, 6.000")
        );
    }
}
