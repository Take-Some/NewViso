use newviso_host as host;
use serde_json::{json, Value};
use std::path::Path;

const ASSET_SERVICE: &str = "engine.assets";
const MOUNT_METHOD: &str = "asset.mount_source_json_v1";
const TEXT_METHOD: &str = "asset.text_v1";
const RAW_BYTES_METHOD: &str = "asset.raw_bytes_v1";

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
