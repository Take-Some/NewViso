//! Materialize resident scene models, resolving source material bindings.
//! Kept separate from streaming scheduling and background preparation.
use super::*;
use crate::application_streaming::{
    material_binding_blocks_model_materialization,
    material_binding_uses_local_base_color,
};

impl EngineApplication {
    pub(super) fn materialize_scene_asset(
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
                    // Most unresolved optional inputs are authored globals
                    // (environment, long_hair_noise, givemechecker, etc.) and
                    // must not be reinterpreted as entity-local YTD entries.
                    // Base-color samplers are the exception: legacy/streamed-ped
                    // cooks may know the texture name but leave the direct ref
                    // unresolved. If the entity explicitly supplies a texture
                    // dictionary, resolve that albedo there and treat it as a
                    // materialization dependency.
                    if !binding.required && !material_binding_uses_local_base_color(binding) {
                        return Ok(None);
                    }
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
                        texture_name.to_ascii_lowercase()
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
                            if material_binding_blocks_model_materialization(texture) {
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
                            if material_binding_blocks_model_materialization(texture) {
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
                    let native_multilayer =
                        material.shader.eq_ignore_ascii_case("newviso.multilayer");
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
                    let mut auxiliary_textures = BTreeMap::new();
                    for binding in &material.textures {
                        let role = binding.slot.trim().to_ascii_lowercase();
                        // These native multilayer roles already occupy the
                        // five fixed renderer samplers below. Keeping copies in
                        // auxiliary_textures repeated Arc references and GPU
                        // readiness checks for every one of the twenty layers.
                        if native_multilayer
                            && matches!(role.as_str(), "roughness" | "layer_mask" | "microblend")
                        {
                            continue;
                        }
                        if matches!(
                            role.as_str(),
                            "base_color"
                                | "albedo"
                                | "diffuse"
                                | "base"
                                | "normal"
                                | "normal_map"
                                | "normals"
                                | "specular"
                                | "spec"
                                | "specular_map"
                                | "emissive"
                                | "emission"
                                | "emissive_map"
                                | "environment"
                                | "environment_map"
                                | "reflection"
                                | "generic"
                        ) {
                            continue;
                        }
                        let Some(texture_address) = resolve_material_texture_address(binding)?
                        else {
                            continue;
                        };
                        if let Some(texture) = self
                            .asset_streamer
                            .get::<newviso_textures::TextureResource>(&texture_address)
                        {
                            auxiliary_textures.insert(role, texture);
                        }
                    }

                    Ok(newviso_scene::SceneResolvedMaterial {
                        material: material.clone(),
                        base_color,
                        normal: resolve_role("normal")?,
                        // Native multilayer: source roughness, MLMask and microblend
                        // occupy three renderer samplers with explicit semantic roles.
                        // Other materials keep their legacy PBR mapping untouched.
                        specular: if native_multilayer {
                            resolve_role("roughness")?
                        } else { resolve_role("specular")? },
                        emissive: if native_multilayer {
                            resolve_role("layer_mask")?
                        } else { resolve_role("emissive")? },
                        environment: if native_multilayer {
                            resolve_role("microblend")?
                        } else { resolve_role("environment")? },
                        auxiliary_textures,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let prepared = self
                .scene_prepared_models
                .get(&model.id.0)
                .and_then(|result| result.as_ref().ok())
                .cloned();
            if self.scene.install_entity_model_prepared(
                stable_id,
                model,
                &resolved_scene_materials,
                prepared.as_deref(),
            )? {
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
            let _ = self.bind_vehicle_model_presentation(stable_id, model)?;
            let _ = self.apply_scene_animation_binding(stable_id)?;
        }

        if let Some(collision) = collision.as_deref() {
            if self.physics.is_some() {
                let key = (stable_id, collision.id.0);
                let prepared = self
                    .scene_prepared_collisions
                    .remove(&key)
                    .ok_or_else(|| {
                        format!(
                            "scene collision entity={} asset='{}' reached commit without background preparation",
                            stable_id,
                            address.canonical()
                        )
                    })??;
                let (changed, nav_source) = self
                    .physics
                    .as_mut()
                    .expect("physics checked")
                    .install_prepared_streamed_collision(prepared)?;
                if let Some(nav_source) = nav_source {
                    let walkable = self
                        .physical_characters
                        .upsert_navigation_tile(nav_source)?;
                    host::debug(
                        "newviso.navigation",
                        format!(
                            "navigation tile materialized entity={} walkable_polygons={}",
                            stable_id, walkable
                        ),
                    );
                }
                if changed {
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
}
