use super::*;

impl EngineApplication {
    pub(super) fn pump_startup_map_loading(
        &mut self,
        surface: PlatformSurfaceMetricsV1,
    ) -> Result<(), String> {
        const STARTUP_UPLOAD_BYTES: u64 = 64 * 1024 * 1024;
        const STARTUP_UPLOAD_JOBS: u32 = 256;
        const STARTUP_UPLOAD_BLOCKING_MS: f32 = 4.0;
        // Two cycles were enough to observe a transient empty queue between
        // dependency/material batches. Require a wider stable window so the
        // compositor is not dismissed while a just-registered startup entity
        // is still causing secondary texture/material work.
        const READY_STREAK_REQUIRED: u8 = 5;

        self.world_save_allowed = false;
        self.startup_map_spinner_phase = self.startup_map_spinner_phase.wrapping_add(1);

        // Build the same initial camera/streaming set that the first playable
        // frame would use, without acquiring or presenting a renderer frame.
        self.scene
            .refresh_streaming_plan(surface.width.max(1), surface.height.max(1));
        let interests = self.scene.streaming_interests();
        let interest_count = interests.len();
        self.startup_map_interest_peak = self.startup_map_interest_peak.max(interest_count);

        // CPU/source residency: models, material closure and texture dependencies.
        self.sync_scene_streaming_interests()?;
        self.pump_asset_streaming()?;

        let claimed_count = interests
            .iter()
            .filter(|request| {
                self.scene_stream_claims
                    .get(&request.stable_id)
                    .is_some_and(|address| address.matches_canonical(&request.asset_ref))
            })
            .count();
        let resident_count = interests
            .iter()
            .filter(|request| {
                self.scene_stream_claims
                    .get(&request.stable_id)
                    .is_some_and(|address| {
                        address.matches_canonical(&request.asset_ref)
                            && self.asset_streamer.is_resident(address)
                    })
            })
            .count();
        let materialized_count = interests
            .iter()
            .filter(|request| {
                self.scene_stream_claims
                    .get(&request.stable_id)
                    .is_some_and(|address| address.matches_canonical(&request.asset_ref))
                    && !self
                        .scene_materialization_pending
                        .contains(&request.stable_id)
            })
            .count();

        let dependency_total = self
            .scene_stream_aux_claims
            .values()
            .map(BTreeSet::len)
            .sum::<usize>();
        let dependency_resident = self
            .scene_stream_aux_claims
            .values()
            .flat_map(|addresses| addresses.iter())
            .filter(|address| self.asset_streamer.is_resident(address))
            .count();

        // Static map resources may enqueue deferred Vulkan texture/buffer transfers.
        // Drain them while the loading compositor still owns presentation.
        self.scene.warmup_static_asset_gpu()?;
        let uploads = RenderClient::new().pump_loading_uploads(
            STARTUP_UPLOAD_BYTES,
            STARTUP_UPLOAD_JOBS,
            STARTUP_UPLOAD_BLOCKING_MS,
        )?;
        self.startup_gpu_upload_peak_jobs = self.startup_gpu_upload_peak_jobs.max(
            uploads
                .processed_jobs
                .saturating_add(uploads.remaining_jobs),
        );
        let gpu_warmup = self.scene.startup_gpu_warmup_status();

        let (streaming_queued, streaming_loading, streaming_waiting_dependencies, streaming_failed) = {
            let stats = self.asset_streamer.stats();
            (
                stats.queued,
                stats.loading,
                stats.waiting_dependencies,
                stats.failed,
            )
        };
        let materialization_queue = self.scene_materialization_queue.len();
        let materialization_pending = self.scene_materialization_pending.len();
        let model_prepare_in_flight = self.scene_model_prepare_in_flight.len();
        let collision_prepare_in_flight = self.scene_collision_prepare_in_flight.len();

        let ratio = |done: usize, total: usize| {
            if total == 0 {
                1.0
            } else {
                done.min(total) as f32 / total as f32
            }
        };
        let claimed_ratio = ratio(claimed_count, interest_count);
        let resident_ratio = ratio(resident_count, interest_count);
        let materialized_ratio = ratio(materialized_count, interest_count);
        let texture_gpu_ratio = ratio(gpu_warmup.ready_textures, gpu_warmup.required_textures);
        let material_gpu_ratio = ratio(gpu_warmup.ready_materials, gpu_warmup.required_materials);
        let gpu_ratio = texture_gpu_ratio * 0.72 + material_gpu_ratio * 0.28;

        let measured_progress = 0.06
            + claimed_ratio * 0.14
            + resident_ratio * 0.20
            + materialized_ratio * 0.24
            + gpu_ratio * 0.35;
        self.startup_map_progress = self
            .startup_map_progress
            .max(measured_progress.clamp(0.02, 0.99));

        let all_claimed = claimed_count == interest_count;
        let all_resident = resident_count == interest_count;
        let all_materialized = materialized_count == interest_count;
        let cpu_idle = streaming_queued == 0
            && streaming_loading == 0
            && streaming_waiting_dependencies == 0
            && materialization_queue == 0
            && materialization_pending == 0
            && model_prepare_in_flight == 0
            && collision_prepare_in_flight == 0;
        let no_failures = streaming_failed == 0 && uploads.failed_jobs == 0;
        let gpu_idle = uploads.remaining_jobs == 0;
        let gpu_assets_ready = gpu_warmup.fully_ready();
        let ready_candidate = all_claimed
            && all_resident
            && all_materialized
            && cpu_idle
            && no_failures
            && gpu_idle
            && gpu_assets_ready;

        self.startup_map_ready_streak = if ready_candidate {
            self.startup_map_ready_streak.saturating_add(1)
        } else {
            0
        };

        if self.startup_map_ready_streak >= READY_STREAK_REQUIRED {
            self.startup_map_ready = true;
            self.startup_map_progress = 1.0;
            self.startup_map_status = "Map assets ready".to_owned();
            self.startup_map_detail = format!(
                "Initial residency complete: models={}/{} textures={}/{} materials={}/{} GPU queue=0.",
                materialized_count,
                interest_count,
                gpu_warmup.ready_textures,
                gpu_warmup.required_textures,
                gpu_warmup.ready_materials,
                gpu_warmup.required_materials
            );
            self.world_save_allowed = true;
            host::info(
                "newviso.startup",
                format!(
                    "initial map residency ready models={}/{} dependencies={}/{} textures={}/{} materials={}/{} peak_interests={} gpu_upload_peak_jobs={}",
                    materialized_count,
                    interest_count,
                    dependency_resident,
                    dependency_total,
                    gpu_warmup.ready_textures,
                    gpu_warmup.required_textures,
                    gpu_warmup.ready_materials,
                    gpu_warmup.required_materials,
                    self.startup_map_interest_peak,
                    self.startup_gpu_upload_peak_jobs
                ),
            );
            return Ok(());
        }

        self.startup_map_status = if streaming_failed > 0 || uploads.failed_jobs > 0 {
            "Retrying map assets...".to_owned()
        } else if !all_claimed {
            "Discovering map assets...".to_owned()
        } else if !all_resident || !all_materialized || !cpu_idle {
            "Loading map models and textures...".to_owned()
        } else if gpu_warmup.missing_textures > 0 {
            "Queueing map textures...".to_owned()
        } else if gpu_warmup.pending_textures > 0 || !gpu_idle {
            "Uploading map textures...".to_owned()
        } else if gpu_warmup.ready_materials < gpu_warmup.required_materials {
            "Building map materials...".to_owned()
        } else {
            "Finalizing map residency...".to_owned()
        };

        self.startup_map_detail = format!(
            "Models {}/{} resident {}/{} | source deps {}/{} | GPU textures {}/{} pending={} not-queued={} | GPU materials {}/{} | stream q={} load={} deps={} | prepare={}+{}+{} | GPU jobs={} ({} KiB).",
            materialized_count,
            interest_count,
            resident_count,
            interest_count,
            dependency_resident,
            dependency_total,
            gpu_warmup.ready_textures,
            gpu_warmup.required_textures,
            gpu_warmup.pending_textures,
            gpu_warmup.missing_textures,
            gpu_warmup.ready_materials,
            gpu_warmup.required_materials,
            streaming_queued,
            streaming_loading,
            streaming_waiting_dependencies,
            materialization_queue + materialization_pending,
            model_prepare_in_flight,
            collision_prepare_in_flight,
            uploads.remaining_jobs,
            uploads.remaining_bytes / 1024
        );

        Ok(())
    }

    pub(super) fn startup_map_loading_overlay(&self) -> PlatformLoadingOverlayV1 {
        if !self.ready || self.startup_map_ready || self.exit_requested {
            return PlatformLoadingOverlayV1::default();
        }

        PlatformLoadingOverlayV1 {
            active: true,
            progress_01: self.startup_map_progress.clamp(0.0, 1.0),
            spinner_phase: self.startup_map_spinner_phase,
            title: "NORTH STAR ENGINE".into(),
            status: self.startup_map_status.clone().into(),
            detail: self.startup_map_detail.clone().into(),
            view_json: String::new().into(),
        }
    }
}
