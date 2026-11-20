use crate::format::texture_payload_len;
use crate::mips::TextureEncodedMipData;
use crate::{
    COLOR_SPACE_LINEAR, COLOR_SPACE_SRGB, PIXEL_FORMAT_BC1_RGBA_UNORM, PIXEL_FORMAT_BC2_RGBA_UNORM,
    PIXEL_FORMAT_BC3_RGBA_UNORM, PIXEL_FORMAT_BC5_RG_UNORM, PIXEL_FORMAT_RGBA8_SRGB,
    PIXEL_FORMAT_RGBA8_UNORM,
};

use super::{
    reader_utils::{checked_mip_slice, choose_legacy_row_pitches, read_fourcc_at, read_u32_at},
    source_layout::DdsSourceLayout,
    DdsImportError, DdsRuntimeTexture, DDPF_FOURCC, DDPF_LUMINANCE, DDPF_RGB,
};

/// Imports a 2D DDS texture into the canonical runtime mip representation used by YTD.
///
/// Legacy BGRA8, RGB24/BGR24 and L8 source payloads are normalized to RGBA8 while
/// preserving every supplied mip level. BC payloads remain GPU-native and are copied
/// without decompression. Runtime texture references remain `.ytd@entry`; this decoder
/// is the shared engine/import-tool boundary for authoring DDS sources.
pub fn read_dds_runtime_texture(bytes: &[u8]) -> Result<DdsRuntimeTexture, DdsImportError> {
    if bytes.len() < 128 || bytes.get(0..4) != Some(b"DDS ") {
        return Err(DdsImportError::InvalidHeader(
            "missing DDS magic or 124-byte legacy header".to_owned(),
        ));
    }
    let header_size = read_u32_at(bytes, 4)?;
    if header_size != 124 {
        return Err(DdsImportError::InvalidHeader(format!(
            "unsupported header size {header_size}; expected 124"
        )));
    }
    let header_flags = read_u32_at(bytes, 8)?;
    let height = read_u32_at(bytes, 12)?;
    let width = read_u32_at(bytes, 16)?;
    if width == 0 || height == 0 {
        return Err(DdsImportError::InvalidExtent { width, height });
    }
    let pitch_or_linear_size = read_u32_at(bytes, 20)? as usize;
    let mip_count = read_u32_at(bytes, 28)?.max(1);
    let pixel_format_size = read_u32_at(bytes, 76)?;
    if pixel_format_size != 32 {
        return Err(DdsImportError::InvalidHeader(format!(
            "unsupported pixel-format size {pixel_format_size}; expected 32"
        )));
    }

    let pixel_format_flags = read_u32_at(bytes, 80)?;
    let fourcc = read_fourcc_at(bytes, 84)?;
    let rgb_bit_count = read_u32_at(bytes, 88)?;
    let r_mask = read_u32_at(bytes, 92)?;
    let g_mask = read_u32_at(bytes, 96)?;
    let b_mask = read_u32_at(bytes, 100)?;
    let a_mask = read_u32_at(bytes, 104)?;

    let mut data_offset = 128usize;
    let (format, source_layout) = if (pixel_format_flags & DDPF_FOURCC) != 0 {
        match &fourcc {
            b"DXT1" => (PIXEL_FORMAT_BC1_RGBA_UNORM.to_owned(), DdsSourceLayout::Native),
            b"DXT3" => (PIXEL_FORMAT_BC2_RGBA_UNORM.to_owned(), DdsSourceLayout::Native),
            b"DXT5" => (PIXEL_FORMAT_BC3_RGBA_UNORM.to_owned(), DdsSourceLayout::Native),
            b"ATI2" | b"BC5U" => {
                (PIXEL_FORMAT_BC5_RG_UNORM.to_owned(), DdsSourceLayout::Native)
            }
            b"DX10" => {
                if bytes.len() < 148 {
                    return Err(DdsImportError::InvalidHeader(
                        "DX10 header is truncated".to_owned(),
                    ));
                }
                data_offset = 148;
                let dxgi = read_u32_at(bytes, 128)?;
                dxgi_runtime_format(dxgi)?
            }
            other => {
                return Err(DdsImportError::UnsupportedFormat(format!(
                    "FourCC '{}'; supported DXT1/DXT3/DXT5/ATI2/BC5U and DX10 RGBA8/BC1/BC2/BC3/BC5/BC6H/BC7",
                    String::from_utf8_lossy(other)
                )))
            }
        }
    } else if (pixel_format_flags & DDPF_RGB) != 0 {
        match (rgb_bit_count, r_mask, g_mask, b_mask, a_mask) {
            (32, 0x0000_00ff, 0x0000_ff00, 0x00ff_0000, 0xff00_0000) => {
                (PIXEL_FORMAT_RGBA8_UNORM.to_owned(), DdsSourceLayout::Rgba8)
            }
            (32, 0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000) => {
                (PIXEL_FORMAT_RGBA8_UNORM.to_owned(), DdsSourceLayout::Bgra8)
            }
            (24, 0x0000_00ff, 0x0000_ff00, 0x00ff_0000, 0) => {
                (PIXEL_FORMAT_RGBA8_UNORM.to_owned(), DdsSourceLayout::Rgb24)
            }
            (24, 0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0) => {
                (PIXEL_FORMAT_RGBA8_UNORM.to_owned(), DdsSourceLayout::Bgr24)
            }
            _ => {
                return Err(DdsImportError::UnsupportedFormat(format!(
                    "legacy RGB bits={rgb_bit_count} flags={pixel_format_flags:#x} masks r={r_mask:#x} g={g_mask:#x} b={b_mask:#x} a={a_mask:#x}"
                )))
            }
        }
    } else if (pixel_format_flags & DDPF_LUMINANCE) != 0 {
        match (rgb_bit_count, r_mask, g_mask, b_mask, a_mask) {
            (8, 0x0000_00ff, 0, 0, 0) => {
                (PIXEL_FORMAT_RGBA8_UNORM.to_owned(), DdsSourceLayout::L8)
            }
            _ => {
                return Err(DdsImportError::UnsupportedFormat(format!(
                    "legacy luminance bits={rgb_bit_count} masks r={r_mask:#x} g={g_mask:#x} b={b_mask:#x} a={a_mask:#x}"
                )))
            }
        }
    } else {
        return Err(DdsImportError::UnsupportedFormat(format!(
            "flags={pixel_format_flags:#x} bits={rgb_bit_count}"
        )));
    };

    let color_space = if format.ends_with("_SRGB") {
        COLOR_SPACE_SRGB.to_owned()
    } else {
        COLOR_SPACE_LINEAR.to_owned()
    };
    let available_payload = bytes.len().saturating_sub(data_offset);
    let mut mips = Vec::with_capacity(mip_count as usize);
    let mut offset = data_offset;
    let mut mip_width = width;
    let mut mip_height = height;

    if source_layout.is_native() {
        for level in 0..mip_count {
            let len = texture_payload_len(&format, mip_width, mip_height).map_err(|e| {
                DdsImportError::InvalidPayload(format!(
                    "native mip length failed level={level} format={format}: {e}"
                ))
            })?;
            let payload = checked_mip_slice(bytes, offset, len, level)?;
            mips.push(TextureEncodedMipData {
                level,
                width: mip_width,
                height: mip_height,
                bytes: payload.to_vec(),
            });
            offset = offset
                .checked_add(len)
                .ok_or(DdsImportError::Overflow("native mip offset"))?;
            mip_width = (mip_width / 2).max(1);
            mip_height = (mip_height / 2).max(1);
        }
    } else {
        let row_pitches = choose_legacy_row_pitches(
            source_layout,
            width,
            height,
            mip_count,
            header_flags,
            pitch_or_linear_size,
            available_payload,
        )?;
        for (level, row_pitch) in row_pitches.into_iter().enumerate() {
            let level = level as u32;
            let len = row_pitch
                .checked_mul(mip_height as usize)
                .ok_or(DdsImportError::Overflow("legacy mip source length"))?;
            let payload = checked_mip_slice(bytes, offset, len, level)?;
            let rgba = source_layout.decode_rows(payload, mip_width, mip_height, row_pitch)?;
            mips.push(TextureEncodedMipData {
                level,
                width: mip_width,
                height: mip_height,
                bytes: rgba,
            });
            offset = offset
                .checked_add(len)
                .ok_or(DdsImportError::Overflow("legacy mip offset"))?;
            mip_width = (mip_width / 2).max(1);
            mip_height = (mip_height / 2).max(1);
        }
    }

    Ok(DdsRuntimeTexture {
        width,
        height,
        format,
        color_space,
        mips,
    })
}

