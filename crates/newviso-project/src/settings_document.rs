//! Schema-driven authoring of the project runtime layer. No filesystem/backend access.
use crate::{
    ProjectConfiguration, ProjectRuntimeSettings, ResolvedRuntimeConfiguration,
    RuntimeConfigurationReport, RuntimeConfigurationRequest, RUNTIME_SETTINGS_SCHEMA_V1,
};
use newviso_config::{merge_json, setting_path, SettingOverride};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct RuntimeSettingField {
    pub path: String,
    pub section: String,
    pub label: String,
    pub schema: Value,
    pub live: bool,
}

#[derive(Clone, Debug)]
pub struct RuntimeSettingsDocument {
    configuration: ProjectConfiguration,
    request: RuntimeConfigurationRequest,
    shared: Value,
    shared_source: String,
    sources: BTreeMap<String, Value>,
    target: String,
    saved: Value,
    value: Value,
    effective: Value,
    applied: Value,
    report: RuntimeConfigurationReport,
}

impl RuntimeSettingsDocument {
    pub fn capture(
        configuration: &ProjectConfiguration,
        target: &str,
        shared: Value,
        shared_source: &str,
        request: &RuntimeConfigurationRequest,
        mut read: impl FnMut(&str) -> Result<Value, String>,
    ) -> Result<Self, String> {
        let mut sources = BTreeMap::new();
        let resolved = ResolvedRuntimeConfiguration::resolve(
            configuration,
            target,
            shared.clone(),
            shared_source,
            request,
            |path| {
                let value = read(path)?;
                sources.insert(path.to_owned(), value.clone());
                Ok(value)
            },
        )
        .map_err(|e| e.to_string())?;
        let value = sources
            .get(target)
            .ok_or("runtime layer was not captured")?
            .clone();
        let effective = serde_json::to_value(resolved.settings).map_err(|e| e.to_string())?;
        Ok(Self {
            configuration: configuration.clone(),
            request: request.clone(),
            shared,
            shared_source: shared_source.into(),
            sources,
            target: target.into(),
            saved: value.clone(),
            value,
            applied: effective.clone(),
            effective,
            report: resolved.report,
        })
    }
    pub fn target(&self) -> &str {
        &self.target
    }
    pub fn value(&self) -> &Value {
        &self.value
    }
    pub fn effective(&self) -> &Value {
        &self.effective
    }
    pub fn report(&self) -> &RuntimeConfigurationReport {
        &self.report
    }
    pub fn is_dirty(&self) -> bool {
        self.value != self.saved
    }
    pub fn mark_saved(&mut self) {
        self.saved = self.value.clone();
    }
    pub fn mark_applied(&mut self) {
        for (key, schema) in ProjectRuntimeSettings::schema()["properties"]
            .as_object()
            .expect("schema")
        {
            if schema["x-newviso-change-timing"] == "live" {
                self.applied[key] = self.effective[key].clone();
            }
        }
    }
    pub fn revert(&mut self) -> Result<(), String> {
        self.replace(self.saved.clone())
    }
    pub fn serialized(&self) -> String {
        serde_json::to_string_pretty(&self.value).expect("settings JSON") + "\n"
    }
    pub fn captured_source(&self, path: &str) -> Option<&Value> {
        self.sources.get(path)
    }
    pub fn fields(&self) -> Vec<RuntimeSettingField> {
        let mut fields = Vec::new();
        for (section, schema) in ProjectRuntimeSettings::schema()["properties"]
            .as_object()
            .expect("settings schema")
        {
            if section == "schema" {
                continue;
            }
            let live = schema["x-newviso-change-timing"] == "live";
            if let Some(props) = schema["properties"].as_object() {
                for (key, prop) in props {
                    fields.push(RuntimeSettingField {
                        path: format!("/{section}/{key}"),
                        section: section.clone(),
                        label: key.replace('_', " "),
                        schema: prop.clone(),
                        live,
                    });
                }
            } else {
                fields.push(RuntimeSettingField {
                    path: format!("/{section}"),
                    section: section.clone(),
                    label: section.replace('_', " "),
                    schema: schema.clone(),
                    live,
                });
            }
        }
        fields
    }
    pub fn set(&mut self, path: &str, value: Value) -> Result<(), String> {
        let field = self
            .fields()
            .into_iter()
            .find(|f| f.path == path)
            .ok_or("unregistered settings property")?;
        validate_type(&value, &field.schema)?;
        // A higher-priority layer must not hide an invalid native numeric value.
        let mut unmasked = self.effective.clone();
        *unmasked
            .pointer_mut(path)
            .ok_or("missing effective settings field")? = value.clone();
        ProjectRuntimeSettings::from_value(unmasked).map_err(|e| e.to_string())?;

        let mut candidate = self.value.clone();
        let parts = setting_path(path)?;
        let mut current = &mut candidate;
        for part in &parts[..parts.len() - 1] {
            if !current.is_object() {
                *current = json!({});
            }
            current = current
                .as_object_mut()
                .expect("object")
                .entry(part)
                .or_insert_with(|| json!({}));
        }
        if !current.is_object() {
            *current = json!({});
        }
        current
            .as_object_mut()
            .expect("object")
            .insert(parts.last().expect("field").clone(), value);
        self.replace(candidate)
    }
    pub fn reset(&mut self, path: &str) -> Result<(), String> {
        if !self.fields().iter().any(|f| f.path == path) {
            return Err("unregistered settings property".into());
        }
        fn remove(value: &mut Value, parts: &[String]) {
            let Some(object) = value.as_object_mut() else {
                return;
            };
            if parts.len() == 1 {
                object.remove(&parts[0]);
            } else if let Some(child) = object.get_mut(&parts[0]) {
                remove(child, &parts[1..]);
                if child.as_object().is_some_and(|v| v.is_empty()) {
                    object.remove(&parts[0]);
                }
            }
        }
        let mut candidate = self.value.clone();
        remove(&mut candidate, &setting_path(path)?);
        self.replace(candidate)
    }
    pub fn replace(&mut self, candidate: Value) -> Result<(), String> {
        if candidate["schema"] != RUNTIME_SETTINGS_SCHEMA_V1 {
            return Err("settings schema cannot change".into());
        }
        let resolved = ResolvedRuntimeConfiguration::resolve(
            &self.configuration,
            &self.target,
            self.shared.clone(),
            &self.shared_source,
            &self.request,
            |path| {
                if path == self.target {
                    Ok(candidate.clone())
                } else {
                    self.sources
                        .get(path)
                        .cloned()
                        .ok_or_else(|| format!("uncaptured configuration source '{path}'"))
                }
            },
        )
        .map_err(|e| e.to_string())?;
        let effective = serde_json::to_value(resolved.settings).map_err(|e| e.to_string())?;
        self.value = candidate;
        self.effective = effective;
        self.report = resolved.report;
        Ok(())
    }
    /// Only changed live fields, preserving unrelated edits made by scripts or editor guards.
    pub fn live_patch(&self) -> Result<(Value, BTreeMap<String, Value>), String> {
        let mut patch = json!({});
        let mut expected = BTreeMap::new();
        for field in self.fields().into_iter().filter(|f| f.live) {
            let current = self.effective.pointer(&field.path).unwrap_or(&Value::Null);
            let previous = self.applied.pointer(&field.path).unwrap_or(&Value::Null);
            if current != previous {
                // A JSON object uses merge semantics in runtime.configure. Replacing/removing
                // arbitrary project data requires a restart instead of promising false removal.
                if current.is_object() && !contains_previous_keys(current, previous) {
                    return Err(format!(
                        "{} removes object keys; save and restart to apply",
                        field.path
                    ));
                }
                let assignment = SettingOverride {
                    path: field.path.clone(),
                    value: current.clone(),
                };
                merge_json(&mut patch, &assignment.patch()?);
                expected.insert(field.path, previous.clone());
            }
        }
        Ok((patch, expected))
    }
    pub fn restart_pending(&self) -> bool {
        self.fields()
            .iter()
            .filter(|f| !f.live)
            .any(|f| self.effective.pointer(&f.path) != self.applied.pointer(&f.path))
    }
}

