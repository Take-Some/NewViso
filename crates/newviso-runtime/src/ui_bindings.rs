use super::*;

pub(super) fn materialize_ui_template(
    template: &Value,
    bindings: &BTreeMap<String, Value>,
) -> Value {
    match template {
        Value::String(value) => Value::String(apply_string_bindings(value, bindings)),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| materialize_ui_template(value, bindings))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), materialize_ui_template(value, bindings)))
                .collect(),
        ),
        other => other.clone(),
    }
}

pub(super) fn apply_string_bindings(source: &str, bindings: &BTreeMap<String, Value>) -> String {
    let mut output = source.to_owned();
    for (key, value) in bindings {
        let token = format!("{{{{{key}}}}}");
        if output.contains(&token) {
            output = output.replace(&token, &binding_text(value));
        }
    }
    output
}

pub(super) fn binding_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}
