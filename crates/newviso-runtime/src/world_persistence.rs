use super::*;
use newviso_project::ProjectWorldPersistence;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

mod storage;

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
    /// Added after world-save v1 shipped. Missing field in older saves decodes as
    /// null and therefore restores an empty item runtime.
    #[serde(default)]
    items: Value,
    #[serde(default)]
    climate: Value,
    #[serde(default)]
    cold: Value,
}

pub(super) struct WorldStartup {
    pub(super) world: LivingWorldRuntime,
    pub(super) presentations: BTreeMap<String, WorldActorPresentationBinding>,
    pub(super) items: ItemsRuntime,
    pub(super) climate: Value,
    pub(super) cold: Value,
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
            items: ItemsRuntime::default(),
            climate: Value::Null,
            cold: Value::Null,
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
            let (world, presentations, items, climate, cold, bytes) = match primary {
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
            startup.items = items;
            startup.climate = climate;
            startup.cold = cold;
        }
        startup.persistence = Some(store);
        Ok(startup)
    }
}

impl WorldPersistence {
    /// Admit a save attempt before building any potentially large snapshots.
    fn autosave_due(&mut self,dt:f32)->bool{
        if dt.is_finite()&&dt>0.0{self.elapsed_since_attempt+=f64::from(dt);}
        let interval=self.settings.autosave_interval_seconds;
        if interval<=0.0||self.elapsed_since_attempt<interval{return false;}
        self.elapsed_since_attempt=0.0;
        true
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
        let Some(store)=self.world_persistence.as_mut() else{return;};
        if !store.autosave_due(dt){return;}
        // Climate and the full cold field are materialized only for an admitted save.
        let climate=self.climate_checkpoint();
        let cold=self.cold.checkpoint();
        let Some(store)=self.world_persistence.as_mut() else{return;};
        if let Err(error) = store.save(
            &self.living_world,
            &self.world_actor_presentations,
            &self.items,
            &climate,
            &cold,
        ) {
            host::warn("newviso.world", format!("world autosave failed: {error}"));
            store.last_error = Some(error);
        }
    }

    pub(super) fn save_world_on_shutdown(&mut self) {
        if !self.ready || !self.world_save_allowed {
            return;
        }
        if !self.world_persistence.as_ref().is_some_and(|store|store.settings.save_on_shutdown){
            return;
        }
        let climate=self.climate_checkpoint();
        let cold=self.cold.checkpoint();
        let Some(store)=self.world_persistence.as_mut() else{return;};
        if let Err(error) = store.save(
            &self.living_world,
            &self.world_actor_presentations,
            &self.items,
            &climate,
            &cold,
        ) {
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
