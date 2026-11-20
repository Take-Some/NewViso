use crate::error::{Result, TextureContainerError};
use crate::{
    COLOR_SPACE_LINEAR, COLOR_SPACE_SRGB, PIXEL_FORMAT_BC1_RGBA_SRGB, PIXEL_FORMAT_BC1_RGBA_UNORM,
    PIXEL_FORMAT_BC2_RGBA_SRGB, PIXEL_FORMAT_BC2_RGBA_UNORM, PIXEL_FORMAT_BC3_RGBA_SRGB,
    PIXEL_FORMAT_BC3_RGBA_UNORM, PIXEL_FORMAT_BC5_RG_UNORM, PIXEL_FORMAT_BC6H_SF16,
    PIXEL_FORMAT_BC6H_UF16, PIXEL_FORMAT_BC7_RGBA_SRGB, PIXEL_FORMAT_BC7_RGBA_UNORM,
    PIXEL_FORMAT_RGBA8_SRGB, PIXEL_FORMAT_RGBA8_UNORM,
};

const FORMAT_RGBA8_UNORM: u16 = 1;
const FORMAT_RGBA8_SRGB: u16 = 2;
const FORMAT_BC1_RGBA_UNORM: u16 = 101;
const FORMAT_BC1_RGBA_SRGB: u16 = 102;
const FORMAT_BC3_RGBA_UNORM: u16 = 103;
const FORMAT_BC3_RGBA_SRGB: u16 = 104;
const FORMAT_BC5_RG_UNORM: u16 = 105;
const FORMAT_BC7_RGBA_UNORM: u16 = 106;
const FORMAT_BC7_RGBA_SRGB: u16 = 107;
const FORMAT_BC2_RGBA_UNORM: u16 = 108;
const FORMAT_BC2_RGBA_SRGB: u16 = 109;
const FORMAT_BC6H_UF16: u16 = 110;
const FORMAT_BC6H_SF16: u16 = 111;
const COLOR_LINEAR: u16 = 1;
const COLOR_SRGB: u16 = 2;

pub(super) fn format_to_id(format: &str, name: &str) -> Result<u16> {
    match format {
        PIXEL_FORMAT_RGBA8_UNORM => Ok(FORMAT_RGBA8_UNORM),
        PIXEL_FORMAT_RGBA8_SRGB => Ok(FORMAT_RGBA8_SRGB),
        PIXEL_FORMAT_BC1_RGBA_UNORM => Ok(FORMAT_BC1_RGBA_UNORM),
        PIXEL_FORMAT_BC1_RGBA_SRGB => Ok(FORMAT_BC1_RGBA_SRGB),
        PIXEL_FORMAT_BC2_RGBA_UNORM => Ok(FORMAT_BC2_RGBA_UNORM),
        PIXEL_FORMAT_BC2_RGBA_SRGB => Ok(FORMAT_BC2_RGBA_SRGB),
        PIXEL_FORMAT_BC3_RGBA_UNORM => Ok(FORMAT_BC3_RGBA_UNORM),
        PIXEL_FORMAT_BC3_RGBA_SRGB => Ok(FORMAT_BC3_RGBA_SRGB),
        PIXEL_FORMAT_BC5_RG_UNORM => Ok(FORMAT_BC5_RG_UNORM),
        PIXEL_FORMAT_BC6H_UF16 => Ok(FORMAT_BC6H_UF16),
        PIXEL_FORMAT_BC6H_SF16 => Ok(FORMAT_BC6H_SF16),
        PIXEL_FORMAT_BC7_RGBA_UNORM => Ok(FORMAT_BC7_RGBA_UNORM),
        PIXEL_FORMAT_BC7_RGBA_SRGB => Ok(FORMAT_BC7_RGBA_SRGB),
        other => Err(TextureContainerError::InvalidFormat {
            name: name.to_owned(),
            format: other.to_owned(),
        }),
    }
}

pub(super) fn format_from_id(id: u16, name: &str) -> Result<&'static str> {
    match id {
        FORMAT_RGBA8_UNORM => Ok(PIXEL_FORMAT_RGBA8_UNORM),
        FORMAT_RGBA8_SRGB => Ok(PIXEL_FORMAT_RGBA8_SRGB),
        FORMAT_BC1_RGBA_UNORM => Ok(PIXEL_FORMAT_BC1_RGBA_UNORM),
        FORMAT_BC1_RGBA_SRGB => Ok(PIXEL_FORMAT_BC1_RGBA_SRGB),
        FORMAT_BC2_RGBA_UNORM => Ok(PIXEL_FORMAT_BC2_RGBA_UNORM),
        FORMAT_BC2_RGBA_SRGB => Ok(PIXEL_FORMAT_BC2_RGBA_SRGB),
        FORMAT_BC3_RGBA_UNORM => Ok(PIXEL_FORMAT_BC3_RGBA_UNORM),
        FORMAT_BC3_RGBA_SRGB => Ok(PIXEL_FORMAT_BC3_RGBA_SRGB),
        FORMAT_BC5_RG_UNORM => Ok(PIXEL_FORMAT_BC5_RG_UNORM),
        FORMAT_BC6H_UF16 => Ok(PIXEL_FORMAT_BC6H_UF16),
        FORMAT_BC6H_SF16 => Ok(PIXEL_FORMAT_BC6H_SF16),
        FORMAT_BC7_RGBA_UNORM => Ok(PIXEL_FORMAT_BC7_RGBA_UNORM),
        FORMAT_BC7_RGBA_SRGB => Ok(PIXEL_FORMAT_BC7_RGBA_SRGB),
        _ => Err(TextureContainerError::InvalidFormat {
            name: name.to_owned(),
            format: format!("id:{id}"),
        }),
    }
}

pub(super) fn color_space_to_id(color_space: &str, name: &str) -> Result<u16> {
    match color_space {
        COLOR_SPACE_LINEAR => Ok(COLOR_LINEAR),
        COLOR_SPACE_SRGB => Ok(COLOR_SRGB),
        other => Err(TextureContainerError::InvalidColorSpace {
            name: name.to_owned(),
            color_space: other.to_owned(),
        }),
    }
}

pub(super) fn color_space_from_id(id: u16, name: &str) -> Result<&'static str> {
    match id {
        COLOR_LINEAR => Ok(COLOR_SPACE_LINEAR),
        COLOR_SRGB => Ok(COLOR_SPACE_SRGB),
        _ => Err(TextureContainerError::InvalidColorSpace {
            name: name.to_owned(),
            color_space: format!("id:{id}"),
        }),
    }
}
