use super::*;
use newviso_project::{RuntimeSettingField, RuntimeSettingsDocument};
use serde_json::json;
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug)]
pub(super) struct SettingsEditor {
    document: RuntimeSettingsDocument,
    relative_path: Option<String>,
    saved_bytes: Vec<u8>,
    drafts: BTreeMap<String, String>,
    section: String,
    search: String,
    undo: VecDeque<Value>,
    redo: Vec<Value>,
}

impl ContentManager {
    pub fn set_runtime_settings(
        &mut self,
        document: RuntimeSettingsDocument,
        relative_path: Option<String>,
    ) -> Result<(), String> {
        let saved_bytes = if let Some(path) = &relative_path {
            let target = self.resolve(path)?;
            let bytes = fs::read(&target).map_err(|e| e.to_string())?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if &value != document.value() {
                return Err("Runtime layer changed since launch; reopen the editor".into());
            }
            bytes
        } else {
            Vec::new()
        };
        self.settings_editor = Some(SettingsEditor {
            document,
            relative_path,
            saved_bytes,
            drafts: BTreeMap::new(),
            section: "streaming".into(),
            search: String::new(),
            undo: VecDeque::new(),
            redo: Vec::new(),
        });
        self.invalidate_surfaces();
        Ok(())
    }
    pub fn settings_applied(&mut self) {
        if let Some(editor) = &mut self.settings_editor {
            editor.document.mark_applied();
        }
        self.invalidate_surfaces();
    }
    pub(super) fn save_settings(&mut self) -> Result<(), String> {
        let editor = self
            .settings_editor
            .as_ref()
            .ok_or("No runtime settings loaded")?;
        editor.validate()?;
        let relative = editor
            .relative_path
            .as_ref()
            .ok_or("Runtime layer is outside the writable project mounts")?;
        let path = self.resolve(relative)?;
        let editor = self.settings_editor.as_mut().expect("settings");
        let bytes = editor.document.serialized().into_bytes();
        if editor.document.is_dirty() {
            newviso_scene_document::write_authoring_file(&path, &editor.saved_bytes, &bytes)?;
            editor.saved_bytes = bytes;
            editor.document.mark_saved();
        }
        self.status = "Project settings saved. Apply Live updates supported fields; other fields need restart.".into();
        self.invalidate_surfaces();
        Ok(())
    }
    pub(super) fn settings_action(
        &mut self,
        id: &str,
        payload: &Value,
    ) -> Result<Option<ContentEffect>, String> {
        if id == "content.settings.open" {
            if !self.settings_visible && self.is_dirty() {
                return Err("Save current edits before opening Project Settings".into());
            }
            if self.settings_editor.is_none() {
                return Err("No project runtime configuration is attached".into());
            }
            self.settings_visible = true;
            self.status =
                "Editing project runtime layer; profiles and launch overrides retain precedence"
                    .into();
            return Ok(None);
        }
        let editor = self
            .settings_editor
            .as_mut()
            .ok_or("No runtime settings loaded")?;
        match id {
            "content.settings.close" => {
                if editor.is_dirty() {
                    return Err(
                        "Save or revert Project Settings before returning to the scene".into(),
                    );
                }
                self.settings_visible = false;
            }
            "content.settings.section" => {
                editor.section = payload["section"].as_str().unwrap_or("streaming").into();
            }
            "content.settings.revert" => {
                editor.document.revert()?;
                editor.drafts.clear();
                editor.undo.clear();
                editor.redo.clear();
            }
            "content.settings.boolean" => {
                let path = payload["path"]
                    .as_str()
                    .ok_or("missing boolean settings path")?;
                editor.patch(
                    &format!("content.settings.property:{path}"),
                    &payload["value"],
                )?;
            }
            "content.settings.reset" => {
                let before = editor.document.value().clone();
                let path = payload["path"].as_str().ok_or("missing settings path")?;
                editor.document.reset(path)?;
                editor.remember(before);
                editor.drafts.remove(path);
            }
            "content.settings.undo" => {
                if let Some(previous) = editor.undo.pop_back() {
                    let current = editor.document.value().clone();
                    editor.document.replace(previous)?;
                    editor.redo.push(current);
                    editor.drafts.clear();
                }
            }
            "content.settings.redo" => {
                if let Some(next) = editor.redo.pop() {
                    let current = editor.document.value().clone();
                    editor.document.replace(next)?;
                    editor.undo.push_back(current);
                    editor.drafts.clear();
                }
            }
            "content.settings.apply" => {
                editor.validate()?;
                let (patch, expected) = editor.document.live_patch()?;
                if expected.is_empty() {
                    self.status =
                        "No changed live settings. Saved restart fields apply on next launch."
                            .into();
                } else {
                    return Ok(Some(ContentEffect::RuntimeSettingsApply {
                        patch,
                        expected,
                    }));
                }
            }
            "content.settings.save" => {
                self.save_settings()?;
            }
            _ => {}
        }
        Ok(None)
    }
}

