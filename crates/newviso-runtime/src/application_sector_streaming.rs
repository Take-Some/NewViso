//! Independent on-disk scene sectors for large project maps.
//!
//! The canonical scene remains untouched. A small bootstrap scene contains
//! the spawn environment; the rest of the city's static meshes are read and
//! decoded on background workers, then installed in bounded owner-thread
//! slices. Geometry, GPU resources and collision preparation are still owned
//! by the ordinary NewViso resource streamer.

use super::*;

#[derive(Clone, Debug, Deserialize)]
struct SectorManifest {
    schema: String,
    sector_count: usize,
    entity_count: usize,
    bootstrap_count: usize,
    load_distance_m: f32,
    unload_distance_m: f32,
    load_workers: usize,
    max_entities_per_frame: usize,
    sectors: Vec<SectorIndex>,
}

#[derive(Clone, Debug, Deserialize)]
struct SectorIndex {
    id: String,
    file: String,
    count: usize,
    bounds_xz: [f32; 4],
    #[serde(default)]
    has_solid: bool,
}

#[derive(Deserialize)]
struct SectorFile {
    schema: String,
    entities: Vec<StreamedMesh>,
}

#[derive(Deserialize)]
struct StreamedMesh {
    name: String,
    kind: String,
    transform: StreamTransform,
    mesh: StreamMesh,
    lod: StreamLod,
    #[serde(default)]
    collider: StreamCollider,
}

#[derive(Deserialize)]
struct StreamTransform {
    position: [f32; 3],
    #[serde(default)]
    rotation_degrees: Option<[f32; 3]>,
    #[serde(default)]
    scale: Option<[f32; 3]>,
}

#[derive(Deserialize)]
struct StreamMesh {
    asset: String,
}

#[derive(Deserialize)]
struct StreamLod {
    visible_distance: f32,
    stream_distance: f32,
    #[serde(default)]
    fade_range: f32,
}

#[derive(Default, Deserialize)]
struct StreamCollider {
    #[serde(default)]
    solid: bool,
    #[serde(default)]
    local_bounds: Option<StreamLocalBounds>,
}

#[derive(Deserialize)]
struct StreamLocalBounds {
    min: [f32; 3],
    max: [f32; 3],
}

impl StreamedMesh {
    fn into_desc(self) -> Result<(String, SceneRuntimeEntityDesc, Option<([f32; 3], [f32; 3])>), String> {
        if self.kind != "mesh" || self.name.is_empty() || self.mesh.asset.trim().is_empty() {
            return Err("invalid sector mesh record".to_owned());
        }
        let mut desc = SceneRuntimeEntityDesc {
            visual: SceneRuntimeVisualKind::None,
            asset_ref: Some(self.mesh.asset),
            position: self.transform.position,
            rotation_degrees: self.transform.rotation_degrees.unwrap_or([0.0; 3]),
            scale: self.transform.scale.unwrap_or([1.0; 3]),
            solid: self.collider.solid,
            bounds_half_extent: self.transform.scale.unwrap_or([1.0; 3])
                .map(|scale| (scale.abs() * 0.5).max(0.1)),
            visible_distance: self.lod.visible_distance,
            stream_distance: self.lod.stream_distance,
            fade_range: self.lod.fade_range,
            ..Default::default()
        };
        let bounds = self.collider.local_bounds.map(|local| (local.min, local.max));
        if let Some((min, max)) = bounds {
            if min.iter().chain(max.iter()).any(|v| !v.is_finite())
                || (0..3).any(|axis| min[axis] > max[axis])
            {
                return Err(format!("invalid sector collider bounds on '{}'", self.name));
            }
            // Correct conservative AABB for collision-only models with distant
            // mesh origins; actual rotated bounds are restored by Scene3dRuntime.
            for axis in 0..3 {
                desc.bounds_half_extent[axis] = ((max[axis] - min[axis]).abs() * 0.5)
                    .max(0.1).min(1.0e6);
            }
        }
        Ok((self.name, desc, bounds))
    }
}

