use super::*;

impl EngineApplication {
    pub(super) fn runtime_state(&self) -> Value {
        self.compose_runtime_state(self.living_world.runtime_state())
    }

    pub(super) fn script_frame_state(&mut self) -> Value {
        let mut runtime_state = self.scene.script_frame_state();
        let root = runtime_state
            .as_object_mut()
            .expect("scene script frame state must be a JSON object");

        let interval = f64::from(self.settings.scripting.world_snapshot_interval_seconds);
        let world_snapshot_fresh = interval <= 0.0
            || self.elapsed_seconds + 1.0e-9 >= self.next_script_world_snapshot_seconds;
        if world_snapshot_fresh {
            self.next_script_world_snapshot_seconds = if interval <= 0.0 {
                self.elapsed_seconds
            } else {
                self.elapsed_seconds + interval
            };
            root.insert("living_world".to_owned(), self.living_world.frame_state());
            root.insert("agents".to_owned(), self.agents.runtime_state());
            root.insert("peds".to_owned(), self.peds.runtime_state());
            root.insert(
                "physical_characters".to_owned(),
                self.physical_characters.runtime_state(),
            );
        }
        // Inventory/pickup interaction is player-input critical and must not be
        // throttled with the large living-world snapshot.
        root.insert("arm_ik".to_owned(), self.scene.entity_arm_ik_snapshot());
        root.insert("items".to_owned(), self.items.runtime_state());
        root.insert("vehicles".to_owned(), self.vehicles.runtime_state());
        root.insert("vehicle_tracks".to_owned(), self.vehicle_tracks_state());
        root.insert(
            "vehicle_presentation".to_owned(),
            self.vehicle_presentation_runtime_state(),
        );
        root.insert(
            "vehicle_debris".to_owned(),
            self.vehicle_debris_runtime_state(),
        );
        root.insert(
            "world_snapshot_fresh".to_owned(),
            Value::Bool(world_snapshot_fresh),
        );
        root.insert(
            "world_snapshot_seconds".to_owned(),
            json!(self.elapsed_seconds),
        );

        // Player/controller code needs current physics every frame.
        if let Some(physics) = self.physics.as_ref() {
            root.insert("physics".to_owned(), physics.runtime_state());
        }

        let streaming = self.asset_streamer.stats();
        root.insert(
            "asset_streaming".to_owned(),
            json!({
                "frame": streaming.frame,
                "queued": streaming.queued,
                "loading": streaming.loading,
                "waiting_dependencies": streaming.waiting_dependencies,
                "resident": streaming.resident,
                "failed": streaming.failed,
                "resident_bytes": streaming.resident_bytes
            }),
        );

        let scene_world_frame = root
            .get("scene")
            .and_then(|scene| scene.get("world"))
            .and_then(|world| world.get("frame"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let scene_process_due = root
            .get("scene")
            .and_then(|scene| scene.get("world"))
            .and_then(|world| world.get("process_due"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let scene_resident = root
            .get("scene")
            .and_then(|scene| scene.get("world"))
            .and_then(|world| world.get("resident"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let scene_stream_requests = root
            .get("scene")
            .and_then(|scene| scene.get("world"))
            .and_then(|world| world.get("stream_requests"))
            .and_then(Value::as_u64)
            .unwrap_or(0);

        let materialization_queue = self.scene_materialization_queue.len();
        let materialization_pending = self.scene_materialization_pending.len();
        let model_prepare_in_flight = self.scene_model_prepare_in_flight.len();
        let collision_prepare_in_flight = self.scene_collision_prepare_in_flight.len();
        let scene_streaming_started =
            streaming.frame > 0 && scene_world_frame > 0 && !self.scene_stream_claims.is_empty();
        let scene_ready = scene_streaming_started
            && scene_resident > 0
            && scene_stream_requests == 0
            && scene_process_due == 0
            && streaming.queued == 0
            && streaming.loading == 0
            && streaming.waiting_dependencies == 0
            && streaming.failed == 0
            && materialization_queue == 0
            && materialization_pending == 0
            && model_prepare_in_flight == 0
            && collision_prepare_in_flight == 0;

        root.insert(
            "startup".to_owned(),
            json!({
                "scene_ready": scene_ready,
                "streaming_started": scene_streaming_started,
                "scene_world_frame": scene_world_frame,
                "scene_process_due": scene_process_due,
                "scene_resident": scene_resident,
                "scene_stream_requests": scene_stream_requests,
                "materialization_queue": materialization_queue,
                "materialization_pending": materialization_pending,
                "model_prepare_in_flight": model_prepare_in_flight,
                "collision_prepare_in_flight": collision_prepare_in_flight
            }),
        );
        runtime_state
    }

    pub(super) fn compose_runtime_state(&self, mut living_world_state: Value) -> Value {
        let actor_views = self.living_world.actor_runtime_views();
        let presentations = self
            .world_actor_presentations
            .iter()
            .map(|(actor_id, binding)| {
                let actor = actor_views.iter().find(|actor| actor.id == *actor_id);
                json!({
                    "actor_id": actor_id,
                    "scene_key": binding.scene_key,
                    "logical_position": actor.map(|actor| actor.position),
                    "representation": actor.map(|actor| actor.representation),
                    "enabled": actor.map(|actor| actor.enabled),
                    "materialized_representations": binding.materialized_representations,
                    "scene_entity": self.scene.runtime_entity_state(&binding.scene_key),
                })
            })
            .collect::<Vec<_>>();

        living_world_state
            .as_object_mut()
            .expect("living world runtime state must be a JSON object")
            .insert("presentations".to_owned(), Value::Array(presentations));

        let mut runtime_state = self.scene.runtime_state();
        let root = runtime_state
            .as_object_mut()
            .expect("scene runtime state must be a JSON object");
        living_world_state["persistence"] = self
            .world_persistence
            .as_ref()
            .map(WorldPersistence::runtime_state)
            .unwrap_or_else(|| json!({"enabled": false, "restored": false}));
        living_world_state["presentation_transitions"] = self.world_presentations_state();
        root.insert(
            "settings".to_owned(),
            serde_json::to_value(&self.settings).expect("validated runtime settings"),
        );
        root.insert("living_world".to_owned(), living_world_state);
        root.insert("agents".to_owned(), self.agents.runtime_state());
            root.insert("peds".to_owned(), self.peds.runtime_state());
        root.insert(
            "physical_characters".to_owned(),
            self.physical_characters.runtime_state(),
        );
        root.insert("items".to_owned(), self.items.runtime_state());
        root.insert("vehicles".to_owned(), self.vehicles.runtime_state());
        root.insert("vehicle_tracks".to_owned(), self.vehicle_tracks_state());
        root.insert(
            "vehicle_presentation".to_owned(),
            self.vehicle_presentation_runtime_state(),
        );
        root.insert(
            "vehicle_debris".to_owned(),
            self.vehicle_debris_runtime_state(),
        );
        if let Some(physics) = self.physics.as_ref() {
            root.insert("physics".to_owned(), physics.runtime_state());
        }
        let streaming = self.asset_streamer.stats();
        root.insert(
            "asset_streaming".to_owned(),
            json!({
                "frame": streaming.frame,
                "entries": streaming.entries,
                "queued": streaming.queued,
                "loading": streaming.loading,
                "waiting_dependencies": streaming.waiting_dependencies,
                "resident": streaming.resident,
                "failed": streaming.failed,
                "resident_sources": streaming.resident_sources,
                "resident_bytes": streaming.resident_bytes,
                "external_claims": streaming.external_claims,
                "dependency_claims": streaming.dependency_claims,
                "total_loads": streaming.total_loads,
                "total_evictions": streaming.total_evictions,
                "total_failures": streaming.total_failures,
                "over_budget": streaming.over_budget
            }),
        );

        let mut gta_character_addresses = self
            .world_actor_presentations
            .values()
            .filter_map(|binding| binding.asset_ref.as_deref())
            .filter(|reference| {
                reference
                    .to_ascii_lowercase()
                    .contains("models/characters/gta/")
            })
            .filter_map(|reference| AssetAddress::parse(reference).ok())
            .collect::<Vec<_>>();
        gta_character_addresses.sort_by(|a, b| a.canonical().cmp(&b.canonical()));
        gta_character_addresses.dedup_by(|a, b| a.canonical() == b.canonical());
        let gta_character_streaming = gta_character_addresses
            .iter()
            .filter_map(|address| {
                let snapshot = self.asset_streamer.snapshot(address)?;
                let dependencies = snapshot
                    .dependencies
                    .iter()
                    .map(|dependency| {
                        let dependency_snapshot = self.asset_streamer.snapshot(dependency);
                        json!({
                            "address": dependency.canonical(),
                            "state": dependency_snapshot
                                .as_ref()
                                .map(|snapshot| format!("{:?}", snapshot.state))
                                .unwrap_or_else(|| "Missing".to_owned()),
                            "priority": dependency_snapshot
                                .as_ref()
                                .map(|snapshot| snapshot.effective_priority),
                            "error": dependency_snapshot
                                .as_ref()
                                .and_then(|snapshot| snapshot.error.clone()),
                        })
                    })
                    .collect::<Vec<_>>();
                Some(json!({
                    "address": snapshot.address.canonical(),
                    "state": format!("{:?}", snapshot.state),
                    "priority": snapshot.effective_priority,
                    "error": snapshot.error,
                    "dependencies": dependencies,
                }))
            })
            .collect::<Vec<_>>();
        root.insert(
            "gta_character_streaming".to_owned(),
            Value::Array(gta_character_streaming),
        );

        let gta_character_aux_streaming = self
            .scene_stream_aux_claims
            .iter()
            .filter_map(|(stable_id, addresses)| {
                let entity = self.scene.entity_state(*stable_id)?;
                let asset_ref = entity
                    .get("asset_ref")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !asset_ref
                    .to_ascii_lowercase()
                    .contains("models/characters/gta/")
                {
                    return None;
                }
                let assets = addresses
                    .iter()
                    .map(|address| {
                        let snapshot = self.asset_streamer.snapshot(address);
                        json!({
                            "address": address.canonical(),
                            "state": snapshot
                                .as_ref()
                                .map(|snapshot| format!("{:?}", snapshot.state))
                                .unwrap_or_else(|| "Missing".to_owned()),
                            "is_texture": self
                                .asset_streamer
                                .get::<newviso_textures::TextureResource>(address)
                                .is_some(),
                            "error": snapshot
                                .as_ref()
                                .and_then(|snapshot| snapshot.error.clone()),
                        })
                    })
                    .collect::<Vec<_>>();
                Some(json!({
                    "entity": stable_id,
                    "scene_residency": entity.get("residency").cloned(),
                    "asset_ref": asset_ref,
                    "assets": assets,
                }))
            })
            .collect::<Vec<_>>();
        root.insert(
            "gta_character_aux_streaming".to_owned(),
            Value::Array(gta_character_aux_streaming),
        );

        let gta_character_scene_interests = self
            .scene
            .streaming_interests()
            .into_iter()
            .filter(|request| {
                request
                    .asset_ref
                    .to_ascii_lowercase()
                    .contains("models/characters/gta/")
            })
            .map(|request| {
                json!({
                    "entity": request.stable_id,
                    "asset_ref": request.asset_ref,
                    "priority": request.priority,
                })
            })
            .collect::<Vec<_>>();
        root.insert(
            "gta_character_scene_interests".to_owned(),
            Value::Array(gta_character_scene_interests),
        );
        runtime_state
    }
}
