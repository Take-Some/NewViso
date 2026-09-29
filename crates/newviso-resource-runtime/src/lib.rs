mod streaming;
pub use streaming::*;

use newviso_assets_client::{normalize_logical_path, AssetClient};
use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    marker::PhantomData,
    sync::Arc,
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetAddress {
    path: String,
    entry: Option<String>,
}

impl AssetAddress {
    pub fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if value.is_empty() {
            return Err("asset address is empty".to_owned());
        }
        // '@' is an entry selector only after the filename extension:
        //   model.asset@entry  -> selector
        //   hi@district.ybn  -> literal filename
        // RSC7 uses '@' inside real basenames, so splitting on every final '@'
        // corrupts valid YBN/YDR paths before they reach AssetManager.
        let last_at = value.rfind('@');
        let last_sep = value
            .rfind(|character| character == '/' || character == '\\')
            .map(|index| index + 1)
            .unwrap_or(0);
        let last_dot = value[last_sep..].rfind('.').map(|index| last_sep + index);
        let selector_at = last_at.filter(|at| {
            *at >= last_sep
                && match last_dot {
                    Some(dot) => *at > dot,
                    None => true,
                }
        });
        let (path, entry) = match selector_at {
            Some(at) => {
                let path = &value[..at];
                let entry = value[at + 1..].trim();
                if entry.is_empty() {
                    return Err(format!("asset address has empty @entry: '{value}'"));
                }
                (path, Some(entry.to_owned()))
            }
            None => (value, None),
        };
        Ok(Self {
            path: normalize_logical_path(path)?,
            entry,
        })
    }

    pub fn logical_path(&self) -> &str {
        &self.path
    }
    pub fn entry(&self) -> Option<&str> {
        self.entry.as_deref()
    }
    pub fn canonical(&self) -> String {
        match &self.entry {
            Some(entry) => format!("{}@{}", self.path, entry),
            None => self.path.clone(),
        }
    }

    pub fn matches_canonical(&self, value: &str) -> bool {
        match self.entry.as_deref() {
            Some(entry) => {
                let Some((path, selector)) = value.rsplit_once('@') else {
                    return false;
                };
                path == self.path && selector == entry
            }
            None => value == self.path,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AssetId(pub u64);

impl AssetId {
    pub fn from_address(address: &AssetAddress) -> Self {
        let hash = blake3::hash(address.canonical().as_bytes());
        Self(u64::from_le_bytes(
            hash.as_bytes()[0..8].try_into().expect("BLAKE3 prefix"),
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AssetDomain(&'static str);

impl AssetDomain {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Debug)]
pub struct AssetRef<T: ?Sized> {
    address: AssetAddress,
    _marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized> Clone for AssetRef<T> {
    fn clone(&self) -> Self {
        Self::new(self.address.clone())
    }
}

impl<T: ?Sized> AssetRef<T> {
    pub fn new(address: AssetAddress) -> Self {
        Self {
            address,
            _marker: PhantomData,
        }
    }
    pub fn address(&self) -> &AssetAddress {
        &self.address
    }
}

pub trait AssetResource: Any + Send + Sync {
    fn asset_id(&self) -> AssetId;
    fn domain(&self) -> AssetDomain;
    fn dependencies(&self) -> Vec<AssetAddress> {
        Vec::new()
    }
    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

/// Format adapters live outside ResourceRuntime. A decoder may recognize a
/// source container and replace the generic ResidentAsset with an engine
/// semantic resource (model, texture, collision, ...). Returning Ok(None)
/// means "not my format" and lets the next decoder/fallback handle the bytes.
pub trait ResourceDecoder: Send + Sync {
    fn name(&self) -> &'static str;
    fn decode(
        &self,
        address: &AssetAddress,
        bytes: &[u8],
    ) -> Result<Option<Arc<dyn AssetResource>>, String>;
}

pub const RESIDENT_ASSET_DOMAIN: AssetDomain = AssetDomain::new("engine.assets.resident");

#[derive(Clone, Debug)]
pub struct ResidentAsset {
    pub id: AssetId,
    pub address: AssetAddress,
    pub bytes: Arc<[u8]>,
    pub dependency_refs: Vec<AssetAddress>,
}

impl AssetResource for ResidentAsset {
    fn asset_id(&self) -> AssetId {
        self.id
    }
    fn domain(&self) -> AssetDomain {
        RESIDENT_ASSET_DOMAIN
    }
    fn dependencies(&self) -> Vec<AssetAddress> {
        self.dependency_refs.clone()
    }
    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ResolvedAssetSource {
    pub mount_id: String,
    pub logical_path: String,
    pub source_revision: u64,
}

pub struct ResolvedAsset {
    pub source: ResolvedAssetSource,
    pub bytes: Arc<[u8]>,
    /// Generic dependency addresses supplied by the asset service/source.
    /// ResourceRuntime never derives these from a concrete file format.
    pub dependencies: Vec<AssetAddress>,
}

pub trait AssetSource: Send + Sync {
    fn resolve(&self, logical_path: &str) -> Result<ResolvedAsset, String>;

    fn invalidate(&self) {}
}

const ASSET_CLIENT_RAW_CACHE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone)]
struct CachedRawSource {
    bytes: Arc<[u8]>,
    source_revision: u64,
}

enum RawSourceCacheEntry {
    Loading,
    Ready(CachedRawSource),
}

#[derive(Default)]
struct RawSourceCacheState {
    entries: HashMap<String, RawSourceCacheEntry>,
    order: VecDeque<String>,
    resident_bytes: u64,
}

#[derive(Default)]
struct RawSourceCache {
    state: std::sync::Mutex<RawSourceCacheState>,
    wake: std::sync::Condvar,
}

#[derive(Clone, Default)]
pub struct AssetClientSource {
    cache: Arc<RawSourceCache>,
}

impl std::fmt::Debug for AssetClientSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AssetClientSource")
            .finish_non_exhaustive()
    }
}

impl AssetSource for AssetClientSource {
    fn resolve(&self, logical_path: &str) -> Result<ResolvedAsset, String> {
        let logical_path = normalize_logical_path(logical_path)?;

        loop {
            let mut state = self.cache.state.lock().expect("raw source cache poisoned");
            match state.entries.get(&logical_path) {
                Some(RawSourceCacheEntry::Ready(cached)) => {
                    return Ok(ResolvedAsset {
                        source: ResolvedAssetSource {
                            mount_id: "engine.assets".to_owned(),
                            logical_path,
                            source_revision: cached.source_revision,
                        },
                        bytes: Arc::clone(&cached.bytes),
                        dependencies: Vec::new(),
                    });
                }
                Some(RawSourceCacheEntry::Loading) => {
                    state = self
                        .cache
                        .wake
                        .wait(state)
                        .expect("raw source cache poisoned");
                    drop(state);
                    continue;
                }
                None => {
                    state
                        .entries
                        .insert(logical_path.clone(), RawSourceCacheEntry::Loading);
                    break;
                }
            }
        }

        let loaded = AssetClient::new().raw_bytes(&logical_path);
        let mut state = self.cache.state.lock().expect("raw source cache poisoned");
        match loaded {
            Ok(bytes) => {
                let bytes: Arc<[u8]> = Arc::from(bytes);
                let hash = blake3::hash(bytes.as_ref());
                let source_revision =
                    u64::from_le_bytes(hash.as_bytes()[0..8].try_into().expect("BLAKE3 prefix"));
                let cached = CachedRawSource {
                    bytes: Arc::clone(&bytes),
                    source_revision,
                };
                state.resident_bytes = state.resident_bytes.saturating_add(bytes.len() as u64);
                state.order.push_back(logical_path.clone());
                state
                    .entries
                    .insert(logical_path.clone(), RawSourceCacheEntry::Ready(cached));

                while state.resident_bytes > ASSET_CLIENT_RAW_CACHE_BYTES {
                    let Some(oldest) = state.order.pop_front() else {
                        break;
                    };
                    if oldest == logical_path {
                        state.order.push_back(oldest);
                        break;
                    }
                    if let Some(RawSourceCacheEntry::Ready(old)) = state.entries.remove(&oldest) {
                        state.resident_bytes =
                            state.resident_bytes.saturating_sub(old.bytes.len() as u64);
                    }
                }
                self.cache.wake.notify_all();
                Ok(ResolvedAsset {
                    source: ResolvedAssetSource {
                        mount_id: "engine.assets".to_owned(),
                        logical_path,
                        source_revision,
                    },
                    bytes,
                    dependencies: Vec::new(),
                })
            }
            Err(error) => {
                state.entries.remove(&logical_path);
                self.cache.wake.notify_all();
                Err(error)
            }
        }
    }

    fn invalidate(&self) {
        let mut state = self.cache.state.lock().expect("raw source cache poisoned");
        state.entries.clear();
        state.order.clear();
        state.resident_bytes = 0;
        self.cache.wake.notify_all();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceState {
    Unloaded,
    Loading,
    ResolvingDependencies,
    Ready,
    Failed,
}

#[derive(Clone, Debug)]
pub struct ResourceRecord {
    pub state: ResourceState,
    pub dependencies: Vec<AssetAddress>,
    pub source: Option<ResolvedAssetSource>,
    pub source_size_bytes: u64,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct ResourceLoad {
    pub resource: Arc<dyn AssetResource>,
    pub source: ResolvedAssetSource,
    pub source_size_bytes: u64,
}

pub type ResourcePrepareFn =
    Arc<dyn Fn(&AssetAddress) -> Result<ResourceLoad, String> + Send + Sync + 'static>;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ResourceCacheKey {
    address: AssetAddress,
    vfs_generation: u64,
    source_revision: u64,
}

pub struct ResourceManager<S: AssetSource> {
    source: Arc<S>,
    vfs_generation: u64,
    decoders: Vec<Arc<dyn ResourceDecoder>>,
    resources: HashMap<ResourceCacheKey, Arc<dyn AssetResource>>,
    states: HashMap<AssetAddress, ResourceRecord>,
}

impl<S: AssetSource> ResourceManager<S> {
    pub fn new(source: S) -> Self {
        Self {
            source: Arc::new(source),
            vfs_generation: 1,
            decoders: Vec::new(),
            resources: HashMap::new(),
            states: HashMap::new(),
        }
    }

    pub fn register_decoder<D>(&mut self, decoder: D)
    where
        D: ResourceDecoder + 'static,
    {
        self.decoders.push(Arc::new(decoder));
        // Decoder order is part of semantic resolution. Existing cached
        // fallbacks must not survive a registry change.
        self.bump_vfs_generation();
    }

    pub fn decoder_names(&self) -> Vec<&'static str> {
        self.decoders.iter().map(|decoder| decoder.name()).collect()
    }

    pub fn bump_vfs_generation(&mut self) {
        self.source.invalidate();
        self.vfs_generation = self.vfs_generation.wrapping_add(1).max(1);
        self.resources.clear();
        self.states.clear();
    }

    pub fn state(&self, address: &AssetAddress) -> ResourceState {
        self.states
            .get(address)
            .map(|r| r.state)
            .unwrap_or(ResourceState::Unloaded)
    }

    pub fn record(&self, address: &AssetAddress) -> Option<&ResourceRecord> {
        self.states.get(address)
    }

    pub fn load_erased(
        &mut self,
        address: &AssetAddress,
    ) -> Result<Arc<dyn AssetResource>, String> {
        self.load_erased_with_info(address)
            .map(|load| load.resource)
    }

    pub fn load_erased_with_info(
        &mut self,
        address: &AssetAddress,
    ) -> Result<ResourceLoad, String> {
        self.states.insert(
            address.clone(),
            ResourceRecord {
                state: ResourceState::Loading,
                dependencies: Vec::new(),
                source: None,
                source_size_bytes: 0,
                error: None,
            },
        );
        let prepared = self.prepare_erased_with_info(address);
        self.commit_prepared_load(address, prepared)
    }

    /// Immutable decode function for persistent streaming workers.
    ///
    /// It intentionally does not mutate or consult the owner-thread resource
    /// cache. AssetStreamer guarantees one in-flight job per address; the
    /// completed ResourceLoad is committed into the cache deterministically on
    /// the owner thread.
    pub fn prepare_fn(&self) -> ResourcePrepareFn
    where
        S: 'static,
    {
        let source = Arc::clone(&self.source);
        let decoders = self.decoders.clone();
        Arc::new(move |address| prepare_uncached(source.as_ref(), &decoders, address))
    }

    /// Resolve and decode a resource without mutating ResourceManager state.
    ///
    /// AssetStreamer uses this to run independent source I/O + semantic decode
    /// concurrently. The resulting load is committed on the owning thread so
    /// cache/state mutation remains deterministic.
    pub fn prepare_erased_with_info(&self, address: &AssetAddress) -> Result<ResourceLoad, String> {
        let resolved = self.source.as_ref().resolve(address.logical_path())?;
        let source_size_bytes = resolved.bytes.len() as u64;
        let cache_key = ResourceCacheKey {
            address: address.clone(),
            vfs_generation: self.vfs_generation,
            source_revision: resolved.source.source_revision,
        };

        if let Some(resource) = self.resources.get(&cache_key).cloned() {
            return Ok(ResourceLoad {
                resource,
                source: resolved.source,
                source_size_bytes,
            });
        }

        let mut decoded: Option<Arc<dyn AssetResource>> = None;
        for decoder in &self.decoders {
            match decoder.decode(address, resolved.bytes.as_ref()) {
                Ok(Some(resource)) => {
                    decoded = Some(resource);
                    break;
                }
                Ok(None) => {}
                Err(error) => {
                    return Err(format!(
                        "resource decoder '{}' failed asset '{}': {error}",
                        decoder.name(),
                        address.canonical()
                    ));
                }
            }
        }

        let resource: Arc<dyn AssetResource> = decoded.unwrap_or_else(|| {
            Arc::new(ResidentAsset {
                id: AssetId::from_address(address),
                address: address.clone(),
                bytes: Arc::clone(&resolved.bytes),
                dependency_refs: resolved.dependencies,
            })
        });

        Ok(ResourceLoad {
            resource,
            source: resolved.source,
            source_size_bytes,
        })
    }

    /// Commit a prepared parallel load on the ResourceManager owner thread.
    pub fn commit_prepared_load(
        &mut self,
        address: &AssetAddress,
        prepared: Result<ResourceLoad, String>,
    ) -> Result<ResourceLoad, String> {
        let load = match prepared {
            Ok(load) => load,
            Err(error) => return self.fail(address, error),
        };
        let dependencies = load.resource.dependencies();
        let cache_key = ResourceCacheKey {
            address: address.clone(),
            vfs_generation: self.vfs_generation,
            source_revision: load.source.source_revision,
        };
        self.states.insert(
            address.clone(),
            ResourceRecord {
                state: ResourceState::Ready,
                dependencies,
                source: Some(load.source.clone()),
                source_size_bytes: load.source_size_bytes,
                error: None,
            },
        );
        self.resources.insert(cache_key, load.resource.clone());
        Ok(load)
    }

    pub fn unload(&mut self, address: &AssetAddress) -> bool {
        let before = self.resources.len();
        self.resources.retain(|key, _| &key.address != address);
        self.states.insert(
            address.clone(),
            ResourceRecord {
                state: ResourceState::Unloaded,
                dependencies: Vec::new(),
                source: None,
                source_size_bytes: 0,
                error: None,
            },
        );
        self.resources.len() != before
    }

    pub fn cached_resource_count(&self) -> usize {
        self.resources.len()
    }
    pub fn cached_container_count(&self) -> usize {
        0
    }

    pub fn load<T: AssetResource + 'static>(
        &mut self,
        address: &AssetAddress,
    ) -> Result<Arc<T>, String> {
        let resource = self.load_erased(address)?;
        let actual_domain = resource.domain();
        resource.into_any_arc().downcast::<T>().map_err(|_| format!(
            "asset '{}' is resident as semantic domain '{}' but requested Rust resource type differs",
            address.canonical(), actual_domain.as_str()
        ))
    }

    fn fail<T>(&mut self, address: &AssetAddress, error: String) -> Result<T, String> {
        self.states.insert(
            address.clone(),
            ResourceRecord {
                state: ResourceState::Failed,
                dependencies: Vec::new(),
                source: None,
                source_size_bytes: 0,
                error: Some(error.clone()),
            },
        );
        Err(error)
    }
}

fn prepare_uncached<S: AssetSource>(
    source: &S,
    decoders: &[Arc<dyn ResourceDecoder>],
    address: &AssetAddress,
) -> Result<ResourceLoad, String> {
    let resolved = source.resolve(address.logical_path())?;
    let source_size_bytes = resolved.bytes.len() as u64;

    let mut decoded: Option<Arc<dyn AssetResource>> = None;
    for decoder in decoders {
        match decoder.decode(address, resolved.bytes.as_ref()) {
            Ok(Some(resource)) => {
                decoded = Some(resource);
                break;
            }
            Ok(None) => {}
            Err(error) => {
                return Err(format!(
                    "resource decoder '{}' failed asset '{}': {error}",
                    decoder.name(),
                    address.canonical()
                ));
            }
        }
    }

    let resource: Arc<dyn AssetResource> = decoded.unwrap_or_else(|| {
        Arc::new(ResidentAsset {
            id: AssetId::from_address(address),
            address: address.clone(),
            bytes: Arc::clone(&resolved.bytes),
            dependency_refs: resolved.dependencies,
        })
    });

    Ok(ResourceLoad {
        resource,
        source: resolved.source,
        source_size_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_is_part_of_asset_identity_without_interpreting_file_type() {
        let body = AssetAddress::parse("assets/vehicle.asset@body").unwrap();
        let wheel = AssetAddress::parse("assets/vehicle.asset@wheel_fl").unwrap();
        assert_ne!(body, wheel);
        assert_ne!(AssetId::from_address(&body), AssetId::from_address(&wheel));
    }

    #[test]
    fn at_sign_before_extension_is_a_literal_filename_character() {
        let address = AssetAddress::parse("maps/import/ybn/hi@bh1_06_0.ybn").unwrap();
        assert_eq!(address.logical_path(), "maps/import/ybn/hi@bh1_06_0.ybn");
        assert_eq!(address.entry(), None);
    }

    #[test]
    fn at_sign_after_extension_is_an_entry_selector() {
        let address = AssetAddress::parse("models/world.asset@building_high").unwrap();
        assert_eq!(address.logical_path(), "models/world.asset");
        assert_eq!(address.entry(), Some("building_high"));
    }
}
