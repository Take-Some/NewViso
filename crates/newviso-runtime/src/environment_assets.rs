use super::*;

pub(super) fn read_sky_vec3(stream: &ModelVertexStream, index: usize) -> Result<[f32; 3], String> {
    if stream.format != ModelVertexFormat::Float32x3 {
        return Err(format!(
            "sky vertex stream {:?} must be Float32x3, actual={:?}",
            stream.semantic, stream.format
        ));
    }
    let bytes = sky_stream_record(stream, index, 12)?;
    Ok([
        f32::from_le_bytes(bytes[0..4].try_into().expect("four bytes")),
        f32::from_le_bytes(bytes[4..8].try_into().expect("four bytes")),
        f32::from_le_bytes(bytes[8..12].try_into().expect("four bytes")),
    ])
}

pub(super) fn read_sky_vec4(stream: &ModelVertexStream, index: usize) -> Result<[f32; 4], String> {
    if stream.format != ModelVertexFormat::Float32x4 {
        return Err(format!(
            "sky vertex stream {:?} must be Float32x4, actual={:?}",
            stream.semantic, stream.format
        ));
    }
    let bytes = sky_stream_record(stream, index, 16)?;
    Ok([
        f32::from_le_bytes(bytes[0..4].try_into().expect("four bytes")),
        f32::from_le_bytes(bytes[4..8].try_into().expect("four bytes")),
        f32::from_le_bytes(bytes[8..12].try_into().expect("four bytes")),
        f32::from_le_bytes(bytes[12..16].try_into().expect("four bytes")),
    ])
}

pub(super) fn read_sky_vec2(stream: &ModelVertexStream, index: usize) -> Result<[f32; 2], String> {
    if stream.format != ModelVertexFormat::Float32x2 {
        return Err(format!(
            "sky vertex stream {:?} must be Float32x2, actual={:?}",
            stream.semantic, stream.format
        ));
    }
    let bytes = sky_stream_record(stream, index, 8)?;
    Ok([
        f32::from_le_bytes(bytes[0..4].try_into().expect("four bytes")),
        f32::from_le_bytes(bytes[4..8].try_into().expect("four bytes")),
    ])
}

pub(super) fn sky_stream_record<'a>(
    stream: &'a ModelVertexStream,
    index: usize,
    record_bytes: usize,
) -> Result<&'a [u8], String> {
    if index >= stream.vertex_count as usize {
        return Err(format!(
            "sky vertex index {index} exceeds stream vertex_count={}",
            stream.vertex_count
        ));
    }
    let stride = usize::try_from(stream.stride)
        .map_err(|_| "sky vertex stream stride exceeds usize".to_owned())?;
    if stride < record_bytes {
        return Err(format!(
            "sky vertex stream {:?} stride={} is smaller than record bytes={record_bytes}",
            stream.semantic, stride
        ));
    }
    let offset = index
        .checked_mul(stride)
        .ok_or_else(|| "sky vertex stream offset overflow".to_owned())?;
    let end = offset
        .checked_add(record_bytes)
        .ok_or_else(|| "sky vertex stream range overflow".to_owned())?;
    stream.data.get(offset..end).ok_or_else(|| {
        format!(
            "sky vertex stream {:?} record[{index}] range={}..{} exceeds bytes={}",
            stream.semantic,
            offset,
            end,
            stream.data.len()
        )
    })
}

pub(super) fn decode_sky_indices(
    buffer: &ModelIndexBuffer,
) -> Result<(SkyIndexFormat, Vec<u32>), String> {
    match buffer.format {
        ModelIndexFormat::U16 => {
            let expected = buffer.index_count as usize * 2;
            if buffer.data.len() < expected {
                return Err(format!(
                    "sky U16 index buffer bytes={} expected={expected}",
                    buffer.data.len()
                ));
            }
            Ok((
                SkyIndexFormat::U16,
                buffer.data[..expected]
                    .chunks_exact(2)
                    .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) as u32)
                    .collect(),
            ))
        }
        ModelIndexFormat::U32 => {
            let expected = buffer.index_count as usize * 4;
            if buffer.data.len() < expected {
                return Err(format!(
                    "sky U32 index buffer bytes={} expected={expected}",
                    buffer.data.len()
                ));
            }
            Ok((
                SkyIndexFormat::U32,
                buffer.data[..expected]
                    .chunks_exact(4)
                    .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four bytes")))
                    .collect(),
            ))
        }
    }
}

pub(super) fn load_sky_texture(
    assets: &AssetClient,
    reference: &str,
    srgb: bool,
) -> Result<SkyTextureResources, String> {
    let address = AssetAddress::parse(reference)
        .map_err(|error| format!("invalid sky texture address '{reference}': {error}"))?;
    let entry = address
        .entry()
        .ok_or_else(|| format!("sky texture '{reference}' requires @entry"))?;
    let bytes = assets.decode(
        address.logical_path(),
        "texture.rgba8",
        json!({"texture_name": entry}),
    )?;
    if bytes.len() < 20 {
        return Err(format!(
            "sky texture '{reference}' returned short RGBA8 frame bytes={}",
            bytes.len()
        ));
    }
    if &bytes[0..4] != b"NTRT" {
        return Err(format!(
            "sky texture '{reference}' returned invalid RGBA8 magic"
        ));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != 1 {
        return Err(format!(
            "sky texture '{reference}' returned unsupported RGBA8 version {version}"
        ));
    }
    let width = u32::from_le_bytes(bytes[8..12].try_into().expect("four bytes"));
    let height = u32::from_le_bytes(bytes[12..16].try_into().expect("four bytes"));
    let payload_len = u32::from_le_bytes(bytes[16..20].try_into().expect("four bytes")) as usize;
    if bytes.len() != 20 + payload_len {
        return Err(format!(
            "sky texture '{reference}' RGBA8 frame size mismatch bytes={} expected={}",
            bytes.len(),
            20 + payload_len
        ));
    }
    let expected = width as usize * height as usize * 4;
    if payload_len != expected {
        return Err(format!(
            "sky texture '{reference}' RGBA8 payload={} expected={} for {}x{}",
            payload_len, expected, width, height
        ));
    }

    Ok(SkyTextureResources {
        name: entry.to_owned(),
        width,
        height,
        srgb,
        rgba8: bytes[20..].to_vec(),
    })
}
