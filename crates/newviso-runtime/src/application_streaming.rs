use super::*;

impl EngineApplication {
    pub(super) fn scene_stream_owner(stable_id: u64) -> StreamingOwnerId {
        StreamingOwnerId::from_label(&format!("newviso.scene.entity.{stable_id}"))
    }

    fn release_scene_aux_claims(&mut self, stable_id: u64) {
        if let Some(addresses) = self.scene_stream_aux_claims.remove(&stable_id) {
            let owner = Self::scene_stream_owner(stable_id);
            for address in addresses {
                self.asset_streamer.release(owner, &address);
            }
        }
    }

    fn dematerialize_scene_asset(&mut self, stable_id: u64) -> Result<(), String> {
        self.scene.remove_entity_model(stable_id);
        if let Some(physics) = self.physics.as_mut() {
            physics.remove_streamed_collision(stable_id);
        }
        self.scene.mark_entity_unloaded(stable_id)
    }

    fn materialize_scene_asset(
        &mut self,
        stable_id: u64,
        address: &AssetAddress,
    ) -> Result<(), String> {
        let model = self.asset_streamer.get::<ModelResource>(address);
        let collision = self.asset_streamer.get::<CollisionMeshResource>(address);

        if let Some(model) = model.as_deref() {
            // Material storage is a policy detail. Built-in and external slots are
            // both resolved to the same MaterialResource contract before Scene sees them.
            let context = self.scene.entity_asset_binding_context(stable_id);
            let dictionary = context.texture_dictionary.as_deref();
            let request_priority = self
                .asset_streamer
                .snapshot(address)
                .map(|snapshot| snapshot.effective_priority)
                .unwrap_or(1.0)
                .max(0.0);
            let owner = Self::scene_stream_owner(stable_id);

            let resolve_material_texture_address =
                |binding: &newviso_materials::MaterialTextureBinding| -> Result<Option<AssetAddress>, String> {
                    if let Some(texture) = binding.texture.as_ref() {
                        return Ok(Some(texture.address().clone()));
                    }
                    let Some(texture_name) = binding.texture_name.as_deref() else {
                        return Ok(None);
                    };
                    let Some(dictionary) = dictionary else {
                        return Ok(None);
                    };
                    let dictionary = AssetAddress::parse(dictionary).map_err(|error| {
                        format!(
                            "scene entity {} texture_dictionary '{}' is invalid: {error}",
                            stable_id, dictionary
                        )
                    })?;
                    if dictionary.entry().is_some() {
                        return Err(format!(
                            "scene entity {} texture_dictionary '{}' must name a dictionary, not an @entry",
                            stable_id,
                            dictionary.canonical()
                        ));
                    }
                    Ok(Some(AssetAddress::parse(&format!(
                        "{}@{}",
                        dictionary.logical_path(),
                        texture_name
                    ))?))
                };

            let mut resolved_materials = Vec::with_capacity(model.material_slots.len());
            let mut pending_material_dependencies = 0usize;
            for slot in &model.material_slots {
                let material = match &slot.material {
                    newviso_model::ModelMaterialBinding::BuiltIn(material) => {
                        Some(material.clone())
                    }
                    newviso_model::ModelMaterialBinding::External(material_ref) => {
                        let material_address = material_ref.address();
                        match self
                            .asset_streamer
                            .get::<newviso_materials::MaterialResource>(material_address)
                        {
                            Some(material) => Some(material),
                            None => {
                                self.asset_streamer.request(
                                    owner,
                                    material_address.clone(),
                                    StreamingClaim::new(request_priority),
                                )?;
                                self.scene_stream_aux_claims
                                    .entry(stable_id)
                                    .or_default()
                                    .insert(material_address.clone());
                                pending_material_dependencies =
                                    pending_material_dependencies.saturating_add(1);
                                None
                            }
                        }
                    }
                };

                if let Some(material) = material.as_ref() {
                    for texture in &material.textures {
                        let Some(texture_address) = resolve_material_texture_address(texture)?
                        else {
                            if texture.required {
                                pending_material_dependencies =
                                    pending_material_dependencies.saturating_add(1);
                            }
                            continue;
                        };
                        if self
                            .asset_streamer
                            .get::<newviso_textures::TextureResource>(&texture_address)
                            .is_none()
                        {
                            self.asset_streamer.request(
                                owner,
                                texture_address.clone(),
                                StreamingClaim::new(request_priority),
                            )?;
                            self.scene_stream_aux_claims
                                .entry(stable_id)
                                .or_default()
                                .insert(texture_address);
                            if texture.required {
                                pending_material_dependencies =
                                    pending_material_dependencies.saturating_add(1);
                            }
                        }
                    }
                }
                resolved_materials.push(material);
            }

            if pending_material_dependencies > 0 {
                host::debug(
                    "newviso.assets.streaming",
                    format!(
                        "defer model materialization entity={} asset='{}' pending_material_dependencies={}",
                        stable_id,
                        address.canonical(),
                        pending_material_dependencies
                    ),
                );
                return Ok(());
            }

            let resolved_scene_materials = resolved_materials
                .iter()
                .enumerate()
                .map(|(slot_index, material)| {
                    let material = material.clone().ok_or_else(|| {
                        format!(
                            "model '{}' material slot {} resolved without MaterialResource",
                            model.name, slot_index
                        )
                    })?;
                    let resolve_role = |role: &str| -> Result<
                        Option<Arc<newviso_textures::TextureResource>>,
                        String,
                    > {
                        let Some(binding) = material.textures.iter().find(|binding| {
                            let name = binding.slot.trim().to_ascii_lowercase();
                            name == role
                                || (role == "base_color"
                                    && matches!(name.as_str(), "albedo" | "diffuse" | "base"))
                                || (role == "normal"
                                    && matches!(name.as_str(), "normal_map" | "normals"))
                                || (role == "specular"
                                    && matches!(name.as_str(), "spec" | "specular_map"))
                                || (role == "emissive"
                                    && matches!(name.as_str(), "emission" | "emissive_map"))
                                || (role == "environment"
                                    && matches!(name.as_str(), "environment_map" | "reflection"))
                        }) else {
                            return Ok(None);
                        };
                        let Some(texture_address) = resolve_material_texture_address(binding)?
                        else {
                            return Ok(None);
                        };
                        Ok(self
                            .asset_streamer
                            .get::<newviso_textures::TextureResource>(&texture_address))
                    };

                    let base_color = match resolve_role("base_color")? {
                        Some(texture) => Some(texture),
                        None => resolve_role("generic")?,
                    };
                    Ok(newviso_scene::SceneResolvedMaterial {
                        material: material.clone(),
                        base_color,
                        normal: resolve_role("normal")?,
                        specular: resolve_role("specular")?,
                        emissive: resolve_role("emissive")?,
                        environment: resolve_role("environment")?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            if self
                .scene
                .install_entity_model(stable_id, model, &resolved_scene_materials)?
            {
                host::debug(
                    "newviso.assets.streaming",
                    format!(
                        "materialized model entity={} asset='{}' meshes={}",
                        stable_id,
                        address.canonical(),
                        model.meshes.len()
                    ),
                );
            }
            let _ = self.apply_scene_animation_binding(stable_id)?;
        }

        if let Some(collision) = collision.as_deref() {
            if let Some(physics) = self.physics.as_mut() {
                let (position, rotation_degrees, scale) = self
                    .scene
                    .entity_transform_values(stable_id)
                    .ok_or_else(|| {
                        format!(
                            "scene entity {} disappeared during collision materialization",
                            stable_id
                        )
                    })?;
                if physics.install_streamed_collision(
                    stable_id,
                    collision,
                    position,
                    rotation_degrees,
                    scale,
                )? {
                    host::debug(
                        "newviso.assets.streaming",
                        format!(
                            "materialized collision entity={} asset='{}' vertices={} triangles={}",
                            stable_id,
                            address.canonical(),
                            collision.vertices.len(),
                            collision.triangles.len()
                        ),
                    );
                }
            }
        }

        if model.is_none() && collision.is_none() {
            return Err(format!(
                "scene asset '{}' became resident with unsupported runtime resource type",
                address.canonical()
            ));
        }

        self.scene.mark_entity_resident(stable_id)
    }
    pub(super) fn sync_scene_streaming_interests(&mut self) -> Result<(), String> {
        let interests = self.scene.streaming_interests();
        let active_ids = interests
            .iter()
            .map(|request| request.stable_id)
            .collect::<std::collections::BTreeSet<_>>();

        let stale_ids = self
            .scene_stream_claims
            .keys()
            .copied()
            .filter(|stable_id| !active_ids.contains(stable_id))
            .collect::<Vec<_>>();

        for stable_id in stale_ids {
            if let Some(address) = self.scene_stream_claims.remove(&stable_id) {
                self.asset_streamer
                    .release(Self::scene_stream_owner(stable_id), &address);
                self.release_scene_aux_claims(stable_id);
                self.dematerialize_scene_asset(stable_id)?;
            }
        }

        for request in interests {
            let address = AssetAddress::parse(&request.asset_ref).map_err(|error| {
                format!(
                    "scene entity {} has invalid asset_ref '{}': {error}",
                    request.stable_id, request.asset_ref
                )
            })?;

            if let Some(previous) = self.scene_stream_claims.get(&request.stable_id).cloned() {
                if previous != address {
                    self.asset_streamer
                        .release(Self::scene_stream_owner(request.stable_id), &previous);
                    self.release_scene_aux_claims(request.stable_id);
                    self.dematerialize_scene_asset(request.stable_id)?;
                }
            }

            let raw_priority = request.priority.max(0.0);
            let quantized_priority = (raw_priority * 4.0).round() * 0.25;
            self.asset_streamer.request(
                Self::scene_stream_owner(request.stable_id),
                address.clone(),
                StreamingClaim::new(quantized_priority),
            )?;
            self.scene_stream_claims.insert(request.stable_id, address);
        }

        Ok(())
    }
    pub(super) fn apply_scene_streaming_residency(&mut self) -> Result<(), String> {
        let mut claims = self
            .scene_stream_claims
            .iter()
            .map(|(stable_id, address)| (*stable_id, address.clone()))
            .collect::<Vec<_>>();
        // Append repeated model instances next to each other in the packed
        // static vertex stream. This lets the renderer coalesce contiguous
        // ranges sharing one material/bind group into fewer draw commands.
        claims.sort_by(|(a_id, a), (b_id, b)| {
            a.canonical()
                .cmp(&b.canonical())
                .then_with(|| a_id.cmp(b_id))
        });
        let model_budget = self.settings.streaming.max_model_materializations_per_frame;
        let mut model_materializations = 0usize;

        for (stable_id, address) in claims {
            if self.asset_streamer.is_resident(&address) {
                let pending_model_instance =
                    self.asset_streamer.get::<ModelResource>(&address).is_some()
                        && !self.scene.entity_model_installed(stable_id);

                if pending_model_instance
                    && model_budget != 0
                    && model_materializations >= model_budget
                {
                    continue;
                }

                self.materialize_scene_asset(stable_id, &address)?;
                if pending_model_instance && self.scene.entity_model_installed(stable_id) {
                    model_materializations = model_materializations.saturating_add(1);
                }
            } else {
                self.dematerialize_scene_asset(stable_id)?;
            }
        }
        Ok(())
    }
    pub(super) fn pump_asset_streaming(&mut self) -> Result<(), String> {
        let report = self.asset_streamer.pump();
        for (address, error) in &report.failed {
            host::warn(
                "newviso.assets.streaming",
                format!("asset='{}' streaming failed: {error}", address.canonical()),
            );
        }

        self.apply_scene_streaming_residency()?;

        if !report.loaded.is_empty()
            || !report.became_resident.is_empty()
            || !report.evicted.is_empty()
        {
            host::debug(
                "newviso.assets.streaming",
                format!(
                    "frame={} loaded={} resident_promotions={} evicted={} source_bytes_loaded={} resident_bytes={} over_budget={} parallel_loads={} model_materialization_budget={}",
                    report.frame,
                    report.loaded.len(),
                    report.became_resident.len(),
                    report.evicted.len(),
                    report.source_bytes_loaded,
                    report.resident_bytes,
                    report.over_budget,
                    self.settings.streaming.parallel_loads,
                    self.settings.streaming.max_model_materializations_per_frame
                ),
            );
        }
        Ok(())
    }
}
