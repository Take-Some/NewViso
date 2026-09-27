use super::*;

impl<S: AssetSource> AssetStreamer<S> {
    pub fn pump(&mut self) -> StreamingTickReport {
        self.frame = self.frame.wrapping_add(1).max(1);
        let mut report = StreamingTickReport {
            frame: self.frame,
            ..StreamingTickReport::default()
        };

        self.requeue_retryable_failures();
        self.reconcile_unclaimed_entries();
        self.promote_ready_entries(&mut report);

        let max_loads = if self.policy.max_loads_per_tick == 0 {
            usize::MAX
        } else {
            self.policy.max_loads_per_tick
        };
        let worker_count = self.policy.parallel_loads.max(1);

        while report.loaded.len() + report.failed.len() < max_loads {
            if self.policy.max_source_bytes_per_tick != 0
                && report.source_bytes_loaded >= self.policy.max_source_bytes_per_tick
            {
                break;
            }

            let completed = report.loaded.len() + report.failed.len();
            let batch_limit = worker_count.min(max_loads.saturating_sub(completed));
            let batch = self.next_load_candidates(batch_limit);
            if batch.is_empty() {
                break;
            }

            for address in &batch {
                if let Some(entry) = self.entries.get_mut(address) {
                    entry.state = StreamingState::Loading;
                    entry.error = None;
                }
            }

            // Source resolution + semantic decode are independent for different
            // asset addresses and dominate large-map startup cost. Execute them
            // concurrently, then commit state/cache/dependency changes on this
            // owner thread in deterministic candidate order.
            let prepared = if batch.len() == 1 {
                let address = batch[0].clone();
                let result = self.resources.prepare_erased_with_info(&address);
                vec![(address, result)]
            } else {
                let resources = &self.resources;
                std::thread::scope(|scope| {
                    let handles = batch
                        .iter()
                        .cloned()
                        .map(|address| {
                            let worker_address = address.clone();
                            let handle = scope
                                .spawn(move || resources.prepare_erased_with_info(&worker_address));
                            (address, handle)
                        })
                        .collect::<Vec<_>>();

                    handles
                        .into_iter()
                        .map(|(address, handle)| {
                            let result = handle.join().unwrap_or_else(|_| {
                                Err(format!(
                                    "streaming decode worker panicked asset='{}'",
                                    address.canonical()
                                ))
                            });
                            (address, result)
                        })
                        .collect::<Vec<_>>()
                })
            };

            for (address, prepared_load) in prepared {
                match self.finish_prepared_load(&address, prepared_load) {
                    Ok(source_bytes) => {
                        report.source_bytes_loaded =
                            report.source_bytes_loaded.saturating_add(source_bytes);
                        report.loaded.push(address);
                    }
                    Err(error) => {
                        report.failed.push((address, error));
                    }
                }
            }

            self.promote_ready_entries(&mut report);
        }

        self.evict_expired_unclaimed(&mut report);
        self.evict_to_budget(&mut report);
        self.promote_ready_entries(&mut report);

        report.resident_bytes = self.resident_bytes();
        report.over_budget = self.policy.max_resident_bytes != 0
            && report.resident_bytes > self.policy.max_resident_bytes;
        report
    }

    pub fn force_evict(&mut self, address: &AssetAddress) -> bool {
        self.evict_entry(address, true)
    }
    pub fn bump_vfs_generation(&mut self) {
        self.resources.bump_vfs_generation();
        self.resident_sources.clear();

        for entry in self.entries.values_mut() {
            entry.dependency_claims.clear();
            entry.dependencies.clear();
            entry.resource = None;
            entry.source = None;
            entry.source_size_bytes = 0;
            entry.resident_since_frame = None;
            entry.failure_frame = None;
            entry.error = None;
            entry.state = if entry.external_claims.is_empty() {
                StreamingState::Unloaded
            } else {
                StreamingState::Queued
            };
            entry.last_touched_frame = self.frame;
        }
    }
    pub(super) fn next_load_candidates(&self, limit: usize) -> Vec<AssetAddress> {
        if limit == 0 {
            return Vec::new();
        }
        let mut candidates = self
            .entries
            .values()
            .filter(|entry| {
                entry.has_claims()
                    && matches!(
                        entry.state,
                        StreamingState::Unloaded | StreamingState::Queued
                    )
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| compare_load_candidates(b, a));
        candidates
            .into_iter()
            .take(limit)
            .map(|entry| entry.address.clone())
            .collect()
    }

    fn finish_prepared_load(
        &mut self,
        address: &AssetAddress,
        prepared: Result<ResourceLoad, String>,
    ) -> Result<u64, String> {
        let load = match self.resources.commit_prepared_load(address, prepared) {
            Ok(load) => load,
            Err(error) => {
                let entry = self
                    .entries
                    .entry(address.clone())
                    .or_insert_with(|| StreamingEntry::new(address.clone(), self.frame));
                entry.state = StreamingState::Failed;
                entry.error = Some(error.clone());
                entry.failure_frame = Some(self.frame);
                self.total_failures = self.total_failures.saturating_add(1);
                return Err(error);
            }
        };

        self.total_loads = self.total_loads.saturating_add(1);
        let source_bytes = load.source_size_bytes;
        let dependencies = load.resource.dependencies();

        if dependencies.iter().any(|dependency| dependency == address) {
            let error = format!(
                "streaming dependency cycle: '{}' directly depends on itself",
                address.canonical()
            );
            self.mark_failed_loaded(address, load, error.clone());
            return Err(error);
        }

        for dependency in &dependencies {
            if self.dependency_reaches(dependency, address) {
                let error = format!(
                    "streaming dependency cycle detected parent='{}' dependency='{}'",
                    address.canonical(),
                    dependency.canonical()
                );
                self.mark_failed_loaded(address, load, error.clone());
                return Err(error);
            }
        }

        self.acquire_source(&load);
        {
            let entry = self
                .entries
                .entry(address.clone())
                .or_insert_with(|| StreamingEntry::new(address.clone(), self.frame));
            entry.dependencies = dependencies;
            entry.resource = Some(load.resource);
            entry.source = Some(load.source);
            entry.source_size_bytes = source_bytes;
            entry.failure_frame = None;
            entry.error = None;
            entry.last_touched_frame = self.frame;
            entry.state = if entry.dependencies.is_empty() {
                entry.resident_since_frame = Some(self.frame);
                StreamingState::Resident
            } else {
                entry.resident_since_frame = None;
                StreamingState::WaitingForDependencies
            };
        }

        self.refresh_dependency_claims(address);
        Ok(source_bytes)
    }

    pub(super) fn mark_failed_loaded(
        &mut self,
        address: &AssetAddress,
        load: ResourceLoad,
        error: String,
    ) {
        self.resources.unload(address);
        let entry = self
            .entries
            .entry(address.clone())
            .or_insert_with(|| StreamingEntry::new(address.clone(), self.frame));
        entry.resource = None;
        entry.source = None;
        entry.source_size_bytes = load.source_size_bytes;
        entry.dependencies.clear();
        entry.state = StreamingState::Failed;
        entry.error = Some(error);
        entry.failure_frame = Some(self.frame);
        self.total_failures = self.total_failures.saturating_add(1);
    }
}
