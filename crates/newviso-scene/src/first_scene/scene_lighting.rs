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
        revision: 0,
        last_mutation_frame: 0,
        process_claims: SceneProcessClaims::default(),
        last_process_frame: None,
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
            self.world.activate_entity(id)?;
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
            self.world.activate_entity(id)?;
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
    pub fn sky_clouds(&self) -> SkyCloudDesc {
        self.sky_clouds
    }
    pub fn sky_atmosphere(&self) -> SkyAtmosphereDesc {
        self.sky_atmosphere
    }

    pub fn set_sky_atmosphere(&mut self, desc: SkyAtmosphereDesc) -> Result<(), String> {
        let colors = [
            desc.night_zenith,
            desc.night_horizon,
            desc.astronomical_zenith,
            desc.astronomical_horizon,
            desc.nautical_zenith,
            desc.nautical_horizon,
            desc.civil_zenith,
            desc.civil_horizon,
            desc.day_zenith,
            desc.day_horizon,
            desc.sunset_tint,
            desc.cloud_night,
            desc.cloud_twilight_shadow,
            desc.cloud_twilight_light,
            desc.cloud_day_shadow,
            desc.cloud_day_light,
            desc.star_tint,
            desc.silver_lining_tint,
        ];
        let invalid_color = colors
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > 64.0);
        let twilight = desc.twilight_altitudes;
        let daylight = desc.daylight_altitudes;
        let stars = desc.star_visibility_altitudes;
        let invalid = invalid_color
            || twilight.iter().any(|v| !v.is_finite() || v.abs() > 90.0)
            || !(twilight[0] < twilight[1]
                && twilight[1] < twilight[2]
                && twilight[2] < twilight[3])
            || daylight.iter().any(|v| !v.is_finite() || v.abs() > 90.0)
            || daylight[0] >= daylight[1]
            || !desc.horizon_power.is_finite()
            || !(0.01..=32.0).contains(&desc.horizon_power)
            || !desc.tonemap_shoulder.is_finite()
            || !(0.0..=16.0).contains(&desc.tonemap_shoulder)
            || !desc.sunset_strength.is_finite()
            || !(0.0..=64.0).contains(&desc.sunset_strength)
            || !desc.star_intensity.is_finite()
            || !(0.0..=64.0).contains(&desc.star_intensity)
            || stars.iter().any(|v| !v.is_finite() || v.abs() > 90.0)
            || stars[0] >= stars[1]
            || !desc.cloud_occlusion.is_finite()
            || !(0.0..=1.0).contains(&desc.cloud_occlusion)
            || !desc.silver_lining_strength.is_finite()
            || !(0.0..=64.0).contains(&desc.silver_lining_strength)
            || desc
                .cloud_alpha_range
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            || desc.cloud_alpha_range[0] > desc.cloud_alpha_range[1];
        if invalid {
            return Err("invalid generic SkyAtmosphereDesc parameters".to_owned());
        }
        self.sky_atmosphere = desc;
        Ok(())
    }
    pub fn set_sky_clouds(&mut self, desc: SkyCloudDesc) -> Result<(), String> {
        if !desc.coverage.is_finite()
            || !(0.0..=1.0).contains(&desc.coverage)
            || !desc.density.is_finite()
            || !(0.0..=2.0).contains(&desc.density)
            || !desc.softness.is_finite()
            || !(0.01..=1.0).contains(&desc.softness)
            || !desc.scale.is_finite()
            || !(0.05..=32.0).contains(&desc.scale)
            || !desc.detail_scale.is_finite()
            || !(0.1..=64.0).contains(&desc.detail_scale)
            || desc
                .speed
                .iter()
                .any(|value| !value.is_finite() || value.abs() > 4.0)
            || !desc.horizon_fade.is_finite()
            || !(0.0..=1.0).contains(&desc.horizon_fade)
            || !desc.macro_scale.is_finite()
            || !(0.02..=8.0).contains(&desc.macro_scale)
            || !desc.macro_strength.is_finite()
            || !(0.0..=2.0).contains(&desc.macro_strength)
            || !desc.detail_strength.is_finite()
            || !(0.0..=2.0).contains(&desc.detail_strength)
            || !desc.micro_strength.is_finite()
            || !(0.0..=2.0).contains(&desc.micro_strength)
            || !desc.erosion_strength.is_finite()
            || !(0.0..=2.0).contains(&desc.erosion_strength)
            || !desc.warp_strength.is_finite()
            || !(0.0..=1.0).contains(&desc.warp_strength)
            || !desc.shape_contrast.is_finite()
            || !(0.25..=4.0).contains(&desc.shape_contrast)
            || desc
                .shear_speed
                .iter()
                .any(|value| !value.is_finite() || value.abs() > 4.0)
            || desc
                .seed_offset
                .iter()
                .any(|value| !value.is_finite() || value.abs() > 4096.0)
        {
            return Err("invalid generic SkyCloudDesc parameters".to_owned());
        }
        self.sky_clouds = desc;
        Ok(())
    }

    pub fn scene_environment(&self) -> SceneEnvironmentDesc {
        self.scene_environment
    }

    pub fn set_scene_environment(&mut self, desc: SceneEnvironmentDesc) -> Result<(), String> {
        let invalid_color = desc
            .ambient_color
            .iter()
            .chain(desc.fog_color.iter())
            .chain(desc.haze_color.iter())
            .any(|value| !value.is_finite() || *value < 0.0 || *value > 64.0);
        let invalid = invalid_color
            || !desc.ambient_intensity.is_finite()
            || !(0.0..=64.0).contains(&desc.ambient_intensity)
            || !desc.fog_density.is_finite()
            || !(0.0..=16.0).contains(&desc.fog_density)
            || !desc.fog_start_distance.is_finite()
            || desc.fog_start_distance < 0.0
            || !desc.fog_height_falloff.is_finite()
            || !(0.0..=16.0).contains(&desc.fog_height_falloff)
            || !desc.fog_base_height.is_finite()
            || !desc.fog_max_opacity.is_finite()
            || !(0.0..=1.0).contains(&desc.fog_max_opacity)
            || !desc.haze_density.is_finite()
            || !(0.0..=16.0).contains(&desc.haze_density)
            || !desc.haze_start_distance.is_finite()
            || desc.haze_start_distance < 0.0;
        if invalid {
            return Err("invalid generic SceneEnvironmentDesc parameters".to_owned());
        }
        self.scene_environment = desc;
        Ok(())
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

        self.set_sky_clouds(sky.clouds)?;
        self.sky = Some(sky);
        Ok(())
    }
}
