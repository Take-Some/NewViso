/// Generic JSON-control service method names shared by host and providers.
///
/// Domain API crates may re-export these names, but the literals are owned here
/// so host-side adapters and plugin services do not drift.
pub mod standard_method {
    /// Returns domain-specific backend/provider metadata as JSON.
    pub const INFO_JSON: &str = "info_json";

    /// Invokes a domain-specific JSON request envelope and returns a JSON response envelope.
    pub const INVOKE_JSON: &str = "invoke_json";

    /// Optional explicit shutdown hook called before service unregister/drop.
    pub const SHUTDOWN_V1: &str = "shutdown_v1";
}

pub const SERVICE_METHOD_INFO_JSON: &str = standard_method::INFO_JSON;
pub const SERVICE_METHOD_INVOKE_JSON: &str = standard_method::INVOKE_JSON;
pub const SERVICE_METHOD_SHUTDOWN_V1: &str = standard_method::SHUTDOWN_V1;

/// Required method set for backend services that use the common JSON-control
/// transport: `info_json`, `invoke_json`, `shutdown_v1`.
pub const JSON_CONTROL_SERVICE_METHODS_V1: &[&str] = &[
    SERVICE_METHOD_INFO_JSON,
    SERVICE_METHOD_INVOKE_JSON,
    SERVICE_METHOD_SHUTDOWN_V1,
];
