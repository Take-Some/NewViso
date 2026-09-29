use std::num::NonZeroU32;

use super::codec::*;
use crate::{
    Extent2D, TextureDataPolicy, TextureDesc, TextureFormat, TextureId, TextureMipDataDesc,
    TextureUsage,
};

const CREATE_TEXTURE_BIN_MAGIC: &[u8; 8] = b"NECT\x01\0\0\0";
const CREATE_TEXTURE_RESPONSE_BIN_MAGIC: &[u8; 8] = b"NETR\x01\0\0\0";

/// Compact binary transport for `TextureDesc`, including an optional full mip chain.
/// JSON is intentionally not used here because large `Vec<u8>` payloads expand into
/// millions of decimal tokens and can stall the native window for tens of seconds.
pub fn encode_create_texture_bin(desc: &TextureDesc) -> Result<Vec<u8>, String> {
    let payload_len = desc.data.as_ref().map_or(0, Vec::len);
    let mut out = Vec::with_capacity(payload_len.saturating_add(128));
    out.extend_from_slice(CREATE_TEXTURE_BIN_MAGIC);
    match desc.label.as_deref() {
        Some(label) => {
            put_u8(&mut out, 1);
            put_bytes(&mut out, label.as_bytes(), "texture label")?;
        }
        None => put_u8(&mut out, 0),
    }
    put_u32(&mut out, desc.extent.width);
    put_u32(&mut out, desc.extent.height);
    put_u8(&mut out, texture_format_tag(desc.format));
    put_u8(&mut out, texture_usage_tag(desc.usage));
    put_u32(&mut out, desc.mip_levels.get());
    put_u8(&mut out, texture_data_policy_tag(desc.data_policy));
    put_len(&mut out, desc.mip_data.len(), "texture mip layout")?;
    for mip in &desc.mip_data {
        put_u32(&mut out, mip.level);
        put_u32(&mut out, mip.width);
        put_u32(&mut out, mip.height);
        put_u64(&mut out, mip.offset);
        put_u64(&mut out, mip.byte_len);
    }
    match desc.data.as_deref() {
        Some(data) => {
            put_u8(&mut out, 1);
            put_bytes(&mut out, data, "texture payload")?;
        }
        None => put_u8(&mut out, 0),
    }
    Ok(out)
}

pub fn decode_create_texture_bin(bytes: &[u8]) -> Result<TextureDesc, String> {
    let mut r = BinReader::new(bytes);
    if r.take(8)? != CREATE_TEXTURE_BIN_MAGIC {
        return Err("create-texture binary packet has invalid magic".to_owned());
    }
    let label = match r.u8()? {
        0 => None,
        1 => Some(r.string()?),
        tag => return Err(format!("invalid create-texture label presence tag {tag}")),
    };
    let extent = Extent2D::new(r.u32()?, r.u32()?);
    let format = texture_format_from_tag(r.u8()?)?;
    let usage = texture_usage_from_tag(r.u8()?)?;
    let mip_levels = NonZeroU32::new(r.u32()?)
        .ok_or_else(|| "create-texture binary packet has zero mip levels".to_owned())?;
    let data_policy = texture_data_policy_from_tag(r.u8()?)?;
    let mip_count = r.u32()? as usize;
    let mut mip_data = Vec::with_capacity(mip_count);
    for _ in 0..mip_count {
        mip_data.push(TextureMipDataDesc::new(
            r.u32()?,
            r.u32()?,
            r.u32()?,
            r.u64()?,
            r.u64()?,
        ));
    }
    let data = match r.u8()? {
        0 => None,
        1 => Some(r.bytes_vec()?),
        tag => return Err(format!("invalid create-texture data presence tag {tag}")),
    };
    if !r.is_eof() {
        return Err("create-texture binary packet has trailing bytes".to_owned());
    }
    Ok(TextureDesc {
        label,
        extent,
        format,
        usage,
        mip_levels,
        data,
        mip_data,
        data_policy,
    })
}

pub fn encode_texture_id_bin(id: TextureId) -> Vec<u8> {
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(CREATE_TEXTURE_RESPONSE_BIN_MAGIC);
    put_u32(&mut out, id.get());
    out
}