enum SectorStage {
    Reading,
    Ready {
        entities: Vec<StreamedMesh>,
        cursor: usize,
        names: Vec<String>,
        stable_ids: Vec<u64>,
        solid_ids: Vec<u64>,
    },
    Resident {
        names: Vec<String>,
        stable_ids: Vec<u64>,
        solid_ids: Vec<u64>,
    },
    Evicting {
        names: Vec<String>,
        stable_ids: Vec<u64>,
        cursor: usize,
    },
    Failed {
        retry_frame: u64,
    },
}

struct SectorCompletion {
    id: String,
    result: Result<Vec<StreamedMesh>, String>,
}

struct SectorLoadJob {
    id: String,
    path: PathBuf,
    expected_count: usize,
}

fn read_sector_file(path: &Path, expected_count: usize) -> Result<Vec<StreamedMesh>, String> {
    let raw = fs::read(path)
        .map_err(|error| format!("read sector '{}': {error}", path.display()))?;
    let file: SectorFile = serde_json::from_slice(&raw)
        .map_err(|error| format!("parse sector '{}': {error}", path.display()))?;
    if file.schema != "newviso.scene.sector.v1" || file.entities.len() != expected_count {
        return Err(format!("invalid entity count/schema in '{}'", path.display()));
    }
    Ok(file.entities)
}

pub(super) struct SectorStreamer {
    project_root: PathBuf,
    manifest: SectorManifest,
    receiver: std::sync::mpsc::Receiver<SectorCompletion>,
    job_sender: Option<std::sync::mpsc::Sender<SectorLoadJob>>,
    workers: Vec<std::thread::JoinHandle<()>>,
    stages: BTreeMap<String, SectorStage>,
    frame: u64,
    previous_focus: Option<[f32; 3]>,
    teleport_guard: bool,
    last_log_frame: u64,
}

fn secure_project_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty() || path.is_absolute() || path.components().any(|component| {
        !matches!(component, std::path::Component::Normal(_))
    }) {
        return Err(format!("unsafe sector asset path '{relative}'"));
    }
    Ok(root.join(path))
}

fn sector_distance(index: &SectorIndex, focus: [f32; 3]) -> f32 {
    let [min_x, min_z, max_x, max_z] = index.bounds_xz;
    let dx = (min_x - focus[0]).max(focus[0] - max_x).max(0.0);
    let dz = (min_z - focus[2]).max(focus[2] - max_z).max(0.0);
    (dx * dx + dz * dz).sqrt()
}

