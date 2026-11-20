use crate::binary_directory;
use crate::error::{Result, TextureContainerError};
use crate::format::{is_rgba8_format, texture_payload_len};
use crate::header::HeaderV2;
use crate::manifest::{TextureDictionaryManifest, TextureEntryMeta};
use crate::storage::{decode_stored_data, FLAG_DATA_RAW};
use crate::{slice_checked, slice_checked_len};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy)]
pub struct TextureEntryView<'a> {
    pub meta: &'a TextureEntryMeta,
    data_region: &'a [u8],
}

impl<'a> TextureEntryView<'a> {
    #[inline]
    pub fn mip_bytes(&self, level: u32) -> Option<&'a [u8]> {
        let mip = self.meta.mips.iter().find(|m| m.level == level)?;
        slice_checked(self.data_region, mip.byte_offset, mip.byte_len).ok()
    }

    #[inline]
    pub fn base_mip_bytes(&self) -> Option<&'a [u8]> {
        self.mip_bytes(0)
    }

    #[inline]
    pub fn base_mip_rgba8(&self) -> Option<&'a [u8]> {
        is_rgba8_format(&self.meta.format)
            .then(|| self.mip_bytes(0))
            .flatten()
    }
}

#[derive(Debug, Clone)]
pub struct TextureDictionary<'a> {
    header: HeaderV2,
    manifest: TextureDictionaryManifest,
    data_region: TextureDataRegion<'a>,
}

#[derive(Debug, Clone)]
enum TextureDataRegion<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

impl<'a> TextureDataRegion<'a> {
    #[inline]
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Borrowed(v) => v,
            Self::Owned(v) => v.as_slice(),
        }
    }
}

impl<'a> TextureDictionary<'a> {
    #[inline]
    pub fn header(&self) -> HeaderV2 {
        self.header
    }
    #[inline]
    pub fn manifest(&self) -> &TextureDictionaryManifest {
        &self.manifest
    }
    #[inline]
    pub fn entries(&self) -> &[TextureEntryMeta] {
        &self.manifest.entries
    }
    #[inline]
    pub fn first_entry(&self) -> Result<TextureEntryView<'_>> {
        let meta = self
            .manifest
            .entries
            .first()
            .ok_or(TextureContainerError::EmptyDictionary)?;
        Ok(TextureEntryView {
            meta,
            data_region: self.data_region.as_slice(),
        })
    }

    pub fn entry(&self, name: &str) -> Result<TextureEntryView<'_>> {
        let meta = self
            .manifest
            .entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| TextureContainerError::MissingEntry(name.to_owned()))?;
        Ok(TextureEntryView {
            meta,
            data_region: self.data_region.as_slice(),
        })
    }

    pub fn entry_by_hash(&self, hash: u64) -> Result<TextureEntryView<'_>> {
        let meta = self
            .manifest
            .entries
            .iter()
            .find(|e| e.name_hash == hash)
            .ok_or_else(|| TextureContainerError::MissingEntry(format!("hash:{hash}")))?;
        Ok(TextureEntryView {
            meta,
            data_region: self.data_region.as_slice(),
        })
    }
}

/// Parse only the texture dictionary header + directory manifest.
///
/// This is the fast metadata path used by asset catalog UI projections, material validation and
/// launch-gate dependency checks. It deliberately does not inflate/decode the
/// texture data region. Runtime texture packet selection still uses `parse()` so
/// payload validation and GPU upload bytes remain strict.
pub fn parse_manifest_only(bytes: &[u8]) -> Result<TextureDictionaryManifest> {
    let header = HeaderV2::parse(bytes)?;
    header.validate_runtime_flags()?;

    let dir =
        slice_checked(bytes, header.directory_offset, header.directory_len).map_err(|_| {
            TextureContainerError::InvalidRange {
                what: "directory",
                offset: header.directory_offset,
                len: header.directory_len,
                total: bytes.len(),
            }
        })?;
    if !binary_directory::sniff(dir) {
        return Err(TextureContainerError::InvalidDirectory("texture dictionary V2 requires binary directory NTDX; JSON directories are not accepted"));
    }
    let manifest = binary_directory::decode(dir)?;
    validate_manifest_directory_only(header, &manifest, bytes.len())?;
    Ok(manifest)
}