fn contains_previous_keys(next: &Value, previous: &Value) -> bool {
    previous.as_object().is_none_or(|object| {
        object.iter().all(|(key, value)| {
            next.get(key)
                .is_some_and(|child| !child.is_object() || contains_previous_keys(child, value))
        })
    })
}

fn validate_type(value: &Value, schema: &Value) -> Result<(), String> {
    let matches = |kind: &str| match kind {
        "null" => value.is_null(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => false,
    };
    let kind = &schema["type"];
    let valid = kind.as_str().is_some_and(matches)
        || kind
            .as_array()
            .is_some_and(|k| k.iter().filter_map(Value::as_str).any(matches));
    if !valid {
        return Err(format!("Expected {kind}"));
    }
    if let Some(n) = value.as_f64() {
        for (key, good) in [
            ("minimum", 0),
            ("maximum", 1),
            ("exclusiveMinimum", 2),
            ("exclusiveMaximum", 3),
        ] {
            if let Some(bound) = schema[key].as_f64() {
                if !n.is_finite()
                    || !match good {
                        0 => n >= bound,
                        1 => n <= bound,
                        2 => n > bound,
                        _ => n < bound,
                    }
                {
                    return Err(format!("Value violates {key} {bound}"));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc() -> RuntimeSettingsDocument {
        let mut c = ProjectConfiguration::default();
        c.profiles.insert(
            "fast".into(),
            crate::RuntimeProfile {
                inherits: vec![],
                layers: vec!["fast".into()],
            },
        );
        c.default_profile = Some("fast".into());
        RuntimeSettingsDocument::capture(&c, "runtime", crate::runtime_settings::tests::complete_base(),
            "base", &RuntimeConfigurationRequest::default(), |path| Ok(if path=="fast" {
                json!({"schema":RUNTIME_SETTINGS_SCHEMA_V1,"streaming":{"parallel_loads":8}})
            } else { json!({"schema":RUNTIME_SETTINGS_SCHEMA_V1,"window":{"width":1000},"variables":{"x":1}}) })).unwrap()
    }
    #[test]
    fn edits_keep_precedence_and_reset_restores_inheritance() {
        let mut d = doc();
        d.set("/streaming/parallel_loads", json!(6)).unwrap();
        assert_eq!(d.value()["streaming"]["parallel_loads"], 6);
        assert_eq!(d.effective()["streaming"]["parallel_loads"], 8);
        assert_eq!(
            d.report().field_sources["/streaming/parallel_loads"],
            "fast"
        );
        assert!(d.live_patch().unwrap().0.as_object().unwrap().is_empty());
        d.reset("/window/width").unwrap();
        assert_eq!(d.effective()["window"]["width"], 1280);
        assert!(d.restart_pending());
        d.revert().unwrap();
        assert!(!d.is_dirty());
    }
    #[test]
    fn invalid_or_shadowed_values_cannot_bypass_schema_and_live_patch_is_minimal() {
        let mut d = doc();
        let before = d.value().clone();
        for (path, value) in [
            ("/streaming/parallel_loads", json!(0)),
            ("/window/width", json!(false)),
            ("/camera/min_distance", json!(1000)),
            ("/schema", json!("other")),
        ] {
            assert!(d.set(path, value).is_err());
            assert_eq!(d.value(), &before);
        }
        d.set("/scheduling/scene_focus_observer", json!(false))
            .unwrap();
        let (patch, expected) = d.live_patch().unwrap();
        assert_eq!(patch, json!({"scheduling":{"scene_focus_observer":false}}));
        assert_eq!(expected["/scheduling/scene_focus_observer"], true);
        d.mark_applied();
        assert!(d.live_patch().unwrap().0.as_object().unwrap().is_empty());
        d.set("/variables", json!({})).unwrap();
        assert!(d.live_patch().is_err());
    }
}
