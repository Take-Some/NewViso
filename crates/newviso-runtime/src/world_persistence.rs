use super::*;
use newviso_project::ProjectWorldPersistence;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const SAVE_SCHEMA: &str = "newviso.world.save.v1";
const MAX_SAVE_BYTES: u64 = 128 * 1024 * 1024;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize, Deserialize)]
struct SaveEnvelope {
    schema: String,
    project_id: String,
    saved_at_unix_seconds: u64,
    checksum: String,
    /// Exact bytes are checksummed, independent of floating-point JSON re-encoding.
    payload: String,
}

#[derive(Serialize, Deserialize)]
struct SavePayload {
    world: Value,
    presentations: BTreeMap<String, WorldActorPresentationBinding>,
}

pub(super) struct WorldStartup {
    pub(super) world: LivingWorldRuntime,
    pub(super) presentations: BTreeMap<String, WorldActorPresentationBinding>,
    pub(super) persistence: Option<WorldPersistence>,
}

pub(super) struct WorldPersistence {
    root: PathBuf,
    path: PathBuf,
    project_id: String,
    settings: ProjectWorldPersistence,
    last_good: Option<Vec<u8>>,
    elapsed_since_attempt: f64,
    restored: bool,
    recovered_backup: bool,
    last_saved_world_seconds: Option<f64>,
    save_count: u64,
    last_error: Option<String>,
}

impl WorldStartup {
    pub(super) fn open(
        root: &Path,
        project_id: &str,
        settings: &ProjectWorldPersistence,
    ) -> Result<Self, String> {
        settings.validate()?;
        let mut startup = Self {
            world: LivingWorldRuntime::default(),
            presentations: BTreeMap::new(),
            persistence: None,
        };
        let Some(relative) = settings.save_path.as_deref() else {
            return Ok(startup);
        };
        let root = root.canonicalize().map_err(|e| format!("save root: {e}"))?;
        let path = root.join(relative.replace('\\', "/"));
        let mut store = WorldPersistence {
            root,
            path,
            project_id: project_id.into(),
            settings: settings.clone(),
            last_good: None,
            elapsed_since_attempt: 0.0,
            restored: false,
            recovered_backup: false,
            last_saved_world_seconds: None,
            save_count: 0,
            last_error: None,
        };
        store.check_path(&store.path)?;
        if settings.load_on_start && (store.path.exists() || store.backup_path().exists()) {
            let primary = store.read_valid(&store.path);
            let (world, presentations, bytes) = match primary {
                Ok(value) => value,
                Err(primary_error) => {
                    let result = store.read_valid(&store.backup_path()).map_err(|backup_error| {
                        format!("world save could not be restored; primary: {primary_error}; backup: {backup_error}")
                    })?;
                    store.recovered_backup = true;
                    store.last_error = Some(format!("recovered backup after: {primary_error}"));
                    result
                }
            };
            store.restored = true;
            store.last_good = Some(bytes);
            store.last_saved_world_seconds =
                world.runtime_state()["clock"]["world_seconds"].as_f64();
            startup.world = world;
            startup.presentations = presentations;
        }
        startup.persistence = Some(store);
        Ok(startup)
    }
}

impl WorldPersistence {
    fn backup_path(&self) -> PathBuf {
        PathBuf::from(format!("{}.bak", self.path.display()))
    }

    fn check_path(&self, path: &Path) -> Result<(), String> {
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

    fn read_valid(
        &self,
        path: &Path,
    ) -> Result<
        (
            LivingWorldRuntime,
            BTreeMap<String, WorldActorPresentationBinding>,
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
        Ok((world, payload.presentations, bytes))
    }

    fn atomic_write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
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

    fn save(
        &mut self,
        world: &LivingWorldRuntime,
        presentations: &BTreeMap<String, WorldActorPresentationBinding>,
    ) -> Result<(), String> {
        let payload = serde_json::to_string(&SavePayload {
            world: world.checkpoint()?,
            presentations: presentations.clone(),
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

    pub(super) fn runtime_state(&self) -> Value {
        json!({"enabled": true, "restored": self.restored, "recovered_backup": self.recovered_backup,
            "save_count": self.save_count, "last_saved_world_seconds": self.last_saved_world_seconds,
            "last_error": self.last_error})
    }
}

impl EngineApplication {
    pub(super) fn autosave_world(&mut self, dt: f32) {
        if !self.world_save_allowed {
            return;
        }
        let Some(store) = self.world_persistence.as_mut() else {
            return;
        };
        if dt.is_finite() && dt > 0.0 {
            store.elapsed_since_attempt += f64::from(dt);
        }
        let interval = store.settings.autosave_interval_seconds;
        if interval <= 0.0 || store.elapsed_since_attempt < interval {
            return;
        }
        store.elapsed_since_attempt = 0.0;
        if let Err(error) = store.save(&self.living_world, &self.world_actor_presentations) {
            host::warn("newviso.world", format!("world autosave failed: {error}"));
            store.last_error = Some(error);
        }
    }

    pub(super) fn save_world_on_shutdown(&mut self) {
        if !self.ready || !self.world_save_allowed {
            return;
        }
        let Some(store) = self.world_persistence.as_mut() else {
            return;
        };
        if !store.settings.save_on_shutdown {
            return;
        }
        if let Err(error) = store.save(&self.living_world, &self.world_actor_presentations) {
            host::warn(
                "newviso.world",
                format!("world shutdown save failed: {error}"),
            );
            store.last_error = Some(error);
        } else {
            host::info(
                "newviso.world",
                format!("world checkpoint saved: {}", store.path.display()),
            );
        }
    }
}

#[cfg(test)]
mod tests;
