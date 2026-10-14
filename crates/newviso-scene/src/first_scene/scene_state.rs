use super::*;

impl Scene3dRuntime {
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn set_clear_color(&mut self, clear_color: [f32; 4]) {
        self.clear_color = clear_color;
    }

    pub(super) fn update_scene_world(&mut self, dt: f32) -> Result<(), String> {
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
    pub(super) fn sync_runtime_camera_to_flecs(&self) -> Result<(), String> {
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
}