impl SectorStreamer {
    pub(super) fn from_project(
        root: PathBuf,
        config: &Value,
    ) -> Result<Option<Self>, String> {
        let Some(manifest_path) = config
            .get("manifest")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
        else {
            return Ok(None);
        };
        let manifest_file = secure_project_path(&root, manifest_path)?;
        let bytes = fs::read(&manifest_file)
            .map_err(|e| format!("read streamed sector manifest '{}': {e}", manifest_file.display()))?;
        let manifest: SectorManifest = serde_json::from_slice(&bytes)
            .map_err(|e| format!("parse streamed sector manifest: {e}"))?;
        if manifest.schema != "newviso.scene.sectors.v1"
            || manifest.sector_count != manifest.sectors.len()
            || manifest.bootstrap_count == 0
            || manifest.entity_count <= manifest.bootstrap_count
            || !(32.0..=10000.0).contains(&manifest.load_distance_m)
            || manifest.unload_distance_m <= manifest.load_distance_m + 64.0
            || !(1..=8).contains(&manifest.load_workers)
            || !(1..=512).contains(&manifest.max_entities_per_frame)
        {
            return Err("invalid streamed sector manifest parameters".to_owned());
        }
        let mut keys = BTreeSet::new();
        for entry in &manifest.sectors {
            secure_project_path(&root, &entry.file)?;
            if !keys.insert(entry.id.clone())
                || entry.count == 0
                || entry.bounds_xz.iter().any(|v| !v.is_finite())
                || entry.bounds_xz[0] > entry.bounds_xz[2]
                || entry.bounds_xz[1] > entry.bounds_xz[3]
            {
                return Err(format!("invalid/duplicate scene sector '{}'", entry.id));
            }
        }
        let (completion_sender, receiver) = std::sync::mpsc::channel();
        let (job_sender, jobs) = std::sync::mpsc::channel::<SectorLoadJob>();
        let jobs = std::sync::Arc::new(std::sync::Mutex::new(jobs));
        let mut workers = Vec::with_capacity(manifest.load_workers);
        for index in 0..manifest.load_workers {
            let jobs = std::sync::Arc::clone(&jobs);
            let completion_sender = completion_sender.clone();
            let worker = std::thread::Builder::new()
                .name(format!("newviso-sector-io-{index}"))
                .spawn(move || loop {
                    // Only queue receive is serialized; decoding runs independently.
                    let job = match jobs.lock() {
                        Ok(queue) => match queue.recv() {
                            Ok(job) => job,
                            Err(_) => break,
                        },
                        Err(_) => break,
                    };
                    let result = read_sector_file(&job.path, job.expected_count);
                    if completion_sender.send(SectorCompletion { id: job.id, result }).is_err() {
                        break;
                    }
                });
            match worker {
                Ok(handle) => workers.push(handle),
                Err(error) => {
                    drop(job_sender);
                    for handle in workers {
                        let _ = handle.join();
                    }
                    return Err(format!("spawn sector I/O worker {index}: {error}"));
                }
            }
        }
        host::info("newviso.sectors", format!(
            "sectorized scene initialized sectors={} boot_entities={} total={} radius={:.0}/{:.0}m io_workers={}",
            manifest.sector_count, manifest.bootstrap_count, manifest.entity_count,
            manifest.load_distance_m, manifest.unload_distance_m, workers.len()
        ));
        Ok(Some(Self {
            project_root: root,
            manifest,
            receiver,
            job_sender: Some(job_sender),
            workers,
            stages: BTreeMap::new(),
            frame: 0,
            previous_focus: None,
            // Hold simulation until spawn-adjacent solid sectors are ready.
            teleport_guard: true,
            last_log_frame: 0,
        }))
    }

    pub(super) fn collision_guard_active(&self) -> bool {
        self.teleport_guard
    }

    fn schedule(&mut self, focus: [f32; 3]) {
        let mut wanted = self.manifest.sectors.iter()
            .map(|entry| (sector_distance(entry, focus), entry.id.clone(), entry.file.clone(), entry.count))
            .filter(|(distance, id, _, _)| {
                *distance <= self.manifest.load_distance_m && !self.stages.contains_key(id)
            })
            .collect::<Vec<_>>();
        wanted.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut available_slots = self.manifest.load_workers.saturating_sub(
            self.stages.values().filter(|state| matches!(state, SectorStage::Reading)).count()
        );
        for (_, id, relative_path, expected_count) in wanted {
            if available_slots == 0 {
                break;
            }
            let Ok(path) = secure_project_path(&self.project_root, &relative_path) else {
                continue;
            };
            let job = SectorLoadJob { id: id.clone(), path, expected_count };
            if self.job_sender.as_ref().is_some_and(|sender| sender.send(job).is_ok()) {
                self.stages.insert(id, SectorStage::Reading);
                available_slots -= 1;
            } else {
                host::warn("newviso.sectors", format!("sector worker pool unavailable for '{id}'"));
                self.stages.insert(id, SectorStage::Failed {
                    retry_frame: self.frame.saturating_add(120),
                });
            }
        }
    }

    fn drain_completions(&mut self, focus: [f32; 3]) {
        while let Ok(completion) = self.receiver.try_recv() {
            if !matches!(self.stages.get(&completion.id), Some(SectorStage::Reading)) {
                continue;
            }
            let Some(index) = self.manifest.sectors.iter()
                .find(|entry| entry.id == completion.id)
            else {
                continue;
            };
            // A pending read is allowed to finish after a rapid teleport, but
            // the result is discarded instead of inflating memory at old focus.
            if sector_distance(index, focus) > self.manifest.unload_distance_m {
                self.stages.remove(&completion.id);
                continue;
            }
            match completion.result {
                Ok(entities) => {
                    self.stages.insert(completion.id, SectorStage::Ready {
                        entities, cursor: 0, names: Vec::new(), stable_ids: Vec::new(),
                        solid_ids: Vec::new(),
                    });
                }
                Err(error) => {
                    host::warn("newviso.sectors", error);
                    self.stages.insert(completion.id, SectorStage::Failed {
                        retry_frame: self.frame.saturating_add(180),
                    });
                }
            }
        }
    }

