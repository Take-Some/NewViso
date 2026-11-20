use crate::error::{Result, TextureContainerError};
use std::io::Cursor;

/// Stored data region is raw runtime mip payload. For BCn material dictionaries this is GPU-native block data.
pub const FLAG_DATA_RAW: u16 = 0;
/// Stored data region is one zstd frame containing runtime mip payload. GPU-native material dictionaries should use raw data for direct upload.
pub const FLAG_DATA_ZSTD: u16 = 0x0001;
const SUPPORTED_FLAGS: u16 = FLAG_DATA_ZSTD;
const DEFAULT_ZSTD_LEVEL: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureDataCompression {
    Raw,
    Zstd { level: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureBuildOptions {
    pub compression: TextureDataCompression,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredTextureData {
    pub flags: u16,
    pub bytes: Vec<u8>,
    pub uncompressed_len: u64,
}

impl Default for TextureBuildOptions {
    #[inline]
    fn default() -> Self {
        Self::raw_runtime()
    }
}

impl TextureBuildOptions {
    #[inline]
    pub const fn uncompressed() -> Self {
        Self {
            compression: TextureDataCompression::Raw,
        }
    }

    #[inline]
    pub const fn raw_runtime() -> Self {
        Self::uncompressed()
    }

    #[inline]
    pub const fn zstd_runtime() -> Self {
        Self {
            compression: TextureDataCompression::Zstd {
                level: DEFAULT_ZSTD_LEVEL,
            },
        }
    }

    #[inline]
    pub const fn zstd_with_level(level: i32) -> Self {
        Self {
            compression: TextureDataCompression::Zstd { level },
        }
    }
}

#[inline]
pub(crate) fn validate_header_flags(flags: u16) -> Result<()> {
    if flags & !SUPPORTED_FLAGS == 0 {
        Ok(())
    } else {
        Err(TextureContainerError::CompressedPayloadUnsupported(flags))
    }
}

pub(crate) fn store_data(data: &[u8], options: TextureBuildOptions) -> Result<StoredTextureData> {
    match options.compression {
        TextureDataCompression::Raw => Ok(StoredTextureData {
            flags: FLAG_DATA_RAW,
            bytes: data.to_vec(),
            uncompressed_len: 0,
        }),
        TextureDataCompression::Zstd { level } => {
            let level = level.clamp(1, 22);
            let encoded = zstd::stream::encode_all(Cursor::new(data), level).map_err(|e| {
                TextureContainerError::CompressionFailed(format!("zstd encode failed: {e}"))
            })?;

            // Deterministic safety valve: tiny/noisy dictionaries can grow after zstd framing.
            // In that case keep the runtime file raw instead of producing a larger payload.
            if encoded.len() >= data.len() {
                Ok(StoredTextureData {
                    flags: FLAG_DATA_RAW,
                    bytes: data.to_vec(),
                    uncompressed_len: 0,
                })
            } else {
                Ok(StoredTextureData {
                    flags: FLAG_DATA_ZSTD,
                    bytes: encoded,
                    uncompressed_len: data.len() as u64,
                })
            }
        }
    }
}

pub(crate) fn decode_stored_data(
    flags: u16,
    stored: &[u8],
    expected_uncompressed_len: u64,
) -> Result<Vec<u8>> {
    validate_header_flags(flags)?;
    match flags {
        FLAG_DATA_RAW => {
            if expected_uncompressed_len != 0 && expected_uncompressed_len != stored.len() as u64 {
                return Err(TextureContainerError::PayloadSizeMismatch {
                    name: "<dictionary-data>".to_owned(),
                    mip: 0,
                    bytes: stored.len(),
                    expected: expected_uncompressed_len as usize,
                });
            }
            Ok(stored.to_vec())
        }
        FLAG_DATA_ZSTD => {
            if expected_uncompressed_len == 0 {
                return Err(TextureContainerError::InvalidDirectory(
                    "zstd data requires data_uncompressed_len",
                ));
            }
            let decoded = zstd::stream::decode_all(Cursor::new(stored)).map_err(|e| {
                TextureContainerError::CompressionFailed(format!("zstd decode failed: {e}"))
            })?;
            if decoded.len() as u64 != expected_uncompressed_len {
                return Err(TextureContainerError::PayloadSizeMismatch {
                    name: "<dictionary-data>".to_owned(),
                    mip: 0,
                    bytes: decoded.len(),
                    expected: expected_uncompressed_len as usize,
                });
            }
            Ok(decoded)
        }
        _ => Err(TextureContainerError::CompressedPayloadUnsupported(flags)),
    }
}
