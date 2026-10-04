use super::*;

pub(super) fn command_number(value: &Value, key: &str, index: usize) -> Result<f32, String> {
    let number = value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("script command item[{index}] '{key}' must be numeric"))?;
    let number = number as f32;
    if !number.is_finite() {
        return Err(format!(
            "script command item[{index}] '{key}' must be finite"
        ));
    }
    Ok(number)
}

pub(super) fn command_f64(value: &Value, key: &str, index: usize) -> Result<f64, String> {
    let number = value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("script command item[{index}] '{key}' must be numeric"))?;
    if !number.is_finite() {
        return Err(format!(
            "script command item[{index}] '{key}' must be finite"
        ));
    }
    Ok(number)
}

pub(super) fn command_vector<const N: usize>(
    value: &Value,
    key: &str,
    index: usize,
) -> Result<[f32; N], String> {
    let array = value.get(key).and_then(Value::as_array).ok_or_else(|| {
        format!("script command item[{index}] '{key}' must be an array of {N} numbers")
    })?;
    if array.len() != N {
        return Err(format!(
            "script command item[{index}] '{key}' must contain exactly {N} numbers"
        ));
    }
    let mut out = [0.0; N];
    for (slot, item) in out.iter_mut().zip(array) {
        let number = item
            .as_f64()
            .ok_or_else(|| format!("script command item[{index}] '{key}' contains a non-number"))?
            as f32;
        if !number.is_finite() {
            return Err(format!(
                "script command item[{index}] '{key}' contains a non-finite number"
            ));
        }
        *slot = number;
    }
    Ok(out)
}

pub(super) fn command_vec3(value: &Value, key: &str, index: usize) -> Result<[f32; 3], String> {
    command_vector(value, key, index)
}

pub(super) fn command_optional_vec3(
    value: &Value,
    key: &str,
    index: usize,
) -> Result<Option<[f32; 3]>, String> {
    if value.get(key).is_none() || value.get(key).is_some_and(Value::is_null) {
        return Ok(None);
    }
    command_vec3(value, key, index).map(Some)
}

pub(super) fn command_vec4(value: &Value, key: &str, index: usize) -> Result<[f32; 4], String> {
    command_vector(value, key, index)
}

pub(super) fn command_u32(value: &Value, key: &str, index: usize) -> Result<u32, String> {
    let number = value.get(key).and_then(Value::as_u64).ok_or_else(|| {
        format!("script command item[{index}] '{key}' must be an unsigned integer")
    })?;
    u32::try_from(number)
        .map_err(|_| format!("script command item[{index}] '{key}' is out of u32 range"))
}

pub(super) fn command_u64(value: &Value, key: &str, index: usize) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("script command item[{index}] '{key}' must be an unsigned integer"))
}

pub(super) fn command_i32(value: &Value, key: &str, index: usize) -> Result<i32, String> {
    let number = value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("script command item[{index}] '{key}' must be an integer"))?;
    i32::try_from(number)
        .map_err(|_| format!("script command item[{index}] '{key}' is out of i32 range"))
}

pub(super) fn command_strings(
    value: &Value,
    key: &str,
    index: usize,
) -> Result<Vec<String>, String> {
    let values = value.get(key).and_then(Value::as_array).ok_or_else(|| {
        format!("script command item[{index}] '{key}' must be an array of strings")
    })?;
    values
        .iter()
        .enumerate()
        .map(|(item_index, item)| {
            item.as_str().map(str::to_owned).ok_or_else(|| {
                format!("script command item[{index}] '{key}[{item_index}]' must be a string")
            })
        })
        .collect()
}

pub(super) fn command_float_map(
    value: &Value,
    key: &str,
    index: usize,
) -> Result<BTreeMap<String, f32>, String> {
    let Some(map) = value.get(key) else {
        return Ok(BTreeMap::new());
    };
    let map = map.as_object().ok_or_else(|| {
        format!("script command item[{index}] '{key}' must be an object of numeric values")
    })?;
    let mut out = BTreeMap::new();
    for (name, value) in map {
        let number = value
            .as_f64()
            .ok_or_else(|| format!("script command item[{index}] '{key}.{name}' must be numeric"))?
            as f32;
        if !number.is_finite() {
            return Err(format!(
                "script command item[{index}] '{key}.{name}' must be finite"
            ));
        }
        out.insert(name.clone(), number);
    }
    Ok(out)
}
