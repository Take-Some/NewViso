/// Engine-facing asset service gateway id.
///
/// Runtime and provider plugins must request assets through this stable host-owned
/// gateway, not through a concrete AssetManager/provider service id. The host
/// resolves this gateway to the active asset backend by declared capability.
pub const ENGINE_ASSET_SERVICE_ID: &str = newviso_service_api::ENGINE_ASSETS_GATEWAY_ID;

/// Canonical client-facing service id for asset access.
pub const ASSET_SERVICE_ID: &str = ENGINE_ASSET_SERVICE_ID;

/// Default provider service id used by the first-party AssetManager backend.
///
/// This is provider-owned identity, not the id consumers should call. Third-party
/// providers may register a different service id as long as they declare
/// `asset_manager.backend`; the engine gateway still resolves them.
pub const ASSET_PROVIDER_SERVICE_ID: &str = "asset_manager.api";

/// Backend capability declared by plugins that provide an asset service backend.
pub const ASSET_BACKEND_CAPABILITY_ID: &str = "asset_manager.backend";

/// Wire method namespace for asset-domain service calls.
pub const ASSET_METHOD_PREFIX: &str = "asset.";

/// Canonical provider-neutral VFS selection rule. Providers may implement the
/// storage and codecs differently, but a compiled candidate for one logical
/// asset id must be considered before its authoring-source candidate.
pub const ASSET_RESOLUTION_POLICY_COMPILED_FIRST_SOURCE_FALLBACK_V1: &str =
    "compiled_first_source_fallback.v1";

/// Stable JSON values accepted by asset.mount_source_json_v1 in asset_role.
pub mod asset_source_role {
    pub const COMPILED: &str = "compiled";
    pub const RUNTIME: &str = "runtime";
    pub const SOURCE: &str = "source";
}

/// Stable semantic texture domain id used by `.ytd` format descriptors.
/// Runtime texture bytes are decoded through `engine.assets` `asset.decode_v1`; there is no
/// standalone `engine.assets.textures` provider or backend capability.
pub const ENGINE_ASSETS_TEXTURES_SERVICE_ID: &str = "engine.assets.textures";
/// Semantic definition/archetype metadata gateway id. File-type descriptors route
/// `.ytyp` meaning here; scene/world systems may consume definitions later, but
/// do not own the file type.
pub const ENGINE_ASSETS_DEFINITIONS_SERVICE_ID: &str =
    newviso_service_api::ENGINE_ASSETS_DEFINITIONS_GATEWAY_ID;
pub const DEFINITIONS_SERVICE_ID: &str = "definitions.api";
pub const DEFINITIONS_BACKEND_CAPABILITY_ID: &str = "assets.definitions.backend";
pub const DEFINITIONS_RUNTIME_CONTRACT: &str = "newengine.assets.definitions.runtime.v1";
pub const ENGINE_ASSETS_MODELS_SERVICE_ID: &str =
    newviso_service_api::ENGINE_ASSETS_MODELS_GATEWAY_ID;
pub const ENGINE_ASSETS_MATERIALS_SERVICE_ID: &str =
    newviso_service_api::ENGINE_ASSETS_MATERIALS_GATEWAY_ID;
/// Semantic authored map/world placement gateway id. `.ymap` owns map composition,
/// placements and references to `.ytyp` Definition Entries; it does not replace
/// `.ytyp` as the generic metadata/knowledge source.
pub const ENGINE_ASSETS_MAPS_SERVICE_ID: &str =
    newviso_service_api::ENGINE_ASSETS_MAPS_GATEWAY_ID;
pub const MAPS_SERVICE_ID: &str = "maps.api";
pub const MAPS_BACKEND_CAPABILITY_ID: &str = "assets.maps.backend";
pub const MAPS_RUNTIME_CONTRACT: &str = "newengine.assets.maps.runtime.v1";

