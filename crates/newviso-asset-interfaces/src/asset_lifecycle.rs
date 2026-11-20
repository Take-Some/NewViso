use super::*;
use core::time::Duration;

/// Asset lifecycle state as observed through an AssetManager-like service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetState {
    Unloaded,
    Loading,
    Ready,
    Failed,
    Unknown,
}

/// Residency domain. Stages are meaningful only inside a domain: VFS bytes, CPU-decoded payloads, and GPU resources are not interchangeable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetResidencyDomain {
    Vfs,
    Cpu,
    Gpu,
    Unknown,
}

impl AssetResidencyDomain {
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vfs => "vfs",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Unknown => "unknown",
        }
    }
}

impl core::fmt::Display for AssetResidencyDomain {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable, high-resolution asset lifecycle stage used by tooling, loading screens and render gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetStatusStage {
    Declared,
    Requested,
    Resolving,
    Queued,
    Reading,
    Importing,
    Imported,
    UploadQueued,
    Uploading,
    Resident,
    Failed,
    Stale,
    Unknown,
}

impl AssetStatusStage {
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Requested => "requested",
            Self::Resolving => "resolving",
            Self::Queued => "queued",
            Self::Reading => "reading",
            Self::Importing => "importing",
            Self::Imported => "imported",
            Self::UploadQueued => "upload_queued",
            Self::Uploading => "uploading",
            Self::Resident => "resident",
            Self::Failed => "failed",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }
}

impl core::fmt::Display for AssetStatusStage {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// AssetStatus is the canonical read-model row for one asset graph node.
///
/// The service serializes the same shape as JSON via `asset.status_json_v1`.
/// Runtime systems may keep richer local states, but they should be projected
/// from this model instead of inventing incompatible lifecycle enums.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetStatus {
    pub id_hex32: String,
    pub logical_path: Option<String>,
    pub state: AssetState,
    pub domain: AssetResidencyDomain,
    pub stage: AssetStatusStage,
    pub source: Option<String>,
    pub codec_id: Option<String>,
    pub type_id: Option<String>,
    pub format: Option<String>,
    pub bytes: Option<u64>,
    pub error: Option<String>,
    pub detail: Option<String>,
    pub updated_unix_ms: u64,
}

impl AssetStatus {
    #[inline]
    pub fn unknown(id_hex32: impl Into<String>) -> Self {
        Self {
            id_hex32: id_hex32.into(),
            logical_path: None,
            state: AssetState::Unknown,
            domain: AssetResidencyDomain::Unknown,
            stage: AssetStatusStage::Unknown,
            source: None,
            codec_id: None,
            type_id: None,
            format: None,
            bytes: None,
            error: None,
            detail: Some("AssetManager has no status row for this asset".to_string()),
            updated_unix_ms: 0,
        }
    }
}

/// Minimal engine-facing Asset access surface.
///
/// Implementations may be plugin-backed, filesystem-backed, HTTP-backed, etc.
fn parse_texture_entry_selector(
    texture_ref: &str,
) -> Result<(AssetReference, Option<String>, Option<u64>), String> {
    let reference = require_asset_reference_extension(texture_ref, &["ytd"], true)
        .map_err(|error| error.to_string())?;
    let entry = reference.entry.as_deref().unwrap_or_default();
    let texture_hash = entry
        .strip_prefix("hash:")
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("invalid texture hash selector '{entry}'"))
        })
        .transpose()?;
    let texture_name = texture_hash.is_none().then(|| entry.to_owned());
    Ok((reference, texture_name, texture_hash))
}

pub trait AssetAccess {
    /// Enqueue codec-owned asset load/decode by logical path. Returns an opaque stable id (hex32 string).
    fn import_v1(&self, logical_path: &str) -> Result<String, String>;

    /// Progress background AssetManager work through the stable v1 pump method.
    fn pump(&self);

    /// Query current state for an enqueued asset.
    fn state(&self, id_hex32: &str) -> Result<AssetState, String>;

    /// Query the canonical AssetStatus read-model row for an asset id or logical path.
    fn status_json_v1(&self, id_or_logical_path: &str) -> Result<serde_json::Value, String>;

    /// Query the full AssetStatus graph node for an asset id or logical path.
    fn status_graph_json_v1(&self, id_or_logical_path: &str) -> Result<serde_json::Value, String>;

