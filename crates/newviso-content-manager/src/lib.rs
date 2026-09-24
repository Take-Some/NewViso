use newviso_assets_client::AssetClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

mod dispatch;
mod surfaces;
mod workspace;

const MAX_EDIT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_VFS_ENTRIES: usize = 4096;
pub const CONTENT_MANAGER_SURFACE_ID: &str = "newviso.content_manager";
const BUILTIN_EDITOR_SHELL_JSON: &str = include_str!("assets/editor_shell.json");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContentEntry {
    pub path: String,
    pub kind: String,
    pub size_bytes: u64,
    pub editable: bool,
}

#[derive(Clone, Debug)]
pub enum ContentEffect {
    ScriptChanged {
        logical_path: String,
        bytes: Vec<u8>,
    },
}

#[derive(Clone, Debug)]
pub struct ProjectCapabilityView {
    pub id: String,
    pub min_version: u32,
    pub required: bool,
    pub requested_provider: Option<String>,
    pub resolved_provider: Option<String>,
    pub gateway: Option<String>,
    pub available: bool,
}

#[derive(Clone, Debug)]
pub struct ProjectWorkspaceView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub manifest: String,
    pub runtime: String,
    pub environment: String,
    pub scene: String,
    pub scripts: Option<String>,
    pub ui: Option<String>,
    pub capabilities: Vec<ProjectCapabilityView>,
}

#[derive(Debug)]
pub struct ContentManager {
    root: PathBuf,
    project: Option<ProjectWorkspaceView>,
    entries: Vec<ContentEntry>,
    vfs_entries: Vec<ContentEntry>,
    selected: Option<String>,
    vfs_selected: Option<String>,
    vfs_preview: String,
    vfs_directory: String,
    vfs_search: String,
    editor_text: String,
    saved_text: String,
    status: String,
}

impl ContentManager {}

fn collect_files(root: &Path, current: &Path, out: &mut Vec<ContentEntry>) -> Result<(), String> {
    let read = fs::read_dir(current).map_err(|error| {
        format!(
            "content directory read failed '{}': {error}",
            current.display()
        )
    })?;

    for entry in read {
        let entry = entry.map_err(|error| format!("content directory entry failed: {error}"))?;
        let path = entry.path();
        let file_name = entry.file_name();
        if file_name == ".newviso" || file_name == "target" || file_name == ".git" {
            continue;
        }

        if path.is_dir() {
            collect_files(root, &path, out)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .map_err(|error| format!("content relative path failed: {error}"))?;
        let logical = relative.to_string_lossy().replace('\\', "/");
        let metadata = entry
            .metadata()
            .map_err(|error| format!("content file metadata failed: {error}"))?;

        out.push(ContentEntry {
            kind: kind_for_path(&logical).to_owned(),
            editable: metadata.len() <= MAX_EDIT_BYTES && is_editable_path(&logical),
            path: logical,
            size_bytes: metadata.len(),
        });
    }

    Ok(())
}

fn collect_vfs_entries() -> Result<Vec<ContentEntry>, String> {
    let client = AssetClient::new();
    let mut out = Vec::new();
    let mut pending = vec![String::new()];
    let mut visited = BTreeSet::new();

    while let Some(dir) = pending.pop() {
        if !visited.insert(dir.clone()) {
            continue;
        }
        if out.len() >= MAX_VFS_ENTRIES {
            break;
        }

        let listing = client.vfs_list(&dir)?;
        let Some(entries) = listing.get("entries").and_then(Value::as_array) else {
            continue;
        };

        for entry in entries {
            let path = entry
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim_matches('/')
                .to_owned();
            if path.is_empty()
                || path
                    .split('/')
                    .any(|segment| segment == ".newviso" || segment == ".git")
            {
                continue;
            }

            match entry.get("kind").and_then(Value::as_str).unwrap_or("") {
                "directory" => pending.push(path),
                _ => {
                    let source_kind = entry
                        .get("source_kind")
                        .and_then(Value::as_str)
                        .unwrap_or("vfs");
                    out.push(ContentEntry {
                        path,
                        kind: format!("vfs/{source_kind}"),
                        size_bytes: entry.get("byte_len").and_then(Value::as_u64).unwrap_or(0),
                        editable: false,
                    });
                    if out.len() >= MAX_VFS_ENTRIES {
                        break;
                    }
                }
            }
        }
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.dedup_by(|a, b| a.path == b.path);
    Ok(out)
}

fn builtin_document() -> &'static Value {
    static BUILTIN: OnceLock<Value> = OnceLock::new();

    BUILTIN.get_or_init(|| {
        serde_json::from_str(BUILTIN_EDITOR_SHELL_JSON)
            .expect("newviso-content-manager built-in editor_shell.json must be valid JSON")
    })
}

fn builtin_surface(name: &str) -> Value {
    builtin_document()
        .get("surfaces")
        .and_then(|surfaces| surfaces.get(name))
        .cloned()
        .unwrap_or_else(|| {
            panic!("newviso-content-manager built-in editor_shell.json has no surface '{name}'")
        })
}

fn builtin_node(name: &str) -> Value {
    builtin_document()
        .get("nodes")
        .and_then(|nodes| nodes.get(name))
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "newviso-content-manager built-in editor_shell.json has no node template '{name}'"
            )
        })
}

