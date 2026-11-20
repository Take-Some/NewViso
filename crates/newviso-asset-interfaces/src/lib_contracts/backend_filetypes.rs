/// Generic host/plugin backend declaration for the asset service family.
pub const ASSET_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets",
        ENGINE_ASSET_SERVICE_ID,
        ASSET_PROVIDER_SERVICE_ID,
        ASSET_BACKEND_CAPABILITY_ID,
    );

/// Engine-facing file-type registry gateway id.
///
/// This is a descriptor/navigation surface over runtime asset containers. It does
/// not replace `engine.assets`; registered file-type handlers still read payloads
/// through the AssetManager/VFS gateway.
pub const ENGINE_ASSET_TYPES_SERVICE_ID: &str = "engine.assets.types";
pub const ASSET_TYPES_SERVICE_ID: &str = "asset.types.api";
pub const ASSET_TYPES_BACKEND_CAPABILITY_ID: &str = "assets.types.backend";

pub const ASSET_TYPES_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets.types",
        ENGINE_ASSET_TYPES_SERVICE_ID,
        ASSET_TYPES_SERVICE_ID,
        ASSET_TYPES_BACKEND_CAPABILITY_ID,
    );

pub mod file_type_method {
    pub const INFO_JSON: &str = newviso_service_api::SERVICE_METHOD_INFO_JSON;
    pub const INVOKE_JSON: &str = newviso_service_api::SERVICE_METHOD_INVOKE_JSON;
    pub const SHUTDOWN_V1: &str = newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1;
    pub const MANIFEST_JSON_V1: &str = "asset.types.manifest_json_v1";
    pub const REGISTER_JSON_V1: &str = "asset.types.register_json_v1";
    pub const PROBE_JSON_V1: &str = "asset.types.probe_json_v1";
    pub const RESOLVE_JSON_V1: &str = "asset.types.resolve_json_v1";
}

pub const ASSET_TYPES_SERVICE_METHODS: &[&str] = &[
    file_type_method::INFO_JSON,
    file_type_method::INVOKE_JSON,
    file_type_method::SHUTDOWN_V1,
    file_type_method::MANIFEST_JSON_V1,
    file_type_method::REGISTER_JSON_V1,
    file_type_method::PROBE_JSON_V1,
    file_type_method::RESOLVE_JSON_V1,
];

pub const ASSET_TYPES_RUNTIME_CONTRACT_SPEC: newviso_service_api::RuntimeServiceContractSpec =
    newviso_service_api::RuntimeServiceContractSpec::new(
        ENGINE_ASSET_TYPES_SERVICE_ID,
        "newengine.assets-types-api >= 0.1.x",
        ASSET_TYPES_SERVICE_METHODS,
    );

pub const ASSET_TYPES_RUNTIME_REQUIREMENT_SPEC:
    newviso_service_api::RuntimeServiceRequirementSpec =
    newviso_service_api::RuntimeServiceRequirementSpec::new(
        ASSET_TYPES_RUNTIME_CONTRACT_SPEC,
        Some(ASSET_TYPES_BACKEND_CAPABILITY_ID),
        Some("NEWENGINE_REQUIRE_ASSET_TYPES"),
    );
