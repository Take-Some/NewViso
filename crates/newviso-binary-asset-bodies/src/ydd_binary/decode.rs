//! Strict bounded decoder for the binary YDD body layout.

use super::*;

pub(super) const BODY_HEADER_LEN: usize = 40;
pub(super) const ENTRY_RECORD_LEN: usize = 80;
pub(super) const MESH_HEADER_LEN_V2: usize = 40;
pub(super) const MESH_HEADER_LEN_V3: usize = 48;
pub(super) const MESH_HEADER_LEN_V5: usize = 52;
pub(super) const VERTEX_STRIDE: usize = 32;
pub(super) const SKIN_VERTEX_STRIDE_V3: usize = 24;
pub(super) const SKIN_VERTEX_STRIDE_V4: usize = 48;
pub(super) const MESH_FLAG_SKINNED: u32 = 0x0000_0001;
pub(super) const ENTRY_FLAG_SKIN_SOURCE_TO_MODEL: u32 = 0x0000_0001;

pub fn decode_ydd_binary_body(body: &[u8]) -> Result<YddBinaryDocument, String> {
    if body
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        == Some(b'{')
    {
        return Err(
            "JSON YDD geometry is unsupported; migrate the asset to newengine.ydd.binary_mesh"
                .to_owned(),
        );
    }
    if body.len() < BODY_HEADER_LEN {
        return Err(format!(
            "binary YDD body too small bytes={} expected>={BODY_HEADER_LEN}",
            body.len()
        ));
    }
    let version = read_u32(body, 0)?;
    if !matches!(
        version,
        YDD_BINARY_SCHEMA_VERSION_V2
            | YDD_BINARY_SCHEMA_VERSION_V3
            | YDD_BINARY_SCHEMA_VERSION_V4
            | YDD_BINARY_SCHEMA_VERSION
    ) {
        return Err(format!(
            "unsupported binary YDD schema version={version} supported=[{YDD_BINARY_SCHEMA_VERSION_V2},{YDD_BINARY_SCHEMA_VERSION_V3},{YDD_BINARY_SCHEMA_VERSION}]"
        ));
    }
    let mesh_header_len = if version >= YDD_BINARY_SCHEMA_VERSION {
        MESH_HEADER_LEN_V5
    } else if version >= YDD_BINARY_SCHEMA_VERSION_V3 {
        MESH_HEADER_LEN_V3
    } else {
        MESH_HEADER_LEN_V2
    };
    let entry_count = read_u32(body, 4)? as usize;
    if entry_count == 0 {
        return Err("binary YDD contains no entries".to_owned());
    }
    let table_offset = usize_from_u64(read_u64(body, 8)?, "entry table offset")?;
    let string_offset = usize_from_u64(read_u64(body, 16)?, "string table offset")?;
    let string_len = usize_from_u64(read_u64(body, 24)?, "string table length")?;
    let payload_floor = usize_from_u64(read_u64(body, 32)?, "payload floor")?;
    let table_len = entry_count
        .checked_mul(ENTRY_RECORD_LEN)
        .ok_or("binary YDD entry table size overflow")?;
    checked_slice(body, table_offset, table_len, "entry table")?;
    let strings = checked_slice(body, string_offset, string_len, "string table")?;
    if payload_floor > body.len() {
        return Err("binary YDD payload floor outside body".to_owned());
    }

    let mut entries = Vec::with_capacity(entry_count);
    for entry_index in 0..entry_count {
        let record = table_offset + entry_index * ENTRY_RECORD_LEN;
        let name = read_string(strings, read_u32(body, record + 8)?)?;
        let source_path = read_string(strings, read_u32(body, record + 12)?)?;
        let declared_mesh_count = read_u32(body, record + 16)? as usize;
        let declared_vertex_count = read_u32(body, record + 20)? as usize;
        let declared_index_count = read_u32(body, record + 24)? as usize;
        let properties_offset = read_u32(body, record + 32)?;
        let properties_ref = optional_string(strings, properties_offset)?;
        let bounds_min = read_vec3(body, record + 36)?;
        let bounds_max = read_vec3(body, record + 48)?;
        let payload_offset = usize_from_u64(read_u64(body, record + 64)?, "entry payload offset")?;
        let payload_len = usize_from_u64(read_u64(body, record + 72)?, "entry payload length")?;
        if payload_offset < payload_floor {
            return Err(format!(
                "binary YDD entry payload precedes payload table entry='{name}'"
            ));
        }
        let payload_end = payload_offset
            .checked_add(payload_len)
            .ok_or("binary YDD entry payload range overflow")?;
        if payload_end > body.len() {
            return Err(format!(
                "binary YDD entry payload outside body entry='{name}'"
            ));
        }
        let mesh_count = read_u32(body, payload_offset)? as usize;
        if mesh_count != declared_mesh_count {
            return Err(format!(
                "binary YDD mesh count mismatch entry='{name}' declared={declared_mesh_count} payload={mesh_count}"
            ));
        }
        let entry_flags = read_u32(body, payload_offset + 4)?;
        if version < YDD_BINARY_SCHEMA_VERSION_V3 && entry_flags != 0 {
            return Err(format!(
                "binary YDD v2 entry has non-zero payload flags entry='{name}' flags=0x{entry_flags:08x}"
            ));
        }
        if entry_flags & !ENTRY_FLAG_SKIN_SOURCE_TO_MODEL != 0 {
            return Err(format!(
                "binary YDD entry has unsupported payload flags entry='{name}' flags=0x{entry_flags:08x}"
            ));
        }
        let mut cursor = payload_offset + 8;
        let skin_source_to_model = if version >= YDD_BINARY_SCHEMA_VERSION_V3
            && entry_flags & ENTRY_FLAG_SKIN_SOURCE_TO_MODEL != 0
        {
            let matrix = read_mat4(body, cursor)?;
            cursor += 64;
            Some(matrix)
        } else {
            None
        };
        let mut meshes = Vec::with_capacity(mesh_count);
        let mut actual_vertex_count = 0usize;
        let mut actual_index_count = 0usize;
        for mesh_index in 0..mesh_count {
            checked_slice(body, cursor, mesh_header_len, "mesh header")?;
            if cursor + mesh_header_len > payload_end {
                return Err(format!(
                    "binary YDD mesh header outside entry entry='{name}' mesh={mesh_index}"
                ));
            }
            let mesh_name = read_string(strings, read_u32(body, cursor)?)?;
            let material_ref = optional_string(strings, read_u32(body, cursor + 4)?)?;
            let inline_material = if version >= YDD_BINARY_SCHEMA_VERSION {
                optional_string(strings, read_u32(body, cursor + 48)?)?
                    .map(|encoded| {
                        if encoded.len() > 65536 {
                            return Err(format!(
                                "YDD inline material exceeds 64 KiB mesh='{mesh_name}'"
                            ));
                        }
                        let value: serde_json::Value =
                            serde_json::from_str(&encoded).map_err(|error| {
                                format!(
                                    "YDD inline material JSON invalid mesh='{mesh_name}': {error}"
                                )
                            })?;
                        if !value.is_object() {
                            return Err(format!(
                                "YDD inline material must be an object mesh='{mesh_name}'"
                            ));
                        }
                        Ok(value)
                    })
                    .transpose()?
            } else {
                None
            };
            let vertex_count = read_u32(body, cursor + 8)? as usize;
            let index_count = read_u32(body, cursor + 12)? as usize;
            let mesh_bounds_min = read_vec3(body, cursor + 16)?;
            let mesh_bounds_max = read_vec3(body, cursor + 28)?;
            let (mesh_flags, declared_skin_stride) = if version >= YDD_BINARY_SCHEMA_VERSION_V3 {
                (
                    read_u32(body, cursor + 40)?,
                    read_u32(body, cursor + 44)? as usize,
                )
            } else {
                (0, 0)
            };
            let has_skin = mesh_flags & MESH_FLAG_SKINNED != 0;
            if mesh_flags & !MESH_FLAG_SKINNED != 0 {
                return Err(format!(
                    "binary YDD mesh has unsupported flags entry='{name}' mesh='{mesh_name}' flags=0x{mesh_flags:08x}"
                ));
            }
            let expected_skin_stride = match version {
                YDD_BINARY_SCHEMA_VERSION_V3 => SKIN_VERTEX_STRIDE_V3,
                YDD_BINARY_SCHEMA_VERSION_V4 | YDD_BINARY_SCHEMA_VERSION => SKIN_VERTEX_STRIDE_V4,
                _ => 0,
            };
            if has_skin && declared_skin_stride != expected_skin_stride {
                return Err(format!(
                    "binary YDD skin stride mismatch entry='{name}' mesh='{mesh_name}' stride={declared_skin_stride} expected={expected_skin_stride} version={version}"
                ));
            }
            if !has_skin && declared_skin_stride != 0 {
                return Err(format!(
                    "binary YDD unskinned mesh declares skin stride entry='{name}' mesh='{mesh_name}' stride={declared_skin_stride}"
                ));
            }
            cursor += mesh_header_len;
            if vertex_count == 0 || index_count == 0 {
                return Err(format!(
                    "binary YDD mesh is empty entry='{name}' mesh='{mesh_name}' vertices={vertex_count} indices={index_count}"
                ));
            }
            if vertex_count > 10_000_000 || index_count > 60_000_000 {
                return Err(format!(
                    "binary YDD mesh exceeds runtime limits entry='{name}' mesh='{mesh_name}' vertices={vertex_count} indices={index_count}"
                ));
            }
            let vertex_bytes = vertex_count
                .checked_mul(VERTEX_STRIDE)
                .ok_or("binary YDD vertex byte range overflow")?;
            let skin_bytes = if has_skin {
                vertex_count
                    .checked_mul(declared_skin_stride)
                    .ok_or("binary YDD skin byte range overflow")?
            } else {
                0
            };
            let index_bytes = index_count
                .checked_mul(4)
                .ok_or("binary YDD index byte range overflow")?;
            let mesh_end = cursor
                .checked_add(vertex_bytes)
                .and_then(|value| value.checked_add(skin_bytes))
                .and_then(|value| value.checked_add(index_bytes))
                .ok_or("binary YDD mesh range overflow")?;
            if mesh_end > payload_end {
                return Err(format!(
                    "binary YDD mesh payload outside entry entry='{name}' mesh='{mesh_name}'"
                ));
            }
            let mut vertices = Vec::with_capacity(vertex_count);
            for _ in 0..vertex_count {
                vertices.push(YddBinaryVertex {
                    position: read_vec3(body, cursor)?,
                    normal: read_vec3(body, cursor + 12)?,
                    uv0: read_vec2(body, cursor + 24)?,
                });
                cursor += VERTEX_STRIDE;
            }
            let skin = if has_skin {
                let mut skin = Vec::with_capacity(vertex_count);
                for _ in 0..vertex_count {
                    let joints = [
                        read_u16(body, cursor)?,
                        read_u16(body, cursor + 2)?,
                        read_u16(body, cursor + 4)?,
                        read_u16(body, cursor + 6)?,
                    ];
                    let weights = [
                        read_f32(body, cursor + 8)?,
                        read_f32(body, cursor + 12)?,
                        read_f32(body, cursor + 16)?,
                        read_f32(body, cursor + 20)?,
                    ];
                    let (joints_extra, weights_extra) =
                        if declared_skin_stride == SKIN_VERTEX_STRIDE_V4 {
                            (
                                [
                                    read_u16(body, cursor + 24)?,
                                    read_u16(body, cursor + 26)?,
                                    read_u16(body, cursor + 28)?,
                                    read_u16(body, cursor + 30)?,
                                ],
                                [
                                    read_f32(body, cursor + 32)?,
                                    read_f32(body, cursor + 36)?,
                                    read_f32(body, cursor + 40)?,
                                    read_f32(body, cursor + 44)?,
                                ],
                            )
                        } else {
                            ([0; 4], [0.0; 4])
                        };
                    if weights
                        .iter()
                        .chain(weights_extra.iter())
                        .any(|weight| !weight.is_finite() || *weight < 0.0)
                    {
                        return Err(format!(
                            "binary YDD skin contains invalid weight entry='{name}' mesh='{mesh_name}' weights={weights:?} extra={weights_extra:?}"
                        ));
                    }
                    let sum = weights.iter().chain(weights_extra.iter()).sum::<f32>();
                    if !sum.is_finite() || (sum - 1.0).abs() > 0.01 {
                        return Err(format!(
                            "binary YDD skin weights are not normalized entry='{name}' mesh='{mesh_name}' sum={sum}"
                        ));
                    }
                    skin.push(YddBinarySkinVertex {
                        joints,
                        weights,
                        joints_extra,
                        weights_extra,
                    });
                    cursor += declared_skin_stride;
                }
                Some(skin)
            } else {
                None
            };
            let mut indices = Vec::with_capacity(index_count);
            for _ in 0..index_count {
                let index = read_u32(body, cursor)?;
                cursor += 4;
                if index as usize >= vertex_count {
                    return Err(format!(
                        "binary YDD index out of bounds entry='{name}' mesh='{mesh_name}' index={index} vertices={vertex_count}"
                    ));
                }
                indices.push(index);
            }
            if indices.len() % 3 != 0 {
                return Err(format!(
                    "binary YDD index count is not triangular entry='{name}' mesh='{mesh_name}' indices={}",
                    indices.len()
                ));
            }
            actual_vertex_count = actual_vertex_count.saturating_add(vertex_count);
            actual_index_count = actual_index_count.saturating_add(index_count);
            meshes.push(YddBinaryMesh {
                name: mesh_name,
                material_ref,
                inline_material,
                bounds_min: mesh_bounds_min,
                bounds_max: mesh_bounds_max,
                vertices,
                skin,
                indices,
            });
        }
        if cursor != payload_end {
            return Err(format!(
                "binary YDD entry has trailing payload bytes entry='{name}' trailing={}",
                payload_end - cursor
            ));
        }
        if actual_vertex_count != declared_vertex_count
            || actual_index_count != declared_index_count
        {
            return Err(format!(
                "binary YDD entry count mismatch entry='{name}' vertices={actual_vertex_count}/{declared_vertex_count} indices={actual_index_count}/{declared_index_count}"
            ));
        }
        entries.push(YddBinaryEntry {
            name,
            source_path,
            properties_ref,
            bounds_min,
            bounds_max,
            skin_source_to_model,
            meshes,
        });
    }
    Ok(YddBinaryDocument { entries })
}

