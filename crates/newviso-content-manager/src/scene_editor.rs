use super::*;
use newviso_scene_document::{SceneDocument, SceneEdit, SceneTypeRegistry};
use serde_json::json;
use std::collections::BTreeMap;
mod viewport;
pub use viewport::{ViewportEntity, ViewportOptions, ViewportVisual};

#[derive(Debug)]
pub(super) struct SceneEditor {
    document: SceneDocument,
    saved_document: Value,
    saved_identities: Vec<u64>,
    selection_active: bool,
    viewport: viewport::ViewportState,
    selected: usize,
    drafts: BTreeMap<String, String>,
    search: String,
    page: usize,
    context: Option<(usize, u64, [f64; 2])>,
    pub(super) context_press: Option<(usize, [f64; 2])>,
}

impl SceneEditor {
    pub fn parse(text: &str) -> Result<Option<Self>, String> {
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            return Ok(None);
        };
        if value["schema"] != "newviso.scene.v1" {
            return Ok(None);
        }
        Ok(Some(Self {
            saved_identities: (1..=value["entities"].as_array().map_or(0, Vec::len) as u64)
                .collect(),
            selection_active: false,
            viewport: viewport::ViewportState::default(),
            saved_document: value.clone(),
            document: SceneDocument::new(value, SceneTypeRegistry::builtin())?,
            selected: 0,
            drafts: BTreeMap::new(),
            search: String::new(),
            page: 0,
            context: None,
            context_press: None,
        }))
    }
    pub fn selected_index(&self) -> usize {
        self.selected
    }
    pub fn context_contains(&self, p: [f64; 2]) -> bool {
        self.context.is_some_and(|(_, _, origin)| {
            p[0] >= origin[0]
                && p[1] >= origin[1]
                && p[0] <= origin[0] + 280.0
                && p[1] <= origin[1] + 350.0
        })
    }
    pub fn context_open(&self) -> bool {
        self.context.is_some()
    }
    pub fn is_dirty(&self) -> bool {
        self.document.value() != &self.saved_document
            || !self.drafts.is_empty()
            || self.viewport.preview.is_some()
    }
    pub fn mark_saved(&mut self) {
        self.saved_document = self.document.value().clone();
        self.saved_identities = self.document.entity_identities().to_vec();
    }
    fn sync(&self, text: &mut String) {
        *text = serde_json::to_string_pretty(self.document.value()).expect("scene JSON") + "\n";
    }
    pub fn patch(&mut self, path: &str, value: &Value, text: &mut String) -> Result<(), String> {
        if path == "content.scene.search" {
            self.search = value.as_str().unwrap_or("").to_owned();
            self.page = 0;
            return Ok(());
        }
        let Some(pointer) = path.strip_prefix("content.scene.property:") else {
            return Ok(());
        };
        if self.viewport.preview.is_some() {
            return Err("Finish or cancel the viewport drag before editing properties".into());
        }
        let raw = value.as_str().ok_or("property editor requires text")?;
        self.drafts.insert(pointer.to_owned(), raw.to_owned());
        let value = if pointer == "/name"
            || SceneTypeRegistry::builtin()
                .property(pointer)
                .is_some_and(|p| {
                    matches!(
                        p.kind,
                        newviso_scene_document::PropertyKind::String
                            | newviso_scene_document::PropertyKind::Asset
                            | newviso_scene_document::PropertyKind::Enum
                    )
                }) {
            Value::String(raw.to_owned())
        } else {
            serde_json::from_str(raw).map_err(|e| format!("{pointer}: {e}"))?
        };
        self.document.apply(
            self.document.revision(),
            &[SceneEdit::Set {
                entity: self.selected,
                path: pointer.to_owned(),
                value,
            }],
            SceneTypeRegistry::builtin(),
        )?;
        self.drafts.remove(pointer);
        self.sync(text);
        Ok(())
    }
    pub fn action(&mut self, id: &str, payload: &Value, text: &mut String) -> Result<(), String> {
        if self.viewport.preview.is_some() {
            return Err("Finish or cancel the viewport drag before another scene action".into());
        }
        if id == "content.scene.menu.close" {
            self.context = None;
            return Ok(());
        }
        if id == "content.scene.menu.open" {
            let entity = payload["entity"].as_u64().ok_or("missing entity index")? as usize;
            if self.document.value()["entities"]
                .as_array()
                .is_none_or(|v| entity >= v.len())
            {
                return Err("entity index is out of range".into());
            }
            if !self.drafts.is_empty() && entity != self.selected {
                return Err("Fix or undo invalid input before selecting another entity".into());
            }
            self.selected = entity;
            self.selection_active = true;
            let rect = &payload["global_rect"];
            let pos = &payload["local_pos"];
            let w = payload["viewport"][0].as_f64().unwrap_or(1280.0);
            let h = payload["viewport"][1].as_f64().unwrap_or(720.0);
            let x = payload["position"][0].as_f64().unwrap_or_else(|| {
                rect[0].as_f64().unwrap_or(20.0) + pos[0].as_f64().unwrap_or(0.0)
            });
            let y = payload["position"][1].as_f64().unwrap_or_else(|| {
                rect[1].as_f64().unwrap_or(80.0) + pos[1].as_f64().unwrap_or(0.0)
            });
            self.context = Some((
                entity,
                self.document.revision(),
                [
                    x.clamp(0.0, (w - 300.0).max(0.0)),
                    y.clamp(0.0, (h - 380.0).max(0.0)),
                ],
            ));
            return Ok(());
        }
        if id == "content.scene.menu.execute" {
            let (entity, revision, _) = self.context.ok_or("Object menu is closed")?;
            if revision != self.document.revision() {
                self.context = None;
                return Err("Scene changed since the object menu opened; reopen it".into());
            }
            self.selected = entity;
            let command = payload["command"].as_str().ok_or("missing object action")?;
            if !matches!(
                command,
                "content.scene.properties"
                    | "content.scene.duplicate"
                    | "content.scene.remove"
                    | "content.scene.component.add"
                    | "content.scene.component.remove"
            ) {
                return Err("unsupported object action".into());
            }
            self.context = None;
            return self.action(command, payload, text);
        }
        if id != "content.scene.page" {
            self.context = None;
        }
        if matches!(
            id,
            "content.scene.select"
                | "content.scene.add"
                | "content.scene.duplicate"
                | "content.scene.properties"
        ) {
            self.selection_active = true;
        }
        match id {
            "content.scene.properties" => {}
            "content.scene.select" => {
                if !self.drafts.is_empty() {
                    return Err("Fix or undo invalid input before selecting another entity".into());
                }
                self.selected = payload["entity"].as_u64().ok_or("missing entity index")? as usize;
                self.drafts.clear();
            }
            "content.scene.page" => {
                self.page = payload["page"].as_u64().unwrap_or(0) as usize;
            }
            "content.scene.revert" => {
                self.document.restore_checkpoint(
                    self.saved_document.clone(),
                    self.saved_identities.clone(),
                    SceneTypeRegistry::builtin(),
                )?;
                self.drafts.clear();
                self.sync(text);
            }
            "content.scene.undo" => {
                self.document.undo();
                self.drafts.clear();
                self.sync(text);
            }
            "content.scene.redo" => {
                self.document.redo();
                self.drafts.clear();
                self.sync(text);
            }
            _ => {
                let edit = match id {
                    "content.scene.add" => SceneEdit::Add {
                        type_id: payload["type_id"].as_str().ok_or("missing type")?.into(),
                    },
                    "content.scene.duplicate" => SceneEdit::Duplicate {
                        entity: self.selected,
                    },
                    "content.scene.remove" => SceneEdit::Remove {
                        entity: self.selected,
                    },
                    "content.scene.component.add" => SceneEdit::AddComponent {
                        entity: self.selected,
                        component: payload["component"]
                            .as_str()
                            .ok_or("missing component")?
                            .into(),
                    },
                    "content.scene.component.remove" => SceneEdit::RemoveComponent {
                        entity: self.selected,
                        component: payload["component"]
                            .as_str()
                            .ok_or("missing component")?
                            .into(),
                    },
                    _ => return Ok(()),
                };
                self.document.apply(
                    self.document.revision(),
                    &[edit],
                    SceneTypeRegistry::builtin(),
                )?;
                if matches!(id, "content.scene.add" | "content.scene.duplicate") {
                    self.selected = self.document.value()["entities"]
                        .as_array()
                        .expect("validated scene")
                        .len()
                        .saturating_sub(1);
                }
                self.drafts.clear();
                self.sync(text);
            }
        }
        self.selected = self.selected.min(
            self.document.value()["entities"]
                .as_array()
                .map_or(0, |v| v.len().saturating_sub(1)),
        );
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.viewport.preview.is_some() {
            return Err("Release the gizmo or press Escape before saving".into());
        }
        if !self.drafts.is_empty() {
            return Err("Finish or undo invalid property input before saving".into());
        }
        Ok(())
    }
    pub fn outliner(&self) -> Value {
        let entities = self.document.value()["entities"]
            .as_array()
            .expect("scene entities");
        let query = self.search.to_lowercase();
        let filtered = entities
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&query)
            })
            .collect::<Vec<_>>();
        let pages = filtered.len().max(1).div_ceil(100);
        let page = self.page.min(pages - 1);
        let mut children = vec![
            json!({"id":"scene.heading","component_id":"text","text":format!("Scene Outliner · {} entities",entities.len())}),
            json!({"id":"scene.search","component_id":"input","value":self.search,"props":{"state_path":"content.scene.search","placeholder":"Find entity"}}),
        ];
        let mut nav = vec![];
        if page > 0 {
            nav.push(button(
                "scene.prev",
                "Previous",
                "content.scene.page",
                json!({"page":page-1}),
            ));
        }
        if page + 1 < pages {
            nav.push(button(
                "scene.next",
                "Next",
                "content.scene.page",
                json!({"page":page+1}),
            ));
        }
        children.push(json!({"id":"scene.pages","component_id":"row","children":nav}));
        for (i, e) in filtered.into_iter().skip(page * 100).take(100) {
            let mut node = button(
                &format!("scene.entity.{i}"),
                e["name"].as_str().unwrap_or("Entity"),
                "content.scene.select",
                json!({"entity":i}),
            );
            node["detail"] = json!(e["kind"]);
            if self.selection_active && i == self.selected {
                node["tone"] = json!("accent");
            }
            node["events"] = json!([{"trigger":"context_menu","action_id":"content.scene.menu.open","payload":{"entity":i}}]);
            children.push(json!({"id":format!("scene.row.{i}"),"component_id":"row","children":[node,
                button(&format!("scene.more.{i}"),"...","content.scene.menu.open",json!({"entity":i}))]}));
        }
        json!({"id":"scene.outliner","component_id":"column","children":children})
    }
    pub fn inspector(&self) -> Value {
        let registry = SceneTypeRegistry::builtin();
        if !self.selection_active {
            return json!({"id":"scene.inspector","component_id":"column","children":[self.viewport_toolbar(),{"id":"scene.no_selection","component_id":"text","text":"Click an object in the viewport or Scene Outliner. RMB + WASD: fly · MMB: pan · Alt + LMB: orbit · F: focus"}]});
        }
        let e = &self.document.value()["entities"][self.selected];
        let mut children = vec![
            self.viewport_toolbar(),
            json!({"id":"scene.revision","component_id":"text","text":format!("Entity {} · document revision {}",self.selected,self.document.revision())}),
        ];
        let mut actions = vec![button(
            "scene.revert",
            "Revert saved",
            "content.scene.revert",
            Value::Null,
        )];
        if self.document.can_undo() {
            actions.push(button(
                "scene.undo",
                "Undo",
                "content.scene.undo",
                Value::Null,
            ));
        }
        if self.document.can_redo() {
            actions.push(button(
                "scene.redo",
                "Redo",
                "content.scene.redo",
                Value::Null,
            ));
        }
        actions.push(button(
            "scene.duplicate",
            "Duplicate",
            "content.scene.duplicate",
            Value::Null,
        ));
        actions.push(button(
            "scene.remove",
            "Delete",
            "content.scene.remove",
            Value::Null,
        ));
        children.push(json!({"id":"scene.actions","component_id":"row","children":actions}));
        children.push(self.input("/name", "Name", &e["name"]));
        for c in registry.components.values().filter(|c| c.applies_to(e)) {
            if e.get(&c.id).is_some() {
                children.push(json!({"id":format!("scene.component.{}",c.id),"component_id":"text","text":c.label}));
                for p in &c.properties {
                    if let Some(value) = e.pointer(&p.path) {
                        children.push(self.input(&p.path, &p.label, value));
                    }
                }
                children.push(button(
                    &format!("scene.remove.{}", c.id),
                    &format!("Remove {}", c.label),
                    "content.scene.component.remove",
                    json!({"component":c.id}),
                ));
            } else {
                children.push(button(
                    &format!("scene.add.{}", c.id),
                    &format!("Add {}", c.label),
                    "content.scene.component.add",
                    json!({"component":c.id}),
                ));
            }
        }
        for t in registry.types.values() {
            children.push(button(
                &format!("scene.type.{}", t.id),
                &format!("Create {}", t.label),
                "content.scene.add",
                json!({"type_id":t.id}),
            ));
        }
        json!({"id":"scene.inspector","component_id":"column","props":{"scroll_y":true,"max_height_px":480,"overflow":"auto","h_px":480},"children":children})
    }
    pub fn context_surface(&self) -> Value {
        let mut surface = json!({"version":1,"surface_id":"newviso.content_manager.object_menu","source":"newviso.content-manager",
            "visible":self.context.is_some(),"modal":true,"z_order":1200,"title":"Object actions",
            "style":{"anchor":"center","min_size_px":[300,420],"max_size_px":[300,420]},"components":[]});
        if let Some((index, _, position)) = self.context {
            let e = &self.document.value()["entities"][index];
            surface["subtitle"] = json!(e["name"]);
            let mut actions = vec![];
            for (label, command) in [
                ("Properties", "content.scene.properties"),
                ("Duplicate", "content.scene.duplicate"),
                ("Delete", "content.scene.remove"),
            ] {
                actions.push(button(
                    &format!("scene.menu.{command}"),
                    label,
                    "content.scene.menu.execute",
                    json!({"command":command}),
                ));
            }
            for c in SceneTypeRegistry::builtin()
                .components
                .values()
                .filter(|c| c.applies_to(e))
            {
                let present = e.get(&c.id).is_some();
                actions.push(button(&format!("scene.menu.component.{}",c.id),&format!("{} {}",if present{"Remove"}else{"Add"},c.label),"content.scene.menu.execute",
                    json!({"command":if present{"content.scene.component.remove"}else{"content.scene.component.add"},"component":c.id})));
            }
            actions.push(button(
                "scene.menu.close",
                "Close",
                "content.scene.menu.close",
                Value::Null,
            ));
            surface["title"] = json!("");
            surface["subtitle"] = json!("");
            surface["metrics"] = json!({"chrome":false,"dock":"overlay","overlay_x":position[0],"overlay_y":position[1],"overlay_width":280});
            surface["style_tags"] = json!([]);
            surface["components"] = json!([{"id":"scene.menu.panel","component_id":"column","text":e["name"],
                "props":{"max_height":350,"overflow":"auto","scroll_y":true,"max_height_px":350,"h_px":350},"children":actions}]);
        }
        surface
    }
    fn input(&self, path: &str, label: &str, value: &Value) -> Value {
        let preview = self.viewport.preview.and_then(|(_, t)| match path {
            "/transform/position" => Some(json!(t.position)),
            "/transform/rotation_degrees" => Some(json!(t.rotation_degrees)),
            "/transform/scale" => Some(json!(t.scale)),
            _ => None,
        });
        let value = preview.as_ref().unwrap_or(value);
        let text = self.drafts.get(path).cloned().unwrap_or_else(|| {
            value.as_str().map(str::to_owned).unwrap_or_else(|| {
                if value.is_object() {
                    serde_json::to_string_pretty(value).expect("game property JSON")
                } else {
                    value.to_string()
                }
            })
        });
        let kind = if value.is_object() {
            "code_editor"
        } else {
            "input"
        };
        json!({"id":format!("scene.property:{path}"),"component_id":kind,"text":label,"value":text,
            "props":{"state_path":format!("content.scene.property:{path}"),"placeholder":label}})
    }
}

