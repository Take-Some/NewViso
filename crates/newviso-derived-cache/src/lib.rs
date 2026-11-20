#![forbid(unsafe_op_in_unsafe_fn)]
//! Content-addressed, opt-in derived-resource cache.
//!
//! This cache NEVER owns original VFS sources, controls physics readiness,
//! or returns unvalidated payload bytes. It is a best-effort optimization
//! behind the authoritative engine.assets semantic decoder.
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
const MAGIC: &[u8; 8] = b"NVDCC01\0";
const HEADER: usize = 8 + 32 + 8 + 32;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DerivedKey([u8; 32]);

impl DerivedKey {
    /// Framed components prevent `("ab", "c")` and `("a", "bc")` collisions.
    /// Revision must change whenever the provider changes semantic output.
    pub fn from_source(
        source: &[u8],
        logical_path: &str,
        selector: &str,
        output_kind: &str,
        decoder_revision: &str,
        provider_revision: &str,
    ) -> Self {
        let mut hash = blake3::Hasher::new();
        for part in [
            b"newviso.ddc.v1".as_slice(),
            blake3::hash(source).as_bytes().as_slice(),
            logical_path.as_bytes(),
            selector.as_bytes(),
            output_kind.as_bytes(),
            decoder_revision.as_bytes(),
            provider_revision.as_bytes(),
        ] {
            hash.update(&(part.len() as u64).to_le_bytes());
            hash.update(part);
        }
        Self(*hash.finalize().as_bytes())
    }
    pub fn hex(self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }
}

#[derive(Clone, Debug)]
pub struct DerivedDataCache {
    root: PathBuf,
    max_entry_bytes: usize,
    max_disk_bytes: u64,
}

