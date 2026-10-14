//! Validated checkpoint encoding and atomic primary/backup writes.
use super::*;

impl WorldPersistence {
    pub(super) fn backup_path(&self) -> PathBuf {
        PathBuf::from(format!("{}.bak", self.path.display()))
    }

    pub(super) fn check_path(&self, path: &Path) -> Result<(), String> {
        let mut existing = path;
        while !existing.exists() {
            existing = existing
                .parent()
                .ok_or("save path has no existing parent")?;
        }
        let canonical = existing
            .canonicalize()
            .map_err(|e| format!("save path resolution: {e}"))?;
        if !canonical.starts_with(&self.root) {
            return Err("world save path escapes project root".into());
        }
        Ok(())
    }

    pub(super) fn read_valid(
        &self,
        path: &Path,
    ) -> Result<
        (
            LivingWorldRuntime,
            BTreeMap<String, WorldActorPresentationBinding>,
            ItemsRuntime,
            Value,
            Value,
            Vec<u8>,
        ),
        String,
    > {
        self.check_path(path)?;
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(MAX_SAVE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_SAVE_BYTES {
            return Err("world save exceeds size limit".into());
        }
        let envelope: SaveEnvelope = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if envelope.schema != SAVE_SCHEMA || envelope.project_id != self.project_id {
            return Err("world save schema or project identity mismatch".into());
        }
        if blake3::hash(envelope.payload.as_bytes()).to_hex().as_str() != envelope.checksum {
            return Err("world save checksum mismatch".into());
        }
        let payload: SavePayload =
            serde_json::from_str(&envelope.payload).map_err(|e| e.to_string())?;
        let world = LivingWorldRuntime::from_checkpoint(payload.world)?;
        let mut keys = std::collections::BTreeSet::new();
        for (actor, binding) in &payload.presentations {
            binding.validate(actor)?;
            if !keys.insert(&binding.scene_key) {
                return Err("duplicate saved presentation scene key".into());
            }
        }
        let items = ItemsRuntime::from_checkpoint(payload.items)?;
        Ok((world, payload.presentations, items, payload.climate, payload.cold, bytes))
    }

    pub(super) fn atomic_write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        self.check_path(path)?;
        let parent = path.parent().ok_or("save file has no parent")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        self.check_path(path)?;
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(".world-save-{}-{serial}.tmp", std::process::id()));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|e| e.to_string())?;
            file.write_all(bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            fs::rename(&temporary, path).map_err(|e| format!("atomic save replace failed: {e}"))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    pub(super) fn save(
        &mut self,
        world: &LivingWorldRuntime,
        presentations: &BTreeMap<String, WorldActorPresentationBinding>,
        items: &ItemsRuntime,
        climate: &Value,
        cold: &Value,
    ) -> Result<(), String> {
        let payload = serde_json::to_string(&SavePayload {
            world: world.checkpoint()?,
            presentations: presentations.clone(),
            items: items.checkpoint()?,
            climate: climate.clone(),
            cold: cold.clone(),
        })
        .map_err(|e| e.to_string())?;
        let envelope = SaveEnvelope {
            schema: SAVE_SCHEMA.into(),
            project_id: self.project_id.clone(),
            saved_at_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            checksum: blake3::hash(payload.as_bytes()).to_hex().to_string(),
            payload,
        };
        let bytes = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_SAVE_BYTES {
            return Err("world save exceeds size limit".into());
        }
        // Use the last validated bytes, never a potentially corrupted primary file.
        if let Some(previous) = &self.last_good {
            self.atomic_write(&self.backup_path(), previous)?;
        }
        self.atomic_write(&self.path, &bytes)?;
        self.last_good = Some(bytes);
        self.last_saved_world_seconds = world.runtime_state()["clock"]["world_seconds"].as_f64();
        self.last_error = None;
        self.save_count += 1;
        Ok(())
    }

}
