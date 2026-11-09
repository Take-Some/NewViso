use super::*;

impl ContentManager {
    pub fn apply_dispatch(&mut self, dispatch: &Value) -> Result<Vec<ContentEffect>, String> {
        let mut effects = Vec::new();

        if let Some(patches) = dispatch.get("state_patches").and_then(Value::as_array) {
            for patch in patches {
                if let Some(changes) = patch.get("changes").and_then(Value::as_array) {
                    for change in changes {
                        let path = change.get("path").and_then(Value::as_str).unwrap_or("");
                        if path.starts_with("content.settings.") {
                            if let Some(editor) = self.settings_editor.as_mut() {
                                let result = editor.patch(path, &change["value"]);
                                self.invalidate_surfaces();
                                if let Err(error) = result {
                                    self.status = error;
                                }
                            }
                        } else if path.starts_with("content.scene.") {
                            if let Some(editor) = self.scene_editor.as_mut() {
                                let result =
                                    editor.patch(path, &change["value"], &mut self.editor_text);
                                self.invalidate_surfaces();
                                if let Err(error) = result {
                                    self.status = error;
                                }
                            }
                        } else if path == "content.editor.value" {
                            if self.scene_editor.is_some() {
                                return Err(
                                    "Scene properties must be edited through the inspector".into(),
                                );
                            }
                            if let Some(value) = change.get("value").and_then(Value::as_str) {
                                if self.editor_text != value {
                                    self.editor_text = value.to_owned();
                                    self.invalidate_surfaces();
                                }
                            }
                        } else if path == "content.vfs.search" {
                            if let Some(value) = change.get("value").and_then(Value::as_str) {
                                if self.vfs_search != value {
                                    self.vfs_search = value.to_owned();
                                    self.invalidate_surfaces();
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(actions) = dispatch.get("actions").and_then(Value::as_array) {
            for action in actions {
                let id = action
                    .get("action_id")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if matches!(
                    id,
                    "content.select"
                        | "content.vfs.select"
                        | "content.vfs.open_dir"
                        | "content.vfs.up"
                        | "content.refresh"
                        | "content.save"
                ) {
                    self.invalidate_surfaces();
                }
                if id.starts_with("content.viewport.") {
                    self.viewport_action(id, &action["payload"])?;
                    continue;
                }
                if id.starts_with("content.settings.") {
                    let effect = self.settings_action(id, &action["payload"]);
                    self.invalidate_surfaces();
                    if let Some(effect) = effect? {
                        effects.push(effect);
                    }
                    continue;
                }
                if id.starts_with("content.scene.") {
                    if self.settings_visible {
                        if self.is_dirty() {
                            return Err(
                                "Save or revert Project Settings before editing the scene".into()
                            );
                        }
                        self.settings_visible = false;
                    }
                    if let Some(editor) = self.scene_editor.as_mut() {
                        let result = editor.action(id, &action["payload"], &mut self.editor_text);
                        self.invalidate_surfaces();
                        self.status = result
                            .map(|_| {
                                "Scene edited; transforms update live in the editor. Save writes to disk".into()
                            })
                            .unwrap_or_else(|e| e);
                    }
                    continue;
                }
                match id {
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
}
