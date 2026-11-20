use crate::error::{Result, TextureContainerError};

pub const PIXEL_FORMAT_RGBA8_UNORM: &str = "RGBA8_UNORM";
pub const PIXEL_FORMAT_RGBA8_SRGB: &str = "RGBA8_SRGB";
pub const PIXEL_FORMAT_BC1_RGBA_UNORM: &str = "BC1_RGBA_UNORM";
pub const PIXEL_FORMAT_BC1_RGBA_SRGB: &str = "BC1_RGBA_SRGB";
pub const PIXEL_FORMAT_BC2_RGBA_UNORM: &str = "BC2_RGBA_UNORM";
pub const PIXEL_FORMAT_BC2_RGBA_SRGB: &str = "BC2_RGBA_SRGB";
pub const PIXEL_FORMAT_BC3_RGBA_UNORM: &str = "BC3_RGBA_UNORM";
pub const PIXEL_FORMAT_BC3_RGBA_SRGB: &str = "BC3_RGBA_SRGB";
pub const PIXEL_FORMAT_BC5_RG_UNORM: &str = "BC5_RG_UNORM";
pub const PIXEL_FORMAT_BC6H_UF16: &str = "BC6H_UF16";
pub const PIXEL_FORMAT_BC6H_SF16: &str = "BC6H_SF16";
pub const PIXEL_FORMAT_BC7_RGBA_UNORM: &str = "BC7_RGBA_UNORM";
pub const PIXEL_FORMAT_BC7_RGBA_SRGB: &str = "BC7_RGBA_SRGB";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexturePixelFormat {
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

impl TexturePixelFormat {
    #[inline]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "RGBA8_UNORM" | "RGBA8" => Some(Self::Rgba8Unorm),
            "RGBA8_SRGB" => Some(Self::Rgba8Srgb),
            "BC1_RGBA_UNORM" | "BC1_UNORM" | "BC1" => Some(Self::Bc1RgbaUnorm),
            "BC1_RGBA_SRGB" | "BC1_SRGB" => Some(Self::Bc1RgbaSrgb),
            "BC2_RGBA_UNORM" | "BC2_UNORM" | "BC2" | "DXT3" => Some(Self::Bc2RgbaUnorm),
            "BC2_RGBA_SRGB" | "BC2_SRGB" => Some(Self::Bc2RgbaSrgb),
            "BC3_RGBA_UNORM" | "BC3_UNORM" | "BC3" | "DXT5" => Some(Self::Bc3RgbaUnorm),
            "BC3_RGBA_SRGB" | "BC3_SRGB" => Some(Self::Bc3RgbaSrgb),
            "BC5_RG_UNORM" | "BC5_UNORM" | "BC5" => Some(Self::Bc5RgUnorm),
            "BC6H_UF16" | "BC6H" | "BC6" => Some(Self::Bc6hUf16),
            "BC6H_SF16" => Some(Self::Bc6hSf16),
            "BC7_RGBA_UNORM" | "BC7_UNORM" | "BC7" => Some(Self::Bc7RgbaUnorm),
            "BC7_RGBA_SRGB" | "BC7_SRGB" => Some(Self::Bc7RgbaSrgb),
            _ => None,
        }
    }

    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rgba8Unorm => PIXEL_FORMAT_RGBA8_UNORM,
            Self::Rgba8Srgb => PIXEL_FORMAT_RGBA8_SRGB,
            Self::Bc1RgbaUnorm => PIXEL_FORMAT_BC1_RGBA_UNORM,
            Self::Bc1RgbaSrgb => PIXEL_FORMAT_BC1_RGBA_SRGB,
            Self::Bc2RgbaUnorm => PIXEL_FORMAT_BC2_RGBA_UNORM,
            Self::Bc2RgbaSrgb => PIXEL_FORMAT_BC2_RGBA_SRGB,
            Self::Bc3RgbaUnorm => PIXEL_FORMAT_BC3_RGBA_UNORM,
            Self::Bc3RgbaSrgb => PIXEL_FORMAT_BC3_RGBA_SRGB,
            Self::Bc5RgUnorm => PIXEL_FORMAT_BC5_RG_UNORM,
            Self::Bc6hUf16 => PIXEL_FORMAT_BC6H_UF16,
            Self::Bc6hSf16 => PIXEL_FORMAT_BC6H_SF16,
            Self::Bc7RgbaUnorm => PIXEL_FORMAT_BC7_RGBA_UNORM,
            Self::Bc7RgbaSrgb => PIXEL_FORMAT_BC7_RGBA_SRGB,
        }
    }

    #[inline]
    pub const fn is_block_compressed(self) -> bool {
        matches!(
            self,
            Self::Bc1RgbaUnorm
                | Self::Bc1RgbaSrgb
                | Self::Bc2RgbaUnorm
                | Self::Bc2RgbaSrgb
                | Self::Bc3RgbaUnorm
                | Self::Bc3RgbaSrgb
                | Self::Bc5RgUnorm
                | Self::Bc6hUf16
                | Self::Bc6hSf16
                | Self::Bc7RgbaUnorm
                | Self::Bc7RgbaSrgb
        )
    }

    #[inline]
    pub const fn is_rgba8(self) -> bool {
        matches!(self, Self::Rgba8Unorm | Self::Rgba8Srgb)
    }

    #[inline]
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

    #[inline]
    pub const fn block_bytes(self) -> usize {
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

    #[inline]
    pub const fn block_extent(self) -> u32 {
        if self.is_block_compressed() {
            4
        } else {
            1
        }
    }
}

#[inline]
pub fn parse_pixel_format(format: &str, name: &str) -> Result<TexturePixelFormat> {
    TexturePixelFormat::parse(format).ok_or_else(|| TextureContainerError::InvalidFormat {
        name: name.to_owned(),
        format: format.to_owned(),
    })
}

#[inline]
pub fn texture_payload_len(format: &str, width: u32, height: u32) -> Result<usize> {
    let f = parse_pixel_format(format, "<payload>")?;
    Ok(match f {
        TexturePixelFormat::Rgba8Unorm | TexturePixelFormat::Rgba8Srgb => (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4),
        _ => {
            let block = f.block_extent() as usize;
            let bw = (width as usize).saturating_add(block - 1) / block;
            let bh = (height as usize).saturating_add(block - 1) / block;
            bw.saturating_mul(bh).saturating_mul(f.block_bytes())
        }
    })
}

#[inline]
pub fn is_rgba8_format(format: &str) -> bool {
    TexturePixelFormat::parse(format)
        .map(TexturePixelFormat::is_rgba8)
        .unwrap_or(false)
}

#[inline]
pub fn is_block_compressed_format(format: &str) -> bool {
    TexturePixelFormat::parse(format)
        .map(TexturePixelFormat::is_block_compressed)
        .unwrap_or(false)
}
