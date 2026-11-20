use super::*;

pub mod codec_type {
    /// Container codec. May expose nested VFS entries and may recursively host
    /// other assets. Example: .nepak.
    pub const CONTAINER: &str = "containerType";
    /// List codec. A single file contains multiple same-domain records selected
    /// by name/hash/index, but it cannot host nested assets. Examples: domain dictionaries projected from NEF8 entries.
    pub const LIST: &str = "listType";
    /// Canonical NEF8 ListFile binary envelope. The file extension remains domain-facing
    /// (`.ytyp`, `.ytd`, `.ydd`, `.nemat`) while the header content_kind selects the payload domain.
    pub const LIST_FILE: &str = "listFile";
    /// Single binary file with magic bytes and one decoded object. Not used for `.nemat`, which is a NEF8 material library.
    pub const SINGLE: &str = "singleType";
    /// Asset definition metadata. It is not tied to a text encoding: the same
    /// logical format may be XML today, binary tomorrow, or compressed binary
    /// later. Example: .ytyp Definition Entries.
    pub const DEFINITION: &str = "definitionType";
    /// Plain UTF-8 text without magic bytes. Example: future .bindings.json codec.
    pub const PLAIN_TEXT: &str = "plainText";
}

#[inline]
pub fn codec_type_allows_nested_assets(codec_type: &str) -> bool {
    codec_type
        .trim()
        .eq_ignore_ascii_case(codec_type::CONTAINER)
}