/// Semantic UI dictionary gateway id. `.neui` meaning lives here: XMLcentral validation,
/// entry selection, binding/action/dependency extraction and authored-to-runtime DTO
/// compilation. `engine.assets` remains byte/VFS/codec owner; `engine.ui` remains live runtime.
pub const ENGINE_ASSETS_UI_SERVICE_ID: &str = newviso_service_api::ENGINE_ASSETS_UI_GATEWAY_ID;
pub const ASSETS_UI_SERVICE_ID: &str = "assets.ui.api";
pub const ASSETS_UI_BACKEND_CAPABILITY_ID: &str = "assets.ui.backend";
pub const ASSETS_UI_RUNTIME_CONTRACT: &str = "newengine.assets.ui.runtime.v1";

/// Runtime scene gateway. It consumes resolved map/definition DTOs and mutates the world; it does not own authored map file semantics.
pub const ENGINE_SCENE_SERVICE_ID: &str = newviso_service_api::ENGINE_SCENE_GATEWAY_ID;

/// Runtime scripting gateway. `.ysc` script modules are opaque to core and
/// are routed through this domain; AssetManager still owns VFS bytes and ListFile codec dispatch.
pub const ENGINE_SCRIPTING_SERVICE_ID: &str = newviso_service_api::ENGINE_SCRIPTING_GATEWAY_ID;

/// Semantic asset graph gateway id. This resolver owns declarative dependency
/// graph expansion over .ytyp/.ydd/.nemat/.ytd refs; it uses engine.assets only
/// for VFS bytes and codec dispatch.
pub const ENGINE_ASSETS_GRAPH_SERVICE_ID: &str =
    newviso_service_api::ENGINE_ASSETS_GRAPH_GATEWAY_ID;
pub const ASSET_GRAPH_SERVICE_ID: &str = "asset_graph.api";
pub const ASSET_GRAPH_BACKEND_CAPABILITY_ID: &str = "assets.graph.backend";
pub const ASSET_GRAPH_RUNTIME_CONTRACT: &str = "newengine.assets.graph.runtime.v1";

/// Editor/import lifecycle sub-gateways and capability ids.
///
/// These are Godot-inspired lifecycle surfaces, but they do not adopt Godot's
/// `.tres/.res` resource model. `engine.assets` remains the byte/VFS/codec host;
/// these sub-gateways expose inspectable editor/import read-model slices.
pub const ENGINE_ASSETS_UID_SERVICE_ID: &str = newviso_service_api::ENGINE_ASSETS_UID_GATEWAY_ID;
pub const ASSETS_UID_SERVICE_ID: &str = "assets.uid.api";
pub const ASSETS_UID_BACKEND_CAPABILITY_ID: &str = "assets.uid.backend";
pub const ASSETS_UID_RUNTIME_CONTRACT: &str = "newengine.assets.uid.v1";

pub const ENGINE_ASSETS_DEPENDENCIES_SERVICE_ID: &str =
    newviso_service_api::ENGINE_ASSETS_DEPENDENCIES_GATEWAY_ID;
pub const ASSETS_DEPENDENCIES_SERVICE_ID: &str = "assets.dependencies.api";
pub const ASSETS_DEPENDENCIES_BACKEND_CAPABILITY_ID: &str = "assets.dependencies.backend";
pub const ASSETS_DEPENDENCIES_RUNTIME_CONTRACT: &str = "newengine.assets.dependencies.v1";

pub const ENGINE_ASSETS_IMPORT_QUEUE_SERVICE_ID: &str =
    newviso_service_api::ENGINE_ASSETS_IMPORT_QUEUE_GATEWAY_ID;
pub const ASSETS_IMPORT_QUEUE_SERVICE_ID: &str = "assets.import_queue.api";
pub const ASSETS_IMPORT_QUEUE_BACKEND_CAPABILITY_ID: &str = "assets.import_queue.backend";
pub const ASSETS_IMPORT_QUEUE_RUNTIME_CONTRACT: &str = "newengine.assets.import_queue.v1";

pub const ENGINE_ASSETS_PACKAGE_WRITER_SERVICE_ID: &str =
    newviso_service_api::ENGINE_ASSETS_PACKAGE_WRITER_GATEWAY_ID;
