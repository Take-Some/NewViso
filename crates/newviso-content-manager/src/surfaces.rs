use super::*;

impl ContentManager {
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
}
