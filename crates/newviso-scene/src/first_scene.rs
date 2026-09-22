use crate::{
    camera::{world_to_ndc, Camera, OrbitCamera},
    math::{transform_point, Vec3},
};
use newviso_host as host_runtime;
use newviso_render_client::{
    GraphicsPipelineDesc, RenderClient, ShaderStage, VertexAttribute, VertexFormat,
};
use serde_json::{json, Value};

const SCENE_SERVICE: &str = "engine.scene";
const INPUT_SERVICE: &str = "engine.input";
const ECS_SERVICE: &str = "engine.ecs";
const PRIMARY_MOUSE_BUTTON: u64 = 1;

const VERTEX_SHADER_PATH: &str = "shaders/game_debug_lines.vert";
const FRAGMENT_SHADER_PATH: &str = "shaders/game_debug_lines.frag";

const CUBE_VERTEX_COUNT: u32 = 36;
const FLOATS_PER_VERTEX: usize = 7;
const VERTEX_STRIDE: u64 = (FLOATS_PER_VERTEX * std::mem::size_of::<f32>()) as u64;
const VERTEX_BUFFER_BYTES: u64 =
    CUBE_VERTEX_COUNT as u64 * FLOATS_PER_VERTEX as u64 * std::mem::size_of::<f32>() as u64;

#[derive(Clone, Debug)]
struct Cube {
    position: Vec3,
    rotation_degrees: Vec3,
    scale: Vec3,
    base_color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
struct GpuScene {
    vertex_buffer: u32,
    vertex_shader: u32,
    fragment_shader: u32,
    pipeline: u32,
}

#[derive(Debug)]
pub struct Scene3dRuntime {
    title: String,
    camera_entity_id: u64,
    camera: Camera,
    orbit: OrbitCamera,
    cube: Cube,
    gpu: Option<GpuScene>,
    frame_index: u64,
}

#[derive(Clone, Debug)]
pub struct Scene3dLoadReport {
    pub title: String,
    pub entity_count: usize,
    pub camera_name: String,
    pub mesh_name: String,
}

impl Scene3dRuntime {
    pub fn load_first_scene() -> Result<(Self, Scene3dLoadReport), String> {
        let scene = json!({
            "schema": "newviso.scene.v1",
            "version": 1,
            "title": "NewViso First 3D Scene",
            "entities": [
                {
                    "kind": "camera",
                    "name": "MainCamera",
                    "transform": {
                        "position": [4.2, 3.0, 6.0],
                        "target": [0.0, 0.0, 0.0],
                        "up": [0.0, 1.0, 0.0]
                    },
                    "camera": {
                        "projection": "perspective",
                        "fov_y_degrees": 58.0,
                        "near": 0.1,
                        "far": 100.0
                    }
                },
                {
                    "kind": "mesh",
                    "name": "Cube",
                    "transform": {
                        "position": [0.0, 0.0, 0.0],
                        "rotation_degrees": [18.0, 32.0, 0.0],
                        "scale": [1.45, 1.45, 1.45]
                    },
                    "mesh": {
                        "primitive": "cube"
                    },
                    "material": {
                        "base_color": [0.95, 0.42, 0.12, 1.0]
                    }
                }
            ]
        });

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
        let mut mesh_record: Option<&Value> = None;

        for entity in entities {
            let Some(record) = entity
                .get("components")
                .and_then(|components| components.get("newengine.scene.entity"))
            else {
                continue;
            };

            match record.get("kind").and_then(Value::as_str) {
                Some("camera") if camera_record.is_none() => {
                    camera_record = Some(record);
                    camera_entity_id = entity
                        .get("handle")
                        .and_then(|handle| handle.get("stable_id"))
                        .and_then(Value::as_u64);
                }
                Some("mesh") if mesh_record.is_none() => mesh_record = Some(record),
                _ => {}
            }
        }

        let camera_record =
            camera_record.ok_or_else(|| "NewViso scene snapshot has no camera".to_owned())?;
        let mesh_record =
            mesh_record.ok_or_else(|| "NewViso scene snapshot has no mesh".to_owned())?;
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

        let mesh_transform = mesh_record
            .get("transform")
            .ok_or_else(|| "Cube has no transform".to_owned())?;
        let material = mesh_record
            .get("material")
            .ok_or_else(|| "Cube has no material".to_owned())?;

        let primitive = mesh_record
            .get("mesh")
            .and_then(|mesh| mesh.get("primitive"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if primitive != "cube" {
            return Err(format!(
                "first NewViso scene expects primitive='cube', got '{primitive}'"
            ));
        }

        let orbit = OrbitCamera::from_camera(&camera);

        let cube = Cube {
            position: read_vec3(mesh_transform, "position", Vec3::ZERO)?,
            rotation_degrees: read_vec3(mesh_transform, "rotation_degrees", Vec3::ZERO)?,
            scale: read_vec3(mesh_transform, "scale", Vec3::ONE)?,
            base_color: read_color4(material, "base_color", [0.95, 0.42, 0.12, 1.0])?,
        };

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
                cube,
                gpu: None,
                frame_index: 0,
            },
            report,
        ))
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn initialize_renderer(&mut self) -> Result<(), String> {
        std::env::set_var("NEWENGINE_SHADER_EMERGENCY_FALLBACK", "1");
        let render = RenderClient::new();

        let vertex_buffer = render.create_buffer(
            "newviso.first_scene.vertices",
            VERTEX_BUFFER_BYTES,
            "Vertex",
            "CpuToGpu",
        )?;

        let vertex_shader = render.create_shader(
            "newviso.first_scene.vertex",
            ShaderStage::Vertex,
            VERTEX_SHADER_PATH,
            "newviso/first_scene",
        )?;

        let fragment_shader = render.create_shader(
            "newviso.first_scene.fragment",
            ShaderStage::Fragment,
            FRAGMENT_SHADER_PATH,
            "newviso/first_scene",
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
            color_format: "Bgra8Unorm",
            depth_format: "Depth32Float",
            cull_mode: "None",
            blend_mode: "Opaque",
            cache_key: "newviso.first_scene.v1",
        })?;