    fn commit(&mut self, app: &mut EngineApplication, focus: [f32; 3]) -> Result<(), String> {
        let started = std::time::Instant::now();
        let deadline = std::time::Duration::from_micros(1500);
        let mut total = 0;
        let mut prioritized = self.manifest.sectors.iter()
            .map(|entry| (sector_distance(entry, focus), entry.id.clone()))
            .filter(|(distance,id)| *distance <= self.manifest.unload_distance_m
                && matches!(self.stages.get(id), Some(SectorStage::Ready { .. })))
            .collect::<Vec<_>>();
        prioritized.sort_by(|a,b| a.0.total_cmp(&b.0).then_with(||a.1.cmp(&b.1)));
        for (_, id) in prioritized {
            if total >= self.manifest.max_entities_per_frame || started.elapsed() >= deadline {break;}
            let mut finished = false;
            if let Some(SectorStage::Ready { entities, cursor, names, stable_ids, solid_ids }) = self.stages.get_mut(&id) {
                while *cursor < entities.len()
                    && total < self.manifest.max_entities_per_frame
                    && started.elapsed() < deadline
                {
                    // Take record ownership without cloning strings and material paths.
                    let record = std::mem::replace(&mut entities[*cursor], StreamedMesh::placeholder());
                    let (name, desc, bounds) = record.into_desc()?;
                    let is_solid = desc.solid;
                    let stable_id = app.scene.upsert_streamed_static_mesh(&name, desc, bounds)?;
                    names.push(name);
                    stable_ids.push(stable_id);
                    if is_solid { solid_ids.push(stable_id); }
                    *cursor += 1;
                    total += 1;
                }
                finished = *cursor >= entities.len();
            }
            if finished {
                if let Some(SectorStage::Ready { names, stable_ids, solid_ids, .. }) = self.stages.remove(&id) {
                    self.stages.insert(id, SectorStage::Resident { names, stable_ids, solid_ids });
                }
            }
        }
        Ok(())
    }