fn dxgi_runtime_format(dxgi: u32) -> Result<(String, DdsSourceLayout), DdsImportError> {
    let value = match dxgi {
        28 => (PIXEL_FORMAT_RGBA8_UNORM.to_owned(), DdsSourceLayout::Rgba8),
        29 => (PIXEL_FORMAT_RGBA8_SRGB.to_owned(), DdsSourceLayout::Rgba8),
        71 => (
            crate::PIXEL_FORMAT_BC1_RGBA_UNORM.to_owned(),
            DdsSourceLayout::Native,
        ),
        72 => (
            crate::PIXEL_FORMAT_BC1_RGBA_SRGB.to_owned(),
            DdsSourceLayout::Native,
        ),
        74 => (
            crate::PIXEL_FORMAT_BC2_RGBA_UNORM.to_owned(),
            DdsSourceLayout::Native,
        ),
        75 => (
            crate::PIXEL_FORMAT_BC2_RGBA_SRGB.to_owned(),
            DdsSourceLayout::Native,
        ),
        77 => (
            crate::PIXEL_FORMAT_BC3_RGBA_UNORM.to_owned(),
            DdsSourceLayout::Native,
        ),
        78 => (
            crate::PIXEL_FORMAT_BC3_RGBA_SRGB.to_owned(),
            DdsSourceLayout::Native,
        ),
        83 => (
            crate::PIXEL_FORMAT_BC5_RG_UNORM.to_owned(),
            DdsSourceLayout::Native,
        ),
        95 => (
            crate::PIXEL_FORMAT_BC6H_UF16.to_owned(),
            DdsSourceLayout::Native,
        ),
        96 => (
            crate::PIXEL_FORMAT_BC6H_SF16.to_owned(),
            DdsSourceLayout::Native,
        ),
        98 => (
            crate::PIXEL_FORMAT_BC7_RGBA_UNORM.to_owned(),
            DdsSourceLayout::Native,
        ),
        99 => (
            crate::PIXEL_FORMAT_BC7_RGBA_SRGB.to_owned(),
            DdsSourceLayout::Native,
        ),
        other => {
            return Err(DdsImportError::UnsupportedFormat(format!(
                "DXGI format {other}; supported RGBA8, BC1, BC2, BC3, BC5, BC6H and BC7"
            )))
        }
    };
    Ok(value)
}
