use super::*;

fn write_sky_vec4(out: &mut [f32; SKY_UNIFORM_FLOATS], offset: usize, color: [f32; 3], w: f32) {
    out[offset] = color[0];
    out[offset + 1] = color[1];
    out[offset + 2] = color[2];
    out[offset + 3] = w;
}

type SceneLightEntry = (SceneEntityId, SceneTransform, LightComponent);

fn light_influence_score(camera: &Camera, entry: &SceneLightEntry) -> f32 {
    let (_, transform, light) = entry;
    match light.light_type {
        LightType::Directional => f32::INFINITY,
        LightType::Point | LightType::Spot | LightType::Area => {
            let delta = transform.position.sub(camera.position);
            let distance_sq = delta.dot(delta).max(0.01);
            light.intensity.max(0.0) * light.range.max(0.001) / distance_sq
        }
    }
}

/// Conservative pre-admission for the fixed forward-light budget.
///
/// A local light farther than camera far distance plus its range cannot overlap
/// any point inside the camera far sphere, regardless of orientation. This is a
/// cheap world-space first stage; screen/tile classification can be layered on
/// later by the render backend.
fn light_can_affect_camera_volume(camera: &Camera, entry: &SceneLightEntry) -> bool {
    let (_, transform, light) = entry;
    if light.light_type == LightType::Directional {
        return true;
    }
    let delta = transform.position.sub(camera.position);
    let max_distance = camera.far.max(0.0) + light.range.max(0.0);
    delta.dot(delta) <= max_distance * max_distance
}

