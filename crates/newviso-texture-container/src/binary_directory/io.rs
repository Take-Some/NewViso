use crate::error::{Result, TextureContainerError};

use super::{
    DirectoryHeader, DIRECTORY_HEADER_LEN, DIRECTORY_MAGIC, DIRECTORY_VERSION, ENTRY_RECORD_LEN,
    MIP_RECORD_LEN,
};

pub(super) fn read_header(bytes: &[u8]) -> Result<DirectoryHeader> {
    if bytes.len() < DIRECTORY_HEADER_LEN {
        return Err(TextureContainerError::InvalidDirectory(
            "short binary directory header",
        ));
    }
    if &bytes[0..4] != DIRECTORY_MAGIC.as_slice() {
        return Err(TextureContainerError::InvalidDirectory(
            "bad binary directory magic",
        ));
    }
    let version = read_u16(bytes, 4);
    if version != DIRECTORY_VERSION {
        return Err(TextureContainerError::InvalidDirectory(
            "unsupported binary directory version",
        ));
    }
    let entry_record_len = read_u16(bytes, 6);
    let mip_record_len = read_u16(bytes, 8);
    if entry_record_len as usize != ENTRY_RECORD_LEN || mip_record_len as usize != MIP_RECORD_LEN {
        return Err(TextureContainerError::InvalidDirectory(
            "unsupported binary directory record sizes",
        ));
    }
    Ok(DirectoryHeader {
        entry_count: read_u32(bytes, 12),
        mip_count: read_u32(bytes, 16),
        entries_offset: read_u32(bytes, 20),
        mips_offset: read_u32(bytes, 24),
        names_offset: read_u32(bytes, 28),
        names_len: read_u32(bytes, 32),
    })
}

pub(super) struct HeaderFields {
    pub entry_count: u32,
    pub mip_count: u32,
    pub entries_offset: u32,
    pub mips_offset: u32,
    pub names_offset: u32,
    pub names_len: u32,
}

pub(super) fn write_header(out: &mut [u8], fields: HeaderFields) {
    out.fill(0);
    out[0..4].copy_from_slice(&DIRECTORY_MAGIC);
    write_u16(out, 4, DIRECTORY_VERSION);
    write_u16(out, 6, ENTRY_RECORD_LEN as u16);
    write_u16(out, 8, MIP_RECORD_LEN as u16);
    write_u16(out, 10, 0);
    write_u32(out, 12, fields.entry_count);
    write_u32(out, 16, fields.mip_count);
    write_u32(out, 20, fields.entries_offset);
    write_u32(out, 24, fields.mips_offset);
    write_u32(out, 28, fields.names_offset);
    write_u32(out, 32, fields.names_len);
}

pub(super) fn checked_range(
    total: usize,
    offset: usize,
    len: usize,
    what: &'static str,
) -> Result<()> {
    let end = offset
        .checked_add(len)
        .ok_or(TextureContainerError::InvalidRange {
            what,
            offset: offset as u64,
            len: len as u64,
            total,
        })?;
    if offset > total || end > total {
        return Err(TextureContainerError::InvalidRange {
            what,
            offset: offset as u64,
            len: len as u64,
            total,
        });
    }
    Ok(())
}

#[inline]
pub(super) fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

#[inline]
pub(super) fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

#[inline]
pub(super) fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

#[inline]
pub(super) fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

#[inline]
pub(super) fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[inline]
pub(super) fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
