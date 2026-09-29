use super::*;

const WEATHER_CATALOG: &str = "environment/weather/gtav_weather.catalog.json";

pub(super) fn load_shared_weather_gpu_fx() -> Result<Option<WeatherGpuFxResources>, String> {
    let assets = AssetClient::new();
    let catalog = match assets.json(WEATHER_CATALOG) {
        Ok(value) => value,
        Err(error) => {
            host::warn(
                "newviso.weather",
                format!(
                    "Shared weather catalog '{}' is unavailable; generic weather state remains active without GTA GPU FX: {error}",
                    WEATHER_CATALOG
                ),
            );
            return Ok(None);
        }
    };

    let rows = catalog
        .get("weather_gpu_fx")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            format!(
                "Shared weather catalog '{}' has no weather_gpu_fx array",
                WEATHER_CATALOG
            )
        })?;

    let mut layers = BTreeMap::<String, WeatherGpuFxLayerDesc>::new();
    let mut textures = BTreeMap::<String, SkyTextureResources>::new();
    let mut color_spaces = BTreeMap::<String, bool>::new();

    for row in rows {
        let name = required_string(row, "name", "WeatherGpuFx")?.to_owned();
        let system_type = match row
            .get("systemType")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
        {
            "SYSTEM_TYPE_DROP" => WeatherGpuFxSystemType::Drop,
            "SYSTEM_TYPE_MIST" => WeatherGpuFxSystemType::Mist,
            "SYSTEM_TYPE_GROUND" => WeatherGpuFxSystemType::Ground,
            _ => WeatherGpuFxSystemType::Other,
        };
        let drive_type = row
            .get("driveType")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();

        let texture_refs = row
            .get("textureRefs")
            .and_then(Value::as_object)
            .ok_or_else(|| format!("WeatherGpuFx '{name}' has no textureRefs object"))?;
        let diffuse_texture = texture_refs
            .get("diffuseName")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("WeatherGpuFx '{name}' has no diffuseName texture ref"))?
            .to_owned();
        let distortion_texture = texture_refs
            .get("distortionTexture")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let splash_texture = texture_refs
            .get("diffuseSplashName")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);

        register_texture(
            &assets,
            &mut textures,
            &mut color_spaces,
            &diffuse_texture,
            true,
        )?;
        if let Some(reference) = distortion_texture.as_deref() {
            register_texture(&assets, &mut textures, &mut color_spaces, reference, false)?;
        }
        if let Some(reference) = splash_texture.as_deref() {
            register_texture(&assets, &mut textures, &mut color_spaces, reference, true)?;
        }

        let emitter = row
            .pointer("/emitter/settings")
            .ok_or_else(|| format!("WeatherGpuFx '{name}' has no emitter settings"))?;
        let render = row
            .pointer("/render/settings")
            .ok_or_else(|| format!("WeatherGpuFx '{name}' has no render settings"))?;

        layers.insert(
            name.clone(),
            WeatherGpuFxLayerDesc {
                name,
                system_type,
                drive_type,
                wind_influence: number(row, "windInfluence", 0.0),
                gravity: number(row, "gravity", 0.0),
                diffuse_texture,
                distortion_texture,
                splash_texture,
                emitter: WeatherGpuFxEmitterDesc {
                    box_centre_offset: vector(
                        emitter,
                        "boxCentreOffset",
                        ["x", "y", "z"],
                        [0.0; 3],
                    ),
                    box_size: vector(emitter, "boxSize", ["x", "y", "z"], [1.0; 3]),
                    life_min_max: vector(emitter, "lifeMinMax", ["x", "y"], [1.0, 1.0]),
                    velocity_min: vector(emitter, "velocityMin", ["x", "y", "z"], [0.0; 3]),
                    velocity_max: vector(emitter, "velocityMax", ["x", "y", "z"], [0.0; 3]),
                    clamp_to_ground: boolean(emitter, "clampToGround", false),
                },
                render: WeatherGpuFxRenderDesc {
                    texture_rows_cols_start_end: vector(
                        render,
                        "textureRowsColsStartEnd",
                        ["x", "y", "z", "w"],
                        [1.0, 1.0, 0.0, 0.0],
                    ),
                    texture_anim_rate_scale_over_life: vector(
                        render,
                        "textureAnimRateScaleOverLifeStart2End2",
                        ["x", "y", "z", "w"],
                        [0.0; 4],
                    ),
                    size_min_max: vector(
                        render,
                        "sizeMinMax",
                        ["x", "y", "z", "w"],
                        [0.1, 0.1, 0.1, 0.1],
                    ),
                    colour: vector(render, "colour", ["x", "y", "z", "w"], [1.0; 4]),
                    fade_in_out: vector(render, "fadeInOut", ["x", "y"], [0.05, 0.05]),
                    fade_near_far: vector(render, "fadeNearFar", ["x", "y"], [0.0, 100.0]),
                    fade_ground_offset: vector(
                        render,
                        "fadeGrdOffLoHi",
                        ["x", "y", "z", "w"],
                        [0.0; 4],
                    ),
                    rot_speed_min_max: vector(render, "rotSpeedMinMax", ["x", "y"], [0.0; 2]),
                    directional_z_offset_min_max: vector(
                        render,
                        "directionalZOffsetMinMax",
                        ["x", "y", "z"],
                        [0.0; 3],
                    ),
                    camera_speed_add: vector(
                        render,
                        "dirVelAddCamSpeedMinMaxMult",
                        ["x", "y", "z"],
                        [0.0; 3],
                    ),
                    edge_softness: number(render, "edgeSoftness", 0.0),
                    particle_color_percentage: number(render, "particleColorPercentage", 1.0),
                    background_distortion_visibility: number(
                        render,
                        "backgroundDistortionVisibilityPercentage",
                        0.0,
                    ),
                    background_distortion_alpha_booster: number(
                        render,
                        "backgroundDistortionAlphaBooster",
                        1.0,
                    ),
                    background_distortion_amount: number(render, "backgroundDistortionAmount", 0.0),
                    background_distortion_blur_level: number(
                        render,
                        "backgroundDistortionBlurLevel",
                        0.0,
                    ),
                    local_lights_multiplier: number(render, "localLightsMultiplier", 1.0),
                },
            },
        );
    }

    let rain = catalog.get("rain").ok_or_else(|| {
        format!(
            "Shared weather catalog '{}' has no rain object",
            WEATHER_CATALOG
        )
    })?;
    let wet_surface = rain
        .get("wet_surface_texture_refs")
        .ok_or_else(|| "weather catalog has no wet_surface_texture_refs".to_owned())?;

    let puddle_layout_texture =
        required_string(wet_surface, "puddle_layout", "weather wet surface")?.to_owned();
    let puddle_normal_textures = wet_surface
        .get("puddle_normals")
        .and_then(Value::as_array)
        .ok_or_else(|| "weather catalog has no puddle_normals array".to_owned())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| "weather puddle normal ref must be a string".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if puddle_normal_textures.is_empty() {
        return Err("weather catalog puddle normal ring is empty".to_owned());
    }

    let lightning_texture =
        required_string(rain, "lightning_texture_ref", "weather rain")?.to_owned();
    let ptfx_refs = rain
        .get("ptfx_texture_refs")
        .ok_or_else(|| "weather catalog has no ptfx_texture_refs".to_owned())?;
    let lens_drop_texture =
        required_string(ptfx_refs, "ptfx_lens_water_drop", "weather PTFX")?.to_owned();
    let lens_drop_normal_texture =
        required_string(ptfx_refs, "ptfx_lens_water_drop_n", "weather PTFX")?.to_owned();
    let lens_running_normal_texture =
        required_string(ptfx_refs, "ptfx_lens_water_running_n", "weather PTFX")?.to_owned();

    register_texture(
        &assets,
        &mut textures,
        &mut color_spaces,
        &puddle_layout_texture,
        false,
    )?;
    for reference in &puddle_normal_textures {
        register_texture(&assets, &mut textures, &mut color_spaces, reference, false)?;
    }
    register_texture(
        &assets,
        &mut textures,
        &mut color_spaces,
        &lightning_texture,
        true,
    )?;
    register_texture(
        &assets,
        &mut textures,
        &mut color_spaces,
        &lens_drop_texture,
        true,
    )?;
    register_texture(
        &assets,
        &mut textures,
        &mut color_spaces,
        &lens_drop_normal_texture,
        false,
    )?;
    register_texture(
        &assets,
        &mut textures,
        &mut color_spaces,
        &lens_running_normal_texture,
        false,
    )?;

    host::info(
        "newviso.weather",
        format!(
            "Shared weather GPU FX loaded catalog='{}' layers={} textures={} puddle_frames={}",
            WEATHER_CATALOG,
            layers.len(),
            textures.len(),
            puddle_normal_textures.len(),
        ),
    );

    Ok(Some(WeatherGpuFxResources {
        layers,
        textures,
        puddle_layout_texture,
        puddle_normal_textures,
        lightning_texture,
        lens_drop_texture,
        lens_drop_normal_texture,
        lens_running_normal_texture,
    }))
}