pub const ASSETS_PACKAGE_WRITER_SERVICE_ID: &str = "assets.package_writer.api";
pub const ASSETS_PACKAGE_WRITER_CAPABILITY_ID: &str = "assets.package_writer";
pub const ASSETS_PACKAGE_WRITER_RUNTIME_CONTRACT: &str = "newengine.assets.package_writer.v1";

pub const ASSETS_REIMPORT_CAPABILITY_ID: &str = "assets.reimport";
pub const ASSETS_THUMBNAIL_CAPABILITY_ID: &str = "assets.thumbnail";
pub const ASSETS_DIRTY_SCAN_CAPABILITY_ID: &str = "assets.dirty_scan";
pub const ASSETS_IMPORT_LIFECYCLE_CAPABILITY_ID: &str = "assets.import_lifecycle";

pub const ASSETS_UID_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets.uid",
        ENGINE_ASSETS_UID_SERVICE_ID,
        ASSET_PROVIDER_SERVICE_ID,
        ASSETS_UID_BACKEND_CAPABILITY_ID,
    );

pub const ASSETS_DEPENDENCIES_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets.dependencies",
        ENGINE_ASSETS_DEPENDENCIES_SERVICE_ID,
        ASSET_PROVIDER_SERVICE_ID,
        ASSETS_DEPENDENCIES_BACKEND_CAPABILITY_ID,
    );

pub const ASSETS_IMPORT_QUEUE_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets.import_queue",
        ENGINE_ASSETS_IMPORT_QUEUE_SERVICE_ID,
        ASSET_PROVIDER_SERVICE_ID,
        ASSETS_IMPORT_QUEUE_BACKEND_CAPABILITY_ID,
    );

pub const ASSETS_PACKAGE_WRITER_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets.package_writer",
        ENGINE_ASSETS_PACKAGE_WRITER_SERVICE_ID,
        ASSET_PROVIDER_SERVICE_ID,
        ASSETS_PACKAGE_WRITER_CAPABILITY_ID,
    );

/// Asset streaming gateway and capability ids.
///
/// Streaming is a first-class diagnostics/jobs-visible domain, not a hidden
/// loader thread behind AssetManager. Providers should expose request queues,
/// residency, defragmentation and cache loading through this capability family.
pub const ENGINE_ASSETS_STREAMING_SERVICE_ID: &str = "engine.assets.streaming";
pub const ASSETS_STREAMING_SERVICE_ID: &str = "assets.streaming.api";
pub const ASSETS_STREAMING_BACKEND_CAPABILITY_ID: &str = "assets.streaming.backend";
pub const ASSETS_STREAMING_REQUEST_QUEUE_CAPABILITY_ID: &str = "assets.streaming.request_queue";
pub const ASSETS_STREAMING_RESIDENCY_CAPABILITY_ID: &str = "assets.streaming.residency";
pub const ASSETS_STREAMING_DEFRAG_CAPABILITY_ID: &str = "assets.streaming.defrag";
pub const ASSETS_STREAMING_CACHE_LOADER_CAPABILITY_ID: &str = "assets.streaming.cache_loader";
// `engine.assets.streaming` is currently a child gateway routed to the same
// `asset_manager.api` provider service as the root `engine.assets` gateway. Runtime
// contract validation therefore checks the provider service family here; the
// streaming-specific surface is enforced by `ASSETS_STREAMING_SERVICE_METHODS` and
// `assets.streaming.backend`. A dedicated future streaming provider may publish its
// own exact runtime contract family once it owns a distinct provider service.
pub const ASSETS_STREAMING_RUNTIME_CONTRACT: &str = "newengine.assets-api >= 0.8.x";

/// Concrete provider routing for the global asset residency controller.
/// The provider service remains `asset_manager.api`; this sub-gateway is the
/// ownership boundary for request/pin/residency/cleanup policy.
pub const ASSETS_STREAMING_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "assets.streaming",
        ENGINE_ASSETS_STREAMING_SERVICE_ID,
        ASSET_PROVIDER_SERVICE_ID,
        ASSETS_STREAMING_BACKEND_CAPABILITY_ID,
    );

