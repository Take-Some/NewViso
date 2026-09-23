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

impl ContentManager {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|error| format!("content manager project root resolve failed: {error}"))?;

        let mut manager = Self {
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
            status: "Ready".to_owned(),
        };
        manager.refresh()?;
        Ok(manager)
    }

    pub fn set_project_workspace(&mut self, project: ProjectWorkspaceView) {
        self.project = Some(project);
    }

    pub fn refresh(&mut self) -> Result<(), String> {
        let mut entries = Vec::new();
        collect_files(&self.root, &self.root, &mut entries)?;
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        self.entries = entries;

        self.vfs_entries = collect_vfs_entries().unwrap_or_default();
        self.status = format!(
            "{} project files · {} VFS assets",
            self.entries.len(),
            self.vfs_entries.len()
        );
        Ok(())
    }

    pub fn surfaces(&self) -> Vec<Value> {
        let fallback_file_nodes = self
            .entries
            .iter()
            .map(|entry| {
                let mut node = builtin_node("file_action");
                set_surface_field(
                    &mut node,
                    "id",
                    Value::String(format!("content.file.{}", entry.path)),
                );
                set_surface_field(&mut node, "text", Value::String(entry.path.clone()));
                set_surface_field(
                    &mut node,
                    "detail",
                    Value::String(format!("{} · {} bytes", entry.kind, entry.size_bytes)),
                );
                set_surface_field(
                    &mut node,
                    "tone",
                    Value::String(
                        if self.selected.as_deref() == Some(entry.path.as_str()) {
                            "accent"
                        } else {
                            "normal"
                        }
                        .to_owned(),
                    ),
                );
                set_payload_path(&mut node, entry.path.clone());
                node
            })
            .collect::<Vec<_>>();

        let (project_name, project_subtitle, manifest_path, project_components) =
            if let Some(project) = self.project.as_ref() {
                let mut entry_files = vec![
                    project_file_action("scene", "Entry Scene", &project.scene),
                    project_file_action("runtime", "Runtime Settings", &project.runtime),
                    project_file_action("environment", "Environment", &project.environment),
                ];
                if let Some(path) = project.scripts.as_deref() {
                    entry_files.push(project_file_action("scripts", "Scripts", path));
                }
                if let Some(path) = project.ui.as_deref() {
                    entry_files.push(project_file_action("ui", "Game UI", path));
                }

                let capability_nodes = if project.capabilities.is_empty() {
                    let mut node = builtin_node("text");
                    set_surface_field(
                        &mut node,
                        "id",
                        Value::String("project.capabilities.empty".to_owned()),
                    );
                    set_surface_field(
                        &mut node,
                        "text",
                        Value::String("No project capabilities declared".to_owned()),
                    );
                    vec![node]
                } else {
                    project
                        .capabilities
                        .iter()
                        .map(|capability| {
                            let provider = capability
                                .resolved_provider
                                .as_deref()
                                .or(capability.requested_provider.as_deref())
                                .unwrap_or("auto");
                            let gateway = capability.gateway.as_deref().unwrap_or("-");

                            let mut node = builtin_node("capability");
                            set_surface_field(
                                &mut node,
                                "id",
                                Value::String(format!("project.capability.{}", capability.id)),
                            );
                            set_surface_field(
                                &mut node,
                                "text",
                                Value::String(format!(
                                    "{} v{}",
                                    capability.id, capability.min_version
                                )),
                            );
                            set_surface_field(
                                &mut node,
                                "detail",
                                Value::String(format!(
                                    "{} · {} · provider={} · gateway={}",
                                    if capability.required {
                                        "Required"
                                    } else {
                                        "Optional"
                                    },
                                    if capability.available {
                                        "Ready"
                                    } else {
                                        "Unavailable"
                                    },
                                    provider,
                                    gateway
                                )),
                            );
                            set_surface_field(
                                &mut node,
                                "tone",
                                Value::String(
                                    if capability.available {
                                        "accent"
                                    } else {
                                        "normal"
                                    }
                                    .to_owned(),
                                ),
                            );
                            set_payload_path(&mut node, project.manifest.clone());
                            node
                        })
                        .collect::<Vec<_>>()
                };

                let mut manifest = builtin_node("file_action");
                set_surface_field(
                    &mut manifest,
                    "id",
                    Value::String("project.manifest".to_owned()),
                );
                set_surface_field(
                    &mut manifest,
                    "text",
                    Value::String("Project Settings".to_owned()),
                );
                set_surface_field(
                    &mut manifest,
                    "detail",
                    Value::String(project.manifest.clone()),
                );
                set_surface_field(
                    &mut manifest,
                    "tone",
                    Value::String(
                        if self.selected.as_deref() == Some(project.manifest.as_str()) {
                            "accent"
                        } else {
                            "normal"
                        }
                        .to_owned(),
                    ),
                );
                set_payload_path(&mut manifest, project.manifest.clone());

                let mut entry_heading = builtin_node("text");
                set_surface_field(
                    &mut entry_heading,
                    "id",
                    Value::String("project.entry.heading".to_owned()),
                );
                set_surface_field(
                    &mut entry_heading,
                    "text",
                    Value::String("Game Entry".to_owned()),
                );

                let mut entry_list = builtin_node("list");
                set_surface_field(
                    &mut entry_list,
                    "id",
                    Value::String("project.entry.files".to_owned()),
                );
                set_surface_field(&mut entry_list, "children", Value::Array(entry_files));

                let mut capability_heading = builtin_node("text");
                set_surface_field(
                    &mut capability_heading,
                    "id",
                    Value::String("project.capabilities.heading".to_owned()),
                );
                set_surface_field(
                    &mut capability_heading,
                    "text",
                    Value::String("Engine Capabilities".to_owned()),
                );

                let mut capability_list = builtin_node("list");
                set_surface_field(
                    &mut capability_list,
                    "id",
                    Value::String("project.capabilities".to_owned()),
                );
                set_surface_field(
                    &mut capability_list,
                    "children",
                    Value::Array(capability_nodes),
                );

                (
                    project.name.clone(),
                    format!("{} · v{}", project.id, project.version),
                    project.manifest.clone(),
                    vec![
                        manifest,
                        entry_heading,
                        entry_list,
                        capability_heading,
                        capability_list,
                    ],
                )
            } else {
                let mut list = builtin_node("list");
                set_surface_field(&mut list, "id", Value::String("content.files".to_owned()));
                set_surface_field(&mut list, "children", Value::Array(fallback_file_nodes));

                (
                    "Project".to_owned(),
                    format!("{} source files", self.entries.len()),
                    "project.json".to_owned(),
                    vec![list],
                )
            };

        let mut source_dirs = BTreeSet::new();
        for entry in &self.vfs_entries {
            let parts = entry.path.split('/').collect::<Vec<_>>();
            let mut directory = String::new();
            for part in parts.iter().take(parts.len().saturating_sub(1)) {
                if !directory.is_empty() {
                    directory.push('/');
                }
                directory.push_str(part);
                source_dirs.insert(directory.clone());
            }
        }

        let mut root_source = builtin_node("source_directory");
        set_surface_field(
            &mut root_source,
            "id",
            Value::String("content.vfs.source.root".to_owned()),
        );
        set_surface_field(&mut root_source, "text", Value::String("/".to_owned()));
        set_surface_field(
            &mut root_source,
            "tone",
            Value::String(
                if self.vfs_directory.is_empty() {
                    "accent"
                } else {
                    "normal"
                }
                .to_owned(),
            ),
        );
        set_payload_path(&mut root_source, "");

        let mut source_nodes = vec![root_source];
        source_nodes.extend(source_dirs.iter().map(|directory| {
            let mut node = builtin_node("source_directory");
            set_surface_field(
                &mut node,
                "id",
                Value::String(format!("content.vfs.source.{directory}")),
            );
            set_surface_field(&mut node, "text", Value::String(format!("/{directory}")));
            set_surface_field(
                &mut node,
                "tone",
                Value::String(
                    if self.vfs_directory == *directory {
                        "accent"
                    } else {
                        "normal"
                    }
                    .to_owned(),
                ),
            );
            set_payload_path(&mut node, directory.clone());
            node
        }));

        let search = self.vfs_search.trim().to_ascii_lowercase();
        let current_prefix = if self.vfs_directory.is_empty() {
            String::new()
        } else {
            format!("{}/", self.vfs_directory.trim_matches('/'))
        };

        let mut folders = BTreeSet::new();
        let mut browser_nodes = Vec::new();

        for entry in &self.vfs_entries {
            if !search.is_empty() {
                if !entry.path.to_ascii_lowercase().contains(&search) {
                    continue;
                }
                browser_nodes.push(asset_card(entry, self.vfs_selected.as_deref()));
                continue;
            }

            let Some(relative) = entry.path.strip_prefix(&current_prefix) else {
                continue;
            };

            if let Some((folder, _)) = relative.split_once('/') {
                folders.insert(folder.to_owned());
            } else {
                browser_nodes.push(asset_card(entry, self.vfs_selected.as_deref()));
            }
        }

        if search.is_empty() {
            let mut folder_cards = folders
                .into_iter()
                .map(|folder| {
                    let path = if self.vfs_directory.is_empty() {
                        folder.clone()
                    } else {
                        format!("{}/{}", self.vfs_directory.trim_matches('/'), folder)
                    };

                    let mut node = builtin_node("folder_card");
                    set_surface_field(
                        &mut node,
                        "id",
                        Value::String(format!("content.vfs.folder.{path}")),
                    );
                    set_surface_field(&mut node, "text", Value::String(folder));
                    set_payload_path(&mut node, path);
                    node
                })
                .collect::<Vec<_>>();
            folder_cards.append(&mut browser_nodes);
            browser_nodes = folder_cards;
        }

        let breadcrumb = if self.vfs_directory.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}", self.vfs_directory)
        };

        let selected = self
            .selected
            .as_deref()
            .or(self.vfs_selected.as_deref())
            .unwrap_or("<nothing selected>");

        let editable = self
            .selected
            .as_deref()
            .and_then(|path| self.entries.iter().find(|entry| entry.path == path))
            .is_some_and(|entry| entry.editable);

        let details = if editable {
            let mut node = builtin_node("code_editor");
            set_surface_field(&mut node, "text", Value::String(selected.to_owned()));
            set_surface_field(&mut node, "value", Value::String(self.editor_text.clone()));
            set_prop_field(
                &mut node,
                "language",
                Value::String(language_for_path(selected).to_owned()),
            );
            node
        } else {
            let preview = if self.vfs_selected.is_some() {
                if self.vfs_preview.is_empty() {
                    "Selected VFS asset has no text preview.".to_owned()
                } else {
                    self.vfs_preview.clone()
                }
            } else if self.selected.is_some() {
                "Binary or non-editable project asset. Metadata preview only.".to_owned()
            } else {
                "Select a project file or VFS asset.".to_owned()
            };

            let mut node = builtin_node("preview");
            set_surface_field(&mut node, "text", Value::String(preview));
            node
        };

        let mut toolbar = builtin_surface("toolbar");
        set_component_field(
            &mut toolbar,
            "project.name",
            "text",
            Value::String(project_name),
        );
        if let Some(node) = find_component_mut(&mut toolbar, "project.settings") {
            let mut value = Value::Object(node.clone());
            set_payload_path(&mut value, manifest_path.clone());
            *node = value
                .as_object()
                .expect("project.settings template must stay an object")
                .clone();
        } else {
            panic!("built-in content-manager toolbar is missing project.settings");
        }
        set_component_field(
            &mut toolbar,
            "content.save",
            "tone",
            Value::String(if self.is_dirty() { "accent" } else { "normal" }.to_owned()),
        );
        set_component_field(
            &mut toolbar,
            "content.selected",
            "text",
            Value::String(selected.to_owned()),
        );

        let mut project_panel = builtin_surface("project");
        set_surface_field(
            &mut project_panel,
            "subtitle",
            Value::String(project_subtitle),
        );
        set_surface_field(
            &mut project_panel,
            "components",
            Value::Array(project_components),
        );

        let details_title = if selected == manifest_path {
            "Project Settings"
        } else if editable && is_script_path(selected) {
            "Script Editor"
        } else if editable {
            "Asset Editor"
        } else {
            "Details"
        };
        let details_footer = if self.is_dirty() {
            "Unsaved changes · Ctrl+S to Save + Reload"
        } else if editable {
            "Ctrl+S · Save + Reload"
        } else {
            "Project asset details"
        };

        let mut details_panel = builtin_surface("details");
        set_surface_field(
            &mut details_panel,
            "title",
            Value::String(details_title.to_owned()),
        );
        set_surface_field(
            &mut details_panel,
            "subtitle",
            Value::String(selected.to_owned()),
        );
        set_surface_field(
            &mut details_panel,
            "footer_lines",
            Value::Array(vec![Value::String(details_footer.to_owned())]),
        );
        set_surface_field(
            &mut details_panel,
            "components",
            Value::Array(vec![details]),
        );

        let mut content_browser = builtin_surface("content_browser");
        set_surface_field(
            &mut content_browser,
            "subtitle",
            Value::String(format!("{} runtime VFS assets", self.vfs_entries.len())),
        );
        set_component_field(
            &mut content_browser,
            "content.browser.sources.list",
            "children",
            Value::Array(source_nodes),
        );
        set_component_field(
            &mut content_browser,
            "content.browser.path",
            "text",
            Value::String(breadcrumb),
        );
        set_component_field(
            &mut content_browser,
            "content.browser.search",
            "value",
            Value::String(self.vfs_search.clone()),
        );
        set_component_field(
            &mut content_browser,
            "content.browser.grid",
            "children",
            Value::Array(browser_nodes),
        );

        vec![toolbar, project_panel, details_panel, content_browser]
    }

    pub fn apply_dispatch(&mut self, dispatch: &Value) -> Result<Vec<ContentEffect>, String> {
        let mut effects = Vec::new();

        if let Some(patches) = dispatch.get("state_patches").and_then(Value::as_array) {
            for patch in patches {
                if let Some(changes) = patch.get("changes").and_then(Value::as_array) {
                    for change in changes {
                        let path = change.get("path").and_then(Value::as_str).unwrap_or("");
                        if path == "content.editor.value" {
                            if let Some(value) = change.get("value").and_then(Value::as_str) {
                                self.editor_text = value.to_owned();
                            }
                        } else if path == "content.vfs.search" {
                            if let Some(value) = change.get("value").and_then(Value::as_str) {
                                self.vfs_search = value.to_owned();
                            }
                        }
                    }
                }
            }
        }

        if let Some(actions) = dispatch.get("actions").and_then(Value::as_array) {
            for action in actions {
                match action
                    .get("action_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                {
                    "content.select" => {
                        let path = action
                            .get("payload")
                            .and_then(|payload| payload.get("path"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        if !path.is_empty() {
                            self.select(path)?;
                        }
                    }
                    "content.vfs.select" => {
                        let path = action
                            .get("payload")
                            .and_then(|payload| payload.get("path"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        if !path.is_empty() {
                            self.select_vfs(path)?;
                        }
                    }
                    "content.vfs.open_dir" => {
                        let path = action
                            .get("payload")
                            .and_then(|payload| payload.get("path"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        self.vfs_directory = normalize(path);
                        self.vfs_search.clear();
                        self.vfs_selected = None;
                        self.vfs_preview.clear();
                        self.status = if self.vfs_directory.is_empty() {
                            "Content Browser /".to_owned()
                        } else {
                            format!("Content Browser /{}", self.vfs_directory)
                        };
                    }
                    "content.vfs.up" => {
                        self.vfs_directory = self
                            .vfs_directory
                            .rsplit_once('/')
                            .map(|(parent, _)| parent.to_owned())
                            .unwrap_or_default();
                        self.vfs_search.clear();
                        self.vfs_selected = None;
                        self.vfs_preview.clear();
                    }
                    "content.refresh" => self.refresh()?,
                    "content.save" => {
                        if let Some(effect) = self.save()? {
                            effects.push(effect);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(effects)
    }

    pub fn report_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }

    pub fn is_dirty(&self) -> bool {
        self.selected.is_some() && self.editor_text != self.saved_text
    }

    fn select(&mut self, logical_path: &str) -> Result<(), String> {
        let path = self.resolve(logical_path)?;
        let metadata = fs::metadata(&path)
            .map_err(|error| format!("content asset metadata failed '{logical_path}': {error}"))?;

        self.vfs_selected = None;
        self.vfs_preview.clear();

        if metadata.len() > MAX_EDIT_BYTES || !is_editable_path(logical_path) {
            self.selected = Some(normalize(logical_path));
            self.editor_text.clear();
            self.saved_text.clear();
            self.status = format!("Selected {logical_path}");
            return Ok(());
        }

        let text = fs::read_to_string(&path)
            .map_err(|error| format!("content asset read failed '{logical_path}': {error}"))?;
        self.selected = Some(normalize(logical_path));
        self.saved_text = text.clone();
        self.editor_text = text;
        self.status = format!("Editing {logical_path}");
        Ok(())
    }

    fn select_vfs(&mut self, logical_path: &str) -> Result<(), String> {
        let logical_path = normalize(logical_path);
        let entry = self
            .vfs_entries
            .iter()
            .find(|entry| entry.path == logical_path)
            .cloned()
            .ok_or_else(|| format!("VFS asset '{logical_path}' is not in the current catalog"))?;

        self.selected = None;
        self.editor_text.clear();
        self.saved_text.clear();
        self.vfs_selected = Some(logical_path.clone());

        let client = AssetClient::new();
        self.vfs_preview = if entry.size_bytes <= MAX_EDIT_BYTES {
            match client.text(&logical_path) {
                Ok(text) => text,
                Err(_) => format!(
                    "VFS asset: {}\nKind: {}\nSize: {} bytes\nPreview: binary/opaque; format interpretation belongs to engine.assets",
                    entry.path, entry.kind, entry.size_bytes
                ),
            }
        } else {
            format!(
                "VFS asset: {}\nKind: {}\nSize: {} bytes\nPreview skipped (> {} bytes)",
                entry.path, entry.kind, entry.size_bytes, MAX_EDIT_BYTES
            )
        };

        self.status = format!("VFS asset {logical_path}");
        Ok(())
    }

    fn save(&mut self) -> Result<Option<ContentEffect>, String> {
        let Some(logical_path) = self.selected.clone() else {
            self.status = "Nothing writable selected".to_owned();
            return Ok(None);
        };
        if !is_editable_path(&logical_path) {
            self.status = format!("'{logical_path}' is not text-editable");
            return Ok(None);
        }
        if !self.is_dirty() {
            self.status = format!("'{logical_path}' has no changes");
            return Ok(None);
        }

        let target = self.resolve(&logical_path)?;
        let parent = target
            .parent()
            .ok_or_else(|| format!("content asset '{logical_path}' has no parent"))?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("content asset parent create failed: {error}"))?;

        let temp = target.with_extension(format!(
            "{}.newviso.tmp",
            target
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("file")
        ));

        {
            let mut file = fs::File::create(&temp)
                .map_err(|error| format!("content temp file create failed: {error}"))?;
            file.write_all(self.editor_text.as_bytes())
                .map_err(|error| format!("content temp file write failed: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("content temp file sync failed: {error}"))?;
        }

        if target.exists() {
            fs::remove_file(&target)
                .map_err(|error| format!("content target replace failed: {error}"))?;
        }
        fs::rename(&temp, &target)
            .map_err(|error| format!("content temp commit failed: {error}"))?;

        self.saved_text = self.editor_text.clone();
        self.status = format!("Saved {logical_path}");

        if is_script_path(&logical_path) {
            return Ok(Some(ContentEffect::ScriptChanged {
                logical_path,
                bytes: self.saved_text.as_bytes().to_vec(),
            }));
        }

        Ok(None)
    }

    fn resolve(&self, logical_path: &str) -> Result<PathBuf, String> {
        let normalized = normalize(logical_path);
        let relative = Path::new(&normalized);

        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(format!(
                "content path escapes project root: '{logical_path}'"
            ));
        }

        Ok(self.root.join(relative))
    }
}

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

fn set_prop_field(node: &mut Value, key: &str, value: Value) {
    let object = node
        .as_object_mut()
        .expect("built-in content-manager node must be a JSON object");
    let props = object
        .entry("props")
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .expect("built-in content-manager node props must be a JSON object");
    props.insert(key.to_owned(), value);
}

fn set_payload_path(node: &mut Value, path: impl Into<String>) {
    let object = node
        .as_object_mut()
        .expect("built-in content-manager node must be a JSON object");
    let props = object
        .entry("props")
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .expect("built-in content-manager node props must be a JSON object");
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
