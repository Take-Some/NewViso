/// Normative contract behind the generic `asset.decode_v1` codec boundary.
/// The advertised id intentionally remains the method token already carried by
/// AssetManager/codec descriptors; the stable registry key is version-neutral.
pub const ASSET_DECODE_PROTOCOL_CONTRACT_SPEC: newviso_contract_api::ContractSpec =
    newviso_contract_api::ContractSpec::new(
        "asset.decode.protocol",
        newviso_contract_api::ContractKind::Protocol,
        newviso_contract_api::ContractVersion::major(1),
        newviso_contract_api::ContractCompatibility::Exact,
        "newengine-assets-api",
        Some(method::DECODE_V1),
    );

/// Normative contract for editor/package write-back through container codecs.
pub const CONTAINER_WRITE_BYTES_PROTOCOL_CONTRACT_SPEC: newviso_contract_api::ContractSpec =
    newviso_contract_api::ContractSpec::new(
        "asset.container.write_bytes.protocol",
        newviso_contract_api::ContractKind::Protocol,
        newviso_contract_api::ContractVersion::major(1),
        newviso_contract_api::ContractCompatibility::Exact,
        "newengine-assets-api",
        Some("container.write_bytes_v1"),
    );

/// Descriptor-driven preview contract shared by AssetManager and Editor surfaces.
pub const ASSET_PREVIEW_PROTOCOL_ID: &str = "newengine.assets.preview.contract.v1";
pub const ASSET_PREVIEW_PROTOCOL_CONTRACT_SPEC: newviso_contract_api::ContractSpec =
    newviso_contract_api::ContractSpec::new(
        "asset.preview.protocol",
        newviso_contract_api::ContractKind::Protocol,
        newviso_contract_api::ContractVersion::major(1),
        newviso_contract_api::ContractCompatibility::Exact,
        "newengine-assets-api",
        Some(ASSET_PREVIEW_PROTOCOL_ID),
    );

pub mod textures_method {
    pub const INFO_JSON: &str = newviso_service_api::SERVICE_METHOD_INFO_JSON;
    pub const INVOKE_JSON: &str = newviso_service_api::SERVICE_METHOD_INVOKE_JSON;
    pub const SHUTDOWN_V1: &str = newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1;
    pub const MANIFEST_JSON_V1: &str = "assets.textures.manifest_v1";
    pub const ENTRY_RUNTIME_V1: &str = "assets.textures.entry_runtime_v1";
    pub const ENTRY_RGBA8_V1: &str = "assets.textures.entry_rgba8_v1";
    pub const VALIDATE_REF_V1: &str = "assets.textures.validate_ref_v1";
    pub const DESCRIBE_REF_JSON_V1: &str = "assets.textures.describe_ref_json_v1";
}

pub mod definitions_method {
    pub const INFO_JSON: &str = newviso_service_api::SERVICE_METHOD_INFO_JSON;
    pub const INVOKE_JSON: &str = newviso_service_api::SERVICE_METHOD_INVOKE_JSON;
    pub const SHUTDOWN_V1: &str = newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1;
    pub const MANIFEST_JSON_V1: &str = "assets.definitions.manifest_v1";
    pub const ENTRY_JSON_V1: &str = "assets.definitions.entry_v1";
    pub const RESOLVE_REFS_V1: &str = "assets.definitions.resolve_refs_v1";
    pub const VALIDATE_V1: &str = "assets.definitions.validate_v1";
    pub const DESCRIBE_SIDE_EFFECTS_V1: &str = "assets.definitions.describe_side_effects_v1";
}

pub const DEFINITIONS_SERVICE_METHODS: &[&str] = &[
    definitions_method::INFO_JSON,
    definitions_method::INVOKE_JSON,
    definitions_method::SHUTDOWN_V1,
    definitions_method::MANIFEST_JSON_V1,
    definitions_method::ENTRY_JSON_V1,
    definitions_method::RESOLVE_REFS_V1,
    definitions_method::VALIDATE_V1,
    definitions_method::DESCRIBE_SIDE_EFFECTS_V1,
];

pub mod maps_method {
    pub const INFO_JSON: &str = newviso_service_api::SERVICE_METHOD_INFO_JSON;
    pub const INVOKE_JSON: &str = newviso_service_api::SERVICE_METHOD_INVOKE_JSON;
    pub const SHUTDOWN_V1: &str = newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1;
    pub const INDEX_V1: &str = "assets.maps.index_v1";
    pub const CELL_V1: &str = "assets.maps.cell_v1";
    pub const CELL_V2: &str = "assets.maps.cell_v2";
    pub const VALIDATE_V1: &str = "assets.maps.validate_v1";
    pub const DEPENDENCIES_V1: &str = "assets.maps.dependencies_v1";
}

