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

        // Never wait for I/O/decode on the caller thread. Workers publish
        // completed immutable ResourceLoad values; pump only drains what is
        // already ready and commits a bounded amount of work.
        loop {
            match self.load_pool.try_recv() {
                Ok(completed) => self.completed_loads.push_back(completed),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }

        let max_commits = if self.policy.max_loads_per_tick == 0 {
            usize::MAX
        } else {
            self.policy.max_loads_per_tick
        };
        let mut committed = 0usize;
        while committed < max_commits {
            let Some(completed) = self.completed_loads.pop_front() else {
                break;
            };

            self.in_flight.remove(&completed.address);
            if completed.generation != self.load_generation {
                continue;
            }

            let still_claimed = self
                .entries
                .get(&completed.address)
                .is_some_and(StreamingEntry::has_claims);
            if !still_claimed {
                if let Some(entry) = self.entries.get_mut(&completed.address) {
                    entry.state = StreamingState::Unloaded;
                    entry.error = None;
                    entry.failure_frame = None;
                }
                continue;
            }

            let prepared_bytes = completed
                .prepared
                .as_ref()
                .map(|load| load.source_size_bytes)
                .unwrap_or(0);
            if self.policy.max_source_bytes_per_tick != 0
                && report.source_bytes_loaded != 0
                && report.source_bytes_loaded.saturating_add(prepared_bytes)
                    > self.policy.max_source_bytes_per_tick
            {
                self.completed_loads.push_front(completed);
                break;
            }

            match self.finish_prepared_load(&completed.address, completed.prepared) {
                Ok(source_bytes) => {
                    report.source_bytes_loaded =
                        report.source_bytes_loaded.saturating_add(source_bytes);
                    report.loaded.push(completed.address);
                }
                Err(error) => {
                    report.failed.push((completed.address, error));
                }
            }
            committed = committed.saturating_add(1);
        }

        self.promote_ready_entries(&mut report);

        // Keep only the configured number of source/decode jobs in flight.
        // Scheduling is cheap and non-blocking: no thread creation and no join.
        let available_slots = self
            .policy
            .parallel_loads
            .saturating_sub(self.in_flight.len());
        let max_submissions = if self.policy.max_loads_per_tick == 0 {
            available_slots
        } else {
            available_slots.min(self.policy.max_loads_per_tick)
        };
        if max_submissions != 0 {
            let batch = self.next_load_candidates(max_submissions);
            for address in batch {
                if !self.in_flight.insert(address.clone()) {
                    continue;
                }
                if let Some(entry) = self.entries.get_mut(&address) {
                    entry.state = StreamingState::Loading;
                    entry.error = None;
                }
                self.load_pool.submit(StreamingLoadTask {
                    generation: self.load_generation,
                    address,
                });
            }
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
        self.load_generation = self.load_generation.wrapping_add(1).max(1);
        self.load_pool.clear_queued();
        self.in_flight.clear();
        self.completed_loads.clear();
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
