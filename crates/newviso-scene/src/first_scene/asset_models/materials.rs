//! Resolved material interpretation and renderer uniform contract.
use super::*;

fn material_float(material: &MaterialResource, name: &str, default: f32) -> f32 {
    material
        .params
        .iter()
        .find(|parameter| parameter.name.eq_ignore_ascii_case(name))
        .and_then(|parameter| match &parameter.value {
            MaterialParamValue::Float(value) if value.is_finite() => Some(*value),
            _ => None,
        })
        .unwrap_or(default)
}

fn material_u32(material: &MaterialResource, name: &str, default: u32) -> u32 {
    material
        .params
        .iter()
        .find(|parameter| parameter.name.eq_ignore_ascii_case(name))
        .and_then(|parameter| match parameter.value {
            MaterialParamValue::Int(value) if value >= 0 => Some(value as u32),
            _ => None,
        })
        .unwrap_or(default)
}

impl CpuAssetMaterial {
    /// Exact GPU-sampled dependency set for both upload collection and
    /// readiness gating. A single source avoids allocating and maintaining
    /// two divergent lists every streaming frame.
    pub(in crate::first_scene) fn sampled_textures(
        &self,
    ) -> impl Iterator<Item = &Arc<TextureResource>> {
        [
            self.textures.base_color.as_ref(),
            self.textures.normal.as_ref(),
            self.textures.specular.as_ref(),
            self.textures.emissive.as_ref(),
            self.textures.environment.as_ref(),
        ]
        .into_iter()
        .flatten()
        .chain(self.textures.auxiliary_textures.values())
    }

