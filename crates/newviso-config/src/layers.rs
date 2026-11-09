use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Objects merge recursively. Arrays, scalars and null replace literally.
pub fn merge_json(target: &mut Value, patch: &Value) {
    if let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            match target.get_mut(key) {
                Some(current) => merge_json(current, value),
                None => {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
    } else {
        *target = patch.clone();
    }
}

fn pointer_part(part: &str) -> String {
    part.replace('~', "~0").replace('/', "~1")
}

/// A JSON Pointer, or a dotted convenience path. Array indexing is deliberately
/// excluded: an authored array is one atomic configuration value.
pub fn setting_path(path: &str) -> Result<Vec<String>, String> {
    let parts = if let Some(path) = path.strip_prefix('/') {
        path.split('/')
            .map(|part| {
                let mut value = String::new();
                let mut chars = part.chars();
                while let Some(c) = chars.next() {
                    if c != '~' {
                        value.push(c);
                        continue;
                    }
                    match chars.next() {
                        Some('0') => value.push('~'),
                        Some('1') => value.push('/'),
                        _ => return Err("invalid JSON Pointer escape".to_owned()),
                    }
                }
                Ok(value)
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        path.split('.').map(str::to_owned).collect()
    };
    if parts
        .iter()
        .any(|p| p.is_empty() || p.chars().any(char::is_control))
    {
        return Err("setting path contains an empty or control-character segment".to_owned());
    }
    Ok(parts)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingOverride {
    pub path: String,
    pub value: Value,
}

impl SettingOverride {
    pub fn parse(assignment: &str) -> Result<Self, String> {
        let (path, raw) = assignment
            .split_once('=')
            .ok_or("runtime override expects path=value")?;
        let path = path.trim().to_owned();
        setting_path(&path)?;
        let value = match serde_json::from_str(raw) {
            Ok(value) => value,
            Err(error) if raw.trim_start().starts_with(['{', '[', '"']) => {
                return Err(format!(
                    "invalid JSON for runtime setting '{path}': {error}"
                ));
            }
            Err(_) => Value::String(raw.to_owned()),
        };
        Ok(Self { path, value })
    }

    pub fn patch(&self) -> Result<Value, String> {
        let mut value = self.value.clone();
        for part in setting_path(&self.path)?.into_iter().rev() {
            value = Value::Object([(part, value)].into_iter().collect());
        }
        Ok(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigurationLayer {
    pub source: String,
    pub kind: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConfigurationStack {
    pub value: Value,
    pub layers: Vec<ConfigurationLayer>,
    /// RFC 6901 JSON Pointer -> last layer that wrote the leaf value.
    pub field_sources: BTreeMap<String, String>,
}

impl ConfigurationStack {
    pub fn apply(&mut self, source: &str, kind: &str, patch: &Value) {
        fn visit(
            target: &mut Value,
            patch: &Value,
            path: &str,
            source: &str,
            origins: &mut BTreeMap<String, String>,
        ) {
            let objects = target.is_object() && patch.is_object();
            if !objects {
                origins.remove(path);
                let prefix = format!("{path}/");
                let stale: Vec<_> = origins
                    .range(prefix.clone()..)
                    .take_while(|(key, _)| key.starts_with(&prefix))
                    .map(|(key, _)| key.clone())
                    .collect();
                for key in stale {
                    origins.remove(&key);
                }
                *target = if patch.is_object() {
                    serde_json::json!({})
                } else {
                    patch.clone()
                };
            }
            if let Some(object) = patch.as_object() {
                if object.is_empty() {
                    if !objects {
                        origins.insert(path.to_owned(), source.to_owned());
                    }
                } else {
                    origins.remove(path);
                    for (key, value) in object {
                        let next = format!("{path}/{}", pointer_part(key));
                        let child = target
                            .as_object_mut()
                            .expect("object initialized")
                            .entry(key.clone())
                            .or_insert(Value::Null);
                        visit(child, value, &next, source, origins);
                    }
                }
            } else {
                origins.insert(path.to_owned(), source.to_owned());
            }
        }
        visit(&mut self.value, patch, "", source, &mut self.field_sources);
        self.layers.push(ConfigurationLayer {
            source: source.to_owned(),
            kind: kind.to_owned(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn traces_only_surviving_values_and_preserves_explicit_null() {
        let mut stack = ConfigurationStack::default();
        stack.apply("base", "engine", &json!({"a":{"x":1,"y":2},"b":[1,2]}));
        stack.apply("profile", "profile", &json!({"a":{"x":3},"b":null}));
        assert_eq!(stack.value, json!({"a":{"x":3,"y":2},"b":null}));
        assert_eq!(stack.field_sources["/a/y"], "base");
        assert_eq!(stack.field_sources["/a/x"], "profile");
        stack.apply("user", "user", &json!({"a":false}));
        assert!(!stack.field_sources.contains_key("/a/y"));
        stack.apply("cli", "cli", &json!({"a":{}}));
        assert_eq!(stack.field_sources["/a"], "cli");
        let mut plain = json!({"a":false,"b":null});
        merge_json(&mut plain, &json!({"a":{}}));
        assert_eq!(plain, stack.value);
    }

    #[test]
    fn pointer_and_dotted_overrides_have_unambiguous_json_values() {
        assert_eq!(
            SettingOverride::parse("streaming.parallel_loads=8")
                .unwrap()
                .patch()
                .unwrap(),
            json!({"streaming":{"parallel_loads":8}})
        );
        assert_eq!(
            SettingOverride::parse("/variables/a~1b~0c=true")
                .unwrap()
                .patch()
                .unwrap(),
            json!({"variables":{"a/b~c":true}})
        );
        assert_eq!(
            SettingOverride::parse("window.title=Test scene")
                .unwrap()
                .value,
            json!("Test scene")
        );
        for bad in ["a..b=1", "/bad~2=1", "=1", "a={broken", "a=[1,"] {
            assert!(SettingOverride::parse(bad).is_err(), "accepted {bad}");
        }
    }
}
