#![forbid(unsafe_op_in_unsafe_fn)]

//! NewEngine Texture Dictionary container payload for canonical `.ytd` assets.
//!
//! The runtime format is a binary texture dictionary: fixed header, binary
//! directory and runtime-ready mip payloads. Runtime material dictionaries are GPU-native:
//! BC1/BC3/BC5/BC7 entries store complete mip chains directly in the data region so the
//! renderer can upload compressed blocks without CPU-side image decompression. RGBA8 remains
//! supported for UI/editor tooling. `.ytd` never stores raw source-image
//! paths, authoring provenance or JSON directory data.

pub mod bcn;
pub mod binary_directory;
pub mod builder;
pub mod dds;
pub mod dictionary;
pub mod error;
pub mod format;
pub mod header;
pub mod manifest;
pub mod mips;
pub mod names;
pub mod selector;
pub mod storage;

pub use bcn::{decode_bcn_to_rgba8, encode_rgba8_mips_to_bcn, infer_bcn_format, BcnEncodeError};
pub use builder::{
    pack, pack_encoded, pack_encoded_with_options, pack_with_options, TextureBuildEntry,
    TextureEncodedBuildEntry,
};
pub use dds::{
    read_dds_runtime_texture, write_dds_rgba8, write_dds_rgba8_mip_chain,
    write_dds_runtime_mip_chain, DdsExportError, DdsImportError, DdsRuntimeTexture,
};
pub use dictionary::{parse, parse_manifest_only, TextureDictionary, TextureEntryView};
pub use error::{Result, TextureContainerError};
pub use format::{
    is_block_compressed_format, is_rgba8_format, parse_pixel_format, texture_payload_len,
    TexturePixelFormat,
};
pub use header::HeaderV2;
pub use manifest::{TextureDictionaryManifest, TextureEntryMeta, TextureMipMeta};
pub use mips::{generate_rgba8_mips, rgba8_len, TextureEncodedMipData, TextureMipData};
pub use names::{
    infer_color_space_from_name, normalize_color_space, normalize_texture_name, stable_name_hash64,
};
pub use selector::{TextureDictionarySelector, TextureSelectorError};
pub use storage::{TextureBuildOptions, TextureDataCompression, FLAG_DATA_RAW, FLAG_DATA_ZSTD};

/// Inner payload magic for TextureDictionaryPayloadV1 stored inside NEF8 .ytd bodies.
pub const TEXTURE_DICTIONARY_PAYLOAD_MAGIC: [u8; 4] = *b"NETD";
#[doc(hidden)]
pub const MAGIC: [u8; 4] = TEXTURE_DICTIONARY_PAYLOAD_MAGIC;
pub const VERSION_V2: u16 = 2;
pub const HEADER_LEN: usize = 64;
pub const EXTENSION: &str = "ytd";
pub use format::{
    PIXEL_FORMAT_BC1_RGBA_SRGB, PIXEL_FORMAT_BC1_RGBA_UNORM, PIXEL_FORMAT_BC2_RGBA_SRGB,
    PIXEL_FORMAT_BC2_RGBA_UNORM, PIXEL_FORMAT_BC3_RGBA_SRGB, PIXEL_FORMAT_BC3_RGBA_UNORM,
    PIXEL_FORMAT_BC5_RG_UNORM, PIXEL_FORMAT_BC6H_SF16, PIXEL_FORMAT_BC6H_UF16,
    PIXEL_FORMAT_BC7_RGBA_SRGB, PIXEL_FORMAT_BC7_RGBA_UNORM, PIXEL_FORMAT_RGBA8_SRGB,
    PIXEL_FORMAT_RGBA8_UNORM,
};
pub const COLOR_SPACE_LINEAR: &str = "linear";
pub const COLOR_SPACE_SRGB: &str = "srgb";

#[inline]
pub(crate) fn align_u64(v: u64, alignment: u64) -> u64 {
    v.div_ceil(alignment) * alignment
}

#[inline]
pub(crate) fn align_vec(v: &mut Vec<u8>, alignment: usize) {
    let aligned = v.len().div_ceil(alignment) * alignment;
    v.resize(aligned, 0);
}

#[inline]
pub(crate) fn slice_checked(bytes: &[u8], offset: u64, len: u64) -> std::result::Result<&[u8], ()> {
    let start = offset as usize;
    let len_usize = len as usize;
    let end = start.checked_add(len_usize).ok_or(())?;
    if start > bytes.len() || end > bytes.len() {
        return Err(());
    }
    Ok(&bytes[start..end])
}

#[inline]
pub(crate) fn slice_checked_len(
    total_len: usize,
    offset: u64,
    len: u64,
) -> std::result::Result<usize, ()> {
    let start = offset as usize;
    let len_usize = len as usize;
    let end = start.checked_add(len_usize).ok_or(())?;
    if start > total_len || end > total_len {
        return Err(());
    }
    Ok(len_usize)
}
