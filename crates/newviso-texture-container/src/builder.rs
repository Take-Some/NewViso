use crate::binary_directory;
use crate::error::{Result, TextureContainerError};
use crate::format::{is_rgba8_format, parse_pixel_format, texture_payload_len};
use crate::header::HeaderV2;
use crate::manifest::{TextureDictionaryManifest, TextureEntryMeta, TextureMipMeta};
use crate::mips::{rgba8_len, TextureEncodedMipData, TextureMipData};
use crate::names::{normalize_color_space, normalize_texture_name, stable_name_hash64};
use crate::storage::{store_data, TextureBuildOptions};
use crate::{
    align_u64, align_vec, COLOR_SPACE_SRGB, HEADER_LEN, PIXEL_FORMAT_RGBA8_SRGB,
    PIXEL_FORMAT_RGBA8_UNORM, VERSION_V2,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct TextureBuildEntry {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub color_space: String,
    pub mips: Vec<TextureMipData>,
}

#[derive(Debug, Clone)]
pub struct TextureEncodedBuildEntry {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub color_space: String,
    pub mips: Vec<TextureEncodedMipData>,
}

pub fn pack(entries: Vec<TextureBuildEntry>) -> Result<Vec<u8>> {
    pack_with_options(entries, TextureBuildOptions::default())
}

pub fn pack_with_options(
    entries: Vec<TextureBuildEntry>,
    options: TextureBuildOptions,
) -> Result<Vec<u8>> {
    let encoded = entries
        .into_iter()
        .map(|entry| {
            let color_space = normalize_color_space(&entry.color_space);
            let format = if color_space == COLOR_SPACE_SRGB {
                PIXEL_FORMAT_RGBA8_SRGB
            } else {
                PIXEL_FORMAT_RGBA8_UNORM
            };
            let mips = entry
                .mips
                .into_iter()
                .map(|mip| TextureEncodedMipData {
                    level: mip.level,
                    width: mip.width,
                    height: mip.height,
                    bytes: mip.rgba,
                })
                .collect();
            TextureEncodedBuildEntry {
                name: entry.name,
                width: entry.width,
                height: entry.height,
                format: format.to_owned(),
                color_space,
                mips,
            }
        })
        .collect();
    pack_encoded_with_options(encoded, options)
}

pub fn pack_encoded(entries: Vec<TextureEncodedBuildEntry>) -> Result<Vec<u8>> {
    pack_encoded_with_options(entries, TextureBuildOptions::raw_runtime())
}

pub fn pack_encoded_with_options(
    entries: Vec<TextureEncodedBuildEntry>,
    options: TextureBuildOptions,
) -> Result<Vec<u8>> {
    if entries.is_empty() {
        return Err(TextureContainerError::EmptyDictionary);
    }

    let mut seen = BTreeSet::new();
    let mut metas = Vec::with_capacity(entries.len());
    let mut data = Vec::<u8>::new();
    let mut dedupe = BTreeMap::<Vec<u8>, (u64, u64)>::new();

    for entry in entries {
        let name = normalize_texture_name(&entry.name);
        if !seen.insert(name.clone()) {
            return Err(TextureContainerError::DuplicateEntry(name));
        }
        if entry.width == 0 || entry.height == 0 {
            return Err(TextureContainerError::InvalidExtent {
                name,
                width: entry.width,
                height: entry.height,
            });
        }
        let pixel_format = parse_pixel_format(&entry.format, &name)?;
        if entry.mips.is_empty()
            || entry.mips[0].level != 0
            || entry.mips[0].width != entry.width
            || entry.mips[0].height != entry.height
        {
            return Err(TextureContainerError::InvalidMipChain(name));
        }

        let mut entry_min_offset = u64::MAX;
        let mut entry_max_end = 0u64;
        let mut mip_metas = Vec::with_capacity(entry.mips.len());
        let mut expected_w = entry.width;
        let mut expected_h = entry.height;

        for (i, mip) in entry.mips.into_iter().enumerate() {
            if mip.level != i as u32 || mip.width != expected_w || mip.height != expected_h {
                return Err(TextureContainerError::InvalidMipChain(name.clone()));
            }
            let expected = if pixel_format.is_rgba8() {
                rgba8_len(mip.width, mip.height)
            } else {
                texture_payload_len(&entry.format, mip.width, mip.height)?
            };
            if mip.bytes.len() != expected {
                return Err(TextureContainerError::PayloadSizeMismatch {
                    name: name.clone(),
                    mip: mip.level,
                    bytes: mip.bytes.len(),
                    expected,
                });
            }
            let (offset, len) = if let Some(&(offset, len)) = dedupe.get(&mip.bytes) {
                (offset, len)
            } else {
                align_vec(&mut data, 16);
                let offset = data.len() as u64;
                let len = mip.bytes.len() as u64;
                data.extend_from_slice(&mip.bytes);
                dedupe.insert(mip.bytes.clone(), (offset, len));
                (offset, len)
            };
            entry_min_offset = entry_min_offset.min(offset);
            entry_max_end = entry_max_end.max(offset.saturating_add(len));
            mip_metas.push(TextureMipMeta {
                level: mip.level,
                width: mip.width,
                height: mip.height,
                byte_offset: offset,
                byte_len: len,
            });
            expected_w = (expected_w / 2).max(1);
            expected_h = (expected_h / 2).max(1);
        }

        let entry_offset = if entry_min_offset == u64::MAX {
            0
        } else {
            entry_min_offset
        };
        let entry_len = entry_max_end.saturating_sub(entry_offset);
        let color_space = normalize_color_space(&entry.color_space);
        let format = parse_pixel_format(&entry.format, &name)?
            .as_str()
            .to_owned();
        if is_rgba8_format(&format)
            && color_space == COLOR_SPACE_SRGB
            && format != PIXEL_FORMAT_RGBA8_SRGB
        {
            return Err(TextureContainerError::InvalidFormat { name, format });
        }
        metas.push(TextureEntryMeta {
            name: name.clone(),
            name_hash: stable_name_hash64(&name),
            width: entry.width,
            height: entry.height,
            format,
            color_space,
            byte_offset: entry_offset,
            byte_len: entry_len,
            mip_count: mip_metas.len() as u32,
            mips: mip_metas,
        });
    }

    let manifest = TextureDictionaryManifest {
        version: VERSION_V2,
        default_format: PIXEL_FORMAT_RGBA8_UNORM.to_owned(),
        entries: metas,
    };
    let dir = binary_directory::encode(&manifest)?;

    let stored_data = store_data(&data, options)?;

    let directory_offset = HEADER_LEN as u64;
    let directory_len = dir.len() as u64;
    let data_offset = align_u64(directory_offset + directory_len, 16);
    let mut out = vec![0u8; data_offset as usize];
    out[HEADER_LEN..HEADER_LEN + dir.len()].copy_from_slice(&dir);
    out.extend_from_slice(&stored_data.bytes);

    let header = HeaderV2 {
        version: VERSION_V2,
        flags: stored_data.flags,
        entry_count: manifest.entries.len() as u32,
        directory_offset,
        directory_len,
        data_offset,
        data_len: stored_data.bytes.len() as u64,
        data_uncompressed_len: stored_data.uncompressed_len,
    };
    header.write(&mut out[..HEADER_LEN]);
    Ok(out)
}