        self.gpu = Some(GpuScene {
            vertex_buffer,
            vertex_shader,
            fragment_shader,
            pipeline,
        });

        host_runtime::info(
            "newviso.scene",
            format!(
                "GPU scene ready buffer={} vs={} fs={} pipeline={}",
                vertex_buffer, vertex_shader, fragment_shader, pipeline
            ),
        );
        Ok(())
    }

    pub fn update_input(&mut self) -> Result<(), String> {
        let bytes = host_runtime::call_service(INPUT_SERVICE, "state_json", &[])?;
        let input: Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("engine.input/state_json returned invalid JSON: {error}"))?;

        let mouse = input
            .get("mouse")
            .ok_or_else(|| format!("engine.input snapshot has no mouse state: {input}"))?;
        let dx = mouse
            .get("delta")
            .and_then(|delta| delta.get("x"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32;
        let dy = mouse
            .get("delta")
            .and_then(|delta| delta.get("y"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32;
        let wheel_y = mouse
            .get("wheel")
            .and_then(|wheel| wheel.get("y"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32;
        let rotating = mouse
            .get("down")
            .and_then(Value::as_array)
            .is_some_and(|buttons| {
                buttons
                    .iter()
                    .filter_map(Value::as_u64)
                    .any(|button| button == PRIMARY_MOUSE_BUTTON)
            });

        if (dx != 0.0 || dy != 0.0) && rotating || wheel_y != 0.0 {
            self.orbit.apply_mouse(dx, dy, wheel_y, rotating);
            self.camera.position = self.orbit.position(self.camera.target);
            self.sync_runtime_camera_to_flecs()?;
        }

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
        let gpu = self
            .gpu
            .ok_or_else(|| "3D scene GPU resources are not initialized".to_owned())?;

        let width = width.max(1);
        let height = height.max(1);
        let vertex_data = self.build_cube_vertices(width as f32 / height as f32);
        let render = RenderClient::new();

        render.write_buffer_f32(gpu.vertex_buffer, 0, &vertex_data)?;

        let frame_index = self.frame_index;
        if let Err(error) = self.render_frame_inner(&render, gpu, width, height, frame_index) {
            render.abort_frame();
            return Err(error);
        }

        self.frame_index = self.frame_index.wrapping_add(1);
        Ok(())
    }

    fn render_frame_inner(
        &self,
        render: &RenderClient,
        gpu: GpuScene,
        width: u32,
        height: u32,
        frame_index: u64,
    ) -> Result<(), String> {
        render.begin_frame([0.025, 0.032, 0.045, 1.0], frame_index)?;
        render.set_viewport(width, height)?;
        render.set_scissor(width, height)?;
        render.set_pipeline(gpu.pipeline)?;
        render.set_vertex_buffer(0, gpu.vertex_buffer, 0)?;
        render.draw(CUBE_VERTEX_COUNT)?;
        render.end_frame()
    }

    pub fn shutdown_renderer(&mut self) {
        let Some(gpu) = self.gpu.take() else {
            return;
        };

        let render = RenderClient::new();
        render.destroy_pipeline(gpu.pipeline);
        render.destroy_shader(gpu.fragment_shader);
        render.destroy_shader(gpu.vertex_shader);
        render.destroy_buffer(gpu.vertex_buffer);
    }

    fn build_cube_vertices(&self, aspect: f32) -> Vec<f32> {
        let rotation = self.cube.rotation_degrees;

        let corners = [
            Vec3::new(-0.5, -0.5, -0.5),
            Vec3::new(0.5, -0.5, -0.5),
            Vec3::new(0.5, 0.5, -0.5),
            Vec3::new(-0.5, 0.5, -0.5),
            Vec3::new(-0.5, -0.5, 0.5),
            Vec3::new(0.5, -0.5, 0.5),
            Vec3::new(0.5, 0.5, 0.5),
            Vec3::new(-0.5, 0.5, 0.5),
        ];

        // Six faces, two triangles per face.
        let faces: [([usize; 6], f32); 6] = [
            ([4, 5, 6, 4, 6, 7], 1.00),
            ([1, 0, 3, 1, 3, 2], 0.72),
            ([0, 4, 7, 0, 7, 3], 0.82),
            ([5, 1, 2, 5, 2, 6], 0.92),
            ([3, 7, 6, 3, 6, 2], 1.08),
            ([0, 1, 5, 0, 5, 4], 0.62),
        ];

        let mut out = Vec::with_capacity(CUBE_VERTEX_COUNT as usize * FLOATS_PER_VERTEX);
        for (indices, shade) in faces {
            for index in indices {
                let local = corners[index];
                let world = transform_point(local, self.cube.scale, rotation, self.cube.position);
                let ndc = world_to_ndc(world, &self.camera, aspect);

                out.extend_from_slice(&[
                    ndc.x,
                    ndc.y,
                    ndc.z,
                    (self.cube.base_color[0] * shade).min(1.0),
                    (self.cube.base_color[1] * shade).min(1.0),
                    (self.cube.base_color[2] * shade).min(1.0),
                    self.cube.base_color[3],
                ]);
            }
        }
        out
    }
}

impl Drop for Scene3dRuntime {
    fn drop(&mut self) {
        self.shutdown_renderer();
    }
}

fn read_f32(object: &Value, key: &str, default: f32) -> Result<f32, String> {
    let Some(value) = object.get(key) else {
        return Ok(default);
    };
    value
        .as_f64()
        .map(|value| value as f32)
        .ok_or_else(|| format!("'{key}' must be numeric"))
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

    Ok(Vec3::new(
        array[0]
            .as_f64()
            .ok_or_else(|| format!("'{key}[0]' must be numeric"))? as f32,
        array[1]
            .as_f64()
            .ok_or_else(|| format!("'{key}[1]' must be numeric"))? as f32,
        array[2]
            .as_f64()
            .ok_or_else(|| format!("'{key}[2]' must be numeric"))? as f32,
    ))
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
        result[index] =
            item.as_f64()
                .ok_or_else(|| format!("'{key}[{index}]' must be numeric"))? as f32;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_projects_origin_inside_clip_space() {
        let camera = Camera {
            position: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y_degrees: 60.0,
            near: 0.1,
            far: 100.0,
        };

        let p = world_to_ndc(Vec3::ZERO, &camera, 16.0 / 9.0);
        assert!(p.x.abs() < 0.001);
        assert!(p.y.abs() < 0.001);
        assert!(p.z > 0.0 && p.z < 1.0);
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
    fn cube_buffer_size_matches_protocol() {
        assert_eq!(
            VERTEX_BUFFER_BYTES,
            CUBE_VERTEX_COUNT as u64 * VERTEX_STRIDE
        );
    }
}
