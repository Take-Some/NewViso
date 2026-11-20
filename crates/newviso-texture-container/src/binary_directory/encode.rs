use crate::error::{Result, TextureContainerError};
use crate::manifest::{TextureDictionaryManifest, TextureEntryMeta, TextureMipMeta};

use super::{
    entry_mip_count, entry_name_bytes,
    format_ids::{color_space_to_id, format_to_id},
    io::{write_header, write_u16, write_u32, write_u64, HeaderFields},
    mip_count, mip_level, DIRECTORY_HEADER_LEN, ENTRY_RECORD_LEN, MIP_RECORD_LEN,
};

pub fn encode(manifest: &TextureDictionaryManifest) -> Result<Vec<u8>> {
    let entry_count = u32::try_from(manifest.entries.len())
        .map_err(|_| TextureContainerError::DirectoryTooLarge("entry_count"))?;
    let mip_count = u32::try_from(entry_mip_count(manifest))
        .map_err(|_| TextureContainerError::DirectoryTooLarge("mip_count"))?;

    let entries_offset = DIRECTORY_HEADER_LEN as u32;
    let mips_offset = entries_offset
        .checked_add(
            entry_count
                .checked_mul(ENTRY_RECORD_LEN as u32)
                .ok_or(TextureContainerError::DirectoryTooLarge("entries"))?,
        )
        .ok_or(TextureContainerError::DirectoryTooLarge("mips_offset"))?;
    let names_offset = mips_offset
        .checked_add(
            mip_count
                .checked_mul(MIP_RECORD_LEN as u32)
                .ok_or(TextureContainerError::DirectoryTooLarge("mips"))?,
        )
        .ok_or(TextureContainerError::DirectoryTooLarge("names_offset"))?;

    let estimated_names_len = manifest.entries.iter().map(|entry| entry.name.len()).sum();
    let mut names = Vec::with_capacity(estimated_names_len);
    let mut out = vec![0u8; names_offset as usize];
    let mut mip_cursor = 0u32;

    write_header(
        &mut out[..DIRECTORY_HEADER_LEN],
        HeaderFields {
            entry_count,
            mip_count,
            entries_offset,
            mips_offset,
            names_offset,
            names_len: 0,
        },
    );

    for (entry_index, entry) in manifest.entries.iter().enumerate() {
        let name_offset = u32::try_from(names.len())
            .map_err(|_| TextureContainerError::DirectoryTooLarge("name_offset"))?;
        let name_bytes = entry_name_bytes(entry);
        let name_len = u16::try_from(name_bytes.len())
            .map_err(|_| TextureContainerError::NameTooLong(entry.name.clone()))?;
        names.extend_from_slice(name_bytes);

        let entry_record_offset = entries_offset as usize + entry_index * ENTRY_RECORD_LEN;
        write_entry_record(
            &mut out[entry_record_offset..entry_record_offset + ENTRY_RECORD_LEN],
            entry,
            name_offset,
            name_len,
            mip_cursor,
        )?;

        for mip in &entry.mips {
            let mip_record_offset = mips_offset as usize + mip_cursor as usize * MIP_RECORD_LEN;
            write_mip_record(
                &mut out[mip_record_offset..mip_record_offset + MIP_RECORD_LEN],
                mip,
            );
            mip_cursor = mip_cursor
                .checked_add(1)
                .ok_or(TextureContainerError::DirectoryTooLarge("mip_cursor"))?;
        }
    }

    let names_len = u32::try_from(names.len())
        .map_err(|_| TextureContainerError::DirectoryTooLarge("names_len"))?;
    write_u32(&mut out, 32, names_len);
    out.extend_from_slice(&names);
    Ok(out)
}

fn write_entry_record(
    out: &mut [u8],
    entry: &TextureEntryMeta,
    name_offset: u32,
    name_len: u16,
    first_mip: u32,
) -> Result<()> {
    out.fill(0);
    write_u64(out, 0, entry.name_hash);
    write_u64(out, 8, entry.byte_offset);
    write_u64(out, 16, entry.byte_len);
    write_u32(out, 24, name_offset);
    write_u16(out, 28, name_len);
    write_u16(out, 30, format_to_id(&entry.format, &entry.name)?);
    write_u32(out, 32, entry.width);
    write_u32(out, 36, entry.height);
    write_u32(out, 40, first_mip);
    write_u32(out, 44, mip_count(entry) as u32);
    write_u16(out, 48, color_space_to_id(&entry.color_space, &entry.name)?);
    Ok(())
}

fn write_mip_record(out: &mut [u8], mip: &TextureMipMeta) {
    out.fill(0);
    write_u64(out, 0, mip.byte_offset);
    write_u64(out, 8, mip.byte_len);
    write_u32(out, 16, mip.width);
    write_u32(out, 20, mip.height);
    write_u16(out, 24, mip_level(mip));
}
