use crate::error::{Result, TextureContainerError};
use crate::manifest::{TextureDictionaryManifest, TextureEntryMeta, TextureMipMeta};
use crate::{PIXEL_FORMAT_RGBA8_UNORM, VERSION_V2};

use super::{
    format_ids::{color_space_from_id, format_from_id},
    io::{checked_range, read_header, read_u16, read_u32, read_u64},
    ENTRY_RECORD_LEN, MIP_RECORD_LEN,
};

pub fn decode(bytes: &[u8]) -> Result<TextureDictionaryManifest> {
    let header = read_header(bytes)?;
    let entries_start = header.entries_offset as usize;
    let mips_start = header.mips_offset as usize;
    let names_start = header.names_offset as usize;
    let names_len = header.names_len as usize;

    checked_range(
        bytes.len(),
        entries_start,
        header.entry_count as usize * ENTRY_RECORD_LEN,
        "directory.entries",
    )?;
    checked_range(
        bytes.len(),
        mips_start,
        header.mip_count as usize * MIP_RECORD_LEN,
        "directory.mips",
    )?;
    checked_range(bytes.len(), names_start, names_len, "directory.names")?;

    let names = &bytes[names_start..names_start + names_len];
    let mut entries = Vec::with_capacity(header.entry_count as usize);

    for entry_index in 0..header.entry_count as usize {
        let offset = entries_start + entry_index * ENTRY_RECORD_LEN;
        let record = &bytes[offset..offset + ENTRY_RECORD_LEN];
        let name_hash = read_u64(record, 0);
        let byte_offset = read_u64(record, 8);
        let byte_len = read_u64(record, 16);
        let name_offset = read_u32(record, 24) as usize;
        let name_len = read_u16(record, 28) as usize;
        let format_id = read_u16(record, 30);
        let width = read_u32(record, 32);
        let height = read_u32(record, 36);
        let first_mip = read_u32(record, 40) as usize;
        let mip_count = read_u32(record, 44) as usize;
        let color_space_id = read_u16(record, 48);

        checked_range(names.len(), name_offset, name_len, "entry.name")?;
        checked_range(
            header.mip_count as usize,
            first_mip,
            mip_count,
            "entry.mips",
        )?;
        let name = std::str::from_utf8(&names[name_offset..name_offset + name_len])
            .map_err(|_| TextureContainerError::InvalidDirectory("entry name is not utf-8"))?
            .to_owned();
        let format = format_from_id(format_id, &name)?.to_owned();
        let color_space = color_space_from_id(color_space_id, &name)?.to_owned();

        let mut mips = Vec::with_capacity(mip_count);
        for mip_index in first_mip..first_mip + mip_count {
            let mip_offset = mips_start + mip_index * MIP_RECORD_LEN;
            let mip_record = &bytes[mip_offset..mip_offset + MIP_RECORD_LEN];
            mips.push(TextureMipMeta {
                byte_offset: read_u64(mip_record, 0),
                byte_len: read_u64(mip_record, 8),
                width: read_u32(mip_record, 16),
                height: read_u32(mip_record, 20),
                level: u32::from(read_u16(mip_record, 24)),
            });
        }

        entries.push(TextureEntryMeta {
            name,
            name_hash,
            width,
            height,
            format,
            color_space,
            byte_offset,
            byte_len,
            mip_count: mip_count as u32,
            mips,
        });
    }

    Ok(TextureDictionaryManifest {
        version: VERSION_V2,
        default_format: PIXEL_FORMAT_RGBA8_UNORM.to_owned(),
        entries,
    })
}
