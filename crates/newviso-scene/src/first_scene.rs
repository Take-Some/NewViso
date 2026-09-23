use crate::{
    camera::{Camera, OrbitCamera},
    math::{transform_point, Vec3},
    world::{
        LightComponent, LightType, SceneBounds, SceneEntity, SceneEntityId, SceneEntityKind,
        SceneFocusSource, SceneFramePlan, SceneLifecycle, SceneLodPolicy, SceneMobility,
        SceneResidency, SceneTransform, SceneView, SceneWorld, VisibilityMask, VisibilityModule,
    },
};
use newviso_host as host_runtime;
use newviso_input_client::InputSnapshot;
use newviso_render_client::{
    GraphicsPipelineDesc, RenderClient, ShaderStage, TextureMipUpload, VertexAttribute,
    VertexFormat,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[path = "geometry.rs"]
mod geometry;

const SCENE_SERVICE: &str = "engine.scene";
const ASSET_SERVICE: &str = "engine.assets";
const ECS_SERVICE: &str = "engine.ecs";
const PRIMARY_MOUSE_BUTTON: u64 = 1;
const BUILTIN_FIRST_SCENE_JSON: &str = include_str!("assets/first_scene.json");

const VERTEX_SHADER: &[u8] = include_bytes!("assets/scene.vert.spv");
const FRAGMENT_SHADER: &[u8] = include_bytes!("assets/scene.frag.spv");
const SKY_VERTEX_SHADER: &[u8] = include_bytes!("assets/sky.vert.spv");
const SKY_FRAGMENT_SHADER: &[u8] = include_bytes!("assets/sky.frag.spv");
const SHADOW_VERTEX_SHADER: &[u8] = include_bytes!("assets/shadow.vert.spv");
const SHADOW_FRAGMENT_SHADER: &[u8] = include_bytes!("assets/shadow.frag.spv");
const SKY_FLOATS_PER_VERTEX: usize = 5;
const SKY_VERTEX_STRIDE: u64 = (SKY_FLOATS_PER_VERTEX * std::mem::size_of::<f32>()) as u64;

const CUBE_VERTEX_COUNT: u32 = 36;
const FLOATS_PER_VERTEX: usize = 11;
const MAX_LIGHTS: usize = 4;
const SCENE_FRAME_UNIFORM_FLOATS: usize = 120;
const DEFAULT_SHADOW_RESOLUTION: u32 = 2048;
const MAX_TRANSIENT_SPHERES: usize = 256;
const MAX_OVERLAY_QUADS: usize = 256;
const VERTEX_STRIDE: u64 = (FLOATS_PER_VERTEX * std::mem::size_of::<f32>()) as u64;

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
    pub radius: f32,
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct SceneOverlayQuad {
    pub rect: [f32; 4],
    pub color: [f32; 4],
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
            intensity: 1.0,
            range: 10.0,
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

#[derive(Clone, Copy, Debug)]
struct GpuScene {
    vertex_buffer: u32,
    shadow_vertex_buffer: u32,
    frame_uniform: u32,
    bind_group_layout: u32,
    bind_group: u32,
    shadow_bind_group_layout: u32,
    shadow_bind_group: u32,
    shadow_render_target: u32,
    shadow_sampler: u32,
    vertex_shader: u32,
    fragment_shader: u32,
    pipeline: u32,
    shadow_vertex_shader: u32,
    shadow_fragment_shader: u32,
    shadow_pipeline: u32,
    shadow_resolution: u32,
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
}

#[derive(Clone, Copy, Debug)]
struct GpuSky {
    vertex_buffer: u32,
    index_buffer: u32,
    base_noise: u32,
    starfield: u32,
    detail_noise: u32,
    sampler: u32,
    bind_group_layout: u32,
    bind_group: u32,
    camera_uniform: u32,
    vertex_shader: u32,
    fragment_shader: u32,
    pipeline: u32,
    index_count: u32,
    index_format: &'static str,
}

#[derive(Debug)]
pub struct Scene3dRuntime {
    title: String,
    camera_entity_id: u64,
    camera: Camera,
    orbit: OrbitCamera,
    cubes: Vec<Cube>,
    transient_spheres: Vec<SceneTransientSphere>,
    overlay_quads: Vec<SceneOverlayQuad>,
    runtime_entity_ids: BTreeMap<String, u64>,
    next_runtime_entity_id: u64,
    world: SceneWorld,
    frame_plan: SceneFramePlan,
    clear_color: [f32; 4],
    gpu: Option<GpuScene>,
    sky: Option<SkyDomeResources>,
    gpu_sky: Option<GpuSky>,
    frame_index: u64,
}

#[derive(Clone, Debug)]
pub struct Scene3dLoadReport {
    pub title: String,
    pub entity_count: usize,
    pub camera_name: String,
    pub mesh_name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneStreamRequest {
    pub stable_id: u64,
    pub asset_ref: String,
    pub priority: f32,
}

impl Scene3dRuntime {
    pub fn load_first_scene() -> Result<(Self, Scene3dLoadReport), String> {
        let scene: Value = serde_json::from_str(BUILTIN_FIRST_SCENE_JSON)
            .map_err(|error| format!("invalid built-in first_scene.json: {error}"))?;
        Self::load_scene_value(scene)
    }

    pub fn load_from_asset(logical_path: &str) -> Result<(Self, Scene3dLoadReport), String> {
        let logical_path = logical_path.trim().replace('\\', "/");
        let logical_path = logical_path.trim_start_matches('/');
        if logical_path.is_empty() {
            return Err("startup scene logical path is empty".to_owned());
        }

        let bytes =
            host_runtime::call_service(ASSET_SERVICE, "asset.text_v1", logical_path.as_bytes())
                .map_err(|error| format!("failed to load scene asset '{logical_path}': {error}"))?;

        let scene: Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid scene asset '{logical_path}': {error}"))?;

        Self::load_scene_value(scene)
    }

    fn load_scene_value(scene: Value) -> Result<(Self, Scene3dLoadReport), String> {
        let load = host_runtime::call_json(
            SCENE_SERVICE,
            "scene.load_json_v1",
            &json!({
                "replace": true,
                "scene": scene
            }),
        )?;

        if load.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(format!("Flecs rejected NewViso scene: {load}"));
        }

        let save = host_runtime::call_json(
            SCENE_SERVICE,
            "scene.save_json_v1",
            &json!({
                "path": "",
                "pretty": false
            }),
        )?;

        let snapshot = save
            .get("payload")
            .ok_or_else(|| format!("Flecs scene snapshot has no payload: {save}"))?;
        let entities = snapshot
            .get("entities")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("Flecs scene snapshot has no entities: {snapshot}"))?;

        let mut camera_record: Option<&Value> = None;
        let mut camera_entity_id: Option<u64> = None;
        let mut mesh_records: Vec<(u64, &Value)> = Vec::new();
        let mut logic_records: Vec<(u64, &Value)> = Vec::new();

        for (snapshot_index, entity) in entities.iter().enumerate() {
            let Some(record) = entity
                .get("components")
                .and_then(|components| components.get("newengine.scene.entity"))
            else {
                continue;
            };

            let stable_id = entity
                .get("handle")
                .and_then(|handle| handle.get("stable_id"))
                .and_then(Value::as_u64)
                .unwrap_or((snapshot_index + 1) as u64);

            match record.get("kind").and_then(Value::as_str) {
                Some("camera") if camera_record.is_none() => {
                    camera_record = Some(record);
                    camera_entity_id = Some(stable_id);
                }
                Some("mesh") => mesh_records.push((stable_id, record)),
                Some(_) => logic_records.push((stable_id, record)),
                None => {}
            }
        }

        let camera_record =
            camera_record.ok_or_else(|| "NewViso scene snapshot has no camera".to_owned())?;
        let mesh_record = mesh_records
            .first()
            .map(|(_, record)| *record)
            .ok_or_else(|| "NewViso scene snapshot has no mesh".to_owned())?;
        let camera_entity_id = camera_entity_id
            .ok_or_else(|| "MainCamera Flecs entity has no stable id".to_owned())?;

        let camera_transform = camera_record
            .get("transform")
            .ok_or_else(|| "MainCamera has no transform".to_owned())?;
        let camera_desc = camera_record
            .get("camera")
            .ok_or_else(|| "MainCamera has no camera component".to_owned())?;

        let camera = Camera {
            position: read_vec3(camera_transform, "position", Vec3::new(4.2, 3.0, 6.0))?,
            target: read_vec3(camera_transform, "target", Vec3::ZERO)?,
            up: read_vec3(camera_transform, "up", Vec3::Y)?,
            fov_y_degrees: read_f32(camera_desc, "fov_y_degrees", 58.0)?,
            near: read_f32(camera_desc, "near", 0.1)?,
            far: read_f32(camera_desc, "far", 100.0)?,
        };

        let orbit = OrbitCamera::from_camera(&camera);

        let mut world = SceneWorld::new(camera.position);
        world.add_entity(SceneEntity {
            id: SceneEntityId(camera_entity_id),
            name: camera_record
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("MainCamera")
                .to_owned(),
            kind: SceneEntityKind::Camera,
            mobility: SceneMobility::Dynamic,
            lifecycle: SceneLifecycle::Constructed,
            transform: SceneTransform {
                position: camera.position,
                rotation_degrees: Vec3::ZERO,
                scale: Vec3::ONE,
            },
            light: None,
            bounds: SceneBounds::from_center_half_extent(
                camera.position,
                Vec3::new(0.05, 0.05, 0.05),
            ),
            parent: None,
            children: Vec::new(),
            visibility: VisibilityMask::default(),
            lod: SceneLodPolicy::default(),
            solid: false,
            asset_ref: None,
            render_slot: None,
            residency: SceneResidency::Resident,
            priority_score: 0.0,
            lod_alpha: 1.0,
            last_visible_frame: None,
        })?;

        let mut cubes = Vec::with_capacity(mesh_records.len());
        let mut parent_links = Vec::<(SceneEntityId, SceneEntityId)>::new();
        for (stable_id, record) in mesh_records {
            let transform = record.get("transform").ok_or("mesh has no transform")?;
            let primitive = record
                .pointer("/mesh/primitive")
                .and_then(Value::as_str)
                .unwrap_or("");
            let asset_ref = record
                .pointer("/mesh/asset")
                .or_else(|| record.pointer("/mesh/model"))
                .and_then(Value::as_str)
                .map(str::to_owned);

            if primitive != "cube" {
                let Some(asset_ref) = asset_ref else {
                    return Err(format!(
                        "scene mesh '{}' has neither primitive='cube' nor mesh.asset",
                        record.get("name").and_then(Value::as_str).unwrap_or("Mesh")
                    ));
                };
                let position = read_vec3(transform, "position", Vec3::ZERO)?;
                let rotation_degrees = read_vec3(transform, "rotation_degrees", Vec3::ZERO)?;
                let scale = read_vec3(transform, "scale", Vec3::ONE)?;
                let half_extent = Vec3::new(
                    scale.x.abs().max(0.1) * 0.5,
                    scale.y.abs().max(0.1) * 0.5,
                    scale.z.abs().max(0.1) * 0.5,
                );
                let mobility = match record.get("mobility").and_then(Value::as_str) {
                    Some("dynamic") => SceneMobility::Dynamic,
                    _ => SceneMobility::Static,
                };
                world.add_entity(SceneEntity {
                    id: SceneEntityId(stable_id),
                    name: record
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("AssetMesh")
                        .to_owned(),
                    kind: match mobility {
                        SceneMobility::Static => SceneEntityKind::StaticMesh,
                        SceneMobility::Dynamic => SceneEntityKind::DynamicMesh,
                    },
                    mobility,
                    lifecycle: SceneLifecycle::Constructed,
                    transform: SceneTransform {
                        position,
                        rotation_degrees,
                        scale,
                    },
                    light: None,
                    bounds: SceneBounds::from_center_half_extent(position, half_extent),
                    parent: None,
                    children: Vec::new(),
                    visibility: read_visibility_mask(record),
                    lod: read_lod_policy(record)?,
                    solid: record.pointer("/collider/solid").and_then(Value::as_bool) == Some(true),
                    asset_ref: Some(asset_ref),
                    render_slot: None,
                    residency: SceneResidency::Unloaded,
                    priority_score: 0.0,
                    lod_alpha: 1.0,
                    last_visible_frame: None,
                })?;
                if let Some(parent_id) = read_parent_id(record) {
                    parent_links.push((SceneEntityId(stable_id), SceneEntityId(parent_id)));
                }
                continue;
            }

            let material = record.get("material").ok_or("cube mesh has no material")?;
            let cube = Cube {
                position: read_vec3(transform, "position", Vec3::ZERO)?,
                rotation_degrees: read_vec3(transform, "rotation_degrees", Vec3::ZERO)?,
                scale: read_vec3(transform, "scale", Vec3::ONE)?,
                base_color: read_color4(material, "base_color", [0.95, 0.42, 0.12, 1.0])?,
            };
            let bounds = cube.bounds();
            let render_slot = cubes.len();
            let solid = record.pointer("/collider/solid").and_then(Value::as_bool) == Some(true);
            let mobility = match record.get("mobility").and_then(Value::as_str) {
                Some("dynamic") => SceneMobility::Dynamic,
                _ => SceneMobility::Static,
            };
            let kind = match mobility {
                SceneMobility::Static => SceneEntityKind::StaticMesh,
                SceneMobility::Dynamic => SceneEntityKind::DynamicMesh,
            };
            let visibility = read_visibility_mask(record);
            let lod = read_lod_policy(record)?;

            world.add_entity(SceneEntity {
                id: SceneEntityId(stable_id),
                name: record
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Mesh")
                    .to_owned(),
                kind,
                mobility,
                lifecycle: SceneLifecycle::Constructed,
                transform: SceneTransform {
                    position: cube.position,
                    rotation_degrees: cube.rotation_degrees,
                    scale: cube.scale,
                },
                light: None,
                bounds: SceneBounds {
                    min: Vec3::new(bounds.min[0], bounds.min[1], bounds.min[2]),
                    max: Vec3::new(bounds.max[0], bounds.max[1], bounds.max[2]),
                },
                parent: None,
                children: Vec::new(),
                visibility,
                lod,
                solid,
                asset_ref: None,
                render_slot: Some(render_slot),
                residency: SceneResidency::Resident,
                priority_score: 0.0,
                lod_alpha: 1.0,
                last_visible_frame: None,
            })?;
            if let Some(parent_id) = read_parent_id(record) {
                parent_links.push((SceneEntityId(stable_id), SceneEntityId(parent_id)));
            }
            cubes.push(cube);
        }

        for (stable_id, record) in logic_records {
            let kind_name = record
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let kind = match kind_name {
                "light" => SceneEntityKind::Light,
                "trigger" => SceneEntityKind::Trigger,
                "portal" => SceneEntityKind::Portal,
                _ => SceneEntityKind::Unknown,
            };
            let mobility = match record.get("mobility").and_then(Value::as_str) {
                Some("dynamic") => SceneMobility::Dynamic,
                _ => SceneMobility::Static,
            };
            let transform_record = record.get("transform").unwrap_or(&Value::Null);
            let position = read_vec3(transform_record, "position", Vec3::ZERO)?;
            let rotation_degrees = read_vec3(transform_record, "rotation_degrees", Vec3::ZERO)?;
            let scale = read_vec3(transform_record, "scale", Vec3::ONE)?;
            let half_extent = Vec3::new(
                scale.x.abs().max(0.1) * 0.5,
                scale.y.abs().max(0.1) * 0.5,
                scale.z.abs().max(0.1) * 0.5,
            );

            world.add_entity(SceneEntity {
                id: SceneEntityId(stable_id),
                name: record
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(kind_name)
                    .to_owned(),
                kind,
                mobility,
                lifecycle: SceneLifecycle::Constructed,
                transform: SceneTransform {
                    position,
                    rotation_degrees,
                    scale,
                },
                light: None,
                bounds: SceneBounds::from_center_half_extent(position, half_extent),
                parent: None,
                children: Vec::new(),
                visibility: read_visibility_mask(record),
                lod: read_lod_policy(record)?,
                solid: false,
                asset_ref: record
                    .pointer("/asset/ref")
                    .or_else(|| record.pointer("/mesh/asset"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                render_slot: None,
                residency: SceneResidency::Resident,
                priority_score: 0.0,
                lod_alpha: 1.0,
                last_visible_frame: None,
            })?;
            if let Some(parent_id) = read_parent_id(record) {
                parent_links.push((SceneEntityId(stable_id), SceneEntityId(parent_id)));
            }
        }

        for (child, parent) in parent_links {
            world.set_parent(child, Some(parent))?;
        }
        world.activate_all();
        let frame_plan = SceneFramePlan::default();

        let title = snapshot
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("NewViso 3D Scene")
            .to_owned();
        let camera_name = camera_record
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("MainCamera")
            .to_owned();
        let mesh_name = mesh_record
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Cube")
            .to_owned();

        let report = Scene3dLoadReport {
            title: title.clone(),
            entity_count: entities.len(),
            camera_name,
            mesh_name,
        };

        Ok((
            Self {
                title,
                camera_entity_id,
                camera,
                orbit,
                cubes,
                transient_spheres: Vec::new(),
                overlay_quads: Vec::new(),
                runtime_entity_ids: BTreeMap::new(),
                next_runtime_entity_id: 0x4e56_5343_0000_0000,
                world,
                frame_plan,
                clear_color: [0.025, 0.032, 0.045, 1.0],
                gpu: None,
                sky: None,
                gpu_sky: None,
                frame_index: 0,
            },
            report,
        ))
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn set_clear_color(&mut self, clear_color: [f32; 4]) {
        self.clear_color = clear_color;
    }

    pub fn upsert_runtime_light(&mut self, key: &str, desc: SceneLightDesc) -> Result<u64, String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("runtime light id must not be empty".to_owned());
        }

        let light = LightComponent {
            light_type: match desc.light_type {
                SceneLightType::Directional => LightType::Directional,
                SceneLightType::Point => LightType::Point,
                SceneLightType::Spot => LightType::Spot,
                SceneLightType::Area => LightType::Area,
            },
            color: desc.color,
            intensity: desc.intensity,
            range: desc.range,
            cone_inner_degrees: desc.cone_inner_degrees,
            cone_outer_degrees: desc.cone_outer_degrees,
            casts_shadows: desc.casts_shadows,
            shadow_bias: desc.shadow_bias,
            shadow_normal_bias: desc.shadow_normal_bias,
            shadow_resolution: desc.shadow_resolution,
            shadow_distance: desc.shadow_distance,
        }
        .validate()?;

        let existing = self
            .runtime_entity_ids
            .get(key)
            .copied()
            .map(SceneEntityId)
            .or_else(|| self.world.entity_id_by_name(key));

        let id = if let Some(id) = existing {
            if let Some(entity) = self.world.entity(id) {
                if entity.kind != SceneEntityKind::Light && entity.light.is_none() {
                    return Err(format!(
                        "runtime entity '{}' already exists and is not a light",
                        key
                    ));
                }
            }
            self.world.set_light(id, Some(light))?;
            id
        } else {
            let id = SceneEntityId(self.next_runtime_entity_id);
            self.next_runtime_entity_id = self.next_runtime_entity_id.wrapping_add(1);
            self.world.add_entity(SceneEntity {
                id,
                name: key.to_owned(),
                kind: SceneEntityKind::Light,
                mobility: SceneMobility::Dynamic,
                lifecycle: SceneLifecycle::Constructed,
                transform: SceneTransform {
                    position: Vec3::ZERO,
                    rotation_degrees: Vec3::ZERO,
                    scale: Vec3::ONE,
                },
                light: Some(light),
                bounds: SceneBounds::from_center_half_extent(Vec3::ZERO, Vec3::new(0.1, 0.1, 0.1)),
                parent: None,
                children: Vec::new(),
                visibility: VisibilityMask::default(),
                lod: SceneLodPolicy::default(),
                solid: false,
                asset_ref: None,
                render_slot: None,
                residency: SceneResidency::Resident,
                priority_score: 0.0,
                lod_alpha: 1.0,
                last_visible_frame: None,
            })?;
            self.world.activate_all();
            self.runtime_entity_ids.insert(key.to_owned(), id.0);
            id
        };

        Ok(id.0)
    }

    pub fn set_runtime_entity_transform(
        &mut self,
        key: &str,
        position: Option<[f32; 3]>,
        rotation_degrees: Option<[f32; 3]>,
        scale: Option<[f32; 3]>,
    ) -> Result<(), String> {
        let key = key.trim();
        let id = self
            .runtime_entity_ids
            .get(key)
            .copied()
            .map(SceneEntityId)
            .or_else(|| self.world.entity_id_by_name(key))
            .ok_or_else(|| format!("runtime entity '{}' does not exist", key))?;
        let current = self
            .world
            .entity(id)
            .ok_or_else(|| format!("runtime entity '{}' disappeared", key))?
            .transform;

        let p = position.unwrap_or([current.position.x, current.position.y, current.position.z]);
        let r = rotation_degrees.unwrap_or([
            current.rotation_degrees.x,
            current.rotation_degrees.y,
            current.rotation_degrees.z,
        ]);
        let s = scale.unwrap_or([current.scale.x, current.scale.y, current.scale.z]);
        self.set_entity_transform(id.0, p, r, s)
    }

    pub fn remove_runtime_entity(&mut self, key: &str) -> Result<(), String> {
        let key = key.trim();
        let id = self
            .runtime_entity_ids
            .get(key)
            .copied()
            .map(SceneEntityId)
            .or_else(|| self.world.entity_id_by_name(key))
            .ok_or_else(|| format!("runtime entity '{}' does not exist", key))?;
        self.world.request_remove(id)?;
        self.runtime_entity_ids.remove(key);
        Ok(())
    }

    pub fn set_scene_focus_camera(&mut self) {
        self.world.set_focus_camera();
    }

    pub fn set_scene_focus_entity(&mut self, stable_id: u64) -> Result<(), String> {
        self.world.set_focus_entity(SceneEntityId(stable_id))
    }

    pub fn set_scene_focus_override(&mut self, position: [f32; 3], velocity: [f32; 3]) {
        self.world.set_focus_override(
            Vec3::new(position[0], position[1], position[2]),
            Vec3::new(velocity[0], velocity[1], velocity[2]),
        );
    }

    pub fn set_entity_visibility(
        &mut self,
        stable_id: u64,
        module: &str,
        visible: bool,
    ) -> Result<(), String> {
        let module = visibility_module_from_name(module)
            .ok_or_else(|| format!("unknown scene visibility module '{module}'"))?;
        self.world
            .set_visibility(SceneEntityId(stable_id), module, visible)
    }

    pub fn request_remove_entity(&mut self, stable_id: u64) -> Result<(), String> {
        self.world.request_remove(SceneEntityId(stable_id))
    }

    pub fn pending_stream_requests(&self) -> Vec<SceneStreamRequest> {
        self.frame_plan
            .requested_entities
            .iter()
            .filter_map(|id| {
                let entity = self.world.entity(*id)?;
                Some(SceneStreamRequest {
                    stable_id: id.0,
                    asset_ref: entity.asset_ref.clone()?,
                    priority: entity.priority_score,
                })
            })
            .collect()
    }

    pub fn streaming_interests(&self) -> Vec<SceneStreamRequest> {
        self.frame_plan
            .streaming_entities
            .iter()
            .filter_map(|id| {
                let entity = self.world.entity(*id)?;
                Some(SceneStreamRequest {
                    stable_id: id.0,
                    asset_ref: entity.asset_ref.clone()?,
                    priority: entity.priority_score,
                })
            })
            .collect()
    }

    pub fn mark_entity_resident(&mut self, stable_id: u64) -> Result<(), String> {
        self.world
            .set_residency(SceneEntityId(stable_id), SceneResidency::Resident)
    }

    pub fn mark_entity_unloaded(&mut self, stable_id: u64) -> Result<(), String> {
        self.world
            .set_residency(SceneEntityId(stable_id), SceneResidency::Unloaded)
    }

    pub fn set_entity_parent(
        &mut self,
        child_stable_id: u64,
        parent_stable_id: Option<u64>,
    ) -> Result<(), String> {
        self.world.set_parent(
            SceneEntityId(child_stable_id),
            parent_stable_id.map(SceneEntityId),
        )
    }

    pub fn set_entity_transform(
        &mut self,
        stable_id: u64,
        position: [f32; 3],
        rotation_degrees: [f32; 3],
        scale: [f32; 3],
    ) -> Result<(), String> {
        let id = SceneEntityId(stable_id);
        let transform = SceneTransform {
            position: Vec3::new(position[0], position[1], position[2]),
            rotation_degrees: Vec3::new(
                rotation_degrees[0],
                rotation_degrees[1],
                rotation_degrees[2],
            ),
            scale: Vec3::new(scale[0], scale[1], scale[2]),
        };
        if [
            position[0],
            position[1],
            position[2],
            rotation_degrees[0],
            rotation_degrees[1],
            rotation_degrees[2],
            scale[0],
            scale[1],
            scale[2],
        ]
        .iter()
        .any(|value| !value.is_finite())
        {
            return Err("scene transform values must be finite".to_owned());
        }

        let (render_slot, previous_bounds) = {
            let entity = self
                .world
                .entity(id)
                .ok_or_else(|| format!("scene entity {} does not exist", stable_id))?;
            (entity.render_slot, entity.bounds)
        };

        let bounds = if let Some(render_slot) = render_slot {
            let cube = self
                .cubes
                .get_mut(render_slot)
                .ok_or_else(|| format!("scene render slot {} is invalid", render_slot))?;
            cube.position = transform.position;
            cube.rotation_degrees = transform.rotation_degrees;
            cube.scale = transform.scale;
            let bounds = cube.bounds();
            SceneBounds {
                min: Vec3::new(bounds.min[0], bounds.min[1], bounds.min[2]),
                max: Vec3::new(bounds.max[0], bounds.max[1], bounds.max[2]),
            }
        } else {
            let old_center = previous_bounds.center();
            let old_half = previous_bounds.max.sub(old_center);
            SceneBounds::from_center_half_extent(transform.position, old_half)
        };

        self.world.update_spatial(id, transform, bounds)
    }

    pub fn entity_state(&self, stable_id: u64) -> Option<Value> {
        let entity = self.world.entity(SceneEntityId(stable_id))?;
        let kind = match entity.kind {
            SceneEntityKind::Camera => "camera",
            SceneEntityKind::StaticMesh => "static_mesh",
            SceneEntityKind::DynamicMesh => "dynamic_mesh",
            SceneEntityKind::Light => "light",
            SceneEntityKind::Trigger => "trigger",
            SceneEntityKind::Portal => "portal",
            SceneEntityKind::Unknown => "unknown",
        };
        let mobility = match entity.mobility {
            SceneMobility::Static => "static",
            SceneMobility::Dynamic => "dynamic",
        };
        let lifecycle = match entity.lifecycle {
            SceneLifecycle::Constructed => "constructed",
            SceneLifecycle::Added => "added",
            SceneLifecycle::Active => "active",
            SceneLifecycle::Dormant => "dormant",
            SceneLifecycle::PendingRemove => "pending_remove",
            SceneLifecycle::Removed => "removed",
        };
        let residency = match entity.residency {
            SceneResidency::Unloaded => "unloaded",
            SceneResidency::Requested => "requested",
            SceneResidency::Resident => "resident",
        };

        Some(json!({
            "id": entity.id.0,
            "name": entity.name,
            "kind": kind,
            "mobility": mobility,
            "lifecycle": lifecycle,
            "residency": residency,
            "asset_ref": entity.asset_ref,
            "visibility_mask": entity.visibility.raw(),
            "priority_score": entity.priority_score,
            "lod_alpha": entity.lod_alpha,
            "transform": {
                "position": [
                    entity.transform.position.x,
                    entity.transform.position.y,
                    entity.transform.position.z
                ],
                "rotation_degrees": [
                    entity.transform.rotation_degrees.x,
                    entity.transform.rotation_degrees.y,
                    entity.transform.rotation_degrees.z
                ],
                "scale": [
                    entity.transform.scale.x,
                    entity.transform.scale.y,
                    entity.transform.scale.z
                ]
            }
        }))
    }

    pub fn set_sky_dome(&mut self, sky: SkyDomeResources) -> Result<(), String> {
        if self.gpu.is_some() || self.gpu_sky.is_some() {
            return Err(
                "sky dome resources must be assigned before renderer initialization".to_owned(),
            );
        }

        if sky.mesh.vertices.is_empty() {
            return Err(format!(
                "sky dome model '{}' has no vertices",
                sky.model_name
            ));
        }
        if sky.mesh.indices.is_empty() {
            return Err(format!(
                "sky dome model '{}' has no indices",
                sky.model_name
            ));
        }
        if !sky
            .mesh
            .bounds_min
            .iter()
            .chain(sky.mesh.bounds_max.iter())
            .all(|value| value.is_finite())
        {
            return Err(format!(
                "sky dome model '{}' has non-finite bounds",
                sky.model_name
            ));
        }
        if sky.mesh.vertices.iter().any(|vertex| {
            vertex
                .position
                .iter()
                .chain(vertex.uv.iter())
                .any(|value| !value.is_finite())
        }) {
            return Err(format!(
                "sky dome model '{}' has non-finite vertex data",
                sky.model_name
            ));
        }
        let vertex_count = sky.mesh.vertices.len() as u32;
        if let Some(index) = sky
            .mesh
            .indices
            .iter()
            .find(|index| **index >= vertex_count)
        {
            return Err(format!(
                "sky dome model '{}' has out-of-range index {} for {} vertices",
                sky.model_name, index, vertex_count
            ));
        }
        if sky.mesh.index_format == SkyIndexFormat::U16
            && sky
                .mesh
                .indices
                .iter()
                .any(|index| *index > u16::MAX as u32)
        {
            return Err(format!(
                "sky dome model '{}' declares U16 indices but contains an index > {}",
                sky.model_name,
                u16::MAX
            ));
        }

        self.sky = Some(sky);
        Ok(())
    }

    pub fn runtime_state(&self) -> Value {
        let focus = self.world.focus();
        let focus_source = match focus.source {
            SceneFocusSource::Camera => "camera",
            SceneFocusSource::Entity(_) => "entity",
            SceneFocusSource::Override => "override",
        };
        let solids = self
            .world
            .solid_bounds()
            .map(|bounds| {
                json!({
                    "min": [bounds.min.x, bounds.min.y, bounds.min.z],
                    "max": [bounds.max.x, bounds.max.y, bounds.max.z]
                })
            })
            .collect::<Vec<_>>();
        let lights = self.world.active_lights();
        let shadow_casters = lights
            .iter()
            .filter(|(_, _, light)| light.casts_shadows)
            .count();
        let light_state = lights
            .iter()
            .map(|(id, transform, light)| {
                let light_type = match light.light_type {
                    LightType::Directional => "directional",
                    LightType::Point => "point",
                    LightType::Spot => "spot",
                    LightType::Area => "area",
                };
                json!({
                    "entity": id.0,
                    "type": light_type,
                    "rotation_degrees": [
                        transform.rotation_degrees.x,
                        transform.rotation_degrees.y,
                        transform.rotation_degrees.z
                    ],
                    "casts_shadows": light.casts_shadows
                })
            })
            .collect::<Vec<_>>();

        json!({
            "scene": {
                "title": self.title,
                "mesh_count": self.cubes.len(),
                "world": {
                    "frame": self.frame_plan.frame,
                    "entities": self.world.entity_count(),
                    "static_entities": self.world.static_count(),
                    "dynamic_entities": self.world.dynamic_count(),
                    "visible": self.frame_plan.visible_count,
                    "culled": self.frame_plan.culled_count,
                    "resident": self.frame_plan.resident_count,
                    "stream_requests": self.frame_plan.requested_entities.len(),
                    "solids": solids,
                    "focus": {
                        "source": focus_source,
                        "position": [focus.position.x, focus.position.y, focus.position.z],
                        "velocity": [focus.velocity.x, focus.velocity.y, focus.velocity.z]
                    }
                },
                "camera": {
                    "position": {
                        "x": self.camera.position.x,
                        "y": self.camera.position.y,
                        "z": self.camera.position.z
                    },
                    "target": {
                        "x": self.camera.target.x,
                        "y": self.camera.target.y,
                        "z": self.camera.target.z
                    },
                    "up": {
                        "x": self.camera.up.x,
                        "y": self.camera.up.y,
                        "z": self.camera.up.z
                    },
                    "fov_y_degrees": self.camera.fov_y_degrees,
                    "near": self.camera.near,
                    "far": self.camera.far,
                    "orbit": {
                        "yaw_radians": self.orbit.yaw,
                        "pitch_radians": self.orbit.pitch,
                        "yaw_degrees": self.orbit.yaw.to_degrees(),
                        "pitch_degrees": self.orbit.pitch.to_degrees(),
                        "distance": self.orbit.distance
                    }
                },
                "lighting": {
                    "active_lights": lights.len(),
                    "shadow_casters": shadow_casters,
                    "lights": light_state
                },
                "transient": {
                    "spheres": self.transient_spheres.len(),
                    "overlay_quads": self.overlay_quads.len()
                }
            }
        })
    }

    pub fn configure_orbit(
        &mut self,
        rotate_sensitivity: f32,
        zoom_sensitivity: f32,
        min_distance: f32,
        max_distance: f32,
    ) -> Result<(), String> {
        if rotate_sensitivity <= 0.0
            || zoom_sensitivity <= 0.0
            || min_distance <= 0.0
            || max_distance < min_distance
        {
            return Err("invalid orbit camera settings".to_owned());
        }

        self.orbit.rotate_sensitivity = rotate_sensitivity;
        self.orbit.zoom_sensitivity = zoom_sensitivity;
        self.orbit.min_distance = min_distance;
        self.orbit.max_distance = max_distance;
        self.orbit.distance = self.orbit.distance.clamp(min_distance, max_distance);
        self.camera.position = self.orbit.position(self.camera.target);
        Ok(())
    }

    pub fn initialize_renderer(&mut self) -> Result<(), String> {
        let render = RenderClient::new();

        if self.sky.is_some() {
            self.initialize_sky_renderer(&render)?;
        }

        let vertex_buffer = render.create_buffer(
            "newviso.first_scene.vertices",
            self.vertex_capacity() as u64 * VERTEX_STRIDE,
            "Vertex",
            "CpuToGpu",
        )?;
        let shadow_vertex_buffer = render.create_buffer(
            "newviso.first_scene.shadow_vertices",
            self.shadow_vertex_capacity() as u64 * VERTEX_STRIDE,
            "Vertex",
            "CpuToGpu",
        )?;
        let frame_uniform = render.create_buffer(
            "newviso.scene.frame_uniform",
            (SCENE_FRAME_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
            "Uniform",
            "CpuToGpu",
        )?;

        let shadow_render_target = render.create_render_target(
            "newviso.scene.shadow_map",
            DEFAULT_SHADOW_RESOLUTION,
            DEFAULT_SHADOW_RESOLUTION,
            "R32Float",
            Some("Depth32Float"),
        )?;
        let shadow_texture = render.render_target_color_texture(shadow_render_target)?;
        let shadow_sampler = render.create_sampler_clamp_linear("newviso.scene.shadow_sampler")?;

        let bind_group_layout = render.create_bind_group_layout(
            "newviso.scene.frame_bindings",
            &["UniformBuffer", "Texture2D", "Sampler"],
        )?;
        let bind_group = render.create_bind_group(
            "newviso.scene.frame_bind_group",
            bind_group_layout,
            [Some(shadow_texture), None, None],
            Some(shadow_sampler),
            Some((
                frame_uniform,
                0,
                (SCENE_FRAME_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
            )),
        )?;

        let shadow_bind_group_layout =
            render.create_bind_group_layout("newviso.scene.shadow_bindings", &["UniformBuffer"])?;
        let shadow_bind_group = render.create_bind_group(
            "newviso.scene.shadow_bind_group",
            shadow_bind_group_layout,
            [None, None, None],
            None,
            Some((
                frame_uniform,
                0,
                (SCENE_FRAME_UNIFORM_FLOATS * std::mem::size_of::<f32>()) as u64,
            )),
        )?;

        let vertex_shader = render.create_shader_spirv(
            "newviso.first_scene.vertex",
            ShaderStage::Vertex,
            VERTEX_SHADER,
        )?;
        let fragment_shader = render.create_shader_spirv(
            "newviso.first_scene.fragment",
            ShaderStage::Fragment,
            FRAGMENT_SHADER,
        )?;
        let shadow_vertex_shader = render.create_shader_spirv(
            "newviso.scene.shadow.vertex",
            ShaderStage::Vertex,
            SHADOW_VERTEX_SHADER,
        )?;
        let shadow_fragment_shader = render.create_shader_spirv(
            "newviso.scene.shadow.fragment",
            ShaderStage::Fragment,
            SHADOW_FRAGMENT_SHADER,
        )?;

        let attributes = [
            VertexAttribute {
                location: 0,
                offset: 0,
                format: VertexFormat::Float32x4,
            },
            VertexAttribute {
                location: 1,
                offset: 16,
                format: VertexFormat::Float32x3,
            },
            VertexAttribute {
                location: 2,
                offset: 28,
                format: VertexFormat::Float32x4,
            },
        ];

        let pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.first_scene.pipeline",
            vertex_shader,
            fragment_shader,
            vertex_stride: VERTEX_STRIDE,
            attributes: &attributes,
            topology: "TriangleList",
            bind_group_layouts: &[bind_group_layout],
            color_format: "Bgra8Unorm",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: true,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.scene.lit.v1",
        })?;

        let shadow_attributes = [VertexAttribute {
            location: 0,
            offset: 0,
            format: VertexFormat::Float32x4,
        }];
        let shadow_pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.scene.shadow.pipeline",
            vertex_shader: shadow_vertex_shader,
            fragment_shader: shadow_fragment_shader,
            vertex_stride: VERTEX_STRIDE,
            attributes: &shadow_attributes,
            topology: "TriangleList",
            bind_group_layouts: &[shadow_bind_group_layout],
            color_format: "R32Float",
            depth_format: Some("Depth32Float"),
            depth_test: true,
            depth_write: true,
            depth_compare: "LessOrEqual",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.scene.shadow.v1",
        })?;

        self.gpu = Some(GpuScene {
            vertex_buffer,
            shadow_vertex_buffer,
            frame_uniform,
            bind_group_layout,
            bind_group,
            shadow_bind_group_layout,
            shadow_bind_group,
            shadow_render_target,
            shadow_sampler,
            vertex_shader,
            fragment_shader,
            pipeline,
            shadow_vertex_shader,
            shadow_fragment_shader,
            shadow_pipeline,
            shadow_resolution: DEFAULT_SHADOW_RESOLUTION,
        });

        host_runtime::info(
            "newviso.scene",
            format!(
                "GPU scene ready buffer={} shadow_buffer={} pipeline={} shadow_pipeline={} shadow_map={}x{} sky={}",
                vertex_buffer,
                shadow_vertex_buffer,
                pipeline,
                shadow_pipeline,
                DEFAULT_SHADOW_RESOLUTION,
                DEFAULT_SHADOW_RESOLUTION,
                self.gpu_sky.is_some()
            ),
        );
        Ok(())
    }

    fn initialize_sky_renderer(&mut self, render: &RenderClient) -> Result<(), String> {
        let sky = self
            .sky
            .as_ref()
            .ok_or_else(|| "sky resources are not assigned".to_owned())?;
        let mesh = &sky.mesh;
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            return Err("sky runtime mesh is empty".to_owned());
        }

        let sky_center = [
            (mesh.bounds_min[0] + mesh.bounds_max[0]) * 0.5,
            (mesh.bounds_min[1] + mesh.bounds_max[1]) * 0.5,
            (mesh.bounds_min[2] + mesh.bounds_max[2]) * 0.5,
        ];
        let mut static_vertices = Vec::with_capacity(mesh.vertices.len() * SKY_FLOATS_PER_VERTEX);
        for (index, vertex) in mesh.vertices.iter().enumerate() {
            let x = vertex.position[0] - sky_center[0];
            let y = vertex.position[1] - sky_center[1];
            let z = vertex.position[2] - sky_center[2];
            let length = (x * x + y * y + z * z).sqrt();
            if !length.is_finite() || length <= 1.0e-6 {
                return Err(format!("sky dome vertex {index} is at the mesh centre"));
            }
            static_vertices.extend_from_slice(&[
                x / length,
                y / length,
                z / length,
                vertex.uv[0],
                vertex.uv[1],
            ]);
        }

        let index_bytes = match mesh.index_format {
            SkyIndexFormat::U16 => {
                let mut bytes = Vec::with_capacity(mesh.indices.len() * 2);
                for &index in &mesh.indices {
                    let value = u16::try_from(index).map_err(|_| {
                        format!("sky index {index} does not fit declared U16 index format")
                    })?;
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                bytes
            }
            SkyIndexFormat::U32 => {
                let mut bytes = Vec::with_capacity(mesh.indices.len() * 4);
                for &index in &mesh.indices {
                    bytes.extend_from_slice(&index.to_le_bytes());
                }
                bytes
            }
        };

        let vertex_buffer = render.create_buffer(
            "newviso.sky.vertices",
            mesh.vertices.len() as u64 * SKY_VERTEX_STRIDE,
            "Vertex",
            "CpuToGpu",
        )?;
        let index_buffer = render.create_buffer(
            "newviso.sky.indices",
            index_bytes.len() as u64,
            "Index",
            "CpuToGpu",
        )?;
        let camera_uniform =
            render.create_buffer("newviso.sky.camera", 64, "Uniform", "CpuToGpu")?;

        render.write_buffer_f32(vertex_buffer, 0, &static_vertices)?;
        render.write_buffer(index_buffer, 0, &index_bytes)?;

        let base_noise = upload_sky_texture(render, "newviso.sky.base_noise", &sky.base_noise)?;
        let starfield = upload_sky_texture(render, "newviso.sky.starfield", &sky.starfield)?;
        let detail_noise =
            upload_sky_texture(render, "newviso.sky.detail_noise", &sky.detail_noise)?;
        let sampler = render.create_sampler_repeat_linear("newviso.sky.sampler")?;
        let bind_group_layout = render.create_bind_group_layout(
            "newviso.sky.bindings",
            &[
                "UniformBuffer",
                "Texture2D",
                "Texture2D",
                "Texture2D",
                "Sampler",
            ],
        )?;
        let bind_group = render.create_bind_group(
            "newviso.sky.bind_group",
            bind_group_layout,
            [Some(base_noise), Some(starfield), Some(detail_noise)],
            Some(sampler),
            Some((camera_uniform, 0, 64)),
        )?;

        let vertex_shader = render.create_shader_spirv(
            "newviso.sky.vertex",
            ShaderStage::Vertex,
            SKY_VERTEX_SHADER,
        )?;
        let fragment_shader = render.create_shader_spirv(
            "newviso.sky.fragment",
            ShaderStage::Fragment,
            SKY_FRAGMENT_SHADER,
        )?;

        let attributes = [
            VertexAttribute {
                location: 0,
                offset: 0,
                format: VertexFormat::Float32x3,
            },
            VertexAttribute {
                location: 1,
                offset: 12,
                format: VertexFormat::Float32x2,
            },
        ];
        let pipeline = render.create_pipeline(GraphicsPipelineDesc {
            label: "newviso.sky.pipeline",
            vertex_shader,
            fragment_shader,
            vertex_stride: SKY_VERTEX_STRIDE,
            attributes: &attributes,
            topology: "TriangleList",
            bind_group_layouts: &[bind_group_layout],
            color_format: "Bgra8Unorm",
            depth_format: Some("Depth32Float"),
            depth_test: false,
            depth_write: false,
            depth_compare: "Always",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.sky.semantic.v3",
        })?;

        let index_format = match mesh.index_format {
            SkyIndexFormat::U16 => "U16",
            SkyIndexFormat::U32 => "U32",
        };
        self.gpu_sky = Some(GpuSky {
            vertex_buffer,
            index_buffer,
            base_noise,
            starfield,
            detail_noise,
            sampler,
            bind_group_layout,
            bind_group,
            camera_uniform,
            vertex_shader,
            fragment_shader,
            pipeline,
            index_count: mesh.indices.len() as u32,
            index_format,
        });

        host_runtime::info(
            "newviso.scene",
            format!(
                "sky dome GPU ready model='{}' material='{}' vertices={} indices={} textures=[{},{},{}]",
                sky.model_name,
                sky.material_name,
                mesh.vertices.len(),
                mesh.indices.len(),
                sky.base_noise.name,
                sky.starfield.name,
                sky.detail_noise.name
            ),
        );
        Ok(())
    }

    pub fn update_native_input_from_snapshot(
        &mut self,
        input: &InputSnapshot,
        dt: f32,
        camera_navigation_enabled: bool,
    ) -> Result<(), String> {
        if camera_navigation_enabled {
            let [dx, dy] = input.mouse_delta();
            let wheel_y = input.mouse_wheel_y();
            let rotating = input.mouse_button_down(PRIMARY_MOUSE_BUTTON);
            if ((dx != 0.0 || dy != 0.0) && rotating) || wheel_y != 0.0 {
                self.orbit.apply_mouse(dx, dy, wheel_y, rotating);
                self.camera.position = self.orbit.position(self.camera.target);
                self.sync_runtime_camera_to_flecs()?;
            }
        }
        self.tick(dt)
    }

    pub fn tick(&mut self, dt: f32) -> Result<(), String> {
        self.update_scene_world(dt)
    }

    pub fn set_camera_pose(
        &mut self,
        position: [f32; 3],
        target: [f32; 3],
        up: Option<[f32; 3]>,
        fov_y_degrees: Option<f32>,
    ) -> Result<(), String> {
        if position
            .iter()
            .chain(target.iter())
            .any(|value| !value.is_finite())
        {
            return Err("scene.camera.set contains a non-finite position or target".to_owned());
        }
        let position = Vec3::new(position[0], position[1], position[2]);
        let target = Vec3::new(target[0], target[1], target[2]);
        if target.sub(position).length() < 0.0001 {
            return Err("scene.camera.set target must differ from position".to_owned());
        }

        self.camera.position = position;
        self.camera.target = target;
        if let Some(up) = up {
            if up.iter().any(|value| !value.is_finite()) {
                return Err("scene.camera.set contains a non-finite up vector".to_owned());
            }
            let up = Vec3::new(up[0], up[1], up[2]);
            if up.length() < 0.0001 {
                return Err("scene.camera.set up vector must be non-zero".to_owned());
            }
            self.camera.up = up.normalized();
        }
        if let Some(fov) = fov_y_degrees {
            if !fov.is_finite() || !(1.0..179.0).contains(&fov) {
                return Err(
                    "scene.camera.set fov_y_degrees must be finite and in 1..179".to_owned(),
                );
            }
            self.camera.fov_y_degrees = fov;
        }
        self.orbit = OrbitCamera::from_camera(&self.camera);
        self.sync_runtime_camera_to_flecs()
    }

    pub fn set_transient_spheres(
        &mut self,
        spheres: Vec<SceneTransientSphere>,
    ) -> Result<(), String> {
        if spheres.len() > MAX_TRANSIENT_SPHERES {
            return Err(format!(
                "scene.transient_spheres.set exceeds the generic limit of {MAX_TRANSIENT_SPHERES}"
            ));
        }
        for sphere in &spheres {
            if sphere.position.iter().any(|value| !value.is_finite())
                || !sphere.radius.is_finite()
                || sphere.radius <= 0.0
                || sphere.color.iter().any(|value| !value.is_finite())
            {
                return Err("scene.transient_spheres.set contains invalid sphere data".to_owned());
            }
        }
        self.transient_spheres = spheres;
        Ok(())
    }

    pub fn set_overlay_quads(&mut self, quads: Vec<SceneOverlayQuad>) -> Result<(), String> {
        if quads.len() > MAX_OVERLAY_QUADS {
            return Err(format!(
                "scene.overlay_quads.set exceeds the generic limit of {MAX_OVERLAY_QUADS}"
            ));
        }
        for quad in &quads {
            if quad.rect.iter().any(|value| !value.is_finite())
                || quad.color.iter().any(|value| !value.is_finite())
            {
                return Err("scene.overlay_quads.set contains non-finite data".to_owned());
            }
        }
        self.overlay_quads = quads;
        Ok(())
    }

    fn update_scene_world(&mut self, dt: f32) -> Result<(), String> {
        self.world.update_transform(
            SceneEntityId(self.camera_entity_id),
            SceneTransform {
                position: self.camera.position,
                rotation_degrees: Vec3::ZERO,
                scale: Vec3::ONE,
            },
        )?;
        self.world.pre_update(self.camera.position, dt);
        self.world.update();
        Ok(())
    }

    fn sync_runtime_camera_to_flecs(&self) -> Result<(), String> {
        let response = host_runtime::call_json(
            ECS_SERVICE,
            "command_json_v1",
            &json!({
                "commands": [{
                    "op": "set_component_json",
                    "entity_id": self.camera_entity_id,
                    "component_type": "newviso.camera.runtime",
                    "payload": {
                        "position": [
                            self.camera.position.x,
                            self.camera.position.y,
                            self.camera.position.z
                        ],
                        "target": [
                            self.camera.target.x,
                            self.camera.target.y,
                            self.camera.target.z
                        ],
                        "orbit": {
                            "yaw_radians": self.orbit.yaw,
                            "pitch_radians": self.orbit.pitch,
                            "distance": self.orbit.distance
                        }
                    }
                }]
            }),
        )?;

        let ok = response
            .get("results")
            .and_then(Value::as_array)
            .and_then(|results| results.first())
            .and_then(|result| result.get("ok"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !ok {
            return Err(format!("Flecs rejected runtime camera update: {response}"));
        }
        Ok(())
    }

    pub fn render_frame(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.render_frame_with_overlay(width, height, || Ok(()))
    }

    pub fn render_frame_with_overlay<F>(
        &mut self,
        width: u32,
        height: u32,
        overlay: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        let gpu = self
            .gpu
            .ok_or_else(|| "3D scene GPU resources are not initialized".to_owned())?;

        let width = width.max(1);
        let height = height.max(1);
        let aspect = width as f32 / height as f32;
        let forward = self.camera.target.sub(self.camera.position).normalized();
        self.frame_plan = self.world.scan_visibility(SceneView {
            position: self.camera.position,
            forward,
            up: self.camera.up,
            near: self.camera.near,
            far: self.camera.far,
            fov_y_radians: self.camera.fov_y_degrees.to_radians(),
            aspect,
        });
        let vertex_data = self.build_cube_vertices(aspect);
        let shadow_vertex_data = self.build_shadow_vertices();
        let (frame_uniform, shadow_enabled) =
            self.scene_frame_uniform(aspect, gpu.shadow_resolution);
        let render = RenderClient::new();

        render.write_buffer_f32(gpu.vertex_buffer, 0, &vertex_data)?;
        render.write_buffer_f32(gpu.shadow_vertex_buffer, 0, &shadow_vertex_data)?;
        render.write_buffer_f32(gpu.frame_uniform, 0, &frame_uniform)?;
        if let Some(gpu_sky) = self.gpu_sky {
            let sky_camera = self.sky_camera_uniform(width as f32 / height as f32);
            render.write_buffer_f32(gpu_sky.camera_uniform, 0, &sky_camera)?;
        }

        let frame_index = self.frame_index;
        if let Err(error) = self.render_frame_inner(
            &render,
            gpu,
            width,
            height,
            frame_index,
            shadow_enabled,
            overlay,
        ) {
            render.abort_frame();
            return Err(error);
        }

        self.frame_index = self.frame_index.wrapping_add(1);
        Ok(())
    }

    fn render_frame_inner<F>(
        &self,
        render: &RenderClient,
        gpu: GpuScene,
        width: u32,
        height: u32,
        frame_index: u64,
        shadow_enabled: bool,
        overlay: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        render.begin_frame(self.clear_color, frame_index)?;

        if shadow_enabled && self.shadow_vertex_count() > 0 {
            render.begin_render_target(
                gpu.shadow_render_target,
                Some([1.0, 1.0, 1.0, 1.0]),
                Some(1.0),
            )?;
            render.set_viewport(gpu.shadow_resolution, gpu.shadow_resolution)?;
            render.set_scissor(gpu.shadow_resolution, gpu.shadow_resolution)?;
            render.set_pipeline(gpu.shadow_pipeline)?;
            render.set_bind_group(0, gpu.shadow_bind_group)?;
            render.set_vertex_buffer(0, gpu.shadow_vertex_buffer, 0)?;
            render.draw(self.shadow_vertex_count())?;
            render.end_render_target()?;
        }

        render.set_viewport(width, height)?;
        render.set_scissor(width, height)?;

        if let Some(sky) = self.gpu_sky {
            render.set_pipeline(sky.pipeline)?;
            render.set_bind_group(0, sky.bind_group)?;
            render.set_vertex_buffer(0, sky.vertex_buffer, 0)?;
            render.set_index_buffer(sky.index_buffer, 0, sky.index_format)?;
            render.draw_indexed(sky.index_count)?;
        }

        render.set_pipeline(gpu.pipeline)?;
        render.set_bind_group(0, gpu.bind_group)?;
        render.set_vertex_buffer(0, gpu.vertex_buffer, 0)?;
        render.draw(self.vertex_count())?;
        overlay()?;
        render.end_frame()
    }

    pub fn shutdown_renderer(&mut self) {
        let render = RenderClient::new();
        if let Some(sky) = self.gpu_sky.take() {
            render.destroy_pipeline(sky.pipeline);
            render.destroy_shader(sky.fragment_shader);
            render.destroy_shader(sky.vertex_shader);
            render.destroy_bind_group(sky.bind_group);
            render.destroy_bind_group_layout(sky.bind_group_layout);
            render.destroy_sampler(sky.sampler);
            render.destroy_buffer(sky.camera_uniform);
            render.destroy_texture(sky.detail_noise);
            render.destroy_texture(sky.starfield);
            render.destroy_texture(sky.base_noise);
            render.destroy_buffer(sky.index_buffer);
            render.destroy_buffer(sky.vertex_buffer);
        }

        let Some(gpu) = self.gpu.take() else {
            return;
        };
        render.destroy_pipeline(gpu.shadow_pipeline);
        render.destroy_pipeline(gpu.pipeline);
        render.destroy_shader(gpu.shadow_fragment_shader);
        render.destroy_shader(gpu.shadow_vertex_shader);
        render.destroy_shader(gpu.fragment_shader);
        render.destroy_shader(gpu.vertex_shader);
        render.destroy_bind_group(gpu.shadow_bind_group);
        render.destroy_bind_group(gpu.bind_group);
        render.destroy_bind_group_layout(gpu.shadow_bind_group_layout);
        render.destroy_bind_group_layout(gpu.bind_group_layout);
        render.destroy_sampler(gpu.shadow_sampler);
        render.destroy_render_target(gpu.shadow_render_target);
        render.destroy_buffer(gpu.frame_uniform);
        render.destroy_buffer(gpu.shadow_vertex_buffer);
        render.destroy_buffer(gpu.vertex_buffer);
    }

    fn scene_frame_uniform(
        &self,
        aspect: f32,
        shadow_resolution: u32,
    ) -> ([f32; SCENE_FRAME_UNIFORM_FLOATS], bool) {
        let mut out = [0.0_f32; SCENE_FRAME_UNIFORM_FLOATS];
        let view_proj = camera_view_projection(&self.camera, aspect);
        out[0..16].copy_from_slice(&view_proj);

        let lights = self.world.active_lights();
        let light_count = lights.len().min(MAX_LIGHTS);
        let mut shadow_light_index: Option<usize> = None;
        let mut shadow_matrix = identity_matrix();
        let mut shadow_bias = 0.0015;
        let mut shadow_normal_bias = 0.02;

        for (index, (_, transform, light)) in lights.iter().take(MAX_LIGHTS).enumerate() {
            let direction = light_direction(transform.rotation_degrees);
            let type_code = match light.light_type {
                LightType::Directional => 0.0,
                LightType::Point => 1.0,
                LightType::Spot => 2.0,
                LightType::Area => 3.0,
            };

            let meta = 40 + index * 4;
            out[meta] = type_code;
            out[meta + 1] = light.intensity;
            out[meta + 2] = light.range.max(0.001);
            out[meta + 3] = if light.casts_shadows { 1.0 } else { 0.0 };

            let pos = 56 + index * 4;
            out[pos] = transform.position.x;
            out[pos + 1] = transform.position.y;
            out[pos + 2] = transform.position.z;
            out[pos + 3] = 1.0;

            let dir = 72 + index * 4;
            out[dir] = direction.x;
            out[dir + 1] = direction.y;
            out[dir + 2] = direction.z;

            let color = 88 + index * 4;
            out[color] = light.color[0];
            out[color + 1] = light.color[1];
            out[color + 2] = light.color[2];
            out[color + 3] = 1.0;

            let cone = 104 + index * 4;
            out[cone] = light.cone_inner_degrees.to_radians().cos();
            out[cone + 1] = light.cone_outer_degrees.to_radians().cos();

            if shadow_light_index.is_none() && light.casts_shadows {
                let candidate = match light.light_type {
                    LightType::Directional => Some(directional_shadow_view_projection(
                        direction,
                        self.camera.position,
                        self.camera.target.sub(self.camera.position).normalized(),
                        light.shadow_distance,
                    )),
                    LightType::Spot => Some(spot_shadow_view_projection(
                        transform.position,
                        direction,
                        light.cone_outer_degrees,
                        light.range.max(light.shadow_distance).max(1.0),
                    )),
                    LightType::Point | LightType::Area => None,
                };

                if let Some(matrix) = candidate {
                    shadow_light_index = Some(index);
                    shadow_matrix = matrix;
                    shadow_bias = light.shadow_bias;
                    shadow_normal_bias = light.shadow_normal_bias;
                }
            }
        }

        out[16..32].copy_from_slice(&shadow_matrix);
        out[32] = light_count as f32;
        out[33] = 0.16;
        out[34] = shadow_light_index.map(|index| index as f32).unwrap_or(-1.0);
        out[35] = if shadow_light_index.is_some() {
            1.0
        } else {
            0.0
        };
        out[36] = shadow_bias;
        out[37] = shadow_normal_bias;
        out[38] = shadow_resolution as f32;
        out[39] = 1.0;

        (out, shadow_light_index.is_some())
    }

    fn sky_camera_uniform(&self, aspect: f32) -> [f32; 16] {
        let forward = self.camera.target.sub(self.camera.position).normalized();
        let right = forward.cross(self.camera.up).normalized();
        let up = right.cross(forward).normalized();
        let inv_tan = 1.0
            / (self.camera.fov_y_degrees.to_radians() * 0.5)
                .tan()
                .max(0.0001);

        [
            right.x,
            right.y,
            right.z,
            0.0,
            up.x,
            up.y,
            up.z,
            0.0,
            forward.x,
            forward.y,
            forward.z,
            0.0,
            inv_tan / aspect.max(0.0001),
            inv_tan,
            0.0,
            0.0,
        ]
    }

    fn vertex_capacity(&self) -> u32 {
        self.cubes.len() as u32 * CUBE_VERTEX_COUNT
            + MAX_TRANSIENT_SPHERES as u32 * geometry::SPHERE_VERTEX_COUNT
            + MAX_OVERLAY_QUADS as u32 * 6
    }

    fn shadow_vertex_capacity(&self) -> u32 {
        self.cubes.len() as u32 * CUBE_VERTEX_COUNT
            + MAX_TRANSIENT_SPHERES as u32 * geometry::SPHERE_VERTEX_COUNT
    }

    fn vertex_count(&self) -> u32 {
        self.frame_plan.visible_render_slots.len() as u32 * CUBE_VERTEX_COUNT
            + self.transient_spheres.len() as u32 * geometry::SPHERE_VERTEX_COUNT
            + self.overlay_quads.len() as u32 * 6
    }

    fn shadow_vertex_count(&self) -> u32 {
        self.world.active_render_slots().len() as u32 * CUBE_VERTEX_COUNT
            + self.transient_spheres.len() as u32 * geometry::SPHERE_VERTEX_COUNT
    }

    fn build_shadow_vertices(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.shadow_vertex_count() as usize * FLOATS_PER_VERTEX);
        for slot in self.world.active_render_slots() {
            if let Some(cube) = self.cubes.get(slot) {
                cube.append_vertices(&mut out);
            }
        }
        for sphere in &self.transient_spheres {
            geometry::append_sphere_vertices(
                Vec3::new(sphere.position[0], sphere.position[1], sphere.position[2]),
                sphere.radius,
                sphere.color,
                &mut out,
            );
        }
        out
    }

    fn build_cube_vertices(&self, _aspect: f32) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.vertex_count() as usize * FLOATS_PER_VERTEX);
        for slot in &self.frame_plan.visible_render_slots {
            if let Some(cube) = self.cubes.get(*slot) {
                cube.append_vertices(&mut out);
            }
        }

        for sphere in &self.transient_spheres {
            geometry::append_sphere_vertices(
                Vec3::new(sphere.position[0], sphere.position[1], sphere.position[2]),
                sphere.radius,
                sphere.color,
                &mut out,
            );
        }

        for quad in &self.overlay_quads {
            let [x0, y0, x1, y1] = quad.rect;
            for [x, y] in [[x0, y0], [x1, y0], [x1, y1], [x0, y0], [x1, y1], [x0, y1]] {
                geometry::append_overlay_vertex(&mut out, x, y, quad.color);
            }
        }

        out
    }
}

