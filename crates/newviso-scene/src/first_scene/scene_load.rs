use super::*;

impl Scene3dRuntime {
    pub fn load_first_scene() -> Result<(Self, Scene3dLoadReport), String> {
        let scene: Value = serde_json::from_str(BUILTIN_FIRST_SCENE_JSON)
            .map_err(|error| format!("invalid built-in first_scene.json: {error}"))?;
        Self::load_scene_value(scene)
    }
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
        let camera_entity_id = camera_entity_id
            .ok_or_else(|| "MainCamera Flecs entity has no stable id".to_owned())?;

        let camera_transform = camera_record
            .get("transform")
            .ok_or_else(|| "MainCamera has no transform".to_owned())?;
        let camera_desc = camera_record
            .get("camera")
            .ok_or_else(|| "MainCamera has no camera component".to_owned())?;

        let camera = Camera {
            position: read_vec3(camera_transform, "position", Vec3::new(4.2, 3.0, 6.0))?,
            target: read_vec3(camera_transform, "target", Vec3::ZERO)?,
            up: read_vec3(camera_transform, "up", Vec3::Y)?,
            fov_y_degrees: read_f32(camera_desc, "fov_y_degrees", 58.0)?,
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
                .unwrap_or("MainCamera")
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
            asset_ref: None,
            render_slot: None,
            residency: SceneResidency::Resident,
            priority_score: 0.0,
            lod_alpha: 1.0,
            last_visible_frame: None,
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
                let half_extent = Vec3::new(
                    scale.x.abs().max(0.1) * 0.5,
                    scale.y.abs().max(0.1) * 0.5,
                    scale.z.abs().max(0.1) * 0.5,
                );
                let mobility = match record.get("mobility").and_then(Value::as_str) {
                    Some("dynamic") => SceneMobility::Dynamic,
                    _ => SceneMobility::Static,
                };
                world.add_entity(SceneEntity {
                    id: SceneEntityId(stable_id),
                    name: record
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("AssetMesh")
                        .to_owned(),
                    kind: match mobility {
                        SceneMobility::Static => SceneEntityKind::StaticMesh,
                        SceneMobility::Dynamic => SceneEntityKind::DynamicMesh,
                    },
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
                    solid: record.pointer("/collider/solid").and_then(Value::as_bool) == Some(true),
                    asset_ref: Some(asset_ref),
                    render_slot: None,
                    residency: SceneResidency::Unloaded,
                    priority_score: 0.0,
                    lod_alpha: 1.0,
                    last_visible_frame: None,
                })?;
                if let Some(parent_id) = read_parent_id(record) {
                    parent_links.push((SceneEntityId(stable_id), SceneEntityId(parent_id)));
                }
                continue;
            }

            let material = record.get("material").ok_or("cube mesh has no material")?;
            let cube = Cube {
                position: read_vec3(transform, "position", Vec3::ZERO)?,
                rotation_degrees: read_vec3(transform, "rotation_degrees", Vec3::ZERO)?,
                scale: read_vec3(transform, "scale", Vec3::ONE)?,
                base_color: read_color4(material, "base_color", [0.95, 0.42, 0.12, 1.0])?,
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
                asset_ref: None,
                render_slot: Some(render_slot),
                residency: SceneResidency::Resident,
                priority_score: 0.0,
                lod_alpha: 1.0,
                last_visible_frame: None,
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
                asset_ref: record
                    .pointer("/asset/ref")
                    .or_else(|| record.pointer("/mesh/asset"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                render_slot: None,
                residency: SceneResidency::Resident,
                priority_score: 0.0,
                lod_alpha: 1.0,
                last_visible_frame: None,
            })?;
            if let Some(parent_id) = read_parent_id(record) {
                parent_links.push((SceneEntityId(stable_id), SceneEntityId(parent_id)));
            }
        }

        for (child, parent) in parent_links {
            world.set_parent(child, Some(parent))?;
        }
        world.activate_all();
        let frame_plan = SceneFramePlan::default();

        let title = snapshot
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("NewViso 3D Scene")
            .to_owned();
        let camera_name = camera_record
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("MainCamera")
            .to_owned();
        let mesh_name = mesh_record
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Cube")
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
                cubes,
                transient_spheres: Vec::new(),
                overlay_quads: Vec::new(),
                sky_visuals: BTreeMap::new(),
                lens_flares: BTreeMap::new(),
                runtime_entity_ids: BTreeMap::new(),
                next_runtime_entity_id: 0x4e56_5343_0000_0000,
                world,
                frame_plan,
                clear_color: [0.025, 0.032, 0.045, 1.0],
                gpu: None,
                sky: None,
                gpu_sky: None,
                frame_index: 0,
            },
            report,
        ))
    }
}
