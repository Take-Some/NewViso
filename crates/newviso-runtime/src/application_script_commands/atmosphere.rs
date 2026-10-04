use super::*;

impl EngineApplication {
    pub(super) fn apply_atmosphere_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "scene.light.upsert" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] scene.light.upsert requires string 'id'")
                })?;
                let light_type = match command
                    .get("light_type")
                    .or_else(|| command.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("point")
                    .trim()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "directional" => SceneLightType::Directional,
                    "point" => SceneLightType::Point,
                    "spot" => SceneLightType::Spot,
                    "area" => SceneLightType::Area,
                    other => {
                        return Err(format!(
                            "script command[{index}] unknown light_type '{other}'"
                        ));
                    }
                };
                let mut desc = SceneLightDesc::default();
                desc.light_type = light_type;
                if command.get("color").is_some() {
                    desc.color = command_vec3(command, "color", index)?;
                }
                if command.get("intensity").is_some() {
                    desc.intensity = command_number(command, "intensity", index)?;
                }
                if command.get("range").is_some() {
                    desc.range = command_number(command, "range", index)?;
                }
                if command.get("cone_inner_degrees").is_some() {
                    desc.cone_inner_degrees = command_number(command, "cone_inner_degrees", index)?;
                }
                if command.get("cone_outer_degrees").is_some() {
                    desc.cone_outer_degrees = command_number(command, "cone_outer_degrees", index)?;
                }
                if let Some(value) = command.get("casts_shadows") {
                    desc.casts_shadows = value.as_bool().ok_or_else(|| {
                        format!(
                        "script command[{index}] scene.light.upsert 'casts_shadows' must be boolean"
                    )
                    })?;
                }
                if command.get("shadow_bias").is_some() {
                    desc.shadow_bias = command_number(command, "shadow_bias", index)?;
                }
                if command.get("shadow_normal_bias").is_some() {
                    desc.shadow_normal_bias = command_number(command, "shadow_normal_bias", index)?;
                }
                if let Some(value) = command.get("shadow_resolution") {
                    let resolution = value.as_u64().ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.light.upsert 'shadow_resolution' must be unsigned integer"
                    )
                })?;
                    desc.shadow_resolution = u32::try_from(resolution).map_err(|_| {
                        format!(
                        "script command[{index}] scene.light.upsert shadow_resolution out of range"
                    )
                    })?;
                }
                if command.get("shadow_distance").is_some() {
                    desc.shadow_distance = command_number(command, "shadow_distance", index)?;
                }
                self.scene.upsert_runtime_light(id, desc)?;
            }
            "scene.cloudhat.keyframe.set" => {
                let current = self.scene.cloudhat_keyframe();
                let enabled = command
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let optional_vec4 = |key: &str, fallback: [f32; 4]| -> Result<[f32; 4], String> {
                    command
                        .get(key)
                        .map(|_| command_vec4(command, key, index))
                        .transpose()
                        .map(|value| value.unwrap_or(fallback))
                };
                self.scene.set_cloudhat_keyframe(CloudHatKeyframeState {
                    enabled,
                    cloud_color: optional_vec4("cloud_color", current.cloud_color)?,
                    cloud_light_color: optional_vec4(
                        "cloud_light_color",
                        current.cloud_light_color,
                    )?,
                    cloud_ambient_color: optional_vec4(
                        "cloud_ambient_color",
                        current.cloud_ambient_color,
                    )?,
                    cloud_sky_color: optional_vec4("cloud_sky_color", current.cloud_sky_color)?,
                    cloud_bounce_color: optional_vec4(
                        "cloud_bounce_color",
                        current.cloud_bounce_color,
                    )?,
                    cloud_east_color: optional_vec4("cloud_east_color", current.cloud_east_color)?,
                    cloud_west_color: optional_vec4("cloud_west_color", current.cloud_west_color)?,
                    scale_fill_colors: optional_vec4(
                        "scale_fill_colors",
                        current.scale_fill_colors,
                    )?,
                    density_shift_scale_scattering: optional_vec4(
                        "density_shift_scale_scattering",
                        current.density_shift_scale_scattering,
                    )?,
                    piercing_light: optional_vec4("piercing_light", current.piercing_light)?,
                    scale_diffuse_fill_ambient_wrap: optional_vec4(
                        "scale_diffuse_fill_ambient_wrap",
                        current.scale_diffuse_fill_ambient_wrap,
                    )?,
                })?;
            }
            "scene.sky_clouds.set" => {
                let current = self.scene.sky_clouds();
                let enabled = command
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(current.enabled);
                let coverage = command
                    .get("coverage")
                    .map(|_| command_number(command, "coverage", index))
                    .transpose()?
                    .unwrap_or(current.coverage);
                let density = command
                    .get("density")
                    .map(|_| command_number(command, "density", index))
                    .transpose()?
                    .unwrap_or(current.density);
                let softness = command
                    .get("softness")
                    .map(|_| command_number(command, "softness", index))
                    .transpose()?
                    .unwrap_or(current.softness);
                let scale = command
                    .get("scale")
                    .map(|_| command_number(command, "scale", index))
                    .transpose()?
                    .unwrap_or(current.scale);
                let detail_scale = command
                    .get("detail_scale")
                    .map(|_| command_number(command, "detail_scale", index))
                    .transpose()?
                    .unwrap_or(current.detail_scale);
                let speed = command
                    .get("speed")
                    .map(|_| command_vector::<2>(command, "speed", index))
                    .transpose()?
                    .unwrap_or(current.speed);
                let horizon_fade = command
                    .get("horizon_fade")
                    .map(|_| command_number(command, "horizon_fade", index))
                    .transpose()?
                    .unwrap_or(current.horizon_fade);
                let macro_scale = command
                    .get("macro_scale")
                    .map(|_| command_number(command, "macro_scale", index))
                    .transpose()?
                    .unwrap_or(current.macro_scale);
                let macro_strength = command
                    .get("macro_strength")
                    .map(|_| command_number(command, "macro_strength", index))
                    .transpose()?
                    .unwrap_or(current.macro_strength);
                let detail_strength = command
                    .get("detail_strength")
                    .map(|_| command_number(command, "detail_strength", index))
                    .transpose()?
                    .unwrap_or(current.detail_strength);
                let micro_strength = command
                    .get("micro_strength")
                    .map(|_| command_number(command, "micro_strength", index))
                    .transpose()?
                    .unwrap_or(current.micro_strength);
                let erosion_strength = command
                    .get("erosion_strength")
                    .map(|_| command_number(command, "erosion_strength", index))
                    .transpose()?
                    .unwrap_or(current.erosion_strength);
                let warp_strength = command
                    .get("warp_strength")
                    .map(|_| command_number(command, "warp_strength", index))
                    .transpose()?
                    .unwrap_or(current.warp_strength);
                let shape_contrast = command
                    .get("shape_contrast")
                    .map(|_| command_number(command, "shape_contrast", index))
                    .transpose()?
                    .unwrap_or(current.shape_contrast);
                let shear_speed = command
                    .get("shear_speed")
                    .map(|_| command_vector::<2>(command, "shear_speed", index))
                    .transpose()?
                    .unwrap_or(current.shear_speed);
                let seed_offset = command
                    .get("seed_offset")
                    .map(|_| command_vector::<2>(command, "seed_offset", index))
                    .transpose()?
                    .unwrap_or(current.seed_offset);
                let large_speed = command
                    .get("large_speed")
                    .map(|_| command_number(command, "large_speed", index))
                    .transpose()?
                    .unwrap_or(current.large_speed);
                let small_speed = command
                    .get("small_speed")
                    .map(|_| command_number(command, "small_speed", index))
                    .transpose()?
                    .unwrap_or(current.small_speed);
                let overall_detail_speed = command
                    .get("overall_detail_speed")
                    .map(|_| command_number(command, "overall_detail_speed", index))
                    .transpose()?
                    .unwrap_or(current.overall_detail_speed);
                let edge_detail_speed = command
                    .get("edge_detail_speed")
                    .map(|_| command_number(command, "edge_detail_speed", index))
                    .transpose()?
                    .unwrap_or(current.edge_detail_speed);
                let noise_phase_scale = command
                    .get("noise_phase_scale")
                    .map(|_| command_number(command, "noise_phase_scale", index))
                    .transpose()?
                    .unwrap_or(current.noise_phase_scale);

                self.scene.set_sky_clouds(SkyCloudDesc {
                    enabled,
                    coverage,
                    density,
                    softness,
                    scale,
                    detail_scale,
                    speed,
                    horizon_fade,
                    macro_scale,
                    macro_strength,
                    detail_strength,
                    micro_strength,
                    erosion_strength,
                    warp_strength,
                    shape_contrast,
                    shear_speed,
                    seed_offset,
                    large_speed,
                    small_speed,
                    overall_detail_speed,
                    edge_detail_speed,
                    noise_phase_scale,
                })?;
            }
            "scene.volumetric_clouds.set" => {
                let mut desc = self.scene.volumetric_clouds().unwrap_or_default();
                if let Some(value) = command.get("enabled") {
                    desc.enabled = value.as_bool().ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.volumetric_clouds.set 'enabled' must be boolean"
                    )
                })?;
                }
                macro_rules! optional_cloud_number {
                    ($field:ident, $name:literal) => {
                        if command.get($name).is_some() {
                            desc.$field = command_number(command, $name, index)?;
                        }
                    };
                }
                optional_cloud_number!(base_altitude, "base_altitude");
                optional_cloud_number!(top_altitude, "top_altitude");
                optional_cloud_number!(max_distance, "max_distance");
                optional_cloud_number!(resolution_scale, "resolution_scale");
                optional_cloud_number!(coverage, "coverage");
                optional_cloud_number!(density, "density");
                optional_cloud_number!(shape_scale, "shape_scale");
                optional_cloud_number!(detail_scale, "detail_scale");
                optional_cloud_number!(detail_strength, "detail_strength");
                optional_cloud_number!(erosion_strength, "erosion_strength");
                optional_cloud_number!(extinction, "extinction");
                optional_cloud_number!(scattering, "scattering");
                optional_cloud_number!(ambient, "ambient");
                optional_cloud_number!(phase_forward, "phase_forward");
                optional_cloud_number!(powder_strength, "powder_strength");
                optional_cloud_number!(temporal_blend, "temporal_blend");
                optional_cloud_number!(jitter_strength, "jitter_strength");
                if let Some(value) = command.get("ray_steps") {
                    let value = value.as_u64().ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.volumetric_clouds.set 'ray_steps' must be unsigned integer"
                    )
                })?;
                    desc.ray_steps = u32::try_from(value).map_err(|_| {
                        format!(
                        "script command[{index}] scene.volumetric_clouds.set ray_steps out of range"
                    )
                    })?;
                }
                if let Some(value) = command.get("light_steps") {
                    let value = value.as_u64().ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.volumetric_clouds.set 'light_steps' must be unsigned integer"
                    )
                })?;
                    desc.light_steps = u32::try_from(value).map_err(|_| {
                    format!(
                        "script command[{index}] scene.volumetric_clouds.set light_steps out of range"
                    )
                })?;
                }
                self.scene.set_volumetric_clouds(desc)?;
            }
            "scene.atmospheric_cloud_layer.target.set" => {
                let layer = command
                    .get("layer")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!(
                        "script command[{index}] atmospheric cloud target requires string 'layer'"
                    )
                    })?;
                let alpha = command_number(command, "alpha", index)?;
                let transition_seconds = command
                    .get("transition_seconds")
                    .map(|_| command_number(command, "transition_seconds", index))
                    .transpose()?
                    .unwrap_or(5.0);
                self.scene
                    .set_atmospheric_cloud_layer_target(layer, alpha, transition_seconds)?;
            }
            "scene.atmospheric_cloud_layer.target.clear" => {
                let layer = command
                    .get("layer")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!(
                        "script command[{index}] atmospheric cloud clear requires string 'layer'"
                    )
                    })?;
                self.scene.clear_atmospheric_cloud_layer_target(layer)?;
            }
            "scene.lens_flare.upsert" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] scene.lens_flare.upsert requires string 'id'")
                })?;
                let source = command.get("source").and_then(Value::as_str).ok_or_else(|| {
                format!("script command[{index}] scene.lens_flare.upsert requires string 'source'")
            })?;
                let enabled = command
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let intensity = if command.get("intensity").is_some() {
                    command_number(command, "intensity", index)?
                } else {
                    1.0
                };
                let scale = if command.get("scale").is_some() {
                    command_number(command, "scale", index)?
                } else {
                    1.0
                };
                let occlusion_test = command
                    .get("occlusion_test")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let elements_value = command
                .get("elements")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    format!("script command[{index}] scene.lens_flare.upsert requires array 'elements'")
                })?;
                let mut elements = Vec::with_capacity(elements_value.len());
                for (element_index, element) in elements_value.iter().enumerate() {
                    let kind = match element
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("ghost")
                        .trim()
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "halo" => LensFlareElementKind::Halo,
                        "ghost" => LensFlareElementKind::Ghost,
                        "streak" => LensFlareElementKind::Streak,
                        other => {
                            return Err(format!(
                            "script command[{index}] flare element[{element_index}] unknown kind '{other}'"
                        ));
                        }
                    };
                    elements.push(LensFlareElementDesc {
                        kind,
                        offset: command_number(element, "offset", element_index)?,
                        size: command_number(element, "size", element_index)?,
                        color: command_vec3(element, "color", element_index)?,
                        alpha: command_number(element, "alpha", element_index)?,
                    });
                }
                self.scene.upsert_lens_flare(
                    id,
                    LensFlareDesc {
                        source: source.to_owned(),
                        enabled,
                        intensity,
                        scale,
                        occlusion_test,
                        elements,
                    },
                )?;
            }
            "scene.lens_flare.remove" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] scene.lens_flare.remove requires string 'id'")
                })?;
                self.scene.remove_lens_flare(id);
            }
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
