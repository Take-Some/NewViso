use super::*;
use crate::{runtime_settings::tests::complete_base, RUNTIME_SETTINGS_SCHEMA_V1};
use serde_json::{json, Value};

fn patch(value: Value) -> Value {
    let mut value = value;
    value["schema"] = json!(RUNTIME_SETTINGS_SCHEMA_V1);
    value
}
fn request(profile: Option<&str>) -> RuntimeConfigurationRequest {
    RuntimeConfigurationRequest {
        platform: "windows".into(),
        profile: profile.map(str::to_owned),
        ..Default::default()
    }
}

#[test]
fn existing_projects_resolve_identically_without_configuration_declarations() {
    let project = patch(json!({"window":{"width":1440},"variables":{"project":"legacy"}}));
    let resolved = ResolvedRuntimeConfiguration::resolve(
        &ProjectConfiguration::default(),
        "runtime.json",
        complete_base(),
        "shared",
        &request(None),
        |_| Ok(project.clone()),
    )
    .unwrap();
    let legacy =
        crate::ProjectRuntimeSettings::from_base_and_override(complete_base(), project).unwrap();
    assert_eq!(
        serde_json::to_value(resolved.settings).unwrap(),
        serde_json::to_value(legacy).unwrap()
    );
    assert_eq!(
        resolved.report.field_sources["/camera/max_distance"],
        "shared"
    );
    assert_eq!(
        resolved.report.field_sources["/window/width"],
        "runtime.json"
    );
}

#[test]
fn full_precedence_diamond_inheritance_and_cli_are_deterministic() {
    let config:ProjectConfiguration=serde_json::from_value(json!({
        "defaults":["defaults"],"platforms":{"windows":["windows"]},
        "profiles":{
            "base":{"layers":["base"]},"a":{"inherits":["base"],"layers":["a"]},
            "b":{"inherits":["base"],"layers":["b"]},"final":{"inherits":["a","b"],"layers":["final"]}
        },"default_profile":"final","user_settings":"user"
    })).unwrap();
    let mut inputs = BTreeMap::new();
    for (i, name) in [
        "defaults", "project", "windows", "base", "a", "b", "final", "user", "cli-file",
    ]
    .into_iter()
    .enumerate()
    {
        inputs.insert(
            name.to_owned(),
            patch(json!({"variables":{"value":i,"array":[i]},"streaming":{"parallel_loads":i+1}})),
        );
    }
    let mut req = request(None);
    req.layers.push("cli-file".into());
    req.overrides
        .push(SettingOverride::parse("streaming.parallel_loads=12").unwrap());
    let mut reads = Vec::new();
    let resolved = ResolvedRuntimeConfiguration::resolve(
        &config,
        "project",
        complete_base(),
        "shared",
        &req,
        |p| {
            reads.push(p.to_owned());
            Ok(inputs[p].clone())
        },
    )
    .unwrap();
    assert_eq!(
        reads,
        ["defaults", "project", "windows", "base", "a", "b", "final", "user", "cli-file"]
    );
    assert_eq!(resolved.settings.streaming.parallel_loads, 12);
    assert_eq!(resolved.settings.variables["array"], json!([8]));
    assert_eq!(resolved.report.profile.as_deref(), Some("final"));
    assert_eq!(
        resolved.report.field_sources["/variables/value"],
        "cli-file"
    );
    assert!(resolved.report.field_sources["/streaming/parallel_loads"].starts_with("command_line:"));
}

#[test]
fn invalid_profiles_layers_and_values_never_silently_fall_back() {
    for config in [
        json!({"default_profile":"missing"}),
        json!({"profiles":{"a":{"inherits":["b"]}}}),
        json!({"profiles":{"a":{"inherits":["b"]},"b":{"inherits":["a"]}}}),
        json!({"defaults":["../outside.json"]}),
    ] {
        assert!(serde_json::from_value::<ProjectConfiguration>(config)
            .unwrap()
            .validate()
            .is_err());
    }
    let config = ProjectConfiguration::default();
    assert!(ResolvedRuntimeConfiguration::resolve(
        &config,
        "project",
        complete_base(),
        "shared",
        &request(Some("missing")),
        |_| Ok(patch(json!({})))
    )
    .is_err());
    let missing = ResolvedRuntimeConfiguration::resolve(
        &config,
        "missing.json",
        complete_base(),
        "shared",
        &request(None),
        |_| Err("not found".into()),
    )
    .unwrap_err()
    .to_string();
    assert!(missing.contains("missing.json") && missing.contains("not found"));
    for input in [
        json!({"schema":"wrong"}),
        patch(json!({"streaming":{"parallel_loads":0}})),
        patch(json!({"camera":{"max_distnce":5}})),
    ] {
        assert!(ResolvedRuntimeConfiguration::resolve(
            &config,
            "project",
            complete_base(),
            "shared",
            &request(None),
            |_| Ok(input.clone())
        )
        .is_err());
    }
}

#[test]
fn array_values_are_atomic_even_inside_open_project_variables() {
    let mut req = request(None);
    req.overrides
        .push(SettingOverride::parse("variables.points.0=3").unwrap());
    let error = ResolvedRuntimeConfiguration::resolve(
        &ProjectConfiguration::default(),
        "project",
        complete_base(),
        "shared",
        &req,
        |_| Ok(patch(json!({"variables":{"points":[1,2]}}))),
    )
    .unwrap_err();
    assert!(error.to_string().contains("traverses an array"));
}
