use super::*;

impl Scene3dRuntime {
    pub(super) fn sync_static_asset_material_gpu(&mut self) -> Result<(), String> {
        self.sync_static_asset_material_gpu_for_working_set(false)
    }

    pub(super) fn sync_startup_asset_material_gpu(&mut self) -> Result<(), String> {
        self.sync_static_asset_material_gpu_for_working_set(true)
    }

    fn sync_static_asset_material_gpu_for_working_set(
        &mut self,
        startup_streaming_set: bool,
    ) -> Result<(), String> {
        let sync_started = std::time::Instant::now();
        let mut textures_created = 0usize;
        let Some(gpu) = self.gpu else {
            return Err("3D scene GPU resources are not initialized".to_owned());
        };

        // Runtime frames use current visibility. Startup deliberately expands
        // the material working set to the whole initial streaming radius around
        // the focus, including assets behind the starting camera.
        let material_entities = if startup_streaming_set {
            &self.frame_plan.streaming_entities
        } else {
            &self.frame_plan.visible_entities
        };
        let mut working_model_ids = std::collections::BTreeSet::<u64>::new();
        let mut working_model_representatives = Vec::<u64>::new();
        for id in material_entities {
            let Some(mesh) = self.asset_meshes.get(&id.0) else {
                continue;
            };
            if working_model_ids.insert(mesh.model_id.0) {
                working_model_representatives.push(id.0);
            }
        }

        let mut pending_textures = BTreeMap::<u64, std::sync::Arc<TextureResource>>::new();
        for stable_id in &working_model_representatives {
            let Some(mesh) = self.asset_meshes.get(stable_id) else {
                continue;
            };
            for material in mesh.materials.iter() {
                for texture in [
                    material.textures.base_color.as_ref(),
                    material.textures.normal.as_ref(),
                    material.textures.specular.as_ref(),
                    material.textures.emissive.as_ref(),
                    material.textures.environment.as_ref(),
                ]
                .into_iter()
                .flatten()
                .chain(material.textures.auxiliary_textures.values())
                {
                    if !self.asset_gpu_textures.contains_key(&texture.id.0) {
                        pending_textures
                            .entry(texture.id.0)
                            .or_insert_with(|| texture.clone());
                    }
                }
            }
        }

        let render = RenderClient::new();

        // Do not execute upload work here. Vulkan BeginFrame owns the only
        // frame-budgeted upload pump, so a streamed city cell cannot consume a
        // second transfer budget inside scene material synchronization.
        let pending_ids = self
            .asset_gpu_pending_textures
            .iter()
            .map(|(asset_id, texture_id)| (*asset_id, *texture_id))
            .collect::<Vec<_>>();
        for (asset_id, texture_id) in pending_ids {
            match render.texture_residency(texture_id)? {
                residency if residency.state == TextureResidencyState::Ready => {
                    self.asset_gpu_pending_textures.remove(&asset_id);
                    self.asset_gpu_textures.insert(asset_id, texture_id);
                }
                residency if residency.state == TextureResidencyState::Failed => {
                    self.asset_gpu_pending_textures.remove(&asset_id);
                    render.destroy_texture(texture_id);
                    host_runtime::warn(
                        "newviso.scene",
                        format!(
                            "streamed texture upload failed asset={} texture={} message={}",
                            asset_id,
                            texture_id,
                            residency.message.as_deref().unwrap_or("<none>")
                        ),
                    );
                }
                _ => {}
            }
        }

        let texture_candidates = pending_textures
            .into_iter()
            .filter(|(asset_id, _)| {
                !self.asset_gpu_pending_textures.contains_key(asset_id)
                    && !self.asset_gpu_textures.contains_key(asset_id)
            })
            .take(if startup_streaming_set {
                64
            } else {
                MAX_STREAMED_TEXTURE_UPLOADS_PER_FRAME
            })
            .collect::<Vec<_>>();
        for (asset_id, texture) in texture_candidates {
            // World-streamed materials do not need 4K/8K top mips at the
            // distances where they become resident. Rebase an existing mip as
            // level 0 instead of doing an expensive CPU resample.
            let first_mip_index = texture
                .mips
                .iter()
                .position(|mip| {
                    mip.width <= MAX_STREAMED_TEXTURE_DIMENSION
                        && mip.height <= MAX_STREAMED_TEXTURE_DIMENSION
                })
                .unwrap_or(0);
            let selected_mips = &texture.mips[first_mip_index..];
            let (gpu_width, gpu_height) = selected_mips
                .first()
                .map(|mip| (mip.width, mip.height))
                .unwrap_or((texture.width, texture.height));

            let mut data = Vec::new();
            let mut mips = Vec::with_capacity(selected_mips.len());
            for (rebased_level, mip) in selected_mips.iter().enumerate() {
                let offset = data.len() as u64;
                data.extend_from_slice(&mip.data);
                mips.push(TextureMipUpload {
                    level: u32::try_from(rebased_level)
                        .map_err(|_| "streamed texture mip level exceeds u32".to_owned())?,
                    width: mip.width,
                    height: mip.height,
                    offset,
                    byte_len: mip.data.len() as u64,
                });
            }
            let texture_label = format!("newviso.scene.material.texture.{}", texture.name);
            let gpu_texture = render.create_texture_deferred(
                &texture_label,
                gpu_width,
                gpu_height,
                texture_format_wire_name(texture.format),
                &mips,
                &data,
            )?;
            self.asset_gpu_pending_textures
                .insert(asset_id, gpu_texture);
            textures_created = textures_created.saturating_add(1);
            host_runtime::debug(
                "newviso.scene",
                format!(
                    "material texture GPU queued name='{}' source={}x{} gpu={}x{} mips={}/{} bytes={} format={}",
                    texture.name,
                    texture.width,
                    texture.height,
                    gpu_width,
                    gpu_height,
                    mips.len(),
                    texture.mips.len(),
                    data.len(),
                    texture_format_wire_name(texture.format),
                ),
            );
        }

        let mut pending_materials = BTreeMap::<(u64, u32), CpuAssetMaterial>::new();
        for stable_id in &working_model_representatives {
            let Some(mesh) = self.asset_meshes.get(stable_id) else {
                continue;
            };
            for (slot, material) in mesh.materials.iter().enumerate() {
                let slot = u32::try_from(slot)
                    .map_err(|_| "material slot index exceeds u32".to_owned())?;
                let key = (mesh.model_id.0, slot);
                if !self.asset_gpu_materials.contains_key(&key) {
                    pending_materials
                        .entry(key)
                        .or_insert_with(|| material.clone());
                }
            }
        }

        let mut materials_created = 0usize;
        let material_creation_limit = if startup_streaming_set {
            256
        } else {
            MAX_STREAMED_MATERIAL_CREATIONS_PER_FRAME
        };
        for (key, material) in pending_materials {
            if materials_created >= material_creation_limit {
                break;
            }

            // Never permanently bind a fallback for a texture that is known to
            // exist but merely has not crossed the GPU upload budget yet.
            let required_textures_ready = [
                material.textures.base_color.as_ref(),
                material.textures.normal.as_ref(),
                material.textures.specular.as_ref(),
                material.textures.emissive.as_ref(),
                material.textures.environment.as_ref(),
            ]
            .into_iter()
            .flatten()
            .chain(material.textures.auxiliary_textures.values())
            .all(|texture| self.asset_gpu_textures.contains_key(&texture.id.0));
            if !required_textures_ready {
                continue;
            }

            let gpu_texture = |texture: Option<&std::sync::Arc<TextureResource>>, fallback: u32| {
                texture
                    .and_then(|texture| self.asset_gpu_textures.get(&texture.id.0).copied())
                    .unwrap_or(fallback)
            };
            let base_color = gpu_texture(
                material.textures.base_color.as_ref(),
                gpu.default_base_color_texture,
            );
            let normal = gpu_texture(
                material.textures.normal.as_ref(),
                gpu.default_normal_texture,
            );
            let specular = gpu_texture(
                material.textures.specular.as_ref(),
                gpu.default_specular_texture,
            );
            let emissive = gpu_texture(
                material.textures.emissive.as_ref(),
                gpu.default_emissive_texture,
            );
            let environment = gpu_texture(
                material.textures.environment.as_ref(),
                gpu.default_environment_texture,
            );

            let uniform_buffer = render.create_buffer(
                &format!("newviso.scene.material.params.{}.{}", key.0, key.1),
                (12 * std::mem::size_of::<f32>()) as u64,
                "Uniform",
                "CpuToGpu",
            )?;
            let params = material.uniform_data();
            render.write_buffer_f32(uniform_buffer, 0, &params)?;

            let bind_group = match render.create_bind_group6(
                &format!("newviso.scene.material.bind.{}.{}", key.0, key.1),
                gpu.material_bind_group_layout,
                [
                    Some(base_color),
                    Some(normal),
                    Some(specular),
                    Some(emissive),
                    Some(environment),
                    None,
                ],
                Some(gpu.material_sampler),
                Some((uniform_buffer, 0, (12 * std::mem::size_of::<f32>()) as u64)),
            ) {
                Ok(group) => group,
                Err(error) => {
                    render.destroy_buffer(uniform_buffer);
                    return Err(error);
                }
            };
            self.asset_gpu_materials.insert(
                key,
                GpuAssetMaterial {
                    uniform_buffer,
                    bind_group,
                },
            );
            materials_created = materials_created.saturating_add(1);
            host_runtime::debug(
                "newviso.scene",
                format!(
                    "material GPU ready model={} slot={} name='{}' shader='{}' two_sided={} normal_strength={:.3} spec_intensity={:.3} spec_falloff={:.1} fresnel={:.3} emissive={:.3} env_reflect={:.3} env_texture={} opacity={:.3} alpha={:?} bucket={}",
                    key.0,
                    key.1,
                    material.name,
                    material.shader_name.as_deref().unwrap_or("<inline>"),
                    material.two_sided,
                    material.normal_strength,
                    material.specular_intensity,
                    material.specular_falloff,
                    material.specular_fresnel,
                    material.emissive_multiplier,
                    material.environment_reflection,
                    material.textures.environment.is_some(),
                    material.opacity,
                    material.alpha_mode,
                    material.render_bucket,
                ),
            );
        }

        let elapsed_ms = sync_started.elapsed().as_secs_f64() * 1000.0;
        if elapsed_ms >= 10.0 {
            host_runtime::info(
                "newviso.perf",
                format!(
                    "material_gpu_sync ms={:.2} working_set={} models={} textures_queued={} materials_created={} gpu_textures_ready={} gpu_textures_pending={} gpu_materials_total={} upload_scheduler=backend_begin_frame",
                    elapsed_ms,
                    if startup_streaming_set { "startup-streaming" } else { "visible" },
                    working_model_ids.len(),
                    textures_created,
                    materials_created,
                    self.asset_gpu_textures.len(),
                    self.asset_gpu_pending_textures.len(),
                    self.asset_gpu_materials.len()
                ),
            );
        }
        Ok(())
    }
}

fn texture_format_wire_name(format: TextureFormat) -> &'static str {
    match format {
        TextureFormat::Rgba8Unorm => "Rgba8Unorm",
        TextureFormat::Rgba8Srgb => "Rgba8Srgb",
        TextureFormat::Bc1RgbaUnorm => "Bc1RgbaUnorm",
        TextureFormat::Bc1RgbaSrgb => "Bc1RgbaSrgb",
        TextureFormat::Bc2RgbaUnorm => "Bc2RgbaUnorm",
        TextureFormat::Bc2RgbaSrgb => "Bc2RgbaSrgb",
        TextureFormat::Bc3RgbaUnorm => "Bc3RgbaUnorm",
        TextureFormat::Bc3RgbaSrgb => "Bc3RgbaSrgb",
        TextureFormat::Bc5RgUnorm => "Bc5RgUnorm",
        TextureFormat::Bc6hUf16 => "Bc6hUf16",
        TextureFormat::Bc6hSf16 => "Bc6hSf16",
        TextureFormat::Bc7RgbaUnorm => "Bc7RgbaUnorm",
        TextureFormat::Bc7RgbaSrgb => "Bc7RgbaSrgb",
    }
}