fn button(id: &str, text: &str, action: &str, payload: Value) -> Value {
    json!({"id":id,"component_id":"button","text":text,"action_id":action,"props":{"payload":payload}})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(label: &str) -> (PathBuf, ContentManager) {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("newviso-scene-editor-{label}-{nonce}"));
        fs::create_dir_all(root.join("scenes")).unwrap();
        let r = SceneTypeRegistry::builtin();
        let scene = json!({"schema":"newviso.scene.v1","version":1,"metadata":{"original":true},"entities":[r.types["camera"].template,r.types["cube"].template]});
        fs::write(
            root.join("scenes/main.json"),
            serde_json::to_string_pretty(&scene).unwrap(),
        )
        .unwrap();
        let mut m = ContentManager::open(&root).unwrap();
        m.select("scenes/main.json").unwrap();
        m.apply_dispatch(
            &json!({"actions":[{"action_id":"content.scene.select","payload":{"entity":1}}]}),
        )
        .unwrap();
        (root, m)
    }
    #[test]
    fn native_inspector_dispatch_edits_undoes_and_saves_authored_scene() {
        let (root, mut m) = fixture("edit");
        m.apply_dispatch(&json!({"state_patches":[{"changes":[{"path":"content.scene.property:/transform/position","value":"[4,5,6]"}]}]})).unwrap();
        assert!(m.is_dirty());
        m.apply_dispatch(&json!({"actions":[{"action_id":"content.scene.undo"}]}))
            .unwrap();
        assert!(
            !m.is_dirty(),
            "undo to saved data must clear dirty state regardless of formatting"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&m.editor_text).unwrap()["entities"][1]["transform"]
                ["position"],
            json!([0, 0, 0])
        );
        m.apply_dispatch(
            &json!({"actions":[{"action_id":"content.scene.redo"},{"action_id":"content.save"}]}),
        )
        .unwrap();
        let saved: Value =
            serde_json::from_slice(&fs::read(root.join("scenes/main.json")).unwrap()).unwrap();
        assert_eq!(
            saved["entities"][1]["transform"]["position"],
            json!([4, 5, 6])
        );
        assert_eq!(saved["metadata"]["original"], true);
        assert!(m.surfaces().iter().any(|v| v["title"] == "Scene Inspector"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn context_menu_targets_clicked_entity_and_edits_project_game_properties() {
        let (root, mut m) = fixture("menu");
        let surfaces = serde_json::to_string(&m.surfaces()).unwrap();
        assert!(surfaces.contains("context_menu"));
        m.apply_dispatch(
            &json!({"actions":[{"action_id":"content.scene.menu.open","payload":{"entity":1}}]}),
        )
        .unwrap();
        assert_eq!(m.surfaces().last().unwrap()["visible"], true);
        m.apply_dispatch(&json!({"actions":[{"action_id":"content.scene.menu.execute","payload":{"command":"content.scene.component.add","component":"properties"}}]})).unwrap();
        m.apply_dispatch(&json!({"state_patches":[{"changes":[{"path":"content.scene.property:/properties/data","value":"{\"item_id\":\"crate.test\",\"usable\":true,\"weight\":2}"}]}]})).unwrap();
        m.save().unwrap();
        let saved: Value =
            serde_json::from_slice(&fs::read(root.join("scenes/main.json")).unwrap()).unwrap();
        assert_eq!(
            saved["entities"][1]["properties"]["data"]["item_id"],
            "crate.test"
        );
        assert_eq!(saved["metadata"]["original"], true);
        m.apply_dispatch(&json!({"actions":[{"action_id":"content.scene.menu.open","payload":{"entity":1}},{"action_id":"content.scene.menu.execute","payload":{"command":"content.scene.duplicate"}}]})).unwrap();
        assert_eq!(
            m.scene_editor.as_ref().unwrap().document.value()["entities"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(m.surfaces().last().unwrap()["visible"], false);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn invalid_scene_draft_blocks_selection_and_save() {
        let (root, mut m) = fixture("draft");
        m.apply_dispatch(&json!({"state_patches":[{"changes":[{"path":"content.scene.property:/transform/position","value":"[1,"}]}]})).unwrap();
        assert!(m.is_dirty());
        assert!(m.select("scenes/main.json").is_err());
        assert!(m.save().is_err());
        assert!(m
            .scene_editor
            .as_mut()
            .unwrap()
            .action(
                "content.scene.menu.open",
                &json!({"entity":0}),
                &mut m.editor_text
            )
            .is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn invalid_input_and_concurrent_disk_edits_cannot_be_saved() {
        let (root, mut m) = fixture("conflict");
        let path = root.join("scenes/main.json");
        let before = fs::read(&path).unwrap();
        m.apply_dispatch(&json!({"state_patches":[{"changes":[{"path":"content.scene.property:/transform/position","value":"[4,"}]}]})).unwrap();
        assert!(m.save().is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        m.apply_dispatch(&json!({"state_patches":[{"changes":[{"path":"content.scene.property:/transform/position","value":"[4,5,6]"}]}]})).unwrap();
        fs::write(&path, b"external edit").unwrap();
        assert!(m.save().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"external edit");
        fs::remove_dir_all(root).unwrap();
    }
}