fn set_surface_field(surface: &mut Value, key: &str, value: Value) {
    let object = surface
        .as_object_mut()
        .expect("built-in content-manager surface must be a JSON object");
    object.insert(key.to_owned(), value);
}

fn node_props_mut(node: &mut Value) -> &mut serde_json::Map<String, Value> {
    node.as_object_mut()
        .expect("built-in content-manager node must be a JSON object")
        .entry("props")
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .expect("built-in content-manager node props must be a JSON object")
}

fn set_prop_field(node: &mut Value, key: &str, value: Value) {
    node_props_mut(node).insert(key.to_owned(), value);
}

fn set_payload_path(node: &mut Value, path: impl Into<String>) {
    let props = node_props_mut(node);
    let payload = props
        .entry("payload")
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .expect("built-in content-manager payload must be a JSON object");
    payload.insert("path".to_owned(), Value::String(path.into()));
}

fn set_component_field(surface: &mut Value, id: &str, key: &str, value: Value) {
    let node = find_component_mut(surface, id)
        .unwrap_or_else(|| panic!("built-in content-manager surface is missing component '{id}'"));
    node.insert(key.to_owned(), value);
}

fn find_component_mut<'a>(
    value: &'a mut Value,
    id: &str,
) -> Option<&'a mut serde_json::Map<String, Value>> {
    match value {
        Value::Object(map) => {
            if map.get("id").and_then(Value::as_str) == Some(id) {
                return Some(map);
            }

            for child in map.values_mut() {
                if let Some(found) = find_component_mut(child, id) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(values) => {
            for child in values {
                if let Some(found) = find_component_mut(child, id) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn project_file_action(id: &str, label: &str, path: &str) -> Value {
    let mut node = builtin_node("file_action");
    set_surface_field(&mut node, "id", Value::String(format!("project.file.{id}")));
    set_surface_field(&mut node, "text", Value::String(label.to_owned()));
    set_surface_field(&mut node, "detail", Value::String(path.to_owned()));
    set_payload_path(&mut node, path);
    node
}

fn asset_card(entry: &ContentEntry, selected: Option<&str>) -> Value {
    let name = entry.path.rsplit('/').next().unwrap_or(entry.path.as_str());
    let type_tag = asset_type_tag(&entry.path, &entry.kind);

    let mut node = builtin_node("asset_card");
    set_surface_field(
        &mut node,
        "id",
        Value::String(format!("content.vfs.asset.{}", entry.path)),
    );
    set_surface_field(&mut node, "text", Value::String(name.to_owned()));
    set_surface_field(
        &mut node,
        "detail",
        Value::String(format!("{} · {} bytes", type_tag, entry.size_bytes)),
    );
    set_surface_field(
        &mut node,
        "tone",
        Value::String(
            if selected == Some(entry.path.as_str()) {
                "accent"
            } else {
                "normal"
            }
            .to_owned(),
        ),
    );
    set_prop_field(&mut node, "type_tag", Value::String(type_tag.to_owned()));
    set_payload_path(&mut node, entry.path.clone());
    node
}

fn asset_type_tag(_path: &str, kind: &str) -> &'static str {
    if kind.contains("directory") {
        "DIR"
    } else if kind.contains("container") || kind.contains("package") {
        "PACKAGE"
    } else if kind.contains("text") {
        "TEXT"
    } else {
        "ASSET"
    }
}

fn normalize(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_owned()
}

fn is_script_path(path: &str) -> bool {
    normalize(path)
        .split('/')
        .next()
        .is_some_and(|segment| segment.eq_ignore_ascii_case("scripts"))
}

fn is_editable_path(_path: &str) -> bool {
    true
}

fn kind_for_path(path: &str) -> &'static str {
    if is_script_path(path) {
        "script"
    } else {
        "asset"
    }
}

fn language_for_path(_path: &str) -> &'static str {
    "text"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builtin_editor_shell_json_is_valid_and_complete() {
        let document: Value = serde_json::from_str(BUILTIN_EDITOR_SHELL_JSON).unwrap();
        for surface in ["toolbar", "project", "details", "content_browser"] {
            assert!(document
                .get("surfaces")
                .and_then(|surfaces| surfaces.get(surface))
                .is_some());
        }
        for node in [
            "file_action",
            "text",
            "list",
            "capability",
            "source_directory",
            "folder_card",
            "asset_card",
            "code_editor",
            "preview",
        ] {
            assert!(document
                .get("nodes")
                .and_then(|nodes| nodes.get(node))
                .is_some());
        }
    }

    #[test]
    fn blocks_parent_traversal() {
        let root = std::env::temp_dir().join("newviso-content-manager-test");
        fs::create_dir_all(&root).unwrap();
        let manager = ContentManager {
            root,
            project: None,
            entries: Vec::new(),
            vfs_entries: Vec::new(),
            selected: None,
            vfs_selected: None,
            vfs_preview: String::new(),
            vfs_directory: String::new(),
            vfs_search: String::new(),
            editor_text: String::new(),
            saved_text: String::new(),
            status: String::new(),
        };
        assert!(manager.resolve("../outside.txt").is_err());
    }

    #[test]
    fn detects_script_extensions() {
        assert!(is_script_path("scripts/main"));
        assert!(is_script_path("scripts/secondary"));
        assert!(!is_script_path("content/main"));
        assert!(!is_script_path("scenes/main"));
    }

    #[test]
    fn reactive_script_edit_saves_and_emits_reload_effect() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "newviso-content-manager-reactive-{}-{stamp}",
            std::process::id()
        ));
        let scripts_dir = root.join("scripts");
        fs::create_dir_all(&scripts_dir).unwrap();
        let script_path = scripts_dir.join("main.ysc");
        fs::write(
            &script_path,
            "export function on_frame(payload: unknown) { return { value: 1 }; }",
        )
        .unwrap();

        let mut manager = ContentManager {
            root: root.clone(),
            project: None,
            entries: vec![ContentEntry {
                path: "scripts/main.ysc".to_owned(),
                kind: "script".to_owned(),
                size_bytes: fs::metadata(&script_path).unwrap().len(),
                editable: true,
            }],
            vfs_entries: Vec::new(),
            selected: None,
            vfs_selected: None,
            vfs_preview: String::new(),
            vfs_directory: String::new(),
            vfs_search: String::new(),
            editor_text: String::new(),
            saved_text: String::new(),
            status: String::new(),
        };

        manager
            .apply_dispatch(&json!({
                "actions": [{
                    "action_id": "content.select",
                    "payload": { "path": "scripts/main.ysc" }
                }]
            }))
            .unwrap();

        let updated = "export function on_frame(payload: unknown) { return { value: 2 }; }";
        let effects = manager
            .apply_dispatch(&json!({
                "state_patches": [{
                    "changes": [{
                        "path": "content.editor.value",
                        "value": updated
                    }]
                }],
                "actions": [{
                    "action_id": "content.save",
                    "payload": null
                }]
            }))
            .unwrap();

        assert_eq!(fs::read_to_string(&script_path).unwrap(), updated);
        assert!(!manager.is_dirty());
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            ContentEffect::ScriptChanged {
                logical_path,
                bytes,
            } => {
                assert_eq!(logical_path, "scripts/main.ysc");
                assert_eq!(bytes, updated.as_bytes());
            }
        }

        fs::remove_dir_all(root).unwrap();
    }
}