pub const MAPS_SERVICE_METHODS: &[&str] = &[
    maps_method::INFO_JSON,
    maps_method::INVOKE_JSON,
    maps_method::SHUTDOWN_V1,
    maps_method::INDEX_V1,
    maps_method::CELL_V1,
    maps_method::CELL_V2,
    maps_method::VALIDATE_V1,
    maps_method::DEPENDENCIES_V1,
];

pub const DEFINITIONS_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets.definitions",
        ENGINE_ASSETS_DEFINITIONS_SERVICE_ID,
        DEFINITIONS_SERVICE_ID,
        DEFINITIONS_BACKEND_CAPABILITY_ID,
    );

pub const MAPS_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets.maps",
        ENGINE_ASSETS_MAPS_SERVICE_ID,
        MAPS_SERVICE_ID,
        MAPS_BACKEND_CAPABILITY_ID,
    );

pub const ASSET_GRAPH_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets.graph",
        ENGINE_ASSETS_GRAPH_SERVICE_ID,
        ASSET_GRAPH_SERVICE_ID,
        ASSET_GRAPH_BACKEND_CAPABILITY_ID,
    );

pub const ASSETS_UI_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets.ui",
        ENGINE_ASSETS_UI_SERVICE_ID,
        ASSETS_UI_SERVICE_ID,
        ASSETS_UI_BACKEND_CAPABILITY_ID,
    );

pub const DEFINITIONS_RUNTIME_CONTRACT_SPEC: newviso_service_api::RuntimeServiceContractSpec =
    newviso_service_api::RuntimeServiceContractSpec::new(
        ENGINE_ASSETS_DEFINITIONS_SERVICE_ID,
        DEFINITIONS_RUNTIME_CONTRACT,
        DEFINITIONS_SERVICE_METHODS,
    );

pub const DEFINITIONS_RUNTIME_REQUIREMENT_SPEC:
    newviso_service_api::RuntimeServiceRequirementSpec =
    newviso_service_api::RuntimeServiceRequirementSpec::new(
        DEFINITIONS_RUNTIME_CONTRACT_SPEC,
        Some(DEFINITIONS_BACKEND_CAPABILITY_ID),
        Some("NEWENGINE_REQUIRE_DEFINITIONS_BACKEND"),
    );

pub const MAPS_RUNTIME_CONTRACT_SPEC: newviso_service_api::RuntimeServiceContractSpec =
    newviso_service_api::RuntimeServiceContractSpec::new(
        ENGINE_ASSETS_MAPS_SERVICE_ID,
        MAPS_RUNTIME_CONTRACT,
        MAPS_SERVICE_METHODS,
    );

pub const MAPS_RUNTIME_REQUIREMENT_SPEC: newviso_service_api::RuntimeServiceRequirementSpec =
    newviso_service_api::RuntimeServiceRequirementSpec::new(
        MAPS_RUNTIME_CONTRACT_SPEC,
        Some(MAPS_BACKEND_CAPABILITY_ID),
        Some("NEWENGINE_REQUIRE_MAPS_BACKEND"),
    );

pub const ASSET_GRAPH_RUNTIME_CONTRACT_SPEC: newviso_service_api::RuntimeServiceContractSpec =
    newviso_service_api::RuntimeServiceContractSpec::new(
        ENGINE_ASSETS_GRAPH_SERVICE_ID,
        ASSET_GRAPH_RUNTIME_CONTRACT,
        ASSET_GRAPH_SERVICE_METHODS,
    );

pub const ASSET_GRAPH_RUNTIME_REQUIREMENT_SPEC:
    newviso_service_api::RuntimeServiceRequirementSpec =
    newviso_service_api::RuntimeServiceRequirementSpec::new(
        ASSET_GRAPH_RUNTIME_CONTRACT_SPEC,
        Some(ASSET_GRAPH_BACKEND_CAPABILITY_ID),
        Some("NEWENGINE_REQUIRE_ASSET_GRAPH_BACKEND"),
    );

pub const ASSETS_UI_RUNTIME_CONTRACT_SPEC: newviso_service_api::RuntimeServiceContractSpec =
    newviso_service_api::RuntimeServiceContractSpec::new(
        ENGINE_ASSETS_UI_SERVICE_ID,
        ASSETS_UI_RUNTIME_CONTRACT,
        ASSETS_UI_SERVICE_METHODS,
    );

pub const ASSETS_UI_RUNTIME_REQUIREMENT_SPEC: newviso_service_api::RuntimeServiceRequirementSpec =
    newviso_service_api::RuntimeServiceRequirementSpec::new(
        ASSETS_UI_RUNTIME_CONTRACT_SPEC,
        Some(ASSETS_UI_BACKEND_CAPABILITY_ID),
        Some("NEWENGINE_REQUIRE_ASSETS_UI_BACKEND"),
    );

