use super::*;

fn rotation(direction: [f32; 3]) -> [f32; 3] {
    [
        -direction[1].clamp(-1.0, 1.0).asin().to_degrees(),
        direction[0].atan2(direction[2]).to_degrees(),
        0.0,
    ]
}
fn linear(color: [f32; 3], intensity: f32) -> [f32; 3] {
    color.map(|c| (c.max(0.0) * c.max(0.0) * intensity.max(0.0)).clamp(0.0, 64.0))
}

// VARTYPE_COL*_LIN values in TimeCycle XML are already linear.
fn scaled(color: [f32; 3], intensity: f32) -> [f32; 3] {
    color.map(|c| (c.max(0.0) * intensity.max(0.0)).clamp(0.0, 64.0))
}

impl ClimateRuntime {
    pub(crate) fn present(&mut self, scene: &mut Scene3dRuntime, dt: f32) -> Result<(), String> {
        self.frame = self.timecycles.evaluate(
            self.clock.hours(),
            self.weather.weights(),
            &self.policy.region,
            self.policy.region_blend,
        )?;
        self.modifier_catalog.fill_defaults(&mut self.frame);
        for modifier in self.modifiers.values() {
            self.frame.apply_modifier(modifier)?;
        }
        let f = &self.frame;
        let current = self.weather.current.as_str();
        let next = self.weather.next.as_str();
        let blend = self.weather.blend;
        let mut effects = application_weather::resolve_weather_effects_from_catalog(
            &self.weather_catalog,
            current,
            next,
            blend,
            scene.weather_effects(),
        )?
        .ok_or("unresolved climate weather profile")?;
        let mut profiles = Vec::new();
        for (id, weight) in self.weather.weights() {
            if *weight > 0.0 {
                let profile = application_weather::resolve_weather_effects_from_catalog(
                    &self.weather_catalog,
                    id,
                    id,
                    0.0,
                    scene.weather_effects(),
                )?
                .ok_or("unknown climate profile")?;
                profiles.push((profile, *weight));
            }
        }
        macro_rules! weighted{($($field:ident),+)=>{$(effects.$field=profiles.iter().map(|(p,w)|p.$field*w).sum();)+};}
        weighted!(
            sun,
            cloud,
            wind_min,
            wind_max,
            rain,
            snow,
            snow_mist,
            fog,
            ripple_bumpiness,
            ripple_min_bumpiness,
            ripple_max_bumpiness,
            ripple_bumpiness_wind_scale,
            ripple_scale,
            ripple_speed,
            ripple_velocity_transfer,
            ocean_bumpiness,
            deep_ocean_scale,
            ocean_noise_min_amplitude,
            ocean_wave_amplitude,
            shore_wave_amplitude,
            ocean_wave_wind_scale,
            shore_wave_wind_scale,
            ocean_wave_min_amplitude,
            shore_wave_min_amplitude,
            ocean_wave_max_amplitude,
            shore_wave_max_amplitude,
            ocean_foam_intensity,
            ocean_foam_scale,
            ripple_disturb,
            lightning,
            sandstorm
        );
        let no_rain = f.number("no_rain", 0.0).clamp(0.0, 1.0);
        let no_fx = f.number("no_weather_fx", 0.0).clamp(0.0, 1.0);
        self.simulation.step(
            dt,
            &self.tuning,
            [
                effects.rain * (1.0 - no_rain),
                effects.snow,
                effects.snow_mist,
            ],
            [effects.wind_min, effects.wind_max],
            scene.focus_position()[1],
            f.number("water_drying_speed_mult", 1.0),
            f.number("wind_speed_mult", 1.0),
            f.number("temperature", 20.0),
        )?;
        effects.rain = self.simulation.rain * (1.0 - no_fx);
        effects.snow = self.simulation.snow * (1.0 - no_fx);
        effects.snow_mist = self.simulation.snow_mist * (1.0 - no_fx);
        effects.wind_speed = self.simulation.wind_speed;
        effects.wind_direction = [
            self.simulation.wind_direction.cos(),
            self.simulation.wind_direction.sin(),
        ];
        if let Some(forcing)=&self.wind_forcing {
            if let Some(snow)=forcing.snow_precipitation{
                self.simulation.snow=snow;
                effects.snow=snow*(1.0-no_fx);
            }
            effects.wind_speed=forcing.wind_speed;
            effects.wind_direction=forcing.wind_direction;
            effects.snow_mist=effects.snow_mist.max(forcing.snow_mist)*(1.0-no_fx);
            if let Some(setting)=&forcing.snow_mist_setting {
                effects.current_mist_setting=setting.clone();
                effects.next_mist_setting=setting.clone();
            }
        }
        if f.number("no_gpu_fx", 0.0) > 0.5 {
            effects.current_drop_setting.clear();
            effects.next_drop_setting.clear();
            effects.current_mist_setting.clear();
            effects.next_mist_setting.clear();
            effects.current_ground_setting.clear();
            effects.next_ground_setting.clear();
        }
        scene.set_weather_backend(current, next, blend)?;
        scene.set_weather_effects(effects.clone())?;
        scene.set_environment_time(
            self.clock.day_seconds as f32,
            self.clock.elapsed_days as f32,
            self.clock.rate() as f32,
        )?;
        scene.set_weather_surface_wetness(self.simulation.wetness)?;

        let celestial = self
            .celestial
            .evaluate(self.clock.hours(), self.clock.elapsed_days);
        let (light_scale, ambient_scale, sky_scale) = if self.policy.reference_render {
            (1.0, 1.0, 1.0)
        } else {
            (
                self.policy.light_scale,
                self.policy.ambient_scale,
                self.policy.sky_scale,
            )
        };
        let override_weight = f.number("light_direction_override", 0.0).clamp(0.0, 1.0);
        let sun_override = [
            f.number("sun_direction_x", celestial.sun_direction[0]),
            f.number("sun_direction_z", celestial.sun_direction[1]),
            -f.number("sun_direction_y", -celestial.sun_direction[2]),
        ];
        let mut light_direction = std::array::from_fn(|i| {
            celestial.light_direction[i]
                + (sun_override[i] - celestial.light_direction[i]) * override_weight
        });
        let n = light_direction.iter().map(|v| v * v).sum::<f32>().sqrt();
        if n > 1e-6 {
            light_direction = light_direction.map(|v| v / n);
        } else {
            light_direction = celestial.light_direction;
        }
        scene.upsert_runtime_light(
            &self.policy.light_id,
            SceneLightDesc {
                light_type: SceneLightType::Directional,
                color: scaled(f.color("light_dir_col", [1.0; 3]), 1.0),
                intensity: f.number("light_dir_mult", 1.0).max(0.0)
                    * celestial.light_intensity
                    * light_scale,
                casts_shadows: true,
                shadow_distance: (self.policy.shadow_distance
                    * f.number("dir_shadow_distance_multiplier", 1.0))
                .max(1.0),
                ..SceneLightDesc::default()
            },
        )?;
        scene.set_runtime_entity_transform(
            &self.policy.light_id,
            None,
            Some(rotation(light_direction)),
            None,
        )?;
        for (id, kind, color, intensity, direction, radius, driver) in [
            (
                &self.policy.sun_visual_id,
                SkyVisualKind::Disc,
                f.color("sky_sun_disc_col", [1.0; 3]),
                f.number("sky_sun_hdr", 1.0) * sky_scale,
                celestial.sun_direction,
                (self.policy.sun_radius_degrees * f.number("sky_sun_disc_size", 2.5) / 2.5)
                    .max(0.001),
                true,
            ),
            (
                &self.policy.moon_visual_id,
                SkyVisualKind::Billboard,
                f.color("sky_moon_col", [0.8, 0.85, 1.0]),
                f.number("sky_moon_iten", 0.5),
                celestial.moon_direction,
                (self.policy.moon_radius_degrees * f.number("sky_moon_disc_size", 3.0) / 3.0)
                    .max(0.001),
                false,
            ),
        ] {
            let visibility = ((direction[1] + 0.07) / 0.12).clamp(0.0, 1.0);
            scene.upsert_runtime_sky_visual(
                id,
                SkyVisualDesc {
                    kind,
                    color: linear(color, 1.0),
                    intensity: intensity.max(0.0) * visibility,
                    angular_size_degrees: radius,
                    halo_size_degrees: radius * 8.0,
                    halo_intensity: intensity.max(0.0) * visibility * 0.04,
                    atmosphere_driver: driver,
                },
            )?;
            scene.set_runtime_entity_transform(id, None, Some(rotation(direction)), None)?;
        }
        let down = scaled(
            f.color("light_natural_amb_down_col", [1.0; 3]),
            f.number("light_natural_amb_down_intensity", 1.0),
        );
        let up = scaled(
            f.color("light_natural_amb_up_col", [1.0; 3]),
            f.number("light_natural_amb_up_intensity", 1.0)
                * f.number("light_natural_amb_up_intensity_mult", 1.0),
        );
        let ambient = std::array::from_fn(|i| (down[i] + up[i]) * 0.5);
        let maximum = ambient.iter().copied().fold(0.0_f32, f32::max).max(1e-6);
        let mut environment = scene.scene_environment();
        environment.render = if self.policy.reference_render {
            rendering::frame_environment(
                f,
                &celestial,
                self.exposure_tuning,
                self.clock.day_seconds as f32,
            )
        } else {
            Default::default()
        };
        environment.render.wetness = self.simulation.wetness;
        environment.render.snow = effects.snow;
        environment.ambient_color = ambient.map(|v| v / maximum);
        environment.ambient_intensity =
            (maximum * ambient_scale * f.number("natural_ambient_multiplier", 1.0))
                .clamp(0.0, 64.0);
        environment.fog_density = (f.number("fog_density", 0.0) / 10000.0).max(0.0);
        environment.fog_enabled = environment.fog_density > 0.0;
        environment.fog_color = scaled(
            f.color("fog_near_col", [0.0; 3]),
            f.number("fog_hdr", 1.0) * sky_scale,
        );
        environment.fog_start_distance = f.number("fog_start", 0.0).max(0.0);
        environment.fog_height_falloff = (f.number("fog_falloff", 0.0) / 1000.0).max(0.0);
        environment.fog_base_height = f.number("fog_base_height", 0.0);
        environment.fog_max_opacity = f.number("fog_alpha", 1.0).clamp(0.0, 1.0);
        environment.haze_color = scaled(
            f.color("fog_haze_col", [0.0; 3]),
            f.number("fog_haze_hdr", 1.0) * sky_scale,
        );
        environment.haze_density = (f.number("fog_haze_density", 0.0) / 10000.0).max(0.0);
        environment.haze_start_distance = f.number("fog_haze_start", 0.0).max(0.0);
        scene.set_scene_environment(environment)?;
        let cloud_hdr = f.number("sky_cloud_hdr", 1.0) * sky_scale;
        let base = linear(f.color("sky_cloud_base_col", [0.4; 3]), cloud_hdr);
        let shadow = linear(f.color("sky_cloud_shadow_col", [0.1; 3]), cloud_hdr);
        let mid = linear(f.color("sky_cloud_mid_col", [0.8; 3]), cloud_hdr);
        let mut atmosphere = scene.sky_atmosphere();
        atmosphere.cloud_night = base;
        atmosphere.cloud_twilight_shadow = shadow;
        atmosphere.cloud_day_shadow = shadow;
        atmosphere.cloud_twilight_light = mid;
        atmosphere.cloud_day_light = mid;
        atmosphere.star_intensity = f.number("sky_stars_iten", 0.5).clamp(0.0, 64.0);
        scene.set_sky_atmosphere(atmosphere)?;
        scene.set_keyframed_sky(newviso_scene::KeyframedSkyDesc {
            east: linear(
                f.color("sky_azimuth_east_col", [0.0; 3]),
                f.number("sky_azimuth_east_col_inten", 1.0),
            ),
            west: linear(
                f.color("sky_azimuth_west_col", [0.0; 3]),
                f.number("sky_azimuth_west_col_inten", 1.0),
            ),
            transition: linear(
                f.color("sky_azimuth_transition_col", [0.0; 3]),
                f.number("sky_azimuth_transition_col_inten", 1.0),
            ),
            zenith: linear(
                f.color("sky_zenith_col", [0.0; 3]),
                f.number("sky_zenith_col_inten", 1.0),
            ),
            zenith_transition: linear(
                f.color("sky_zenith_transition_col", [0.0; 3]),
                f.number("sky_zenith_transition_col_inten", 1.0),
            ),
            zenith_params: [
                f.number("sky_zenith_transition_position", 0.2),
                f.number("sky_zenith_transition_east_blend", 0.7),
                f.number("sky_zenith_transition_west_blend", 0.7),
                1.0 - f.number("sky_zenith_blend_start", 0.0),
            ],
            azimuth_position: f.number("sky_azimuth_transition_position", 0.5),
            hdr: f.number("sky_hdr", 1.0) * sky_scale,
            lunar_phase: celestial.lunar_phase,
            mie_phase: f.number("sky_sun_miephase", 0.9),
            mie_scatter: f.number("sky_sun_miescatter", 1.0),
            mie_intensity: f.number("sky_sun_mie_intensity_mult", 1.0),
            sun_color: linear(
                f.color("sky_sun_col", [1.0; 3]),
                f.number("sky_sun_hdr", 1.0) * sky_scale,
            ),
            clouds: Some(self.policy.cloud_rendering.sky_clouds(f, sky_scale)),
        })?;
        let mut clouds = scene.sky_clouds();
        clouds.coverage =
            (1.0 - f.number("sky_cloud_gen_threshold", 1.0 - effects.cloud)).clamp(0.0, 1.0);
        clouds.density = f
            .number("sky_cloud_density_mult", clouds.density)
            .clamp(0.0, 2.0);
        clouds.softness = f
            .number("sky_cloud_gen_softness", clouds.softness)
            .clamp(0.01, 1.0);
        clouds.detail_strength = f
            .number("sky_cloud_overall_strength", clouds.detail_strength)
            .clamp(0.0, 2.0);
        clouds.micro_strength = f
            .number("sky_cloud_edge_strength", clouds.micro_strength)
            .clamp(0.0, 2.0);
        clouds.speed = effects.wind_direction.map(|v| v * 0.01);
        scene.set_sky_clouds(clouds)?;
        let mut cloud_weights = BTreeMap::<String, f32>::new();
        for (profile, weight) in &profiles {
            *cloud_weights
                .entry(profile.current_cloud_settings.to_ascii_uppercase())
                .or_default() += *weight;
        }
        let curve = self
            .cloud_keyframes
            .evaluate(self.clock.hours(), &cloud_weights)?;
        let mut hat = scene.cloudhat_keyframe();
        hat.enabled = true;
        macro_rules! cloud_value {($($field:ident),+) => {$(hat.$field=*curve.get(stringify!($field)).ok_or(concat!("missing cloud curve ",stringify!($field)))?;)+};}
        cloud_value!(
            cloud_color,
            cloud_light_color,
            cloud_ambient_color,
            cloud_sky_color,
            cloud_bounce_color,
            cloud_east_color,
            cloud_west_color,
            scale_fill_colors,
            density_shift_scale_scattering,
            piercing_light,
            scale_diffuse_fill_ambient_wrap
        );
        let (light_color, lunar_intensity) = cloud_rendering::cloud_light(f, &celestial);
        hat.cloud_light_color = [light_color[0] * sky_scale, light_color[1] * sky_scale,
            light_color[2] * sky_scale, 1.0];
        hat.light_direction = Some(if celestial.sun_fade > 0.0 {
            celestial.sun_direction
        } else {
            celestial.moon_direction
        });
        // The widget's 0.2 strength conversion happens in GPU uniform packing.
        hat.density_shift_scale_scattering[3] *= lunar_intensity;
        scene.set_cloudhat_keyframe(hat)?;
        let changed = self.cloud_selection.update(
            &self.cloud_keyframes,
            &effects.current_cloud_settings,
            self.weather.cycle_index,
            current,
        )?;
        if changed || !self.cloud_targets_installed {
            let duration = if self.cloud_targets_installed {
                self.cloud_transition_seconds
            } else {
                0.0
            };
            for (id, index) in &self.cloud_layers {
                let alpha = if self.cloud_selection.index == Some(*index) {
                    1.0
                } else {
                    0.0
                };
                scene.set_atmospheric_cloud_layer_target(id, alpha, duration)?;
            }
            self.cloud_targets_installed = true;
        }
        Ok(())
    }
}