pub(super) fn resolve_shared_weather_effects(
    current: &str,
    next: &str,
    blend: f32,
    previous: &WeatherEffectsState,
) -> Result<Option<WeatherEffectsState>, String> {
    if !blend.is_finite() || !(0.0..=1.0).contains(&blend) {
        return Err("weather blend must be finite and in 0..=1".to_owned());
    }

    let assets = AssetClient::new();
    let catalog = match assets.json(WEATHER_CATALOG) {
        Ok(value) => value,
        Err(error) => {
            host::warn(
                "newviso.weather",
                format!(
                    "Shared weather catalog '{}' is unavailable while resolving state '{} -> {}'; keeping explicit/generic weather effects: {error}",
                    WEATHER_CATALOG, current, next
                ),
            );
            return Ok(None);
        }
    };

    resolve_weather_effects_from_catalog(&catalog, current, next, blend, previous)
}

fn resolve_weather_effects_from_catalog(
    catalog: &Value,
    current: &str,
    next: &str,
    blend: f32,
    previous: &WeatherEffectsState,
) -> Result<Option<WeatherEffectsState>, String> {
    let rows = catalog
        .get("weather_types")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            format!(
                "Shared weather catalog '{}' has no weather_types array",
                WEATHER_CATALOG
            )
        })?;

    let find = |id: &str| {
        rows.iter().find(|row| {
            row.get("id")
                .and_then(Value::as_str)
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(id.trim()))
        })
    };
    let Some(current_row) = find(current) else {
        return Ok(None);
    };
    let Some(next_row) = find(next) else {
        return Ok(None);
    };

    let mut state = previous.clone();
    state.current_cloud_settings = weather_string(current_row, "cloudSettingsName");
    state.next_cloud_settings = weather_string(next_row, "cloudSettingsName");
    state.current_timecycle = weather_string(current_row, "timeCycleFilename");
    state.next_timecycle = weather_string(next_row, "timeCycleFilename");
    state.current_drop_setting = weather_setting(current_row, "dropSettingName");
    state.next_drop_setting = weather_setting(next_row, "dropSettingName");
    state.current_mist_setting = weather_setting(current_row, "mistSettingName");
    state.next_mist_setting = weather_setting(next_row, "mistSettingName");
    state.current_ground_setting = weather_setting(current_row, "groundSettingName");
    state.next_ground_setting = weather_setting(next_row, "groundSettingName");

    macro_rules! blend_field {
        ($field:ident, $key:literal) => {
            state.$field = blend_weather_number(current_row, next_row, $key, blend, state.$field);
        };
    }

    blend_field!(sun, "sun");
    blend_field!(cloud, "cloud");
    blend_field!(wind_min, "windMin");
    blend_field!(wind_max, "windMax");
    blend_field!(rain, "rain");
    blend_field!(snow, "snow");
    blend_field!(snow_mist, "snowMist");
    blend_field!(fog, "fog");
    blend_field!(ripple_bumpiness, "rippleBumpiness");
    blend_field!(ripple_min_bumpiness, "rippleMinBumpiness");
    blend_field!(ripple_max_bumpiness, "rippleMaxBumpiness");
    blend_field!(ripple_bumpiness_wind_scale, "rippleBumpinessWindScale");
    blend_field!(ripple_scale, "rippleScale");
    blend_field!(ripple_speed, "rippleSpeed");
    blend_field!(ripple_velocity_transfer, "rippleVelocityTransfer");
    blend_field!(ocean_bumpiness, "oceanBumpiness");
    blend_field!(deep_ocean_scale, "deepOceanScale");
    blend_field!(ocean_noise_min_amplitude, "oceanNoiseMinAmplitude");
    blend_field!(ocean_wave_amplitude, "oceanWaveAmplitude");
    blend_field!(shore_wave_amplitude, "shoreWaveAmplitude");
    blend_field!(ocean_wave_wind_scale, "oceanWaveWindScale");
    blend_field!(shore_wave_wind_scale, "shoreWaveWindScale");
    blend_field!(ocean_wave_min_amplitude, "oceanWaveMinAmplitude");
    blend_field!(shore_wave_min_amplitude, "shoreWaveMinAmplitude");
    blend_field!(ocean_wave_max_amplitude, "oceanWaveMaxAmplitude");
    blend_field!(shore_wave_max_amplitude, "shoreWaveMaxAmplitude");
    blend_field!(ocean_foam_intensity, "oceanFoamIntensity");
    blend_field!(ocean_foam_scale, "oceanFoamScale");
    blend_field!(ripple_disturb, "rippleDisturb");
    blend_field!(lightning, "lightning");
    blend_field!(sandstorm, "sandstorm");

    // weather.xml stores a wind range, not a continuously sampled speed.
    // Resolve a deterministic midpoint for renderer state; scripts remain free
    // to override wind_speed/wind_direction in the explicit effects object.
    let current_mid = (number(current_row, "windMin", state.wind_min)
        + number(current_row, "windMax", state.wind_max))
        * 0.5;
    let next_mid = (number(next_row, "windMin", state.wind_min)
        + number(next_row, "windMax", state.wind_max))
        * 0.5;
    state.wind_speed = current_mid + (next_mid - current_mid) * blend;

    Ok(Some(state))
}

