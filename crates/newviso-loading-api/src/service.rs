use serde::{Deserialize, Serialize};

/// Engine-facing loading-screen gateway id.
///
/// Runtime/platform code calls this stable facade. The current implementation is
/// an engine-runtime loading data bridge. UI presentation is rendered only by
/// `engine.ui` providers; `engine.ui.loading` is data/status, not a renderer.
pub const ENGINE_LOADING_SERVICE_ID: &str = "engine.ui.loading";

/// First-party provider service id for loading shell providers.
pub const LOADING_SERVICE_ID: &str = "loading.api";
pub const LOADING_BACKEND_CAPABILITY_ID: &str = "loading.backend";

pub const LOADING_SERVICE_METHOD_INFO: &str = newviso_service_api::SERVICE_METHOD_INFO_JSON;
pub const LOADING_SERVICE_METHOD_INVOKE: &str = newviso_service_api::SERVICE_METHOD_INVOKE_JSON;
pub const LOADING_SERVICE_METHOD_SHUTDOWN_V1: &str =
    newviso_service_api::SERVICE_METHOD_SHUTDOWN_V1;
pub const LOADING_SERVICE_METHOD_SNAPSHOT_JSON_V1: &str = "snapshot_json_v1";
pub const LOADING_SERVICE_METHOD_PUBLISH_JSON_V1: &str = "publish_json_v1";
pub const LOADING_SERVICE_METHOD_PUBLISH_STATUS_JSON_V1: &str = "publish_status_json_v1";
pub const ENGINE_LOADING_STATUS_TOPIC_V1: &str = "engine.ui.loading.status.v1";

pub const LOADING_REQUIRED_METHODS_V1: &[&str] = &[
    LOADING_SERVICE_METHOD_INFO,
    LOADING_SERVICE_METHOD_INVOKE,
    LOADING_SERVICE_METHOD_SHUTDOWN_V1,
    LOADING_SERVICE_METHOD_SNAPSHOT_JSON_V1,
];

pub const LOADING_BACKEND_SERVICE_SPEC: newviso_service_api::BackendServiceSpec =
    newviso_service_api::BackendServiceSpec::new(
        "loading",
        ENGINE_LOADING_SERVICE_ID,
        LOADING_SERVICE_ID,
        LOADING_BACKEND_CAPABILITY_ID,
    );

pub const LOADING_RUNTIME_CONTRACT_SPEC: newviso_service_api::RuntimeServiceContractSpec =
    newviso_service_api::RuntimeServiceContractSpec::new(
        ENGINE_LOADING_SERVICE_ID,
        "newengine.ui.loading-api >= 0.1.x",
        LOADING_REQUIRED_METHODS_V1,
    );

/// Loading is an always-helpful diagnostic domain, but it must not make headless
/// or test runs fatal unless a strict profile explicitly requires it.
pub const LOADING_RUNTIME_REQUIREMENT_SPEC: newviso_service_api::RuntimeServiceRequirementSpec =
    newviso_service_api::RuntimeServiceRequirementSpec::new(
        LOADING_RUNTIME_CONTRACT_SPEC,
        Some(LOADING_BACKEND_CAPABILITY_ID),
        Some("NEWENGINE_REQUIRE_LOADING_BACKEND"),
    );

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadingServiceInfo {
    pub protocol: String,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub methods: Vec<String>,
}

impl Default for LoadingServiceInfo {
    #[inline]
    fn default() -> Self {
        Self {
            protocol: "newengine.ui.loading-api/v1".to_owned(),
            features: vec![
                "engine.ui.loading-projection".to_owned(),
                "shared-snapshot".to_owned(),
                "independent-visual-clock".to_owned(),
                "subsystem-stage-projection".to_owned(),
                "task-event-projection".to_owned(),
                "cooperative-task-control".to_owned(),
            ],
            methods: LOADING_REQUIRED_METHODS_V1
                .iter()
                .map(|it| (*it).to_owned())
                .chain(std::iter::once(
                    LOADING_SERVICE_METHOD_PUBLISH_JSON_V1.to_owned(),
                ))
                .chain(std::iter::once(
                    LOADING_SERVICE_METHOD_PUBLISH_STATUS_JSON_V1.to_owned(),
                ))
                .collect(),
        }
    }
}