impl SettingsEditor {
    pub fn is_dirty(&self) -> bool {
        self.document.is_dirty() || !self.drafts.is_empty()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.drafts.is_empty() {
            Ok(())
        } else {
            Err("Fix invalid settings input, undo or revert before saving/applying".into())
        }
    }
    fn remember(&mut self, previous: Value) {
        if &previous == self.document.value() {
            return;
        }
        self.undo.push_back(previous);
        self.redo.clear();
        while self.undo.len() > 64
            || self.undo.iter().map(|v| v.to_string().len()).sum::<usize>() > 4 * 1024 * 1024
        {
            self.undo.pop_front();
        }
    }
    pub fn patch(&mut self, path: &str, input: &Value) -> Result<(), String> {
        if path == "content.settings.search" {
            self.search = input.as_str().unwrap_or("").into();
            return Ok(());
        }
        let Some(path) = path.strip_prefix("content.settings.property:") else {
            return Ok(());
        };
        let field = self
            .document
            .fields()
            .into_iter()
            .find(|f| f.path == path)
            .ok_or("unregistered settings property")?;
        let text = input
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| input.to_string());
        self.drafts.insert(path.into(), text.clone());
        let value = if field.schema["type"] == "string" {
            Value::String(text)
        } else if field.schema["type"]
            .as_array()
            .is_some_and(|t| t.iter().any(|v| v == "string"))
            && text != "null"
            && !text.starts_with('"')
        {
            Value::String(text)
        } else {
            serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?
        };
        let before = self.document.value().clone();
        self.document.set(path, value)?;
        self.remember(before);
        self.drafts.remove(path);
        Ok(())
    }
    pub fn surface(&self, visible: bool) -> Value {
        json!({"version":1,"surface_id":"newviso.content_manager.settings","source":"newviso.content-manager",
            "visible":visible,"modal":true,"z_order":1100,"title":"Project Settings",
            "subtitle":format!("{} · profile: {}",self.document.target(),self.document.report().profile.as_deref().unwrap_or("none")),
            "style":{"anchor":"center","min_size_px":[940,650],"max_size_px":[940,650]},
            "components":[self.inspector()]})
    }
    pub fn inspector(&self) -> Value {
        let doc = &self.document;
        let mut children = vec![];
        for (row, actions) in [
            vec![
                ("save", "Save Settings"),
                ("apply", "Apply Live"),
                ("close", "Back to Scene"),
            ],
            vec![
                ("undo", "Undo"),
                ("redo", "Redo"),
                ("revert", "Revert Saved"),
            ],
        ]
        .into_iter()
        .enumerate()
        {
            children.push(json!({"id":format!("settings.actions.{row}"),"component_id":"row","children":actions.into_iter().map(|(id,label)|button(&format!("settings.{id}"),label,&format!("content.settings.{id}"),Value::Null)).collect::<Vec<_>>()}));
        }
        children.push(json!({"id":"settings.search","component_id":"input","value":self.search,"props":{"state_path":"content.settings.search","placeholder":"Find setting"}}));
        if self.relative_path.is_none() {
            children.push(text(
                "settings.readonly",
                "Read only: runtime layer belongs to a shared/external mount",
            ));
        }
        if doc.restart_pending() {
            children.push(text(
                "settings.restart",
                "Restart required for changed window, persistence or startup settings",
            ));
        }
        let fields = doc.fields();
        let sections = fields
            .iter()
            .map(|f| f.section.as_str())
            .collect::<BTreeSet<_>>();
        let categories = sections
            .into_iter()
            .map(|s| {
                let mut b = button(
                    &format!("settings.category.{s}"),
                    &s.replace('_', " "),
                    "content.settings.section",
                    json!({"section":s}),
                );
                if s == self.section {
                    b["tone"] = json!("accent");
                }
                b
            })
            .collect::<Vec<_>>();
        let query = self.search.to_lowercase();
        let mut properties = vec![];
        for field in fields.iter().filter(|f| {
            if query.is_empty() {
                f.section == self.section
            } else {
                f.path.to_lowercase().contains(&query) || f.label.to_lowercase().contains(&query)
            }
        }) {
            properties.extend(self.field_nodes(field));
        }
        children.push(json!({"id":"settings.layout","component_id":"row","props":{"h_px":350,"align":"start"},"children":[
            {"id":"settings.categories","component_id":"column","props":{"fixed_width":180,"w_px":180,"h_px":350,"overflow":"auto","scroll_y":true,"max_height_px":350,"max_height":350},"children":categories},
            {"id":"settings.fields","component_id":"column","props":{"grow":1,"h_px":350,"overflow":"auto","scroll_y":true,"max_height_px":350,"max_height":350},"children":properties}
        ]}));
        json!({"id":"settings.inspector","component_id":"column","children":children})
    }
    fn field_nodes(&self, field: &RuntimeSettingField) -> Vec<Value> {
        let effective = self
            .document
            .effective()
            .pointer(&field.path)
            .unwrap_or(&Value::Null);
        let authored = self.document.value().pointer(&field.path);
        let value = authored.unwrap_or(effective);
        let raw = self.drafts.get(&field.path).cloned().unwrap_or_else(|| {
            value.as_str().map(str::to_owned).unwrap_or_else(|| {
                if field.schema["type"] == "number" {
                    value
                        .as_f64()
                        .map(|n| (n as f32).to_string())
                        .unwrap_or_else(|| value.to_string())
                } else if value.is_object() || value.is_array() {
                    serde_json::to_string_pretty(value).expect("settings JSON")
                } else {
                    value.to_string()
                }
            })
        });
        let source = self
            .document
            .report()
            .field_sources
            .get(&field.path)
            .cloned()
            .unwrap_or_else(|| {
                let prefix = format!("{}/", field.path);
                let sources = self
                    .document
                    .report()
                    .field_sources
                    .iter()
                    .filter(|(p, _)| p.starts_with(&prefix))
                    .map(|(_, s)| s.as_str())
                    .collect::<BTreeSet<_>>();
                sources.into_iter().collect::<Vec<_>>().join(", ")
            });
        let label = format!(
            "{} · {}",
            field.label,
            if field.live { "live" } else { "restart" }
        );
        let kind = if field.schema["type"] == "boolean" {
            "checkbox"
        } else if value.is_object() || value.is_array() {
            "code_editor"
        } else {
            "input"
        };
        let mut nodes = vec![
            json!({"id":format!("settings.field:{}",field.path),"component_id":kind,"text":label,"value":raw,"props":{"state_path":format!("content.settings.property:{}",field.path),"language":"json","h_px":if kind=="code_editor"{160}else{30},"placeholder":label}}),
            text(
                &format!("settings.source:{}", field.path),
                &format!(
                    "{} · source: {source}{}",
                    field.path,
                    if authored.is_some() {
                        " · project override"
                    } else {
                        " · inherited"
                    }
                ),
            ),
        ];
        if field.schema["type"] == "boolean" {
            nodes[0] = button(
                &format!("settings.field:{}", field.path),
                &format!("{label}: {}", value.as_bool().unwrap_or(false)),
                "content.settings.boolean",
                json!({"path":field.path,"value":!value.as_bool().unwrap_or(false)}),
            );
        }
        if authored.is_some() && value != effective {
            nodes.push(text(
                &format!("settings.effective:{}", field.path),
                &format!("Effective at launch: {effective}"),
            ));
        }
        if let Some(description) = field.schema["description"].as_str() {
            nodes.push(text(&format!("settings.help:{}", field.path), description));
        }
        if let Some(draft) = self.drafts.get(&field.path) {
            nodes.push(text(
                &format!("settings.invalid:{}", field.path),
                &format!("Invalid draft: {draft}"),
            ));
        }
        if authored.is_some() {
            nodes.push(button(
                &format!("settings.reset:{}", field.path),
                "Use inherited value",
                "content.settings.reset",
                json!({"path":field.path}),
            ));
        }
        nodes
    }
}
fn button(id: &str, label: &str, action: &str, payload: Value) -> Value {
    json!({"id":id,"component_id":"button","text":label,"action_id":action,"props":{"payload":payload}})
}
fn text(id: &str, label: &str) -> Value {
    json!({"id":id,"component_id":"text","text":label})
}

