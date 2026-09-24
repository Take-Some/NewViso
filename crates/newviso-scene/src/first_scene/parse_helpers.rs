use super::*;

pub(super) fn read_f32(object: &Value, key: &str, default: f32) -> Result<f32, String> {
    let Some(value) = object.get(key) else {
        return Ok(default);
    };
    let value = value
        .as_f64()
        .ok_or_else(|| format!("'{key}' must be numeric"))? as f32;
    if !value.is_finite() {
        return Err(format!("'{key}' must be finite"));
    }
    Ok(value)
}
pub(super) fn read_vec3(object: &Value, key: &str, default: Vec3) -> Result<Vec3, String> {
    let Some(value) = object.get(key) else {
        return Ok(default);
    };
    let array = value
        .as_array()
        .ok_or_else(|| format!("'{key}' must be a 3-element array"))?;
    if array.len() != 3 {
        return Err(format!("'{key}' must contain exactly 3 elements"));
    }

    let mut out = [0.0_f32; 3];
    for (index, item) in array.iter().enumerate() {
        let number =
            item.as_f64()
                .ok_or_else(|| format!("'{key}[{index}]' must be numeric"))? as f32;
        if !number.is_finite() {
            return Err(format!("'{key}[{index}]' must be finite"));
        }
        out[index] = number;
    }
    Ok(Vec3::new(out[0], out[1], out[2]))
}
pub(super) fn read_color4(
    object: &Value,
    key: &str,
    default: [f32; 4],
) -> Result<[f32; 4], String> {
    let Some(value) = object.get(key) else {
        return Ok(default);
    };
    let array = value
        .as_array()
        .ok_or_else(|| format!("'{key}' must be a 4-element array"))?;
    if array.len() != 4 {
        return Err(format!("'{key}' must contain exactly 4 elements"));
    }

    let mut result = [0.0; 4];
    for (index, item) in array.iter().enumerate() {
        let number =
            item.as_f64()
                .ok_or_else(|| format!("'{key}[{index}]' must be numeric"))? as f32;
        if !number.is_finite() {
            return Err(format!("'{key}[{index}]' must be finite"));
        }
        result[index] = number;
    }
    Ok(result)
}
pub(super) fn read_visibility_mask(record: &Value) -> VisibilityMask {
    let mut mask = VisibilityMask::default();
    let Some(visibility) = record.get("visibility").and_then(Value::as_object) else {
        return mask;
    };
    for (channel, value) in visibility {
        if let Some(visible) = value.as_bool() {
            mask.set(channel, visible);
        }
    }
    mask
}

pub(super) fn read_lod_policy(record: &Value) -> Result<SceneLodPolicy, String> {
    let Some(lod) = record.get("lod") else {
        return Ok(SceneLodPolicy::default());
    };
    let lod = lod
        .as_object()
        .ok_or_else(|| "'lod' must be an object".to_owned())?;

    let read = |key: &str, default: f32| -> Result<f32, String> {
        let Some(value) = lod.get(key) else {
            return Ok(default);
        };
        let value = value
            .as_f64()
            .ok_or_else(|| format!("lod.{key} must be numeric"))? as f32;
        if !value.is_finite() || value < 0.0 {
            return Err(format!("lod.{key} must be a finite non-negative number"));
        }
        Ok(value)
    };

    let visible_distance = read("visible_distance", f32::INFINITY)?;
    let stream_distance = read("stream_distance", f32::INFINITY)?;
    let fade_range = read("fade_range", 0.0)?;
    if visible_distance.is_finite()
        && stream_distance.is_finite()
        && stream_distance < visible_distance
    {
        return Err("lod.stream_distance must be >= lod.visible_distance".to_owned());
    }
    Ok(SceneLodPolicy {
        visible_distance,
        stream_distance,
        fade_range,
    })
}
pub(super) fn read_parent_id(record: &Value) -> Option<u64> {
    record
        .get("parent_id")
        .and_then(Value::as_u64)
        .or_else(|| record.pointer("/parent/stable_id").and_then(Value::as_u64))
        .or_else(|| record.get("parent").and_then(Value::as_u64))
}
