use super::*;

impl<S: AssetSource> AssetStreamer<S> {
    pub fn policy(&self) -> &StreamingPolicy {
        &self.policy
    }
    pub fn set_policy(&mut self, policy: StreamingPolicy) -> Result<(), String> {
        policy.validate()?;
        self.policy = policy;
        Ok(())
    }
    pub fn resources(&self) -> &ResourceManager<S> {
        &self.resources
    }
    pub fn resources_mut(&mut self) -> &mut ResourceManager<S> {
        &mut self.resources
    }
    pub fn request(
        &mut self,
        owner: StreamingOwnerId,
        address: AssetAddress,
        claim: StreamingClaim,
    ) -> Result<AssetId, String> {
        let claim = claim.validate()?;
        let frame = self.frame;
        let entry = self
            .entries
            .entry(address.clone())
            .or_insert_with(|| StreamingEntry::new(address.clone(), frame));
        entry.external_claims.insert(owner, claim);
        entry.last_touched_frame = frame;
        if entry.state == StreamingState::Unloaded {
            entry.state = StreamingState::Queued;
        }

        self.refresh_dependency_claims(&address);
        Ok(AssetId::from_address(&address))
    }
    pub fn request_str(
        &mut self,
        owner: StreamingOwnerId,
        address: &str,
        claim: StreamingClaim,
    ) -> Result<AssetId, String> {
        self.request(owner, AssetAddress::parse(address)?, claim)
    }
    pub fn release(&mut self, owner: StreamingOwnerId, address: &AssetAddress) -> bool {
        let removed = self
            .entries
            .get_mut(address)
            .and_then(|entry| entry.external_claims.remove(&owner))
            .is_some();
        if removed {
            if let Some(entry) = self.entries.get_mut(address) {
                entry.last_touched_frame = self.frame;
                if !entry.has_claims()
                    && matches!(entry.state, StreamingState::Queued | StreamingState::Failed)
                {
                    entry.state = StreamingState::Unloaded;
                    entry.error = None;
                    entry.failure_frame = None;
                }
            }
            self.refresh_dependency_claims(address);
        }
        removed
    }
    pub fn release_owner(&mut self, owner: StreamingOwnerId) -> usize {
        let addresses = self.entries.keys().cloned().collect::<Vec<_>>();
        let mut removed = 0;
        for address in addresses {
            if self.release(owner, &address) {
                removed += 1;
            }
        }
        removed
    }
    pub fn is_resident(&self, address: &AssetAddress) -> bool {
        self.entries
            .get(address)
            .is_some_and(|entry| entry.state == StreamingState::Resident)
    }
    pub fn state(&self, address: &AssetAddress) -> StreamingState {
        self.entries
            .get(address)
            .map(|entry| entry.state)
            .unwrap_or(StreamingState::Unloaded)
    }
    pub fn get_erased(&self, address: &AssetAddress) -> Option<Arc<dyn AssetResource>> {
        let entry = self.entries.get(address)?;
        if entry.state != StreamingState::Resident {
            return None;
        }
        entry.resource.clone()
    }
    pub fn get<T: AssetResource + 'static>(&self, address: &AssetAddress) -> Option<Arc<T>> {
        let resource = self.get_erased(address)?;
        resource.into_any_arc().downcast::<T>().ok()
    }
    pub fn snapshot(&self, address: &AssetAddress) -> Option<StreamingAssetSnapshot> {
        let entry = self.entries.get(address)?;
        Some(StreamingAssetSnapshot {
            address: entry.address.clone(),
            asset_id: AssetId::from_address(&entry.address),
            state: entry.state,
            external_claims: entry.external_claims.len(),
            dependency_claims: entry.dependency_claims.len(),
            effective_priority: entry.effective_priority(),
            pinned: entry.pinned(),
            dependencies: entry.dependencies.clone(),
            source_size_bytes: entry.source_size_bytes,
            last_touched_frame: entry.last_touched_frame,
            resident_since_frame: entry.resident_since_frame,
            error: entry.error.clone(),
        })
    }
    pub fn stats(&self) -> StreamingStats {
        let mut stats = StreamingStats {
            frame: self.frame,
            entries: self.entries.len(),
            resident_sources: self.resident_sources.len(),
            resident_bytes: self.resident_bytes(),
            total_loads: self.total_loads,
            total_evictions: self.total_evictions,
            total_failures: self.total_failures,
            ..StreamingStats::default()
        };

        for entry in self.entries.values() {
            stats.external_claims += entry.external_claims.len();
            stats.dependency_claims += entry.dependency_claims.len();
            match entry.state {
                StreamingState::Unloaded => {}
                StreamingState::Queued => stats.queued += 1,
                StreamingState::Loading => stats.loading += 1,
                StreamingState::WaitingForDependencies => stats.waiting_dependencies += 1,
                StreamingState::Resident => stats.resident += 1,
                StreamingState::Failed => stats.failed += 1,
            }
        }

        stats.over_budget = self.policy.max_resident_bytes != 0
            && stats.resident_bytes > self.policy.max_resident_bytes;
        stats
    }
    pub fn resident_bytes(&self) -> u64 {
        self.resident_sources
            .values()
            .map(|source| source.bytes)
            .sum()
    }
}
