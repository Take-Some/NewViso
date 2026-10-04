use super::*;

impl EngineApplication {
    pub(super) fn apply_environment_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "scene.clear_color.set" => {
                self.scene
                    .set_clear_color(command_vec4(command, "color", index)?);
            }
            "scene.environment.set" => {
                let mut desc = self.scene.scene_environment();
                if command.get("ambient_color").is_some() {
                    desc.ambient_color = command_vec3(command, "ambient_color", index)?;
                }
                if command.get("ambient_intensity").is_some() {
                    desc.ambient_intensity = command_number(command, "ambient_intensity", index)?;
                }

                if let Some(fog) = command.get("fog") {
                    if !fog.is_object() {
                        return Err(format!(
                            "script command[{index}] scene.environment.set 'fog' must be an object"
                        ));
                    }
                    if let Some(value) = fog.get("enabled") {
                        desc.fog_enabled = value.as_bool().ok_or_else(|| {
                        format!(
                            "script command[{index}] scene.environment.set fog.enabled must be boolean"
                        )
                    })?;
                    }
                    if fog.get("color").is_some() {
                        desc.fog_color = command_vec3(fog, "color", index)?;
                    }
                    if fog.get("density").is_some() {
                        desc.fog_density = command_number(fog, "density", index)?;
                    }
                    if fog.get("start_distance").is_some() {
                        desc.fog_start_distance = command_number(fog, "start_distance", index)?;
                    }
                    if fog.get("height_falloff").is_some() {
                        desc.fog_height_falloff = command_number(fog, "height_falloff", index)?;
                    }
                    if fog.get("base_height").is_some() {
                        desc.fog_base_height = command_number(fog, "base_height", index)?;
                    }
                    if fog.get("max_opacity").is_some() {
                        desc.fog_max_opacity = command_number(fog, "max_opacity", index)?;
                    }
                }

                if let Some(haze) = command.get("haze") {
                    if !haze.is_object() {
                        return Err(format!(
                        "script command[{index}] scene.environment.set 'haze' must be an object"
                    ));
                    }
                    if haze.get("color").is_some() {
                        desc.haze_color = command_vec3(haze, "color", index)?;
                    }
                    if haze.get("density").is_some() {
                        desc.haze_density = command_number(haze, "density", index)?;
                    }
                    if haze.get("start_distance").is_some() {
                        desc.haze_start_distance = command_number(haze, "start_distance", index)?;
                    }
                }

                self.scene.set_scene_environment(desc)?;
            }
            "scene.orbit.configure" => {
                // Preserve the legacy command while keeping the live settings
                // snapshot synchronized with the actual camera policy.
                self.configure_runtime(&json!({"camera": {
                    "rotate_sensitivity": command_number(command, "rotate_sensitivity", index)?,
                    "zoom_sensitivity": command_number(command, "zoom_sensitivity", index)?,
                    "min_distance": command_number(command, "min_distance", index)?,
                    "max_distance": command_number(command, "max_distance", index)?
                }}))?;
            }
            "scene.sky.dome.set" => {
                let sky = command.get("sky").ok_or_else(|| {
                    format!("script command[{index}] scene.sky.dome.set requires 'sky'")
                })?;
                let environment = ProjectEnvironment::from_value(json!({
                    "schema": "newviso.environment.v1", "sky": sky
                }))
                .map_err(|error| format!("script command[{index}] invalid sky: {error}"))?;
                let config = environment.sky.as_ref().ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.sky.dome.set requires a sky configuration"
                    )
                })?;
                self.scene.set_sky_dome(load_environment_sky(config)?)?;
            }
            "scene.sky.time.set" => {
                self.scene
                    .set_sky_time_seconds(command_number(command, "seconds", index)?)?;
            }
            "scene.sky.time_scale.set" => {
                self.scene
                    .set_sky_time_scale(command_number(command, "scale", index)?)?;
            }
            "scene.timecycle.state.set" => {
                let cycle_seconds = command_number(command, "cycle_seconds", index)?;
                let phase = command_number(command, "phase", index)?;
                let rate = command_number(command, "rate", index)?;
                let duration_seconds = command_number(command, "duration_seconds", index)?;
                self.scene
                    .set_timecycle_backend(cycle_seconds, phase, rate, duration_seconds)?;
            }
            "scene.weather.state.set" => {
                let current = command
                    .get("current")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!(
                        "script command[{index}] scene.weather.state.set requires string 'current'"
                    )
                    })?;
                let next = command
                    .get("next")
                    .and_then(Value::as_str)
                    .unwrap_or(current);
                let blend = command_number(command, "blend", index)?;

                let resolved_effects = application_weather::resolve_shared_weather_effects(
                    current,
                    next,
                    blend,
                    self.scene.weather_effects(),
                )?
                .unwrap_or_else(|| self.scene.weather_effects().clone());
                let mut effects =
                    parse_weather_effects(command.get("effects"), &resolved_effects, index)?;
                if let Some(value) = command.get("current_cloud_variant").and_then(Value::as_str) {
                    effects.current_cloud_variant = value.to_owned();
                }
                if let Some(value) = command.get("next_cloud_variant").and_then(Value::as_str) {
                    effects.next_cloud_variant = value.to_owned();
                }

                self.scene.set_weather_backend(current, next, blend)?;
                self.scene.set_weather_effects(effects)?;
            }
            "scene.sky_visual.upsert" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] scene.sky_visual.upsert requires string 'id'")
                })?;
                let kind = match command
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("disc")
                    .trim()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "disc" => SkyVisualKind::Disc,
                    "billboard" => SkyVisualKind::Billboard,
                    other => {
                        return Err(format!(
                            "script command[{index}] unknown sky visual kind '{other}'"
                        ));
                    }
                };
                let mut desc = SkyVisualDesc::default();
                desc.kind = kind;
                if command.get("color").is_some() {
                    desc.color = command_vec3(command, "color", index)?;
                }
                if command.get("intensity").is_some() {
                    desc.intensity = command_number(command, "intensity", index)?;
                }
                if command.get("angular_size_degrees").is_some() {
                    desc.angular_size_degrees =
                        command_number(command, "angular_size_degrees", index)?;
                }
                if command.get("halo_size_degrees").is_some() {
                    desc.halo_size_degrees = command_number(command, "halo_size_degrees", index)?;
                }
                if command.get("halo_intensity").is_some() {
                    desc.halo_intensity = command_number(command, "halo_intensity", index)?;
                }
                if let Some(value) = command.get("atmosphere_driver") {
                    desc.atmosphere_driver = value.as_bool().ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.sky_visual.upsert 'atmosphere_driver' must be boolean"
                    )
                })?;
                }
                self.scene.upsert_runtime_sky_visual(id, desc)?;
            }
            "scene.sky_visual.remove" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] scene.sky_visual.remove requires string 'id'")
                })?;
                self.scene.remove_runtime_sky_visual(id)?;
            }
            "scene.sky.atmosphere.set" => {
                self.scene.set_sky_atmosphere(SkyAtmosphereDesc {
                    twilight_altitudes: command_vec4(command, "twilight_altitudes", index)?,
                    daylight_altitudes: command_vector::<2>(command, "daylight_altitudes", index)?,
                    horizon_power: command_number(command, "horizon_power", index)?,
                    tonemap_shoulder: command_number(command, "tonemap_shoulder", index)?,
                    night_zenith: command_vec3(command, "night_zenith", index)?,
                    night_horizon: command_vec3(command, "night_horizon", index)?,
                    astronomical_zenith: command_vec3(command, "astronomical_zenith", index)?,
                    astronomical_horizon: command_vec3(command, "astronomical_horizon", index)?,
                    nautical_zenith: command_vec3(command, "nautical_zenith", index)?,
                    nautical_horizon: command_vec3(command, "nautical_horizon", index)?,
                    civil_zenith: command_vec3(command, "civil_zenith", index)?,
                    civil_horizon: command_vec3(command, "civil_horizon", index)?,
                    day_zenith: command_vec3(command, "day_zenith", index)?,
                    day_horizon: command_vec3(command, "day_horizon", index)?,
                    sunset_tint: command_vec3(command, "sunset_tint", index)?,
                    sunset_strength: command_number(command, "sunset_strength", index)?,
                    cloud_night: command_vec3(command, "cloud_night", index)?,
                    cloud_twilight_shadow: command_vec3(command, "cloud_twilight_shadow", index)?,
                    cloud_twilight_light: command_vec3(command, "cloud_twilight_light", index)?,
                    cloud_day_shadow: command_vec3(command, "cloud_day_shadow", index)?,
                    cloud_day_light: command_vec3(command, "cloud_day_light", index)?,
                    star_tint: command_vec3(command, "star_tint", index)?,
                    star_intensity: command_number(command, "star_intensity", index)?,
                    star_visibility_altitudes: command_vector::<2>(
                        command,
                        "star_visibility_altitudes",
                        index,
                    )?,
                    cloud_occlusion: command_number(command, "cloud_occlusion", index)?,
                    silver_lining_tint: command_vec3(command, "silver_lining_tint", index)?,
                    silver_lining_strength: command_number(
                        command,
                        "silver_lining_strength",
                        index,
                    )?,
                    cloud_alpha_range: command_vector::<2>(command, "cloud_alpha_range", index)?,
                })?;
            }
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}