impl DerivedDataCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            max_entry_bytes: 256 * 1024 * 1024,
            max_disk_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
    pub fn with_limits(mut self, max_entry_bytes: usize, max_disk_bytes: u64) -> Self {
        self.max_entry_bytes = max_entry_bytes;
        self.max_disk_bytes = max_disk_bytes;
        self
    }
    fn file_for(&self, key: DerivedKey) -> PathBuf {
        let id = key.hex();
        self.root.join(&id[..2]).join(format!("{id}.nvdc"))
    }
    /// Corrupt, mismatched, or stale records are treated as cache misses.
    pub fn read(&self, key: DerivedKey) -> Result<Option<Vec<u8>>, String> {
        let path = self.file_for(key);
        let mut f = match File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("DDC read '{}': {e}", path.display())),
        };
        let metadata = f.metadata().map_err(|e| e.to_string())?;
        if metadata.len() < HEADER as u64 || metadata.len() > (HEADER + self.max_entry_bytes) as u64
        {
            return Ok(None);
        }
        let mut header = [0_u8; HEADER];
        if f.read_exact(&mut header).is_err() {
            return Ok(None);
        }
        if &header[..8] != MAGIC || header[8..40] != key.0 {
            return Ok(None);
        }
        let len = u64::from_le_bytes(header[40..48].try_into().expect("length slice"));
        if len > self.max_entry_bytes as u64 || len + HEADER as u64 != metadata.len() {
            return Ok(None);
        }
        let mut bytes = vec![0; len as usize];
        if f.read_exact(&mut bytes).is_err() {
            return Ok(None);
        }
        if blake3::hash(&bytes).as_bytes() != &header[48..80] {
            return Ok(None);
        }
        Ok(Some(bytes))
    }
    /// Immutable record with atomic publication. Concurrent writers never
    /// alter an existing content-addressed record.
    pub fn write(&self, key: DerivedKey, bytes: &[u8]) -> Result<bool, String> {
        if bytes.len() > self.max_entry_bytes || (bytes.len() + HEADER) as u64 > self.max_disk_bytes
        {
            return Ok(false);
        }
        let target = self.file_for(key);
        if target.is_file() {
            // Repair corrupt DDC records after authoritative provider decode.
            if self.read(key)?.is_some() {
                return Ok(false);
            }
            fs::remove_file(&target).map_err(|error| {
                format!("DDC remove invalid record '{}': {error}", target.display())
            })?;
        }
        let parent = target.parent().expect("cache record parent");
        fs::create_dir_all(parent).map_err(|e| format!("DDC create parent: {e}"))?;
        let unique = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp = parent.join(format!(
            ".{}.{}.{}.tmp",
            key.hex(),
            std::process::id(),
            unique
        ));
        let result = (|| -> Result<(), String> {
            let mut f = File::create_new(&temp).map_err(|e| e.to_string())?;
            f.write_all(MAGIC).map_err(|e| e.to_string())?;
            f.write_all(&key.0).map_err(|e| e.to_string())?;
            f.write_all(&(bytes.len() as u64).to_le_bytes())
                .map_err(|e| e.to_string())?;
            f.write_all(blake3::hash(bytes).as_bytes())
                .map_err(|e| e.to_string())?;
            f.write_all(bytes).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            if target.exists() {
                return Ok(());
            }
            match fs::rename(&temp, &target) {
                Ok(()) => Ok(()),
                Err(_) if target.exists() => Ok(()),
                Err(e) => Err(format!("DDC publish: {e}")),
            }
        })();
        let _ = fs::remove_file(&temp);
        result?;
        self.prune()?;
        Ok(true)
    }
    /// FIFO-by-publication eviction. A hard disk budget is independent of
    /// decoded NEF8 and GPU residency budgets.
    pub fn prune(&self) -> Result<(), String> {
        if !self.root.exists() {
            return Ok(());
        }
        let mut entries = Vec::new();
        let mut total: u64 = 0;
        for group in fs::read_dir(&self.root).map_err(|e| e.to_string())? {
            let group = group.map_err(|e| e.to_string())?;
            if !group.path().is_dir() {
                continue;
            }
            for item in fs::read_dir(group.path()).map_err(|e| e.to_string())? {
                let item = item.map_err(|e| e.to_string())?;
                if item.path().extension().and_then(|e| e.to_str()) != Some("nvdc") {
                    continue;
                }
                let meta = item.metadata().map_err(|e| e.to_string())?;
                if !meta.is_file() {
                    continue;
                }
                total = total.saturating_add(meta.len());
                entries.push((meta.modified().ok(), item.path(), meta.len()));
            }
        }
        if total <= self.max_disk_bytes {
            return Ok(());
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        for (_, path, len) in entries {
            if total <= self.max_disk_bytes {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(len);
            }
        }
        Ok(())
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, DerivedDataCache) {
        let id = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("newviso-ddc-{}-{id}", std::process::id()));
        let cache = DerivedDataCache::new(&root).with_limits(512, 800);
        (root, cache)
    }
    fn key(source: &[u8]) -> DerivedKey {
        DerivedKey::from_source(
            source,
            "world/a.ytd",
            "diffuse",
            "NTRT-v2",
            "v2",
            "provider-3.8.1",
        )
    }
    #[test]
    fn key_is_sensitive_to_source_selector_and_revision() {
        let a = key(b"source");
        assert_ne!(a, key(b"changed"));
        assert_ne!(
            a,
            DerivedKey::from_source(
                b"source",
                "world/a.ytd",
                "normal",
                "NTRT-v2",
                "v2",
                "provider-3.8.1"
            )
        );
        assert_ne!(
            a,
            DerivedKey::from_source(
                b"source",
                "world/a.ytd",
                "diffuse",
                "NTRT-v2",
                "v2",
                "provider-3.8.2"
            )
        );
    }
    #[test]
    fn roundtrip_and_checksum_corruption_fall_back_to_miss() {
        let (root, cache) = fixture();
        let k = key(b"source");
        assert_eq!(cache.read(k).unwrap(), None);
        assert!(cache.write(k, b"texture-data").unwrap());
        assert_eq!(cache.read(k).unwrap(), Some(b"texture-data".to_vec()));
        let f = cache.file_for(k);
        let mut data = fs::read(&f).unwrap();
        *data.last_mut().unwrap() ^= 1;
        fs::write(&f, data).unwrap();
        assert_eq!(cache.read(k).unwrap(), None);
        assert!(cache.write(k, b"texture-data").unwrap());
        assert_eq!(cache.read(k).unwrap(), Some(b"texture-data".to_vec()));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn oversized_entries_are_rejected_without_writes() {
        let (root, cache) = fixture();
        assert!(!cache.write(key(b"one"), &[0; 600]).unwrap());
        assert!(!root.exists());
    }
    #[test]
    fn bounded_disk_budget_evicts_older_records() {
        let (root, cache) = fixture();
        for i in 0..5 {
            cache.write(key(&[i]), &[i; 240]).unwrap();
        }
        let count = (0..5)
            .filter(|i| cache.read(key(&[*i])).unwrap().is_some())
            .count();
        assert!(count <= 2);
        assert!(count >= 1);
        fs::remove_dir_all(root).unwrap();
    }
}
