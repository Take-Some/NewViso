mod common;
mod decode;
mod encode;

use crate::format::{
    parse_pixel_format, TexturePixelFormat, PIXEL_FORMAT_BC1_RGBA_SRGB,
    PIXEL_FORMAT_BC1_RGBA_UNORM, PIXEL_FORMAT_BC3_RGBA_SRGB, PIXEL_FORMAT_BC3_RGBA_UNORM,
    PIXEL_FORMAT_BC5_RG_UNORM,
};
use crate::mips::{rgba8_len, TextureEncodedMipData, TextureMipData};
use crate::{COLOR_SPACE_LINEAR, COLOR_SPACE_SRGB};

#[derive(Debug, thiserror::Error)]
pub enum BcnEncodeError {
    #[error(
        "bcn: invalid RGBA8 payload bytes={bytes} expected={expected} extent={width}x{height}"
    )]
    InvalidRgbaPayload {
        bytes: usize,
        expected: usize,
        width: u32,
        height: u32,
    },
    #[error("bcn: unsupported encoder target '{0}'")]
    UnsupportedTarget(String),
    #[error("bcn: unsupported decoder source '{0}'")]
    UnsupportedSource(String),
    #[error("bcn: invalid BCn payload bytes={bytes} expected={expected} extent={width}x{height} format={format}")]
    InvalidBcnPayload {
        bytes: usize,
        expected: usize,
        width: u32,
        height: u32,
        format: String,
    },
}

pub fn infer_bcn_format(name: &str, color_space: &str, rgba: &[u8]) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.contains("normal")
        || lower.contains("roughness")
        || lower.contains("metallic")
        || lower.contains("occlusion")
        || lower.contains("_ao")
    {
        return PIXEL_FORMAT_BC5_RG_UNORM;
    }

    let has_alpha = rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] < 250)
        || lower.contains("opacity")
        || lower.contains("alpha")
        || lower.contains("leaf")
        || lower.contains("specularglossiness")
        || lower.contains("specular_glossiness");
    if has_alpha {
        return if color_space.eq_ignore_ascii_case(COLOR_SPACE_SRGB) {
            PIXEL_FORMAT_BC3_RGBA_SRGB
        } else {
            PIXEL_FORMAT_BC3_RGBA_UNORM
        };
    }

    if color_space.eq_ignore_ascii_case(COLOR_SPACE_LINEAR) {
        PIXEL_FORMAT_BC1_RGBA_UNORM
    } else {
        PIXEL_FORMAT_BC1_RGBA_SRGB
    }
}

pub fn encode_rgba8_mips_to_bcn(
    format: &str,
    mips: &[TextureMipData],
) -> Result<Vec<TextureEncodedMipData>, BcnEncodeError> {
    let target = parse_pixel_format(format, "<bcn>")
        .map_err(|_| BcnEncodeError::UnsupportedTarget(format.to_owned()))?;
    let mut out = Vec::with_capacity(mips.len());
    for mip in mips {
        let expected = rgba8_len(mip.width, mip.height);
        if mip.rgba.len() != expected {
            return Err(BcnEncodeError::InvalidRgbaPayload {
                bytes: mip.rgba.len(),
                expected,
                width: mip.width,
                height: mip.height,
            });
        }
        let bytes = match target {
            TexturePixelFormat::Bc1RgbaUnorm | TexturePixelFormat::Bc1RgbaSrgb => {
                encode::encode_bc1(mip.width, mip.height, &mip.rgba)
            }
            TexturePixelFormat::Bc3RgbaUnorm | TexturePixelFormat::Bc3RgbaSrgb => {
                encode::encode_bc3(mip.width, mip.height, &mip.rgba)
            }
            TexturePixelFormat::Bc5RgUnorm => encode::encode_bc5(mip.width, mip.height, &mip.rgba),
            _ => return Err(BcnEncodeError::UnsupportedTarget(format.to_owned())),
        };
        out.push(TextureEncodedMipData {
            level: mip.level,
            width: mip.width,
            height: mip.height,
            bytes,
        });
    }
    Ok(out)
}

pub fn decode_bcn_to_rgba8(
    format: &str,
    width: u32,
    height: u32,
    bytes: &[u8],
) -> Result<Vec<u8>, BcnEncodeError> {
    let source = parse_pixel_format(format, "<bcn>")
        .map_err(|_| BcnEncodeError::UnsupportedSource(format.to_owned()))?;
    match source {
        TexturePixelFormat::Bc1RgbaUnorm | TexturePixelFormat::Bc1RgbaSrgb => {
            decode::decode_bc1(width, height, bytes, format)
        }
        TexturePixelFormat::Bc3RgbaUnorm | TexturePixelFormat::Bc3RgbaSrgb => {
            decode::decode_bc3(width, height, bytes, format)
        }
        TexturePixelFormat::Bc5RgUnorm => decode::decode_bc5(width, height, bytes, format),
        _ => Err(BcnEncodeError::UnsupportedSource(format.to_owned())),
    }
}
