use crate::format::{parse_pixel_format, TexturePixelFormat};
use crate::mips::{generate_rgba8_mips, rgba8_len, TextureEncodedMipData, TextureMipData};

use super::DdsExportError;

/// Writes a simple uncompressed RGBA8 DDS file with a full generated mip chain.
///
/// This is an authoring/export helper for the texture tool. Runtime never reads DDS.
pub fn write_dds_rgba8(
    width: u32,
    height: u32,
    rgba: &[u8],
) -> std::result::Result<Vec<u8>, DdsExportError> {
    if width == 0 || height == 0 {
        return Err(DdsExportError::InvalidExtent { width, height });
    }
    let expected = rgba8_len(width, height);
    if rgba.len() != expected {
        return Err(DdsExportError::InvalidPayload {
            bytes: rgba.len(),
            expected,
            width,
            height,
        });
    }
    let mips = generate_rgba8_mips(width, height, rgba.to_vec())
        .map_err(|e| DdsExportError::MipGeneration(e.to_string()))?;
    write_dds_rgba8_mip_chain(width, height, &mips)
}

pub fn write_dds_rgba8_mip_chain(
    width: u32,
    height: u32,
    mips: &[TextureMipData],
) -> std::result::Result<Vec<u8>, DdsExportError> {
    let encoded = mips
        .iter()
        .map(|m| TextureEncodedMipData {
            level: m.level,
            width: m.width,
            height: m.height,
            bytes: m.rgba.clone(),
        })
        .collect::<Vec<_>>();
    write_dds_runtime_mip_chain(width, height, crate::PIXEL_FORMAT_RGBA8_UNORM, &encoded)
}

pub fn write_dds_runtime_mip_chain(
    width: u32,
    height: u32,
    format: &str,
    mips: &[TextureEncodedMipData],
) -> std::result::Result<Vec<u8>, DdsExportError> {
    if width == 0 || height == 0 {
        return Err(DdsExportError::InvalidExtent { width, height });
    }
    if mips.is_empty() {
        return Err(DdsExportError::InvalidPayload {
            bytes: 0,
            expected: rgba8_len(width, height),
            width,
            height,
        });
    }
    let pixel_format = parse_pixel_format(format, "dds")
        .map_err(|_| DdsExportError::UnsupportedFormat(format.to_owned()))?;
    let payload_len = mips.iter().map(|m| m.bytes.len()).sum::<usize>();
    let mip_count = mips.len() as u32;
    let has_mips = mip_count > 1;

    let mut out = Vec::with_capacity(4 + 124 + 20 + payload_len);
    out.extend_from_slice(b"DDS ");
    write_u32(&mut out, 124); // dwSize
    let mut flags = 0x0000_1007; // CAPS | HEIGHT | WIDTH | PIXELFORMAT
    if pixel_format.is_rgba8() {
        flags |= 0x0000_0008;
    }
    // PITCH
    else {
        flags |= 0x0008_0000;
    } // LINEARSIZE
    if has_mips {
        flags |= 0x0002_0000;
    }
    write_u32(&mut out, flags);
    write_u32(&mut out, height);
    write_u32(&mut out, width);
    write_u32(&mut out, pitch_or_linear_size(pixel_format, width, height));
    write_u32(&mut out, 0);
    write_u32(&mut out, mip_count);
    for _ in 0..11 {
        write_u32(&mut out, 0);
    }

    write_u32(&mut out, 32); // DDPIXELFORMAT size
    match pixel_format {
        TexturePixelFormat::Rgba8Unorm | TexturePixelFormat::Rgba8Srgb => {
            write_u32(&mut out, 0x0000_0041); // DDPF_RGB | 0x0000_0001
            write_u32(&mut out, 0);
            write_u32(&mut out, 32);
            write_u32(&mut out, 0x0000_00ff);
            write_u32(&mut out, 0x0000_ff00);
            write_u32(&mut out, 0x00ff_0000);
            write_u32(&mut out, 0xff00_0000);
        }
        TexturePixelFormat::Bc1RgbaUnorm | TexturePixelFormat::Bc1RgbaSrgb => {
            write_fourcc_pf(&mut out, *b"DXT1")
        }
        TexturePixelFormat::Bc2RgbaUnorm => write_fourcc_pf(&mut out, *b"DXT3"),
        TexturePixelFormat::Bc2RgbaSrgb => write_fourcc_pf(&mut out, *b"DX10"),
        TexturePixelFormat::Bc3RgbaUnorm | TexturePixelFormat::Bc3RgbaSrgb => {
            write_fourcc_pf(&mut out, *b"DXT5")
        }
        TexturePixelFormat::Bc5RgUnorm
        | TexturePixelFormat::Bc6hUf16
        | TexturePixelFormat::Bc6hSf16
        | TexturePixelFormat::Bc7RgbaUnorm
        | TexturePixelFormat::Bc7RgbaSrgb => {
            // BC5/BC6/BC7 and sRGB BC2 require DX10 header for unambiguous tooling import.
            write_fourcc_pf(&mut out, *b"DX10");
        }
    }

    let mut caps = 0x0000_1000;
    if has_mips {
        caps |= 0x0000_0008 | 0x0040_0000;
    }
    write_u32(&mut out, caps);
    write_u32(&mut out, 0);
    write_u32(&mut out, 0);
    write_u32(&mut out, 0);
    write_u32(&mut out, 0);

    if matches!(
        pixel_format,
        TexturePixelFormat::Bc2RgbaSrgb
            | TexturePixelFormat::Bc5RgUnorm
            | TexturePixelFormat::Bc6hUf16
            | TexturePixelFormat::Bc6hSf16
            | TexturePixelFormat::Bc7RgbaUnorm
            | TexturePixelFormat::Bc7RgbaSrgb
    ) {
        write_u32(&mut out, dxgi_format(pixel_format));
        write_u32(&mut out, 3); // DDS_DIMENSION_TEXTURE2D
        write_u32(&mut out, 0);
        write_u32(&mut out, 1);
        write_u32(&mut out, 0);
    }

    for mip in mips {
        out.extend_from_slice(&mip.bytes);
    }
    Ok(out)
}

