use super::*;

pub(super) fn load_environment_atmospheric_clouds(
    config: &ProjectAtmosphericClouds,
) -> Result<AtmosphericCloudResources, String> {
    let assets = AssetClient::new();
    let mut resources = builtin_resource_manager();
    let mut layers = Vec::with_capacity(config.layers.len());
    let mut mesh_cache =
        BTreeMap::<(String, usize), (String, AtmosphericCloudMeshResources)>::new();
    let mut texture_cache = BTreeMap::<String, SkyTextureResources>::new();

    let neutral_normal = SkyTextureResources {
        name: "newviso.clouds.neutral_normal".to_owned(),
        width: 1,
        height: 1,
        srgb: false,
        rgba8: vec![128, 128, 255, 255],
    };

    for layer in &config.layers {
        let (model_name, mesh) = if let Some(reference) = layer.model.as_deref() {
            let cache_key = (reference.to_owned(), layer.model_mesh_index);
            if let Some(cached) = mesh_cache.get(&cache_key) {
                cached.clone()
            } else {
                let address = AssetAddress::parse(reference).map_err(|error| {
                    format!(
                        "invalid atmospheric cloud model address '{}' for layer '{}': {error}",
                        reference, layer.id
                    )
                })?;
                let model = resources.load::<ModelResource>(&address)?;
                let mesh = model.meshes.get(layer.model_mesh_index).ok_or_else(|| {
                    format!(
                        "atmospheric cloud model '{}' has no mesh index {} (mesh_count={})",
                        model.name,
                        layer.model_mesh_index,
                        model.meshes.len()
                    )
                })?;
                let stream = |semantic: VertexSemantic, label: &str| {
                    mesh.vertex_streams
                        .iter()
                        .find(|stream| stream.semantic == semantic)
                        .ok_or_else(|| format!("cloud mesh '{}' has no {label} stream", mesh.name))
                };
                let position = stream(VertexSemantic::Position, "position")?;
                let normal = stream(VertexSemantic::Normal, "normal")?;
                let tangent = stream(VertexSemantic::Tangent, "tangent")?;
                let color = stream(VertexSemantic::Color(0), "color0")?;
                let uv = stream(VertexSemantic::TexCoord(0), "texcoord0")?;
                let vertex_count = position.vertex_count;
                for candidate in [normal, tangent, color, uv] {
                    if candidate.vertex_count != vertex_count {
                        return Err(format!(
                            "cloud mesh '{}' vertex stream {:?} count={} differs from position count={}",
                            mesh.name, candidate.semantic, candidate.vertex_count, vertex_count
                        ));
                    }
                }
                let vertices = (0..vertex_count as usize)
                    .map(|index| {
                        Ok(AtmosphericCloudVertex {
                            position: read_sky_vec3(position, index)?,
                            normal: read_sky_vec3(normal, index)?,
                            tangent: read_sky_vec4(tangent, index)?,
                            color: read_sky_vec4(color, index)?,
                            uv: read_sky_vec2(uv, index)?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let (index_format, indices) = decode_sky_indices(&mesh.index_buffer)?;
                let resolved = (
                    model.name.clone(),
                    AtmosphericCloudMeshResources {
                        name: mesh.name.clone(),
                        bounds_min: model.bounds.min,
                        bounds_max: model.bounds.max,
                        vertices,
                        indices,
                        index_format,
                    },
                );
                mesh_cache.insert(cache_key, resolved.clone());
                resolved
            }
        } else {
            (
                "newviso.generated.atmospheric_cloud_sheet".to_owned(),
                generated_atmospheric_cloud_sheet(&layer.id),
            )
        };

        let density_ref = layer
            .density_texture
            .as_deref()
            .or(layer.texture.as_deref())
            .unwrap_or("textures/skydome.ytd@baseperlinnoise3channel");
        let density = load_cached_cloud_texture(&assets, &mut texture_cache, density_ref, false)?;

        let normal = if let Some(reference) = layer.normal_texture.as_deref() {
            load_cached_cloud_texture(&assets, &mut texture_cache, reference, false)?
        } else {
            neutral_normal.clone()
        };

        let detail1_present =
            layer.detail_density_texture.is_some() && layer.detail_normal_texture.is_some();
        let detail2_present =
            layer.detail_density2_texture.is_some() && layer.detail_normal2_texture.is_some();

        let detail_density = if let Some(reference) = layer.detail_density_texture.as_deref() {
            load_cached_cloud_texture(&assets, &mut texture_cache, reference, false)?
        } else {
            density.clone()
        };
        let detail_normal = if let Some(reference) = layer.detail_normal_texture.as_deref() {
            load_cached_cloud_texture(&assets, &mut texture_cache, reference, false)?
        } else {
            neutral_normal.clone()
        };
        let detail_density2 = if let Some(reference) = layer.detail_density2_texture.as_deref() {
            load_cached_cloud_texture(&assets, &mut texture_cache, reference, false)?
        } else {
            density.clone()
        };
        let detail_normal2 = if let Some(reference) = layer.detail_normal2_texture.as_deref() {
            load_cached_cloud_texture(&assets, &mut texture_cache, reference, false)?
        } else {
            neutral_normal.clone()
        };

        let uv_layers = layer.uv_layers.map(|uv| AtmosphericCloudUvLayerDesc {
            enabled: uv.enabled,
            mode: match uv.mode {
                ProjectAtmosphericCloudAnimMode::Combine => AtmosphericCloudAnimMode::Combine,
                ProjectAtmosphericCloudAnimMode::Sculpt => AtmosphericCloudAnimMode::Sculpt,
            },
            velocity: uv.velocity,
            scale: uv.scale,
            weight: uv.weight,
        });

        layers.push(AtmosphericCloudLayerResources {
            model_name,
            mesh,
            textures: AtmosphericCloudTextureSet {
                refs: [
                    density_ref.to_owned(),
                    layer
                        .normal_texture
                        .as_deref()
                        .unwrap_or("__neutral_normal")
                        .to_owned(),
                    layer
                        .detail_density_texture
                        .as_deref()
                        .unwrap_or(density_ref)
                        .to_owned(),
                    layer
                        .detail_normal_texture
                        .as_deref()
                        .unwrap_or("__neutral_normal")
                        .to_owned(),
                    layer
                        .detail_density2_texture
                        .as_deref()
                        .unwrap_or(density_ref)
                        .to_owned(),
                    layer
                        .detail_normal2_texture
                        .as_deref()
                        .unwrap_or("__neutral_normal")
                        .to_owned(),
                ],
                density,
                normal,
                detail_density,
                detail_normal,
                detail_density2,
                detail_normal2,
                detail_present: [detail1_present, detail2_present],
            },
            desc: AtmosphericCloudLayerDesc {
                id: layer.id.clone(),
                position: layer.position,
                rotation_degrees: layer.rotation_degrees,
                scale: layer.scale,
                angular_velocity_degrees: layer.angular_velocity_degrees,
                rotation_scale: layer.rotation_scale,
                camera_position_scale: layer.camera_position_scale,
                altitude_min: layer.altitude_min,
                altitude_min_fade: layer.altitude_min_fade,
                altitude_max: layer.altitude_max,
                altitude_max_fade: layer.altitude_max_fade,
                transition_seconds: layer.transition_seconds,
                transition_in_time_percent: layer.transition_in_time_percent,
                transition_out_time_percent: layer.transition_out_time_percent,
                transition_delay_percent: layer.transition_delay_percent,
                transition_midpoint: layer
                    .transition_midpoint
                    .unwrap_or(config.transition_midpoint),
                transition_alpha_range: layer
                    .transition_alpha_range
                    .unwrap_or(config.transition_alpha_range),
                cost_factor: layer.cost_factor,
                soft_intersection_distance: layer.soft_intersection_distance,
                density: layer.density,
                softness: layer.softness,
                opacity: layer.opacity,
                color: layer.color,
                density_shift_scale: layer.density_shift_scale,
                scatter: layer.scatter,
                piercing: layer.piercing,
                scale_diffuse_fill_ambient: layer.scale_diffuse_fill_ambient,
                wrap_lighting: layer.wrap_lighting,
                rescale_uv: layer.rescale_uv,
                layer_anim_scale: layer.layer_anim_scale,
                weather_weights: layer.weather_weights.clone(),
                uv_layers,
            },
        });
    }

    host::info(
        "newviso.scene",
        format!(
            "atmospheric cloud semantic closure ready layers={} unique_meshes={} unique_textures={} enabled={} cloud_hat_speed={:.3}",
            layers.len(),
            mesh_cache.len(),
            texture_cache.len(),
            config.enabled,
            config.cloud_hat_speed
        ),
    );

    Ok(AtmosphericCloudResources {
        enabled: config.enabled,
        cloud_hat_speed: config.cloud_hat_speed,
        wind_min_speed: config.wind_min_speed,
        wind_max_speed: config.wind_max_speed,
        altitude_scroll_scale: config.altitude_scroll_scale,
        global_alpha: config.global_alpha,
        transition_midpoint: config.transition_midpoint,
        transition_alpha_range: config.transition_alpha_range,
        streaming_budget: config.streaming_budget,
        soft_depth_resolution: config.soft_depth_resolution,
        layers,
    })
}

pub(super) fn load_cached_cloud_texture(
    assets: &AssetClient,
    cache: &mut BTreeMap<String, SkyTextureResources>,
    reference: &str,
    srgb: bool,
) -> Result<SkyTextureResources, String> {
    if let Some(texture) = cache.get(reference) {
        return Ok(texture.clone());
    }
    let texture = load_sky_texture(assets, reference, srgb)?;
    cache.insert(reference.to_owned(), texture.clone());
    Ok(texture)
}

pub(super) fn generated_atmospheric_cloud_sheet(id: &str) -> AtmosphericCloudMeshResources {
    // The fallback is deliberately a world-horizontal sheet. Cloud curvature
    // belongs to weather-scale density/lighting, not to a camera-centred dome:
    // bending the carrier mesh makes formations visibly climb out of the lower
    // hemisphere as the observer turns.
    const SEGMENTS: u32 = 16;
    let side = SEGMENTS + 1;
    let mut vertices = Vec::with_capacity((side * side) as usize);
    for z in 0..=SEGMENTS {
        let v = z as f32 / SEGMENTS as f32;
        let pz = v * 2.0 - 1.0;
        for x in 0..=SEGMENTS {
            let u = x as f32 / SEGMENTS as f32;
            let px = u * 2.0 - 1.0;
            vertices.push(AtmosphericCloudVertex {
                position: [px, 0.0, pz],
                normal: [0.0, 1.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
                uv: [u, v],
            });
        }
    }

    let mut indices = Vec::with_capacity((SEGMENTS * SEGMENTS * 6) as usize);
    for z in 0..SEGMENTS {
        for x in 0..SEGMENTS {
            let i0 = z * side + x;
            let i1 = i0 + 1;
            let i2 = i0 + side;
            let i3 = i2 + 1;
            indices.extend_from_slice(&[i0, i2, i3, i0, i3, i1]);
        }
    }

    AtmosphericCloudMeshResources {
        name: format!("generated_cloud_sheet_{id}"),
        bounds_min: [-1.0, 0.0, -1.0],
        bounds_max: [1.0, 0.0, 1.0],
        vertices,
        indices,
        index_format: SkyIndexFormat::U32,
    }
}
