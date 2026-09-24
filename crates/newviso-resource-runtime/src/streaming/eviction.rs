use super::*;

impl<S: AssetSource> AssetStreamer<S> {
    pub(super) fn promote_ready_entries(&mut self, report: &mut StreamingTickReport) {
        loop {
            let ready = self
                .entries
                .iter()
                .filter_map(|(address, entry)| {
                    if entry.state != StreamingState::WaitingForDependencies {
                        return None;
                    }
                    let all_ready = entry.dependencies.iter().all(|dependency| {
                        self.entries
                            .get(dependency)
                            .is_some_and(|dependency| dependency.state == StreamingState::Resident)
                    });
                    all_ready.then_some(address.clone())
                })
                .collect::<Vec<_>>();

            if ready.is_empty() {
                break;
            }

            for address in ready {
                if let Some(entry) = self.entries.get_mut(&address) {
                    entry.state = StreamingState::Resident;
                    entry.resident_since_frame = Some(self.frame);
                    entry.error = None;
                    report.became_resident.push(address);
                }
            }
        }

        let immediate = self
            .entries
            .iter()
            .filter_map(|(address, entry)| {
                (entry.state == StreamingState::Resident
                    && entry.resident_since_frame == Some(self.frame)
                    && !report.became_resident.contains(address))
                .then_some(address.clone())
            })
            .collect::<Vec<_>>();
        report.became_resident.extend(immediate);
    }
    pub(super) fn requeue_retryable_failures(&mut self) {
        for entry in self.entries.values_mut() {
            if entry.state != StreamingState::Failed || !entry.has_claims() {
                continue;
            }
            let failed_at = entry.failure_frame.unwrap_or(self.frame);
            let age = self.frame.saturating_sub(failed_at);
            if age >= self.policy.failed_retry_frames {
                entry.state = StreamingState::Queued;
                entry.error = None;
            }
        }
    }
    pub(super) fn reconcile_unclaimed_entries(&mut self) {
        for entry in self.entries.values_mut() {
            if entry.has_claims() {
                if entry.state == StreamingState::Unloaded {
                    entry.state = StreamingState::Queued;
                }
                continue;
            }

            if matches!(entry.state, StreamingState::Queued | StreamingState::Failed) {
                entry.state = StreamingState::Unloaded;
                entry.error = None;
                entry.failure_frame = None;
            }
        }
    }
    pub(super) fn evict_expired_unclaimed(&mut self, report: &mut StreamingTickReport) {
        let candidates = self
            .entries
            .values()
            .filter(|entry| {
                entry.resource.is_some()
                    && !entry.has_claims()
                    && !entry.pinned()
                    && self.frame.saturating_sub(entry.last_touched_frame)
                        >= self.policy.eviction_grace_frames
            })
            .map(|entry| entry.address.clone())
            .collect::<Vec<_>>();

        for address in candidates {
            if self.evict_entry(&address, false) {
                report.evicted.push(address);
            }
        }
    }
    pub(super) fn evict_to_budget(&mut self, report: &mut StreamingTickReport) {
        if self.policy.max_resident_bytes == 0 {
            return;
        }

        while self.resident_bytes() > self.policy.max_resident_bytes {
            let candidate = self
                .entries
                .values()
                .filter(|entry| {
                    entry.resource.is_some()
                        && !entry.has_claims()
                        && !entry.pinned()
                        && self.frame.saturating_sub(entry.last_touched_frame)
                            >= self.policy.eviction_grace_frames
                })
                .min_by(|a, b| compare_eviction_candidates(a, b))
                .map(|entry| entry.address.clone());

            let Some(address) = candidate else {
                break;
            };
            if self.evict_entry(&address, false) {
                report.evicted.push(address);
            } else {
                break;
            }
        }
    }
    pub(super) fn evict_entry(&mut self, address: &AssetAddress, force: bool) -> bool {
        let Some(entry) = self.entries.get(address) else {
            return false;
        };
        if entry.resource.is_none() {
            return false;
        }
        if !force && (entry.has_claims() || entry.pinned()) {
            return false;
        }

        let dependencies = entry.dependencies.clone();
        let source = entry.source.clone();
        self.remove_dependency_claims(address, &dependencies);

        if let Some(source) = source.as_ref() {
            self.release_source(source);
        }
        self.resources.unload(address);

        if let Some(entry) = self.entries.get_mut(address) {
            entry.resource = None;
            entry.source = None;
            entry.source_size_bytes = 0;
            entry.dependencies.clear();
            entry.resident_since_frame = None;
            entry.error = None;
            entry.failure_frame = None;
            entry.last_touched_frame = self.frame;
            entry.state = if entry.has_claims() {
                StreamingState::Queued
            } else {
                StreamingState::Unloaded
            };
        }

        self.total_evictions = self.total_evictions.saturating_add(1);
        true
    }
}
