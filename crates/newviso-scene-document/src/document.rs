use crate::SceneTypeRegistry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};

const HISTORY_BYTES: usize = 16 * 1024 * 1024;
const HISTORY_STEPS: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneEdit {
    Set {
        entity: usize,
        path: String,
        value: Value,
    },
    Add {
        type_id: String,
    },
    Duplicate {
        entity: usize,
    },
    Remove {
        entity: usize,
    },
    AddComponent {
        entity: usize,
        component: String,
    },
    RemoveComponent {
        entity: usize,
        component: String,
    },
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    start: usize,
    before: Vec<Value>,
    after: Vec<Value>,
    bytes: usize,
    before_ids: Vec<u64>,
    after_ids: Vec<u64>,
}

#[derive(Clone, Debug)]
pub struct SceneDocument {
    value: Value,
    revision: u64,
    undo: VecDeque<HistoryEntry>,
    redo: VecDeque<HistoryEntry>,
    history_bytes: usize,
    identities: Vec<u64>,
    next_identity: u64,
}

impl SceneDocument {
    pub fn new(value: Value, registry: &SceneTypeRegistry) -> Result<Self, String> {
        validate_scene(&value, registry)?;
        let count = value["entities"]
            .as_array()
            .expect("validated entities")
            .len();
        Ok(Self {
            identities: (1..=count as u64).collect(),
            next_identity: count as u64 + 1,
            value,
            revision: 0,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            history_bytes: 0,
        })
    }
    /// Session identity survives index shifts and undo/redo; it is never serialized.
    pub fn entity_identity(&self, index: usize) -> Option<u64> {
        self.identities.get(index).copied()
    }
    pub fn index_for_identity(&self, id: u64) -> Option<usize> {
        self.identities.iter().position(|v| *v == id)
    }
    pub fn entity_identities(&self) -> &[u64] {
        &self.identities
    }
    /// Restore a saved in-session checkpoint without reassigning live object identities.
    pub fn restore_checkpoint(
        &mut self,
        value: Value,
        identities: Vec<u64>,
        registry: &SceneTypeRegistry,
    ) -> Result<(), String> {
        validate_scene(&value, registry)?;
        let count = value["entities"]
            .as_array()
            .expect("validated entities")
            .len();
        let unique: std::collections::BTreeSet<_> = identities.iter().copied().collect();
        if identities.len() != count || unique.len() != count || unique.contains(&0) {
            return Err("invalid authoring checkpoint identities".into());
        }
        self.next_identity = self.next_identity.max(
            identities
                .iter()
                .max()
                .copied()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("authoring identity overflow")?,
        );
        self.value = value;
        self.identities = identities;
        self.undo.clear();
        self.redo.clear();
        self.history_bytes = 0;
        self.revision += 1;
        Ok(())
    }
    pub fn value(&self) -> &Value {
        &self.value
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// Every edit in a batch either commits together or leaves document/history untouched.
    pub fn apply(
        &mut self,
        expected_revision: u64,
        edits: &[SceneEdit],
        registry: &SceneTypeRegistry,
    ) -> Result<(), String> {
        if expected_revision != self.revision {
            return Err("scene revision conflict; refresh before editing".into());
        }
        if edits.is_empty() {
            return Ok(());
        }
        if edits.len() > 4096 {
            return Err("scene transaction exceeds 4096 edits".into());
        }
        let mut candidate = self.value.clone();
        let mut identities = self.identities.clone();
        let mut next_identity = self.next_identity;
        for edit in edits {
            apply_edit(&mut candidate, edit, registry)?;
            match edit {
                SceneEdit::Add { .. } | SceneEdit::Duplicate { .. } => {
                    identities.push(next_identity);
                    next_identity = next_identity
                        .checked_add(1)
                        .ok_or("authoring identity overflow")?;
                }
                SceneEdit::Remove { entity } => {
                    identities.remove(*entity);
                }
                _ => {}
            }
        }
        validate_scene(&candidate, registry)?;
        if candidate == self.value && identities == self.identities {
            return Ok(());
        }
        let before = self.value["entities"].as_array().expect("validated scene");
        let after = candidate["entities"].as_array().expect("validated scene");
        let prefix = before
            .iter()
            .zip(after)
            .zip(self.identities.iter().zip(&identities))
            .take_while(|((a, b), (i, j))| a == b && i == j)
            .count();
        let suffix = before[prefix..]
            .iter()
            .rev()
            .zip(after[prefix..].iter().rev())
            .zip(
                self.identities[prefix..]
                    .iter()
                    .rev()
                    .zip(identities[prefix..].iter().rev()),
            )
            .take_while(|((a, b), (i, j))| a == b && i == j)
            .count();
        let before = before[prefix..before.len() - suffix].to_vec();
        let after = after[prefix..after.len() - suffix].to_vec();
        let before_ids = self.identities[prefix..self.identities.len() - suffix].to_vec();
        let after_ids = identities[prefix..identities.len() - suffix].to_vec();
        let bytes = before
            .iter()
            .chain(&after)
            .map(|v| v.to_string().len())
            .sum::<usize>()
            + (before_ids.len() + after_ids.len()) * std::mem::size_of::<u64>();
        self.history_bytes -= self.redo.iter().map(|entry| entry.bytes).sum::<usize>();
        self.redo.clear();
        self.history_bytes += bytes;
        self.undo.push_back(HistoryEntry {
            start: prefix,
            before,
            after,
            bytes,
            before_ids,
            after_ids,
        });
        self.identities = identities;
        self.next_identity = next_identity;
        self.value = candidate;
        self.trim_history();
        self.revision += 1;
        Ok(())
    }
    pub fn undo(&mut self) -> bool {
        if let Some(entry) = self.undo.pop_back() {
            self.value["entities"]
                .as_array_mut()
                .expect("validated scene")
                .splice(
                    entry.start..entry.start + entry.after.len(),
                    entry.before.clone(),
                );
            self.identities.splice(
                entry.start..entry.start + entry.after_ids.len(),
                entry.before_ids.clone(),
            );
            self.redo.push_back(entry);
            self.revision += 1;
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self) -> bool {
        if let Some(entry) = self.redo.pop_back() {
            self.value["entities"]
                .as_array_mut()
                .expect("validated scene")
                .splice(
                    entry.start..entry.start + entry.before.len(),
                    entry.after.clone(),
                );
            self.identities.splice(
                entry.start..entry.start + entry.before_ids.len(),
                entry.after_ids.clone(),
            );
            self.undo.push_back(entry);
            self.revision += 1;
            true
        } else {
            false
        }
    }
    fn trim_history(&mut self) {
        while self.undo.len() + self.redo.len() > HISTORY_STEPS
            || self.history_bytes > HISTORY_BYTES
        {
            if let Some(entry) = self.undo.pop_front().or_else(|| self.redo.pop_front()) {
                self.history_bytes -= entry.bytes;
            } else {
                break;
            }
        }
    }
}

fn apply_edit(
    scene: &mut Value,
    edit: &SceneEdit,
    registry: &SceneTypeRegistry,
) -> Result<(), String> {
    let root_links = scene.get("visibility_graph").is_some();
    let entities = scene["entities"]
        .as_array_mut()
        .ok_or("scene entities must be an array")?;
    match edit {
        SceneEdit::Add { type_id } => {
            let mut entity = registry
                .types
                .get(type_id)
                .ok_or_else(|| format!("unknown entity type '{type_id}'"))?
                .template
                .clone();
            entity["name"] = Value::String(unique_name(
                entities,
                entity["name"].as_str().unwrap_or(type_id),
            ));
            entities.push(entity);
        }
        SceneEdit::Duplicate { entity } => {
            let mut copy = entities
                .get(*entity)
                .ok_or("entity index is out of range")?
                .clone();
            // Runtime handles are assigned by the scene provider, never copied into authored entities.
            if copy.get("id").is_some()
                || copy.get("stable_id").is_some()
                || copy.get("handle").is_some()
            {
                return Err("duplicate of explicitly identified entities requires an identity remapping tool".into());
            }
            copy["name"] = Value::String(unique_name(
                entities,
                copy["name"].as_str().unwrap_or("Entity"),
            ));
            entities.push(copy);
        }
        SceneEdit::Remove { entity } => {
            // Numeric provider handles are order-derived for ordinary authored scenes.
            // Avoid silently retargeting any relationship until explicit identity remapping is available.
            if entities
                .iter()
                .any(|e| e.get("parent_id").is_some() || e.get("parent").is_some())
                || root_links
                || scene_has_external_links(entities)
            {
                return Err(
                    "removal of linked scenes requires explicit relationship remapping".into(),
                );
            }
            if *entity >= entities.len() {
                return Err("entity index is out of range".into());
            }
            entities.remove(*entity);
        }
        SceneEdit::Set {
            entity,
            path,
            value,
        } => {
            let e = entities
                .get_mut(*entity)
                .ok_or("entity index is out of range")?;
            if path == "/name" {
                if !value.as_str().is_some_and(|s| !s.trim().is_empty()) {
                    return Err("entity name must not be empty".into());
                }
                e["name"] = value.clone();
            } else {
                let p = registry
                    .property(path)
                    .ok_or_else(|| format!("property '{path}' is not registered"))?;
                let component = path
                    .trim_start_matches('/')
                    .split('/')
                    .next()
                    .ok_or("invalid property path")?;
                if !registry
                    .components
                    .get(component)
                    .ok_or("unknown component")?
                    .applies_to(e)
                {
                    return Err("property does not apply to this entity type".into());
                }
                p.validate(value)?;
                let (parent, key) = path
                    .rsplit_once('/')
                    .ok_or("property path must be a JSON pointer")?;
                let parent = e
                    .pointer_mut(parent)
                    .and_then(Value::as_object_mut)
                    .ok_or("add the component before editing its properties")?;
                parent.insert(key.replace("~1", "/").replace("~0", "~"), value.clone());
            }
        }
        SceneEdit::AddComponent { entity, component } => {
            let record = entities
                .get(*entity)
                .ok_or("entity index is out of range")?;
            if !registry
                .components
                .get(component)
                .ok_or("unknown component")?
                .applies_to(record)
            {
                return Err("component does not apply to this entity type".into());
            }
            let c = registry
                .components
                .get(component)
                .ok_or("unknown component")?;
            let e = entities
                .get_mut(*entity)
                .and_then(Value::as_object_mut)
                .ok_or("entity index is out of range")?;
            if e.contains_key(component) {
                return Err("component already exists".into());
            }
            e.insert(component.clone(), c.defaults.clone());
        }
        SceneEdit::RemoveComponent { entity, component } => {
            if !registry.components.contains_key(component) {
                return Err("unknown component".into());
            }
            let e = entities
                .get_mut(*entity)
                .and_then(Value::as_object_mut)
                .ok_or("entity index is out of range")?;
            if e.remove(component).is_none() {
                return Err("component is absent".into());
            }
        }
    }
    Ok(())
}

fn scene_has_external_links(entities: &[Value]) -> bool {
    entities
        .iter()
        .any(|e| e.get("kind").and_then(Value::as_str) == Some("portal"))
}

fn unique_name(entities: &[Value], base: &str) -> String {
    let names: std::collections::BTreeSet<_> =
        entities.iter().filter_map(|e| e["name"].as_str()).collect();
    if !names.contains(base) {
        return base.into();
    }
    for n in 2.. {
        let name = format!("{base} {n}");
        if !names.contains(name.as_str()) {
            return name;
        }
    }
    unreachable!()
}

pub fn validate_scene(scene: &Value, registry: &SceneTypeRegistry) -> Result<(), String> {
    if scene["schema"] != "newviso.scene.v1" || scene["version"].as_u64() != Some(1) {
        return Err("scene requires schema newviso.scene.v1 and version 1".into());
    }
    let entities = scene["entities"]
        .as_array()
        .ok_or("scene entities must be an array")?;
    if entities.len() > 1_000_000 {
        return Err("scene entity limit exceeded".into());
    }
    let mut counts = BTreeMap::<&str, usize>::new();
    for (i, e) in entities.iter().enumerate() {
        let kind = e["kind"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("entity {i}: missing kind"))?;
        *counts.entry(kind).or_default() += 1;
        if !e["name"].as_str().is_some_and(|s| !s.trim().is_empty()) {
            return Err(format!("entity {i}: missing name"));
        }
        for c in registry.components.values() {
            if let Some(v) = e.get(&c.id) {
                if !v.is_object() {
                    return Err(format!("entity {i}: {} must be an object", c.id));
                }
                for p in &c.properties {
                    if let Some(v) = e.pointer(&p.path) {
                        p.validate(v).map_err(|err| format!("entity {i}: {err}"))?;
                    }
                }
            }
        }
        if kind == "camera" {
            for p in [
                "/transform/position",
                "/transform/target",
                "/transform/up",
                "/camera/fov_y_degrees",
                "/camera/near",
                "/camera/far",
            ] {
                if e.pointer(p).is_none() {
                    return Err(format!("entity {i}: missing {p}"));
                }
            }
            if e["camera"]["near"].as_f64() >= e["camera"]["far"].as_f64() {
                return Err(format!("entity {i}: camera near must be smaller than far"));
            }
            let vector = |key: &str| -> Result<[f64; 3], String> {
                let values = e["transform"][key]
                    .as_array()
                    .filter(|v| v.len() == 3)
                    .ok_or("camera basis requires three numeric coordinates")?;
                let mut result = [0.0; 3];
                for i in 0..3 {
                    result[i] =
                        values[i].as_f64().ok_or("camera basis must be numeric")? as f32 as f64;
                }
                Ok(result)
            };
            let position = vector("position")?;
            let target = vector("target")?;
            let up = vector("up")?;
            let forward = [
                target[0] - position[0],
                target[1] - position[1],
                target[2] - position[2],
            ];
            let right = [
                forward[1] * up[2] - forward[2] * up[1],
                forward[2] * up[0] - forward[0] * up[2],
                forward[0] * up[1] - forward[1] * up[0],
            ];
            if right.iter().map(|v| v * v).sum::<f64>() <= 1.0e-20 {
                return Err(format!(
                    "entity {i}: camera up must form a non-degenerate basis with its direction"
                ));
            }
            if e["transform"]["position"] == e["transform"]["target"] {
                return Err(format!(
                    "entity {i}: camera position and target must differ"
                ));
            }
        }
        if let Some(value) = e.get("character") {
            crate::SceneCharacterBinding::parse(value)
                .map_err(|err| format!("entity {i}: {err}"))?;
        }
        if kind == "mesh" {
            if !e["transform"].is_object() || !e["mesh"].is_object() {
                return Err(format!(
                    "entity {i}: mesh requires transform and mesh components"
                ));
            }
            let primitive = e
                .pointer("/mesh/primitive")
                .and_then(Value::as_str)
                .unwrap_or("");
            if matches!(primitive, "cube" | "heightfield") {
                if e.pointer("/material/base_color").is_none() {
                    return Err(format!(
                        "entity {i}: primitive requires material.base_color"
                    ));
                }
            } else if e
                .pointer("/mesh/asset")
                .or_else(|| e.pointer("/mesh/model"))
                .and_then(Value::as_str)
                .is_none()
            {
                return Err(format!(
                    "entity {i}: mesh requires a supported primitive or asset reference"
                ));
            }
        }
        if let (Some(v), Some(s)) = (
            e.pointer("/lod/visible_distance").and_then(Value::as_f64),
            e.pointer("/lod/stream_distance").and_then(Value::as_f64),
        ) {
            if s < v {
                return Err(format!(
                    "entity {i}: stream distance must cover visible distance"
                ));
            }
        }
    }
    if counts.get("camera").copied().unwrap_or(0) == 0
        || counts.get("mesh").copied().unwrap_or(0) == 0
    {
        return Err("runtime scenes require at least one camera and one mesh".into());
    }
    Ok(())
}

#[cfg(test)]
mod authoring_identity_tests {
    use super::*;
    fn document() -> SceneDocument {
        let r = SceneTypeRegistry::builtin();
        SceneDocument::new(serde_json::json!({"schema":"newviso.scene.v1","version":1,"entities":[r.types["camera"].template,r.types["cube"].template,r.types["cube"].template]}),r).unwrap()
    }
    #[test]
    fn identical_records_keep_distinct_identities_through_removal_undo_and_branching() {
        let r = SceneTypeRegistry::builtin();
        let mut d = document();
        d.apply(0, &[SceneEdit::Remove { entity: 1 }], r).unwrap();
        assert_eq!(d.entity_identity(1), Some(3));
        assert!(d.undo());
        assert_eq!(d.entity_identities(), &[1, 2, 3]);
        assert!(d.redo());
        d.apply(d.revision(), &[SceneEdit::Duplicate { entity: 1 }], r)
            .unwrap();
        assert_eq!(d.entity_identities(), &[1, 3, 4]);
        assert!(!d.can_redo());
    }
    #[test]
    fn checkpoint_preserves_bindings_and_never_reuses_an_abandoned_identity() {
        let r = SceneTypeRegistry::builtin();
        let mut d = document();
        let saved = d.value().clone();
        let ids = d.entity_identities().to_vec();
        d.apply(0, &[SceneEdit::Duplicate { entity: 1 }], r)
            .unwrap();
        d.restore_checkpoint(saved, ids, r).unwrap();
        assert_eq!(d.entity_identities(), &[1, 2, 3]);
        d.apply(d.revision(), &[SceneEdit::Duplicate { entity: 1 }], r)
            .unwrap();
        assert_eq!(d.entity_identity(3), Some(5));
    }
    #[test]
    fn failed_batch_does_not_consume_identity_or_mutate_history() {
        let r = SceneTypeRegistry::builtin();
        let mut d = document();
        assert!(d
            .apply(
                0,
                &[
                    SceneEdit::Duplicate { entity: 1 },
                    SceneEdit::Remove { entity: 99 }
                ],
                r
            )
            .is_err());
        assert_eq!(d.entity_identities(), &[1, 2, 3]);
        assert_eq!(d.revision(), 0);
        d.apply(0, &[SceneEdit::Duplicate { entity: 1 }], r)
            .unwrap();
        assert_eq!(d.entity_identity(3), Some(4));
    }
}
