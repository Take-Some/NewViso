use super::*;

impl Scene3dRuntime {
    pub fn load_from_asset(logical_path: &str) -> Result<(Self, Scene3dLoadReport), String> {
        let logical_path = logical_path.trim().replace('\\', "/");
        let logical_path = logical_path.trim_start_matches('/');
        if logical_path.is_empty() {
            return Err("startup scene logical path is empty".to_owned());
        }

        let bytes =
            host_runtime::call_service(ASSET_SERVICE, "asset.text_v1", logical_path.as_bytes())
                .map_err(|error| format!("failed to load scene asset '{logical_path}': {error}"))?;

        let scene: Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid scene asset '{logical_path}': {error}"))?;

        Self::load_scene_value(scene)
    }
    pub(super) fn load_scene_value(scene: Value) -> Result<(Self, Scene3dLoadReport), String> {
        let mut portal_visibility = PortalVisibilityGraph::from_scene_value(&scene)?;
        let load = host_runtime::call_json(
            SCENE_SERVICE,
            "scene.load_json_v1",
            &json!({
                "replace": true,
                "scene": scene
            }),
        )?;

        if load.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(format!("Flecs rejected NewViso scene: {load}"));
        }

        let save = host_runtime::call_json(
            SCENE_SERVICE,
            "scene.save_json_v1",
            &json!({
                "path": "",
                "pretty": false
            }),
        )?;

