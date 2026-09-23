mod streaming;
pub use streaming::*;

use newviso_assets_client::{normalize_logical_path, AssetClient};
use std::{any::Any, collections::HashMap, marker::PhantomData, sync::Arc};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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
        let (path, entry) = match value.rsplit_once('@') {
            Some((path, entry)) if !entry.trim().is_empty() => {
                (path, Some(entry.trim().to_owned()))
            }
            Some(_) => return Err(format!("asset address has empty @entry: '{value}'")),
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
    pub bytes: Vec<u8>,
    /// Generic dependency addresses supplied by the asset service/source.
    /// ResourceRuntime never derives these from a concrete file format.
    pub dependencies: Vec<AssetAddress>,
}

pub trait AssetSource: Send + Sync {
    fn resolve(&self, logical_path: &str) -> Result<ResolvedAsset, String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AssetClientSource;

impl AssetSource for AssetClientSource {
    fn resolve(&self, logical_path: &str) -> Result<ResolvedAsset, String> {
        let logical_path = normalize_logical_path(logical_path)?;
        let bytes = AssetClient::new().raw_bytes(&logical_path)?;
        let hash = blake3::hash(&bytes);
        Ok(ResolvedAsset {
            source: ResolvedAssetSource {
                mount_id: "engine.assets".to_owned(),
                logical_path,
                source_revision: u64::from_le_bytes(
                    hash.as_bytes()[0..8].try_into().expect("BLAKE3 prefix"),
                ),
            },
            bytes,
            dependencies: Vec::new(),
        })
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ResourceCacheKey {
    address: AssetAddress,
    vfs_generation: u64,
    source_revision: u64,
}

pub struct ResourceManager<S: AssetSource> {
    source: S,
    vfs_generation: u64,
    resources: HashMap<ResourceCacheKey, Arc<dyn AssetResource>>,
    states: HashMap<AssetAddress, ResourceRecord>,
}

impl<S: AssetSource> ResourceManager<S> {
    pub fn new(source: S) -> Self {
        Self {
            source,
            vfs_generation: 1,
            resources: HashMap::new(),
            states: HashMap::new(),
        }
    }

    pub fn bump_vfs_generation(&mut self) {
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

        let resolved = match self.source.resolve(address.logical_path()) {
            Ok(value) => value,
            Err(error) => return self.fail(address, error),
        };
        let source_size_bytes = resolved.bytes.len() as u64;
        let cache_key = ResourceCacheKey {
            address: address.clone(),
            vfs_generation: self.vfs_generation,
            source_revision: resolved.source.source_revision,
        };

        if let Some(resource) = self.resources.get(&cache_key).cloned() {
            let dependencies = resource.dependencies();
            self.states.insert(
                address.clone(),
                ResourceRecord {
                    state: ResourceState::Ready,
                    dependencies,
                    source: Some(resolved.source.clone()),
                    source_size_bytes,
                    error: None,
                },
            );
            return Ok(ResourceLoad {
                resource,
                source: resolved.source,
                source_size_bytes,
            });
        }

        let resource: Arc<dyn AssetResource> = Arc::new(ResidentAsset {
            id: AssetId::from_address(address),
            address: address.clone(),
            bytes: Arc::from(resolved.bytes),
            dependency_refs: resolved.dependencies,
        });
        let dependencies = resource.dependencies();
        self.states.insert(
            address.clone(),
            ResourceRecord {
                state: ResourceState::Ready,
                dependencies,
                source: Some(resolved.source.clone()),
                source_size_bytes,
                error: None,
            },
        );
        self.resources.insert(cache_key, resource.clone());
        Ok(ResourceLoad {
            resource,
            source: resolved.source,
            source_size_bytes,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_is_part_of_asset_identity_without_interpreting_file_type() {
        let body = AssetAddress::parse("assets/vehicle@body").unwrap();
        let wheel = AssetAddress::parse("assets/vehicle@wheel_fl").unwrap();
        assert_ne!(body, wheel);
        assert_ne!(AssetId::from_address(&body), AssetId::from_address(&wheel));
    }
}
