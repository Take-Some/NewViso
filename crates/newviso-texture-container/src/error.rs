use crate::HEADER_LEN;

#[derive(Debug, thiserror::Error)]
pub enum TextureContainerError {
    #[error("texture_dictionary: input is shorter than header: bytes={0} expected={HEADER_LEN}")]
    ShortHeader(usize),
    #[error("texture_dictionary: bad magic")]
    BadMagic,
    #[error("texture_dictionary: unsupported version {0}")]
    UnsupportedVersion(u16),
    #[error("texture_dictionary: unsupported storage/header flags: flags=0x{0:04x}")]
    CompressedPayloadUnsupported(u16),
    #[error("texture_dictionary: compression error: {0}")]
    CompressionFailed(String),
    #[error("texture_dictionary: invalid range {what}: offset={offset} len={len} total={total}")]
    InvalidRange {
        what: &'static str,
        offset: u64,
        len: u64,
        total: usize,
    },
    #[error("texture_dictionary: invalid binary directory: {0}")]
    InvalidDirectory(&'static str),
    #[error("texture_dictionary: binary directory is too large: {0}")]
    DirectoryTooLarge(&'static str),
    #[error("texture_dictionary: texture name is too long for binary directory: {0}")]
    NameTooLong(String),
    #[error("texture_dictionary: entry_count mismatch header={header} directory={directory}")]
    EntryCountMismatch { header: u32, directory: usize },
    #[error("texture_dictionary: duplicate texture entry '{0}'")]
    DuplicateEntry(String),
    #[error("texture_dictionary: missing texture entry '{0}'")]
    MissingEntry(String),
    #[error("texture_dictionary: empty texture dictionary")]
    EmptyDictionary,
    #[error("texture_dictionary: invalid texture extent for '{name}': {width}x{height}")]
    InvalidExtent {
        name: String,
        width: u32,
        height: u32,
    },
    #[error("texture_dictionary: invalid mip chain for '{0}'")]
    InvalidMipChain(String),
    #[error("texture_dictionary: payload size mismatch for '{name}' mip={mip} bytes={bytes} expected={expected}")]
    PayloadSizeMismatch {
        name: String,
        mip: u32,
        bytes: usize,
        expected: usize,
    },
    #[error("texture_dictionary: invalid pixel format '{format}' for '{name}'")]
    InvalidFormat { name: String, format: String },
    #[error("texture_dictionary: invalid color space '{color_space}' for '{name}'")]
    InvalidColorSpace { name: String, color_space: String },
}

pub type Result<T> = std::result::Result<T, TextureContainerError>;