fn parse_weather_effects(
    value: Option<&Value>,
    current: &WeatherEffectsState,
    index: usize,
) -> Result<WeatherEffectsState, String> {
    let Some(value) = value else {
        return Ok(current.clone());
    };
    if !value.is_object() {
        return Err(format!(
            "script command[{index}] weather 'effects' must be an object"
        ));
    }

    let mut state = current.clone();
    macro_rules! string_field {
        ($field:ident, $key:literal) => {
            if let Some(raw) = value.get($key) {
                state.$field = raw
                    .as_str()
                    .ok_or_else(|| {
                        format!(
                            "script command[{index}] weather effects '{}' must be a string",
                            $key
                        )
                    })?
                    .to_owned();
            }
        };
    }
    macro_rules! number_field {
        ($field:ident, $key:literal) => {
            if value.get($key).is_some() {
                state.$field = command_number(value, $key, index)?;
            }
        };
    }

    string_field!(current_cloud_settings, "current_cloud_settings");
    string_field!(next_cloud_settings, "next_cloud_settings");
    string_field!(current_timecycle, "current_timecycle");
    string_field!(next_timecycle, "next_timecycle");
    string_field!(current_drop_setting, "current_drop_setting");
    string_field!(next_drop_setting, "next_drop_setting");
    string_field!(current_mist_setting, "current_mist_setting");
    string_field!(next_mist_setting, "next_mist_setting");
    string_field!(current_ground_setting, "current_ground_setting");
    string_field!(next_ground_setting, "next_ground_setting");
    string_field!(current_cloud_variant, "current_cloud_variant");
    string_field!(next_cloud_variant, "next_cloud_variant");

    number_field!(sun, "sun");
    number_field!(cloud, "cloud");
    number_field!(wind_min, "wind_min");
    number_field!(wind_max, "wind_max");
    number_field!(wind_speed, "wind_speed");
    number_field!(rain, "rain");
    number_field!(snow, "snow");
    number_field!(snow_mist, "snow_mist");
    number_field!(fog, "fog");
    number_field!(ripple_bumpiness, "ripple_bumpiness");
    number_field!(ripple_min_bumpiness, "ripple_min_bumpiness");
    number_field!(ripple_max_bumpiness, "ripple_max_bumpiness");
    number_field!(ripple_bumpiness_wind_scale, "ripple_bumpiness_wind_scale");
    number_field!(ripple_scale, "ripple_scale");
    number_field!(ripple_speed, "ripple_speed");
    number_field!(ripple_velocity_transfer, "ripple_velocity_transfer");
    number_field!(ocean_bumpiness, "ocean_bumpiness");
    number_field!(deep_ocean_scale, "deep_ocean_scale");
    number_field!(ocean_noise_min_amplitude, "ocean_noise_min_amplitude");
    number_field!(ocean_wave_amplitude, "ocean_wave_amplitude");
    number_field!(shore_wave_amplitude, "shore_wave_amplitude");
    number_field!(ocean_wave_wind_scale, "ocean_wave_wind_scale");
    number_field!(shore_wave_wind_scale, "shore_wave_wind_scale");
    number_field!(ocean_wave_min_amplitude, "ocean_wave_min_amplitude");
    number_field!(shore_wave_min_amplitude, "shore_wave_min_amplitude");
    number_field!(ocean_wave_max_amplitude, "ocean_wave_max_amplitude");
    number_field!(shore_wave_max_amplitude, "shore_wave_max_amplitude");
    number_field!(ocean_foam_intensity, "ocean_foam_intensity");
    number_field!(ocean_foam_scale, "ocean_foam_scale");
    number_field!(ripple_disturb, "ripple_disturb");
    number_field!(lightning, "lightning");
    number_field!(sandstorm, "sandstorm");

    if value.get("wind_direction").is_some() {
        state.wind_direction = command_vector::<2>(value, "wind_direction", index)?;
    }
    Ok(state)
}
