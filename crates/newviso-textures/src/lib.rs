use newviso_resource_runtime::{AssetDomain, AssetId, AssetResource};
use std::{any::Any, sync::Arc};

pub const TEXTURES_DOMAIN: AssetDomain = AssetDomain::new("engine.textures");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureFormat {
    Rgba8Unorm,
    Rgba8Srgb,
    Bc1RgbaUnorm,
    Bc1RgbaSrgb,
    Bc2RgbaUnorm,
    Bc2RgbaSrgb,
    Bc3RgbaUnorm,
    Bc3RgbaSrgb,
    Bc5RgUnorm,
    Bc6hUf16,
    Bc6hSf16,
    Bc7RgbaUnorm,
    Bc7RgbaSrgb,
}

impl TextureFormat {
    pub const fn is_block_compressed(self) -> bool {
        !matches!(self, Self::Rgba8Unorm | Self::Rgba8Srgb)
    }

    pub const fn is_srgb(self) -> bool {
        matches!(
            self,
            Self::Rgba8Srgb
                | Self::Bc1RgbaSrgb
                | Self::Bc2RgbaSrgb
                | Self::Bc3RgbaSrgb
                | Self::Bc7RgbaSrgb
        )
    }

    pub const fn bytes_per_block(self) -> usize {
        match self {
            Self::Rgba8Unorm | Self::Rgba8Srgb => 4,
            Self::Bc1RgbaUnorm | Self::Bc1RgbaSrgb => 8,
            Self::Bc2RgbaUnorm
            | Self::Bc2RgbaSrgb
            | Self::Bc3RgbaUnorm
            | Self::Bc3RgbaSrgb
            | Self::Bc5RgUnorm
            | Self::Bc6hUf16
            | Self::Bc6hSf16
            | Self::Bc7RgbaUnorm
            | Self::Bc7RgbaSrgb => 16,
        }
    }

    pub const fn block_extent(self) -> usize {
        if self.is_block_compressed() {
            4
        } else {
            1
        }
    }

    pub fn expected_mip_bytes(self, width: u32, height: u32) -> Option<usize> {
        let width = usize::try_from(width).ok()?;
        let height = usize::try_from(height).ok()?;
        if self.is_block_compressed() {
            let block = self.block_extent();
            let blocks_w = width.checked_add(block - 1)?.checked_div(block)?;
            let blocks_h = height.checked_add(block - 1)?.checked_div(block)?;
            blocks_w
                .checked_mul(blocks_h)?
                .checked_mul(self.bytes_per_block())
        } else {
            width
                .checked_mul(height)?
                .checked_mul(self.bytes_per_block())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureColorSpace {
    Linear,
    Srgb,
}

#[derive(Clone, Debug)]
pub struct TextureMip {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    /// Runtime-ready bytes. BC formats remain block-compressed.
    pub data: Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub struct TextureResource {
    pub id: AssetId,
    pub name: String,
    pub name_hash: u64,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub color_space: TextureColorSpace,
    pub mips: Vec<TextureMip>,
}

impl AssetResource for TextureResource {
    fn asset_id(&self) -> AssetId {
        self.id
    }

    fn domain(&self) -> AssetDomain {
        TEXTURES_DOMAIN
    }

    fn into_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}
