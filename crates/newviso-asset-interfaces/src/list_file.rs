#![forbid(unsafe_op_in_unsafe_fn)]

use std::{collections::BTreeMap, ops::Range};

mod nef8;
pub use nef8::{
    decode_list_file_envelope, encode_list_file, DecodedListFileEnvelope, ListFileEncodeRequest,
};

/// Canonical manifest schema returned by dictionary/container codecs for the
/// data-driven asset world.
///
/// `listFiles` answers one generic question for every format: which addressable
/// entries does this logical file expose, and through which gateway/method should
/// those entries be resolved?
pub const ASSET_FILE_MANIFEST_SCHEMA: &str = "newengine.asset.list_files";
pub const ASSET_LIST_FILE_MANIFEST_OUTPUT: &str = "asset.list_file_manifest";
pub const ASSET_LIST_FILE_HEADER_OUTPUT: &str = "asset.list_file_header";
pub const ASSET_LIST_FILE_BODY_OUTPUT: &str = "asset.list_file_body";

/// Declarative route from an asset entry to the gateway that owns its semantic
/// interpretation. AssetManager still owns VFS bytes/codec dispatch; domain
/// gateways own meaning. This is semantic metadata, not a provider-service id.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetGatewayRoute {
    pub gateway: String,
    pub method: String,
    pub semantic_owner: String,
}

impl Default for AssetGatewayRoute {
    fn default() -> Self {
        Self {
            gateway: "engine.assets".to_owned(),
            method: "asset.decode_v1".to_owned(),
            semantic_owner: "asset".to_owned(),
        }
    }
}

impl AssetGatewayRoute {
    #[inline]
    pub fn new(
        gateway: impl Into<String>,
        method: impl Into<String>,
        semantic_owner: impl Into<String>,
    ) -> Self {
        Self {
            gateway: gateway.into(),
            method: method.into(),
            semantic_owner: semantic_owner.into(),
        }
    }
}

/// Edge from one `file@entry` to another. Resolvers use this, not ad-hoc
/// renderer or scene-bridge code, to build asset dependency graphs.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetEntryDependency {
    pub reference: String,
    /// Compatibility projection for older manifest consumers. New graph code should prefer `role`.
    pub kind: String,
    /// Semantic edge role, e.g. `material_slot/head` or `texture/base_color`.
    pub role: String,
    /// Owning semantic domain for the referenced entry, e.g. `engine.materials`.
    pub domain: String,
    pub required: bool,
}

impl Default for AssetEntryDependency {
    fn default() -> Self {
        Self {
            reference: String::new(),
            kind: String::new(),
            role: String::new(),
            domain: String::new(),
            required: true,
        }
    }
}

impl AssetEntryDependency {
    #[inline]
    pub fn required(reference: impl Into<String>, kind: impl Into<String>) -> Self {
        let kind = kind.into();
        Self {
            reference: reference.into(),
            role: kind.clone(),
            kind,
            domain: String::new(),
            required: true,
        }
    }

    #[inline]
    pub fn optional(reference: impl Into<String>, kind: impl Into<String>) -> Self {
        let kind = kind.into();
        Self {
            reference: reference.into(),
            role: kind.clone(),
            kind,
            domain: String::new(),
            required: false,
        }
    }

    #[inline]
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = domain.into();
        self
    }

    #[inline]
    pub fn role(mut self, role: impl Into<String>) -> Self {
        let role = role.into();
        self.kind = role.clone();
        self.role = role;
        self
    }
}

/// Common dependency record used inside NEF8/ListFile domain bodies.
///
/// This is the binary-domain counterpart of `AssetEntryDependency`: codecs and
/// domain handlers can preserve exact roles without forcing AssetManager to know
/// material/model/texture semantics.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetDependencyRecord {
    pub reference: String,
    pub role: String,
    pub required: bool,
    pub domain: String,
}

impl Default for AssetDependencyRecord {
    fn default() -> Self {
        Self {
            reference: String::new(),
            role: String::new(),
            required: true,
            domain: String::new(),
        }
    }
}

impl AssetDependencyRecord {
    #[inline]
    pub fn new(
        reference: impl Into<String>,
        role: impl Into<String>,
        domain: impl Into<String>,
        required: bool,
    ) -> Self {
        Self {
            reference: reference.into(),
            role: role.into(),
            domain: domain.into(),
            required,
        }
    }
}

/// Common addressable entry record for every NEF8/ListFile asset dictionary.
///
/// Domain bodies may keep their own compact tables, but they should project this
/// shape for asset catalog UI projections, AssetGraphResolver and conformance tests. Payload
/// ranges are relative to the inflated NEF8 body, never to a filesystem path.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ListFileEntryRecord {
    pub name: String,
    pub stable_hash: u64,
    pub entry_kind: String,
    pub schema: String,
    pub payload_offset: u64,
    pub payload_len: u64,
    pub payload_hash: [u8; 32],
    pub dependencies_range: Range<u32>,
    pub metadata_range: Range<u32>,
    pub flags: u32,
}

