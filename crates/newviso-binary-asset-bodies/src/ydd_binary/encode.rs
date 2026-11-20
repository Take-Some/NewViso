//! Canonical encoder for the current binary YDD body schema.

use std::collections::BTreeMap;

use super::decode::{
    BODY_HEADER_LEN, ENTRY_FLAG_SKIN_SOURCE_TO_MODEL, ENTRY_RECORD_LEN, MESH_FLAG_SKINNED,
    MESH_HEADER_LEN_V3, MESH_HEADER_LEN_V5, SKIN_VERTEX_STRIDE_V4, VERTEX_STRIDE,
};
use super::*;

pub fn encode_ydd_binary_body(document: &YddBinaryDocument) -> Result<Vec<u8>, String> {
    if document.entries.is_empty() {
        return Err("binary YDD encode requires at least one entry".to_owned());
    }

    // Preserve the V4 binary wire exactly for documents without inline materials.
    // V5 extends each mesh record by one optional JSON string-table offset.
    let version = if document.entries.iter().any(|entry| {
        entry
            .meshes
            .iter()
            .any(|mesh| mesh.inline_material.is_some())
    }) {
        YDD_BINARY_SCHEMA_VERSION
    } else {
        YDD_BINARY_SCHEMA_VERSION_V4
    };
    let mut strings = Vec::<u8>::new();
    let mut offsets = BTreeMap::<String, u32>::new();
    for entry in &document.entries {
        require_text(&entry.name, "entry name")?;
        require_text(&entry.source_path, "entry source_path")?;
        intern(&mut strings, &mut offsets, &entry.name)?;
        intern(&mut strings, &mut offsets, &entry.source_path)?;
        if let Some(value) = entry.properties_ref.as_deref() {
            intern(&mut strings, &mut offsets, value)?;
        }
        for mesh in &entry.meshes {
            require_text(&mesh.name, "mesh name")?;
            intern(&mut strings, &mut offsets, &mesh.name)?;
            if let Some(value) = mesh.material_ref.as_deref() {
                intern(&mut strings, &mut offsets, value)?;
            }
            if let Some(value) = mesh.inline_material.as_ref() {
                if !value.is_object() {
                    return Err(format!(
                        "binary YDD inline material must be object mesh='{}'",
                        mesh.name
                    ));
                }
                let encoded = serde_json::to_string(value).map_err(|error| {
                    format!("binary YDD inline material JSON encode failed: {error}")
                })?;
                if encoded.len() > 65536 {
                    return Err(format!(
                        "binary YDD inline material exceeds 64 KiB mesh='{}'",
                        mesh.name
                    ));
                }
                intern(&mut strings, &mut offsets, &encoded)?;
            }
        }
    }

    let mut payloads = Vec::with_capacity(document.entries.len());
    let mut summaries = Vec::with_capacity(document.entries.len());
    for entry in &document.entries {
        validate_bounds(entry.bounds_min, entry.bounds_max, "entry")?;
        if entry.meshes.is_empty() {
            return Err(format!(
                "binary YDD entry '{}' contains no meshes",
                entry.name
            ));
        }
        let has_skinned_mesh = entry.meshes.iter().any(YddBinaryMesh::is_skinned);
        if has_skinned_mesh && entry.skin_source_to_model.is_none() {
            return Err(format!(
                "binary YDD skinned entry '{}' requires skin_source_to_model",
                entry.name
            ));
        }
        if let Some(matrix) = entry.skin_source_to_model {
            if matrix.iter().any(|value| !value.is_finite()) {
                return Err(format!(
                    "binary YDD entry '{}' contains non-finite skin_source_to_model",
                    entry.name
                ));
            }
        }

        let mut payload = Vec::new();
        push_u32(&mut payload, u32_len(entry.meshes.len(), "mesh count")?);
        let entry_flags = if entry.skin_source_to_model.is_some() {
            ENTRY_FLAG_SKIN_SOURCE_TO_MODEL
        } else {
            0
        };
        push_u32(&mut payload, entry_flags);
        if let Some(matrix) = entry.skin_source_to_model {
            for value in matrix {
                push_f32(&mut payload, value);
            }
        }

        let mut total_vertices = 0usize;
        let mut total_indices = 0usize;
        for mesh in &entry.meshes {
            validate_bounds(mesh.bounds_min, mesh.bounds_max, "mesh")?;
            if mesh.vertices.is_empty() || mesh.indices.is_empty() {
                return Err(format!(
                    "binary YDD mesh is empty entry='{}' mesh='{}' vertices={} indices={}",
                    entry.name,
                    mesh.name,
                    mesh.vertices.len(),
                    mesh.indices.len()
                ));
            }
            if mesh.indices.len() % 3 != 0 {
                return Err(format!(
                    "binary YDD mesh index count is not triangular entry='{}' mesh='{}' indices={}",
                    entry.name,
                    mesh.name,
                    mesh.indices.len()
                ));
            }
            let skin = mesh.skin.as_deref();
            if let Some(skin) = skin {
                if skin.len() != mesh.vertices.len() {
                    return Err(format!(
                        "binary YDD skin vertex-count mismatch entry='{}' mesh='{}' skin={} vertices={}",
                        entry.name,
                        mesh.name,
                        skin.len(),
                        mesh.vertices.len()
                    ));
                }
            }

            push_u32(&mut payload, string_offset(&offsets, &mesh.name)?);
            push_u32(
                &mut payload,
                optional_string_offset(&offsets, mesh.material_ref.as_deref())?,
            );
            push_u32(&mut payload, u32_len(mesh.vertices.len(), "vertex count")?);
            push_u32(&mut payload, u32_len(mesh.indices.len(), "index count")?);
            push_vec3(&mut payload, mesh.bounds_min);
            push_vec3(&mut payload, mesh.bounds_max);
            push_u32(
                &mut payload,
                if skin.is_some() { MESH_FLAG_SKINNED } else { 0 },
            );
            push_u32(
                &mut payload,
                if skin.is_some() {
                    SKIN_VERTEX_STRIDE_V4 as u32
                } else {
                    0
                },
            );
            if version >= YDD_BINARY_SCHEMA_VERSION {
                let encoded = mesh
                    .inline_material
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|error| format!("YDD inline material encode: {error}"))?;
                push_u32(
                    &mut payload,
                    optional_string_offset(&offsets, encoded.as_deref())?,
                );
            }
            debug_assert_eq!(payload.len() % 4, 0);

            for vertex in &mesh.vertices {
                if vertex
                    .position
                    .iter()
                    .chain(vertex.normal.iter())
                    .chain(vertex.uv0.iter())
                    .any(|value| !value.is_finite())
                {
                    return Err(format!(
                        "binary YDD vertex contains non-finite value entry='{}' mesh='{}'",
                        entry.name, mesh.name
                    ));
                }
                push_vec3(&mut payload, vertex.position);
                push_vec3(&mut payload, vertex.normal);
                push_f32(&mut payload, vertex.uv0[0]);
                push_f32(&mut payload, vertex.uv0[1]);
            }
            if let Some(skin) = skin {
                for vertex in skin {
                    validate_skin_vertex(vertex, &entry.name, &mesh.name)?;
                    for joint in vertex.joints {
                        push_u16(&mut payload, joint);
                    }
                    for weight in vertex.weights {
                        push_f32(&mut payload, weight);
                    }
                    for joint in vertex.joints_extra {
                        push_u16(&mut payload, joint);
                    }
                    for weight in vertex.weights_extra {
                        push_f32(&mut payload, weight);
                    }
                }
            }
            for &index in &mesh.indices {
                if index as usize >= mesh.vertices.len() {
                    return Err(format!(
                        "binary YDD index out of bounds entry='{}' mesh='{}' index={} vertices={}",
                        entry.name,
                        mesh.name,
                        index,
                        mesh.vertices.len()
                    ));
                }
                push_u32(&mut payload, index);
            }
            total_vertices = total_vertices
                .checked_add(mesh.vertices.len())
                .ok_or("binary YDD total vertex count overflow")?;
            total_indices = total_indices
                .checked_add(mesh.indices.len())
                .ok_or("binary YDD total index count overflow")?;
        }
        payloads.push(payload);
        summaries.push((total_vertices, total_indices));
    }

    let table_offset = BODY_HEADER_LEN;
    let table_len = document
        .entries
        .len()
        .checked_mul(ENTRY_RECORD_LEN)
        .ok_or("binary YDD entry table overflow")?;
    let string_table_offset = table_offset
        .checked_add(table_len)
        .ok_or("binary YDD string table offset overflow")?;
    let payload_floor = string_table_offset
        .checked_add(strings.len())
        .ok_or("binary YDD payload floor overflow")?;
    let mut payload_offsets = Vec::with_capacity(payloads.len());
    let mut cursor = payload_floor;
    for payload in &payloads {
        payload_offsets.push(cursor);
        cursor = cursor
            .checked_add(payload.len())
            .ok_or("binary YDD output size overflow")?;
    }

    let mut out = vec![0u8; cursor];
    write_u32(&mut out, 0, version)?;
    write_u32(&mut out, 4, u32_len(document.entries.len(), "entry count")?)?;
    write_u64(&mut out, 8, table_offset as u64)?;
    write_u64(&mut out, 16, string_table_offset as u64)?;
    write_u64(&mut out, 24, strings.len() as u64)?;
    write_u64(&mut out, 32, payload_floor as u64)?;
    out[string_table_offset..string_table_offset + strings.len()].copy_from_slice(&strings);

    for (index, entry) in document.entries.iter().enumerate() {
        let record = table_offset + index * ENTRY_RECORD_LEN;
        let (total_vertices, total_indices) = summaries[index];
        write_u64(&mut out, record, 0)?;
        write_u32(&mut out, record + 8, string_offset(&offsets, &entry.name)?)?;
        write_u32(
            &mut out,
            record + 12,
            string_offset(&offsets, &entry.source_path)?,
        )?;
        write_u32(
            &mut out,
            record + 16,
            u32_len(entry.meshes.len(), "mesh count")?,
        )?;
        write_u32(
            &mut out,
            record + 20,
            u32_len(total_vertices, "vertex count")?,
        )?;
        write_u32(
            &mut out,
            record + 24,
            u32_len(total_indices, "index count")?,
        )?;
        write_u32(&mut out, record + 28, 0)?;
        write_u32(
            &mut out,
            record + 32,
            optional_string_offset(&offsets, entry.properties_ref.as_deref())?,
        )?;
        write_vec3(&mut out, record + 36, entry.bounds_min)?;
        write_vec3(&mut out, record + 48, entry.bounds_max)?;
        write_u32(&mut out, record + 60, 0)?;
        write_u64(&mut out, record + 64, payload_offsets[index] as u64)?;
        write_u64(&mut out, record + 72, payloads[index].len() as u64)?;
        let start = payload_offsets[index];
        out[start..start + payloads[index].len()].copy_from_slice(&payloads[index]);
    }
    Ok(out)
}