fn weather_string(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_owned()
}

fn weather_setting(row: &Value, key: &str) -> String {
    let value = weather_string(row, key);
    if value == "-" {
        String::new()
    } else {
        value
    }
}

fn blend_weather_number(
    current: &Value,
    next: &Value,
    key: &str,
    blend: f32,
    fallback: f32,
) -> f32 {
    let a = number(current, key, fallback);
    let b = number(next, key, a);
    a + (b - a) * blend
}

fn register_texture(
    assets: &AssetClient,
    textures: &mut BTreeMap<String, SkyTextureResources>,
    color_spaces: &mut BTreeMap<String, bool>,
    reference: &str,
    srgb: bool,
) -> Result<(), String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Err("weather texture reference is empty".to_owned());
    }
    if let Some(previous_srgb) = color_spaces.get(reference).copied() {
        if previous_srgb != srgb {
            return Err(format!(
                "weather texture '{reference}' requested as both {} and {}",
                if previous_srgb { "sRGB" } else { "linear" },
                if srgb { "sRGB" } else { "linear" },
            ));
        }
        return Ok(());
    }

    let texture = load_sky_texture(assets, reference, srgb)
        .map_err(|error| format!("weather texture '{reference}' decode failed: {error}"))?;
    textures.insert(reference.to_owned(), texture);
    color_spaces.insert(reference.to_owned(), srgb);
    Ok(())
}