/// Decode only the requested entry payloads from a binary YDD body.
///
/// The YDD wire format already carries a fixed-size entry table and per-entry payload
/// offsets. We preserve the strict canonical decoder by constructing a bounded subset
/// body containing the original string table plus only the selected payload ranges,
/// then pass that subset through `decode_ydd_binary_body`. This avoids materializing
/// unrelated geometry while keeping one validation implementation authoritative.
pub fn decode_ydd_binary_entries(
    body: &[u8],
    selectors: &[String],
) -> Result<YddBinaryDocument, String> {
    if selectors.is_empty() {
        return Ok(YddBinaryDocument {
            entries: Vec::new(),
        });
    }
    if body
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        == Some(b'{')
    {
        return Err(
            "JSON YDD geometry is unsupported; migrate the asset to newengine.ydd.binary_mesh"
                .to_owned(),
        );
    }
    if body.len() < BODY_HEADER_LEN {
        return Err(format!(
            "binary YDD body too small bytes={} expected>={BODY_HEADER_LEN}",
            body.len()
        ));
    }
    let version = read_u32(body, 0)?;
    if !matches!(
        version,
        YDD_BINARY_SCHEMA_VERSION_V2
            | YDD_BINARY_SCHEMA_VERSION_V3
            | YDD_BINARY_SCHEMA_VERSION_V4
            | YDD_BINARY_SCHEMA_VERSION
    ) {
        return Err(format!(
            "unsupported binary YDD schema version={version} supported=[{YDD_BINARY_SCHEMA_VERSION_V2},{YDD_BINARY_SCHEMA_VERSION_V3},{YDD_BINARY_SCHEMA_VERSION}]"
        ));
    }
    let entry_count = read_u32(body, 4)? as usize;
    if entry_count == 0 {
        return Err("binary YDD contains no entries".to_owned());
    }
    let table_offset = usize_from_u64(read_u64(body, 8)?, "entry table offset")?;
    let string_offset = usize_from_u64(read_u64(body, 16)?, "string table offset")?;
    let string_len = usize_from_u64(read_u64(body, 24)?, "string table length")?;
    let payload_floor = usize_from_u64(read_u64(body, 32)?, "payload floor")?;
    let table_len = entry_count
        .checked_mul(ENTRY_RECORD_LEN)
        .ok_or("binary YDD entry table size overflow")?;
    checked_slice(body, table_offset, table_len, "entry table")?;
    let strings = checked_slice(body, string_offset, string_len, "string table")?;
    if payload_floor > body.len() {
        return Err("binary YDD payload floor outside body".to_owned());
    }

    let mut requested = Vec::<String>::new();
    for selector in selectors {
        let selector = selector.trim();
        if selector.is_empty() {
            return Err("binary YDD subset selector is empty".to_owned());
        }
        if !requested
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(selector))
        {
            requested.push(selector.to_owned());
        }
    }

    let mut selected = Vec::<(usize, usize, usize)>::with_capacity(requested.len());
    for selector in &requested {
        let mut found = None;
        for entry_index in 0..entry_count {
            let record = table_offset + entry_index * ENTRY_RECORD_LEN;
            let name = read_string(strings, read_u32(body, record + 8)?)?;
            if !name.eq_ignore_ascii_case(selector) {
                continue;
            }
            let payload_offset =
                usize_from_u64(read_u64(body, record + 64)?, "entry payload offset")?;
            let payload_len = usize_from_u64(read_u64(body, record + 72)?, "entry payload length")?;
            if payload_offset < payload_floor {
                return Err(format!(
                    "binary YDD entry payload precedes payload table entry='{name}'"
                ));
            }
            let payload_end = payload_offset
                .checked_add(payload_len)
                .ok_or("binary YDD entry payload range overflow")?;
            if payload_end > body.len() {
                return Err(format!(
                    "binary YDD entry payload outside body entry='{name}'"
                ));
            }
            found = Some((record, payload_offset, payload_len));
            break;
        }
        selected
            .push(found.ok_or_else(|| format!("binary YDD selector '{selector}' was not found"))?);
    }

    let subset_table_offset = BODY_HEADER_LEN;
    let subset_table_len = selected
        .len()
        .checked_mul(ENTRY_RECORD_LEN)
        .ok_or("binary YDD subset entry table size overflow")?;
    let subset_string_offset = subset_table_offset
        .checked_add(subset_table_len)
        .ok_or("binary YDD subset string table offset overflow")?;
    let subset_payload_floor = subset_string_offset
        .checked_add(string_len)
        .ok_or("binary YDD subset payload floor overflow")?;
    let subset_payload_len = selected.iter().try_fold(0usize, |total, (_, _, len)| {
        total
            .checked_add(*len)
            .ok_or("binary YDD subset payload length overflow")
    })?;
    let subset_len = subset_payload_floor
        .checked_add(subset_payload_len)
        .ok_or("binary YDD subset body length overflow")?;

    let mut subset = Vec::with_capacity(subset_len);
    subset.extend_from_slice(&version.to_le_bytes());
    subset.extend_from_slice(&(selected.len() as u32).to_le_bytes());
    subset.extend_from_slice(&(subset_table_offset as u64).to_le_bytes());
    subset.extend_from_slice(&(subset_string_offset as u64).to_le_bytes());
    subset.extend_from_slice(&(string_len as u64).to_le_bytes());
    subset.extend_from_slice(&(subset_payload_floor as u64).to_le_bytes());
    debug_assert_eq!(subset.len(), BODY_HEADER_LEN);

    for (record, _, _) in &selected {
        subset.extend_from_slice(checked_slice(
            body,
            *record,
            ENTRY_RECORD_LEN,
            "selected entry record",
        )?);
    }
    subset.extend_from_slice(strings);
    debug_assert_eq!(subset.len(), subset_payload_floor);

    let mut next_payload_offset = subset_payload_floor;
    for (index, (_, payload_offset, payload_len)) in selected.iter().enumerate() {
        let subset_record = subset_table_offset + index * ENTRY_RECORD_LEN;
        let payload_offset_field = subset_record + 64;
        subset[payload_offset_field..payload_offset_field + 8]
            .copy_from_slice(&(next_payload_offset as u64).to_le_bytes());
        subset.extend_from_slice(checked_slice(
            body,
            *payload_offset,
            *payload_len,
            "selected entry payload",
        )?);
        next_payload_offset = next_payload_offset
            .checked_add(*payload_len)
            .ok_or("binary YDD subset payload cursor overflow")?;
    }
    debug_assert_eq!(subset.len(), subset_len);

    decode_ydd_binary_body(&subset)
}

