/// Engine-facing render service gateway id. Consumers call this facade; the host
/// resolves it to the active renderer provider service by descriptor metadata.
pub const ENGINE_RENDER_SERVICE_ID: &str = newviso_service_api::ENGINE_RENDER_GATEWAY_ID;
/// Third-level render domain for post-process/effect stack providers.
pub const ENGINE_RENDER_EFFECTS_SERVICE_ID: &str =
    newviso_service_api::ENGINE_RENDER_EFFECTS_GATEWAY_ID;
/// Third-level render domain for material system providers.
pub const ENGINE_RENDER_MATERIALS_SERVICE_ID: &str =
    newviso_service_api::ENGINE_RENDER_MATERIALS_GATEWAY_ID;

/// Default/first-party provider service id for render backends.
pub const RENDER_SERVICE_ID: &str = "render.api";
pub const RENDER_BACKEND_CAPABILITY_ID: &str = "render.backend";
pub const RENDER_PROVIDER_ABI_VERSION: u16 = 2;
pub const RENDER_PROVIDER_ABI_ID: &str = "newengine.render-provider/v2";
pub const RENDER_PROVIDER_ABI_CONTRACT_SPEC: newviso_contract_api::ContractSpec =
    newviso_contract_api::ContractSpec::new(
        "render.provider.abi",
        newviso_contract_api::ContractKind::Abi,
        newviso_contract_api::ContractVersion::major(RENDER_PROVIDER_ABI_VERSION),
        newviso_contract_api::ContractCompatibility::Exact,
        "newengine-render-api",
        Some(RENDER_PROVIDER_ABI_ID),
    );
pub const RENDER_EFFECTS_SERVICE_ID: &str = "render.effects.api";
pub const RENDER_EFFECTS_BACKEND_CAPABILITY_ID: &str = "render.effects.backend";
pub const RENDER_MATERIALS_SERVICE_ID: &str = "render.materials.api";
pub const RENDER_MATERIALS_BACKEND_CAPABILITY_ID: &str = "render.materials.backend";
pub const RENDER_SERVICE_METHOD_INVOKE: &str = newviso_service_api::SERVICE_METHOD_INVOKE_JSON;
pub const RENDER_SERVICE_METHOD_INFO: &str = newviso_service_api::SERVICE_METHOD_INFO_JSON;
pub const RENDER_SERVICE_METHOD_SHUTDOWN_V1: &str = newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1;
/// Binary hot-path method for frame-local unit render commands.
///
/// The regular invoke_json surface remains the typed control protocol. This
/// method is intentionally narrow: it carries only unit commands such as
/// write_buffer/set_pipeline/draw so draw-list extraction does not serialize
/// byte payloads as JSON arrays on every frame.
pub const RENDER_SERVICE_METHOD_COMMAND_BATCH_BIN_V2: &str = "command_batch_bin_v2";
/// Binary allocation path for texture descriptors carrying large mip payloads.
/// This avoids JSON byte-array expansion on the render thread.
pub const RENDER_SERVICE_METHOD_CREATE_TEXTURE_BIN_V1: &str = "create_texture_bin_v1";
/// Host-staged independent multi-adapter vertex transcode. The method is served
/// outside the primary renderer mutex so GPU1/GPU2 can execute concurrently with
/// GPU0 presentation and upload work.
pub const RENDER_SERVICE_METHOD_MULTI_ADAPTER_MESH_TRANSCODE_BIN_V1: &str =
    "multi_adapter_mesh_transcode_bin_v1";

/// Renderer diagnostics surfaces. These are JSON control-plane dumps, not frame hot-path commands.
pub const RENDER_SERVICE_METHOD_DUMP_PHASE_GRAPH_V1: &str = "engine.render.dump_phase_graph_v1";
pub const RENDER_SERVICE_METHOD_DUMP_RESOURCE_LIFETIME_V1: &str =
    "engine.render.dump_resource_lifetime_v1";
pub const RENDER_SERVICE_METHOD_DUMP_SHADER_CACHE_V1: &str = "engine.render.dump_shader_cache_v1";
pub const RENDER_SERVICE_METHOD_DUMP_EFFECT_GRAPH_V1: &str = "engine.render.dump_effect_graph_v1";

/// Generic backend-family declaration for render providers.
pub const RENDER_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "render",
        ENGINE_RENDER_SERVICE_ID,
        RENDER_SERVICE_ID,
        RENDER_BACKEND_CAPABILITY_ID,
    );

pub const RENDER_EFFECTS_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "render.effects",
        ENGINE_RENDER_EFFECTS_SERVICE_ID,
        RENDER_EFFECTS_SERVICE_ID,
        RENDER_EFFECTS_BACKEND_CAPABILITY_ID,
    );

pub const RENDER_MATERIALS_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "render.materials",
        ENGINE_RENDER_MATERIALS_SERVICE_ID,
        RENDER_MATERIALS_SERVICE_ID,
        RENDER_MATERIALS_BACKEND_CAPABILITY_ID,
    );

/// Startup validation contract for the engine-facing render gateway.
pub const RENDER_RUNTIME_CONTRACT_SPEC: newviso_service_api::RuntimeServiceContractSpec =
    newviso_service_api::RuntimeServiceContractSpec::new(
        ENGINE_RENDER_SERVICE_ID,
        "newengine.render-api >= 0.3.x",
        newviso_service_api::JSON_CONTROL_SERVICE_METHODS_V1,
    );

/// Declarative startup requirement for render. Missing render degrades unless
/// the explicit env switch is enabled by a strict test/runtime profile.
pub const RENDER_RUNTIME_REQUIREMENT_SPEC: newviso_service_api::RuntimeServiceRequirementSpec =
    newviso_service_api::RuntimeServiceRequirementSpec::new(
        RENDER_RUNTIME_CONTRACT_SPEC,
        Some(RENDER_BACKEND_CAPABILITY_ID),
        Some("NEWENGINE_REQUIRE_RENDER_BACKEND"),
    );

pub type Color4 = [f32; 4];
pub type RenderWireResult<T> = Result<T, String>;
