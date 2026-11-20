mod reader;
mod reader_utils;
mod source_layout;
mod writer;

use crate::mips::TextureEncodedMipData;

pub use reader::read_dds_runtime_texture;
pub use writer::{write_dds_rgba8, write_dds_rgba8_mip_chain, write_dds_runtime_mip_chain};

pub(super) const DDPF_FOURCC: u32 = 0x0000_0004;
pub(super) const DDPF_RGB: u32 = 0x0000_0040;
pub(super) const DDPF_LUMINANCE: u32 = 0x0002_0000;
pub(super) const DDSD_PITCH: u32 = 0x0000_0008;

#[derive(Debug, Clone)]
pub struct DdsRuntimeTexture {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub color_space: String,
    pub mips: Vec<TextureEncodedMipData>,
}

#[derive(Debug, thiserror::Error)]
pub enum DdsImportError {
    #[error("dds: invalid header: {0}")]
    InvalidHeader(String),
    #[error("dds: invalid extent {width}x{height}")]
    InvalidExtent { width: u32, height: u32 },
    #[error("dds: unsupported pixel format: {0}")]
    UnsupportedFormat(String),
    #[error("dds: payload layout does not match header: {0}")]
    InvalidPayload(String),
    #[error(
        "dds: mip payload truncated level={level} offset={offset} need={needed} available={available}"
    )]
    TruncatedMip {
        level: u32,
        offset: usize,
        needed: usize,
        available: usize,
    },
    #[error("dds: arithmetic overflow while computing {0}")]
    Overflow(&'static str),
}

#[derive(Debug, thiserror::Error)]
pub enum DdsExportError {
    #[error("dds: invalid extent {width}x{height}")]
    InvalidExtent { width: u32, height: u32 },
    #[error("dds: invalid payload bytes={bytes} expected={expected} extent={width}x{height}")]
    InvalidPayload {
        bytes: usize,
        expected: usize,
        width: u32,
        height: u32,
    },
    #[error("dds: mip generation failed: {0}")]
    MipGeneration(String),
    #[error("dds: unsupported pixel format '{0}'")]
    UnsupportedFormat(String),
}

#[cfg(test)]
mod import_tests;
