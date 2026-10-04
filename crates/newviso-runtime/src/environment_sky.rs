use super::*;

pub(super) fn load_environment_sky(
    config: &ProjectSkyEnvironment,
) -> Result<SkyDomeResources, String> {
    let assets = AssetClient::new();
    let model_address = AssetAddress::parse(&config.model)
        .map_err(|error| format!("invalid environment sky model address: {error}"))?;
    if model_address.entry().is_none() {
        return Err("environment sky model requires @entry".to_owned());
    }

    let mut resources = builtin_resource_manager();
    let model = resources.load::<ModelResource>(&model_address)?;
    let mesh = model
        .meshes
        .first()
        .ok_or_else(|| format!("sky model '{}' contains no meshes", model.name))?;
    let position = mesh
        .vertex_streams
        .iter()
        .find(|stream| stream.semantic == VertexSemantic::Position)
        .ok_or_else(|| format!("sky mesh '{}' has no position stream", mesh.name))?;
    let uv = mesh
        .vertex_streams
        .iter()
        .find(|stream| stream.semantic == VertexSemantic::TexCoord(0))
        .ok_or_else(|| format!("sky mesh '{}' has no texcoord0 stream", mesh.name))?;
    if position.vertex_count != uv.vertex_count {
        return Err(format!(
            "sky mesh '{}' position vertex_count={} differs from texcoord0 vertex_count={}",
            mesh.name, position.vertex_count, uv.vertex_count
        ));
    }

    let vertices = (0..position.vertex_count as usize)
        .map(|index| {
            Ok(SkyVertex {
                position: read_sky_vec3(position, index)?,
                uv: read_sky_vec2(uv, index)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let (index_format, indices) = decode_sky_indices(&mesh.index_buffer)?;

    let base_noise_ref = config
        .base_noise_texture
        .as_deref()
        .unwrap_or("textures/skydome.ytd@baseperlinnoise3channel");
    let starfield_ref = config
        .starfield_texture
        .as_deref()
        .unwrap_or("textures/skydome.ytd@starfield");
    let detail_noise_ref = config
        .detail_noise_texture
        .as_deref()
        .unwrap_or("textures/skydome.ytd@noise16_p");

    let base_noise = load_sky_texture(&assets, base_noise_ref, false)?;
    let starfield = load_sky_texture(&assets, starfield_ref, true)?;
    let detail_noise = load_sky_texture(&assets, detail_noise_ref, false)?;
    let billboard_texture = config
        .billboard_texture
        .as_deref()
        .map(|reference| load_sky_texture(&assets, reference, true))
        .transpose()?;

    let model_name = model.name.clone();
    let mesh_name = mesh.name.clone();
    let material_name = "environment.sky".to_owned();
    host::info(
        "newviso.scene",
        format!(
            "sky semantic closure ready model='{}' mesh='{}' vertices={} indices={} textures=[{},{},{}] billboard={}",
            model_name,
            mesh_name,
            vertices.len(),
            indices.len(),
            base_noise.name,
            starfield.name,
            detail_noise.name,
            billboard_texture
                .as_ref()
                .map(|texture| texture.name.as_str())
                .unwrap_or("-")
        ),
    );

    Ok(SkyDomeResources {
        model_name,
        material_name,
        mesh: SkyMeshResources {
            name: mesh_name,
            bounds_min: model.bounds.min,
            bounds_max: model.bounds.max,
            vertices,
            indices,
            index_format,
        },
        base_noise,
        starfield,
        detail_noise,
        billboard_texture,
        clouds: SkyCloudDesc {
            enabled: config.clouds.enabled,
            coverage: config.clouds.coverage,
            density: config.clouds.density,
            softness: config.clouds.softness,
            scale: config.clouds.scale,
            detail_scale: config.clouds.detail_scale,
            speed: config.clouds.speed,
            horizon_fade: config.clouds.horizon_fade,
            macro_scale: config.clouds.macro_scale,
            macro_strength: config.clouds.macro_strength,
            detail_strength: config.clouds.detail_strength,
            micro_strength: config.clouds.micro_strength,
            erosion_strength: config.clouds.erosion_strength,
            warp_strength: config.clouds.warp_strength,
            shape_contrast: config.clouds.shape_contrast,
            shear_speed: config.clouds.shear_speed,
            seed_offset: config.clouds.seed_offset,
            large_speed: config.clouds.large_speed,
            small_speed: config.clouds.small_speed,
            overall_detail_speed: config.clouds.overall_detail_speed,
            edge_detail_speed: config.clouds.edge_detail_speed,
            noise_phase_scale: config.clouds.noise_phase_scale,
        },
        volumetric_clouds: VolumetricCloudDesc {
            enabled: config.volumetric_clouds.enabled,
            base_altitude: config.volumetric_clouds.base_altitude,
            top_altitude: config.volumetric_clouds.top_altitude,
            max_distance: config.volumetric_clouds.max_distance,
            resolution_scale: config.volumetric_clouds.resolution_scale,
            ray_steps: config.volumetric_clouds.ray_steps,
            light_steps: config.volumetric_clouds.light_steps,
            coverage: config.volumetric_clouds.coverage,
            density: config.volumetric_clouds.density,
            shape_scale: config.volumetric_clouds.shape_scale,
            detail_scale: config.volumetric_clouds.detail_scale,
            detail_strength: config.volumetric_clouds.detail_strength,
            erosion_strength: config.volumetric_clouds.erosion_strength,
            extinction: config.volumetric_clouds.extinction,
            scattering: config.volumetric_clouds.scattering,
            ambient: config.volumetric_clouds.ambient,
            phase_forward: config.volumetric_clouds.phase_forward,
            powder_strength: config.volumetric_clouds.powder_strength,
            temporal_blend: config.volumetric_clouds.temporal_blend,
            jitter_strength: config.volumetric_clouds.jitter_strength,
        },
        dome_scale: config.dome_scale,
        horizon_level: config.horizon_level,
    })
}