fn validate_skin_vertex(
    vertex: &YddBinarySkinVertex,
    entry: &str,
    mesh: &str,
) -> Result<(), String> {
    if vertex
        .weights
        .iter()
        .chain(vertex.weights_extra.iter())
        .any(|weight| !weight.is_finite() || *weight < 0.0)
    {
        return Err(format!(
            "binary YDD skin contains invalid weight entry='{entry}' mesh='{mesh}'"
        ));
    }
    let sum = vertex.total_weight();
    if !sum.is_finite() || (sum - 1.0).abs() > 0.01 {
        return Err(format!(
            "binary YDD skin weights are not normalized entry='{entry}' mesh='{mesh}' sum={sum}"
        ));
    }
    Ok(())
}

fn validate_bounds(min: [f32; 3], max: [f32; 3], label: &str) -> Result<(), String> {
    if min.iter().chain(max.iter()).any(|value| !value.is_finite()) {
        return Err(format!(
            "binary YDD {label} bounds contain non-finite values"
        ));
    }
    if (0..3).any(|axis| min[axis] > max[axis]) {
        return Err(format!(
            "binary YDD {label} bounds are inverted min={min:?} max={max:?}"
        ));
    }
    Ok(())
}

fn require_text(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("binary YDD {label} must not be empty"))
    } else {
        Ok(())
    }
}