/// World streaming gateway and capability ids.
///
/// World streaming owns cell visibility and spatial budget decisions. It may
/// request asset streaming work, but all loader/visibility jobs must remain on
/// the engine job system and visible in diagnostics/profiler.
pub const ENGINE_WORLD_STREAMING_SERVICE_ID: &str = "engine.world.streaming";
pub const WORLD_STREAMING_SERVICE_ID: &str = "world.streaming.api";
pub const WORLD_STREAMING_BACKEND_CAPABILITY_ID: &str = "world.streaming.backend";
pub const WORLD_STREAMING_CELLS_CAPABILITY_ID: &str = "world.streaming.cells";
pub const WORLD_STREAMING_VISIBILITY_BUDGET_CAPABILITY_ID: &str =
    "world.streaming.visibility_budget";
pub const WORLD_STREAMING_RUNTIME_CONTRACT: &str = "newengine.world.streaming.runtime.v1";

pub mod asset_graph_method {
    pub const RESOLVE_V1: &str = "assets.graph.resolve_v1";
    pub const VALIDATE_V1: &str = "assets.graph.validate_v1";
    pub const DUMP_JSON_V1: &str = "assets.graph.dump_json_v1";
}

pub const ASSET_GRAPH_SERVICE_METHODS: &[&str] = &[
    newviso_service_api::SERVICE_METHOD_INFO_JSON,
    newviso_service_api::SERVICE_METHOD_INVOKE_JSON,
    newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1,
    asset_graph_method::RESOLVE_V1,
    asset_graph_method::VALIDATE_V1,
    asset_graph_method::DUMP_JSON_V1,
];

pub mod assets_ui_method {
    pub const INFO_JSON: &str = newviso_service_api::SERVICE_METHOD_INFO_JSON;
    pub const INVOKE_JSON: &str = newviso_service_api::SERVICE_METHOD_INVOKE_JSON;
    pub const SHUTDOWN_V1: &str = newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1;
    pub const MANIFEST_V1: &str = "assets.ui.manifest_v1";
    pub const REGISTRY_V1: &str = "assets.ui.registry_v1";
    pub const ENTRY_V1: &str = "assets.ui.entry_v1";
    pub const DOCUMENT_V1: &str = "assets.ui.document_v1";
    pub const COMPILE_DOCUMENT_V1: &str = "assets.ui.compile_document_v1";
    pub const BINDING_PLAN_V1: &str = "assets.ui.binding_plan_v1";
    pub const VALIDATE_V1: &str = "assets.ui.validate_v1";
    pub const DEPENDENCIES_V1: &str = "assets.ui.dependencies_v1";
    pub const DUMP_XMLCENTRAL_V1: &str = "assets.ui.dump_xmlcentral_v1";
    pub const INSPECT_DIALECT_V1: &str = "assets.ui.inspect_dialect_v1";
    pub const INVALIDATE_V1: &str = "assets.ui.invalidate_v1";
}

pub const ASSETS_UI_SERVICE_METHODS: &[&str] = &[
    assets_ui_method::INFO_JSON,
    assets_ui_method::INVOKE_JSON,
    assets_ui_method::SHUTDOWN_V1,
    assets_ui_method::MANIFEST_V1,
    assets_ui_method::REGISTRY_V1,
    assets_ui_method::ENTRY_V1,
    assets_ui_method::DOCUMENT_V1,
    assets_ui_method::COMPILE_DOCUMENT_V1,
    assets_ui_method::BINDING_PLAN_V1,
    assets_ui_method::VALIDATE_V1,
    assets_ui_method::DEPENDENCIES_V1,
    assets_ui_method::DUMP_XMLCENTRAL_V1,
    assets_ui_method::INSPECT_DIALECT_V1,
    assets_ui_method::INVALIDATE_V1,
];