    /// Project a validated lifecycle transition from an owning subsystem, e.g. render GPU residency.
    fn project_status_json_v1(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String>;

    /// Read UTF-8/text asset bytes by logical path through AssetManager/VFS.
    fn text_v1(&self, logical_path: &str) -> Result<Vec<u8>, String>;

    /// Read raw binary asset bytes by logical path through AssetManager/VFS.
    ///
    /// This is still AssetManager-owned VFS access; callers must not use
    /// filesystem paths or bypass mounts.
    fn raw_bytes_v1(&self, logical_path: &str) -> Result<Vec<u8>, String>;

    /// Read a bounded raw byte range through the same VFS ownership/resolution policy.
    fn raw_range_v1(
        &self,
        request: &crate::AssetRawRangeRequest,
    ) -> Result<crate::AssetRawRangeResponse, String>;

    /// Read asset payload using a stable wire format.
    ///
    /// Returns `(meta_json, payload_bytes)`.
    fn blob_wire_v1(&self, id_hex32: &str) -> Result<(String, Vec<u8>), String>;

    /// Decode any registered runtime asset container through the codec registry.
    fn decode_v1(&self, request: &AssetDecodeRequest) -> Result<Vec<u8>, String>;

    /// Read a runtime-ready RGBA8 texture packet.
    ///
    /// The implementation must parse/validate codec metadata inside AssetManager.
    /// Runtime callers must not parse image containers or codec metadata.
    fn texture_rgba8_v1(&self, id_hex32: &str) -> Result<Rgba8TextureAsset, String>;

    /// Select and read a runtime-ready RGBA8 texture from a .ytd dictionary.
    fn texture_dictionary_rgba8_v1(
        &self,
        dictionary_path: &str,
        texture_name: Option<&str>,
        texture_hash: Option<u64>,
    ) -> Result<Rgba8TextureAsset, String>;

    /// Select and read a runtime-ready GPU-native texture from a .ytd dictionary.
    fn texture_dictionary_runtime_v1(
        &self,
        dictionary_path: &str,
        texture_name: Option<&str>,
        texture_hash: Option<u64>,
    ) -> Result<RuntimeTextureAsset, String>;

    /// Select and read a runtime-ready RGBA8 texture through semantic `engine.assets.textures` ownership.
    ///
    /// Generic AssetAccess implementors may bridge to the older dictionary methods, but the
    /// canonical runtime host implementation routes this through `engine.assets.assets.textures.entry_rgba8_v1`.
    fn textures_entry_rgba8_v1(&self, texture_ref: &str) -> Result<Rgba8TextureAsset, String> {
        let (reference, texture_name, texture_hash) = parse_texture_entry_selector(texture_ref)?;
        self.texture_dictionary_rgba8_v1(
            &reference.logical_path,
            texture_name.as_deref(),
            texture_hash,
        )
    }

    /// Select and read a runtime-ready GPU-native texture through semantic `engine.assets.textures` ownership.
    ///
    /// Generic AssetAccess implementors may bridge to the older dictionary methods, but the
    /// canonical runtime host implementation routes this through `engine.assets.assets.textures.entry_runtime_v1`.
    fn textures_entry_runtime_v1(&self, texture_ref: &str) -> Result<RuntimeTextureAsset, String> {
        let (reference, texture_name, texture_hash) = parse_texture_entry_selector(texture_ref)?;
        self.texture_dictionary_runtime_v1(
            &reference.logical_path,
            texture_name.as_deref(),
            texture_hash,
        )
    }
}

/// Extended contract surface.
///
/// Keep this trait small and data-oriented; higher-level systems can build their own
/// caches and decoders above these primitives.
pub trait AssetService: AssetAccess {
    /// Reload/reimport asset by logical path through the stable v1 reload method.
    fn reload(&self, logical_path: &str) -> Result<String, String>;

    /// Query extended info by logical path.
    fn info_json_v1(&self, logical_path: &str) -> Result<serde_json::Value, String>;

    /// List known formats.
    fn formats_json_v1(&self) -> Result<serde_json::Value, String>;

    /// List mounted sources.
    fn sources_json_v1(&self) -> Result<serde_json::Value, String>;

    /// List a mounted VFS directory through AssetManager, not through direct filesystem paths.
    fn vfs_list_json_v1(&self, logical_path: &str) -> Result<serde_json::Value, String>;

