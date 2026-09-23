use newviso_materials::MaterialResource;
use newviso_resource_runtime::{AssetAddress, AssetDomain, AssetId, AssetRef, AssetResource};
use std::{any::Any, sync::Arc};

pub const MODEL_DOMAIN: AssetDomain = AssetDomain::new("engine.model");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds3 {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Bounds3 {
    pub fn is_finite(self) -> bool {
        self.min
            .iter()
            .chain(self.max.iter())
            .all(|value| value.is_finite())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VertexSemantic {
    Position,
    Normal,
    Tangent,
    TexCoord(u8),
    Color(u8),
    JointIndices,
    JointWeights,
    Custom(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VertexFormat {
    Float32x2,
    Float32x3,
    Float32x4,
}

#[derive(Clone, Debug)]
pub struct VertexStream {
    pub semantic: VertexSemantic,
    pub format: VertexFormat,
    pub stride: u32,
    pub vertex_count: u32,
    pub data: Arc<[u8]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexFormat {
    U16,
    U32,
}

#[derive(Clone, Debug)]
pub struct IndexBuffer {
    pub format: IndexFormat,
    pub index_count: u32,
    pub data: Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub struct MeshPrimitive {
    pub first_index: u32,
    pub index_count: u32,
    pub base_vertex: i32,
    pub material_slot: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ModelMaterialSlot {
    pub name: String,
    pub material: AssetRef<MaterialResource>,
}

#[derive(Clone, Debug)]
pub struct MeshResource {
    pub name: String,
    pub vertex_streams: Vec<VertexStream>,
    pub index_buffer: IndexBuffer,
    pub primitives: Vec<MeshPrimitive>,
    pub bounds: Bounds3,
}

#[derive(Clone, Debug)]
pub struct ModelResource {
    pub id: AssetId,
    pub name: String,
    pub bounds: Bounds3,
    pub meshes: Vec<MeshResource>,
    pub material_slots: Vec<ModelMaterialSlot>,
}

impl AssetResource for ModelResource {
    fn asset_id(&self) -> AssetId {
        self.id
    }

    fn domain(&self) -> AssetDomain {
        MODEL_DOMAIN
    }

    fn dependencies(&self) -> Vec<AssetAddress> {
        self.material_slots
            .iter()
            .map(|slot| slot.material.address().clone())
            .collect()
    }

    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}
