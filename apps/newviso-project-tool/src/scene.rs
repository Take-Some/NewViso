use crate::assets::DirectoryAssets;
use newviso_project::ResolvedProject;
use newviso_scene_document::{SceneDocument, SceneEdit, SceneTypeRegistry};
use serde_json::{json, Value};
use std::path::Path;

pub fn execute(
    command: &str,
    project: &ResolvedProject,
    assets: &mut DirectoryAssets,
    logical: Option<&str>,
    edits: Option<&Path>,
    entity: usize,
) -> Result<Value, String> {
    let logical = logical.unwrap_or(&project.manifest.files.scene);
    let value = assets.json(logical)?;
    let path = assets
        .bindings
        .get(logical)
        .ok_or("scene has no filesystem binding")?
        .clone();
    let original = std::fs::read(&path).map_err(|e| e.to_string())?;
    let registry = SceneTypeRegistry::builtin();
    let mut document = SceneDocument::new(value, registry)?;
    if command == "scene-edit" {
        let canonical = path.canonicalize().map_err(|e| e.to_string())?;
        if !canonical.starts_with(project.root.canonicalize().map_err(|e| e.to_string())?) {
            return Err("scene editing is restricted to project-owned files".into());
        }
        let request: Value = serde_json::from_slice(
            &std::fs::read(edits.ok_or("--edits is required")?).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if request["expected_blake3"].as_str() != Some(blake3::hash(&original).to_hex().as_str()) {
            return Err("scene file revision conflict; inspect again before editing".into());
        }
        let edits: Vec<SceneEdit> =
            serde_json::from_value(request["edits"].clone()).map_err(|e| e.to_string())?;
        document.apply(0, &edits, registry)?;
        let bytes = (serde_json::to_string_pretty(document.value()).map_err(|e| e.to_string())?
            + "\n")
            .into_bytes();
        newviso_scene_document::write_authoring_file(&canonical, &original, &bytes)?;
        return Ok(
            json!({"passed":true,"saved":logical,"blake3":blake3::hash(&bytes).to_hex().to_string(),"revision":document.revision(),"apply":"next_scene_launch"}),
        );
    }
    let entities = document.value()["entities"].as_array().unwrap();
    if command == "scene-validate" {
        return Ok(json!({"passed":true,"scene":logical,"entities":entities.len()}));
    }
    let selected = entities.get(entity).ok_or("entity index is out of range")?;
    let properties=registry.components.values().filter(|c|c.applies_to(selected) && selected.get(&c.id).is_some()).map(|c|
        json!({"component":c.id,"label":c.label,"properties":c.properties.iter().filter_map(|p|selected.pointer(&p.path).map(|v|json!({"descriptor":p,"value":v}))).collect::<Vec<_>>()})).collect::<Vec<_>>();
    Ok(
        json!({"scene":logical,"blake3":blake3::hash(&original).to_hex().to_string(),"entities":entities.iter().enumerate().map(|(i,e)|json!({"index":i,"kind":e["kind"],"name":e["name"]})).collect::<Vec<_>>(),
        "selected":{"index":entity,"record":selected,"components":properties}}),
    )
}