    /// Rebuild/repack a NEF8 ListFile and write it back through the winning writable VFS source.
    fn list_file_repack_json_v1(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String>;

    /// Return the engine.assets UID row for a logical asset.
    fn uid_json_v1(&self, logical_path: &str) -> Result<serde_json::Value, String>;

    /// Return the editor/import cache projection over status, codec metadata and dirty flags.
    fn import_cache_json_v1(&self, payload: serde_json::Value)
        -> Result<serde_json::Value, String>;

    /// Mark one logical asset dirty/stale; file watchers should use this before reload/reimport.
    fn import_dirty_json_v1(&self, payload: serde_json::Value)
        -> Result<serde_json::Value, String>;

    /// Bounded VFS scan for editor/import discovery.
    fn import_scan_json_v1(&self, payload: serde_json::Value) -> Result<serde_json::Value, String>;

    /// Import dependency graph projection for one asset.
    fn import_graph_json_v1(&self, payload: serde_json::Value)
        -> Result<serde_json::Value, String>;

    /// Full provider-neutral runtime graph projection for dependency-aware hot reload.
    fn runtime_graph_json_v1(&self) -> Result<crate::AssetRuntimeGraphV1, String>;

    /// Human-readable import diagnostics.
    fn import_diagnostics_json_v1(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String>;

    /// Editor thumbnail metadata/cache-key plan. Final pixels belong to render/UI providers.
    fn import_thumbnails_json_v1(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String>;

    /// Direct dependency/dependent list for one asset.
    fn import_dependencies_json_v1(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String>;

    /// Queue read-model for background import work.
    fn import_queue_json_v1(&self, payload: serde_json::Value)
        -> Result<serde_json::Value, String>;

    /// Explicit dirty+reload lifecycle command.
    fn reimport_v1(&self, payload: serde_json::Value) -> Result<serde_json::Value, String>;

    /// Single asset thumbnail metadata/cache-key plan.
    fn thumbnail_json_v1(&self, payload: serde_json::Value) -> Result<serde_json::Value, String>;

    /// Bounded scan that classifies missing/dirty/stale rows for editor reimport.
    fn dirty_scan_json_v1(&self, payload: serde_json::Value) -> Result<serde_json::Value, String>;

    /// Package/listFile writer capability diagnostics.
    fn package_writer_info_json_v1(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String>;

    /// Explicit .nepak package writer execution through engine.assets.package_writer.
    fn package_write_nepak_json_v1(
        &self,
        payload: NepakPackageWriteRequestV1,
    ) -> Result<NepakPackageWriteResponseV1, String>;

    /// Replace one existing UTF-8 text asset through an explicit writable VFS source.
    fn package_write_text_json_v1(
        &self,
        payload: TextAssetWriteRequestV1,
    ) -> Result<TextAssetWriteResponseV1, String>;

    /// Mount one source through the strict v1 JSON source model.
    fn mount_source_json_v1(&self, payload: serde_json::Value) -> Result<(), String>;

    /// Returns a deterministic trace describing which sources contain the asset.
    fn resolve_trace_json_v1(&self, logical_path: &str) -> Result<serde_json::Value, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitReadyError {
    Timeout,
    Failed(String),
    Transport(String),
}

/// Progress the asset lifecycle once and return readiness.
///
/// This function deliberately does not sleep or spin. Runtime/editor callers must
/// call it from a frame, job, or asset-event callback and retry after the asset
/// pipeline publishes more work. `timeout` is retained as a compatibility guard
/// for callers that pass an already-expired deadline budget.
pub fn wait_ready<A: AssetAccess + ?Sized>(
    assets: &A,
    id_hex32: &str,
    timeout: Duration,
) -> Result<(), WaitReadyError> {
    if timeout.is_zero() {
        return Err(WaitReadyError::Timeout);
    }

    assets.pump();

    match assets.state(id_hex32) {
        Ok(AssetState::Ready) => Ok(()),
        Ok(AssetState::Failed) => Err(WaitReadyError::Failed(id_hex32.to_string())),
        Ok(AssetState::Loading) | Ok(AssetState::Unloaded) => Err(WaitReadyError::Timeout),
        Ok(AssetState::Unknown) => {
            log::warn!("Unknown asset state id='{}'", id_hex32);
            Err(WaitReadyError::Timeout)
        }
        Err(e) => Err(WaitReadyError::Transport(e)),
    }
}