    pub(in crate::first_scene) fn from_resolved(resolved: &SceneResolvedMaterial) -> Self {
        let material = resolved.material.as_ref();
        let render_bucket = material_u32(material, "render_bucket", 0);
        let opacity = material_float(material, "opacity", 1.0).clamp(0.0, 1.0);
        let alpha_mode = match material.blend {
            BlendMode::Alpha | BlendMode::Additive => AssetAlphaMode::Blend,
            BlendMode::Masked => AssetAlphaMode::Cutout,
            BlendMode::Opaque if opacity < 0.999 || render_bucket == 1 || render_bucket >= 3 => {
                AssetAlphaMode::Blend
            }
            BlendMode::Opaque if render_bucket == 2 => AssetAlphaMode::Cutout,
            BlendMode::Opaque => AssetAlphaMode::Opaque,
        };

        let newviso_multilayer = material.shader.eq_ignore_ascii_case("newviso.multilayer");
        let newviso_multilayer_params = if newviso_multilayer {
            [
                material_float(material,"newviso_ml_mat_tile",1.0).clamp(0.01,1024.0),
                material_float(material,"newviso_ml_microblend_tile",1.0).clamp(0.01,1024.0),
                material_float(material,"newviso_ml_microblend_contrast",0.0).clamp(0.0,1.0),
                material_float(material,"newviso_ml_layer_opacity",1.0).clamp(0.0,1.0),
            ]
        } else {
            [1.0,1.0,0.0,1.0]
        };
        let mut textures = resolved.clone();
        let metallic_roughness = material.params.iter().any(|p| {
            p.name.eq_ignore_ascii_case("metallic_factor")
                || p.name.eq_ignore_ascii_case("roughness_factor")
        });
        if metallic_roughness {
            textures.specular = textures
                .auxiliary_textures
                .get("metallic_roughness")
                .cloned();
        }
        let emissive_multiplier = material_float(material, "emissive_multiplier", 0.0).max(0.0);
        if emissive_multiplier > 0.0 && textures.emissive.is_none() {
            textures.emissive = textures.base_color.clone();
        }
        let authored_environment = material_float(material, "environment_reflection", 0.0).max(0.0);
        let environment_reflection =
            if newviso_multilayer {
                // Binding four is a source microblend, never a reflection map.
                0.0
            } else if textures.environment.is_some() && authored_environment <= 0.0 {
                1.0
            } else {
                authored_environment
            };

        Self {
            name: material.name.clone(),
            normal_strength: material_float(material, "normal_strength", 1.0).max(0.0),
            specular_intensity: material_float(material, "specular_intensity", 0.0).max(0.0),
            specular_falloff: material_float(material, "specular_falloff", 32.0).clamp(1.0, 512.0),
            specular_fresnel: material_float(material, "specular_fresnel", 0.0).clamp(0.0, 1.0),
            emissive_multiplier,
            environment_reflection,
            alpha_mode,
            alpha_cutoff: material
                .alpha_cutoff
                .unwrap_or_else(|| material_float(material, "alpha_cutoff", 0.5))
                .clamp(0.0, 1.0),
            opacity,
            render_bucket,
            two_sided: material.two_sided,
            shader_name: Some(material.shader.clone()),
            use_vertex_color: material.params.iter().any(|parameter| {
                parameter.name.eq_ignore_ascii_case("use_vertex_color")
                    && matches!(parameter.value, MaterialParamValue::Bool(true))
            }),
            metallic_roughness,
            metallic_factor: material_float(material, "metallic_factor", 1.0).clamp(0.0, 1.0),
            roughness_factor: material_float(material, "roughness_factor", 1.0).clamp(0.0, 1.0),
            newviso_multilayer,
            newviso_ml_layer_index: material_u32(material, "newviso_ml_layer_index", 0),
            newviso_ml_uv_channels: [
                material_u32(material, "newviso_ml_color_uv", 1).min(1),
                material_u32(material, "newviso_ml_normal_uv", 1).min(1),
                material_u32(material, "newviso_ml_roughness_uv", 1).min(1),
                material_u32(material, "newviso_ml_mask_uv", 0).min(1),
                material_u32(material, "newviso_ml_microblend_uv", 0).min(1),
            ],
            newviso_ml_micro_normal_strength: material_float(
                material, "newviso_ml_micro_normal_strength", 0.0
            ).clamp(-8.0, 8.0),
            newviso_ml_rough_levels: [
                material_float(material, "newviso_ml_rough_in_white", 1.0).clamp(-8.0,8.0),
                material_float(material, "newviso_ml_rough_in_black", 0.0).clamp(-8.0,8.0),
                material_float(material, "newviso_ml_rough_out_white", 1.0).clamp(-8.0,8.0),
                material_float(material, "newviso_ml_rough_out_black", 0.0).clamp(-8.0,8.0),
            ],
            newviso_ml_metalness_mode: material_float(material, "newviso_ml_metalness_mode", 0.0).clamp(-1.0,1.0),
            newviso_ml_uv_offsets: [
                material_float(material, "newviso_ml_offset_uv0", 0.0),
                material_float(material, "newviso_ml_offset_uv1", 0.0),
                material_float(material, "newviso_ml_offset_micro0", 0.0),
                material_float(material, "newviso_ml_offset_micro1", 0.0),
            ],
            newviso_multilayer_params,
            textures,
        }
    }