impl Default for ListFileEntryRecord {
    fn default() -> Self {
        Self {
            name: String::new(),
            stable_hash: 0,
            entry_kind: String::new(),
            schema: String::new(),
            payload_offset: 0,
            payload_len: 0,
            payload_hash: [0; 32],
            dependencies_range: 0..0,
            metadata_range: 0..0,
            flags: 0,
        }
    }
}

impl ListFileEntryRecord {
    #[inline]
    pub fn new(
        name: impl Into<String>,
        entry_kind: impl Into<String>,
        schema: impl Into<String>,
    ) -> Self {
        let name = name.into();
        Self {
            stable_hash: stable_hash_from_text(&name),
            name,
            entry_kind: entry_kind.into(),
            schema: schema.into(),
            ..Default::default()
        }
    }
}

/// Common metadata namespace blob. Unknown namespaces must be preserved by tools
/// and ignored by runtime domains that do not own them.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct ListFileMetadataNamespace {
    pub namespace: String,
    pub schema: String,
    pub payload_offset: u64,
    pub payload_len: u64,
}
/// One addressable entry inside a dictionary/container file.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct AssetEntryManifest {
    pub name: String,
    pub stable_id: String,
    pub asset_kind: String,
    pub entry_ref: String,
    pub route: AssetGatewayRoute,
    pub dependencies: Vec<AssetEntryDependency>,
    pub metadata: BTreeMap<String, String>,
}
impl AssetEntryManifest {
    #[inline]
    pub fn new(
        name: impl Into<String>,
        asset_kind: impl Into<String>,
        entry_ref: impl Into<String>,
    ) -> Self {
        let name = name.into();
        Self {
            stable_id: stable_id_from_text(&name),
            name,
            asset_kind: asset_kind.into(),
            entry_ref: entry_ref.into(),
            ..Default::default()
        }
    }
}

/// Universal `listFiles` result for authored files and package containers.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetFileManifest {
    pub schema: String,
    pub source: String,
    pub file_kind: String,
    pub container: String,
    pub codec: String,
    pub entries: Vec<AssetEntryManifest>,
    pub dependencies: Vec<AssetEntryDependency>,
    pub warnings: Vec<String>,
    pub policy: Vec<String>,
}

impl Default for AssetFileManifest {
    fn default() -> Self {
        Self {
            schema: ASSET_FILE_MANIFEST_SCHEMA.to_owned(),
            source: String::new(),
            file_kind: String::new(),
            container: String::new(),
            codec: String::new(),
            entries: Vec::new(),
            dependencies: Vec::new(),
            warnings: Vec::new(),
            policy: vec![
                "entries are addressed as <logical-path>@entry".to_owned(),
                "logical paths are VFS paths, never physical filesystem paths".to_owned(),
                "domain gateways interpret entries; AssetManager owns bytes and codec dispatch"
                    .to_owned(),
            ],
        }
    }
}

#[inline]
pub fn entry_ref(logical_path: &str, entry: &str) -> String {
    let path = logical_path
        .trim()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_owned();
    let entry = entry.trim();
    if entry.is_empty() {
        path
    } else {
        format!("{path}@{entry}")
    }
}

#[inline]
pub fn stable_id_from_text(value: &str) -> String {
    format!("{:016x}", stable_hash_from_text(value))
}

