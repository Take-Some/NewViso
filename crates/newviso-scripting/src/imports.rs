//! Import graph loading through engine.assets/VFS, never provider filesystem access.
use newviso_assets_client::AssetClient;
use newviso_script_client::{ScriptClient, ScriptModuleLoad, ScriptPermission};
use std::collections::{BTreeMap, HashSet};

const MAX_GRAPH_DEPTH: usize = 48;
const MAX_IMPORTS_PER_MODULE: usize = 128;
const MISSING_IMPORT: &str = "TYPESCRIPT_SCRIPTING_IMPORT_NOT_LOADED:";

pub(super) fn load_module_graph(
    assets: &AssetClient,
    scripting: &ScriptClient,
    asset: &str,
    permissions: &[ScriptPermission],
    metadata: BTreeMap<String, String>,
) -> Result<(), String> {
    load_recursive(
        assets,
        scripting,
        asset,
        permissions,
        &metadata,
        &mut HashSet::new(),
        &mut Vec::new(),
    )
}

fn load_recursive(
    assets: &AssetClient,
    scripting: &ScriptClient,
    asset: &str,
    permissions: &[ScriptPermission],
    metadata: &BTreeMap<String, String>,
    loaded: &mut HashSet<String>,
    stack: &mut Vec<String>,
) -> Result<(), String> {
    let key = asset.to_ascii_lowercase();
    if loaded.contains(&key) {
        return Ok(());
    }
    if stack.iter().any(|item| item == &key) {
        return Err(format!(
            "circular .ysc import graph is not yet supported: {} -> {asset}",
            stack.join(" -> ")
        ));
    }
    if stack.len() >= MAX_GRAPH_DEPTH {
        return Err("YSC import graph exceeds maximum depth".into());
    }
    stack.push(key.clone());
    let result = (|| {
        let bytes = assets.raw_bytes(asset)?;
        let mut attempted = HashSet::new();
        loop {
            let mut record_metadata = metadata.clone();
            record_metadata.insert("logical_path".into(), asset.to_owned());
            let request = ScriptModuleLoad {
                reference: asset,
                module_bytes: &bytes,
                permissions,
                metadata: record_metadata,
            };
            match scripting.load_module(&request) {
                Ok(()) => {
                    loaded.insert(key.clone());
                    return Ok(());
                }
                Err(error) => {
                    let dependency = missing_import_path(&error)
                        .ok_or_else(|| format!("failed to load .ysc asset '{asset}': {error}"))?;
                    if !attempted.insert(dependency.clone())
                        || attempted.len() > MAX_IMPORTS_PER_MODULE
                    {
                        return Err(format!(
                            "unable to resolve .ysc imports for '{asset}': {error}"
                        ));
                    }
                    load_recursive(
                        assets,
                        scripting,
                        &dependency,
                        permissions,
                        metadata,
                        loaded,
                        stack,
                    )?;
                }
            }
        }
    })();
    stack.pop();
    result
}

fn missing_import_path(message: &str) -> Option<String> {
    let (_, rest) = message.split_once(MISSING_IMPORT)?;
    let path = rest
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_')))
        .next()?;
    if !path.ends_with(".ysc")
        || path.starts_with('/')
        || path.split('/').any(|part| part.is_empty() || part == "..")
    {
        return None;
    }
    Some(path.to_owned())
}

#[cfg(test)]
mod tests {
    use super::missing_import_path;
    #[test]
    fn parses_provider_missing_import_without_arbitrary_paths() {
        let err = "scripting provider rejected module load: TYPESCRIPT_SCRIPTING_IMPORT_NOT_LOADED:scripts/lib/math.ysc | diagnostic";
        assert_eq!(
            missing_import_path(err).as_deref(),
            Some("scripts/lib/math.ysc")
        );
        assert_eq!(missing_import_path("unrelated error"), None);
        assert_eq!(
            missing_import_path("TYPESCRIPT_SCRIPTING_IMPORT_NOT_LOADED:../../passwd.ysc"),
            None
        );
    }
}
