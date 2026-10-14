use super::*;

impl ContentManager {
    pub fn apply_dispatch(&mut self, dispatch: &Value) -> Result<Vec<ContentEffect>, String> {
        let mut effects = Vec::new();

        if let Some(patches) = dispatch.get("state_patches").and_then(Value::as_array) {
            for patch in patches {
                if let Some(changes) = patch.get("changes").and_then(Value::as_array) {
                    for change in changes {
                        let path = change.get("path").and_then(Value::as_str).unwrap_or("");
                        if path == "content.editor.value" {
                            if let Some(value) = change.get("value").and_then(Value::as_str) {
                                if self.editor_text!=value{self.editor_text=value.to_owned();self.invalidate_surfaces();}
                            }
                        } else if path == "content.vfs.search" {
                            if let Some(value) = change.get("value").and_then(Value::as_str) {
                                if self.vfs_search!=value{self.vfs_search=value.to_owned();self.invalidate_surfaces();}
                            }
                        }
                    }
                }
            }
        }

        if let Some(actions) = dispatch.get("actions").and_then(Value::as_array) {
            for action in actions {
                let id=action.get("action_id").and_then(Value::as_str).unwrap_or("");
                if matches!(id,"content.select"|"content.vfs.select"|"content.vfs.open_dir"|
                    "content.vfs.up"|"content.refresh"|"content.save"){self.invalidate_surfaces();}
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
