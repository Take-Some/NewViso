/// Asset codec classification used by AssetManager to apply generic host rules.
///
/// The host does not interpret concrete formats. It only enforces broad safety
/// constraints declared by the codec provider. Encoding/compression is not part
/// of this enum; codecs own their internal source envelope and may support
/// raw XML, native binary, deflate, etc. behind the same logical descriptor.
/// Canonical AssetManager v1 method names.
///
/// There is one supported runtime contract: explicit `*_v1` entry points for
/// import/pump/state/text/texture access. Older alias pairs such as
/// `asset.load`, `asset.pump`, and `asset.load_text_v1` are intentionally not
/// part of this surface.
pub mod method {
    /// Standard service-framework metadata method.
    pub const INFO_JSON: &str = newviso_service_api::SERVICE_METHOD_INFO_JSON;
    /// Standard service-framework JSON control invocation method.
    pub const INVOKE_JSON: &str = newviso_service_api::SERVICE_METHOD_INVOKE_JSON;

    pub const RELOAD_V1: &str = "asset.reload_v1";
    pub const INFO_JSON_V1: &str = "asset.info_json_v1";
    pub const STATE_JSON_V1: &str = "asset.state_json_v1";
    /// Current AssetStatus row by id or logical path. Payload accepts utf8 id_hex32 or logical path.
    pub const STATUS_JSON_V1: &str = "asset.status_json_v1";
    /// Full AssetStatus graph by id or logical path. Payload accepts utf8 id_hex32 or logical path.
    pub const STATUS_GRAPH_JSON_V1: &str = "asset.status_graph_json_v1";
    /// Validated lifecycle projection hook. Payload is JSON with owner/domain/logical_path/stage/proof.
    pub const PROJECT_STATUS_JSON_V1: &str = "asset.project_status_json_v1";
    pub const BLOB_WIRE_V1: &str = "asset.blob_wire_v1";
    /// Runtime-ready RGBA8 texture packet by asset id. AssetManager validates/parses codec metadata.
    pub const TEXTURE_RGBA8_V1: &str = "asset.texture_rgba8_v1";
    /// Generic codec dispatch. Payload is JSON `AssetDecodeRequest`; response is codec-defined bytes.
    pub const DECODE_V1: &str = "asset.decode_v1";
    /// Runtime-ready RGBA8 texture selected from a .ytd dictionary. Payload: JSON { dictionary_path, texture_name | texture_hash }.
    pub const TEXTURE_DICTIONARY_RGBA8_V1: &str = "asset.texture_dictionary_rgba8_v1";
    /// Runtime-ready GPU-native texture selected from a .ytd dictionary.
    /// Returns NTRT v2 with format + complete mip chain. BC1/BC3/BC5/BC7 stay compressed.
    pub const TEXTURE_DICTIONARY_RUNTIME_V1: &str = "asset.texture_dictionary_runtime_v1";
    /// Explicit BCn aliases for callers that want to assert a compressed format class.
    pub const TEXTURE_BC1_V1: &str = "asset.texture_bc1_v1";
    pub const TEXTURE_BC3_V1: &str = "asset.texture_bc3_v1";
    pub const TEXTURE_BC5_V1: &str = "asset.texture_bc5_v1";
    pub const TEXTURE_BC7_V1: &str = "asset.texture_bc7_v1";

    /// Stable v1 import entry point.
    pub const IMPORT_V1: &str = "asset.import_v1";
    /// Stable v1 pump entry point.
    pub const PUMP_V1: &str = "asset.pump_v1";
    /// Raw VFS bytes by logical path. This bypasses codecs but still resolves exclusively through AssetManager mounts.
    pub const RAW_BYTES_V1: &str = "asset.raw_bytes_v1";
    /// Bounded raw VFS byte range. Request is JSON `AssetRawRangeRequest`; response is NARR v1 binary.
    pub const RAW_RANGE_V1: &str = "asset.raw_range_v1";
    /// Raw UTF-8 text by logical path resolved through AssetManager mounts.
    pub const TEXT_V1: &str = "asset.text_v1";
    // Fast-path / batch APIs.
    pub const PRELOAD_MANY_V1: &str = "asset.preload_many_v1";
    pub const GET_STATE_V1: &str = "asset.get_state_v1";