fn select_frame_lights(camera: &Camera, mut lights: Vec<SceneLightEntry>) -> Vec<SceneLightEntry> {
    lights.retain(|entry| light_can_affect_camera_volume(camera, entry));
    lights.sort_unstable_by(|a, b| {
        light_influence_score(camera, b)
            .partial_cmp(&light_influence_score(camera, a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0 .0.cmp(&b.0 .0))
    });
    lights.truncate(MAX_LIGHTS);
    lights
}

impl Scene3dRuntime {
    pub(super) fn renderer_local_lights(&self) -> Vec<RenderLight> {
        let mut lights = self.world.active_lights();
        lights.retain(|entry| {
            light_can_affect_camera_volume(&self.camera, entry)
                && !matches!(entry.2.light_type, LightType::Directional)
        });
        lights.sort_unstable_by(|a, b| {
            light_influence_score(&self.camera, b)
                .partial_cmp(&light_influence_score(&self.camera, a))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0 .0.cmp(&b.0 .0))
        });

        lights
            .into_iter()
            .take(1024)
            .map(|(_, transform, light)| {
                let direction = light_direction(transform.rotation_degrees);
                let kind = match light.light_type {
                    LightType::Directional => RenderLightKind::Directional,
                    LightType::Point => RenderLightKind::Point,
                    LightType::Spot => RenderLightKind::Spot,
                    LightType::Area => RenderLightKind::Area,
                };
                RenderLight::local(
                    kind,
                    [
                        transform.position.x,
                        transform.position.y,
                        transform.position.z,
                    ],
                    [direction.x, direction.y, direction.z],
                    light.color,
                    light.intensity.max(0.0),
                    light.range.max(0.001),
                    light.cone_inner_degrees.to_radians().cos(),
                    light.cone_outer_degrees.to_radians().cos(),
                    light.casts_shadows,
                )
            })
            .collect()
    }

    pub(super) fn scene_frame_uniform(
        &self,
        aspect: f32,
        shadow_resolution: u32,
    ) -> ([f32; SCENE_FRAME_UNIFORM_FLOATS], bool) {
        let mut out = [0.0_f32; SCENE_FRAME_UNIFORM_FLOATS];
        let view_proj = camera_view_projection(&self.camera, aspect);
        out[0..16].copy_from_slice(&view_proj);

        // Directional lights are global and must never be evicted. Local
        // lights first pass a conservative influence-volume test, then the
        // fixed forward-light budget keeps only the strongest camera-relative
        // contributors.
        let lights = select_frame_lights(&self.camera, self.world.active_lights());
        let light_count = lights.len();
        let mut shadow_light_index: Option<usize> = None;
        let mut shadow_matrix = identity_matrix();
        let mut shadow_bias = 0.0015;
        let mut shadow_normal_bias = 0.02;

        let meta_base = 40;
        let pos_base = meta_base + MAX_LIGHTS * 4;
        let dir_base = pos_base + MAX_LIGHTS * 4;
        let color_base = dir_base + MAX_LIGHTS * 4;
        let cone_base = color_base + MAX_LIGHTS * 4;
        let camera_base = cone_base + MAX_LIGHTS * 4;
        let ambient_base = camera_base + 4;
        let fog_color_base = ambient_base + 4;
        let fog_params_base = fog_color_base + 4;
        let haze_color_base = fog_params_base + 4;
        let haze_params_base = haze_color_base + 4;
        let clear_color_base = haze_params_base + 4;

        for (index, (_, transform, light)) in lights.iter().enumerate() {
            let direction = light_direction(transform.rotation_degrees);
            let type_code = match light.light_type {
                LightType::Directional => 0.0,
                LightType::Point => 1.0,
                LightType::Spot => 2.0,
                LightType::Area => 3.0,
            };

            let meta = meta_base + index * 4;
            out[meta] = type_code;
            out[meta + 1] = light.intensity;
            out[meta + 2] = light.range.max(0.001);
            out[meta + 3] = if light.casts_shadows { 1.0 } else { 0.0 };

            let pos = pos_base + index * 4;
            out[pos] = transform.position.x;
            out[pos + 1] = transform.position.y;
            out[pos + 2] = transform.position.z;
            out[pos + 3] = 1.0;

            let dir = dir_base + index * 4;
            out[dir] = direction.x;
            out[dir + 1] = direction.y;
            out[dir + 2] = direction.z;

            let color = color_base + index * 4;
            out[color] = light.color[0];
            out[color + 1] = light.color[1];
            out[color + 2] = light.color[2];
            out[color + 3] = 1.0;

            let cone = cone_base + index * 4;
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

        out[camera_base] = self.camera.position.x;
        out[camera_base + 1] = self.camera.position.y;
        out[camera_base + 2] = self.camera.position.z;
        out[camera_base + 3] = 1.0;

        out[ambient_base] = self.scene_environment.ambient_color[0];
        out[ambient_base + 1] = self.scene_environment.ambient_color[1];
        out[ambient_base + 2] = self.scene_environment.ambient_color[2];
        out[ambient_base + 3] = self.scene_environment.ambient_intensity;

        out[fog_color_base] = self.scene_environment.fog_color[0];
        out[fog_color_base + 1] = self.scene_environment.fog_color[1];
        out[fog_color_base + 2] = self.scene_environment.fog_color[2];
        out[fog_color_base + 3] = if self.scene_environment.fog_enabled {
            self.scene_environment.fog_density
        } else {
            0.0
        };

        out[fog_params_base] = self.scene_environment.fog_start_distance;
        out[fog_params_base + 1] = self.scene_environment.fog_height_falloff;
        out[fog_params_base + 2] = self.scene_environment.fog_base_height;
        out[fog_params_base + 3] = self.scene_environment.fog_max_opacity;

        out[haze_color_base] = self.scene_environment.haze_color[0];
        out[haze_color_base + 1] = self.scene_environment.haze_color[1];
        out[haze_color_base + 2] = self.scene_environment.haze_color[2];
        out[haze_color_base + 3] = self.scene_environment.haze_density;
        out[haze_params_base] = self.scene_environment.haze_start_distance;

        out[clear_color_base..clear_color_base + 4].copy_from_slice(&self.clear_color);

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

        // GTA-style motion contract:
        // xy = integrated wind-driven large-cloud phase,
        // z = continuous time-cycle in days, w = noise phase scale.
        out[176] = self.sky_cloud_noise_phase[0];
        out[177] = self.sky_cloud_noise_phase[1];
        out[178] = self.sky_cloud_cycle_time_days;
        out[179] = self.sky_clouds.noise_phase_scale;

        // Large speed is consumed on CPU while the three independent detail
        // speeds reproduce GTA's speedConstants phase channels in shader.
        out[180] = self.sky_clouds.small_speed;
        out[181] = self.sky_clouds.overall_detail_speed;
        out[182] = self.sky_clouds.edge_detail_speed;
        out[183] = self.sky_clouds.large_speed;

        let (dome_scale, horizon_level) = self
            .sky
            .as_ref()
            .map(|sky| (sky.dome_scale, sky.horizon_level))
            .unwrap_or((20_000.0, 0.0));
        out[184] = dome_scale;
        out[185] = horizon_level;
        out[186] = self.camera.position.y;
        out[187] = 0.0;
        out
    }
}

#[cfg(test)]
mod light_selection_tests {
    use super::*;

    fn camera() -> Camera {
        Camera {
            position: Vec3::ZERO,
            target: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::Y,
            fov_y_degrees: 60.0,
            near: 0.1,
            far: 100.0,
        }
    }

    fn point(id: u64, z: f32, range: f32, intensity: f32) -> SceneLightEntry {
        (
            SceneEntityId(id),
            SceneTransform {
                position: Vec3::new(0.0, 0.0, z),
                rotation_degrees: Vec3::ZERO,
                scale: Vec3::ONE,
            },
            LightComponent {
                light_type: LightType::Point,
                color: [1.0, 1.0, 1.0],
                intensity,
                range,
                cone_inner_degrees: 0.0,
                cone_outer_degrees: 45.0,
                casts_shadows: false,
                shadow_bias: 0.001,
                shadow_normal_bias: 0.01,
                shadow_resolution: 512,
                shadow_distance: 50.0,
            },
        )
    }

    #[test]
    fn local_light_outside_camera_volume_is_rejected() {
        let cam = camera();
        assert!(light_can_affect_camera_volume(
            &cam,
            &point(1, -80.0, 25.0, 1.0)
        ));
        assert!(!light_can_affect_camera_volume(
            &cam,
            &point(2, -140.0, 10.0, 1.0)
        ));
    }

    #[test]
    fn frame_light_selection_keeps_strongest_local_contributors() {
        let cam = camera();
        let mut lights = Vec::new();
        for i in 0..(MAX_LIGHTS + 8) {
            lights.push(point(i as u64, -10.0, 30.0, (i + 1) as f32));
        }
        let selected = select_frame_lights(&cam, lights);
        assert_eq!(selected.len(), MAX_LIGHTS);
        assert!(selected[0].2.intensity > selected[MAX_LIGHTS - 1].2.intensity);
    }
}
