use super::*;

fn runtime_visual_entity(
    id: SceneEntityId,
    name: &str,
    kind: SceneEntityKind,
    light: Option<LightComponent>,
) -> SceneEntity {
    SceneEntity {
        id,
        name: name.to_owned(),
        kind,
        mobility: SceneMobility::Dynamic,
        lifecycle: SceneLifecycle::Constructed,
        transform: SceneTransform {
            position: Vec3::ZERO,
            rotation_degrees: Vec3::ZERO,
            scale: Vec3::ONE,
        },
        light,
        bounds: SceneBounds::from_center_half_extent(Vec3::ZERO, Vec3::new(0.1, 0.1, 0.1)),
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
    }
}

impl Scene3dRuntime {
    pub fn upsert_runtime_light(&mut self, key: &str, desc: SceneLightDesc) -> Result<u64, String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("runtime light id must not be empty".to_owned());
        }

        let light = LightComponent {
            light_type: match desc.light_type {
                SceneLightType::Directional => LightType::Directional,
                SceneLightType::Point => LightType::Point,
                SceneLightType::Spot => LightType::Spot,
                SceneLightType::Area => LightType::Area,
            },
            color: desc.color,
            intensity: desc.intensity,
            range: desc.range,
            cone_inner_degrees: desc.cone_inner_degrees,
            cone_outer_degrees: desc.cone_outer_degrees,
            casts_shadows: desc.casts_shadows,
            shadow_bias: desc.shadow_bias,
            shadow_normal_bias: desc.shadow_normal_bias,
            shadow_resolution: desc.shadow_resolution,
            shadow_distance: desc.shadow_distance,
        }
        .validate()?;

        let existing = self
            .runtime_entity_ids
            .get(key)
            .copied()
            .map(SceneEntityId)
            .or_else(|| self.world.entity_id_by_name(key));

        let id = if let Some(id) = existing {
            if let Some(entity) = self.world.entity(id) {
                if entity.kind != SceneEntityKind::Light && entity.light.is_none() {
                    return Err(format!(
                        "runtime entity '{}' already exists and is not a light",
                        key
                    ));
                }
            }
            self.world.set_light(id, Some(light))?;
            id
        } else {
            let id = SceneEntityId(self.next_runtime_entity_id);
            self.next_runtime_entity_id = self.next_runtime_entity_id.wrapping_add(1);
            self.world.add_entity(runtime_visual_entity(
                id,
                key,
                SceneEntityKind::Light,
                Some(light),
            ))?;
            self.world.activate_all();
            self.runtime_entity_ids.insert(key.to_owned(), id.0);
            id
        };

        Ok(id.0)
    }
    pub fn upsert_runtime_sky_visual(
        &mut self,
        key: &str,
        desc: SkyVisualDesc,
    ) -> Result<u64, String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("runtime sky visual id must not be empty".to_owned());
        }
        if desc
            .color
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
            || !desc.intensity.is_finite()
            || desc.intensity < 0.0
            || !desc.angular_size_degrees.is_finite()
            || desc.angular_size_degrees <= 0.0
            || desc.angular_size_degrees >= 90.0
            || !desc.halo_size_degrees.is_finite()
            || desc.halo_size_degrees < desc.angular_size_degrees
            || desc.halo_size_degrees >= 180.0
            || !desc.halo_intensity.is_finite()
            || desc.halo_intensity < 0.0
        {
            return Err("invalid generic SkyVisualDesc parameters".to_owned());
        }

        let existing = self
            .runtime_entity_ids
            .get(key)
            .copied()
            .map(SceneEntityId)
            .or_else(|| self.world.entity_id_by_name(key));

        let id = if let Some(id) = existing {
            let entity = self
                .world
                .entity_mut(id)
                .ok_or_else(|| format!("runtime entity '{}' disappeared", key))?;
            if !matches!(
                entity.kind,
                SceneEntityKind::SkyVisual | SceneEntityKind::Unknown
            ) {
                return Err(format!(
                    "runtime entity '{}' already exists and is not a sky visual",
                    key
                ));
            }
            entity.kind = SceneEntityKind::SkyVisual;
            id
        } else {
            let id = SceneEntityId(self.next_runtime_entity_id);
            self.next_runtime_entity_id = self.next_runtime_entity_id.wrapping_add(1);
            self.world.add_entity(runtime_visual_entity(
                id,
                key,
                SceneEntityKind::SkyVisual,
                None,
            ))?;
            self.world.activate_all();
            self.runtime_entity_ids.insert(key.to_owned(), id.0);
            id
        };

        self.sky_visuals.insert(key.to_owned(), desc);
        Ok(id.0)
    }
    pub fn remove_runtime_sky_visual(&mut self, key: &str) -> Result<(), String> {
        self.sky_visuals.remove(key);
        if self.runtime_entity_ids.contains_key(key) || self.world.entity_id_by_name(key).is_some()
        {
            self.remove_runtime_entity(key)?;
        }
        Ok(())
    }
    pub fn upsert_lens_flare(&mut self, key: &str, desc: LensFlareDesc) -> Result<(), String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("lens flare id must not be empty".to_owned());
        }
        if desc.source.trim().is_empty()
            || !desc.intensity.is_finite()
            || desc.intensity < 0.0
            || !desc.scale.is_finite()
            || desc.scale <= 0.0
            || desc.elements.len() > MAX_FLARE_ELEMENTS
            || desc.elements.iter().any(|element| {
                !element.offset.is_finite()
                    || !element.size.is_finite()
                    || element.size <= 0.0
                    || !element.alpha.is_finite()
                    || element.alpha < 0.0
                    || element
                        .color
                        .iter()
                        .any(|value| !value.is_finite() || *value < 0.0)
            })
        {
            return Err("invalid generic LensFlareDesc parameters".to_owned());
        }
        if self.lens_flares.len() >= MAX_LENS_FLARES && !self.lens_flares.contains_key(key) {
            return Err(format!(
                "lens flare count exceeds generic limit of {MAX_LENS_FLARES}"
            ));
        }
        self.lens_flares.insert(key.to_owned(), desc);
        Ok(())
    }
    pub fn remove_lens_flare(&mut self, key: &str) {
        self.lens_flares.remove(key);
    }
    pub fn set_sky_dome(&mut self, sky: SkyDomeResources) -> Result<(), String> {
        if self.gpu.is_some() || self.gpu_sky.is_some() {
            return Err(
                "sky dome resources must be assigned before renderer initialization".to_owned(),
            );
        }

        if sky.mesh.vertices.is_empty() {
            return Err(format!(
                "sky dome model '{}' has no vertices",
                sky.model_name
            ));
        }
        if sky.mesh.indices.is_empty() {
            return Err(format!(
                "sky dome model '{}' has no indices",
                sky.model_name
            ));
        }
        if !sky
            .mesh
            .bounds_min
            .iter()
            .chain(sky.mesh.bounds_max.iter())
            .all(|value| value.is_finite())
        {
            return Err(format!(
                "sky dome model '{}' has non-finite bounds",
                sky.model_name
            ));
        }
        if sky.mesh.vertices.iter().any(|vertex| {
            vertex
                .position
                .iter()
                .chain(vertex.uv.iter())
                .any(|value| !value.is_finite())
        }) {
            return Err(format!(
                "sky dome model '{}' has non-finite vertex data",
                sky.model_name
            ));
        }
        let vertex_count = sky.mesh.vertices.len() as u32;
        if let Some(index) = sky
            .mesh
            .indices
            .iter()
            .find(|index| **index >= vertex_count)
        {
            return Err(format!(
                "sky dome model '{}' has out-of-range index {} for {} vertices",
                sky.model_name, index, vertex_count
            ));
        }
        if sky.mesh.index_format == SkyIndexFormat::U16
            && sky
                .mesh
                .indices
                .iter()
                .any(|index| *index > u16::MAX as u32)
        {
            return Err(format!(
                "sky dome model '{}' declares U16 indices but contains an index > {}",
                sky.model_name,
                u16::MAX
            ));
        }

        self.sky = Some(sky);
        Ok(())
    }
}