#[cfg(test)]
mod tests {
    use super::*;
    use newviso_project::{ProjectConfiguration, RuntimeConfigurationRequest};
    fn fixture() -> (PathBuf, ContentManager) {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("newviso-settings-{nonce}"));
        fs::create_dir_all(root.join("config")).unwrap();
        let layer = json!({"schema":"newviso.project.runtime.v1","window":{"width":900}});
        fs::write(root.join("config/runtime.json"), layer.to_string()).unwrap();
        let doc = RuntimeSettingsDocument::capture(
            &ProjectConfiguration::default(),
            "config/runtime.json",
            serde_json::from_str(include_str!("assets/settings-test.json")).unwrap(),
            "base",
            &RuntimeConfigurationRequest::default(),
            |_| Ok(layer.clone()),
        )
        .unwrap();
        let mut manager = ContentManager::open(&root).unwrap();
        manager
            .set_runtime_settings(doc, Some("config/runtime.json".into()))
            .unwrap();
        manager
            .apply_dispatch(&json!({"actions":[{"action_id":"content.settings.open"}]}))
            .unwrap();
        (root, manager)
    }
    fn patch(manager: &mut ContentManager, path: &str, value: &str) {
        manager.apply_dispatch(&json!({"state_patches":[{"changes":[{"path":format!("content.settings.property:{path}"),"value":value}]}]})).unwrap();
    }
    #[test]
    fn settings_dispatch_saves_only_project_layer_and_live_apply_keeps_restart_pending() {
        let (root, mut m) = fixture();
        patch(&mut m, "/streaming/parallel_loads", "6");
        patch(&mut m, "/window/width", "1600");
        let effects = m
            .apply_dispatch(&json!({"actions":[{"action_id":"content.settings.apply"}]}))
            .unwrap();
        match &effects[0] {
            ContentEffect::RuntimeSettingsApply { patch, expected } => {
                assert_eq!(patch, &json!({"streaming":{"parallel_loads":6}}));
                assert_eq!(expected["/streaming/parallel_loads"], 4);
            }
            _ => panic!("wrong effect"),
        }
        m.settings_applied();
        assert!(m
            .settings_editor
            .as_ref()
            .unwrap()
            .document
            .restart_pending());
        m.apply_dispatch(&json!({"actions":[{"action_id":"content.save"}]}))
            .unwrap();
        assert!(!m.is_dirty());
        let value: Value =
            serde_json::from_slice(&fs::read(root.join("config/runtime.json")).unwrap()).unwrap();
        assert_eq!(
            value,
            json!({"schema":"newviso.project.runtime.v1","window":{"width":1600},"streaming":{"parallel_loads":6}})
        );
        m.apply_dispatch(&json!({"actions":[{"action_id":"content.settings.undo"}]}))
            .unwrap();
        assert_eq!(
            m.settings_editor.as_ref().unwrap().document.value()["window"]["width"],
            900
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn settings_invalid_drafts_and_external_writes_are_preserved() {
        let (root, mut m) = fixture();
        let path = root.join("config/runtime.json");
        let before = fs::read(&path).unwrap();
        patch(&mut m, "/streaming/parallel_loads", "0");
        assert!(m.is_dirty());
        assert!(m
            .apply_dispatch(&json!({"actions":[{"action_id":"content.save"}]}))
            .is_err());
        assert!(m
            .apply_dispatch(&json!({"actions":[{"action_id":"content.settings.close"}]}))
            .is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        patch(&mut m, "/streaming/parallel_loads", "5");
        fs::write(&path, b"external edit").unwrap();
        assert!(m.save_settings().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"external edit");
        fs::remove_dir_all(root).unwrap();
    }
}
