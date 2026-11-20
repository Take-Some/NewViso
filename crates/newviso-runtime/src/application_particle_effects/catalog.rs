use super::*;

pub(super) fn behaviour_range3(
    particle: &Value,
    kind: &str,
    min_key: &str,
    max_key: &str,
    rng: &mut Rng,
) -> Option<[f32; 3]> {
    let value = behaviour(particle, kind)?;
    let min = keyframe_value_at(value.get(min_key), false)?;
    let max = keyframe_value_at(value.get(max_key), false).unwrap_or(min);
    Some(std::array::from_fn(|index| {
        rng.range(min[index], max[index])
    }))
}

pub(super) fn behaviour<'a>(particle: &'a Value, kind: &str) -> Option<&'a Value> {
    particle
        .get("behaviours")?
        .as_array()?
        .iter()
        .find(|value| value.get("type").and_then(Value::as_str) == Some(kind))
}

pub(super) fn emitter_property<'a>(emitter: &'a Value, name: &str) -> Option<&'a Value> {
    emitter
        .get("keyframe_properties")?
        .as_array()?
        .iter()
        .find(|value| value.get("name").and_then(Value::as_str) == Some(name))
}
pub(super) fn emitter_range(emitter: &Value, name: &str) -> Option<(f32, f32)> {
    emitter_range_at(emitter, name, 0.0)
}
pub(super) fn emitter_range_at(emitter: &Value, name: &str, phase: f32) -> Option<(f32, f32)> {
    let value = property_value(emitter_property(emitter, name), phase)?;
    Some((value[0], value[1]))
}

pub(super) fn keyframe_value_at(property: Option<&Value>, last: bool) -> Option<[f32; 4]> {
    let values = property?.get("keyframes")?.as_array()?;
    let keyframe = if last {
        values.last()?
    } else {
        values.first()?
    };
    array4(keyframe.get("value"))
}

pub(super) fn particle_texture_names(particle: &Value) -> Vec<String> {
    particle
        .get("shader_vars")
        .and_then(Value::as_array)
        .map(|vars| {
            vars.iter()
                .filter(|value| value.get("type").and_then(Value::as_str) == Some("texture"))
                .filter_map(|value| value.get("texture_name").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn named_map(value: Option<&Value>) -> BTreeMap<String, &Value> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.trim();
                    (!name.is_empty()).then_some((name.to_owned(), item))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn find_named<'a>(value: Option<&'a Value>, name: &str) -> Option<&'a Value> {
    value?.as_array()?.iter().find(|item| {
        item.get("name")
            .and_then(Value::as_str)
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
    })
}

pub(super) fn decode_particle_dictionary_wire(bytes: &[u8]) -> Result<Value, String> {
    if bytes.starts_with(b"{") {
        return serde_json::from_slice(bytes)
            .map_err(|error| format!("particle dictionary JSON decode failed: {error}"));
    }
    if bytes.len() < 20 || &bytes[..4] != PARTICLE_WIRE_MAGIC {
        return Err(format!(
            "expected NVPT semantic wire, got {} bytes magic={:?}",
            bytes.len(),
            bytes.get(..4)
        ));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != 1 {
        return Err(format!("unsupported NVPT semantic version {version}"));
    }
    let meta_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let payload_len = usize::try_from(u64::from_le_bytes(bytes[12..20].try_into().unwrap()))
        .map_err(|_| "NVPT payload length exceeds usize".to_owned())?;
    let meta_end = 20usize
        .checked_add(meta_len)
        .ok_or_else(|| "NVPT metadata range overflow".to_owned())?;
    let total = meta_end
        .checked_add(payload_len)
        .ok_or_else(|| "NVPT payload range overflow".to_owned())?;
    if total != bytes.len() {
        return Err(format!(
            "NVPT size mismatch declared={total} actual={}",
            bytes.len()
        ));
    }
    serde_json::from_slice(&bytes[20..meta_end])
        .map_err(|error| format!("NVPT metadata JSON decode failed: {error}"))
}

pub(super) fn split_asset_ref(value: &str) -> Result<(&str, &str), String> {
    let (path, entry) = value
        .rsplit_once('@')
        .ok_or_else(|| format!("particle effect asset_ref requires @effect: '{value}'"))?;
    let path = path.trim();
    let entry = entry.trim();
    if path.is_empty() || entry.is_empty() {
        return Err(format!("invalid particle effect asset_ref '{value}'"));
    }
    Ok((path, entry))
}

pub(super) fn required_string<'a>(
    value: &'a Value,
    key: &str,
    index: usize,
) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("script command[{index}] requires non-empty string '{key}'"))
}

