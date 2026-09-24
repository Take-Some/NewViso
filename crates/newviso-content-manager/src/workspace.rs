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
    pub fn report_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }
    pub fn is_dirty(&self) -> bool {
        self.selected.is_some() && self.editor_text != self.saved_text
    }
    pub(super) fn select(&mut self, logical_path: &str) -> Result<(), String> {
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
    pub(super) fn select_vfs(&mut self, logical_path: &str) -> Result<(), String> {
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

        Ok(self.root.join(relative))
    }
}