        let snapshot = save
            .get("payload")
            .ok_or_else(|| format!("Flecs scene snapshot has no payload: {save}"))?;
        let entities = snapshot
            .get("entities")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("Flecs scene snapshot has no entities: {snapshot}"))?;

        let mut camera_record: Option<&Value> = None;
        let mut camera_entity_id: Option<u64> = None;
        let mut mesh_records: Vec<(u64, &Value)> = Vec::new();
        let mut logic_records: Vec<(u64, &Value)> = Vec::new();

        for (snapshot_index, entity) in entities.iter().enumerate() {
            let Some(record) = entity
                .get("components")
                .and_then(|components| components.get("newengine.scene.entity"))
            else {
                continue;
            };

            let stable_id = entity
                .get("handle")
                .and_then(|handle| handle.get("stable_id"))
                .and_then(Value::as_u64)
                .unwrap_or((snapshot_index + 1) as u64);

            match record.get("kind").and_then(Value::as_str) {
                Some("camera") if camera_record.is_none() => {
                    camera_record = Some(record);
                    camera_entity_id = Some(stable_id);
                }
                Some("mesh") => mesh_records.push((stable_id, record)),
                Some(_) => logic_records.push((stable_id, record)),
                None => {}
            }
        }

        let camera_record =
            camera_record.ok_or_else(|| "NewViso scene snapshot has no camera".to_owned())?;
        let mesh_record = mesh_records
            .first()
            .map(|(_, record)| *record)
            .ok_or_else(|| "NewViso scene snapshot has no mesh".to_owned())?;
        let camera_entity_id =
            camera_entity_id.ok_or_else(|| "scene camera entity has no stable id".to_owned())?;

        let camera_transform = camera_record
            .get("transform")
            .ok_or_else(|| "scene camera has no transform".to_owned())?;
        let camera_desc = camera_record
            .get("camera")
            .ok_or_else(|| "scene camera has no camera component".to_owned())?;

        for key in ["position", "target", "up"] {
            if camera_transform.get(key).is_none() {
                return Err(format!("project camera transform requires '{key}'"));
            }
        }
        for key in ["fov_y_degrees", "near", "far"] {
            if camera_desc.get(key).is_none() {
                return Err(format!("project camera component requires '{key}'"));
            }
        }
        let camera = Camera {
            position: read_vec3(camera_transform, "position", Vec3::ZERO)?,
            target: read_vec3(camera_transform, "target", Vec3::ZERO)?,
            up: read_vec3(camera_transform, "up", Vec3::Y)?,
            fov_y_degrees: read_f32(camera_desc, "fov_y_degrees", 60.0)?,
            near: read_f32(camera_desc, "near", 0.1)?,
            far: read_f32(camera_desc, "far", 100.0)?,
        };

        let orbit = OrbitCamera::from_camera(&camera);

        let mut world = SceneWorld::new(camera.position);
        world.add_entity(SceneEntity {
            id: SceneEntityId(camera_entity_id),
            name: camera_record
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Camera")
                .to_owned(),
            kind: SceneEntityKind::Camera,
            mobility: SceneMobility::Dynamic,
            lifecycle: SceneLifecycle::Constructed,
            transform: SceneTransform {
                position: camera.position,
                rotation_degrees: Vec3::ZERO,
                scale: Vec3::ONE,
            },
            light: None,
            bounds: SceneBounds::from_center_half_extent(
                camera.position,
                Vec3::new(0.05, 0.05, 0.05),
            ),
            parent: None,
            children: Vec::new(),
            visibility: VisibilityMask::default(),
            lod: SceneLodPolicy::default(),
            solid: false,
            collision_local_bounds: None,
            destructible: None,
            asset_ref: None,
            resident_geometry: false,
            render_slot: None,
            residency: SceneResidency::Resident,
            priority_score: 0.0,
            lod_alpha: 1.0,
            last_visible_frame: None,
            revision: 0,
            last_mutation_frame: 0,
            process_claims: SceneProcessClaims::default(),
            last_process_frame: None,
        })?;

        let mut cubes = Vec::with_capacity(mesh_records.len());
        let mut parent_links = Vec::<(SceneEntityId, SceneEntityId)>::new();
        for (stable_id, record) in mesh_records {
            let transform = record.get("transform").ok_or("mesh has no transform")?;
            let primitive = record
                .pointer("/mesh/primitive")
                .and_then(Value::as_str)
                .unwrap_or("");
            let asset_ref = record
                .pointer("/mesh/asset")
                .or_else(|| record.pointer("/mesh/model"))
                .and_then(Value::as_str)
                .map(str::to_owned);

            if primitive != "cube" {
                let Some(asset_ref) = asset_ref else {
                    return Err(format!(
                        "scene mesh '{}' has neither primitive='cube' nor mesh.asset",
                        record.get("name").and_then(Value::as_str).unwrap_or("Mesh")
                    ));
                };
                let position = read_vec3(transform, "position", Vec3::ZERO)?;
                let rotation_degrees = read_vec3(transform, "rotation_degrees", Vec3::ZERO)?;
                let scale = read_vec3(transform, "scale", Vec3::ONE)?;
                let scene_transform = SceneTransform {
                    position,
                    rotation_degrees,
                    scale,
                };
                let collision_local_bounds = read_collision_local_bounds(record)?;
                let half_extent = Vec3::new(
                    scale.x.abs().max(0.1) * 0.5,
                    scale.y.abs().max(0.1) * 0.5,
                    scale.z.abs().max(0.1) * 0.5,
                );
                // Collision-only scene entities (notably large RSC7 YBN sectors)
                // need their authored local bounds before the asset becomes resident.
                // Otherwise spatial streaming sees only a 1 m placeholder at the
                // sector origin and can leave nearby walls/floors unloaded.
                let initial_bounds = match collision_local_bounds {
                    Some(local) => asset_models::transformed_local_bounds(local, scene_transform)?,
                    None => SceneBounds::from_center_half_extent(position, half_extent),
                };
                let mobility = match record.get("mobility").and_then(Value::as_str) {
                    Some("dynamic") => SceneMobility::Dynamic,
                    _ => SceneMobility::Static,
                };
                world.add_entity(SceneEntity {
                    id: SceneEntityId(stable_id),
                    name: record
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("Mesh")
                        .to_owned(),
                    kind: match mobility {
                        SceneMobility::Static => SceneEntityKind::StaticMesh,
                        SceneMobility::Dynamic => SceneEntityKind::DynamicMesh,
                    },
                    mobility,
                    lifecycle: SceneLifecycle::Constructed,
                    transform: scene_transform,
                    light: None,
                    bounds: initial_bounds,
                    parent: None,
                    children: Vec::new(),
                    visibility: read_visibility_mask(record),
                    lod: read_lod_policy(record)?,
                    solid: record.pointer("/collider/solid").and_then(Value::as_bool) == Some(true),
                    collision_local_bounds,
                    destructible: read_destructible(record)?,
                    asset_ref: Some(asset_ref),
                    resident_geometry: false,
                    render_slot: None,
                    residency: SceneResidency::Unloaded,
                    priority_score: 0.0,
                    lod_alpha: 1.0,
                    last_visible_frame: None,
                    revision: 0,
                    last_mutation_frame: 0,
                    process_claims: SceneProcessClaims::default(),
                    last_process_frame: None,
                })?;
                if let Some(parent_id) = read_parent_id(record) {
                    parent_links.push((SceneEntityId(stable_id), SceneEntityId(parent_id)));
                }
                continue;
            }

            let material = record.get("material").ok_or("cube mesh has no material")?;
            if material.get("base_color").is_none() {
                return Err("cube mesh material requires project-authored 'base_color'".to_owned());
            }
            let cube = Cube {
                position: read_vec3(transform, "position", Vec3::ZERO)?,
                rotation_degrees: read_vec3(transform, "rotation_degrees", Vec3::ZERO)?,
                scale: read_vec3(transform, "scale", Vec3::ONE)?,
                base_color: read_color4(material, "base_color", [1.0, 1.0, 1.0, 1.0])?,
            };
            let bounds = cube.bounds();
            let render_slot = cubes.len();
            let solid = record.pointer("/collider/solid").and_then(Value::as_bool) == Some(true);
            let mobility = match record.get("mobility").and_then(Value::as_str) {
                Some("dynamic") => SceneMobility::Dynamic,
                _ => SceneMobility::Static,
            };
            let kind = match mobility {
                SceneMobility::Static => SceneEntityKind::StaticMesh,
                SceneMobility::Dynamic => SceneEntityKind::DynamicMesh,
            };
            let visibility = read_visibility_mask(record);
            let lod = read_lod_policy(record)?;

            world.add_entity(SceneEntity {
                id: SceneEntityId(stable_id),
                name: record
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Mesh")
                    .to_owned(),
                kind,
                mobility,
                lifecycle: SceneLifecycle::Constructed,
                transform: SceneTransform {
                    position: cube.position,
                    rotation_degrees: cube.rotation_degrees,
                    scale: cube.scale,
                },
                light: None,
                bounds: SceneBounds {
                    min: Vec3::new(bounds.min[0], bounds.min[1], bounds.min[2]),
                    max: Vec3::new(bounds.max[0], bounds.max[1], bounds.max[2]),
                },
                parent: None,
                children: Vec::new(),
                visibility,
                lod,
                solid,
                collision_local_bounds: read_collision_local_bounds(record)?,
                destructible: read_destructible(record)?,
                asset_ref: None,
                resident_geometry: false,
                render_slot: Some(render_slot),
                residency: SceneResidency::Resident,
                priority_score: 0.0,
                lod_alpha: 1.0,
                last_visible_frame: None,
                revision: 0,
                last_mutation_frame: 0,
                process_claims: SceneProcessClaims::default(),
                last_process_frame: None,
            })?;
            if let Some(parent_id) = read_parent_id(record) {
                parent_links.push((SceneEntityId(stable_id), SceneEntityId(parent_id)));
            }
            cubes.push(cube);
        }

        for (stable_id, record) in logic_records {
            let kind_name = record
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let kind = match kind_name {
                "light" => SceneEntityKind::Light,
                "trigger" => SceneEntityKind::Trigger,
                "portal" => SceneEntityKind::Portal,
                _ => SceneEntityKind::Unknown,
            };
            let mobility = match record.get("mobility").and_then(Value::as_str) {
                Some("dynamic") => SceneMobility::Dynamic,
                _ => SceneMobility::Static,
            };
            let transform_record = record.get("transform").unwrap_or(&Value::Null);
            let position = read_vec3(transform_record, "position", Vec3::ZERO)?;
            let rotation_degrees = read_vec3(transform_record, "rotation_degrees", Vec3::ZERO)?;
            let scale = read_vec3(transform_record, "scale", Vec3::ONE)?;
            let half_extent = Vec3::new(
                scale.x.abs().max(0.1) * 0.5,
                scale.y.abs().max(0.1) * 0.5,
                scale.z.abs().max(0.1) * 0.5,
            );

            world.add_entity(SceneEntity {
                id: SceneEntityId(stable_id),
                name: record
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(kind_name)
                    .to_owned(),
                kind,
                mobility,
                lifecycle: SceneLifecycle::Constructed,
                transform: SceneTransform {
                    position,
                    rotation_degrees,
                    scale,
                },
                light: None,
                bounds: SceneBounds::from_center_half_extent(position, half_extent),
                parent: None,
                children: Vec::new(),
                visibility: read_visibility_mask(record),
                lod: read_lod_policy(record)?,
                solid: false,
                collision_local_bounds: read_collision_local_bounds(record)?,
                destructible: read_destructible(record)?,
                asset_ref: record
                    .pointer("/asset/ref")
                    .or_else(|| record.pointer("/mesh/asset"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                resident_geometry: false,
                render_slot: None,
                residency: SceneResidency::Resident,
                priority_score: 0.0,
                lod_alpha: 1.0,
                last_visible_frame: None,
                revision: 0,
                last_mutation_frame: 0,
                process_claims: SceneProcessClaims::default(),
                last_process_frame: None,
            })?;
            if let Some(parent_id) = read_parent_id(record) {
                parent_links.push((SceneEntityId(stable_id), SceneEntityId(parent_id)));
            }
        }

        for (child, parent) in parent_links {
            world.set_parent(child, Some(parent))?;
        }
        world.activate_all();
        // Map/scene data establishes revision zero.  Runtime mutation events
        // begin only after the initial graph has been fully assembled.
        world.seal_initial_state();
        if let Some(graph) = portal_visibility.as_mut() {
            graph.bind_entities(&world);
        }
        let frame_plan = SceneFramePlan::default();

        let title = snapshot
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Scene")
            .to_owned();
        let camera_name = camera_record
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Camera")
            .to_owned();
        let mesh_name = mesh_record
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Mesh")
            .to_owned();

        let report = Scene3dLoadReport {
            title: title.clone(),
            entity_count: entities.len(),
            camera_name,
            mesh_name,
        };

        Ok((
            Self {
                title,
                camera_entity_id,
                camera,
                orbit,
                render_policy: RenderPolicy::default(),
                cubes,
                asset_meshes: BTreeMap::new(),
                skinned_entities: BTreeMap::new(),
                entity_skeletons: BTreeMap::new(),
                joint_attachments: BTreeMap::new(),
                arm_ik_constraints: BTreeMap::new(),
                animation_skinning_pool: animation_skinning::SkinningWorkerPool::new()?,
                animation_skinning_in_flight: BTreeSet::new(),
                main_view_mesh_visibility: Default::default(),
                static_asset_instance_epoch: 0,
                asset_model_cache: BTreeMap::new(),
                asset_draw_range_cache: BTreeMap::new(),
                asset_model_gpu_ranges: BTreeMap::new(),
                asset_gpu_textures: BTreeMap::new(),
                asset_gpu_pending_textures: BTreeMap::new(),
                asset_gpu_materials: BTreeMap::new(),
                vehicle_dashboards: BTreeMap::new(),
                dashboard_gpu_materials: BTreeMap::new(),
                asset_binding_contexts: BTreeMap::new(),
                asset_vertex_data: Vec::new(),
                asset_upload_from_float: None,
                skinned_vertex_data: Vec::new(),
                skinned_dirty_ranges: std::array::from_fn(|_| Vec::new()),
                asset_geometry_full_rebuild: false,
                retired_asset_vertex_buffers: Vec::new(),
                transient_spheres: Vec::new(),
                surface_marks: Vec::new(),
                overlay_quads: Vec::new(),
                particles: Vec::new(),
                physical_particles: Vec::new(),
                physical_particle_spawns: Vec::new(),
                removed_physical_particles: Vec::new(),
                physical_particle_debris_enabled: false,
                next_physical_particle_serial: 0,
                particle_interiors: BTreeMap::new(),
                particle_interior_stats: ParticleInteriorStats::default(),
                particle_interior_contacts: 0,
                particle_textures: BTreeMap::new(),
                particle_gpu_textures: BTreeMap::new(),
                particle_gpu_materials: BTreeMap::new(),
                sky_visuals: BTreeMap::new(),
                lens_flares: BTreeMap::new(),
                sky_clouds: SkyCloudDesc::default(),
                sky_atmosphere: SkyAtmosphereDesc::default(),
                scene_environment: SceneEnvironmentDesc::default(),
                timecycle_backend: TimeCycleBackendState::default(),
                weather_backend: WeatherBackendState::default(),
                weather_effects: WeatherEffectsState::default(),
                cloudhat_keyframe: CloudHatKeyframeState::default(),
                weather_gpu_fx: None,
                weather_fx_time_seconds: 0.0,
                weather_wetness: 0.0,
                weather_outdoor_exposure: 1.0,
                sky_time_seconds: 0.0,
                sky_time_scale: 1.0,
                sky_cloud_noise_phase: [1.0, 1.0],
                sky_cloud_cycle_time_days: 0.0,
                atmospheric_clouds: None,
                atmospheric_cloud_runtime: Vec::new(),
                runtime_entity_ids: BTreeMap::new(),
                next_runtime_entity_id: 0x4e56_5343_0000_0000,
                world,
                frame_plan,
                portal_visibility,
                gpu_instance_table: GpuInstanceTable::default(),
                mass_instances: mass_instances::MassInstanceStore::default(),
                gpu_mass_instances: BTreeMap::new(),
                last_submission_stats: RenderSubmissionStats::default(),
                clear_color: [0.0, 0.0, 0.0, 1.0],
                gpu: None,
                sky: None,
                gpu_sky: None,
                gpu_atmospheric_clouds: None,
                gpu_volumetric_clouds: None,
                gpu_weather: None,
                frame_index: 0,
                last_render_camera: None,
            },
            report,
        ))
    }
}

