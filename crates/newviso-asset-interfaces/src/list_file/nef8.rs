#![forbid(unsafe_op_in_unsafe_fn)]

use super::*;
use flate2::read::DeflateDecoder;
use std::{collections::BTreeSet, io::Read};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedListFileEnvelope {
    pub header: ListFileHeader,
    pub metadata: ListFileHeaderMetadata,
    pub body: Vec<u8>,
}

/// Canonical NEF8/ListFile envelope decode owned by `newengine-assets-api`.
///
/// Domain runtimes must consume `body`; they must not reimplement header ranges,
/// DEFLATE handling, metadata defaults or raw-body hash verification.
pub fn decode_list_file_envelope(
    source: &[u8],
    expected_kind: u32,
    logical_path: &str,
) -> Result<DecodedListFileEnvelope, String> {
    let header = parse_list_file_header(source)?;
    if !header.content_kind_matches(expected_kind) {
        return Err(format!(
            "NEF8 content_kind mismatch path='{}' expected={} actual={}",
            logical_path, expected_kind, header.content_kind
        ));
    }

    let metadata = read_header_metadata(source, &header, logical_path)?;
    validate_metadata_entries(&metadata, logical_path)?;
    let body_slice = read_range(
        source,
        header.body_offset,
        header.body_len,
        "body",
        logical_path,
    )?;
    if !header.is_deflate_body() {
        return Err(format!(
            "NEF8 body must be deflate-compressed path='{logical_path}'"
        ));
    }
    let mut decoder = DeflateDecoder::new(body_slice);
    let mut body = Vec::with_capacity(header.body_uncompressed_len.min(usize::MAX as u64) as usize);
    decoder.read_to_end(&mut body).map_err(|error| {
        format!("NEF8 deflate body decode failed path='{logical_path}' err='{error}'")
    })?;
    if header.body_uncompressed_len != 0 && body.len() as u64 != header.body_uncompressed_len {
        return Err(format!(
            "NEF8 body raw_len mismatch path='{}' expected={} actual={}",
            logical_path,
            header.body_uncompressed_len,
            body.len()
        ));
    }
    if header.has_body_raw_hash() {
        let actual = blake3::hash(&body);
        if actual.as_bytes() != &header.body_raw_hash {
            return Err(format!(
                "NEF8 body hash mismatch path='{}' expected={} actual={}",
                logical_path,
                hex_hash32(&header.body_raw_hash),
                actual.to_hex()
            ));
        }
    }

    Ok(DecodedListFileEnvelope {
        header,
        metadata,
        body,
    })
}

fn read_header_metadata(
    source: &[u8],
    header: &ListFileHeader,
    logical_path: &str,
) -> Result<ListFileHeaderMetadata, String> {
    if header.header_metadata_len == 0 {
        return Ok(ListFileHeaderMetadata {
            logical_path: logical_path.to_owned(),
            content_kind: opaque_content_kind_label(header.content_kind),
            ..ListFileHeaderMetadata::default()
        });
    }
    let bytes = read_range(
        source,
        header.header_metadata_offset,
        header.header_metadata_len,
        "header metadata",
        logical_path,
    )?;
    let mut metadata: ListFileHeaderMetadata = serde_json::from_slice(bytes).map_err(|error| {
        format!("NEF8 header metadata JSON parse failed path='{logical_path}' err='{error}'")
    })?;
    if metadata.logical_path.trim().is_empty() {
        metadata.logical_path = logical_path.to_owned();
    }
    if metadata.content_kind.trim().is_empty() {
        metadata.content_kind = opaque_content_kind_label(header.content_kind);
    }
    Ok(metadata)
}

#[inline]
fn opaque_content_kind_label(content_kind: u32) -> String {
    format!("opaque:{content_kind}")
}

fn validate_metadata_entries(
    metadata: &ListFileHeaderMetadata,
    logical_path: &str,
) -> Result<(), String> {
    let mut names = BTreeSet::<String>::new();
    let mut stable_ids = BTreeSet::<String>::new();
    for entry in &metadata.entries {
        let name = entry.name.trim();
        if name.is_empty() {
            return Err(format!(
                "NEF8 metadata entry has empty name path='{logical_path}'"
            ));
        }
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(format!(
                "NEF8 duplicate metadata entry name path='{logical_path}' name='{name}'"
            ));
        }
        let stable_id = entry.stable_id.trim();
        if stable_id.is_empty() {
            return Err(format!(
                "NEF8 metadata entry has empty stable_id path='{logical_path}' name='{name}'"
            ));
        }
        if !stable_ids.insert(stable_id.to_ascii_lowercase()) {
            return Err(format!(
                "NEF8 duplicate metadata entry hash path='{logical_path}' stable_id='{stable_id}'"
            ));
        }
    }
    Ok(())
}

