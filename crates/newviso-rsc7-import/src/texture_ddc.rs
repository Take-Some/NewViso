//! Opt-in derived NTRT v2 packet cache. Source bytes, decoder contract,
//! selected entry and provider revision are part of its content-derived key.
//! Invalid/cache-unavailable packets never replace authoritative engine.assets.
use newviso_derived_cache::{DerivedDataCache, DerivedKey};
use newviso_resource_runtime::AssetAddress;
use newviso_textures::TextureResource;

use crate::decode_texture_resource;

pub(super) struct TextureCacheBinding {
    cache: DerivedDataCache,
    key: DerivedKey,
}

impl TextureCacheBinding {
    pub(super) fn from_environment(
        source: &[u8],
        address: &AssetAddress,
        entry: &str,
        output_kind: &str,
    ) -> Option<Self> {
        // Require an explicit provider revision. This makes cache invalidation
        // a conscious choice when replacing a StarVault/codec DLL rather than
        // accidentally reusing outputs from a different implementation.
        let root = std::env::var_os("NEWVISO_DDC_DIR").filter(|path| !path.is_empty())?;
        let revision = std::env::var("NEWVISO_DDC_PROVIDER_REVISION")
            .ok()
            .filter(|revision| !revision.trim().is_empty())?;
        let key = DerivedKey::from_source(
            source,
            address.logical_path(),
            entry,
            output_kind,
            concat!(env!("CARGO_PKG_VERSION"), ":NTRT-v2"),
            &revision,
        );
        Some(Self {
            cache: DerivedDataCache::new(root),
            key,
        })
    }

    pub(super) fn read_validated(
        &self,
        address: &AssetAddress,
        entry: &str,
    ) -> Option<TextureResource> {
        let packet = match self.cache.read(self.key) {
            Ok(Some(packet)) => packet,
            _ => return None,
        };
        // A valid BLAKE3 cache record is not necessarily a valid semantic
        // texture: the runtime parser remains authoritative on every hit.
        let decoded = decode_texture_resource(address, entry, &packet).ok()?;
        if std::env::var_os("NEWVISO_PERF_TRACE").is_some() {
            newviso_host::debug(
                "newviso.assets.ddc",
                format!(
                    "hit source={} entry={} bytes={}",
                    address.logical_path(),
                    entry,
                    packet.len()
                ),
            );
        }
        Some(decoded)
    }

    pub(super) fn store(&self, packet: &[u8]) {
        // Cache failures must never change gameplay or readiness behavior.
        if let Err(error) = self.cache.write(self.key, packet) {
            newviso_host::debug("newviso.assets.ddc", format!("write skipped: {error}"));
        }
    }
}