pub fn decode_ydd_binary_entry(body: &[u8], selector: &str) -> Result<YddBinaryEntry, String> {
    let selectors = [selector.to_owned()];
    let mut document = decode_ydd_binary_entries(body, &selectors)?;
    document
        .entries
        .pop()
        .ok_or_else(|| format!("binary YDD selector '{selector}' produced no entry"))
}

#[inline]
fn checked_slice<'a>(
    bytes: &'a [u8],
    offset: usize,
    len: usize,
    label: &str,
) -> Result<&'a [u8], String> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| format!("binary YDD {label} range overflow"))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| format!("binary YDD {label} outside body offset={offset} len={len}"))
}

#[inline]
fn usize_from_u64(value: u64, label: &str) -> Result<usize, String> {
    usize::try_from(value).map_err(|_| format!("binary YDD {label} exceeds platform usize"))
}

#[inline]
fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = checked_slice(bytes, offset, 2, "u16")?;
    Ok(u16::from_le_bytes(value.try_into().expect("u16 slice")))
}

#[inline]
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = checked_slice(bytes, offset, 4, "u32")?;
    Ok(u32::from_le_bytes(value.try_into().expect("u32 slice")))
}

#[inline]
fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let value = checked_slice(bytes, offset, 8, "u64")?;
    Ok(u64::from_le_bytes(value.try_into().expect("u64 slice")))
}