    pub const FORMATS_JSON_V1: &str = "asset.formats_json_v1";
    pub const SOURCES_JSON_V1: &str = "asset.sources_json_v1";
    pub const VERIFY_ASSETS_JSON_V1: &str = "asset.verify_assets_json_v1";
    pub const SOURCE_KINDS_JSON_V1: &str = "asset.source_kinds_json_v1";
    /// Merged VFS directory listing by logical path. Payload accepts either a UTF-8
    /// logical directory path or JSON { logical_path }. Response is JSON only.
    pub const VFS_LIST_JSON_V1: &str = "asset.vfs_list_json_v1";
    /// Rebuild/repack a NEF8 ListFile after an editor-side entry update/delete/rename.
    /// Payload is JSON and write-back is performed only through a writable VFS source.
    pub const LIST_FILE_REPACK_JSON_V1: &str = "asset.list_file_repack_json_v1";
    /// Mount payload accepts asset_role and aliases [{ logical_path, source_path }].
    /// All compiled mounts precede source mounts regardless of numeric priority.
    pub const MOUNT_SOURCE_JSON_V1: &str = "asset.mount_source_json_v1";

    // Debug/diagnostics.
    pub const RESOLVE_TRACE_JSON_V1: &str = "asset.resolve_trace_json_v1";
    /// Standard listFiles manifest for any dictionary/container asset. Codec-defined output; not a raw VFS read.
    pub const LIST_FILE_MANIFEST: &str = "asset.list_file_manifest";

    // Editor/import lifecycle read-model.
    //
    // These methods deliberately stay under `engine.assets`: the asset backend owns
    // source discovery, UID/cache rows, dirty/reimport state and human-readable
    // diagnostics. Format meaning still belongs to codec/domain gateways, and final
    // editor panels/thumbnails remain UI composition over this data.
    pub const UID_JSON_V1: &str = "asset.uid_json_v1";
    pub const IMPORT_CACHE_JSON_V1: &str = "asset.import_cache_json_v1";
    pub const IMPORT_DIRTY_JSON_V1: &str = "asset.import_dirty_json_v1";
    pub const IMPORT_SCAN_JSON_V1: &str = "asset.import_scan_json_v1";
    pub const IMPORT_GRAPH_JSON_V1: &str = "asset.import_graph_json_v1";
    /// Full provider-neutral runtime dependency graph used by hot-reload/invalidation planners.
    pub const RUNTIME_GRAPH_JSON_V1: &str = "asset.runtime_graph_json_v1";
    pub const IMPORT_DIAGNOSTICS_JSON_V1: &str = "asset.import_diagnostics_json_v1";
    pub const IMPORT_THUMBNAILS_JSON_V1: &str = "asset.import_thumbnails_json_v1";
    pub const IMPORT_DEPENDENCIES_JSON_V1: &str = "asset.import_dependencies_json_v1";
    pub const IMPORT_QUEUE_JSON_V1: &str = "asset.import_queue_json_v1";
    pub const REIMPORT_V1: &str = "asset.reimport_v1";
    pub const THUMBNAIL_JSON_V1: &str = "asset.thumbnail_json_v1";
    pub const DIRTY_SCAN_JSON_V1: &str = "asset.dirty_scan_json_v1";
    pub const PACKAGE_WRITER_INFO_JSON_V1: &str = "asset.package_writer_info_json_v1";
    /// Explicit .nepak package writer execution. Payload is NepakPackageWriteRequestV1.
    pub const PACKAGE_WRITE_NEPAK_JSON_V1: &str = "asset.package_write_nepak_json_v1";
    /// Explicit UTF-8 text replacement through the winning writable VFS source.
    pub const PACKAGE_WRITE_TEXT_JSON_V1: &str = "asset.package_write_text_json_v1";

    // Global runtime residency / memory controller.
    pub const STREAMING_REQUEST_V1: &str = "asset.streaming.request_v1";
    /// Scheduler-selected provider admission. Demand selection remains engine-owned.
    pub const STREAMING_ADMIT_V2: &str = "asset.streaming.admit_v2";
    /// Exact scheduler-selected CPU residency eviction.
    pub const STREAMING_EVICT_V2: &str = "asset.streaming.evict_v2";
    /// Provider lifecycle acknowledgement used to reconcile engine residency state.
    pub const STREAMING_LIFECYCLE_V2: &str = "asset.streaming.lifecycle_v2";
    pub const STREAMING_PIN_V1: &str = "asset.streaming.pin_v1";
    pub const STREAMING_UNPIN_V1: &str = "asset.streaming.unpin_v1";
    pub const STREAMING_TOUCH_V1: &str = "asset.streaming.touch_v1";
    pub const STREAMING_CLEANUP_V1: &str = "asset.streaming.cleanup_v1";
    pub const STREAMING_COMPACT_V1: &str = "asset.streaming.compact_v1";
    pub const STREAMING_STATS_V1: &str = "asset.streaming.stats_v1";

    // Generic lifecycle hook understood by the plugin host.
    pub const SHUTDOWN_V1: &str = newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1;
}