fn read_range<'a>(
    source: &'a [u8],
    offset: u64,
    len: u64,
    label: &str,
    logical_path: &str,
) -> Result<&'a [u8], String> {
    let offset = usize::try_from(offset)
        .map_err(|_| format!("NEF8 {label} offset too large path='{logical_path}'"))?;
    let len = usize::try_from(len)
        .map_err(|_| format!("NEF8 {label} len too large path='{logical_path}'"))?;
    let end = offset
        .checked_add(len)
        .ok_or_else(|| format!("NEF8 {label} range overflow path='{logical_path}'"))?;
    source.get(offset..end).ok_or_else(|| {
        format!(
            "NEF8 {label} truncated path='{logical_path}' need={end} have={}",
            source.len()
        )
    })
}

fn hex_hash32(hash: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in hash {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

const BASE_HEADER_LEN: usize = 16;
const LENGTHS_HEADER_LEN: usize = 32;
const HASH_HEADER_LEN: usize = 64;
const IDENTITY_HEADER_LEN: usize = 128;
const MANAGED_FLAGS: u16 = LIST_FILE_FLAG_BODY_DEFLATE
    | LIST_FILE_FLAG_HEADER_METADATA
    | LIST_FILE_FLAG_BODY_HASH_BLAKE3
    | LIST_FILE_FLAG_STABLE_FILE_ID
    | LIST_FILE_FLAG_IMPORT_SETTINGS_HASH;

/// Inputs for the canonical NEF8 envelope writer.
///
/// Compression remains producer-owned: `body_stored` contains the exact bytes
/// written to the file. The header module owns only envelope layout, validation
/// and size-class selection.
pub struct ListFileEncodeRequest<'a> {
    pub content_kind: u32,
    pub content_schema_version: u16,
    pub entry_count: u32,
    pub additional_flags: u16,
    /// Minimum accepted size class. Valid values are 4..=8.
    pub min_size_class: u8,
    /// Optional metadata bytes stored immediately after the variable header.
    pub header_metadata: &'a [u8],
    /// Stored body bytes. Current first-party writers pass raw DEFLATE bytes.
    pub body_stored: &'a [u8],
    /// Zero means the raw length is intentionally omitted, allowing class 4.
    pub body_uncompressed_len: u64,
    /// Full BLAKE3 is available from class 6 (64 bytes) upward.
    pub body_raw_hash: Option<[u8; 32]>,
    /// Stable identity fields are available from class 7 (128 bytes) upward.
    pub stable_file_id: Option<u64>,
    pub import_settings_hash: Option<u64>,
}

impl<'a> ListFileEncodeRequest<'a> {
    #[inline]
    pub fn compact(content_kind: u32, body_stored: &'a [u8]) -> Self {
        Self {
            content_kind,
            content_schema_version: 1,
            entry_count: 0,
            additional_flags: 0,
            min_size_class: LIST_FILE_HEADER_SIZE_CLASS_MIN,
            header_metadata: &[],
            body_stored,
            body_uncompressed_len: 0,
            body_raw_hash: None,
            stable_file_id: None,
            import_settings_hash: None,
        }
    }
}

/// Encode a self-describing NEF8 envelope.
///
/// Layout by size class:
/// - class 4 / 16 bytes: immutable prologue + type/flags/schema/entry_count;
/// - class 5 / 32 bytes: stored and decompressed body lengths;
/// - class 6 / 64 bytes: full BLAKE3 body hash;
/// - class 7 / 128 bytes: stable file/import identities and reserved extension;
/// - class 8 / 256 bytes: same known prefix, remaining bytes reserved for future use.
pub fn encode_list_file(request: ListFileEncodeRequest<'_>) -> Result<Vec<u8>, String> {
    let content_kind = u16::try_from(request.content_kind).map_err(|_| {
        format!(
            "NEF8 content_kind {} does not fit u16",
            request.content_kind
        )
    })?;
    if content_kind == 0 {
        return Err("NEF8 content_kind 0 is reserved".to_owned());
    }
    validate_size_class(request.min_size_class)?;

    let mut required_class = LIST_FILE_HEADER_SIZE_CLASS_MIN;
    if !request.header_metadata.is_empty() || request.body_uncompressed_len != 0 {
        required_class = required_class.max(5);
    }
    if request.body_raw_hash.is_some() {
        required_class = required_class.max(6);
    }
    if request.stable_file_id.is_some() || request.import_settings_hash.is_some() {
        required_class = required_class.max(7);
    }
    let size_class = required_class.max(request.min_size_class);
    validate_size_class(size_class)?;
    let header_len = header_len_from_size_class(size_class)?;

    if size_class == 4 && !request.header_metadata.is_empty() {
        return Err("NEF8 class 4 cannot contain header metadata".to_owned());
    }

    let mut flags = (request.additional_flags & !MANAGED_FLAGS) | LIST_FILE_FLAG_BODY_DEFLATE;
    if !request.header_metadata.is_empty() {
        flags |= LIST_FILE_FLAG_HEADER_METADATA;
    }
    if request.body_raw_hash.is_some() {
        flags |= LIST_FILE_FLAG_BODY_HASH_BLAKE3;
    }
    if request.stable_file_id.is_some() {
        flags |= LIST_FILE_FLAG_STABLE_FILE_ID;
    }
    if request.import_settings_hash.is_some() {
        flags |= LIST_FILE_FLAG_IMPORT_SETTINGS_HASH;
    }

    let total_len = header_len
        .checked_add(request.header_metadata.len())
        .and_then(|value| value.checked_add(request.body_stored.len()))
        .ok_or_else(|| "NEF8 output size overflow".to_owned())?;
    let mut out = vec![0_u8; header_len];
    out[0..4].copy_from_slice(&LIST_FILE_MAGIC_NEF8);
    out[4] = LIST_FILE_VERSION as u8;
    out[5] = size_class;
    write_u16(&mut out, 6, content_kind)?;
    write_u16(&mut out, 8, flags)?;
    write_u16(&mut out, 10, request.content_schema_version)?;
    write_u32(&mut out, 12, request.entry_count)?;

    if header_len >= LENGTHS_HEADER_LEN {
        write_u64(&mut out, 16, request.body_stored.len() as u64)?;
        write_u64(&mut out, 24, request.body_uncompressed_len)?;
    }
    if header_len >= HASH_HEADER_LEN {
        if let Some(hash) = request.body_raw_hash {
            out[32..64].copy_from_slice(&hash);
        }
    }
    if header_len >= IDENTITY_HEADER_LEN {
        write_u64(&mut out, 64, request.stable_file_id.unwrap_or(0))?;
        write_u64(&mut out, 72, request.import_settings_hash.unwrap_or(0))?;
    }

    out.reserve(total_len.saturating_sub(header_len));
    out.extend_from_slice(request.header_metadata);
    out.extend_from_slice(request.body_stored);
    Ok(out)
}

pub(super) fn parse_list_file_header(bytes: &[u8]) -> Result<ListFileHeader, String> {
    if bytes.len() < 6 {
        return Err(format!(
            "NEF8 prologue too small: bytes={} expected>=6",
            bytes.len()
        ));
    }
    if bytes.get(0..4) != Some(&LIST_FILE_MAGIC_NEF8[..]) {
        return Err("NEF8 magic mismatch".to_owned());
    }
    let version = bytes[4] as u16;
    if version != LIST_FILE_VERSION {
        return Err(format!(
            "unsupported NEF8 wire version {version}; expected {LIST_FILE_VERSION}"
        ));
    }
    parse_header(bytes, bytes[5])
}

fn parse_header(bytes: &[u8], size_class: u8) -> Result<ListFileHeader, String> {
    validate_size_class(size_class)?;
    let header_len = header_len_from_size_class(size_class)?;
    if bytes.len() < header_len {
        return Err(format!(
            "NEF8 header truncated: class={} header_len={} file_len={}",
            size_class,
            header_len,
            bytes.len()
        ));
    }

    let content_kind = read_u16(bytes, 6)? as u32;
    if content_kind == LIST_FILE_CONTENT_KIND_UNKNOWN {
        return Err("NEF8 content_kind unknown/invalid".to_owned());
    }
    let flags = read_u16(bytes, 8)?;
    if (flags & LIST_FILE_FLAG_BODY_DEFLATE) == 0 {
        return Err(format!(
            "NEF8 missing deflate body flag flags=0x{flags:04x}"
        ));
    }
    if (flags & LIST_FILE_FLAG_HEADER_METADATA) != 0 && header_len < LENGTHS_HEADER_LEN {
        return Err("NEF8 metadata flag requires size_class >= 5".to_owned());
    }
    if (flags & LIST_FILE_FLAG_BODY_HASH_BLAKE3) != 0 && header_len < HASH_HEADER_LEN {
        return Err("NEF8 body hash flag requires size_class >= 6".to_owned());
    }
    if (flags & (LIST_FILE_FLAG_STABLE_FILE_ID | LIST_FILE_FLAG_IMPORT_SETTINGS_HASH)) != 0
        && header_len < IDENTITY_HEADER_LEN
    {
        return Err("NEF8 identity flags require size_class >= 7".to_owned());
    }

    let (body_offset, body_len, body_uncompressed_len, metadata_offset, metadata_len) =
        if header_len == BASE_HEADER_LEN {
            if (flags & LIST_FILE_FLAG_HEADER_METADATA) != 0 {
                return Err("NEF8 class 4 cannot contain metadata".to_owned());
            }
            (
                header_len as u64,
                (bytes.len() - header_len) as u64,
                0,
                header_len as u64,
                0,
            )
        } else {
            let body_len = read_u64(bytes, 16)?;
            let body_len_usize = usize::try_from(body_len)
                .map_err(|_| format!("NEF8 body_len too large: {body_len}"))?;
            let body_offset = bytes.len().checked_sub(body_len_usize).ok_or_else(|| {
                format!(
                    "NEF8 body exceeds file: body_len={} file_len={}",
                    body_len,
                    bytes.len()
                )
            })?;
            if body_offset < header_len {
                return Err(format!(
                    "NEF8 body overlaps header: body_offset={} header_len={}",
                    body_offset, header_len
                ));
            }
            let metadata_len = body_offset - header_len;
            if metadata_len > 0 && (flags & LIST_FILE_FLAG_HEADER_METADATA) == 0 {
                return Err(format!(
                    "NEF8 unclaimed bytes between header and body: metadata_len={metadata_len}"
                ));
            }
            if metadata_len == 0 && (flags & LIST_FILE_FLAG_HEADER_METADATA) != 0 {
                return Err("NEF8 metadata flag set but metadata region is empty".to_owned());
            }
            (
                body_offset as u64,
                body_len,
                read_u64(bytes, 24)?,
                header_len as u64,
                metadata_len as u64,
            )
        };

    let body_raw_hash = if (flags & LIST_FILE_FLAG_BODY_HASH_BLAKE3) != 0 {
        read_hash32(bytes, 32)?
    } else {
        [0; 32]
    };
    let stable_file_id = if (flags & LIST_FILE_FLAG_STABLE_FILE_ID) != 0 {
        read_u64(bytes, 64)?
    } else {
        0
    };
    let import_settings_hash = if (flags & LIST_FILE_FLAG_IMPORT_SETTINGS_HASH) != 0 {
        read_u64(bytes, 72)?
    } else {
        0
    };

    Ok(ListFileHeader {
        version: LIST_FILE_VERSION,
        size_class,
        header_len: header_len as u16,
        content_kind,
        content_schema_version: read_u16(bytes, 10)?,
        flags,
        compression: LIST_FILE_COMPRESSION_DEFLATE,
        entry_count: read_u32(bytes, 12)?,
        header_metadata_offset: metadata_offset,
        header_metadata_len: metadata_len,
        body_offset,
        body_len,
        body_uncompressed_len,
        body_raw_hash,
        import_settings_hash,
        stable_file_id,
    })
}

fn validate_size_class(size_class: u8) -> Result<(), String> {
    if !(LIST_FILE_HEADER_SIZE_CLASS_MIN..=LIST_FILE_HEADER_SIZE_CLASS_MAX).contains(&size_class) {
        return Err(format!(
            "invalid NEF8 size_class={size_class}; supported={}..={}",
            LIST_FILE_HEADER_SIZE_CLASS_MIN, LIST_FILE_HEADER_SIZE_CLASS_MAX
        ));
    }
    Ok(())
}

fn header_len_from_size_class(size_class: u8) -> Result<usize, String> {
    1_usize
        .checked_shl(size_class as u32)
        .ok_or_else(|| format!("NEF8 size_class shift overflow: {size_class}"))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| format!("NEF8 header truncated at u16 offset {offset}"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("NEF8 header truncated at u32 offset {offset}"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| format!("NEF8 header truncated at u64 offset {offset}"))?;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

fn read_hash32(bytes: &[u8], offset: usize) -> Result<[u8; 32], String> {
    let slice = bytes
        .get(offset..offset + 32)
        .ok_or_else(|| format!("NEF8 header truncated at hash32 offset {offset}"))?;
    let mut out = [0; 32];
    out.copy_from_slice(slice);
    Ok(out)
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<(), String> {
    let target = bytes
        .get_mut(offset..offset + 2)
        .ok_or_else(|| format!("NEF8 output truncated at u16 offset {offset}"))?;
    target.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), String> {
    let target = bytes
        .get_mut(offset..offset + 4)
        .ok_or_else(|| format!("NEF8 output truncated at u32 offset {offset}"))?;
    target.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), String> {
    let target = bytes
        .get_mut(offset..offset + 8)
        .ok_or_else(|| format!("NEF8 output truncated at u64 offset {offset}"))?;
    target.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    include!("nef8/tests.rs");
}
