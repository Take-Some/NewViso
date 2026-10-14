//! Validated index and vertex-stream decoding.
use super::*;

pub(super) fn decode_indices(buffer: &newviso_model::IndexBuffer) -> Result<Vec<u32>, String> {
    match buffer.format {
        ModelIndexFormat::U16 => {
            let expected = buffer.index_count as usize * 2;
            if buffer.data.len() < expected {
                return Err(format!(
                    "U16 index buffer has {} bytes, expected at least {}",
                    buffer.data.len(),
                    expected
                ));
            }
            Ok(buffer.data[..expected]
                .chunks_exact(2)
                .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) as u32)
                .collect())
        }
        ModelIndexFormat::U32 => {
            let expected = buffer.index_count as usize * 4;
            if buffer.data.len() < expected {
                return Err(format!(
                    "U32 index buffer has {} bytes, expected at least {}",
                    buffer.data.len(),
                    expected
                ));
            }
            Ok(buffer.data[..expected]
                .chunks_exact(4)
                .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                .collect())
        }
    }
}

pub(super) fn read_vec2(stream: &newviso_model::VertexStream, index: usize) -> Result<[f32; 2], String> {
    if stream.format != ModelVertexFormat::Float32x2 {
        return Err(format!(
            "vertex stream {:?} must be Float32x2",
            stream.semantic
        ));
    }
    let stride = usize::try_from(stream.stride).map_err(|_| "vertex stride overflow".to_owned())?;
    let offset = index
        .checked_mul(stride)
        .ok_or_else(|| "vertex stream offset overflow".to_owned())?;
    let end = offset + 8;
    if end > stream.data.len() {
        return Err(format!(
            "vertex {} exceeds {:?} stream bytes={}",
            index,
            stream.semantic,
            stream.data.len()
        ));
    }
    Ok([
        f32::from_le_bytes(stream.data[offset..offset + 4].try_into().unwrap()),
        f32::from_le_bytes(stream.data[offset + 4..offset + 8].try_into().unwrap()),
    ])
}

pub(super) fn read_vec3(stream: &newviso_model::VertexStream, index: usize) -> Result<Vec3, String> {
    if stream.format != ModelVertexFormat::Float32x3 {
        return Err(format!(
            "vertex stream {:?} must be Float32x3",
            stream.semantic
        ));
    }
    let stride = usize::try_from(stream.stride).map_err(|_| "vertex stride overflow".to_owned())?;
    let offset = index
        .checked_mul(stride)
        .ok_or_else(|| "vertex stream offset overflow".to_owned())?;
    let end = offset + 12;
    if end > stream.data.len() {
        return Err(format!(
            "vertex {} exceeds {:?} stream bytes={}",
            index,
            stream.semantic,
            stream.data.len()
        ));
    }
    Ok(Vec3::new(
        f32::from_le_bytes(stream.data[offset..offset + 4].try_into().unwrap()),
        f32::from_le_bytes(stream.data[offset + 4..offset + 8].try_into().unwrap()),
        f32::from_le_bytes(stream.data[offset + 8..offset + 12].try_into().unwrap()),
    ))
}

pub(super) fn read_vec4(stream: &newviso_model::VertexStream, index: usize) -> Result<[f32; 4], String> {
    if stream.format != ModelVertexFormat::Float32x4 {
        return Err(format!(
            "vertex stream {:?} must be Float32x4",
            stream.semantic
        ));
    }
    let stride = usize::try_from(stream.stride).map_err(|_| "vertex stride overflow".to_owned())?;
    let offset = index
        .checked_mul(stride)
        .ok_or_else(|| "vertex stream offset overflow".to_owned())?;
    let end = offset + 16;
    if end > stream.data.len() {
        return Err(format!(
            "vertex {} exceeds {:?} stream bytes={}",
            index,
            stream.semantic,
            stream.data.len()
        ));
    }
    Ok([
        f32::from_le_bytes(stream.data[offset..offset + 4].try_into().unwrap()),
        f32::from_le_bytes(stream.data[offset + 4..offset + 8].try_into().unwrap()),
        f32::from_le_bytes(stream.data[offset + 8..offset + 12].try_into().unwrap()),
        f32::from_le_bytes(stream.data[offset + 12..offset + 16].try_into().unwrap()),
    ])
}

pub(super) fn read_skin_influences(
    vertex: &mut AssetTriangleVertex,
    indices: &newviso_model::VertexStream,
    weights: &newviso_model::VertexStream,
    extra_indices: Option<&newviso_model::VertexStream>,
    extra_weights: Option<&newviso_model::VertexStream>,
    vertex_index: usize,
) -> Result<(), String> {
    let primary_indices = read_vec4(indices, vertex_index)?;
    let primary_weights = read_vec4(weights, vertex_index)?;
    let mut count = 0usize;

    let mut append = |joint_values: [f32; 4], weight_values: [f32; 4]| -> Result<(), String> {
        for lane in 0..4 {
            let joint = joint_values[lane];
            let weight = weight_values[lane];
            if !joint.is_finite()
                || joint < 0.0
                || joint.fract().abs() > 1.0e-4
                || joint > u16::MAX as f32
                || !weight.is_finite()
                || weight < 0.0
            {
                return Err(format!(
                    "invalid skin influence vertex={} lane={} joint={} weight={}",
                    vertex_index, lane, joint, weight
                ));
            }
            if weight > 1.0e-8 && count < 8 {
                vertex.joints[count] = joint as u16;
                vertex.weights[count] = weight;
                count += 1;
            }
        }
        Ok(())
    };
    append(primary_indices, primary_weights)?;
    if let (Some(extra_indices), Some(extra_weights)) = (extra_indices, extra_weights) {
        append(
            read_vec4(extra_indices, vertex_index)?,
            read_vec4(extra_weights, vertex_index)?,
        )?;
    }
    let total = vertex.weights[..count].iter().sum::<f32>();
    if count != 0 && total > 1.0e-8 {
        for weight in &mut vertex.weights[..count] {
            *weight /= total;
        }
        vertex.skin_influences = count as u8;
    }
    Ok(())
}
