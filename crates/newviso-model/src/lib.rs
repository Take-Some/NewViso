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
    JointIndicesExtra,
    JointWeightsExtra,
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
pub enum ModelMaterialBinding {
    /// Material data is resident inside the model asset and already normalized
    /// into the same semantic resource used by external material dictionaries.
    BuiltIn(Arc<MaterialResource>),
    /// Material data is resolved as a separately streamable material resource.
    External(AssetRef<MaterialResource>),
}

impl ModelMaterialBinding {
    pub fn external_ref(&self) -> Option<&AssetRef<MaterialResource>> {
        match self {
            Self::BuiltIn(_) => None,
            Self::External(reference) => Some(reference),
        }
    }

    pub fn built_in(&self) -> Option<&Arc<MaterialResource>> {
        match self {
            Self::BuiltIn(material) => Some(material),
            Self::External(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ModelMaterialSlot {
    pub name: String,
    /// Storage policy only. Consumers resolve both branches to MaterialResource.
    pub material: ModelMaterialBinding,
}

#[derive(Clone, Debug)]
pub struct MeshResource {
    pub name: String,
    pub vertex_streams: Vec<VertexStream>,
    pub index_buffer: IndexBuffer,
    pub primitives: Vec<MeshPrimitive>,
    pub bounds: Bounds3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelJoint {
    pub name: String,
    /// Authored skeleton tag used by animation clips. Unlike the dense runtime
    /// index this survives source formats whose bone tags are sparse.
    pub tag: u32,
    pub parent: Option<u16>,
    pub inverse_bind_matrix: [f32; 16],
    pub bind_translation: [f32; 3],
    pub bind_rotation: [f32; 4],
    pub bind_scale: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelSkeleton {
    pub name: String,
    pub joints: Vec<ModelJoint>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnimationInterpolation {
    Step,
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationVec3Key {
    pub time_seconds: f32,
    pub value: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationQuatKey {
    pub time_seconds: f32,
    pub value: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct JointAnimationTrack {
    pub joint: u16,
    pub translation_interpolation: AnimationInterpolation,
    pub rotation_interpolation: AnimationInterpolation,
    pub scale_interpolation: AnimationInterpolation,
    pub translations: Vec<AnimationVec3Key>,
    pub rotations: Vec<AnimationQuatKey>,
    pub scales: Vec<AnimationVec3Key>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelAnimationClip {
    pub name: String,
    pub duration_seconds: f32,
    pub looping: bool,
    pub tracks: Vec<JointAnimationTrack>,
}

#[derive(Clone, Debug)]
pub struct ModelResource {
    pub id: AssetId,
    pub name: String,
    pub bounds: Bounds3,
    pub meshes: Vec<MeshResource>,
    pub material_slots: Vec<ModelMaterialSlot>,
    /// Affine transform from authored skeleton/skin source space into model space.
    /// Identity for ordinary/static models.
    pub skin_source_to_model: [f32; 16],
    pub skeleton: Option<ModelSkeleton>,
    pub animations: Vec<ModelAnimationClip>,
}

impl AssetResource for ModelResource {
    fn asset_id(&self) -> AssetId {
        self.id
    }

    fn domain(&self) -> AssetDomain {
        MODEL_DOMAIN
    }

    fn dependencies(&self) -> Vec<AssetAddress> {
        let mut dependencies = Vec::new();
        for slot in &self.material_slots {
            match &slot.material {
                ModelMaterialBinding::BuiltIn(material) => {
                    dependencies.extend(material.dependencies());
                }
                ModelMaterialBinding::External(material) => {
                    dependencies.push(material.address().clone());
                }
            }
        }
        dependencies.sort_by(|a, b| a.canonical().cmp(&b.canonical()));
        dependencies.dedup_by(|a, b| a.canonical() == b.canonical());
        dependencies
    }

    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}