pub fn decode_texture_id_bin(bytes: &[u8]) -> Result<TextureId, String> {
    let mut r = BinReader::new(bytes);
    if r.take(8)? != CREATE_TEXTURE_RESPONSE_BIN_MAGIC {
        return Err("create-texture binary response has invalid magic".to_owned());
    }
    let id = TextureId::new(r.u32()?);
    if !r.is_eof() {
        return Err("create-texture binary response has trailing bytes".to_owned());
    }
    Ok(id)
}

#[inline]
fn texture_format_tag(format: TextureFormat) -> u8 {
    match format {
        TextureFormat::Rgba8Unorm => 1,
        TextureFormat::Rgba8Srgb => 2,
        TextureFormat::Bgra8Unorm => 3,
        TextureFormat::Bgra8Srgb => 4,
        TextureFormat::Rgba16Float => 5,
        TextureFormat::R32Float => 6,
        TextureFormat::Bc1RgbaUnorm => 7,
        TextureFormat::Bc1RgbaSrgb => 8,
        TextureFormat::Bc3RgbaUnorm => 9,
        TextureFormat::Bc3RgbaSrgb => 10,
        TextureFormat::Bc5RgUnorm => 11,
        TextureFormat::Bc7RgbaUnorm => 12,
        TextureFormat::Bc7RgbaSrgb => 13,
        TextureFormat::Depth24Stencil8 => 14,
        TextureFormat::Depth32Float => 15,
    }
}

fn texture_format_from_tag(tag: u8) -> Result<TextureFormat, String> {
    match tag {
        1 => Ok(TextureFormat::Rgba8Unorm),
        2 => Ok(TextureFormat::Rgba8Srgb),
        3 => Ok(TextureFormat::Bgra8Unorm),
        4 => Ok(TextureFormat::Bgra8Srgb),
        5 => Ok(TextureFormat::Rgba16Float),
        6 => Ok(TextureFormat::R32Float),
        7 => Ok(TextureFormat::Bc1RgbaUnorm),
        8 => Ok(TextureFormat::Bc1RgbaSrgb),
        9 => Ok(TextureFormat::Bc3RgbaUnorm),
        10 => Ok(TextureFormat::Bc3RgbaSrgb),
        11 => Ok(TextureFormat::Bc5RgUnorm),
        12 => Ok(TextureFormat::Bc7RgbaUnorm),
        13 => Ok(TextureFormat::Bc7RgbaSrgb),
        14 => Ok(TextureFormat::Depth24Stencil8),
        15 => Ok(TextureFormat::Depth32Float),
        _ => Err(format!("invalid texture format binary tag {tag}")),
    }
}

#[inline]
fn texture_usage_tag(usage: TextureUsage) -> u8 {
    match usage {
        TextureUsage::Sampled => 1,
        TextureUsage::RenderTarget => 2,
        TextureUsage::DepthStencil => 3,
        TextureUsage::Storage => 4,
    }
}

fn texture_usage_from_tag(tag: u8) -> Result<TextureUsage, String> {
    match tag {
        1 => Ok(TextureUsage::Sampled),
        2 => Ok(TextureUsage::RenderTarget),
        3 => Ok(TextureUsage::DepthStencil),
        4 => Ok(TextureUsage::Storage),
        _ => Err(format!("invalid texture usage binary tag {tag}")),
    }
}

#[inline]
fn texture_data_policy_tag(policy: TextureDataPolicy) -> u8 {
    match policy {
        TextureDataPolicy::Immediate => 1,
        TextureDataPolicy::Deferred => 2,
        TextureDataPolicy::Empty => 3,
    }
}

fn texture_data_policy_from_tag(tag: u8) -> Result<TextureDataPolicy, String> {
    match tag {
        1 => Ok(TextureDataPolicy::Immediate),
        2 => Ok(TextureDataPolicy::Deferred),
        3 => Ok(TextureDataPolicy::Empty),
        _ => Err(format!("invalid texture data-policy binary tag {tag}")),
    }
}
