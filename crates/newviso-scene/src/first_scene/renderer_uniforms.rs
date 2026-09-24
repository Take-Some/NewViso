use super::*;

impl Scene3dRuntime {
    pub(super) fn scene_frame_uniform(
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
                        light.shadow_distance,
                        shadow_resolution,
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
                    shadow_bias = match light.light_type {
                        LightType::Directional => {
                            light.shadow_bias / (light.shadow_distance * 2.0).max(1.0)
                        }
                        LightType::Spot => {
                            light.shadow_bias / light.range.max(light.shadow_distance).max(1.0)
                        }
                        LightType::Point | LightType::Area => light.shadow_bias,
                    };
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
    pub(super) fn sky_frame_uniform(&self, aspect: f32) -> [f32; SKY_UNIFORM_FLOATS] {
        let forward = self.camera.target.sub(self.camera.position).normalized();
        let right = forward.cross(self.camera.up).normalized();
        let up = right.cross(forward).normalized();
        let inv_tan = 1.0
            / (self.camera.fov_y_degrees.to_radians() * 0.5)
                .tan()
                .max(0.0001);

        let mut out = [0.0_f32; SKY_UNIFORM_FLOATS];
        out[0..16].copy_from_slice(&[
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
        ]);

        let mut count = 0usize;
        for (key, visual) in &self.sky_visuals {
            if count >= MAX_SKY_VISUALS {
                break;
            }
            let Some(id) = self
                .runtime_entity_ids
                .get(key)
                .copied()
                .map(SceneEntityId)
                .or_else(|| self.world.entity_id_by_name(key))
            else {
                continue;
            };
            let Some(entity) = self.world.entity(id) else {
                continue;
            };
            if entity.lifecycle != SceneLifecycle::Active {
                continue;
            }

            let direction = sky_visual_direction(entity.transform.rotation_degrees);
            let dir_offset = 20 + count * 4;
            out[dir_offset] = direction.x;
            out[dir_offset + 1] = direction.y;
            out[dir_offset + 2] = direction.z;
            out[dir_offset + 3] = visual.angular_size_degrees;

            let color_offset = 36 + count * 4;
            out[color_offset] = visual.color[0];
            out[color_offset + 1] = visual.color[1];
            out[color_offset + 2] = visual.color[2];
            out[color_offset + 3] = visual.intensity;

            let halo_offset = 52 + count * 4;
            out[halo_offset] = visual.halo_size_degrees;
            out[halo_offset + 1] = visual.halo_intensity;
            out[halo_offset + 2] = match visual.kind {
                SkyVisualKind::Disc => 0.0,
                SkyVisualKind::Billboard => 1.0,
            };

            count += 1;
        }

        out[16] = count as f32;
        out
    }
}
