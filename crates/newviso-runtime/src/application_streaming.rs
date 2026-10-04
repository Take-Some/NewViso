use super::*;

fn material_binding_uses_local_base_color(
    binding: &newviso_materials::MaterialTextureBinding,
) -> bool {
    let Some(texture_name) = binding.texture_name.as_deref() else {
        return false;
    };
    let texture_name = texture_name.trim().to_ascii_lowercase();
    // These are authored shader/global placeholders, not dictionary-local
    // albedo entries even when they happen to occupy DiffuseSampler.
    if matches!(
        texture_name.as_str(),
        "givemechecker" | "long_hair_noise" | "env_smooth_concrete2"
    ) || texture_name.starts_with("enveff_")
    {
        return false;
    }

    let role = binding.slot.trim().to_ascii_lowercase();
    if matches!(role.as_str(), "base_color" | "albedo" | "diffuse" | "base") {
        return true;
    }
    role == "generic" && texture_name.contains("_diff_")
}

fn material_binding_blocks_model_materialization(
    binding: &newviso_materials::MaterialTextureBinding,
) -> bool {
    binding.required || material_binding_uses_local_base_color(binding)
}

enum ScenePrepareTask {
    Model {
        model: Arc<ModelResource>,
    },
    Collision {
        stable_id: u64,
        asset_id: u64,
        collision: Arc<CollisionMeshResource>,
        position: [f32; 3],
        rotation_degrees: [f32; 3],
        scale: [f32; 3],
    },
}

enum ScenePrepareCompletion {
    Model {
        model_id: u64,
        result: Result<newviso_scene::ScenePreparedModelGeometry, String>,
    },
    Collision {
        stable_id: u64,
        asset_id: u64,
        result: Result<application_physics::PreparedStreamedCollision, String>,
    },
}

struct ScenePrepareShared {
    queue: std::sync::Mutex<VecDeque<ScenePrepareTask>>,
    wake: std::sync::Condvar,
    stopping: std::sync::atomic::AtomicBool,
}

