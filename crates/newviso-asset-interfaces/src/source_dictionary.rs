use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub const SOURCE_DICTIONARY_SCHEMA_V1: &str = "newengine.assets.source_dictionary.v1";
pub const SOURCE_DICTIONARY_SNAPSHOT_SCHEMA_V1: &str =
    "newengine.assets.source_dictionary.snapshot.v1";
pub const SOURCE_DICTIONARY_MANIFEST_FILE: &str = "source-dictionary.json";

const SOURCE_DICTIONARY_MAGIC: [u8; 4] = *b"NSDS";
const SOURCE_DICTIONARY_VERSION: u16 = 1;
const SOURCE_DICTIONARY_HEADER_LEN: usize = 24;
const MAX_SOURCE_DICTIONARY_METADATA_BYTES: usize = 16 * 1024 * 1024;
const MAX_SOURCE_DICTIONARY_ENTRIES: usize = 16_384;
const MAX_SOURCE_DICTIONARY_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceDictionaryManifestV1 {
    pub schema: String,
    pub logical_path: String,
    pub importer: String,
    pub primary: String,
    pub files: Vec<String>,
    pub options: BTreeMap<String, String>,
}

impl Default for SourceDictionaryManifestV1 {
    fn default() -> Self {
        Self {
            schema: SOURCE_DICTIONARY_SCHEMA_V1.to_owned(),
            logical_path: String::new(),
            importer: String::new(),
            primary: String::new(),
            files: Vec::new(),
            options: BTreeMap::new(),
        }
    }
}

