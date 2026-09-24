use super::*;

impl Scene3dRuntime {
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn set_clear_color(&mut self, clear_color: [f32; 4]) {
        self.clear_color = clear_color;
    }
    pub fn solid_aabbs(&self) -> Vec<([f32; 3], [f32; 3])> {
        self.world
            .solid_bounds()
            .map(|bounds| {
                (
                    [bounds.min.x, bounds.min.y, bounds.min.z],
                    [bounds.max.x, bounds.max.y, bounds.max.z],
                )
            })
            .collect()
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
                "sky_visuals": {
                    "count": self.sky_visuals.len(),
                    "ids": self.sky_visuals.keys().cloned().collect::<Vec<_>>()
                },
                "lens_flares": {
                    "count": self.lens_flares.len(),
                    "ids": self.lens_flares.keys().cloned().collect::<Vec<_>>()
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
                || sphere
                    .rotation_degrees
                    .iter()
                    .any(|value| !value.is_finite())
                || !sphere.radius.is_finite()
                || sphere.radius <= 0.0
                || sphere.color.iter().any(|value| !value.is_finite())
                || sphere
                    .marker_color
                    .is_some_and(|color| color.iter().any(|value| !value.is_finite()))
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