fn intern(
    strings: &mut Vec<u8>,
    offsets: &mut BTreeMap<String, u32>,
    value: &str,
) -> Result<u32, String> {
    if let Some(offset) = offsets.get(value) {
        return Ok(*offset);
    }
    let offset = u32_len(strings.len(), "string offset")?;
    strings.extend_from_slice(value.as_bytes());
    strings.push(0);
    offsets.insert(value.to_owned(), offset);
    Ok(offset)
}

fn string_offset(offsets: &BTreeMap<String, u32>, value: &str) -> Result<u32, String> {
    offsets
        .get(value)
        .copied()
        .ok_or_else(|| format!("binary YDD string was not interned value='{value}'"))
}

fn optional_string_offset(
    offsets: &BTreeMap<String, u32>,
    value: Option<&str>,
) -> Result<u32, String> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => string_offset(offsets, value),
        None => Ok(u32::MAX),
    }
}

#[inline]
fn u32_len(value: usize, label: &str) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("binary YDD {label} exceeds u32"))
}

#[inline]
fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
#[inline]
fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
#[inline]
fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}
#[inline]
fn push_vec3(out: &mut Vec<u8>, value: [f32; 3]) {
    for component in value {
        push_f32(out, component);
    }
}
fn write_u32(out: &mut [u8], offset: usize, value: u32) -> Result<(), String> {
    write_bytes(out, offset, &value.to_le_bytes())
}
fn write_u64(out: &mut [u8], offset: usize, value: u64) -> Result<(), String> {
    write_bytes(out, offset, &value.to_le_bytes())
}
fn write_f32(out: &mut [u8], offset: usize, value: f32) -> Result<(), String> {
    write_bytes(out, offset, &value.to_le_bytes())
}
fn write_vec3(out: &mut [u8], offset: usize, value: [f32; 3]) -> Result<(), String> {
    for (index, component) in value.into_iter().enumerate() {
        write_f32(out, offset + index * 4, component)?;
    }
    Ok(())
}
fn write_bytes(out: &mut [u8], offset: usize, value: &[u8]) -> Result<(), String> {
    let end = offset
        .checked_add(value.len())
        .ok_or("binary YDD write range overflow")?;
    let target = out
        .get_mut(offset..end)
        .ok_or("binary YDD write outside output")?;
    target.copy_from_slice(value);
    Ok(())
}

const _: () = {
    assert!(MESH_HEADER_LEN_V3 == 48);
    assert!(MESH_HEADER_LEN_V5 == 52);
    assert!(VERTEX_STRIDE == 32);
};