impl SourceDictionaryManifestV1 {
    pub fn validate_for_runtime_path(&self, runtime_path: &str) -> Result<(), String> {
        if self.schema != SOURCE_DICTIONARY_SCHEMA_V1 {
            return Err(format!(
                "source dictionary manifest schema '{}' is unsupported; expected '{}'",
                self.schema, SOURCE_DICTIONARY_SCHEMA_V1
            ));
        }
        let actual = normalize_logical_path(&self.logical_path)?;
        let runtime_base = runtime_path
            .split_once('@')
            .map_or(runtime_path, |(base, _)| base);
        let expected = normalize_logical_path(runtime_base)?;
        if actual != expected {
            return Err(format!(
                "source dictionary logical_path mismatch manifest='{actual}' runtime='{expected}'"
            ));
        }
        if self.importer.trim().is_empty() {
            return Err("source dictionary manifest importer is empty".to_owned());
        }
        let primary = normalize_relative_path(&self.primary)?;
        if self.files.is_empty() {
            return Err("source dictionary manifest files is empty".to_owned());
        }
        let mut unique = BTreeSet::new();
        let mut contains_primary = false;
        for path in &self.files {
            let path = normalize_relative_path(path)?;
            if !unique.insert(path.clone()) {
                return Err(format!(
                    "source dictionary manifest duplicate file '{path}'"
                ));
            }
            contains_primary |= path == primary;
        }
        if !contains_primary {
            return Err(format!(
                "source dictionary manifest primary '{primary}' is not declared in files"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SourceDictionarySnapshotEntryV1 {
    pub path: String,
    pub offset: u64,
    pub len: u64,
    pub blake3: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceDictionarySnapshotMetadataV1 {
    pub schema: String,
    pub logical_path: String,
    pub source_root: String,
    pub entries: Vec<SourceDictionarySnapshotEntryV1>,
}

impl Default for SourceDictionarySnapshotMetadataV1 {
    fn default() -> Self {
        Self {
            schema: SOURCE_DICTIONARY_SNAPSHOT_SCHEMA_V1.to_owned(),
            logical_path: String::new(),
            source_root: String::new(),
            entries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceDictionarySnapshotV1 {
    pub metadata: SourceDictionarySnapshotMetadataV1,
    payload: Vec<u8>,
    index: BTreeMap<String, usize>,
}

impl SourceDictionarySnapshotV1 {
    #[inline]
    pub fn entry(&self, path: &str) -> Option<&[u8]> {
        let path = normalize_relative_path(path).ok()?;
        let entry = self.metadata.entries.get(*self.index.get(&path)?)?;
        let start = usize::try_from(entry.offset).ok()?;
        let len = usize::try_from(entry.len).ok()?;
        self.payload.get(start..start.checked_add(len)?)
    }
}

pub fn is_source_dictionary_snapshot(bytes: &[u8]) -> bool {
    bytes.len() >= SOURCE_DICTIONARY_HEADER_LEN
        && bytes.get(0..4) == Some(&SOURCE_DICTIONARY_MAGIC)
        && read_u16(bytes, 4) == Some(SOURCE_DICTIONARY_VERSION)
        && read_u16(bytes, 6) == Some(SOURCE_DICTIONARY_HEADER_LEN as u16)
}

pub fn encode_source_dictionary_snapshot(
    logical_path: &str,
    source_root: &str,
    files: Vec<(String, Vec<u8>)>,
) -> Result<Vec<u8>, String> {
    let logical_path = normalize_logical_path(logical_path)?;
    if files.is_empty() {
        return Err("source dictionary snapshot requires at least one file".to_owned());
    }
    if files.len() > MAX_SOURCE_DICTIONARY_ENTRIES {
        return Err(format!(
            "source dictionary snapshot has {} entries; limit={MAX_SOURCE_DICTIONARY_ENTRIES}",
            files.len()
        ));
    }

    let mut normalized = Vec::with_capacity(files.len());
    let mut seen = BTreeSet::new();
    let mut total_payload = 0usize;
    for (path, bytes) in files {
        let path = normalize_relative_path(&path)?;
        if !seen.insert(path.clone()) {
            return Err(format!(
                "source dictionary snapshot duplicate entry '{path}'"
            ));
        }
        total_payload = total_payload
            .checked_add(bytes.len())
            .ok_or_else(|| "source dictionary snapshot payload size overflow".to_owned())?;
        if total_payload > MAX_SOURCE_DICTIONARY_PAYLOAD_BYTES {
            return Err(format!(
                "source dictionary snapshot payload exceeds {} MiB limit",
                MAX_SOURCE_DICTIONARY_PAYLOAD_BYTES / (1024 * 1024)
            ));
        }
        normalized.push((path, bytes));
    }
    normalized.sort_by(|left, right| left.0.cmp(&right.0));

    let mut payload = Vec::with_capacity(total_payload);
    let mut entries = Vec::with_capacity(normalized.len());
    for (path, bytes) in normalized {
        let offset = payload.len() as u64;
        let len = bytes.len() as u64;
        let blake3 = *blake3::hash(&bytes).as_bytes();
        payload.extend_from_slice(&bytes);
        entries.push(SourceDictionarySnapshotEntryV1 {
            path,
            offset,
            len,
            blake3,
        });
    }
    let metadata = SourceDictionarySnapshotMetadataV1 {
        schema: SOURCE_DICTIONARY_SNAPSHOT_SCHEMA_V1.to_owned(),
        logical_path,
        source_root: source_root.trim().replace('\\', "/"),
        entries,
    };
    let metadata_bytes = serde_json::to_vec(&metadata)
        .map_err(|error| format!("source dictionary metadata encode failed: {error}"))?;
    if metadata_bytes.len() > MAX_SOURCE_DICTIONARY_METADATA_BYTES {
        return Err("source dictionary snapshot metadata exceeds limit".to_owned());
    }

    let total = SOURCE_DICTIONARY_HEADER_LEN
        .checked_add(metadata_bytes.len())
        .and_then(|value| value.checked_add(payload.len()))
        .ok_or_else(|| "source dictionary snapshot total size overflow".to_owned())?;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&SOURCE_DICTIONARY_MAGIC);
    out.extend_from_slice(&SOURCE_DICTIONARY_VERSION.to_le_bytes());
    out.extend_from_slice(&(SOURCE_DICTIONARY_HEADER_LEN as u16).to_le_bytes());
    out.extend_from_slice(&(metadata_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&(metadata.entries.len() as u32).to_le_bytes());
    out.extend_from_slice(&metadata_bytes);
    out.extend_from_slice(&payload);
    Ok(out)
}

pub fn decode_source_dictionary_snapshot(
    bytes: &[u8],
) -> Result<SourceDictionarySnapshotV1, String> {
    if !is_source_dictionary_snapshot(bytes) {
        return Err("source dictionary snapshot magic/version/header mismatch".to_owned());
    }
    let metadata_len = read_u32(bytes, 8)
        .ok_or_else(|| "source dictionary snapshot missing metadata length".to_owned())?
        as usize;
    let payload_len = usize::try_from(
        read_u64(bytes, 12)
            .ok_or_else(|| "source dictionary snapshot missing payload length".to_owned())?,
    )
    .map_err(|_| "source dictionary snapshot payload length exceeds usize".to_owned())?;
    let declared_entries = read_u32(bytes, 20)
        .ok_or_else(|| "source dictionary snapshot missing entry count".to_owned())?
        as usize;
    if metadata_len > MAX_SOURCE_DICTIONARY_METADATA_BYTES
        || payload_len > MAX_SOURCE_DICTIONARY_PAYLOAD_BYTES
        || declared_entries > MAX_SOURCE_DICTIONARY_ENTRIES
    {
        return Err("source dictionary snapshot exceeds bounded decode limits".to_owned());
    }
    let metadata_start = SOURCE_DICTIONARY_HEADER_LEN;
    let metadata_end = metadata_start
        .checked_add(metadata_len)
        .ok_or_else(|| "source dictionary metadata range overflow".to_owned())?;
    let payload_end = metadata_end
        .checked_add(payload_len)
        .ok_or_else(|| "source dictionary payload range overflow".to_owned())?;
    if payload_end != bytes.len() {
        return Err(format!(
            "source dictionary snapshot length mismatch declared={payload_end} actual={}",
            bytes.len()
        ));
    }
    let metadata: SourceDictionarySnapshotMetadataV1 = serde_json::from_slice(
        bytes
            .get(metadata_start..metadata_end)
            .ok_or_else(|| "source dictionary metadata is truncated".to_owned())?,
    )
    .map_err(|error| format!("source dictionary metadata decode failed: {error}"))?;
    if metadata.schema != SOURCE_DICTIONARY_SNAPSHOT_SCHEMA_V1 {
        return Err(format!(
            "source dictionary snapshot schema '{}' is unsupported",
            metadata.schema
        ));
    }
    if metadata.entries.len() != declared_entries {
        return Err(format!(
            "source dictionary entry count mismatch header={declared_entries} metadata={}",
            metadata.entries.len()
        ));
    }
    normalize_logical_path(&metadata.logical_path)?;
    let payload = bytes[metadata_end..payload_end].to_vec();
    let mut index = BTreeMap::new();
    let mut prior_end = 0usize;
    for (ordinal, entry) in metadata.entries.iter().enumerate() {
        let path = normalize_relative_path(&entry.path)?;
        if path != entry.path {
            return Err(format!(
                "source dictionary entry path is not canonical '{}'; expected '{path}'",
                entry.path
            ));
        }
        if index.insert(path.clone(), ordinal).is_some() {
            return Err(format!("source dictionary duplicate entry '{path}'"));
        }
        let start = usize::try_from(entry.offset)
            .map_err(|_| format!("source dictionary entry '{path}' offset exceeds usize"))?;
        let len = usize::try_from(entry.len)
            .map_err(|_| format!("source dictionary entry '{path}' len exceeds usize"))?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| format!("source dictionary entry '{path}' range overflow"))?;
        if start != prior_end || end > payload.len() {
            return Err(format!(
                "source dictionary entry '{path}' has invalid/non-contiguous range {start}..{end} payload={}",
                payload.len()
            ));
        }
        if *blake3::hash(&payload[start..end]).as_bytes() != entry.blake3 {
            return Err(format!(
                "source dictionary entry '{path}' payload hash mismatch"
            ));
        }
        prior_end = end;
    }
    if prior_end != payload.len() {
        return Err("source dictionary payload contains unindexed trailing bytes".to_owned());
    }
    Ok(SourceDictionarySnapshotV1 {
        metadata,
        payload,
        index,
    })
}

fn normalize_logical_path(value: &str) -> Result<String, String> {
    let normalized = value.trim().replace('\\', "/");
    let normalized = normalized.trim_start_matches("./").trim_start_matches('/');
    if normalized.is_empty() || normalized.contains(':') {
        return Err(format!("invalid source dictionary logical path '{value}'"));
    }
    if normalized
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("unsafe source dictionary logical path '{value}'"));
    }
    Ok(normalized.to_owned())
}

fn normalize_relative_path(value: &str) -> Result<String, String> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with('/') || normalized.contains(':') {
        return Err(format!("invalid source dictionary relative path '{value}'"));
    }
    if normalized
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("unsafe source dictionary relative path '{value}'"));
    }
    Ok(normalized)
}

#[inline]
fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

#[inline]
fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[inline]
fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trip_preserves_binary_entries_deterministically() {
        let manifest = br#"{"schema":"newengine.assets.source_dictionary.v1"}"#.to_vec();
        let first = encode_source_dictionary_snapshot(
            "models/hero.ydd",
            "Source/models/hero",
            vec![
                ("hero.bin".to_owned(), vec![0, 1, 2, 255]),
                (SOURCE_DICTIONARY_MANIFEST_FILE.to_owned(), manifest.clone()),
            ],
        )
        .unwrap();
        let second = encode_source_dictionary_snapshot(
            "models/hero.ydd",
            "Source/models/hero",
            vec![
                (SOURCE_DICTIONARY_MANIFEST_FILE.to_owned(), manifest),
                ("hero.bin".to_owned(), vec![0, 1, 2, 255]),
            ],
        )
        .unwrap();
        assert_eq!(first, second);
        assert!(is_source_dictionary_snapshot(&first));
        assert!(!is_source_dictionary_snapshot(b"NEF8"));
        let decoded = decode_source_dictionary_snapshot(&first).unwrap();
        assert_eq!(decoded.entry("hero.bin"), Some([0, 1, 2, 255].as_slice()));
        assert!(decoded.entry("missing.bin").is_none());
    }

    #[test]
    fn snapshot_rejects_payload_corruption() {
        let mut bytes = encode_source_dictionary_snapshot(
            "models/hero.ydd",
            "Source/models/hero",
            vec![("hero.bin".to_owned(), vec![1, 2, 3])],
        )
        .unwrap();
        *bytes.last_mut().unwrap() ^= 0xff;
        assert!(decode_source_dictionary_snapshot(&bytes)
            .unwrap_err()
            .contains("hash mismatch"));
    }

    #[test]
    fn manifest_validates_primary_and_runtime_identity() {
        let manifest = SourceDictionaryManifestV1 {
            logical_path: "models/hero.ydd".to_owned(),
            importer: "gltf.ydd.v1".to_owned(),
            primary: "hero.gltf".to_owned(),
            files: vec!["hero.gltf".to_owned(), "hero.bin".to_owned()],
            ..Default::default()
        };
        assert!(manifest
            .validate_for_runtime_path("models/hero.ydd")
            .is_ok());
        assert!(manifest
            .validate_for_runtime_path("models/hero.ydd@hero")
            .is_ok());
        assert!(manifest
            .validate_for_runtime_path("models/other.ydd@hero")
            .is_err());
    }
}