fn identity_matrix() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn matrix_from_rows(rows: [[f32; 4]; 4]) -> [f32; 16] {
    [
        rows[0][0], rows[1][0], rows[2][0], rows[3][0], rows[0][1], rows[1][1], rows[2][1],
        rows[3][1], rows[0][2], rows[1][2], rows[2][2], rows[3][2], rows[0][3], rows[1][3],
        rows[2][3], rows[3][3],
    ]
}

fn view_basis(forward: Vec3, up_hint: Vec3) -> (Vec3, Vec3, Vec3) {
    let forward = forward.normalized();
    let fallback_up = if forward.dot(Vec3::Y).abs() > 0.98 {
        Vec3::new(0.0, 0.0, 1.0)
    } else {
        up_hint
    };
    let right = forward.cross(fallback_up).normalized();
    let up = right.cross(forward).normalized();
    (right, up, forward)
}

fn camera_view_projection(camera: &Camera, aspect: f32) -> [f32; 16] {
    perspective_view_projection(
        camera.position,
        camera.target.sub(camera.position).normalized(),
        camera.up,
        camera.fov_y_degrees,
        aspect,
        camera.near.max(0.001),
        camera.far.max(camera.near + 0.001),
    )
}

fn perspective_view_projection(
    eye: Vec3,
    forward: Vec3,
    up_hint: Vec3,
    fov_y_degrees: f32,
    aspect: f32,
    near: f32,
    far: f32,
) -> [f32; 16] {
    let (right, up, forward) = view_basis(forward, up_hint);
    let inv_tan = 1.0 / (fov_y_degrees.to_radians() * 0.5).tan().max(0.0001);
    let x_scale = inv_tan / aspect.max(0.0001);
    let y_scale = inv_tan;
    let depth_scale = far / (far - near);
    let depth_bias = -far * near / (far - near);

    matrix_from_rows([
        [
            right.x * x_scale,
            right.y * x_scale,
            right.z * x_scale,
            -right.dot(eye) * x_scale,
        ],
        [
            -up.x * y_scale,
            -up.y * y_scale,
            -up.z * y_scale,
            up.dot(eye) * y_scale,
        ],
        [
            forward.x * depth_scale,
            forward.y * depth_scale,
            forward.z * depth_scale,
            depth_bias - forward.dot(eye) * depth_scale,
        ],
        [forward.x, forward.y, forward.z, -forward.dot(eye)],
    ])
}

