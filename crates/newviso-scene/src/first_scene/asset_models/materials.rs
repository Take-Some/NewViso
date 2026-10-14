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

        let mut textures = resolved.clone();
        let metallic_roughness = material.params.iter().any(|p|
            p.name.eq_ignore_ascii_case("metallic_factor") || p.name.eq_ignore_ascii_case("roughness_factor"));
        if metallic_roughness {
            textures.specular = textures.auxiliary_textures.get("metallic_roughness").cloned();
        }
        let emissive_multiplier = material_float(material, "emissive_multiplier", 0.0).max(0.0);
        if emissive_multiplier > 0.0 && textures.emissive.is_none() {
            textures.emissive = textures.base_color.clone();
        }
        let authored_environment = material_float(material, "environment_reflection", 0.0).max(0.0);
        let environment_reflection =
            if textures.environment.is_some() && authored_environment <= 0.0 {
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
            textures,
        }
    }

    pub(in crate::first_scene) fn uniform_data(&self) -> [f32; 12] {
        let mut flags = 0u32;
        if self.use_vertex_color {
            flags |= 64;
        }
        if self.metallic_roughness { flags |= 256; }
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
            self.specular_intensity,
            self.specular_falloff,
            self.specular_fresnel,
            self.emissive_multiplier,
            self.alpha_cutoff,
            flags as f32,
            self.opacity,
            self.environment_reflection,
            self.metallic_factor,
            self.roughness_factor,
            0.0,
        ]
    }
}