use super::*;

fn write_sky_vec4(out: &mut [f32; SKY_UNIFORM_FLOATS], offset: usize, color: [f32; 3], w: f32) {
    out[offset] = color[0];
    out[offset + 1] = color[1];
    out[offset + 2] = color[2];
    out[offset + 3] = w;
}

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
        out[33] = self.scene_environment.ambient_intensity;
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

        out[120] = self.camera.position.x;
        out[121] = self.camera.position.y;
        out[122] = self.camera.position.z;
        out[123] = 1.0;

        out[124] = self.scene_environment.ambient_color[0];
        out[125] = self.scene_environment.ambient_color[1];
        out[126] = self.scene_environment.ambient_color[2];
        out[127] = self.scene_environment.ambient_intensity;

        out[128] = self.scene_environment.fog_color[0];
        out[129] = self.scene_environment.fog_color[1];
        out[130] = self.scene_environment.fog_color[2];
        out[131] = if self.scene_environment.fog_enabled {
            self.scene_environment.fog_density
        } else {
            0.0
        };

        out[132] = self.scene_environment.fog_start_distance;
        out[133] = self.scene_environment.fog_height_falloff;
        out[134] = self.scene_environment.fog_base_height;
        out[135] = self.scene_environment.fog_max_opacity;

        out[136] = self.scene_environment.haze_color[0];
        out[137] = self.scene_environment.haze_color[1];
        out[138] = self.scene_environment.haze_color[2];
        out[139] = self.scene_environment.haze_density;
        out[140] = self.scene_environment.haze_start_distance;
        out[141] = 0.0;
        out[142] = 0.0;
        out[143] = 0.0;

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
            out[halo_offset + 3] = if visual.atmosphere_driver { 1.0 } else { 0.0 };

            count += 1;
        }

        out[16] = count as f32;
        out[17] = self.sky_time_seconds;
        out[18] = self.sky_clouds.horizon_fade;
        out[19] = if self.sky_clouds.enabled { 1.0 } else { 0.0 };

        out[68] = self.sky_clouds.coverage;
        out[69] = self.sky_clouds.density;
        out[70] = self.sky_clouds.softness;
        out[71] = self.sky_clouds.scale;
        out[72] = self.sky_clouds.speed[0];
        out[73] = self.sky_clouds.speed[1];
        out[74] = self.sky_clouds.detail_scale;
        out[75] = 0.0;

        let atmosphere = self.sky_atmosphere;
        out[76..80].copy_from_slice(&atmosphere.twilight_altitudes);
        out[80] = atmosphere.daylight_altitudes[0];
        out[81] = atmosphere.daylight_altitudes[1];
        out[82] = atmosphere.horizon_power;
        out[83] = atmosphere.tonemap_shoulder;

        write_sky_vec4(&mut out, 84, atmosphere.night_zenith, 0.0);
        write_sky_vec4(&mut out, 88, atmosphere.night_horizon, 0.0);
        write_sky_vec4(&mut out, 92, atmosphere.astronomical_zenith, 0.0);
        write_sky_vec4(&mut out, 96, atmosphere.astronomical_horizon, 0.0);
        write_sky_vec4(&mut out, 100, atmosphere.nautical_zenith, 0.0);
        write_sky_vec4(&mut out, 104, atmosphere.nautical_horizon, 0.0);
        write_sky_vec4(&mut out, 108, atmosphere.civil_zenith, 0.0);
        write_sky_vec4(&mut out, 112, atmosphere.civil_horizon, 0.0);
        write_sky_vec4(&mut out, 116, atmosphere.day_zenith, 0.0);
        write_sky_vec4(&mut out, 120, atmosphere.day_horizon, 0.0);
        write_sky_vec4(
            &mut out,
            124,
            atmosphere.sunset_tint,
            atmosphere.sunset_strength,
        );
        write_sky_vec4(&mut out, 128, atmosphere.cloud_night, 0.0);
        write_sky_vec4(&mut out, 132, atmosphere.cloud_twilight_shadow, 0.0);
        write_sky_vec4(&mut out, 136, atmosphere.cloud_twilight_light, 0.0);
        write_sky_vec4(&mut out, 140, atmosphere.cloud_day_shadow, 0.0);
        write_sky_vec4(&mut out, 144, atmosphere.cloud_day_light, 0.0);
        write_sky_vec4(
            &mut out,
            148,
            atmosphere.star_tint,
            atmosphere.star_intensity,
        );
        out[152] = atmosphere.star_visibility_altitudes[0];
        out[153] = atmosphere.star_visibility_altitudes[1];
        out[154] = atmosphere.cloud_occlusion;
        out[155] = 0.0;
        write_sky_vec4(
            &mut out,
            156,
            atmosphere.silver_lining_tint,
            atmosphere.silver_lining_strength,
        );
        out[160] = atmosphere.cloud_alpha_range[0];
        out[161] = atmosphere.cloud_alpha_range[1];
        out[162] = 0.0;
        out[163] = 0.0;

        out[164] = self.sky_clouds.macro_scale;
        out[165] = self.sky_clouds.macro_strength;
        out[166] = self.sky_clouds.detail_strength;
        out[167] = self.sky_clouds.micro_strength;

        out[168] = self.sky_clouds.erosion_strength;
        out[169] = self.sky_clouds.warp_strength;
        out[170] = self.sky_clouds.shape_contrast;
        out[171] = 0.0;

        out[172] = self.sky_clouds.shear_speed[0];
        out[173] = self.sky_clouds.shear_speed[1];
        out[174] = self.sky_clouds.seed_offset[0];
        out[175] = self.sky_clouds.seed_offset[1];
        out
    }
}