    fn evict(&mut self, app: &mut EngineApplication, focus: [f32; 3]) -> Result<(), String> {
        let ids = self.manifest.sectors.iter()
            .filter(|entry| sector_distance(entry, focus) > self.manifest.unload_distance_m)
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            if let Some(state) = self.stages.remove(&id) {
                match state {
                    SectorStage::Resident { names, stable_ids, .. } => {
                        self.stages.insert(id, SectorStage::Evicting {
                            names, stable_ids, cursor: 0
                        });
                    }
                    SectorStage::Ready { names, stable_ids, .. } => {
                        self.stages.insert(id, SectorStage::Evicting {
                            names, stable_ids, cursor: 0
                        });
                    }
                    SectorStage::Reading => {
                        self.stages.insert(id, SectorStage::Reading);
                    }
                    other => {
                        self.stages.insert(id, other);
                    }
                }
            }
        }
        let started = std::time::Instant::now();
        let mut done = 0;
        let keys = self.stages.iter()
            .filter_map(|(id, stage)| matches!(stage, SectorStage::Evicting{..}).then_some(id.clone()))
            .collect::<Vec<_>>();
        for key in keys {
            let mut finished = false;
            if let Some(SectorStage::Evicting { names, stable_ids, cursor }) = self.stages.get_mut(&key) {
                while *cursor < names.len()
                    && done < 64
                    && started.elapsed() < std::time::Duration::from_micros(1500)
                {
                    let stable_id = stable_ids[*cursor];
                    if let Some(address) = app.scene_stream_claims.remove(&stable_id) {
                        app.asset_streamer.release(EngineApplication::scene_stream_owner(stable_id), &address);
                    }
                    app.scene_stream_priorities.remove(&stable_id);
                    app.scene_materialization_pending.remove(&stable_id);
                    app.release_scene_aux_claims(stable_id);
                    app.dematerialize_scene_asset(stable_id)?;
                    app.scene.remove_runtime_entity(&names[*cursor])?;
                    *cursor += 1;
                    done += 1;
                }
                finished = *cursor >= names.len();
            }
            if finished { self.stages.remove(&key); }
            if done >= 64 {break;}
        }
        Ok(())
    }

    fn update_teleport_guard(&mut self, app: &EngineApplication, focus: [f32; 3]) {
        let mut safe = true;
        for entry in &self.manifest.sectors {
            if !entry.has_solid || sector_distance(entry, focus) > 120.0 {
                continue;
            }
            match self.stages.get(&entry.id) {
                Some(SectorStage::Resident { solid_ids, .. }) => {
                    if solid_ids.iter().any(|id| {
                        let bounds=app.scene.streamed_entity_bounds_xz(*id);
                        let near=bounds.is_some_and(|b| {
                            let dx=(b[0]-focus[0]).max(focus[0]-b[2]).max(0.0);
                            let dz=(b[1]-focus[2]).max(focus[2]-b[3]).max(0.0);
                            dx*dx+dz*dz <= 120.0*120.0
                        });
                        near && (app.scene_materialization_pending.contains(id)
                            || !app.scene_stream_claims.contains_key(id))
                    }) {
                        safe = false;
                        break;
                    }
                }
                _ => {
                    safe = false;
                    break;
                }
            }
        }
        if safe {
            self.teleport_guard = false;
        }
    }

    pub(super) fn tick(&mut self, app: &mut EngineApplication) -> Result<(), String> {
        self.frame = self.frame.saturating_add(1);
        // A missing/broken sector must not create a busy failure/retry loop.
        self.stages.retain(|_, state| !matches!(
            state, SectorStage::Failed { retry_frame } if self.frame >= *retry_frame
        ));
        let focus = app.scene.focus_position();
        if let Some(old) = self.previous_focus {
            let displacement = ((focus[0]-old[0]).powi(2)
                + (focus[2]-old[2]).powi(2)).sqrt();
            if displacement > 200.0 {
                self.teleport_guard = true;
                host::warn("newviso.sectors", format!(
                    "fast travel displacement {:.1} m: holding physics until nearby solid sectors ready",
                    displacement
                ));
            }
        }
        self.previous_focus = Some(focus);
        self.drain_completions(focus);
        self.schedule(focus);
        self.commit(app, focus)?;
        self.evict(app, focus)?;
        if self.teleport_guard {
            self.update_teleport_guard(app, focus);
        }
        if self.frame.saturating_sub(self.last_log_frame) >= 600 {
            self.last_log_frame = self.frame;
            let ready = self.stages.values().filter(|s|matches!(s, SectorStage::Resident{..})).count();
            host::debug("newviso.sectors", format!(
                "sector streaming focus=[{:.0},{:.0}] resident={}/{} io={} guard={}",
                focus[0],focus[2],ready,self.manifest.sector_count,
                self.stages.values().filter(|s| matches!(s, SectorStage::Reading)).count(),self.teleport_guard
            ));
        }
        Ok(())
    }
}