    pub(in crate::first_scene) fn uniform_data(&self) -> [f32; 20] {
        let mut flags = 0u32;
        if self.use_vertex_color {
            flags |= 64;
        }
        if self.newviso_multilayer {
            flags |= 32768; // Native source-mask multilayer shader.
            // Exact f32 integer bitfield (< 2^24). Each texture independently
            // chooses source UV0 or UV1; this is not a vehicle-specific rule.
            for (index, channel) in self.newviso_ml_uv_channels.iter().enumerate() {
                if *channel == 1 { flags |= 1 << (16 + index); }
            }
        }
        if self.metallic_roughness {
            flags |= 256;
        }
        if self.textures.normal.is_some() {
            flags |= 1;
        }
        if self.textures.specular.is_some() {
            flags |= 2;
        }
        if self.emissive_multiplier > 0.0 && self.textures.emissive.is_some() {
            flags |= 4;
        }
        if self.environment_reflection > 0.0 {
            flags |= 32;
        }
        if self.textures.environment.is_some() {
            flags |= 128;
        }
        if self.alpha_mode == AssetAlphaMode::Cutout {
            flags |= 8;
        }
        if self.alpha_mode == AssetAlphaMode::Blend {
            flags |= 16;
        }
        [
            self.normal_strength,
            if self.newviso_multilayer { self.newviso_ml_metalness_mode } else { self.specular_intensity },
            if self.newviso_multilayer { self.newviso_ml_uv_offsets[0] } else { self.specular_falloff },
            if self.newviso_multilayer { self.newviso_ml_uv_offsets[1] } else { self.specular_fresnel },
            if self.newviso_multilayer { self.newviso_ml_uv_offsets[2] } else { self.emissive_multiplier },
            if self.newviso_multilayer { self.newviso_ml_uv_offsets[3] } else { self.alpha_cutoff },
            flags as f32,
            self.opacity,
            if self.newviso_multilayer { self.newviso_multilayer_params[0] } else { self.environment_reflection },
            if self.newviso_multilayer { self.newviso_multilayer_params[1] } else { self.metallic_factor },
            if self.newviso_multilayer { self.newviso_multilayer_params[2] } else { self.roughness_factor },
            if self.newviso_multilayer { self.newviso_multilayer_params[3] } else { 0.0 },
            self.newviso_ml_rough_levels[0],
            self.newviso_ml_rough_levels[1],
            self.newviso_ml_rough_levels[2],
            self.newviso_ml_rough_levels[3],
            self.newviso_ml_micro_normal_strength,
            0.0, 0.0, 0.0,
        ]
    }
}


#[cfg(test)]
mod newviso_multilayer_tests {
    use super::*;
    use newviso_resource_runtime::AssetId;

    #[test]
    fn source_multilayer_is_exposed_under_newviso_shader_namespace() {
        let mat = MaterialResource {
            id: AssetId(801),
            name: "cp77.streets.highway.test".into(),
            shader: "newviso.multilayer".into(),
            surface_domain: "surface".into(),
            shading_model: "pbr".into(),
            blend: BlendMode::Alpha,
            two_sided: false,
            alpha_cutoff: None,
            textures: vec![],
            params: vec![
                ("newviso_ml_mat_tile", MaterialParamValue::Float(8.0)),
                ("newviso_ml_microblend_tile", MaterialParamValue::Float(2.0)),
                ("newviso_ml_microblend_contrast", MaterialParamValue::Float(0.4)),
                ("newviso_ml_layer_opacity", MaterialParamValue::Float(0.75)),
            ].into_iter().map(|(name,value)| {
                newviso_materials::MaterialParameter { name:name.into(), value }
            }).collect(),
        };
        let resolved = SceneResolvedMaterial {
            material: Arc::new(mat),
            base_color: None,
            normal: None,
            specular: None,
            emissive: None,
            environment: None,
            auxiliary_textures: Default::default(),
        };
        let cpu = CpuAssetMaterial::from_resolved(&resolved);
        assert!(cpu.newviso_multilayer);
        assert_eq!(cpu.alpha_mode, AssetAlphaMode::Blend);
        let u = cpu.uniform_data();
        assert_eq!((u[6] as u32) & 32768, 32768);
        // Default independent UV routing: source material maps sample UV1;
        // source MLMask and microblend still sample UV0.
        let uv_bits = (u[6] as u32) >> 16 & 0b11111;
        assert_eq!(uv_bits, 0b00111);
        assert_eq!(&u[8..12], &[8.0,2.0,0.4,0.75]);
    }