pub(super) struct ScenePreparePool {
    shared: Arc<ScenePrepareShared>,
    completed: std::sync::mpsc::Receiver<ScenePrepareCompletion>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl ScenePreparePool {
    pub(super) fn new(stream_parallelism: usize) -> Result<Self, String> {
        // Keep at least one logical core for gameplay/render and avoid letting
        // model/collision conversion compete with the I/O/decode pool for every core.
        let available = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(4);
        let worker_count = stream_parallelism
            .div_ceil(2)
            .max(1)
            .min(available.saturating_sub(2).max(1))
            .min(4);
        let shared = Arc::new(ScenePrepareShared {
            queue: std::sync::Mutex::new(VecDeque::new()),
            wake: std::sync::Condvar::new(),
            stopping: std::sync::atomic::AtomicBool::new(false),
        });
        let (tx, completed) = std::sync::mpsc::channel();
        let mut workers = Vec::with_capacity(worker_count);
        for index in 0..worker_count {
            let shared = Arc::clone(&shared);
            let tx = tx.clone();
            let worker = std::thread::Builder::new()
                .name(format!("newviso-scene-prepare-{index}"))
                .spawn(move || scene_prepare_worker(shared, tx))
                .map_err(|error| format!("failed to spawn scene prepare worker: {error}"))?;
            workers.push(worker);
        }
        Ok(Self {
            shared,
            completed,
            workers,
        })
    }

    fn submit(&self, task: ScenePrepareTask) {
        self.shared
            .queue
            .lock()
            .expect("scene prepare queue poisoned")
            .push_back(task);
        self.shared.wake.notify_one();
    }

    fn try_recv(&self) -> Option<ScenePrepareCompletion> {
        self.completed.try_recv().ok()
    }

    fn worker_count(&self) -> usize {
        self.workers.len()
    }
}

impl Drop for ScenePreparePool {
    fn drop(&mut self) {
        self.shared
            .stopping
            .store(true, std::sync::atomic::Ordering::Release);
        self.shared.wake.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn scene_prepare_worker(
    shared: Arc<ScenePrepareShared>,
    completed: std::sync::mpsc::Sender<ScenePrepareCompletion>,
) {
    loop {
        let task = {
            let mut queue = shared.queue.lock().expect("scene prepare queue poisoned");
            loop {
                if let Some(task) = queue.pop_front() {
                    break Some(task);
                }
                if shared.stopping.load(std::sync::atomic::Ordering::Acquire) {
                    break None;
                }
                queue = shared
                    .wake
                    .wait(queue)
                    .expect("scene prepare queue poisoned");
            }
        };
        let Some(task) = task else {
            return;
        };

        let completion = match task {
            ScenePrepareTask::Model { model } => ScenePrepareCompletion::Model {
                model_id: model.id.0,
                result: newviso_scene::prepare_model_geometry(&model),
            },
            ScenePrepareTask::Collision {
                stable_id,
                asset_id,
                collision,
                position,
                rotation_degrees,
                scale,
            } => ScenePrepareCompletion::Collision {
                stable_id,
                asset_id,
                result: PhysicsRuntime::prepare_streamed_collision(
                    stable_id,
                    &collision,
                    position,
                    rotation_degrees,
                    scale,
                ),
            },
        };
        if completed.send(completion).is_err() {
            return;
        }
    }
}

impl EngineApplication {
    pub(super) fn scene_stream_owner(stable_id: u64) -> StreamingOwnerId {
        StreamingOwnerId::from_label(&format!("newviso.scene.entity.{stable_id}"))
    }

    fn drain_scene_prepare_completions(&mut self) {
        while let Some(completion) = self.scene_prepare_pool.try_recv() {
            match completion {
                ScenePrepareCompletion::Model { model_id, result } => {
                    self.scene_model_prepare_in_flight.remove(&model_id);
                    self.scene_prepared_models
                        .insert(model_id, result.map(Arc::new));
                    let waiters = self
                        .scene_model_prepare_waiters
                        .remove(&model_id)
                        .unwrap_or_default();
                    for stable_id in waiters {
                        if self.scene_materialization_pending.contains(&stable_id) {
                            self.queue_scene_materialization(stable_id);
                        }
                    }
                }
                ScenePrepareCompletion::Collision {
                    stable_id,
                    asset_id,
                    result,
                } => {
                    self.scene_collision_prepare_in_flight
                        .remove(&(stable_id, asset_id));
                    let still_current = self
                        .scene_stream_claims
                        .get(&stable_id)
                        .and_then(|address| {
                            self.asset_streamer.get::<CollisionMeshResource>(address)
                        })
                        .is_some_and(|collision| collision.id.0 == asset_id);
                    if still_current {
                        self.scene_prepared_collisions
                            .insert((stable_id, asset_id), result);
                        if self.scene_materialization_pending.contains(&stable_id) {
                            self.queue_scene_materialization(stable_id);
                        }
                    }
                }
            }
        }
    }

    fn release_scene_aux_claims(&mut self, stable_id: u64) {
        if let Some(addresses) = self.scene_stream_aux_claims.remove(&stable_id) {
            let owner = Self::scene_stream_owner(stable_id);
            for address in addresses {
                self.asset_streamer.release(owner, &address);
            }
        }
    }

    fn dematerialize_scene_asset(&mut self, stable_id: u64) -> Result<(), String> {
        self.scene_prepared_collisions
            .retain(|(entity, _), _| *entity != stable_id);
        for waiters in self.scene_model_prepare_waiters.values_mut() {
            waiters.remove(&stable_id);
        }
        self.scene.remove_entity_model(stable_id);
        self.physical_characters
            .remove_navigation_tile(&format!("scene.collider.{stable_id}"));
        if let Some(physics) = self.physics.as_mut() {
            physics.remove_streamed_collision(stable_id);
        }
        self.scene.mark_entity_unloaded(stable_id)
    }

    fn materialize_scene_asset(
        &mut self,
        stable_id: u64,
        address: &AssetAddress,
    ) -> Result<(), String> {
        let model = self.asset_streamer.get::<ModelResource>(address);
        let collision = self.asset_streamer.get::<CollisionMeshResource>(address);

        if let Some(model) = model.as_deref() {
            // Material storage is a policy detail. Built-in and external slots are
            // both resolved to the same MaterialResource contract before Scene sees them.
            let context = self.scene.entity_asset_binding_context(stable_id);
            let dictionary = context.texture_dictionary.as_deref();
            let request_priority = self
                .asset_streamer
                .snapshot(address)
                .map(|snapshot| snapshot.effective_priority)
                .unwrap_or(1.0)
                .max(0.0);
            let owner = Self::scene_stream_owner(stable_id);

            let resolve_material_texture_address =
                |binding: &newviso_materials::MaterialTextureBinding| -> Result<Option<AssetAddress>, String> {
                    if let Some(texture) = binding.texture.as_ref() {
                        return Ok(Some(texture.address().clone()));
                    }
                    let Some(texture_name) = binding.texture_name.as_deref() else {
                        return Ok(None);
                    };
                    // Most unresolved optional inputs are authored globals
                    // (environment, long_hair_noise, givemechecker, etc.) and
                    // must not be reinterpreted as entity-local YTD entries.
                    // Base-color samplers are the exception: legacy/streamed-ped
                    // cooks may know the texture name but leave the direct ref
                    // unresolved. If the entity explicitly supplies a texture
                    // dictionary, resolve that albedo there and treat it as a
                    // materialization dependency.
                    if !binding.required && !material_binding_uses_local_base_color(binding) {
                        return Ok(None);
                    }
                    let Some(dictionary) = dictionary else {
                        return Ok(None);
                    };
                    let dictionary = AssetAddress::parse(dictionary).map_err(|error| {
                        format!(
                            "scene entity {} texture_dictionary '{}' is invalid: {error}",
                            stable_id, dictionary
                        )
                    })?;
                    if dictionary.entry().is_some() {
                        return Err(format!(
                            "scene entity {} texture_dictionary '{}' must name a dictionary, not an @entry",
                            stable_id,
                            dictionary.canonical()
                        ));
                    }
                    Ok(Some(AssetAddress::parse(&format!(
                        "{}@{}",
                        dictionary.logical_path(),
                        texture_name.to_ascii_lowercase()
                    ))?))
                };

            let mut resolved_materials = Vec::with_capacity(model.material_slots.len());
            let mut pending_material_dependencies = 0usize;
            for slot in &model.material_slots {
                let material = match &slot.material {
                    newviso_model::ModelMaterialBinding::BuiltIn(material) => {
                        Some(material.clone())
                    }
                    newviso_model::ModelMaterialBinding::External(material_ref) => {
                        let material_address = material_ref.address();
                        match self
                            .asset_streamer
                            .get::<newviso_materials::MaterialResource>(material_address)
                        {
                            Some(material) => Some(material),
                            None => {
                                self.asset_streamer.request(
                                    owner,
                                    material_address.clone(),
                                    StreamingClaim::new(request_priority),
                                )?;
                                self.scene_stream_aux_claims
                                    .entry(stable_id)
                                    .or_default()
                                    .insert(material_address.clone());
                                pending_material_dependencies =
                                    pending_material_dependencies.saturating_add(1);
                                None
                            }
                        }
                    }
                };

                if let Some(material) = material.as_ref() {
                    for texture in &material.textures {
                        let Some(texture_address) = resolve_material_texture_address(texture)?
                        else {
                            if material_binding_blocks_model_materialization(texture) {
                                pending_material_dependencies =
                                    pending_material_dependencies.saturating_add(1);
                            }
                            continue;
                        };
                        if self
                            .asset_streamer
                            .get::<newviso_textures::TextureResource>(&texture_address)
                            .is_none()
                        {
                            self.asset_streamer.request(
                                owner,
                                texture_address.clone(),
                                StreamingClaim::new(request_priority),
                            )?;
                            self.scene_stream_aux_claims
                                .entry(stable_id)
                                .or_default()
                                .insert(texture_address);
                            if material_binding_blocks_model_materialization(texture) {
                                pending_material_dependencies =
                                    pending_material_dependencies.saturating_add(1);
                            }
                        }
                    }
                }
                resolved_materials.push(material);
            }

            if pending_material_dependencies > 0 {
                host::debug(
                    "newviso.assets.streaming",
                    format!(
                        "defer model materialization entity={} asset='{}' pending_material_dependencies={}",
                        stable_id,
                        address.canonical(),
                        pending_material_dependencies
                    ),
                );
                return Ok(());
            }

            let resolved_scene_materials = resolved_materials
                .iter()
                .enumerate()
                .map(|(slot_index, material)| {
                    let material = material.clone().ok_or_else(|| {
                        format!(
                            "model '{}' material slot {} resolved without MaterialResource",
                            model.name, slot_index
                        )
                    })?;
                    let resolve_role = |role: &str| -> Result<
                        Option<Arc<newviso_textures::TextureResource>>,
                        String,
                    > {
                        let Some(binding) = material.textures.iter().find(|binding| {
                            let name = binding.slot.trim().to_ascii_lowercase();
                            name == role
                                || (role == "base_color"
                                    && matches!(name.as_str(), "albedo" | "diffuse" | "base"))
                                || (role == "normal"
                                    && matches!(name.as_str(), "normal_map" | "normals"))
                                || (role == "specular"
                                    && matches!(name.as_str(), "spec" | "specular_map"))
                                || (role == "emissive"
                                    && matches!(name.as_str(), "emission" | "emissive_map"))
                                || (role == "environment"
                                    && matches!(name.as_str(), "environment_map" | "reflection"))
                        }) else {
                            return Ok(None);
                        };
                        let Some(texture_address) = resolve_material_texture_address(binding)?
                        else {
                            return Ok(None);
                        };
                        Ok(self
                            .asset_streamer
                            .get::<newviso_textures::TextureResource>(&texture_address))
                    };

                    let base_color = match resolve_role("base_color")? {
                        Some(texture) => Some(texture),
                        None => resolve_role("generic")?,
                    };
                    let mut auxiliary_textures = BTreeMap::new();
                    for binding in &material.textures {
                        let role = binding.slot.trim().to_ascii_lowercase();
                        if matches!(
                            role.as_str(),
                            "base_color"
                                | "albedo"
                                | "diffuse"
                                | "base"
                                | "normal"
                                | "normal_map"
                                | "normals"
                                | "specular"
                                | "spec"
                                | "specular_map"
                                | "emissive"
                                | "emission"
                                | "emissive_map"
                                | "environment"
                                | "environment_map"
                                | "reflection"
                                | "generic"
                        ) {
                            continue;
                        }
                        let Some(texture_address) = resolve_material_texture_address(binding)?
                        else {
                            continue;
                        };
                        if let Some(texture) = self
                            .asset_streamer
                            .get::<newviso_textures::TextureResource>(&texture_address)
                        {
                            auxiliary_textures.insert(role, texture);
                        }
                    }

                    Ok(newviso_scene::SceneResolvedMaterial {
                        material: material.clone(),
                        base_color,
                        normal: resolve_role("normal")?,
                        specular: resolve_role("specular")?,
                        emissive: resolve_role("emissive")?,
                        environment: resolve_role("environment")?,
                        auxiliary_textures,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let prepared = self
                .scene_prepared_models
                .get(&model.id.0)
                .and_then(|result| result.as_ref().ok())
                .cloned();
            if self.scene.install_entity_model_prepared(
                stable_id,
                model,
                &resolved_scene_materials,
                prepared.as_deref(),
            )? {
                host::debug(
                    "newviso.assets.streaming",
                    format!(
                        "materialized model entity={} asset='{}' meshes={}",
                        stable_id,
                        address.canonical(),
                        model.meshes.len()
                    ),
                );
            }
            let _ = self.bind_vehicle_model_presentation(stable_id, model)?;
            let _ = self.apply_scene_animation_binding(stable_id)?;
        }

        if let Some(collision) = collision.as_deref() {
            if self.physics.is_some() {
                let key = (stable_id, collision.id.0);
                let prepared = self
                    .scene_prepared_collisions
                    .remove(&key)
                    .ok_or_else(|| {
                        format!(
                            "scene collision entity={} asset='{}' reached commit without background preparation",
                            stable_id,
                            address.canonical()
                        )
                    })??;
                let (changed, nav_source) = self
                    .physics
                    .as_mut()
                    .expect("physics checked")
                    .install_prepared_streamed_collision(prepared)?;
                if let Some(nav_source) = nav_source {
                    let walkable = self
                        .physical_characters
                        .upsert_navigation_tile(nav_source)?;
                    host::debug(
                        "newviso.navigation",
                        format!(
                            "navigation tile materialized entity={} walkable_polygons={}",
                            stable_id, walkable
                        ),
                    );
                }
                if changed {
                    host::debug(
                        "newviso.assets.streaming",
                        format!(
                            "materialized collision entity={} asset='{}' vertices={} triangles={}",
                            stable_id,
                            address.canonical(),
                            collision.vertices.len(),
                            collision.triangles.len()
                        ),
                    );
                }
            }
        }

        if model.is_none() && collision.is_none() {
            return Err(format!(
                "scene asset '{}' became resident with unsupported runtime resource type",
                address.canonical()
            ));
        }

        self.scene.mark_entity_resident(stable_id)
    }
    fn queue_scene_materialization(&mut self, stable_id: u64) {
        self.scene_materialization_pending.insert(stable_id);
        if !self.scene_materialization_queue.contains(&stable_id) {
            self.scene_materialization_queue.push_back(stable_id);
        }
    }

    pub(super) fn sync_scene_streaming_interests(&mut self) -> Result<(), String> {
        let interests = self.scene.streaming_interests();
        let active_ids = interests
            .iter()
            .map(|request| request.stable_id)
            .collect::<std::collections::HashSet<_>>();

        // Streaming-interest reconciliation is owner-thread bookkeeping, so it
        // must be a bounded slice rather than a full-world transactional pass.
        // The visibility system already orders interests by priority: newly
        // entering nearby assets are therefore claimed first, while old claims
        // may linger for a few frames without affecting gameplay correctness.
        let started = std::time::Instant::now();
        let time_budget = std::time::Duration::from_micros(1_500);
        const NEW_CLAIM_BUDGET: usize = 96;
        const ADDRESS_CHANGE_BUDGET: usize = 4;
        const STALE_RELEASE_BUDGET: usize = 4;
        const PRIORITY_UPDATE_BUDGET: usize = 32;

        let mut new_claims = 0usize;
        let mut address_changes = 0usize;
        let mut stale_releases = 0usize;
        let mut priority_updates = 0usize;

        // 1) Claim new interests first. Do not parse/normalize thousands of
        // unchanged addresses every frame.
        for request in &interests {
            if new_claims >= NEW_CLAIM_BUDGET || started.elapsed() >= time_budget {
                break;
            }
            if self.scene_stream_claims.contains_key(&request.stable_id) {
                continue;
            }
            let address = AssetAddress::parse(&request.asset_ref).map_err(|error| {
                format!(
                    "scene entity {} has invalid asset_ref '{}': {error}",
                    request.stable_id, request.asset_ref
                )
            })?;
            let quantized_priority = (request.priority.max(0.0) * 4.0).round() * 0.25;
            self.asset_streamer.request(
                Self::scene_stream_owner(request.stable_id),
                address.clone(),
                StreamingClaim::new(quantized_priority),
            )?;
            self.scene_stream_claims
                .insert(request.stable_id, address.clone());
            self.scene_stream_priorities
                .insert(request.stable_id, quantized_priority);
            self.scene_materialization_pending.insert(request.stable_id);
            if self.asset_streamer.is_resident(&address) {
                self.queue_scene_materialization(request.stable_id);
            }
            new_claims = new_claims.saturating_add(1);
        }

        // 2) Asset identity changes are rare but can dematerialize collision/nav
        // state, so process only a tiny bounded number in one frame.
        if started.elapsed() < time_budget {
            for request in &interests {
                if address_changes >= ADDRESS_CHANGE_BUDGET || started.elapsed() >= time_budget {
                    break;
                }
                let Some(previous) = self.scene_stream_claims.get(&request.stable_id) else {
                    continue;
                };
                if previous.matches_canonical(&request.asset_ref) {
                    continue;
                }

                let address = AssetAddress::parse(&request.asset_ref).map_err(|error| {
                    format!(
                        "scene entity {} has invalid asset_ref '{}': {error}",
                        request.stable_id, request.asset_ref
                    )
                })?;
                let previous = previous.clone();
                self.asset_streamer
                    .release(Self::scene_stream_owner(request.stable_id), &previous);
                self.release_scene_aux_claims(request.stable_id);
                self.dematerialize_scene_asset(request.stable_id)?;

                let quantized_priority = (request.priority.max(0.0) * 4.0).round() * 0.25;
                self.asset_streamer.request(
                    Self::scene_stream_owner(request.stable_id),
                    address.clone(),
                    StreamingClaim::new(quantized_priority),
                )?;
                self.scene_stream_claims
                    .insert(request.stable_id, address.clone());
                self.scene_stream_priorities
                    .insert(request.stable_id, quantized_priority);
                self.scene_materialization_pending.insert(request.stable_id);
                if self.asset_streamer.is_resident(&address) {
                    self.queue_scene_materialization(request.stable_id);
                }
                address_changes = address_changes.saturating_add(1);
            }
        }

        // 3) Retire only a few stale interests per frame. Keeping an out-of-range
        // resource claimed for several extra frames is harmless and avoids an
        // unload/nav teardown burst when the camera crosses a streaming boundary.
        if started.elapsed() < time_budget {
            let stale_ids = self
                .scene_stream_claims
                .keys()
                .copied()
                .filter(|stable_id| !active_ids.contains(stable_id))
                .take(STALE_RELEASE_BUDGET)
                .collect::<Vec<_>>();
            for stable_id in stale_ids {
                if started.elapsed() >= time_budget {
                    break;
                }
                if let Some(address) = self.scene_stream_claims.remove(&stable_id) {
                    self.asset_streamer
                        .release(Self::scene_stream_owner(stable_id), &address);
                    self.scene_stream_priorities.remove(&stable_id);
                    self.scene_materialization_pending.remove(&stable_id);
                    self.release_scene_aux_claims(stable_id);
                    self.dematerialize_scene_asset(stable_id)?;
                    stale_releases = stale_releases.saturating_add(1);
                }
            }
        }

        // 4) Priority drift is soft state. Quantization plus a bounded update
        // slice prevents camera motion from rewriting thousands of claims at once.
        if started.elapsed() < time_budget {
            for request in &interests {
                if priority_updates >= PRIORITY_UPDATE_BUDGET || started.elapsed() >= time_budget {
                    break;
                }
                let Some(address) = self.scene_stream_claims.get(&request.stable_id).cloned()
                else {
                    continue;
                };
                if !address.matches_canonical(&request.asset_ref) {
                    continue;
                }
                let quantized_priority = (request.priority.max(0.0) * 4.0).round() * 0.25;
                let priority_changed = self
                    .scene_stream_priorities
                    .get(&request.stable_id)
                    .is_none_or(|previous| (*previous - quantized_priority).abs() > f32::EPSILON);
                if !priority_changed {
                    continue;
                }
                self.asset_streamer.request(
                    Self::scene_stream_owner(request.stable_id),
                    address,
                    StreamingClaim::new(quantized_priority),
                )?;
                self.scene_stream_priorities
                    .insert(request.stable_id, quantized_priority);
                priority_updates = priority_updates.saturating_add(1);
            }
        }

        if started.elapsed() >= time_budget
            || new_claims != 0
            || address_changes != 0
            || stale_releases != 0
            || priority_updates != 0
        {
            host::debug(
                "newviso.assets.streaming",
                format!(
                    "interest sync active={} new={} changed={} stale={} priorities={} elapsed_ms={:.3}",
                    interests.len(),
                    new_claims,
                    address_changes,
                    stale_releases,
                    priority_updates,
                    started.elapsed().as_secs_f64() * 1000.0
                ),
            );
        }

        Ok(())
    }

    pub(super) fn apply_scene_streaming_residency(
        &mut self,
        report: &newviso_resource_runtime::StreamingTickReport,
    ) -> Result<(), String> {
        self.drain_scene_prepare_completions();

        // Residency is edge-driven. A large stable world must not be
        // re-sorted/re-materialized every frame. Newly resident top-level
        // assets and newly resident auxiliary material dependencies wake only
        // the scene entities that can make progress.
        if !report.became_resident.is_empty() {
            let became = report
                .became_resident
                .iter()
                .collect::<std::collections::BTreeSet<_>>();

            let owners = self
                .scene_stream_claims
                .iter()
                .filter_map(|(stable_id, address)| became.contains(address).then_some(*stable_id))
                .collect::<Vec<_>>();
            for stable_id in owners {
                self.queue_scene_materialization(stable_id);
            }

            let aux_owners = self
                .scene_stream_aux_claims
                .iter()
                .filter_map(|(stable_id, addresses)| {
                    addresses
                        .iter()
                        .any(|address| became.contains(address))
                        .then_some(*stable_id)
                })
                .collect::<Vec<_>>();
            for stable_id in aux_owners {
                self.queue_scene_materialization(stable_id);
            }
        }

        let model_budget = self.settings.streaming.max_model_materializations_per_frame;
        let materialization_budget = if model_budget == 0 {
            usize::MAX
        } else {
            model_budget
        };
        // The gameplay/owner thread only commits already-prepared state. CPU-heavy
        // expansion/BVH work is owned by ScenePreparePool.
        let max_commits = if model_budget == 0 {
            12
        } else {
            model_budget.saturating_mul(2).clamp(2, 16)
        };
        let commit_started = std::time::Instant::now();
        let commit_time_budget = std::time::Duration::from_micros(2_000);

        let probe_budget = materialization_budget
            .saturating_mul(4)
            .clamp(16, 128)
            .min(self.scene_materialization_queue.len().max(1));
        let mut probes = 0usize;
        let mut model_materializations = 0usize;
        let mut commits = 0usize;
        let mut background_submissions = 0usize;

        while probes < probe_budget && !self.scene_materialization_queue.is_empty() {
            let stable_id = self
                .scene_materialization_queue
                .pop_front()
                .expect("queue checked non-empty");
            probes = probes.saturating_add(1);

            if !self.scene_materialization_pending.contains(&stable_id) {
                continue;
            }
            let Some(address) = self.scene_stream_claims.get(&stable_id).cloned() else {
                self.scene_materialization_pending.remove(&stable_id);
                continue;
            };
            if !self.asset_streamer.is_resident(&address) {
                continue;
            }

            if commits >= max_commits
                || (commits > 0 && commit_started.elapsed() >= commit_time_budget)
            {
                self.scene_materialization_queue.push_front(stable_id);
                break;
            }

            let model = self.asset_streamer.get::<ModelResource>(&address);
            let collision = self.asset_streamer.get::<CollisionMeshResource>(&address);
            let has_model = model.is_some();
            let has_collision = collision.is_some();
            let pending_model_instance = model
                .as_ref()
                .is_some_and(|_| !self.scene.entity_model_installed(stable_id));

            if pending_model_instance && model_materializations >= materialization_budget {
                self.scene_materialization_queue.push_front(stable_id);
                break;
            }

            let mut background_ready = true;

            if let Some(model) = model.as_ref() {
                if pending_model_instance && !self.scene.model_geometry_cached(model.id.0) {
                    match self.scene_prepared_models.get(&model.id.0) {
                        Some(Ok(_)) => {}
                        Some(Err(error)) => {
                            return Err(format!(
                                "background model preparation failed model='{}': {}",
                                model.name, error
                            ));
                        }
                        None => {
                            self.scene_model_prepare_waiters
                                .entry(model.id.0)
                                .or_default()
                                .insert(stable_id);
                            if self.scene_model_prepare_in_flight.insert(model.id.0) {
                                self.scene_prepare_pool.submit(ScenePrepareTask::Model {
                                    model: Arc::clone(model),
                                });
                                background_submissions = background_submissions.saturating_add(1);
                            }
                            background_ready = false;
                        }
                    }
                }
            }

            if let Some(collision) = collision.as_ref() {
                if self.physics.is_some() {
                    let key = (stable_id, collision.id.0);
                    match self.scene_prepared_collisions.get(&key) {
                        Some(Ok(_)) => {}
                        Some(Err(error)) => {
                            return Err(format!(
                                "background collision preparation failed entity={} asset='{}': {}",
                                stable_id,
                                address.canonical(),
                                error
                            ));
                        }
                        None => {
                            if self.scene_collision_prepare_in_flight.insert(key) {
                                let (position, rotation_degrees, scale) = self
                                    .scene
                                    .entity_transform_values(stable_id)
                                    .ok_or_else(|| {
                                        format!(
                                            "scene entity {} disappeared before collision preparation",
                                            stable_id
                                        )
                                    })?;
                                self.scene_prepare_pool.submit(ScenePrepareTask::Collision {
                                    stable_id,
                                    asset_id: collision.id.0,
                                    collision: Arc::clone(collision),
                                    position,
                                    rotation_degrees,
                                    scale,
                                });
                                background_submissions = background_submissions.saturating_add(1);
                            }
                            background_ready = false;
                        }
                    }
                }
            }

            if !background_ready {
                // Completion re-enqueues this entity. Do not poll unfinished jobs.
                continue;
            }

            self.materialize_scene_asset(stable_id, &address)?;
            commits = commits.saturating_add(1);

            let complete = (!has_model || self.scene.entity_model_installed(stable_id))
                && (has_model || has_collision);
            if complete {
                self.scene_materialization_pending.remove(&stable_id);
                if pending_model_instance {
                    model_materializations = model_materializations.saturating_add(1);
                }
            } else {
                let waiting_on_aux =
                    self.scene_stream_aux_claims
                        .get(&stable_id)
                        .is_some_and(|addresses| {
                            addresses
                                .iter()
                                .any(|address| !self.asset_streamer.is_resident(address))
                        });
                if !waiting_on_aux {
                    self.scene_materialization_queue.push_back(stable_id);
                }
            }
        }

        if commits > 0 || background_submissions > 0 {
            host::debug(
                "newviso.assets.streaming",
                format!(
                    "scene materialization slice probes={} commits={} models={} background_submissions={} prepare_workers={} elapsed_ms={:.3} queued={}",
                    probes,
                    commits,
                    model_materializations,
                    background_submissions,
                    self.scene_prepare_pool.worker_count(),
                    commit_started.elapsed().as_secs_f64() * 1000.0,
                    self.scene_materialization_queue.len()
                ),
            );
        }

        Ok(())
    }

    pub(super) fn pump_asset_streaming(&mut self) -> Result<(), String> {
        let report = self.asset_streamer.pump();
        for (address, error) in &report.failed {
            host::warn(
                "newviso.assets.streaming",
                format!("asset='{}' streaming failed: {error}", address.canonical()),
            );
        }

        self.apply_scene_streaming_residency(&report)?;

        if !report.loaded.is_empty()
            || !report.became_resident.is_empty()
            || !report.evicted.is_empty()
        {
            host::debug(
                "newviso.assets.streaming",
                format!(
                    "frame={} loaded={} resident_promotions={} evicted={} source_bytes_loaded={} resident_bytes={} over_budget={} parallel_loads={} model_materialization_budget={}",
                    report.frame,
                    report.loaded.len(),
                    report.became_resident.len(),
                    report.evicted.len(),
                    report.source_bytes_loaded,
                    report.resident_bytes,
                    report.over_budget,
                    self.settings.streaming.parallel_loads,
                    self.settings.streaming.max_model_materializations_per_frame
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod material_binding_tests {
    use super::*;

    fn binding(
        slot: &str,
        name: &str,
        required: bool,
    ) -> newviso_materials::MaterialTextureBinding {
        newviso_materials::MaterialTextureBinding {
            slot: slot.to_owned(),
            texture_name: Some(name.to_owned()),
            texture: None,
            required,
        }
    }

    #[test]
    fn unresolved_local_diffuse_uses_entity_texture_dictionary() {
        let diffuse = binding("base_color", "head_diff_000_d_whi", false);
        assert!(material_binding_uses_local_base_color(&diffuse));
        assert!(material_binding_blocks_model_materialization(&diffuse));

        let legacy = binding("generic", "uppr_diff_031_a_uni", false);
        assert!(material_binding_uses_local_base_color(&legacy));
        assert!(material_binding_blocks_model_materialization(&legacy));
    }

    #[test]
    fn unresolved_global_shader_inputs_do_not_use_entity_texture_dictionary() {
        for sampler in [
            binding("hair_noise", "long_hair_noise", false),
            binding("generic", "givemechecker", false),
            binding("base_color", "givemechecker", false),
            binding("environment", "ENV_SMOOTH_CONCRETE2", false),
            binding("generic", "ENVEFF_Gray", false),
        ] {
            assert!(!material_binding_uses_local_base_color(&sampler));
            assert!(!material_binding_blocks_model_materialization(&sampler));
        }
    }
}