fn orthographic_view_projection(
    eye: Vec3,
    forward: Vec3,
    up_hint: Vec3,
    half_extent: f32,
    near: f32,
    far: f32,
) -> [f32; 16] {
    let (right, up, forward) = view_basis(forward, up_hint);
    let extent = half_extent.max(0.001);
    let depth_range = (far - near).max(0.001);
    let depth_scale = 1.0 / depth_range;

    matrix_from_rows([
        [
            right.x / extent,
            right.y / extent,
            right.z / extent,
            -right.dot(eye) / extent,
        ],
        [
            -up.x / extent,
            -up.y / extent,
            -up.z / extent,
            up.dot(eye) / extent,
        ],
        [
            forward.x * depth_scale,
            forward.y * depth_scale,
            forward.z * depth_scale,
            (-forward.dot(eye) - near) * depth_scale,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

fn light_direction(rotation_degrees: Vec3) -> Vec3 {
    transform_point(
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::ONE,
        rotation_degrees,
        Vec3::ZERO,
    )
    .normalized()
}

fn directional_shadow_view_projection(
    direction: Vec3,
    camera_position: Vec3,
    camera_forward: Vec3,
    shadow_distance: f32,
) -> [f32; 16] {
    let distance = shadow_distance.max(8.0);
    let center = camera_position.add(camera_forward.mul(distance * 0.35));
    let eye = center.sub(direction.normalized().mul(distance));
    orthographic_view_projection(
        eye,
        direction,
        Vec3::Y,
        distance * 0.55,
        0.1,
        distance * 2.0,
    )
}

fn spot_shadow_view_projection(
    position: Vec3,
    direction: Vec3,
    outer_cone_degrees: f32,
    range: f32,
) -> [f32; 16] {
    perspective_view_projection(
        position,
        direction,
        Vec3::Y,
        (outer_cone_degrees * 2.0).clamp(1.0, 175.0),
        1.0,
        0.05,
        range.max(0.1),
    )
}

fn read_f32(object: &Value, key: &str, default: f32) -> Result<f32, String> {
    let Some(value) = object.get(key) else {
        return Ok(default);
    };
    let value = value
        .as_f64()
        .ok_or_else(|| format!("'{key}' must be numeric"))? as f32;
    if !value.is_finite() {
        return Err(format!("'{key}' must be finite"));
    }
    Ok(value)
}

fn read_vec3(object: &Value, key: &str, default: Vec3) -> Result<Vec3, String> {
    let Some(value) = object.get(key) else {
        return Ok(default);
    };
    let array = value
        .as_array()
        .ok_or_else(|| format!("'{key}' must be a 3-element array"))?;
    if array.len() != 3 {
        return Err(format!("'{key}' must contain exactly 3 elements"));
    }

    let mut out = [0.0_f32; 3];
    for (index, item) in array.iter().enumerate() {
        let number =
            item.as_f64()
                .ok_or_else(|| format!("'{key}[{index}]' must be numeric"))? as f32;
        if !number.is_finite() {
            return Err(format!("'{key}[{index}]' must be finite"));
        }
        out[index] = number;
    }
    Ok(Vec3::new(out[0], out[1], out[2]))
}

fn read_color4(object: &Value, key: &str, default: [f32; 4]) -> Result<[f32; 4], String> {
    let Some(value) = object.get(key) else {
        return Ok(default);
    };
    let array = value
        .as_array()
        .ok_or_else(|| format!("'{key}' must be a 4-element array"))?;
    if array.len() != 4 {
        return Err(format!("'{key}' must contain exactly 4 elements"));
    }

    let mut result = [0.0; 4];
    for (index, item) in array.iter().enumerate() {
        let number =
            item.as_f64()
                .ok_or_else(|| format!("'{key}[{index}]' must be numeric"))? as f32;
        if !number.is_finite() {
            return Err(format!("'{key}[{index}]' must be finite"));
        }
        result[index] = number;
    }
    Ok(result)
}

fn visibility_module_from_name(name: &str) -> Option<VisibilityModule> {
    match name.trim().to_ascii_lowercase().as_str() {
        "debug" => Some(VisibilityModule::Debug),
        "camera" => Some(VisibilityModule::Camera),
        "script" => Some(VisibilityModule::Script),
        "gameplay" => Some(VisibilityModule::Gameplay),
        "frontend" => Some(VisibilityModule::Frontend),
        "vfx" => Some(VisibilityModule::Vfx),
        "world" => Some(VisibilityModule::World),
        "player" => Some(VisibilityModule::Player),
        _ => None,
    }
}

fn read_visibility_mask(record: &Value) -> VisibilityMask {
    let mut mask = VisibilityMask::default();
    let Some(visibility) = record.get("visibility").and_then(Value::as_object) else {
        return mask;
    };
    for (name, value) in visibility {
        if let (Some(module), Some(visible)) = (visibility_module_from_name(name), value.as_bool())
        {
            mask.set(module, visible);
        }
    }
    mask
}

fn read_lod_policy(record: &Value) -> Result<SceneLodPolicy, String> {
    let Some(lod) = record.get("lod") else {
        return Ok(SceneLodPolicy::default());
    };
    let lod = lod
        .as_object()
        .ok_or_else(|| "'lod' must be an object".to_owned())?;

    let read = |key: &str, default: f32| -> Result<f32, String> {
        let Some(value) = lod.get(key) else {
            return Ok(default);
        };
        let value = value
            .as_f64()
            .ok_or_else(|| format!("lod.{key} must be numeric"))? as f32;
        if !value.is_finite() || value < 0.0 {
            return Err(format!("lod.{key} must be a finite non-negative number"));
        }
        Ok(value)
    };

    let visible_distance = read("visible_distance", f32::INFINITY)?;
    let stream_distance = read("stream_distance", f32::INFINITY)?;
    let fade_range = read("fade_range", 0.0)?;
    if visible_distance.is_finite()
        && stream_distance.is_finite()
        && stream_distance < visible_distance
    {
        return Err("lod.stream_distance must be >= lod.visible_distance".to_owned());
    }
    Ok(SceneLodPolicy {
        visible_distance,
        stream_distance,
        fade_range,
    })
}

fn read_parent_id(record: &Value) -> Option<u64> {
    record
        .get("parent_id")
        .and_then(Value::as_u64)
        .or_else(|| record.pointer("/parent/stable_id").and_then(Value::as_u64))
        .or_else(|| record.get("parent").and_then(Value::as_u64))
}

fn upload_sky_texture(
    render: &RenderClient,
    label: &str,
    texture: &SkyTextureResources,
) -> Result<u32, String> {
    if texture.width == 0 || texture.height == 0 {
        return Err(format!("sky texture '{}' has zero extent", texture.name));
    }
    let expected = texture.width as usize * texture.height as usize * 4;
    if texture.rgba8.len() != expected {
        return Err(format!(
            "sky texture '{}' rgba8 byte length {} does not match {}x{} RGBA8 ({expected})",
            texture.name,
            texture.rgba8.len(),
            texture.width,
            texture.height
        ));
    }
    let mip = TextureMipUpload {
        level: 0,
        width: texture.width,
        height: texture.height,
        offset: 0,
        byte_len: texture.rgba8.len() as u64,
    };
    render.create_texture(
        label,
        texture.width,
        texture.height,
        if texture.srgb {
            "Rgba8Srgb"
        } else {
            "Rgba8Unorm"
        },
        &[mip],
        &texture.rgba8,
    )
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
        let distance = orbit.distance;
        orbit.apply_mouse(0.0, 0.0, 120.0, false);
        assert!(orbit.distance < distance);
        orbit.apply_mouse(0.0, 0.0, 100000.0, false);
        assert_eq!(orbit.distance, orbit.min_distance);
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
        assert_eq!(VERTEX_STRIDE, 44);
        assert_eq!(cube.bounds().min, [0.0, 0.0, -6.0]);
        assert_eq!(cube.bounds().max, [4.0, 2.0, 0.0]);
        assert_eq!(out[3], 0.0);
        assert_eq!(&out[4..7], &[0.0, 0.0, 1.0]);
        assert_eq!(&out[7..11], &[1.0, 1.0, 1.0, 1.0]);
    }
}