fn read_collision_local_bounds(record: &Value) -> Result<Option<SceneBounds>, String> {
    let Some(value) = record.pointer("/collider/local_bounds") else {
        return Ok(None);
    };
    let min = read_vec3(value, "min", Vec3::ZERO)?;
    let max = read_vec3(value, "max", Vec3::ZERO)?;
    if min.x > max.x || min.y > max.y || min.z > max.z {
        return Err("scene collider.local_bounds min must not exceed max".to_owned());
    }
    if [min.x, min.y, min.z, max.x, max.y, max.z]
        .iter()
        .any(|value| !value.is_finite())
    {
        return Err("scene collider.local_bounds must be finite".to_owned());
    }
    Ok(Some(SceneBounds { min, max }))
}

fn read_destructible(record: &Value) -> Result<Option<SceneDestructible>, String> {
    let Some(value) = record.get("destructible") else {
        return Ok(None);
    };
    let object = value
        .as_object()
        .ok_or_else(|| "scene destructible component must be an object".to_owned())?;
    if object
        .get("enabled")
        .and_then(Value::as_bool)
        .is_some_and(|enabled| !enabled)
    {
        return Ok(None);
    }

    let number = |key: &str, default: f32| -> Result<f32, String> {
        match object.get(key) {
            Some(value) => value
                .as_f64()
                .map(|value| value as f32)
                .filter(|value| value.is_finite())
                .ok_or_else(|| format!("scene destructible '{key}' must be finite numeric")),
            None => Ok(default),
        }
    };

    let max_health = number("health", 100.0)?;
    SceneDestructible {
        max_health,
        health: max_health,
        impact_damage_threshold: number("impact_damage_threshold", 8.0)?,
        impact_damage_scale: number("impact_damage_scale", 0.5)?,
        break_impulse: number("break_impulse", 80.0)?,
        impulse_transfer: number("impulse_transfer", 0.85)?,
        density: number("density", 40.0)?,
        friction: number("friction", 0.65)?,
        restitution: number("restitution", 0.08)?,
        linear_damping: number("linear_damping", 0.12)?,
        angular_damping: number("angular_damping", 0.18)?,
        broken: false,
    }
    .validate()
    .map(Some)
}