pub const ASSETS_STREAMING_SERVICE_METHODS: &[&str] = &[
    newviso_service_api::SERVICE_METHOD_INFO_JSON,
    newviso_service_api::SERVICE_METHOD_INVOKE_JSON,
    newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1,
    method::STREAMING_REQUEST_V1,
    method::STREAMING_ADMIT_V2,
    method::STREAMING_EVICT_V2,
    method::STREAMING_LIFECYCLE_V2,
    method::STREAMING_PIN_V1,
    method::STREAMING_UNPIN_V1,
    method::STREAMING_TOUCH_V1,
    method::STREAMING_CLEANUP_V1,
    method::STREAMING_COMPACT_V1,
    method::STREAMING_STATS_V1,
];

pub const ASSETS_STREAMING_RUNTIME_CONTRACT_SPEC:
    newviso_service_api::RuntimeServiceContractSpec =
    newviso_service_api::RuntimeServiceContractSpec::new(
        ENGINE_ASSETS_STREAMING_SERVICE_ID,
        ASSETS_STREAMING_RUNTIME_CONTRACT,
        ASSETS_STREAMING_SERVICE_METHODS,
    );

pub const ASSETS_STREAMING_RUNTIME_REQUIREMENT_SPEC:
    newviso_service_api::RuntimeServiceRequirementSpec =
    newviso_service_api::RuntimeServiceRequirementSpec::new(
        ASSETS_STREAMING_RUNTIME_CONTRACT_SPEC,
        Some(ASSETS_STREAMING_BACKEND_CAPABILITY_ID),
        Some("NEWENGINE_REQUIRE_ASSET_STREAMING_BACKEND"),
    );

/// Required runtime methods for AssetManager 0.6+ deployments.
///
/// The engine validates these before scene bootstrap so an old DLL cannot fail
/// later as "unknown method" inside foliage/profile loading.
pub const REQUIRED_RUNTIME_METHODS_V1: &[&str] = &[
    method::INFO_JSON,
    method::INVOKE_JSON,
    method::SHUTDOWN_V1,
    method::RAW_BYTES_V1,
    method::RAW_RANGE_V1,
    method::TEXT_V1,
    method::IMPORT_V1,
    method::TEXTURE_RGBA8_V1,
    method::DECODE_V1,
    method::PUMP_V1,
    method::STATUS_JSON_V1,
    method::STATUS_GRAPH_JSON_V1,
    method::PROJECT_STATUS_JSON_V1,
    method::FORMATS_JSON_V1,
    method::SOURCES_JSON_V1,
    method::VFS_LIST_JSON_V1,
    method::LIST_FILE_REPACK_JSON_V1,
    method::UID_JSON_V1,
    method::IMPORT_CACHE_JSON_V1,
    method::IMPORT_DIRTY_JSON_V1,
    method::IMPORT_SCAN_JSON_V1,
    method::IMPORT_GRAPH_JSON_V1,
    method::RUNTIME_GRAPH_JSON_V1,
    method::IMPORT_DIAGNOSTICS_JSON_V1,
    method::IMPORT_THUMBNAILS_JSON_V1,
    method::IMPORT_DEPENDENCIES_JSON_V1,
    method::IMPORT_QUEUE_JSON_V1,
    method::REIMPORT_V1,
    method::THUMBNAIL_JSON_V1,
    method::DIRTY_SCAN_JSON_V1,
    method::PACKAGE_WRITER_INFO_JSON_V1,
    method::PACKAGE_WRITE_NEPAK_JSON_V1,
    method::PACKAGE_WRITE_TEXT_JSON_V1,
    method::STREAMING_REQUEST_V1,
    method::STREAMING_ADMIT_V2,
    method::STREAMING_EVICT_V2,
    method::STREAMING_LIFECYCLE_V2,
    method::STREAMING_PIN_V1,
    method::STREAMING_UNPIN_V1,
    method::STREAMING_TOUCH_V1,
    method::STREAMING_CLEANUP_V1,
    method::STREAMING_COMPACT_V1,
    method::STREAMING_STATS_V1,
];

/// Startup validation contract for the engine-facing asset gateway.
///
/// Validation reads the active backend provider description through the gateway.
pub const ASSET_RUNTIME_CONTRACT_SPEC: newviso_service_api::RuntimeServiceContractSpec =
    newviso_service_api::RuntimeServiceContractSpec::new(
        ASSET_SERVICE_ID,
        "newengine.assets-api >= 0.8.x",
        REQUIRED_RUNTIME_METHODS_V1,
    );

/// Declarative startup requirement for the engine-facing asset gateway. Missing
/// assets degrade unless a strict runtime profile explicitly requires them.
pub const ASSET_RUNTIME_REQUIREMENT_SPEC: newviso_service_api::RuntimeServiceRequirementSpec =
    newviso_service_api::RuntimeServiceRequirementSpec::new(
        ASSET_RUNTIME_CONTRACT_SPEC,
        Some(ASSET_BACKEND_CAPABILITY_ID),
        Some("NEWENGINE_REQUIRE_ASSET_MANAGER"),
    );
