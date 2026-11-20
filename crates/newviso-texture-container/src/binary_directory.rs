mod decode;
mod encode;
mod format_ids;
mod io;

use crate::manifest::{TextureDictionaryManifest, TextureEntryMeta, TextureMipMeta};

pub const DIRECTORY_MAGIC: [u8; 4] = *b"NTDX";
pub const DIRECTORY_VERSION: u16 = 1;
pub const DIRECTORY_HEADER_LEN: usize = 40;
pub const ENTRY_RECORD_LEN: usize = 64;
pub const MIP_RECORD_LEN: usize = 32;

#[derive(Debug, Clone, Copy)]
pub struct BinaryDirectoryStats {
    pub entry_count: u32,
    pub mip_count: u32,
}

pub use decode::decode;
pub use encode::encode;

#[inline]
pub fn sniff(bytes: &[u8]) -> bool {
    bytes.len() >= DIRECTORY_HEADER_LEN && &bytes[0..4] == DIRECTORY_MAGIC.as_slice()
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DirectoryHeader {
    pub(super) entry_count: u32,
    pub(super) mip_count: u32,
    pub(super) entries_offset: u32,
    pub(super) mips_offset: u32,
    pub(super) names_offset: u32,
    pub(super) names_len: u32,
}

pub(super) fn entry_mip_count(manifest: &TextureDictionaryManifest) -> usize {
    manifest.entries.iter().map(|entry| entry.mips.len()).sum()
}

pub(super) fn entry_name_bytes(entry: &TextureEntryMeta) -> &[u8] {
    entry.name.as_bytes()
}

pub(super) fn mip_count(entry: &TextureEntryMeta) -> usize {
    entry.mips.len()
}

pub(super) fn mip_level(mip: &TextureMipMeta) -> u16 {
    mip.level as u16
}