#[inline]
pub fn stable_hash_from_text(value: &str) -> u64 {
    fnv1a64(value.as_bytes())
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Binary ListFile envelope magic used by every authored asset dictionary file.
///
/// File extensions stay domain-facing (`.ytyp`, `.ytd`, `.ydd`, `.nemat`, ...),
/// but the top-level binary envelope is always NEF8. Concrete content identity is
/// declared by `content_kind`, not by per-format magic bytes.
pub const LIST_FILE_MAGIC_NEF8: [u8; 4] = *b"NEF8";
/// Current self-describing variable-header wire version.
pub const LIST_FILE_VERSION: u16 = 2;
pub const NEF8_WIRE_CONTRACT_SPEC: newviso_contract_api::ContractSpec =
    newviso_contract_api::ContractSpec::new(
        "asset.nef8.wire",
        newviso_contract_api::ContractKind::Wire,
        newviso_contract_api::ContractVersion::major(LIST_FILE_VERSION),
        newviso_contract_api::ContractCompatibility::Exact,
        "newengine-assets-api",
        None,
    );
pub const LIST_FILE_HEADER_SIZE_CLASS_MIN: u8 = 4;
pub const LIST_FILE_HEADER_SIZE_CLASS_MAX: u8 = 8;
pub const LIST_FILE_HEADER_LEN_MIN: usize = 16;
/// Bodies below this threshold default to compact headers without full BLAKE3.
pub const LIST_FILE_FULL_HASH_BODY_THRESHOLD: usize = 4096;
pub const LIST_FILE_FLAG_BODY_DEFLATE: u16 = 0x0001;
pub const LIST_FILE_FLAG_HEADER_METADATA: u16 = 0x0002;
pub const LIST_FILE_FLAG_BODY_HASH_BLAKE3: u16 = 0x0004;
pub const LIST_FILE_FLAG_STABLE_FILE_ID: u16 = 0x0008;
pub const LIST_FILE_FLAG_IMPORT_SETTINGS_HASH: u16 = 0x0010;
pub const LIST_FILE_COMPRESSION_DEFLATE: u16 = 1;

pub const LIST_FILE_CONTENT_KIND_UNKNOWN: u32 = 0;
pub const LIST_FILE_CONTENT_KIND_YTD: u32 = 1;
pub const LIST_FILE_CONTENT_KIND_YDD: u32 = 2;
pub const LIST_FILE_CONTENT_KIND_YTYP: u32 = 3;
pub const LIST_FILE_CONTENT_KIND_NEMAT: u32 = 4;
pub const LIST_FILE_CONTENT_KIND_YMAP: u32 = 5;
pub const LIST_FILE_CONTENT_KIND_YDR: u32 = 6;
pub const LIST_FILE_CONTENT_KIND_YFT: u32 = 7;
/// Bounds dictionary type identifier.
pub const LIST_FILE_CONTENT_KIND_YBN: u32 = 8;
/// Y Font Dictionary type identifier. Wire id 22 is retained across the format rename.
pub const LIST_FILE_CONTENT_KIND_YFD: u32 = 22;
pub const LIST_FILE_CONTENT_KIND_YMF: u32 = 9;
pub const LIST_FILE_CONTENT_KIND_YMT: u32 = 10;
pub const LIST_FILE_CONTENT_KIND_YCD: u32 = 11;
pub const LIST_FILE_CONTENT_KIND_YED: u32 = 12;
pub const LIST_FILE_CONTENT_KIND_YLD: u32 = 14;
pub const LIST_FILE_CONTENT_KIND_YPDB: u32 = 15;
pub const LIST_FILE_CONTENT_KIND_YVR: u32 = 16;
pub const LIST_FILE_CONTENT_KIND_YWR: u32 = 17;
pub const LIST_FILE_CONTENT_KIND_YSC: u32 = 18;
pub const LIST_FILE_CONTENT_KIND_YBD: u32 = 19;
pub const LIST_FILE_CONTENT_KIND_YTF: u32 = 20;
pub const LIST_FILE_CONTENT_KIND_YTYD: u32 = 21;
/// NewEngine UI dictionary: surfaces/layouts/themes/components/bindings in XMLcentral payload.
pub const LIST_FILE_CONTENT_KIND_NEUI: u32 = 32;
/// NewEngine authored item/inventory definition package.
pub const LIST_FILE_CONTENT_KIND_NEITEMS: u32 = 33;
/// Y Sound Cue Dictionary: embedded encoded audio payloads + cue playback metadata.
pub const LIST_FILE_CONTENT_KIND_YSNCD: u32 = 34;
/// Project-authored FX Dictionary: semantic VFX graphs and project texture references.
pub const LIST_FILE_CONTENT_KIND_FXD: u32 = 35;

/// Stable human-readable label for a NEF8/ListFile content kind.
///
/// Tooling uses this projection for diagnostics and inspect output; wire identity
/// remains the numeric `content_kind`. Unknown/future ids intentionally collapse
/// to `unknown` so old tools can inspect newer envelopes without inventing meaning.
#[inline]
pub fn list_file_content_kind_label(content_kind: u32) -> &'static str {
    match content_kind {
        LIST_FILE_CONTENT_KIND_YTD => "ytd_texture_dictionary",
        LIST_FILE_CONTENT_KIND_YDD => "ydd_drawable_dictionary",
        LIST_FILE_CONTENT_KIND_YTYP => "ytyp_archetype_dictionary",
        LIST_FILE_CONTENT_KIND_NEMAT => "nemat_material_library",
        LIST_FILE_CONTENT_KIND_YMAP => "ymap_map_data",
        LIST_FILE_CONTENT_KIND_YDR => "ydr_drawable",
        LIST_FILE_CONTENT_KIND_YFT => "yft_fragment",
        LIST_FILE_CONTENT_KIND_YBN => "ybn_bounds_dictionary",
        LIST_FILE_CONTENT_KIND_YMF => "ymf_manifest",
        LIST_FILE_CONTENT_KIND_YMT => "ymt_metadata",
        LIST_FILE_CONTENT_KIND_YCD => "ycd_clip_dictionary",
        LIST_FILE_CONTENT_KIND_YED => "yed_expression_dictionary",
        LIST_FILE_CONTENT_KIND_YLD => "yld_cloth_dictionary",
        LIST_FILE_CONTENT_KIND_YPDB => "ypdb_particle_dictionary",
        LIST_FILE_CONTENT_KIND_YVR => "yvr_vehicle_recording",
        LIST_FILE_CONTENT_KIND_YWR => "ywr_waypoint_recording",
        LIST_FILE_CONTENT_KIND_YSC => "ysc_script_dictionary",
        LIST_FILE_CONTENT_KIND_YBD => "ybd_bytecode_dictionary",
        LIST_FILE_CONTENT_KIND_YTF => "ytf_texture_fragment",
        LIST_FILE_CONTENT_KIND_YTYD => "ytyd_type_dictionary",
        LIST_FILE_CONTENT_KIND_YFD => "yfd_font_dictionary",
        LIST_FILE_CONTENT_KIND_NEUI => "neui_ui_dictionary",
        LIST_FILE_CONTENT_KIND_NEITEMS => "neitems_item_dictionary",
        LIST_FILE_CONTENT_KIND_YSNCD => "ysncd_sound_cue_dictionary",
        LIST_FILE_CONTENT_KIND_FXD => "fxd_effect_dictionary",
        _ => "unknown",
    }
}