#[inline]
fn write_fourcc_pf(out: &mut Vec<u8>, cc: [u8; 4]) {
    write_u32(out, 0x0000_0004); // DDPF_FOURCC
    out.extend_from_slice(&cc);
    write_u32(out, 0);
    write_u32(out, 0);
    write_u32(out, 0);
    write_u32(out, 0);
    write_u32(out, 0);
}

#[inline]
fn pitch_or_linear_size(format: TexturePixelFormat, width: u32, height: u32) -> u32 {
    match format {
        TexturePixelFormat::Rgba8Unorm | TexturePixelFormat::Rgba8Srgb => width.saturating_mul(4),
        TexturePixelFormat::Bc1RgbaUnorm | TexturePixelFormat::Bc1RgbaSrgb => width
            .div_ceil(4)
            .saturating_mul(height.div_ceil(4))
            .saturating_mul(8),
        _ => width
            .div_ceil(4)
            .saturating_mul(height.div_ceil(4))
            .saturating_mul(16),
    }
}

#[inline]
fn dxgi_format(format: TexturePixelFormat) -> u32 {
    match format {
        TexturePixelFormat::Bc2RgbaUnorm => 74, // DXGI_FORMAT_BC2_UNORM
        TexturePixelFormat::Bc2RgbaSrgb => 75,  // DXGI_FORMAT_BC2_UNORM_SRGB
        TexturePixelFormat::Bc5RgUnorm => 83,   // DXGI_FORMAT_BC5_UNORM
        TexturePixelFormat::Bc6hUf16 => 95,     // DXGI_FORMAT_BC6H_UF16
        TexturePixelFormat::Bc6hSf16 => 96,     // DXGI_FORMAT_BC6H_SF16
        TexturePixelFormat::Bc7RgbaUnorm => 98, // DXGI_FORMAT_BC7_UNORM
        TexturePixelFormat::Bc7RgbaSrgb => 99,  // DXGI_FORMAT_BC7_UNORM_SRGB
        _ => 0,
    }
}

#[inline]
fn write_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
