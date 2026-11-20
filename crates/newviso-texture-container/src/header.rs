use crate::error::{Result, TextureContainerError};
use crate::storage::validate_header_flags;
use crate::{HEADER_LEN, MAGIC, VERSION_V2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderV2 {
    pub version: u16,
    pub flags: u16,
    pub entry_count: u32,
    pub directory_offset: u64,
    pub directory_len: u64,
    pub data_offset: u64,
    pub data_len: u64,
    /// For compressed payloads, original raw RGBA8 mip-data length. Raw payloads set this to zero.
    pub data_uncompressed_len: u64,
}

impl HeaderV2 {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            version: VERSION_V2,
            flags: 0,
            entry_count: 0,
            directory_offset: HEADER_LEN as u64,
            directory_len: 0,
            data_offset: HEADER_LEN as u64,
            data_len: 0,
            data_uncompressed_len: 0,
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER_LEN {
            return Err(TextureContainerError::ShortHeader(bytes.len()));
        }
        if &bytes[0..4] != MAGIC.as_slice() {
            return Err(TextureContainerError::BadMagic);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != VERSION_V2 {
            return Err(TextureContainerError::UnsupportedVersion(version));
        }
        let flags = u16::from_le_bytes([bytes[6], bytes[7]]);
        let entry_count = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        let directory_offset = read_u64(bytes, 16);
        let directory_len = read_u64(bytes, 24);
        let data_offset = read_u64(bytes, 32);
        let data_len = read_u64(bytes, 40);
        let data_uncompressed_len = read_u64(bytes, 48);
        Ok(Self {
            version,
            flags,
            entry_count,
            directory_offset,
            directory_len,
            data_offset,
            data_len,
            data_uncompressed_len,
        })
    }

    pub fn write(self, out: &mut [u8]) {
        out[..HEADER_LEN].fill(0);
        out[0..4].copy_from_slice(&MAGIC);
        out[4..6].copy_from_slice(&self.version.to_le_bytes());
        out[6..8].copy_from_slice(&self.flags.to_le_bytes());
        out[8..12].copy_from_slice(&(HEADER_LEN as u32).to_le_bytes());
        out[12..16].copy_from_slice(&self.entry_count.to_le_bytes());
        out[16..24].copy_from_slice(&self.directory_offset.to_le_bytes());
        out[24..32].copy_from_slice(&self.directory_len.to_le_bytes());
        out[32..40].copy_from_slice(&self.data_offset.to_le_bytes());
        out[40..48].copy_from_slice(&self.data_len.to_le_bytes());
        out[48..56].copy_from_slice(&self.data_uncompressed_len.to_le_bytes());
    }

    #[inline]
    pub fn validate_runtime_flags(self) -> Result<()> {
        validate_header_flags(self.flags)
    }
}

#[inline]
fn read_u64(bytes: &[u8], offset: usize) -> u64 {
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
