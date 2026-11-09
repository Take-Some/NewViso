use serde_json::Value;
use std::sync::OnceLock;

pub(crate) fn is_live_section(key: &str) -> bool {
    matches!(
        key,
        "camera" | "streaming" | "scripting" | "scheduling" | "variables"
    )
}

impl crate::ProjectRuntimeSettings {
    /// Editor/tool metadata, with no artistic presets or engine default values.
    /// Native decoding/validation remains authoritative for cross-field rules.
    pub fn schema() -> &'static Value {
        static SCHEMA: OnceLock<Value> = OnceLock::new();
        SCHEMA.get_or_init(|| {
            let mut schema: Value =
                serde_json::from_str(include_str!("assets/runtime-settings.schema.json"))
                    .expect("built-in runtime settings schema");
            for (key, section) in schema["properties"]
                .as_object_mut()
                .expect("schema properties")
            {
                section["x-newviso-change-timing"] = Value::String(
                    if is_live_section(key) {
                        "live"
                    } else {
                        "restart"
                    }
                    .into(),
                );
            }
            schema
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_covers_the_actual_serialized_settings_and_live_policy() {
        let base = crate::runtime_settings::tests::complete_base();
        let decoded = crate::ProjectRuntimeSettings::from_value(base).unwrap();
        let actual = serde_json::to_value(decoded).unwrap();
        fn check(value: &Value, schema: &Value, path: &str) {
            if path == "/variables" || path == "/startup_commands" {
                return;
            }
            if let Some(object) = value.as_object() {
                let props = schema["properties"].as_object().expect("object schema");
                assert_eq!(object.len(), props.len(), "schema drift at {path}");
                for (key, child) in object {
                    check(
                        child,
                        props
                            .get(key)
                            .unwrap_or_else(|| panic!("missing {path}/{key}")),
                        &format!("{path}/{key}"),
                    );
                }
            }
        }
        check(&actual, crate::ProjectRuntimeSettings::schema(), "");
        for (key, section) in crate::ProjectRuntimeSettings::schema()["properties"]
            .as_object()
            .unwrap()
        {
            assert_eq!(
                section["x-newviso-change-timing"],
                if is_live_section(key) {
                    "live"
                } else {
                    "restart"
                }
            );
        }
    }
}
