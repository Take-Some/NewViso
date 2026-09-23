use newviso_assets_client::AssetClient;
use newviso_capabilities::{resolve_capabilities, CapabilityNeed, ResolvedCapability};
use newviso_compat_abi::platform::{
    PlatformCursorGrabModeV1, PlatformCursorPollV1, PlatformCursorStateV1,
    PlatformSurfaceMetricsV1, PlatformWindowReadyV1,
};
use newviso_config::ResolvedBootstrapConfig;
use newviso_content_manager::{ContentEffect, ContentManager};
use newviso_core::{EngineState, RuntimePhase};
use newviso_host as host;
use newviso_input_client::InputSnapshot;
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
    Scene3dLoadReport, Scene3dRuntime, SceneLightDesc, SceneLightType, SceneOverlayQuad,
    SceneTransientSphere, SkyDomeResources, SkyIndexFormat, SkyMeshResources, SkyTextureResources,
    SkyVertex,
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

    host::info(
        "newviso.scene",
        format!(
            "FPS sky semantic closure ready model='{}' material='{}' mesh='{}' vertices={} indices={} textures=[{},{},{}]",
            model_name,
            material_name,
            mesh_name,
            vertices.len(),
            indices.len(),
            base_noise.name,
            starfield.name,
            detail_noise.name
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

fn required_vec3(value: &Value, key: &str, context: &str) -> Result<[f32; 3], String> {
    let array = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{context} missing vec3 '{key}'"))?;
    if array.len() != 3 {
        return Err(format!("{context} '{key}' must have 3 values"));
    }
    let mut out = [0.0; 3];
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

fn required_vec2(value: &Value, key: &str, context: &str) -> Result<[f32; 2], String> {
    let array = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{context} missing vec2 '{key}'"))?;
    if array.len() != 2 {
        return Err(format!("{context} '{key}' must have 2 values"));
    }
    let mut out = [0.0; 2];
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

fn builtin_vfs_mount_policy() -> Result<VfsMountPolicy, String> {
    let policy: VfsMountPolicy = serde_json::from_str(BUILTIN_VFS_MOUNTS_JSON)
        .map_err(|error| format!("invalid built-in VFS mount policy: {error}"))?;
    if policy.schema != "newviso.runtime.vfs_mount_policy.v1" {
        return Err(format!(
            "unsupported built-in VFS mount policy schema '{}'",
            policy.schema
        ));
    }
    Ok(policy)
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

    fn publish_content_manager_if_changed(&mut self) -> Result<(), String> {
        let Some(manager) = self.content_manager.as_ref() else {
            return Ok(());
        };

        let surfaces = manager.surfaces();
        if self.last_content_surfaces == surfaces {
            return Ok(());
        }

        let ui = UiClient::new();
        for surface in &surfaces {
            ui.publish_surface(surface)
                .map_err(|error| format!("content manager UI publish failed: {error}"))?;
        }

        host::debug(
            "newviso.ui",
            format!(
                "published content-manager surfaces count={}",
                surfaces.len()
            ),
        );
        self.last_content_surfaces = surfaces;
        Ok(())
    }

    fn apply_content_dispatch(&mut self, dispatch: &Value) -> Result<(), String> {
        let effects = match self.content_manager.as_mut() {
            Some(manager) => manager.apply_dispatch(dispatch)?,
            None => Vec::new(),
        };

        for effect in effects {
            match effect {
                ContentEffect::ScriptChanged {
                    logical_path,
                    bytes: _,
                } => {
                    let reload = self
                        .scripts
                        .as_mut()
                        .ok_or_else(|| "no active scripting runtime".to_owned())
                        .and_then(|scripts| scripts.reload_graph_after_asset_change(&logical_path));

                    match reload {
                        Ok(()) => {
                            host::info(
                                "newviso.content",
                                format!("saved and hot-reloaded script '{logical_path}'"),
                            );
                            if let Some(manager) = self.content_manager.as_mut() {
                                manager
                                    .report_status(format!("Saved + hot reloaded {logical_path}"));
                            }
                        }
                        Err(error) => {
                            host::warn(
                                "newviso.content",
                                format!(
                                    "saved script '{logical_path}' but hot reload failed: {error}"
                                ),
                            );
                            if let Some(manager) = self.content_manager.as_mut() {
                                manager.report_status(format!("Saved, reload failed: {error}"));
                            }
                        }
                    }
                }
            }
        }

        self.publish_content_manager_if_changed()
    }

    fn scene_stream_owner(stable_id: u64) -> StreamingOwnerId {
        StreamingOwnerId::from_label(&format!("newviso.scene.entity.{stable_id}"))
    }

    fn sync_scene_streaming_interests(&mut self) -> Result<(), String> {
        let interests = self.scene.streaming_interests();
        let active_ids = interests
            .iter()
            .map(|request| request.stable_id)
            .collect::<Vec<_>>();

        let stale_ids = self
            .scene_stream_claims
            .keys()
            .copied()
            .filter(|stable_id| !active_ids.contains(stable_id))
            .collect::<Vec<_>>();

        for stable_id in stale_ids {
            if let Some(address) = self.scene_stream_claims.remove(&stable_id) {
                self.asset_streamer
                    .release(Self::scene_stream_owner(stable_id), &address);
                self.scene.mark_entity_unloaded(stable_id)?;
            }
        }

        for request in interests {
            let address = AssetAddress::parse(&request.asset_ref).map_err(|error| {
                format!(
                    "scene entity {} has invalid asset_ref '{}': {error}",
                    request.stable_id, request.asset_ref
                )
            })?;

            if let Some(previous) = self.scene_stream_claims.get(&request.stable_id).cloned() {
                if previous != address {
                    self.asset_streamer
                        .release(Self::scene_stream_owner(request.stable_id), &previous);
                    self.scene.mark_entity_unloaded(request.stable_id)?;
                }
            }

            self.asset_streamer.request(
                Self::scene_stream_owner(request.stable_id),
                address.clone(),
                StreamingClaim::new(request.priority.max(0.0)),
            )?;
            self.scene_stream_claims.insert(request.stable_id, address);
        }

        Ok(())
    }

    fn apply_scene_streaming_residency(&mut self) -> Result<(), String> {
        let claims = self
            .scene_stream_claims
            .iter()
            .map(|(stable_id, address)| (*stable_id, address.clone()))
            .collect::<Vec<_>>();

        for (stable_id, address) in claims {
            if self.asset_streamer.is_resident(&address) {
                self.scene.mark_entity_resident(stable_id)?;
            }
        }
        Ok(())
    }

    fn pump_asset_streaming(&mut self) -> Result<(), String> {
        let report = self.asset_streamer.pump();
        for (address, error) in &report.failed {
            host::warn(
                "newviso.assets.streaming",
                format!("asset='{}' streaming failed: {error}", address.canonical()),
            );
        }

        self.apply_scene_streaming_residency()?;

        if !report.loaded.is_empty()
            || !report.became_resident.is_empty()
            || !report.evicted.is_empty()
        {
            host::debug(
                "newviso.assets.streaming",
                format!(
                    "frame={} loaded={} resident_promotions={} evicted={} source_bytes_loaded={} resident_bytes={} over_budget={}",
                    report.frame,
                    report.loaded.len(),
                    report.became_resident.len(),
                    report.evicted.len(),
                    report.source_bytes_loaded,
                    report.resident_bytes,
                    report.over_budget
                ),
            );
        }
        Ok(())
    }

    fn publish_bound_ui_if_changed(&mut self) -> Result<(), String> {
        let Some(template) = self.ui_template.as_ref() else {
            return Ok(());
        };

        let materialized = materialize_ui_template(template, &self.ui_bindings);
        if self.last_ui_surface.as_ref() == Some(&materialized) {
            return Ok(());
        }

        UiClient::new()
            .publish_surface(&materialized)
            .map_err(|error| format!("project UI surface update failed: {error}"))?;
        self.last_ui_surface = Some(materialized);
        Ok(())
    }

    fn apply_script_commands(&mut self, commands: &[Value]) -> Result<(), String> {
        for (index, command) in commands.iter().enumerate() {
            let op = command
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("script command[{index}] has no string 'op'"))?;

            match op {
                "platform.cursor.set" => {
                    let captured = command
                        .get("captured")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] platform.cursor.set requires 'captured'"
                            )
                        })?;
                    self.cursor_captured = captured && self.window_focused;
                }
                "scene.light.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] scene.light.upsert requires string 'id'")
                    })?;
                    let light_type = match command
                        .get("light_type")
                        .or_else(|| command.get("type"))
                        .and_then(Value::as_str)
                        .unwrap_or("point")
                        .trim()
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "directional" => SceneLightType::Directional,
                        "point" => SceneLightType::Point,
                        "spot" => SceneLightType::Spot,
                        "area" => SceneLightType::Area,
                        other => {
                            return Err(format!(
                                "script command[{index}] unknown light_type '{other}'"
                            ))
                        }
                    };
                    let mut desc = SceneLightDesc::default();
                    desc.light_type = light_type;
                    if command.get("color").is_some() {
                        desc.color = command_vec3(command, "color", index)?;
                    }
                    if command.get("intensity").is_some() {
                        desc.intensity = command_number(command, "intensity", index)?;
                    }
                    if command.get("range").is_some() {
                        desc.range = command_number(command, "range", index)?;
                    }
                    if command.get("cone_inner_degrees").is_some() {
                        desc.cone_inner_degrees =
                            command_number(command, "cone_inner_degrees", index)?;
                    }
                    if command.get("cone_outer_degrees").is_some() {
                        desc.cone_outer_degrees =
                            command_number(command, "cone_outer_degrees", index)?;
                    }
                    if let Some(value) = command.get("casts_shadows") {
                        desc.casts_shadows = value.as_bool().ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.light.upsert 'casts_shadows' must be boolean"
                            )
                        })?;
                    }
                    if command.get("shadow_bias").is_some() {
                        desc.shadow_bias = command_number(command, "shadow_bias", index)?;
                    }
                    if command.get("shadow_normal_bias").is_some() {
                        desc.shadow_normal_bias =
                            command_number(command, "shadow_normal_bias", index)?;
                    }
                    if let Some(value) = command.get("shadow_resolution") {
                        let resolution = value.as_u64().ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.light.upsert 'shadow_resolution' must be unsigned integer"
                            )
                        })?;
                        desc.shadow_resolution = u32::try_from(resolution).map_err(|_| {
                            format!(
                                "script command[{index}] scene.light.upsert shadow_resolution out of range"
                            )
                        })?;
                    }
                    if command.get("shadow_distance").is_some() {
                        desc.shadow_distance = command_number(command, "shadow_distance", index)?;
                    }
                    self.scene.upsert_runtime_light(id, desc)?;
                }
                "scene.entity.transform.set" => {
                    let id = command
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.transform.set requires string 'id'"
                            )
                        })?;
                    let position = command
                        .get("position")
                        .map(|_| command_vec3(command, "position", index))
                        .transpose()?;
                    let rotation = command
                        .get("rotation_degrees")
                        .map(|_| command_vec3(command, "rotation_degrees", index))
                        .transpose()?;
                    let scale = command
                        .get("scale")
                        .map(|_| command_vec3(command, "scale", index))
                        .transpose()?;
                    self.scene
                        .set_runtime_entity_transform(id, position, rotation, scale)?;
                }
                "scene.entity.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] scene.entity.remove requires string 'id'")
                    })?;
                    self.scene.remove_runtime_entity(id)?;
                }
                "scene.camera.set" => {
                    let position = command_vec3(command, "position", index)?;
                    let target = command_vec3(command, "target", index)?;
                    let up = command
                        .get("up")
                        .map(|_| command_vec3(command, "up", index))
                        .transpose()?;
                    let fov = command
                        .get("fov_y_degrees")
                        .map(|value| {
                            value.as_f64().map(|v| v as f32).ok_or_else(|| {
                                format!(
                                    "script command[{index}] scene.camera.set 'fov_y_degrees' must be numeric"
                                )
                            })
                        })
                        .transpose()?;
                    self.scene.set_camera_pose(position, target, up, fov)?;
                }
                "scene.transient_spheres.set" => {
                    let items = command
                        .get("items")
                        .and_then(Value::as_array)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.transient_spheres.set requires array 'items'"
                            )
                        })?;
                    let mut spheres = Vec::with_capacity(items.len());
                    for (item_index, item) in items.iter().enumerate() {
                        spheres.push(SceneTransientSphere {
                            position: command_vec3(item, "position", item_index)?,
                            radius: command_number(item, "radius", item_index)?,
                            color: command_vec4(item, "color", item_index)?,
                        });
                    }
                    self.scene.set_transient_spheres(spheres)?;
                }
                "scene.overlay_quads.set" => {
                    let items = command
                        .get("items")
                        .and_then(Value::as_array)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.overlay_quads.set requires array 'items'"
                            )
                        })?;
                    let mut quads = Vec::with_capacity(items.len());
                    for (item_index, item) in items.iter().enumerate() {
                        quads.push(SceneOverlayQuad {
                            rect: command_vec4(item, "rect", item_index)?,
                            color: command_vec4(item, "color", item_index)?,
                        });
                    }
                    self.scene.set_overlay_quads(quads)?;
                }
                other => {
                    return Err(format!(
                        "script command[{index}] uses unsupported engine command '{other}'"
                    ))
                }
            }
        }
        Ok(())
    }
}

