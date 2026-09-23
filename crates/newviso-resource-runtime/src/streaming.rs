use crate::{
    AssetAddress, AssetId, AssetResource, AssetSource, ResolvedAssetSource, ResourceLoad,
    ResourceManager,
};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StreamingOwnerId(pub u64);

impl StreamingOwnerId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn from_label(label: &str) -> Self {
        let hash = blake3::hash(label.as_bytes());
        Self(u64::from_le_bytes(
            hash.as_bytes()[0..8].try_into().expect("BLAKE3 prefix"),
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamingClaim {
    pub priority: f32,
    pub pinned: bool,
}

impl StreamingClaim {
    pub fn new(priority: f32) -> Self {
        Self {
            priority,
            pinned: false,
        }
    }

    pub fn pinned(priority: f32) -> Self {
        Self {
            priority,
            pinned: true,
        }
    }

    fn validate(self) -> Result<Self, String> {
        if !self.priority.is_finite() || self.priority < 0.0 {
            return Err(format!(
                "streaming priority must be finite and >= 0, got {}",
                self.priority
            ));
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamingState {
    Unloaded,
    Queued,
    Loading,
    WaitingForDependencies,
    Resident,
    Failed,
}

#[derive(Clone, Debug)]
pub struct StreamingPolicy {
    /// Unique source-container bytes retained by the streamer.
    /// Zero means unlimited.
    pub max_resident_bytes: u64,
    /// Maximum number of resources decoded per pump.
    /// Zero means unlimited.
    pub max_loads_per_tick: usize,
    /// Approximate source bytes decoded per pump.
    /// Zero means unlimited. A single resource may exceed this limit.
    pub max_source_bytes_per_tick: u64,
    /// Keep unrequested resources resident for this many pump frames.
    pub eviction_grace_frames: u64,
    /// Retry a failed requested resource after this many pump frames.
    /// Zero retries on the next pump.
    pub failed_retry_frames: u64,
    /// Priority inherited by dependencies from their parent.
    pub dependency_priority_scale: f32,
}

impl Default for StreamingPolicy {
    fn default() -> Self {
        Self {
            max_resident_bytes: 512 * 1024 * 1024,
            max_loads_per_tick: 8,
            max_source_bytes_per_tick: 32 * 1024 * 1024,
            eviction_grace_frames: 120,
            failed_retry_frames: 120,
            dependency_priority_scale: 0.95,
        }
    }
}

impl StreamingPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if !self.dependency_priority_scale.is_finite()
            || self.dependency_priority_scale < 0.0
            || self.dependency_priority_scale > 1.0
        {
            return Err(format!(
                "dependency_priority_scale must be finite and in 0..=1, got {}",
                self.dependency_priority_scale
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct StreamingAssetSnapshot {
    pub address: AssetAddress,
    pub asset_id: AssetId,
    pub state: StreamingState,
    pub external_claims: usize,
    pub dependency_claims: usize,
    pub effective_priority: f32,
    pub pinned: bool,
    pub dependencies: Vec<AssetAddress>,
    pub source_size_bytes: u64,
    pub last_touched_frame: u64,
    pub resident_since_frame: Option<u64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct StreamingStats {
    pub frame: u64,
    pub entries: usize,
    pub queued: usize,
    pub loading: usize,
    pub waiting_dependencies: usize,
    pub resident: usize,
    pub failed: usize,
    pub resident_sources: usize,
    pub resident_bytes: u64,
    pub external_claims: usize,
    pub dependency_claims: usize,
    pub total_loads: u64,
    pub total_evictions: u64,
    pub total_failures: u64,
    pub over_budget: bool,
}

#[derive(Clone, Debug, Default)]
pub struct StreamingTickReport {
    pub frame: u64,
    pub loaded: Vec<AssetAddress>,
    pub became_resident: Vec<AssetAddress>,
    pub evicted: Vec<AssetAddress>,
    pub failed: Vec<(AssetAddress, String)>,
    pub source_bytes_loaded: u64,
    pub resident_bytes: u64,
    pub over_budget: bool,
}

struct StreamingEntry {
    address: AssetAddress,
    state: StreamingState,
    external_claims: HashMap<StreamingOwnerId, StreamingClaim>,
    dependency_claims: HashMap<AssetAddress, f32>,
    dependencies: Vec<AssetAddress>,
    resource: Option<Arc<dyn AssetResource>>,
    source: Option<ResolvedAssetSource>,
    source_size_bytes: u64,
    last_touched_frame: u64,
    resident_since_frame: Option<u64>,
    failure_frame: Option<u64>,
    error: Option<String>,
}

impl StreamingEntry {
    fn new(address: AssetAddress, frame: u64) -> Self {
        Self {
            address,
            state: StreamingState::Unloaded,
            external_claims: HashMap::new(),
            dependency_claims: HashMap::new(),
            dependencies: Vec::new(),
            resource: None,
            source: None,
            source_size_bytes: 0,
            last_touched_frame: frame,
            resident_since_frame: None,
            failure_frame: None,
            error: None,
        }
    }

    fn has_claims(&self) -> bool {
        !self.external_claims.is_empty() || !self.dependency_claims.is_empty()
    }

    fn effective_priority(&self) -> f32 {
        self.external_claims
            .values()
            .map(|claim| claim.priority)
            .chain(self.dependency_claims.values().copied())
            .fold(0.0_f32, f32::max)
    }

    fn pinned(&self) -> bool {
        self.external_claims.values().any(|claim| claim.pinned)
    }
}

#[derive(Clone, Debug)]
struct ResidentSource {
    bytes: u64,
    references: usize,
}

pub struct AssetStreamer<S: AssetSource> {
    resources: ResourceManager<S>,
    policy: StreamingPolicy,
    entries: HashMap<AssetAddress, StreamingEntry>,
    resident_sources: HashMap<ResolvedAssetSource, ResidentSource>,
    frame: u64,
    total_loads: u64,
    total_evictions: u64,
    total_failures: u64,
}

impl<S: AssetSource> AssetStreamer<S> {
    pub fn new(resources: ResourceManager<S>, policy: StreamingPolicy) -> Result<Self, String> {
        policy.validate()?;
        Ok(Self {
            resources,
            policy,
            entries: HashMap::new(),
            resident_sources: HashMap::new(),
            frame: 0,
            total_loads: 0,
            total_evictions: 0,
            total_failures: 0,
        })
    }

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

        while report.loaded.len() + report.failed.len() < max_loads {
            if self.policy.max_source_bytes_per_tick != 0
                && report.source_bytes_loaded >= self.policy.max_source_bytes_per_tick
            {
                break;
            }

            let Some(address) = self.next_load_candidate() else {
                break;
            };

            match self.load_one(&address) {
                Ok(source_bytes) => {
                    report.source_bytes_loaded =
                        report.source_bytes_loaded.saturating_add(source_bytes);
                    report.loaded.push(address);
                }
                Err(error) => {
                    report.failed.push((address, error));
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

    fn next_load_candidate(&self) -> Option<AssetAddress> {
        self.entries
            .values()
            .filter(|entry| {
                entry.has_claims()
                    && matches!(
                        entry.state,
                        StreamingState::Unloaded | StreamingState::Queued
                    )
            })
            .max_by(|a, b| compare_load_candidates(a, b))
            .map(|entry| entry.address.clone())
    }

    fn load_one(&mut self, address: &AssetAddress) -> Result<u64, String> {
        if let Some(entry) = self.entries.get_mut(address) {
            entry.state = StreamingState::Loading;
            entry.error = None;
        }

        let load = match self.resources.load_erased_with_info(address) {
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

    fn mark_failed_loaded(&mut self, address: &AssetAddress, load: ResourceLoad, error: String) {
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

    fn acquire_source(&mut self, load: &ResourceLoad) {
        let source = self
            .resident_sources
            .entry(load.source.clone())
            .or_insert(ResidentSource {
                bytes: load.source_size_bytes,
                references: 0,
            });
        source.bytes = source.bytes.max(load.source_size_bytes);
        source.references = source.references.saturating_add(1);
    }

    fn release_source(&mut self, source: &ResolvedAssetSource) {
        let remove = if let Some(record) = self.resident_sources.get_mut(source) {
            record.references = record.references.saturating_sub(1);
            record.references == 0
        } else {
            false
        };
        if remove {
            self.resident_sources.remove(source);
        }
    }

    fn refresh_dependency_claims(&mut self, parent: &AssetAddress) {
        let Some(entry) = self.entries.get(parent) else {
            return;
        };
        if entry.dependencies.is_empty() {
            return;
        }

        let inherited_priority = entry.effective_priority() * self.policy.dependency_priority_scale;
        let dependencies = entry.dependencies.clone();
        for dependency in dependencies {
            let child = self
                .entries
                .entry(dependency.clone())
                .or_insert_with(|| StreamingEntry::new(dependency.clone(), self.frame));
            child
                .dependency_claims
                .insert(parent.clone(), inherited_priority);
            child.last_touched_frame = self.frame;
            if child.state == StreamingState::Unloaded {
                child.state = StreamingState::Queued;
            }
            self.refresh_dependency_claims(&dependency);
        }
    }

    fn remove_dependency_claims(&mut self, parent: &AssetAddress, dependencies: &[AssetAddress]) {
        for dependency in dependencies {
            if let Some(child) = self.entries.get_mut(dependency) {
                child.dependency_claims.remove(parent);
                child.last_touched_frame = self.frame;
                if !child.has_claims()
                    && matches!(child.state, StreamingState::Queued | StreamingState::Failed)
                {
                    child.state = StreamingState::Unloaded;
                    child.error = None;
                    child.failure_frame = None;
                }
            }
        }
    }

    fn promote_ready_entries(&mut self, report: &mut StreamingTickReport) {
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

    fn requeue_retryable_failures(&mut self) {
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

    fn reconcile_unclaimed_entries(&mut self) {
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

    fn evict_expired_unclaimed(&mut self, report: &mut StreamingTickReport) {
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

    fn evict_to_budget(&mut self, report: &mut StreamingTickReport) {
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

    fn evict_entry(&mut self, address: &AssetAddress, force: bool) -> bool {
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

    fn dependency_reaches(&self, start: &AssetAddress, target: &AssetAddress) -> bool {
        if start == target {
            return true;
        }

        let mut visited = HashSet::new();
        let mut stack = vec![start.clone()];
        while let Some(current) = stack.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if &current == target {
                return true;
            }
            if let Some(entry) = self.entries.get(&current) {
                stack.extend(entry.dependencies.iter().cloned());
            }
        }
        false
    }
}

fn compare_load_candidates(a: &StreamingEntry, b: &StreamingEntry) -> Ordering {
    a.effective_priority()
        .partial_cmp(&b.effective_priority())
        .unwrap_or(Ordering::Equal)
        .then_with(|| b.last_touched_frame.cmp(&a.last_touched_frame))
        .then_with(|| b.address.canonical().cmp(&a.address.canonical()))
}

fn compare_eviction_candidates(a: &StreamingEntry, b: &StreamingEntry) -> Ordering {
    a.last_touched_frame
        .cmp(&b.last_touched_frame)
        .then_with(|| {
            a.effective_priority()
                .partial_cmp(&b.effective_priority())
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| b.source_size_bytes.cmp(&a.source_size_bytes))
        .then_with(|| a.address.canonical().cmp(&b.address.canonical()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ResolvedAsset, ResolvedAssetSource};

    #[derive(Clone)]
    struct TestSource {
        assets: Arc<HashMap<String, (Vec<u8>, Vec<AssetAddress>)>>,
    }

    impl AssetSource for TestSource {
        fn resolve(&self, logical_path: &str) -> Result<ResolvedAsset, String> {
            let (bytes, dependencies) = self
                .assets
                .get(logical_path)
                .cloned()
                .ok_or_else(|| format!("test asset '{logical_path}' not found"))?;
            let hash = blake3::hash(&bytes);
            Ok(ResolvedAsset {
                source: ResolvedAssetSource {
                    mount_id: "test".to_owned(),
                    logical_path: logical_path.to_owned(),
                    source_revision: u64::from_le_bytes(
                        hash.as_bytes()[0..8].try_into().expect("hash prefix"),
                    ),
                },
                bytes,
                dependencies,
            })
        }
    }

    fn streamer(
        assets: HashMap<String, (Vec<u8>, Vec<AssetAddress>)>,
        policy: StreamingPolicy,
    ) -> AssetStreamer<TestSource> {
        let resources = ResourceManager::new(TestSource {
            assets: Arc::new(assets),
        });
        AssetStreamer::new(resources, policy).unwrap()
    }

    #[test]
    fn generic_dependency_closure_becomes_resident() {
        let child = AssetAddress::parse("assets/child@main").unwrap();
        let root = AssetAddress::parse("assets/root@main").unwrap();
        let assets = HashMap::from([
            ("assets/root".to_owned(), (vec![1; 32], vec![child.clone()])),
            ("assets/child".to_owned(), (vec![2; 16], Vec::new())),
        ]);
        let mut policy = StreamingPolicy::default();
        policy.max_loads_per_tick = 1;
        policy.max_resident_bytes = 0;
        let mut streaming = streamer(assets, policy);
        streaming
            .request(
                StreamingOwnerId::new(1),
                root.clone(),
                StreamingClaim::new(10.0),
            )
            .unwrap();

        streaming.pump();
        assert_eq!(
            streaming.state(&root),
            StreamingState::WaitingForDependencies
        );
        assert_eq!(streaming.state(&child), StreamingState::Queued);
        streaming.pump();
        assert_eq!(streaming.state(&child), StreamingState::Resident);
        assert_eq!(streaming.state(&root), StreamingState::Resident);
    }

    #[test]
    fn highest_priority_request_loads_first() {
        let assets = HashMap::from([
            ("assets/low".to_owned(), (vec![1; 8], Vec::new())),
            ("assets/high".to_owned(), (vec![2; 8], Vec::new())),
        ]);
        let mut policy = StreamingPolicy::default();
        policy.max_loads_per_tick = 1;
        policy.max_resident_bytes = 0;
        let mut streaming = streamer(assets, policy);
        let owner = StreamingOwnerId::new(2);
        let low = AssetAddress::parse("assets/low@main").unwrap();
        let high = AssetAddress::parse("assets/high@main").unwrap();
        streaming
            .request(owner, low.clone(), StreamingClaim::new(1.0))
            .unwrap();
        streaming
            .request(owner, high.clone(), StreamingClaim::new(50.0))
            .unwrap();
        streaming.pump();
        assert_eq!(streaming.state(&high), StreamingState::Resident);
        assert_eq!(streaming.state(&low), StreamingState::Queued);
    }

    #[test]
    fn shared_source_is_accounted_once() {
        let bytes = vec![7; 128];
        let assets = HashMap::from([("assets/bundle".to_owned(), (bytes.clone(), Vec::new()))]);
        let mut policy = StreamingPolicy::default();
        policy.max_loads_per_tick = 2;
        policy.max_resident_bytes = 0;
        let mut streaming = streamer(assets, policy);
        let owner = StreamingOwnerId::new(3);
        let first = AssetAddress::parse("assets/bundle@first").unwrap();
        let second = AssetAddress::parse("assets/bundle@second").unwrap();
        streaming
            .request(owner, first.clone(), StreamingClaim::new(1.0))
            .unwrap();
        streaming
            .request(owner, second.clone(), StreamingClaim::new(1.0))
            .unwrap();
        streaming.pump();
        assert!(streaming.is_resident(&first));
        assert!(streaming.is_resident(&second));
        assert_eq!(streaming.stats().resident_sources, 1);
        assert_eq!(streaming.resident_bytes(), bytes.len() as u64);
    }
}
