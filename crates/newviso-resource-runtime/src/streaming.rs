use crate::{
    AssetAddress, AssetId, AssetResource, AssetSource, ResolvedAssetSource, ResourceLoad,
    ResourceManager,
};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    sync::Arc,
};

mod dependencies;
mod eviction;
mod loading;
mod requests;

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