#[inline]
fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, String> {
    let value = checked_slice(bytes, offset, 4, "f32")?;
    let value = f32::from_le_bytes(value.try_into().expect("f32 slice"));
    if !value.is_finite() {
        return Err(format!(
            "binary YDD contains non-finite f32 offset={offset}"
        ));
    }
    Ok(value)
}

#[inline]
fn read_vec2(bytes: &[u8], offset: usize) -> Result<[f32; 2], String> {
    Ok([read_f32(bytes, offset)?, read_f32(bytes, offset + 4)?])
}

#[inline]
fn read_vec3(bytes: &[u8], offset: usize) -> Result<[f32; 3], String> {
    Ok([
        read_f32(bytes, offset)?,
        read_f32(bytes, offset + 4)?,
        read_f32(bytes, offset + 8)?,
    ])
}

#[inline]
fn read_mat4(bytes: &[u8], offset: usize) -> Result<[f32; 16], String> {
    let mut out = [0.0; 16];
    for (index, value) in out.iter_mut().enumerate() {
        *value = read_f32(bytes, offset + index * 4)?;
    }
    Ok(out)
}

fn optional_string(strings: &[u8], offset: u32) -> Result<Option<String>, String> {
    if offset == u32::MAX {
        Ok(None)
    } else {
        read_string(strings, offset).map(Some)
    }
}

fn read_string(strings: &[u8], offset: u32) -> Result<String, String> {
    let start = offset as usize;
    let tail = strings
        .get(start..)
        .ok_or_else(|| format!("binary YDD string offset outside table offset={offset}"))?;
    let length = tail
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| format!("binary YDD string is not terminated offset={offset}"))?;
    String::from_utf8(tail[..length].to_vec())
        .map_err(|error| format!("binary YDD string is not UTF-8: {error}"))
}