#[inline]
pub fn codec_type_requires_magic_by_default(codec_type: &str) -> bool {
    !codec_type
        .trim()
        .eq_ignore_ascii_case(codec_type::PLAIN_TEXT)
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetFileTypeDescriptor {
    /// Stable format-module identity. Concrete file-type identity is owned by the
    /// loadable module under StarVault `formats/`, never by core/domain code.
    pub module_id: String,
    /// Broad presentation/organization family such as `textures`, `models`, `audio`.
    pub family: String,
    pub extension: String,
    pub asset_kind: String,
    pub container: String,
    /// Optional provider-declared NEF8/ListFile content kind. The registry stores
    /// this as descriptor data only; core must not derive semantic routing from it.
    pub content_kind: Option<u32>,
    /// Broad codec class. AssetManager uses this only for generic restrictions:
    /// nested VFS is allowed for `containerType` and forbidden for every other kind.
    pub codec_type: String,
    /// Owner of byte access, VFS/package mount and codec dispatch. For normal
    /// runtime assets this is `engine.assets`.
    pub byte_owner: String,
    /// Gateway that owns semantic interpretation of decoded entries.
    pub semantic_gateway: String,
    /// Compatibility projection for older descriptor consumers. It mirrors
    /// `semantic_gateway` and must not be used as the byte owner.
    pub gateway: String,
    pub handler_service: String,
    pub read_method: String,
    pub selector_syntax: Option<String>,
    pub consumer_domains: Vec<String>,
    /// Default semantic route for generic ListFile entries. Format-specific
    /// manifests may override this per entry, but generic writers must not infer
    /// routes from extensions or content-kind tables.
    pub default_entry_route: Option<AssetGatewayRoute>,
    /// Current authored content schema when the format owns one.
    pub content_schema_version: Option<u16>,
    /// Runtime-readable schema revisions, oldest to newest. Empty means the format
    /// does not expose a versioned body contract through this descriptor.
    pub readable_content_schema_versions: Vec<u16>,
    /// Contract-registry key for the current body schema, when applicable.
    pub schema_contract: String,
    /// Canonical authored/source schema id, when the format has a text authoring form.
    pub authored_schema: String,
    /// Read-only compatibility authoring schemas.
    pub legacy_authored_schemas: Vec<String>,
    /// Provider-owned preview classification. Consumers must not derive this from extension.
    pub preview_kind: String,
    pub preview_strategy: String,
    pub preview_gateway: String,
    pub icon_ref: String,
    /// Hex-encoded magic bytes. Required for magic-routed binary codecs, optional
    /// for codecs that deliberately own extension/source-policy routing such as
    /// `definitionType` authored XML beside future binary envelopes.
    pub magic: Option<String>,
    pub outputs: Vec<String>,
    pub priority: i32,
    pub vfs_backed: bool,
    pub runtime_ready: bool,
    /// Provider declares that `engine.assets.inspect` can return a preview DTO for this type.
    pub preview_provider: bool,
    /// Compatibility projection: provider declares schema-editable fields.
    /// New UI should prefer `schema_editable` and `write_back_available`.
    pub editable: bool,
    /// Provider declares that `engine.assets.inspect` can return editable field schema.
    /// This does not imply save/write-back availability.
    pub schema_editable: bool,
    /// True only when a concrete format/package writer capability is registered or declared.
    pub write_back_available: bool,
    /// Explicit capability id required for provider write-back. Empty means missing.
    pub writer_capability: String,
    /// Provider-owned inspect contract id, for example `asset.inspect.ytyp.v1`.
    pub inspect_contract: String,
    /// Provider-owned edit contract id, for example `asset.edit.ytyp.v1`. Empty means read-only transport.
    pub edit_contract: String,
    /// True only for codecs that may expose nested VFS entries. This must match
    /// `codec_type == containerType`; mismatches are rejected by the registry.
    pub allow_nested_assets: bool,
    /// Kept as a semantic flag for tooling: the runtime container is native to
    /// NewEngine, not an authoring/source format. It does not grant nesting.
    pub native_container: bool,
    /// Magic is required by default. `plainText` and carefully scoped
    /// `definitionType` codecs may set this to false and identify by
    /// extension/source policy.
    pub requires_magic: bool,
    pub notes: String,
}

impl Default for AssetFileTypeDescriptor {
    fn default() -> Self {
        Self {
            module_id: String::new(),
            family: String::new(),
            extension: String::new(),
            asset_kind: String::new(),
            container: String::new(),
            content_kind: None,
            codec_type: codec_type::SINGLE.to_owned(),
            byte_owner: ENGINE_ASSET_SERVICE_ID.to_owned(),
            semantic_gateway: String::new(),
            gateway: String::new(),
            handler_service: String::new(),
            read_method: method::DECODE_V1.to_owned(),
            selector_syntax: None,
            consumer_domains: Vec::new(),
            default_entry_route: None,
            content_schema_version: None,
            readable_content_schema_versions: Vec::new(),
            schema_contract: String::new(),
            authored_schema: String::new(),
            legacy_authored_schemas: Vec::new(),
            preview_kind: String::new(),
            preview_strategy: String::new(),
            preview_gateway: String::new(),
            icon_ref: String::new(),
            magic: None,
            outputs: Vec::new(),
            priority: 0,
            vfs_backed: true,
            runtime_ready: false,
            preview_provider: false,
            editable: false,
            schema_editable: false,
            write_back_available: false,
            writer_capability: String::new(),
            inspect_contract: String::new(),
            edit_contract: String::new(),
            allow_nested_assets: false,
            native_container: false,
            requires_magic: true,
            notes: String::new(),
        }
    }
}

impl AssetFileTypeDescriptor {
    pub fn extension_key(extension: &str) -> String {
        extension
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase()
    }

    #[inline]
    pub fn is_container_codec(&self) -> bool {
        codec_type_allows_nested_assets(&self.codec_type)
    }

    #[inline]
    pub fn normalize_layer_contract(&mut self) {
        self.extension = Self::extension_key(&self.extension);
        if self.byte_owner.trim().is_empty() {
            self.byte_owner = ENGINE_ASSET_SERVICE_ID.to_owned();
        }
        // File-type semantics are not inferred here. Each format crate/codec
        // must declare its semantic gateway, handler service and consumers in
        // its own descriptor. The registry is a generic collector/resolver, not
        // a central table of known extensions.
        if self.gateway.trim().is_empty() && !self.semantic_gateway.trim().is_empty() {
            self.gateway = self.semantic_gateway.clone();
        }
        // Keep `gateway` as a semantic projection for descriptor consumers.
        // It must not be used as the byte/VFS owner.
        if self.gateway.trim() != self.semantic_gateway.trim() {
            self.gateway = self.semantic_gateway.clone();
        }
        if self.consumer_domains.is_empty() && !self.semantic_gateway.trim().is_empty() {
            self.consumer_domains = vec![self.semantic_gateway.clone()];
        }
        if self.preview_provider {
            if self.preview_gateway.trim().is_empty() {
                self.preview_gateway = self.semantic_gateway.clone();
            }
            if self.preview_kind.trim().is_empty() {
                self.preview_kind = "generic_asset".to_owned();
            }
            if self.preview_strategy.trim().is_empty() {
                self.preview_strategy = "metadata_card".to_owned();
            }
        }
        if self.preview_provider && self.inspect_contract.trim().is_empty() {
            self.inspect_contract = format!("asset.inspect.{}.v1", self.extension);
        }
        if self.editable && !self.schema_editable {
            self.schema_editable = true;
        }
        if self.schema_editable && self.edit_contract.trim().is_empty() {
            self.edit_contract = format!("asset.edit.{}.v1", self.extension);
        }
        if self.write_back_available && self.writer_capability.trim().is_empty() {
            self.write_back_available = false;
        }
    }

    #[inline]
    pub fn validate_generic_rules(&self) -> Result<(), String> {
        let ext = Self::extension_key(&self.extension);
        if ext.is_empty() {
            return Err("codec descriptor extension is empty".to_owned());
        }
        if self.byte_owner.trim().is_empty() {
            return Err(format!("codec '.{}' descriptor byte_owner is empty", ext));
        }
        if self.semantic_gateway.trim().is_empty() {
            return Err(format!(
                "codec '.{}' descriptor semantic_gateway is empty",
                ext
            ));
        }
        if self.gateway.trim() != self.semantic_gateway.trim() {
            return Err(format!(
                "codec '.{}' descriptor gateway must mirror semantic_gateway ('{}' != '{}')",
                ext, self.gateway, self.semantic_gateway
            ));
        }
        if self.module_id.trim().is_empty() {
            return Err(format!(
                "asset type '.{}' descriptor module_id is empty",
                ext
            ));
        }
        if self.handler_service.trim().is_empty() {
            return Err(format!(
                "codec '.{}' descriptor handler_service is empty",
                ext
            ));
        }
        if let Some(route) = &self.default_entry_route {
            if route.gateway.trim().is_empty()
                || route.method.trim().is_empty()
                || route.semantic_owner.trim().is_empty()
            {
                return Err(format!(
                    "asset type '.{}' default_entry_route is incomplete",
                    ext
                ));
            }
        }
        let is_container = self.is_container_codec();
        if self.allow_nested_assets != is_container {
            return Err(format!(
                "codec '.{}' nesting flag mismatch: allow_nested_assets={} codec_type='{}'",
                ext, self.allow_nested_assets, self.codec_type
            ));
        }
        if codec_type_requires_magic_by_default(&self.codec_type)
            && self.requires_magic
            && self.magic.is_none()
        {
            return Err(format!(
                "codec '.{}' is binary type '{}' but declares no magic bytes",
                ext, self.codec_type
            ));
        }
        if !is_container
            && self
                .outputs
                .iter()
                .any(|o| o == "vfs.layer" || o == "container.vfs_layer")
        {
            return Err(format!(
                "codec '.{}' is '{}' and cannot expose nested VFS outputs",
                ext, self.codec_type
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetFileTypeManifest {
    pub schema: String,
    pub gateway: String,
    pub formats: Vec<AssetFileTypeDescriptor>,
}

impl Default for AssetFileTypeManifest {
    fn default() -> Self {
        Self {
            schema: "newengine.asset_types.v2".to_owned(),
            gateway: ENGINE_ASSET_TYPES_SERVICE_ID.to_owned(),
            formats: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct AssetFileTypeProbeRequest {
    pub logical_path: String,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct AssetFileTypeRegisterRequest {
    pub descriptor: AssetFileTypeDescriptor,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AssetDecodeRequest {
    pub logical_path: String,
    pub output_kind: String,
    pub selector: serde_json::Value,
    /// Authoritative StarVault file-type descriptor injected by AssetManager at the
    /// codec boundary. External callers normally leave this `None`; codecs must not
    /// infer concrete type identity from the path extension.
    pub format_descriptor: Option<AssetFileTypeDescriptor>,
}

impl Default for AssetDecodeRequest {
    fn default() -> Self {
        Self {
            logical_path: String::new(),
            output_kind: String::new(),
            selector: serde_json::Value::Null,
            format_descriptor: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct AssetFileTypeProbeResult {
    pub logical_path: String,
    pub extension: String,
    pub known: bool,
    pub descriptor: Option<AssetFileTypeDescriptor>,
}
