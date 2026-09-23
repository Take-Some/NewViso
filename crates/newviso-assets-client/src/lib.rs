use newviso_host as host;
use serde_json::{json, Value};
use std::path::Path;

const ASSET_SERVICE: &str = "engine.assets";
const MOUNT_METHOD: &str = "asset.mount_source_json_v1";
const TEXT_METHOD: &str = "asset.text_v1";
const RAW_BYTES_METHOD: &str = "asset.raw_bytes_v1";
const VFS_LIST_METHOD: &str = "asset.vfs_list_json_v1";
const DECODE_METHOD: &str = "asset.decode_v1";

#[derive(Clone, Copy, Debug, Default)]
pub struct AssetClient;

impl AssetClient {
    pub const fn new() -> Self {
        Self
    }

    pub fn mount_filesystem(&self, root: &Path, mount: &str, priority: i32) -> Result<(), String> {
        let response = host::call_json(
            ASSET_SERVICE,
            MOUNT_METHOD,
            &json!({
                "kind": "filesystem",
                "priority": priority,
                "mount": mount,
                "config": {
                    "root": root.to_string_lossy()
                }
            }),
        )?;

        if response.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(())
        } else {
            Err(format!(
                "filesystem VFS mount rejected root='{}' mount='{}' priority={}: {}",
                root.display(),
                mount,
                priority,
                response
            ))
        }
    }

    pub fn vfs_list(&self, logical_dir: &str) -> Result<Value, String> {
        let path = normalize_logical_dir(logical_dir)?;
        let bytes = host::call_service(ASSET_SERVICE, VFS_LIST_METHOD, path.as_bytes())?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("asset VFS list returned invalid JSON: {error}"))?;

        if value.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(value)
        } else {
            Err(format!("asset VFS list failed path='{path}': {value}"))
        }
    }

    pub fn text(&self, logical_path: &str) -> Result<String, String> {
        let path = normalize_logical_path(logical_path)?;
        let bytes = host::call_service(ASSET_SERVICE, TEXT_METHOD, path.as_bytes())?;
        String::from_utf8(bytes)
            .map_err(|error| format!("asset '{}' is not valid UTF-8: {error}", path))
    }

    pub fn json(&self, logical_path: &str) -> Result<Value, String> {
        let text = self.text(logical_path)?;
        serde_json::from_str(&text)
            .map_err(|error| format!("invalid JSON asset '{}': {error}", logical_path))
    }

    pub fn decode(
        &self,
        logical_path: &str,
        output_kind: &str,
        selector: Value,
    ) -> Result<Vec<u8>, String> {
        let path = normalize_logical_path(logical_path)?;
        let request = json!({
            "logical_path": path,
            "output_kind": output_kind,
            "selector": selector
        });
        let payload = serde_json::to_vec(&request)
            .map_err(|error| format!("asset decode request encode failed: {error}"))?;
        host::call_service(ASSET_SERVICE, DECODE_METHOD, &payload).map_err(|error| {
            format!(
                "asset decode failed path='{}' output='{}': {error}",
                logical_path, output_kind
            )
        })
    }

    pub fn decode_json(
        &self,
        logical_path: &str,
        output_kind: &str,
        selector: Value,
    ) -> Result<Value, String> {
        let bytes = self.decode(logical_path, output_kind, selector)?;
        serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "asset semantic output is not valid JSON path='{}' output='{}': {error}",
                logical_path, output_kind
            )
        })
    }

    pub fn raw_bytes(&self, logical_path: &str) -> Result<Vec<u8>, String> {
        let path = normalize_logical_path(logical_path)?;
        host::call_service(ASSET_SERVICE, RAW_BYTES_METHOD, path.as_bytes())
            .map_err(|error| format!("failed to read asset '{}': {error}", path))
    }
}

pub fn normalize_logical_path(path: &str) -> Result<String, String> {
    let mut normalized = path.trim().replace('\\', "/");
    while let Some(rest) = normalized.strip_prefix("./") {
        normalized = rest.to_owned();
    }
    normalized = normalized.trim_start_matches('/').to_owned();

    if normalized.is_empty() {
        return Err("logical asset path is empty".to_owned());
    }
    if normalized.split('/').any(|part| part == "..") {
        return Err(format!(
            "logical asset path escapes project root: '{normalized}'"
        ));
    }

    Ok(normalized)
}

pub fn normalize_logical_dir(path: &str) -> Result<String, String> {
    let mut normalized = path.trim().replace('\\', "/");
    while let Some(rest) = normalized.strip_prefix("./") {
        normalized = rest.to_owned();
    }
    normalized = normalized.trim_matches('/').to_owned();

    if normalized.split('/').any(|part| part == "..") {
        return Err(format!(
            "logical asset directory escapes project root: '{normalized}'"
        ));
    }

    Ok(normalized)
}
