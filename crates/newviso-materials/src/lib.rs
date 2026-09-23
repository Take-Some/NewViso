use newviso_resource_runtime::{AssetAddress, AssetDomain, AssetId, AssetRef, AssetResource};
use newviso_textures::TextureResource;
use std::{any::Any, sync::Arc};

pub const MATERIALS_DOMAIN: AssetDomain = AssetDomain::new("engine.materials");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlendMode {
    Opaque,
    Masked,
    Alpha,
    Additive,
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::Opaque
    }
}

#[derive(Clone, Debug)]
pub struct MaterialTextureBinding {
    pub slot: String,
    pub texture: AssetRef<TextureResource>,
    pub required: bool,
}

#[derive(Clone, Debug)]
pub enum MaterialParamValue {
    Float(f32),
    Float2([f32; 2]),
    Float3([f32; 3]),
    Float4([f32; 4]),
    Int(i32),
    Bool(bool),
    Enum(String),
    TextureRef(AssetRef<TextureResource>),
}

#[derive(Clone, Debug)]
pub struct MaterialParameter {
    pub name: String,
    pub value: MaterialParamValue,
}

#[derive(Clone, Debug)]
pub struct MaterialResource {
    pub id: AssetId,
    pub name: String,
    pub shader: String,
    pub surface_domain: String,
    pub shading_model: String,
    pub blend: BlendMode,
    pub two_sided: bool,
    pub alpha_cutoff: Option<f32>,
    pub textures: Vec<MaterialTextureBinding>,
    pub params: Vec<MaterialParameter>,
}

impl AssetResource for MaterialResource {
    fn asset_id(&self) -> AssetId {
        self.id
    }

    fn domain(&self) -> AssetDomain {
        MATERIALS_DOMAIN
    }

    fn dependencies(&self) -> Vec<AssetAddress> {
        let mut out = self
            .textures
            .iter()
            .map(|binding| binding.texture.address().clone())
            .collect::<Vec<_>>();

        for parameter in &self.params {
            if let MaterialParamValue::TextureRef(texture) = &parameter.value {
                out.push(texture.address().clone());
            }
        }

        out
    }

    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}