impl PlatformApplication for EngineApplication {
    fn on_window_ready(&mut self, _ready: PlatformWindowReadyV1) -> Result<(), String> {
        let mut renderer = RunningProvider::load_current(&self.renderer_path)?;
        renderer.initialize()?;

        host::info(
            "newviso.runtime",
            format!("renderer '{}' initialized on live window", renderer.id()),
        );

        self.renderer = Some(renderer);

        self.scene.initialize_renderer()?;

        if self.scripts.is_some() {
            let control = {
                let scripts = self
                    .scripts
                    .as_mut()
                    .ok_or_else(|| "script runtime disappeared during start".to_owned())?;
                scripts.start(&self.project_context)?
            };
            self.exit_requested |= control.exit_requested;
            self.ui_bindings.extend(control.ui_bindings);
            self.apply_script_commands(&control.commands)?;
        }

        self.publish_bound_ui_if_changed()?;
        self.publish_content_manager_if_changed()?;

        self.ready = true;

        host::info(
            "newviso.runtime",
            format!("3D scene '{}' initialized", self.scene.title()),
        );
        Ok(())
    }

    fn step(&mut self, dt: f32, surface: PlatformSurfaceMetricsV1) -> Result<bool, String> {
        if !self.ready {
            return Ok(false);
        }

        self.elapsed_seconds += f64::from(dt);

        let input = InputSnapshot::sample()?;
        let surface_size = [surface.width.max(1), surface.height.max(1)];
        let has_ui = self.ui_template.is_some() || self.content_manager.is_some();

        let mut ui_frame = None;
        if has_ui {
            let ui = UiClient::new();
            let dispatch = ui.dispatch_input(
                self.ui_frame_index,
                &input.ui_input_frame(),
                surface_size,
                surface.pixels_per_point,
            )?;
            self.apply_content_dispatch(&dispatch)?;

            ui_frame = Some(ui.frame(
                self.ui_frame_index,
                dt,
                surface_size,
                surface.pixels_per_point,
            )?);
        }

        let camera_navigation_enabled = ui_frame
            .as_ref()
            .and_then(|frame| frame.input_capture.get("camera_navigation_gated"))
            .and_then(Value::as_bool)
            != Some(true);

        if self.scripts.is_some() {
            let runtime_state = self.scene.runtime_state();
            let frame_context = json!({
                "input": {
                    "state": &input.state,
                    "text": &input.text,
                    "ime_commit": &input.ime_commit
                },
                "surface": {
                    "width": surface.width.max(1),
                    "height": surface.height.max(1),
                    "pixels_per_point": surface.pixels_per_point
                },
                "platform": {
                    "focused": self.window_focused,
                    "cursor_captured": self.cursor_captured
                },
                "camera_navigation_enabled": camera_navigation_enabled
            });

            let control = {
                let scripts = self
                    .scripts
                    .as_mut()
                    .ok_or_else(|| "script runtime disappeared".to_owned())?;
                scripts.frame(
                    dt,
                    self.elapsed_seconds,
                    &self.project_context,
                    &runtime_state,
                    &frame_context,
                )?
            };
            self.exit_requested |= control.exit_requested;
            self.ui_bindings.extend(control.ui_bindings);
            self.apply_script_commands(&control.commands)?;
            self.scene.tick(dt)?;
        } else {
            // Native orbit is an engine/editor navigation fallback, not gameplay.
            self.scene
                .update_native_input_from_snapshot(&input, dt, camera_navigation_enabled)?;
        }

        // The scene contributes only generic asset interest. Residency, dependency
        // closure, retry and eviction remain owned by newviso-resource-runtime.
        self.sync_scene_streaming_interests()?;
        self.pump_asset_streaming()?;

        self.publish_bound_ui_if_changed()?;
        self.publish_content_manager_if_changed()?;

        if self.exit_requested {
            return Ok(true);
        }

        if let Some(ui) = ui_frame {
            if self.ui_frame_index == 0 {
                let vertices = ui
                    .draw_list
                    .pointer("/mesh/vertices")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                let indices = ui
                    .draw_list
                    .pointer("/mesh/indices")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                let cmds = ui
                    .draw_list
                    .pointer("/mesh/cmds")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                let textures = ui
                    .draw_list
                    .pointer("/texture_delta/set")
                    .and_then(Value::as_object)
                    .map_or(0, serde_json::Map::len);

                host::info(
                    "newviso.ui",
                    format!(
                        "first UI draw list screen={} ppp={} vertices={vertices} indices={indices} cmds={cmds} textures={textures} first_vertex={} first_cmd={}",
                        ui.draw_list
                            .get("screen_size_px")
                            .cloned()
                            .unwrap_or(Value::Null),
                        ui.draw_list
                            .get("pixels_per_point")
                            .cloned()
                            .unwrap_or(Value::Null),
                        ui.draw_list
                            .pointer("/mesh/vertices/0")
                            .cloned()
                            .unwrap_or(Value::Null),
                        ui.draw_list
                            .pointer("/mesh/cmds/0")
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                );
                if let Some(set) = ui
                    .draw_list
                    .pointer("/texture_delta/set")
                    .and_then(Value::as_object)
                {
                    host::info(
                        "newviso.ui",
                        format!("first UI texture ids={:?}", set.keys().collect::<Vec<_>>()),
                    );

                    if let Some(texture) = set.get("1") {
                        let alpha = texture
                            .get("rgba8")
                            .and_then(Value::as_array)
                            .map(|bytes| {
                                bytes
                                    .iter()
                                    .skip(3)
                                    .step_by(4)
                                    .filter_map(Value::as_u64)
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        let alpha_min = alpha.iter().copied().min().unwrap_or(0);
                        let alpha_max = alpha.iter().copied().max().unwrap_or(0);
                        host::info(
                            "newviso.ui",
                            format!(
                                "first UI atlas size={} alpha_range={}..{}",
                                texture.get("size").cloned().unwrap_or(Value::Null),
                                alpha_min,
                                alpha_max
                            ),
                        );
                    }
                }
            }
            self.ui_frame_index = self.ui_frame_index.wrapping_add(1);
            self.scene
                .render_frame_with_overlay(surface.width, surface.height, move || {
                    RenderClient::new().set_ui_draw_list(ui.draw_list)
                })?;
        } else {
            self.scene.render_frame(surface.width, surface.height)?;
        }

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.update(dt)?;
            renderer.render(dt)?;
        }

        Ok(false)
    }

    fn on_window_focused(&mut self, focused: bool) -> Result<(), String> {
        self.window_focused = focused;
        if !focused {
            self.cursor_captured = false;
        }
        Ok(())
    }

    fn cursor_state(&mut self) -> PlatformCursorPollV1 {
        let captured = self.ready && self.window_focused && self.cursor_captured;
        PlatformCursorPollV1 {
            has_value: true,
            state: PlatformCursorStateV1 {
                visible: !captured,
                grab: if captured {
                    PlatformCursorGrabModeV1::Locked
                } else {
                    PlatformCursorGrabModeV1::None
                },
            },
        }
    }

    fn shutdown(&mut self) {
        self.cursor_captured = false;
        let claims = std::mem::take(&mut self.scene_stream_claims);
        for (stable_id, address) in claims {
            self.asset_streamer
                .release(Self::scene_stream_owner(stable_id), &address);
        }
        let shutdown_control = if let Some(scripts) = self.scripts.as_mut() {
            match scripts.shutdown(&self.project_context) {
                Ok(control) => Some(control),
                Err(error) => {
                    host::warn(
                        "newviso.scripting",
                        format!("script shutdown hook failed: {error}"),
                    );
                    None
                }
            }
        } else {
            None
        };
        if let Some(control) = shutdown_control {
            if let Err(error) = self.apply_script_commands(&control.commands) {
                host::warn(
                    "newviso.scripting",
                    format!("script shutdown commands failed: {error}"),
                );
            }
        }

        host::info(
            "newviso.scene",
            format!("final runtime state: {}", self.scene.runtime_state()),
        );
        self.scene.shutdown_renderer();
        if let Some(mut renderer) = self.renderer.take() {
            renderer.shutdown();
        }
        self.ready = false;
    }
}

pub fn run(bootstrap: ResolvedBootstrapConfig) -> Result<RuntimeReport, String> {
    host::reset();
    host::ensure_asset_types_registry()?;

    let mut state = EngineState::default();
    let mut bootstrap_providers = Vec::<RunningProvider>::new();

    let result = (|| -> Result<RuntimeReport, String> {
        log_bootstrap(&bootstrap);

        let project = bootstrap
            .project_path
            .as_ref()
            .map(ResolvedProject::load)
            .transpose()
            .map_err(|error| format!("project load failed: {error}"))?;

        let effective_cache_dir = project
            .as_ref()
            .map(|project| project.cache_dir.clone())
            .unwrap_or_else(|| bootstrap.cache_dir.clone());

        fs::create_dir_all(&effective_cache_dir).map_err(|error| {
            format!(
                "failed to create effective cache '{}': {error}",
                effective_cache_dir.display()
            )
        })?;

        env::set_var("NEWENGINE_CACHE_FILES", &effective_cache_dir);
        env::set_var("CACHE_FILES", &effective_cache_dir);
        env::set_var("NEWVISO_CACHE_DIR", &effective_cache_dir);

        if let Some(project) = &project {
            host::info(
                "newviso.project",
                format!(
                    "loaded project '{}' id={} version={} root={} manifest={}",
                    project.manifest.project.name,
                    project.manifest.project.id,
                    project.manifest.project.version,
                    project.root.display(),
                    project.manifest_path.display()
                ),
            );
        }

        host::info(
            "newviso.runtime",
            format!("effective cache root={}", effective_cache_dir.display()),
        );

        let roles = ProviderRoles::resolve(&bootstrap, project.as_ref());

        if !bootstrap.base_dir.is_dir() {
            return Err(format!(
                "bootstrap base directory does not exist: {}",
                bootstrap.base_dir.display()
            ));
        }

        env::set_current_dir(&bootstrap.base_dir).map_err(|error| {
            format!(
                "failed to switch process base directory to '{}': {error}",
                bootstrap.base_dir.display()
            )
        })?;

        state.set_phase(RuntimePhase::Boot);

        let providers = probe_directory(&bootstrap.provider_dir)
            .map_err(|error| format!("provider discovery failed: {error}"))?;
        log_provider_inventory(&providers);

        host::debug("newviso.runtime", "probing provider lifecycle ABI");
        for provider in &providers {
            if provider.root_symbol == RootSymbol::Legacy {
                host::warn(
                    "newviso.providers",
                    format!(
                        "legacy provider '{}' version={} root={} deferred to legacy adapter",
                        provider.id,
                        provider.version,
                        provider.root_symbol.label()
                    ),
                );
                continue;
            }

            let report = probe_lifecycle(&provider.path).map_err(|error| {
                format!("provider ABI probe failed for {}: {error}", provider.id)
            })?;

            host::debug(
                "newviso.providers",
                format!(
                    "ABI probe passed provider='{}' version={} capabilities={} defaults={} bytes={} format={}",
                    report.descriptor_id,
                    report.descriptor_version,
                    report.capability_count,
                    report.default_content_type,
                    report.default_bytes,
                    report.default_format_version
                ),
            );
        }

        state.set_phase(RuntimePhase::Engine);

        for id in [
            roles.logging.as_str(),
            roles.input.as_str(),
            roles.assets.as_str(),
            roles.ecs.as_str(),
        ] {
            let info = require_provider(&providers, id)?;
            let mut running = RunningProvider::load_current(&info.path)?;
            running.initialize()?;

            host::info(
                "newviso.providers",
                format!(
                    "started provider '{}' from {}",
                    running.id(),
                    running.path().display()
                ),
            );

            bootstrap_providers.push(running);
        }

        host::debug(
            "newviso.host",
            format!(
                "registry ready services={:?} event_sinks={}",
                host::registered_service_ids(),
                host::event_sink_count()
            ),
        );

        let resolved_capabilities = if let Some(project) = &project {
            mount_project_files(project, &bootstrap.assets_dir)?;
            let resolved = resolve_project_capabilities(project, &providers)?;
            activate_project_capabilities(&resolved, &mut bootstrap_providers)?;
            resolved
        } else {
            Vec::new()
        };

        let loaded_project = if let Some(project) = &project {
            Some(load_project_files(project)?)
        } else {
            None
        };

        let ui_template = loaded_project
            .as_ref()
            .and_then(|files| files.ui_surface.clone());

        let ui_backend_active = resolved_capabilities.iter().any(|item| {
            item.candidate
                .as_ref()
                .and_then(|candidate| candidate.engine_gateway.as_deref())
                == Some("engine.ui")
        });

        let content_manager = if ui_backend_active {
            project
                .as_ref()
                .map(|project| ContentManager::open(&project.root))
                .transpose()?
        } else {
            None
        };

        if content_manager.is_some() {
            host::info("newviso.content", "project content manager activated");
        }

        if let Some(project) = &project {
            if let Some(ui_path) = project.manifest.files.ui.as_deref() {
                host::info(
                    "newviso.ui",
                    format!("loaded project UI template asset='{ui_path}'"),
                );
            }
        }

        log_resolved_capabilities(&resolved_capabilities);

        let mut scripts = if let Some(files) = &loaded_project {
            start_project_scripting(files.scripts.as_ref(), &providers, &mut bootstrap_providers)?
        } else {
            None
        };

        let runtime_settings = loaded_project
            .as_ref()
            .map(|files| files.runtime.clone())
            .unwrap_or_default();
        let environment = loaded_project
            .as_ref()
            .map(|files| files.environment.clone())
            .unwrap_or_default();

        let (mut scene, scene_report) = match &project {
            Some(project) => Scene3dRuntime::load_from_asset(&project.manifest.files.scene)
                .map_err(|error| format!("startup scene load failed: {error}"))?,
            None => Scene3dRuntime::load_first_scene()
                .map_err(|error| format!("3D scene load failed: {error}"))?,
        };

        scene.set_clear_color(environment.clear_color);
        if let Some(sky_config) = environment.sky.as_ref() {
            scene.set_sky_dome(load_environment_sky(sky_config)?)?;
        }
        scene.configure_orbit(
            runtime_settings.camera.rotate_sensitivity,
            runtime_settings.camera.zoom_sensitivity,
            runtime_settings.camera.min_distance,
            runtime_settings.camera.max_distance,
        )?;

        host::info(
            "newviso.scene",
            format!(
                "loaded scene '{}' entities={} camera='{}' mesh='{}'",
                scene_report.title,
                scene_report.entity_count,
                scene_report.camera_name,
                scene_report.mesh_name
            ),
        );

        let project_context = loaded_project
            .as_ref()
            .map(|files| files.context.clone())
            .unwrap_or_else(|| json!({"project": null}));

        let platform_report = if bootstrap.skip_platform {
            host::info(
                "newviso.runtime",
                "platform runtime skipped by configuration",
            );
            None
        } else {
            let platform = require_provider(&providers, &roles.platform)?;
            let renderer = require_provider(&providers, &roles.renderer)?;

            state.set_phase(RuntimePhase::Platform);
            let app = Box::new(EngineApplication::new(
                renderer.path.clone(),
                scene,
                scripts.take(),
                project_context,
                ui_template,
                content_manager,
                streaming_policy_from_project(&runtime_settings.streaming),
            )?);
            state.set_phase(RuntimePhase::Running);

            let report = run_platform(
                &platform.path,
                PlatformRunConfig {
                    title: runtime_settings
                        .window
                        .title
                        .clone()
                        .or_else(|| {
                            project
                                .as_ref()
                                .map(|project| project.manifest.project.name.clone())
                        })
                        .unwrap_or_else(|| "NewViso".to_owned()),
                    width: runtime_settings.window.width,
                    height: runtime_settings.window.height,
                    frame_limit: bootstrap.max_frames,
                },
                app,
            )?;

            if !report.window_ready {
                return Err("platform runtime exited before window_ready".to_owned());
            }
            if let Some(error) = &report.app_error {
                return Err(format!("runtime application failed: {error}"));
            }

            host::info(
                "newviso.runtime",
                format!(
                    "platform stopped frames={} events={} backend={:?} surface={}x{}",
                    report.frames,
                    report.emitted_events,
                    report.backend,
                    report.width,
                    report.height
                ),
            );

            Some(report)
        };

        Ok(RuntimeReport {
            provider_count: providers.len(),
            scene: scene_report,
            platform: platform_report,
        })
    })();

    if let Err(error) = &result {
        host::error("newviso.runtime", error.clone());
    }

    state.set_phase(RuntimePhase::Shutdown);
    host::info("newviso.runtime", "shutdown");

    for provider in bootstrap_providers.iter_mut().rev() {
        provider.shutdown();
    }

    result
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

fn resolve_project_capabilities(
    project: &ResolvedProject,
    providers: &[ProviderInfo],
) -> Result<Vec<ResolvedCapability>, String> {
    let required = project
        .manifest
        .capabilities
        .required
        .iter()
        .map(|request| CapabilityNeed {
            id: request.id.clone(),
            min_version: request.min_version,
            provider: request.provider.clone(),
            required: true,
        });
    let optional = project
        .manifest
        .capabilities
        .optional
        .iter()
        .map(|request| CapabilityNeed {
            id: request.id.clone(),
            min_version: request.min_version,
            provider: request.provider.clone(),
            required: false,
        });

    resolve_capabilities(providers, required.chain(optional))
}

fn activate_project_capabilities(
    resolved: &[ResolvedCapability],
    running_providers: &mut Vec<RunningProvider>,
) -> Result<(), String> {
    for item in resolved {
        let Some(candidate) = &item.candidate else {
            host::warn(
                "newviso.capabilities",
                format!(
                    "optional capability '{}' version>={} unavailable",
                    item.need.id, item.need.min_version
                ),
            );
            continue;
        };

        if candidate.bootstrap_phase == BootstrapPhase::Platform
            || candidate.engine_gateway.as_deref() == Some("engine.platform")
            || candidate.engine_gateway.as_deref() == Some("engine.render")
        {
            host::info(
                "newviso.capabilities",
                format!(
                    "capability '{}' resolved provider='{}' gateway={} phase={} activation=deferred",
                    candidate.capability_id,
                    candidate.provider_id,
                    candidate.engine_gateway.as_deref().unwrap_or("<none>"),
                    candidate.bootstrap_phase.label()
                ),
            );
            continue;
        }

        let already_running = running_providers
            .iter()
            .any(|provider| provider.id() == candidate.provider_id);

        if !already_running {
            let mut running = RunningProvider::load_current(&candidate.provider_path)?;
            running.initialize()?;
            host::info(
                "newviso.capabilities",
                format!(
                    "activated capability '{}' provider='{}' version={} priority={} path={}",
                    candidate.capability_id,
                    running.id(),
                    candidate.version,
                    candidate.priority,
                    candidate.provider_path.display()
                ),
            );
            running_providers.push(running);
        }

        if let (Some(gateway), Some(service_id)) =
            (&candidate.engine_gateway, &candidate.service_id)
        {
            let registered = host::registered_service_ids()
                .iter()
                .any(|registered| registered == service_id);
            if !registered {
                let message = format!(
                    "capability '{}' provider='{}' did not register descriptor service '{}'",
                    candidate.capability_id, candidate.provider_id, service_id
                );
                if item.need.required {
                    return Err(message);
                }
                host::warn("newviso.capabilities", message);
                continue;
            }

            host::add_alias(gateway.clone(), service_id.clone());
            host::info(
                "newviso.capabilities",
                format!(
                    "bound capability '{}' gateway='{}' -> service='{}' provider='{}'",
                    candidate.capability_id, gateway, service_id, candidate.provider_id
                ),
            );
        }
    }

    Ok(())
}

fn log_resolved_capabilities(resolved: &[ResolvedCapability]) {
    for item in resolved {
        match &item.candidate {
            Some(candidate) => host::info(
                "newviso.capabilities",
                format!(
                    "resolved {} capability='{}' min_version={} provider='{}' version={} priority={} gateway={} service={}",
                    if item.need.required { "required" } else { "optional" },
                    item.need.id,
                    item.need.min_version,
                    candidate.provider_id,
                    candidate.version,
                    candidate.priority,
                    candidate.engine_gateway.as_deref().unwrap_or("<none>"),
                    candidate.service_id.as_deref().unwrap_or("<none>")
                ),
            ),
            None => host::warn(
                "newviso.capabilities",
                format!(
                    "unresolved optional capability='{}' min_version={}",
                    item.need.id, item.need.min_version
                ),
            ),
        }
    }
}

fn mount_project_files(project: &ResolvedProject, shared_assets_dir: &Path) -> Result<(), String> {
    let assets = AssetClient::new();
    let policy = builtin_vfs_mount_policy()?;

    if shared_assets_dir.is_dir() {
        assets.mount_filesystem(
            shared_assets_dir,
            &policy.shared_assets.mount,
            policy.shared_assets.priority,
        )?;
        host::info(
            "newviso.project",
            format!(
                "mounted engine Shared Assets at VFS root path={} priority={}",
                shared_assets_dir.display(),
                policy.shared_assets.priority
            ),
        );
    } else {
        host::warn(
            "newviso.project",
            format!(
                "engine Shared Assets directory is missing path={}",
                shared_assets_dir.display()
            ),
        );
    }

    assets.mount_filesystem(
        &project.root,
        &policy.project_root.mount,
        policy.project_root.priority,
    )?;
    host::info(
        "newviso.project",
        format!(
            "mounted project root at VFS root path={} priority={}",
            project.root.display(),
            policy.project_root.priority
        ),
    );

    if project.assets_dir.is_dir() {
        assets.mount_filesystem(
            &project.assets_dir,
            &policy.project_assets.mount,
            policy.project_assets.priority,
        )?;
        host::info(
            "newviso.project",
            format!(
                "mounted project assets overlay at VFS root path={} priority={}",
                project.assets_dir.display(),
                policy.project_assets.priority
            ),
        );
    }

    Ok(())
}

fn load_project_files(project: &ResolvedProject) -> Result<LoadedProjectFiles, String> {
    let assets = AssetClient::new();

    let runtime = ProjectRuntimeSettings::from_value(
        assets
            .json(&project.manifest.files.runtime)
            .map_err(|error| format!("runtime settings asset load failed: {error}"))?,
    )
    .map_err(|error| error.to_string())?;

    let environment = ProjectEnvironment::from_value(
        assets
            .json(&project.manifest.files.environment)
            .map_err(|error| format!("environment asset load failed: {error}"))?,
    )
    .map_err(|error| error.to_string())?;

    let scripts = if let Some(manifest_scripts) = project.manifest.scripts.as_ref() {
        Some(manifest_scripts.as_runtime_config())
    } else {
        project
            .manifest
            .files
            .scripts
            .as_deref()
            .map(|logical_path| {
                assets
                    .json(logical_path)
                    .map_err(|error| format!("legacy scripts config asset load failed: {error}"))
                    .and_then(|value| {
                        ProjectScripts::from_value(value).map_err(|error| error.to_string())
                    })
            })
            .transpose()?
    };

    let ui_surface = project
        .manifest
        .files
        .ui
        .as_deref()
        .map(|logical_path| {
            assets
                .decode_json(logical_path, "ui.surface.v1", Value::Null)
                .map_err(|error| format!("UI semantic asset load failed: {error}"))
        })
        .transpose()?;

    let context = json!({
        "identity": {
            "id": project.manifest.project.id,
            "name": project.manifest.project.name,
            "version": project.manifest.project.version,
        },
        "files": project.manifest.files,
        "scripts": project.manifest.scripts,
        "runtime": runtime,
        "environment": environment,
        "capabilities": project.manifest.capabilities,
    });

    host::info(
        "newviso.project",
        format!(
            "project files loaded runtime='{}' environment='{}' scene='{}' scripts={} ui={}",
            project.manifest.files.runtime,
            project.manifest.files.environment,
            project.manifest.files.scene,
            project
                .manifest
                .scripts
                .as_ref()
                .map(|scripts| scripts.entrypoint.as_str())
                .or_else(|| project.manifest.files.scripts.as_deref())
                .unwrap_or("<none>"),
            project.manifest.files.ui.as_deref().unwrap_or("<none>")
        ),
    );

    Ok(LoadedProjectFiles {
        runtime,
        environment,
        scripts,
        ui_surface,
        context,
    })
}

fn start_project_scripting(
    scripts: Option<&ProjectScripts>,
    providers: &[ProviderInfo],
    running_providers: &mut Vec<RunningProvider>,
) -> Result<Option<ScriptRuntime>, String> {
    let Some(scripts) = scripts else {
        return Ok(None);
    };
    if !scripts.enabled || scripts.modules.is_empty() {
        host::info("newviso.scripting", "project scripting disabled or empty");
        return Ok(None);
    }

    let provider = require_provider(providers, &scripts.provider).map_err(|_| {
        format!(
            "project requires scripting provider '{}' but its DLL is not deployed",
            scripts.provider
        )
    })?;

    let mut running = RunningProvider::load_current(&provider.path)?;
    running.initialize()?;
    host::info(
        "newviso.scripting",
        format!(
            "started scripting provider '{}' from {}",
            running.id(),
            running.path().display()
        ),
    );
    running_providers.push(running);

    let modules = scripts
        .modules
        .iter()
        .map(|module| ScriptModuleSpec {
            asset: module.asset.clone(),
            on_start: module.on_start.clone(),
            on_frame: module.on_frame.clone(),
            on_shutdown: module.on_shutdown.clone(),
            permissions: module
                .permissions
                .iter()
                .map(|permission| ScriptPermission {
                    id: permission.id.clone(),
                    scope: permission.scope.clone(),
                })
                .collect(),
        })
        .collect();

    let runtime = ScriptRuntime::load(modules)?;
    host::info(
        "newviso.scripting",
        format!(
            "loaded project script entrypoint='{}'",
            scripts
                .modules
                .first()
                .map(|module| module.asset.as_str())
                .unwrap_or("<none>")
        ),
    );
    Ok(Some(runtime))
}

fn require_provider<'a>(
    providers: &'a [ProviderInfo],
    id: &str,
) -> Result<&'a ProviderInfo, String> {
    providers
        .iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| format!("required provider '{id}' is missing"))
}