fn validate_manifest_directory_only(
    header: HeaderV2,
    manifest: &TextureDictionaryManifest,
    file_total_len: usize,
) -> Result<()> {
    if header.entry_count as usize != manifest.entries.len() {
        return Err(TextureContainerError::EntryCountMismatch {
            header: header.entry_count,
            directory: manifest.entries.len(),
        });
    }

    let data_region_len = if header.flags == FLAG_DATA_RAW {
        header.data_len
    } else {
        header.data_uncompressed_len
    } as usize;

    let mut seen = BTreeSet::new();
    for entry in &manifest.entries {
        if !seen.insert(entry.name.to_ascii_lowercase()) {
            return Err(TextureContainerError::DuplicateEntry(entry.name.clone()));
        }
        if entry.width == 0 || entry.height == 0 {
            return Err(TextureContainerError::InvalidExtent {
                name: entry.name.clone(),
                width: entry.width,
                height: entry.height,
            });
        }
        let _ = crate::format::parse_pixel_format(&entry.format, &entry.name)?;
        if entry.mip_count as usize != entry.mips.len() || entry.mips.is_empty() {
            return Err(TextureContainerError::InvalidMipChain(entry.name.clone()));
        }
        let _ = slice_checked_len(data_region_len, entry.byte_offset, entry.byte_len).map_err(
            |_| TextureContainerError::InvalidRange {
                what: "entry-data",
                offset: entry.byte_offset,
                len: entry.byte_len,
                total: file_total_len,
            },
        )?;
        for mip in &entry.mips {
            let mip_bytes = slice_checked_len(data_region_len, mip.byte_offset, mip.byte_len)
                .map_err(|_| TextureContainerError::InvalidRange {
                    what: "mip-data",
                    offset: mip.byte_offset,
                    len: mip.byte_len,
                    total: file_total_len,
                })?;
            let expected = texture_payload_len(&entry.format, mip.width, mip.height)?;
            if mip_bytes != expected {
                return Err(TextureContainerError::PayloadSizeMismatch {
                    name: entry.name.clone(),
                    mip: mip.level,
                    bytes: mip_bytes,
                    expected,
                });
            }
        }
    }
    Ok(())
}

pub fn parse(bytes: &[u8]) -> Result<TextureDictionary<'_>> {
    let header = HeaderV2::parse(bytes)?;
    header.validate_runtime_flags()?;

    let dir =
        slice_checked(bytes, header.directory_offset, header.directory_len).map_err(|_| {
            TextureContainerError::InvalidRange {
                what: "directory",
                offset: header.directory_offset,
                len: header.directory_len,
                total: bytes.len(),
            }
        })?;
    if !binary_directory::sniff(dir) {
        return Err(TextureContainerError::InvalidDirectory("texture dictionary V2 requires binary directory NTDX; JSON directories are not accepted"));
    }
    let manifest = binary_directory::decode(dir)?;

    let stored_data_region =
        slice_checked(bytes, header.data_offset, header.data_len).map_err(|_| {
            TextureContainerError::InvalidRange {
                what: "data",
                offset: header.data_offset,
                len: header.data_len,
                total: bytes.len(),
            }
        })?;

    let data_region = if header.flags == FLAG_DATA_RAW {
        TextureDataRegion::Borrowed(stored_data_region)
    } else {
        TextureDataRegion::Owned(decode_stored_data(
            header.flags,
            stored_data_region,
            header.data_uncompressed_len,
        )?)
    };

    validate_manifest(header, &manifest, data_region.as_slice().len(), bytes.len())?;
    Ok(TextureDictionary {
        header,
        manifest,
        data_region,
    })
}

fn validate_manifest(
    header: HeaderV2,
    manifest: &TextureDictionaryManifest,
    data_region_len: usize,
    file_total_len: usize,
) -> Result<()> {
    if header.entry_count as usize != manifest.entries.len() {
        return Err(TextureContainerError::EntryCountMismatch {
            header: header.entry_count,
            directory: manifest.entries.len(),
        });
    }
    let expected_data_region_len = if header.flags == FLAG_DATA_RAW {
        header.data_len
    } else {
        header.data_uncompressed_len
    } as usize;
    if expected_data_region_len != data_region_len {
        return Err(TextureContainerError::InvalidRange {
            what: "data-region",
            offset: header.data_offset,
            len: expected_data_region_len as u64,
            total: file_total_len,
        });
    }

    let mut seen = BTreeSet::new();
    for entry in &manifest.entries {
        if !seen.insert(entry.name.to_ascii_lowercase()) {
            return Err(TextureContainerError::DuplicateEntry(entry.name.clone()));
        }
        if entry.width == 0 || entry.height == 0 {
            return Err(TextureContainerError::InvalidExtent {
                name: entry.name.clone(),
                width: entry.width,
                height: entry.height,
            });
        }
        let _ = crate::format::parse_pixel_format(&entry.format, &entry.name)?;
        if entry.mip_count as usize != entry.mips.len() || entry.mips.is_empty() {
            return Err(TextureContainerError::InvalidMipChain(entry.name.clone()));
        }
        let _ = slice_checked_len(data_region_len, entry.byte_offset, entry.byte_len).map_err(
            |_| TextureContainerError::InvalidRange {
                what: "entry-data",
                offset: entry.byte_offset,
                len: entry.byte_len,
                total: file_total_len,
            },
        )?;
        for mip in &entry.mips {
            let mip_bytes = slice_checked_len(data_region_len, mip.byte_offset, mip.byte_len)
                .map_err(|_| TextureContainerError::InvalidRange {
                    what: "mip-data",
                    offset: mip.byte_offset,
                    len: mip.byte_len,
                    total: file_total_len,
                })?;
            let expected = texture_payload_len(&entry.format, mip.width, mip.height)?;
            if mip_bytes != expected {
                return Err(TextureContainerError::PayloadSizeMismatch {
                    name: entry.name.clone(),
                    mip: mip.level,
                    bytes: mip_bytes,
                    expected,
                });
            }
        }
    }
    Ok(())
}
