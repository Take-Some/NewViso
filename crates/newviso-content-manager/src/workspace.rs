use super::*;

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
            surface_revision: 0,
            scene_editor: None,
            scene_cache: Default::default(),
            settings_editor: None,
            settings_visible: false,
        };
        manager.refresh()?;
        Ok(manager)
    }
    pub fn set_project_workspace(&mut self, project: ProjectWorkspaceView) {
        if self.selected.is_none() && self.root.join(&project.scene).is_file() {
            if let Err(error) = self.select(&project.scene) {
                self.status = error;
            }
        }
        self.project = Some(project);
        self.invalidate_surfaces();
    }
    pub fn refresh(&mut self) -> Result<(), String> {
        self.invalidate_surfaces();
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
    pub fn report_status(&mut self, message: impl Into<String>) {
        let message = message.into();
        if self.status != message {
            self.status = message;
            self.invalidate_surfaces();
        }
    }
    pub fn is_dirty(&self) -> bool {
        if self.settings_visible {
            return self.settings_editor.as_ref().is_some_and(|e| e.is_dirty());
        }
        self.selected.is_some()
            && self
                .scene_editor
                .as_ref()
                .map_or_else(|| self.editor_text != self.saved_text, |e| e.is_dirty())
    }
    fn stash_scene_editor(&mut self) {
        if let (Some(path), Some(editor)) = (self.selected.clone(), self.scene_editor.take()) {
            // Only the active entry scene needs live runtime identities. Keep at
            // most one suspended document, rather than retaining every map/history.
            if self
                .project
                .as_ref()
                .is_some_and(|project| project.scene != path)
            {
                return;
            }
            self.scene_cache.clear();
            self.scene_cache
                .insert(path, (editor, self.saved_text.clone()));
        }
    }
    pub(super) fn select(&mut self, logical_path: &str) -> Result<(), String> {
        if self.is_dirty() {
            return Err("Save current changes before selecting another file".into());
        }
        if self.selected.as_deref() == Some(normalize(logical_path).as_str()) {
            return Ok(());
        }
        let path = self.resolve(logical_path)?;
        self.settings_visible = false;
        let metadata = fs::metadata(&path)
            .map_err(|error| format!("content asset metadata failed '{logical_path}': {error}"))?;

        self.vfs_selected = None;
        self.vfs_preview.clear();

        if metadata.len() > edit_limit(logical_path) || !is_editable_path(logical_path) {
            self.stash_scene_editor();
            self.selected = Some(normalize(logical_path));
            self.editor_text.clear();
            self.saved_text.clear();
            self.status = format!("Selected {logical_path}");
            return Ok(());
        }

        let text = fs::read_to_string(&path)
            .map_err(|error| format!("content asset read failed '{logical_path}': {error}"))?;
        let key = normalize(logical_path);
        if self
            .scene_cache
            .get(&key)
            .is_some_and(|(_, saved)| saved != &text)
        {
            return Err("Scene changed outside the editor; reopen the project before rebinding live objects".into());
        }
        let next = if let Some((editor, _)) = self.scene_cache.remove(&key) {
            Some(editor)
        } else {
            scene_editor::SceneEditor::parse(&text)?
        };
        self.stash_scene_editor();
        self.selected = Some(key);
        self.saved_text = text.clone();
        self.scene_editor = next;
        self.editor_text = text;
        self.status = format!("Editing {logical_path}");
        Ok(())
    }
    pub(super) fn select_vfs(&mut self, logical_path: &str) -> Result<(), String> {
        if self.is_dirty() {
            return Err("Save current changes before selecting another file".into());
        }
        self.stash_scene_editor();
        self.settings_visible = false;
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
    pub(super) fn save(&mut self) -> Result<Option<ContentEffect>, String> {
        if self.settings_visible {
            self.save_settings()?;
            return Ok(None);
        }
        let Some(logical_path) = self.selected.clone() else {
            self.status = "Nothing writable selected".to_owned();
            return Ok(None);
        };
        if !is_editable_path(&logical_path) {
            self.status = format!("'{logical_path}' is not text-editable");
            return Ok(None);
        }
        if let Some(editor) = &self.scene_editor {
            editor.validate()?;
        }
        if !self.is_dirty() {
            self.status = format!("'{logical_path}' has no changes");
            return Ok(None);
        }

        let target = self.resolve(&logical_path)?;
        newviso_scene_document::write_authoring_file(
            &target,
            self.saved_text.as_bytes(),
            self.editor_text.as_bytes(),
        )?;

        self.saved_text = self.editor_text.clone();
        if let Some(editor) = self.scene_editor.as_mut() {
            editor.mark_saved();
        }
        self.status = if self.scene_editor.is_some() {
            format!("Saved {logical_path}; transforms are live in the editor")
        } else {
            format!("Saved {logical_path}")
        };

        if is_script_path(&logical_path) {
            return Ok(Some(ContentEffect::ScriptChanged {
                logical_path,
                bytes: self.saved_text.as_bytes().to_vec(),
            }));
        }

        Ok(None)
    }
    pub(super) fn resolve(&self, logical_path: &str) -> Result<PathBuf, String> {
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

        let path = self.root.join(relative);
        let canonical = path.canonicalize().map_err(|e| e.to_string())?;
        if !canonical.starts_with(self.root.canonicalize().map_err(|e| e.to_string())?) {
            return Err("content symlink escapes project root".into());
        }
        Ok(canonical)
    }
}