fn log_provider_inventory(providers: &[ProviderInfo]) {
    host::info(
        "newviso.providers",
        format!("discovered {} providers", providers.len()),
    );

    for provider in providers {
        host::debug(
            "newviso.providers",
            format!(
                "provider='{}' version={} kind={} phase={} root={} descriptor_v2={} path={}",
                provider.id,
                provider.version,
                provider.kind.label(),
                provider.bootstrap_phase.label(),
                provider.root_symbol.label(),
                provider.has_descriptor_v2,
                provider.path.display()
            ),
        );
    }
}

fn log_bootstrap(bootstrap: &ResolvedBootstrapConfig) {
    let config_source = if bootstrap.config_loaded {
        bootstrap.config_path.display().to_string()
    } else {
        format!("defaults ({})", bootstrap.config_path.display())
    };

    host::info(
        "newviso.bootstrap",
        format!(
            "starting config={} base={} providers={}",
            config_source,
            bootstrap.base_dir.display(),
            bootstrap.provider_dir.display()
        ),
    );

    if let Some(project_path) = &bootstrap.project_path {
        host::info(
            "newviso.bootstrap",
            format!("project request={}", project_path.display()),
        );
    }

    if let Some(max_frames) = bootstrap.max_frames {
        host::warn(
            "newviso.bootstrap",
            format!("runtime frame limit enabled explicitly: {max_frames}"),
        );
    }

    for item in &bootstrap.cli_overrides {
        host::debug("newviso.bootstrap", format!("CLI override {item}"));
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
