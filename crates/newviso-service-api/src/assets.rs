//! Canonical NewViso asset gateway surface.
//!
//! This is the host/client contract, independent of a specific backend DLL.
//! StarVault may satisfy it through a compatibility service; new providers
//! should publish the same method set under the `engine.assets` gateway.
use std::collections::BTreeSet;

pub const ASSET_GATEWAY_ID: &str = crate::ENGINE_ASSETS_GATEWAY_ID;
pub const ASSET_TYPES_GATEWAY_ID: &str = crate::ENGINE_ASSETS_TYPES_GATEWAY_ID;

pub mod method {
    pub const MOUNT_SOURCE_JSON_V1: &str = "asset.mount_source_json_v1";
    pub const VFS_LIST_JSON_V1: &str = "asset.vfs_list_json_v1";
    pub const TEXT_V1: &str = "asset.text_v1";
    pub const RAW_BYTES_V1: &str = "asset.raw_bytes_v1";
    pub const DECODE_V1: &str = "asset.decode_v1";
    pub const FORMATS_JSON_V1: &str = "asset.formats_json_v1";
    pub const SOURCES_JSON_V1: &str = "asset.sources_json_v1";
    pub const TYPES_RESOLVE_JSON_V1: &str = "asset.types.resolve_json_v1";
    pub const TYPES_REGISTER_JSON_V1: &str = "asset.types.register_json_v1";
}

/// Minimum feature surface required to boot and render a NewViso project.
pub const REQUIRED_RUNTIME_ASSET_METHODS: &[&str] = &[
    method::MOUNT_SOURCE_JSON_V1,
    method::VFS_LIST_JSON_V1,
    method::TEXT_V1,
    method::RAW_BYTES_V1,
    method::DECODE_V1,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetGatewayContract {
    pub provider_service_id: String,
    pub protocol: String,
    pub method_count: usize,
}

/// Reject incompatible or incomplete service surfaces before mounting a scene.
/// This is a contract check, not a provider/vendor or DLL filename check.
pub fn validate_asset_gateway_description(
    description: &serde_json::Value,
) -> Result<AssetGatewayContract, String> {
    let id = description
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "asset provider has no service id".to_owned())?;
    let protocol = description
        .get("contract")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|protocol| !protocol.is_empty())
        .ok_or_else(|| "asset provider has no service contract".to_owned())?;
    let declarations = description
        .get("methods")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "asset provider has no declared method set".to_owned())?;
    let methods: BTreeSet<&str> = declarations
        .iter()
        .filter_map(|entry| entry.get("name").and_then(serde_json::Value::as_str))
        .collect();
    let missing: Vec<_> = REQUIRED_RUNTIME_ASSET_METHODS
        .iter()
        .copied()
        .filter(|method| !methods.contains(method))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "asset provider '{}' contract '{}' lacks required runtime methods: {}",
            id,
            protocol,
            missing.join(", ")
        ));
    }
    Ok(AssetGatewayContract {
        provider_service_id: id.to_owned(),
        protocol: protocol.to_owned(),
        method_count: methods.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_and_compatible_backend_descriptions_are_accepted() {
        for contract in ["newengine.assets-api.v1", "newviso.assets-api.v1"] {
            let methods = REQUIRED_RUNTIME_ASSET_METHODS
                .iter()
                .map(|name| serde_json::json!({"name": name}))
                .collect::<Vec<_>>();
            let value = serde_json::json!({
                "id": "asset_manager.api",
                "contract": contract,
                "methods": methods
            });
            let validated = validate_asset_gateway_description(&value).unwrap();
            assert_eq!(validated.method_count, REQUIRED_RUNTIME_ASSET_METHODS.len());
        }
    }

    #[test]
    fn missing_runtime_decode_method_is_rejected() {
        let value = serde_json::json!({
            "id": "asset_manager.api",
            "contract": "newviso.assets-api.v1",
            "methods": [{"name": method::RAW_BYTES_V1}]
        });
        let error = validate_asset_gateway_description(&value).unwrap_err();
        assert!(error.contains(method::DECODE_V1));
        assert!(error.contains(method::MOUNT_SOURCE_JSON_V1));
    }
}
