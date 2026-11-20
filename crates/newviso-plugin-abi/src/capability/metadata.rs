fn json_string(value: &serde_json::Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn json_strings(value: &serde_json::Value, field: &str) -> Vec<String> {
    match value.get(field) {
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect(),
        Some(serde_json::Value::String(value)) if !value.trim().is_empty() => {
            vec![value.trim().to_owned()]
        }
        _ => Vec::new(),
    }
}

fn collect_json_tags(value: &serde_json::Value, field: &str, out: &mut RVec<SystemTagV2>) {
    for tag in json_strings(value, field) {
        push_unique_tag(out, &tag);
    }
}

fn push_unique_tag(out: &mut RVec<SystemTagV2>, tag: &str) {
    let tag = tag.trim();
    if tag.is_empty() || out.iter().any(|candidate| candidate.as_str() == tag) {
        return;
    }
    out.push(SystemTagV2::new(tag));
}

fn metadata_tag_slug(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '.'
            }
        })
        .collect::<String>()
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(".")
}