impl Drop for SectorStreamer {
    fn drop(&mut self) {
        // Closing the job queue wakes idle workers blocked in recv().
        self.job_sender.take();
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

impl StreamedMesh {
    /// Temporary placeholder used to move decoded records out of a sector
    /// without cloning or serializing them on the game thread.
    fn placeholder() -> Self {
        Self {
            name: String::new(), kind: String::new(),
            transform: StreamTransform {
                position: [0.0; 3], rotation_degrees: None, scale: None,
            },
            mesh: StreamMesh { asset: String::new() },
            lod: StreamLod {
                visible_distance: 1.0, stream_distance: 1.0, fade_range: 0.0,
            },
            collider: StreamCollider::default(),
        }
    }
}

impl EngineApplication {
    pub(super) fn tick_scene_sectors(&mut self) -> Result<(), String> {
        if let Some(mut streamer) = self.sector_streamer.take() {
            let result = streamer.tick(self);
            self.sector_streamer = Some(streamer);
            result?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_sector_distance_and_hysteresis_are_consistent() {
        let item = SectorIndex {
            id: "sector".into(),file:"scene.json".into(),
            count:1,bounds_xz:[256.0,-256.0,768.0,256.0],has_solid:true
        };
        assert_eq!(sector_distance(&item, [256.0,0.0,0.0]),0.0);
        assert_eq!(sector_distance(&item, [0.0,0.0,0.0]),256.0);
        assert_eq!(sector_distance(&item, [1000.0,0.0,0.0]),232.0);
    }

    #[test]
    fn sector_file_paths_cannot_escape_project_root() {
        let root = Path::new("C:/Projects/Current");
        assert!(secure_project_path(root,"scenes/streamed-v1/sectors/c_0_0.json").is_ok());
        assert!(secure_project_path(root,"../OtherProject/scene.json").is_err());
        assert!(secure_project_path(root,"/absolute/scene.json").is_err());
    }

    #[test]
    fn authored_collision_bounds_survive_sector_mesh_conversion() {
        let json = json!({
            "name":"road_collision","kind":"mesh",
            "mesh":{"asset":"models/world/road.ybn"},
            "transform":{"position":[100.0,2.0,30.0],"scale":[1.0,1.0,1.0]},
            "collider":{"solid":true,"local_bounds":{"min":[-30.0,-2.0,-180.0],"max":[30.0,2.0,180.0]}},
            "lod":{"visible_distance":300.0,"stream_distance":450.0,"fade_range":20.0}
        });
        let entity:StreamedMesh=serde_json::from_value(json).unwrap();
        let (_,desc,bounds)=entity.into_desc().unwrap();
        assert!(desc.solid);
        assert_eq!(bounds.unwrap().0,[-30.0,-2.0,-180.0]);
        assert_eq!(desc.position,[100.0,2.0,30.0]);
        assert_eq!(desc.stream_distance,450.0);
    }

    #[test]
    fn persistent_sector_pool_executes_queued_jobs() {
        let root = std::env::temp_dir().join(format!(
            "newviso-sector-workers-{}-{:?}", std::process::id(), std::thread::current().id()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("sector.json"), r#"{"schema":"newviso.scene.sector.v1","entities":[{"name":"a","kind":"mesh","mesh":{"asset":"test.ydr"},"transform":{"position":[0,0,0]},"lod":{"visible_distance":100,"stream_distance":200}}]}"#).unwrap();
        fs::write(root.join("index.json"), r#"{"schema":"newviso.scene.sectors.v1","sector_count":1,"entity_count":2,"bootstrap_count":1,"load_distance_m":400,"unload_distance_m":700,"load_workers":2,"max_entities_per_frame":4,"sectors":[{"id":"s0","file":"sector.json","count":1,"bounds_xz":[-1,-1,1,1],"has_solid":true}]}"#).unwrap();
        let mut streamer = SectorStreamer::from_project(root.clone(), &json!({"manifest":"index.json"})).unwrap().unwrap();
        assert_eq!(streamer.workers.len(), 2);
        assert!(streamer.collision_guard_active());
        streamer.schedule([0.0,0.0,0.0]);
        let completed = streamer.receiver.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        assert_eq!(completed.id, "s0");
        assert_eq!(completed.result.unwrap().len(), 1);
        drop(streamer);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sector_reader_rejects_mismatched_counts_and_schema() {
        let path = std::env::temp_dir().join(format!(
            "newviso-sector-reader-{}.json", std::process::id()
        ));
        fs::write(&path, r#"{"schema":"newviso.scene.sector.v1","entities":[]}"#).unwrap();
        assert!(read_sector_file(&path, 0).is_ok());
        assert!(read_sector_file(&path, 1).is_err());
        fs::write(&path, r#"{"schema":"incorrect","entities":[]}"#).unwrap();
        assert!(read_sector_file(&path, 0).is_err());
        fs::remove_file(path).unwrap();
    }
}