/// The numeric `LIST_FILE_CONTENT_KIND_*` constants above are frozen wire-compatibility
/// aliases for existing assets only. They are not a registry and new asset formats must
/// publish their own opaque content kind through `AssetFileTypeDescriptor`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListFileHeader {
    pub version: u16,
    /// Header size class in `4..=8` (`header_len = 1 << size_class`).
    pub size_class: u8,
    pub header_len: u16,
    pub content_kind: u32,
    pub content_schema_version: u16,
    pub flags: u16,
    pub compression: u16,
    pub entry_count: u32,
    pub header_metadata_offset: u64,
    pub header_metadata_len: u64,
    pub body_offset: u64,
    pub body_len: u64,
    pub body_uncompressed_len: u64,
    /// BLAKE3 hash of the inflated NEF8 body bytes.
    pub body_raw_hash: [u8; 32],
    pub import_settings_hash: u64,
    pub stable_file_id: u64,
}

impl ListFileHeader {
    #[inline]
    pub fn is_deflate_body(&self) -> bool {
        (self.flags & LIST_FILE_FLAG_BODY_DEFLATE) != 0
            && self.compression == LIST_FILE_COMPRESSION_DEFLATE
    }

    #[inline]
    pub fn has_body_raw_hash(&self) -> bool {
        (self.flags & LIST_FILE_FLAG_BODY_HASH_BLAKE3) != 0
    }

    #[inline]
    pub fn has_header_metadata(&self) -> bool {
        (self.flags & LIST_FILE_FLAG_HEADER_METADATA) != 0
    }

    #[inline]
    pub fn has_stable_file_id(&self) -> bool {
        (self.flags & LIST_FILE_FLAG_STABLE_FILE_ID) != 0
    }

    #[inline]
    pub fn has_import_settings_hash(&self) -> bool {
        (self.flags & LIST_FILE_FLAG_IMPORT_SETTINGS_HASH) != 0
    }

    #[inline]
    pub fn content_kind_matches(&self, expected: u32) -> bool {
        self.content_kind == expected
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ListFileHeaderMetadata {
    pub schema: String,
    pub logical_path: String,
    pub content_kind: String,
    pub authored_by: String,
    pub source: String,
    pub build_profile: String,
    pub asset_graph_hash: String,
    pub import_settings_hash: String,
    pub entries: Vec<AssetEntryManifest>,
    pub dependencies: Vec<AssetEntryDependency>,
    pub warnings: Vec<String>,
    pub policy: Vec<String>,
}

impl Default for ListFileHeaderMetadata {
    fn default() -> Self {
        Self {
            schema: "newengine.asset.list_file.header_metadata".to_owned(),
            logical_path: String::new(),
            content_kind: String::new(),
            authored_by: String::new(),
            source: String::new(),
            build_profile: String::new(),
            asset_graph_hash: String::new(),
            import_settings_hash: String::new(),
            entries: Vec::new(),
            dependencies: Vec::new(),
            warnings: Vec::new(),
            policy: vec![
                "NEF8 is the only top-level magic for authored asset dictionary files".to_owned(),
                "content_kind in the header selects the domain payload".to_owned(),
                "body is deflate-compressed and decoded by the content domain handler".to_owned(),
            ],
        }
    }
}

/// Parse the canonical self-describing NEF8 envelope.
#[inline]
pub fn parse_list_file_header(bytes: &[u8]) -> Result<ListFileHeader, String> {
    nef8::parse_list_file_header(bytes)
}