    #[test]
    fn each_multilayer_texture_can_choose_uv0_or_uv1_independently() {
        let mat = MaterialResource {
            id: AssetId(900),
            name: "generic.two_uv_surface".into(),
            shader: "newviso.multilayer".into(),
            surface_domain: "surface".into(),
            shading_model: "pbr".into(),
            blend: BlendMode::Alpha,
            two_sided: false,
            alpha_cutoff: None,
            textures: vec![],
            params: [
                ("newviso_ml_color_uv", 0),
                ("newviso_ml_normal_uv", 1),
                ("newviso_ml_roughness_uv", 0),
                ("newviso_ml_mask_uv", 1),
                ("newviso_ml_microblend_uv", 1),
            ].into_iter().map(|(name, channel)| {
                newviso_materials::MaterialParameter {
                    name: name.into(),
                    value: MaterialParamValue::Int(channel),
                }
            }).collect(),
        };
        let cpu = CpuAssetMaterial::from_resolved(&SceneResolvedMaterial {
            material: Arc::new(mat),
            base_color: None,
            normal: None,
            specular: None,
            emissive: None,
            environment: None,
            auxiliary_textures: Default::default(),
        });
        assert_eq!(cpu.newviso_ml_uv_channels, [0,1,0,1,1]);
        let flags = cpu.uniform_data()[6] as u32;
        assert_eq!((flags >> 16) & 0b11111, 0b11010);
        assert_eq!(flags as f32 as u32, flags);
    }

    #[test]
    fn source_roughness_and_signed_microdetail_survive_material_ubo() {
        let mut mat = MaterialResource {
            id: AssetId(920),
            name: "generic.source.surfaces".into(),
            shader: "newviso.multilayer".into(),
            surface_domain: "surface".into(),
            shading_model: "pbr".into(),
            blend: BlendMode::Alpha,
            two_sided: false,
            alpha_cutoff: None,
            textures: vec![],
            params: vec![],
        };
        for (name, value) in [
            ("newviso_ml_rough_in_white", 1.821),
            ("newviso_ml_rough_in_black", -0.4286),
            ("newviso_ml_rough_out_white", 0.6275),
            ("newviso_ml_rough_out_black", 0.0),
            ("newviso_ml_micro_normal_strength", -4.11),
        ] {
            mat.params.push(newviso_materials::MaterialParameter {
                name: name.into(),
                value: MaterialParamValue::Float(value),
            });
        }
        let material = CpuAssetMaterial::from_resolved(&SceneResolvedMaterial {
            material: Arc::new(mat), base_color: None, normal: None,
            specular: None, emissive: None, environment: None,
            auxiliary_textures: Default::default(),
        });
        let uniform = material.uniform_data();
        assert_eq!(uniform.len(), 20);
        for (actual, expected) in uniform[12..16].iter()
            .zip([1.821, -0.4286, 0.6275, 0.0])
        {
            assert!((actual - expected).abs() < 0.00001);
        }
        assert!((uniform[16] - (-4.11)).abs() < 0.00001);
    }

    #[test]
    fn ordinary_material_keeps_its_legacy_ubo() {
        let mat = MaterialResource {
            id: AssetId(802),
            name: "default".into(),
            shader: "pbr.default".into(),
            surface_domain: "surface".into(),
            shading_model: "pbr".into(),
            blend: BlendMode::Opaque,
            two_sided: false,
            alpha_cutoff: None,
            textures: vec![],
            params: vec![],
        };
        let resolved = SceneResolvedMaterial {
            material: Arc::new(mat),
            base_color: None,
            normal: None,
            specular: None,
            emissive: None,
            environment: None,
            auxiliary_textures: Default::default(),
        };
        let cpu = CpuAssetMaterial::from_resolved(&resolved);
        let u = cpu.uniform_data();
        assert!(!cpu.newviso_multilayer);
        assert_eq!((u[6] as u32) & 32768, 0);
        assert_eq!((u[6] as u32) >> 16, 0);
        assert_eq!(u[8], 0.0);
        assert_eq!(u[9], 1.0);
        assert_eq!(u[10], 1.0);
        assert_eq!(u[11], 0.0);
    }
}