pub(super) fn optional_number(
    value: &Value,
    key: &str,
    index: usize,
) -> Result<Option<f32>, String> {
    let Some(raw) = value.get(key) else {
        return Ok(None);
    };
    let result = raw
        .as_f64()
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("script command[{index}] '{key}' must be finite numeric"))?;
    Ok(Some(result))
}

pub(super) fn vec3(value: &Value, key: &str, index: usize) -> Result<[f32; 3], String> {
    array3(value.get(key))
        .ok_or_else(|| format!("script command[{index}] '{key}' must contain three numbers"))
}

pub(super) fn optional_vec3(
    value: &Value,
    key: &str,
    index: usize,
) -> Result<Option<[f32; 3]>, String> {
    if value.get(key).is_none() {
        return Ok(None);
    }
    vec3(value, key, index).map(Some)
}

pub(super) fn optional_vec4(
    value: &Value,
    key: &str,
    index: usize,
) -> Result<Option<[f32; 4]>, String> {
    if value.get(key).is_none() {
        return Ok(None);
    }
    array4(value.get(key))
        .ok_or_else(|| format!("script command[{index}] '{key}' must contain four numbers"))
        .map(Some)
}

pub(super) fn array3(value: Option<&Value>) -> Option<[f32; 3]> {
    let values = value?.as_array()?;
    if values.len() != 3 {
        return None;
    }
    Some([
        values[0].as_f64()? as f32,
        values[1].as_f64()? as f32,
        values[2].as_f64()? as f32,
    ])
}

pub(super) fn array4(value: Option<&Value>) -> Option<[f32; 4]> {
    let values = value?.as_array()?;
    if values.len() != 4 {
        return None;
    }
    Some([
        values[0].as_f64()? as f32,
        values[1].as_f64()? as f32,
        values[2].as_f64()? as f32,
        values[3].as_f64()? as f32,
    ])
}

pub(super) fn number(value: Option<&Value>) -> Option<f32> {
    value?
        .as_f64()
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
}

pub(super) fn positive_number(value: Option<&Value>) -> Option<f32> {
    number(value).filter(|value| *value > 0.0)
}

pub(super) fn basis_from_forward(direction: [f32; 3]) -> Result<Basis, String> {
    let forward =
        normalize(direction).ok_or_else(|| "particle effect direction is zero".to_owned())?;
    let world_up = [0.0, 1.0, 0.0];
    let right = normalize(cross(forward, world_up))
        .or_else(|| normalize(cross(forward, [1.0, 0.0, 0.0])))
        .ok_or_else(|| "particle effect could not derive orientation basis".to_owned())?;
    let up = normalize(cross(right, forward)).unwrap_or(world_up);
    Ok(Basis { forward, right, up })
}

pub(super) fn local_to_world(local: [f32; 3], basis: Basis, scale: f32) -> [f32; 3] {
    add(
        add(
            mul(basis.forward, local[0] * scale),
            mul(basis.right, local[1] * scale),
        ),
        mul(basis.up, local[2] * scale),
    )
}

pub(super) fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub(super) fn mul(a: [f32; 3], scalar: f32) -> [f32; 3] {
    [a[0] * scalar, a[1] * scalar, a[2] * scalar]
}
pub(super) fn mul4(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|index| a[index] * b[index])
}
pub(super) fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(super) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(super) fn normalize(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(value, value).sqrt();
    if !length.is_finite() || length <= 1.0e-6 {
        None
    } else {
        Some(mul(value, 1.0 / length))
    }
}

pub(super) fn joaat(value: &str) -> u32 {
    let mut hash = 0u32;
    for byte in value.to_ascii_lowercase().bytes() {
        hash = hash.wrapping_add(byte as u32);
        hash = hash.wrapping_add(hash << 10);
        hash ^= hash >> 6;
    }
    hash = hash.wrapping_add(hash << 3);
    hash ^= hash >> 11;
    hash.wrapping_add(hash << 15)
}