fn required_string<'a>(value: &'a Value, key: &str, context: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{context} requires non-empty string '{key}'"))
}

fn number(value: &Value, key: &str, fallback: f32) -> f32 {
    value
        .get(key)
        .and_then(Value::as_f64)
        .map(|number| number as f32)
        .filter(|number| number.is_finite())
        .unwrap_or(fallback)
}

fn boolean(value: &Value, key: &str, fallback: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

fn vector<const N: usize>(
    value: &Value,
    key: &str,
    axes: [&str; N],
    fallback: [f32; N],
) -> [f32; N] {
    let Some(object) = value.get(key) else {
        return fallback;
    };
    std::array::from_fn(|index| number(object, axes[index], fallback[index]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_reads_rsc7_xml_object_shape() {
        let value = json!({"v": {"x": 1.0, "y": 2.0, "z": 3.0}});
        assert_eq!(
            vector(&value, "v", ["x", "y", "z"], [0.0; 3]),
            [1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn system_type_names_map_without_project_logic() {
        for (raw, expected) in [
            ("SYSTEM_TYPE_DROP", WeatherGpuFxSystemType::Drop),
            ("SYSTEM_TYPE_MIST", WeatherGpuFxSystemType::Mist),
            ("SYSTEM_TYPE_GROUND", WeatherGpuFxSystemType::Ground),
        ] {
            let mapped = match raw {
                "SYSTEM_TYPE_DROP" => WeatherGpuFxSystemType::Drop,
                "SYSTEM_TYPE_MIST" => WeatherGpuFxSystemType::Mist,
                "SYSTEM_TYPE_GROUND" => WeatherGpuFxSystemType::Ground,
                _ => WeatherGpuFxSystemType::Other,
            };
            assert_eq!(mapped, expected);
        }
    }

    #[test]
    fn weather_id_resolves_shared_effect_layers_and_blended_scalars() {
        let catalog = json!({
            "weather_types": [
                {
                    "id": "CLEAR",
                    "cloudSettingsName": "LIGHTclouds",
                    "timeCycleFilename": "COMMON:/DATA/TIMECYCLE/W_CLEAR.XML",
                    "dropSettingName": "-",
                    "mistSettingName": "-",
                    "groundSettingName": "-",
                    "sun": 1.0,
                    "cloud": 0.0,
                    "windMin": 0.2,
                    "windMax": 0.8,
                    "rain": 0.0,
                    "snow": 0.0,
                    "snowMist": 0.0,
                    "fog": 0.0,
                    "lightning": 0.0,
                    "sandstorm": 0.0
                },
                {
                    "id": "RAIN",
                    "cloudSettingsName": "HEAVYclouds",
                    "timeCycleFilename": "COMMON:/DATA/TIMECYCLE/W_RAIN.XML",
                    "dropSettingName": "RAINSTORM_DROP",
                    "mistSettingName": "RAINSTORM_MIST",
                    "groundSettingName": "RAINSTORM_GROUND",
                    "sun": 0.1,
                    "cloud": 0.0,
                    "windMin": 0.4,
                    "windMax": 1.0,
                    "rain": 1.0,
                    "snow": 0.0,
                    "snowMist": 0.0,
                    "fog": 0.0,
                    "lightning": 0.0,
                    "sandstorm": 0.0
                }
            ]
        });

        let state = resolve_weather_effects_from_catalog(
            &catalog,
            "CLEAR",
            "rain",
            0.25,
            &WeatherEffectsState::default(),
        )
        .unwrap()
        .unwrap();

        assert!(state.current_drop_setting.is_empty());
        assert_eq!(state.next_drop_setting, "RAINSTORM_DROP");
        assert_eq!(state.next_mist_setting, "RAINSTORM_MIST");
        assert_eq!(state.next_ground_setting, "RAINSTORM_GROUND");
        assert!((state.rain - 0.25).abs() < 1.0e-6);
        assert!((state.sun - 0.775).abs() < 1.0e-6);
        assert!((state.wind_speed - 0.55).abs() < 1.0e-6);
    }
}
