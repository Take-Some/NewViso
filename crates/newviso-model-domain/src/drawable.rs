use std::ops::Range;

use newviso_asset_interfaces::list_file::{
    AssetDependencyRecord, ListFileEntryRecord, ListFileMetadataNamespace,
};
use serde::{Deserialize, Serialize};

use crate::{
    DRAWABLE_DICTIONARY_ASSET_KIND, DRAWABLE_DICTIONARY_CONTAINER,
    DRAWABLE_DICTIONARY_MANIFEST_SCHEMA,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrawableDictionaryBodyV1 {
    pub schema: String,
    pub entries: Vec<DrawableEntryV1>,
    pub common_entries: Vec<ListFileEntryRecord>,
    pub dependencies: Vec<AssetDependencyRecord>,
    pub metadata: Vec<ListFileMetadataNamespace>,
    pub mesh_parts: Vec<MeshPartRecordV1>,
    pub vertex_streams: Vec<VertexStreamDescriptorV1>,
    pub index_buffers: Vec<IndexBufferDescriptorV1>,
    pub material_slots: Vec<MaterialSlotRecordV1>,
    pub lods: Vec<LodRecordV1>,
    pub refs: Vec<String>,
    pub properties_refs: Vec<String>,
    pub bounds: Vec<BoundsRecordV1>,
}
impl Default for DrawableDictionaryBodyV1 {
    fn default() -> Self {
        Self {
            schema: "newengine.ydd.drawable_dictionary.v1".to_owned(),
            entries: Vec::new(),
            common_entries: Vec::new(),
            dependencies: Vec::new(),
            metadata: Vec::new(),
            mesh_parts: Vec::new(),
            vertex_streams: Vec::new(),
            index_buffers: Vec::new(),
            material_slots: Vec::new(),
            lods: Vec::new(),
            refs: Vec::new(),
            properties_refs: Vec::new(),
            bounds: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrawableEntryV1 {
    pub name_hash: u64,
    pub name_offset: u32,
    pub mesh_part_range: Range<u32>,
    pub material_slot_range: Range<u32>,
    pub lod_range: Range<u32>,
    pub skeleton_ref_range: Range<u32>,
    pub collision_ref_range: Range<u32>,
    pub dependency_ref_range: Range<u32>,
    pub bounds_index: u32,
    pub flags: u32,
    pub metadata_range: Range<u32>,
}
impl Default for DrawableEntryV1 {
    fn default() -> Self {
        Self {
            name_hash: 0,
            name_offset: 0,
            mesh_part_range: 0..0,
            material_slot_range: 0..0,
            lod_range: 0..0,
            skeleton_ref_range: 0..0,
            collision_ref_range: 0..0,
            dependency_ref_range: 0..0,
            bounds_index: 0,
            flags: 0,
            metadata_range: 0..0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeshPartRecordV1 {
    pub vertex_layout_id: String,
    pub vertex_stream_range: Range<u32>,
    pub index_buffer_index: u32,
    pub material_slot_index: u32,
    pub primitive_topology: String,
    pub bounds_index: u32,
}
impl Default for MeshPartRecordV1 {
    fn default() -> Self {
        Self {
            vertex_layout_id: String::new(),
            vertex_stream_range: 0..0,
            index_buffer_index: 0,
            material_slot_index: 0,
            primitive_topology: "triangles".to_owned(),
            bounds_index: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct VertexStreamDescriptorV1 {
    pub semantic: String,
    pub format: String,
    pub stride: u32,
    pub payload_offset: u64,
    pub payload_len: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexBufferDescriptorV1 {
    pub index_format: String,
    pub payload_offset: u64,
    pub payload_len: u64,
}
impl Default for IndexBufferDescriptorV1 {
    fn default() -> Self {
        Self {
            index_format: "u32".to_owned(),
            payload_offset: 0,
            payload_len: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialSlotDecl {
    pub slot_name: String,
    pub required: bool,
}
impl Default for MaterialSlotDecl {
    fn default() -> Self {
        Self {
            slot_name: String::new(),
            required: true,
        }
    }
}

/// Material slot declaration stored by `.ydd` model dictionaries.
///
/// This declares an abstract material socket on geometry. Concrete material refs
/// are descriptor-owned and live in `.ytyp` MaterialBindingRef values.
pub type MaterialSlotRecordV1 = MaterialSlotDecl;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialBindingRef {
    pub slot: String,
    pub material_ref: String,
    pub required: bool,
}
impl Default for MaterialBindingRef {
    fn default() -> Self {
        Self {
            slot: String::new(),
            material_ref: String::new(),
            required: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LodRecordV1 {
    pub name: String,
    pub drawable_ref: String,
    pub max_distance: f32,
    pub required: bool,
}
impl Default for LodRecordV1 {
    fn default() -> Self {
        Self {
            name: String::new(),
            drawable_ref: String::new(),
            max_distance: 0.0,
            required: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoundsRecordV1 {
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
    pub sphere_center: [f32; 3],
    pub sphere_radius: f32,
}
impl Default for BoundsRecordV1 {
    fn default() -> Self {
        Self {
            aabb_min: [0.0; 3],
            aabb_max: [0.0; 3],
            sphere_center: [0.0; 3],
            sphere_radius: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DrawableDictionaryRequest {
    pub source: String,
    pub selector: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrawableDictionaryEntry {
    pub name: String,
    /// Optional explicit descriptor for this drawable dictionary cell.
    ///
    /// `.ydd` is a model dictionary; each addressable cell may point to a `.ytyp`
    /// Y-Type Properties document that explains material bindings, render role,
    /// collision policy, LOD and other domain usage. Loaders must follow this
    /// explicit reference instead of searching for files near the model asset.
    pub properties_ref: Option<String>,
    pub name_hash: u64,
    pub mesh_count: u32,
    /// Abstract material slot declarations owned by the geometry cell.
    ///
    /// These are sockets such as `leaf`, `bark`, `sky` or `body_paint`.
    /// Concrete `.nemat` references are descriptor-owned and live in `.ytyp`
    /// `MaterialBindingRef` values.
    pub material_slots: Vec<MaterialSlotDecl>,
    pub skeleton_refs: Vec<String>,
    pub lods: Vec<String>,
    pub collision_refs: Vec<String>,
    pub dependency_refs: Vec<String>,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}
impl Default for DrawableDictionaryEntry {
    fn default() -> Self {
        Self {
            name: String::new(),
            properties_ref: None,
            name_hash: 0,
            mesh_count: 0,
            material_slots: Vec::new(),
            skeleton_refs: Vec::new(),
            lods: Vec::new(),
            collision_refs: Vec::new(),
            dependency_refs: Vec::new(),
            bounds_min: [0.0; 3],
            bounds_max: [0.0; 3],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrawableDictionaryManifest {
    pub schema: String,
    pub source: String,
    pub asset_kind: String,
    pub container: String,
    pub texture_dictionary: Option<String>,
    pub entries: Vec<DrawableDictionaryEntry>,
    pub warnings: Vec<String>,
}
impl Default for DrawableDictionaryManifest {
    fn default() -> Self {
        Self {
            schema: DRAWABLE_DICTIONARY_MANIFEST_SCHEMA.to_owned(),
            source: String::new(),
            asset_kind: DRAWABLE_DICTIONARY_ASSET_KIND.to_owned(),
            container: DRAWABLE_DICTIONARY_CONTAINER.to_owned(),
            texture_dictionary: None,
            entries: Vec::new(),
            warnings: Vec::new(),
        }
    }
}
