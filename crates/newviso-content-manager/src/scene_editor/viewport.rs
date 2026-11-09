//! Authoring bridge; previews do not serialize or grow the document journal.
use super::*;
use newviso_editor_viewport::{add, rotate, sub, unrotate, Gizmo, Space, Tool, Transform};
#[derive(Clone, Debug)]
pub struct ViewportOptions {
    pub gizmo: Gizmo,
    pub focus_request: u64,
    pub preview_lighting: bool,
}
impl Default for ViewportOptions {
    fn default() -> Self {
        Self {
            gizmo: Gizmo::default(),
            focus_request: 0,
            preview_lighting: true,
        }
    }
}
#[derive(Debug, Default)]
pub(super) struct ViewportState {
    pub options: ViewportOptions,
    pub preview: Option<(u64, Transform)>,
    preview_published: Option<(u64, Transform)>,
    preview_published_at: Option<std::time::Instant>,
}
#[derive(Clone, Debug)]
pub enum ViewportVisual {
    Cube {
        color: [f32; 4],
    },
    Asset {
        reference: String,
        texture_dictionary: Option<String>,
    },
    Proxy,
    Unsupported,
}
fn visual(v: &Value) -> ViewportVisual {
    if v["kind"] != "mesh" {
        ViewportVisual::Proxy
    } else if v.pointer("/mesh/primitive").and_then(Value::as_str) == Some("cube") {
        ViewportVisual::Cube {
            color: std::array::from_fn(|i| {
                v["material"]["base_color"][i].as_f64().unwrap_or(1.0) as f32
            }),
        }
    } else if let Some(reference) = v
        .pointer("/mesh/asset")
        .or_else(|| v.pointer("/mesh/model"))
        .and_then(Value::as_str)
    {
        ViewportVisual::Asset {
            reference: reference.into(),
            texture_dictionary: v
                .pointer("/material/texture_dictionary")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }
    } else {
        ViewportVisual::Unsupported
    }
}
#[derive(Clone, Debug)]
pub struct ViewportEntity {
    pub identity: u64,
    pub transform: Transform,
    pub transformable: bool,
    pub visual: ViewportVisual,
}
fn vector(v: &Value, fallback: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| v[i].as_f64().unwrap_or(fallback[i]))
}
fn transform(e: &Value) -> Transform {
    Transform {
        position: vector(&e["transform"]["position"], [0.0; 3]),
        rotation_degrees: vector(&e["transform"]["rotation_degrees"], [0.0; 3]),
        scale: vector(&e["transform"]["scale"], [1.0; 3]),
    }
}
impl SceneEditor {
    pub(super) fn viewport_toolbar(&self) -> Value {
        let g = &self.viewport.options.gizmo;
        let b = |id: &str, label: &str, action: &str, payload: Value, active: bool| {
            let mut n = button(id, label, action, payload);
            if active {
                n["tone"] = json!("accent");
            }
            n
        };
        json!({"id":"viewport.tools","component_id":"column","children":[
         {"id":"viewport.modes","component_id":"row","children":[b("viewport.move","W Move","content.viewport.tool",json!({"tool":"translate"}),g.tool==Tool::Translate),b("viewport.rotate","E Rotate","content.viewport.tool",json!({"tool":"rotate"}),g.tool==Tool::Rotate),b("viewport.scale","R Scale","content.viewport.tool",json!({"tool":"scale"}),g.tool==Tool::Scale)]},
         {"id":"viewport.options","component_id":"row","children":[b("viewport.space",if g.space==Space::World{"World"}else{"Local"},"content.viewport.space",Value::Null,g.space==Space::Local),b("viewport.snap",if g.snap.enabled{"Snap ON"}else{"Snap OFF"},"content.viewport.snap",Value::Null,g.snap.enabled),button("viewport.focus","F Focus","content.viewport.focus",Value::Null),b("viewport.light",if self.viewport.options.preview_lighting{"Preview light"}else{"Scene light"},"content.viewport.light",Value::Null,self.viewport.options.preview_lighting)]},
         {"id":"viewport.steps","component_id":"row","children":[button("viewport.step.move",&format!("Move {} m",g.snap.translation),"content.viewport.step",json!({"kind":"translation"})),button("viewport.step.rotate",&format!("Rotate {}°",g.snap.rotation_degrees),"content.viewport.step",json!({"kind":"rotation"})),button("viewport.step.scale",&format!("Scale {}",g.snap.scale),"content.viewport.step",json!({"kind":"scale"}))]},
         {"id":"viewport.hint","component_id":"text","text":"Drag an axis / plane · center: free move or uniform scale · Ctrl: temporary snap · Esc: cancel"}
        ]})
    }
}
impl ContentManager {
    pub fn viewport_is_active_scene(&self) -> bool {
        self.project
            .as_ref()
            .is_none_or(|p| self.selected.as_deref() == Some(p.scene.as_str()))
    }
    pub fn viewport_arm_context(&mut self, pos: [f64; 2]) {
        if let Some(e) = self.scene_editor.as_mut() {
            if e.selection_active {
                e.context_press = Some((e.selected, pos));
            }
        }
    }
    pub fn viewport_document_key(&self) -> Option<(String, u64)> {
        Some((
            self.selected.clone()?,
            self.scene_editor.as_ref()?.document.revision(),
        ))
    }
    pub fn viewport_options(&self) -> Option<ViewportOptions> {
        self.scene_editor
            .as_ref()
            .map(|e| e.viewport.options.clone())
    }
    pub fn viewport_entities(&self) -> Vec<ViewportEntity> {
        self.scene_editor
            .as_ref()
            .map(|e| {
                e.document.value()["entities"]
                    .as_array()
                    .expect("validated scene")
                    .iter()
                    .enumerate()
                    .map(|(i, v)| ViewportEntity {
                        identity: e.document.entity_identity(i).expect("identity"),
                        visual: visual(v),
                        transform: transform(v),
                        transformable: v.pointer("/mesh/primitive").and_then(Value::as_str)
                            != Some("heightfield"),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn viewport_selection(&self) -> Option<ViewportEntity> {
        let e = self.scene_editor.as_ref()?;
        if !e.selection_active {
            return None;
        }
        let identity = e.document.entity_identity(e.selected)?;
        let v = &e.document.value()["entities"][e.selected];
        Some(ViewportEntity {
            identity,
            visual: visual(v),
            transform: e
                .viewport
                .preview
                .filter(|(id, _)| *id == identity)
                .map(|(_, t)| t)
                .unwrap_or_else(|| transform(v)),
            transformable: v.pointer("/mesh/primitive").and_then(Value::as_str)
                != Some("heightfield"),
        })
    }
    pub fn viewport_blocked(&self) -> bool {
        self.settings_visible
            || self
                .scene_editor
                .as_ref()
                .is_none_or(|e| e.context_open() || !e.drafts.is_empty())
    }
    pub fn viewport_cancel_context_gesture(&mut self) {
        if let Some(e) = self.scene_editor.as_mut() {
            e.context_press = None;
        }
    }
    pub fn viewport_select(&mut self, identity: Option<u64>) -> Result<(), String> {
        let e = self
            .scene_editor
            .as_mut()
            .ok_or("Open the active scene first")?;
        e.validate()?;
        if let Some(id) = identity {
            let index = e
                .document
                .index_for_identity(id)
                .ok_or("Selected object is no longer in the document")?;
            e.selected = index;
            e.selection_active = true;
            e.search.clear();
            e.page = index / 100;
        } else {
            e.selection_active = false;
        }
        e.context = None;
        e.context_press = None;
        self.invalidate_surfaces();
        Ok(())
    }
    pub fn viewport_preview(&mut self, identity: u64, t: Transform) -> Result<(), String> {
        if !t.valid() {
            return Err("Invalid viewport transform".into());
        }
        let e = self.scene_editor.as_mut().ok_or("No scene document")?;
        if e.document.entity_identity(e.selected) != Some(identity) || !e.drafts.is_empty() {
            return Err("Selection changed during the viewport drag".into());
        }
        let sample = (identity, t);
        e.viewport.preview = Some(sample);
        // Native geometry follows every mouse sample. Rebuilding retained
        // inspector/outliner/content surfaces is bounded to ~15 Hz during a drag.
        let changed = e.viewport.preview_published != Some(sample);
        let due = e
            .viewport
            .preview_published_at
            .is_none_or(|at| at.elapsed() >= std::time::Duration::from_millis(66));
        if changed && due {
            e.viewport.preview_published = Some(sample);
            e.viewport.preview_published_at = Some(std::time::Instant::now());
            self.invalidate_surfaces();
        }
        Ok(())
    }
    pub fn viewport_cancel_preview(&mut self) {
        if let Some(e) = self.scene_editor.as_mut() {
            e.viewport.preview = None;
            e.viewport.preview_published = None;
            e.viewport.preview_published_at = None;
        }
        self.invalidate_surfaces();
    }
    pub fn viewport_commit(&mut self, identity: u64, t: Transform) -> Result<(), String> {
        if !t.valid() {
            return Err("Invalid viewport transform".into());
        }
        let e = self.scene_editor.as_mut().ok_or("No scene document")?;
        let i = e
            .document
            .index_for_identity(identity)
            .ok_or("Dragged object was removed")?;
        let record = &e.document.value()["entities"][i];
        let original = transform(record);
        if t == original {
            e.viewport.preview = None;
            e.viewport.preview_published = None;
            e.viewport.preview_published_at = None;
            self.invalidate_surfaces();
            return Ok(());
        }

        let mut edits = vec![
            SceneEdit::Set {
                entity: i,
                path: "/transform/position".into(),
                value: json!(t.position),
            },
            SceneEdit::Set {
                entity: i,
                path: "/transform/rotation_degrees".into(),
                value: json!(t.rotation_degrees),
            },
            SceneEdit::Set {
                entity: i,
                path: "/transform/scale".into(),
                value: json!(t.scale),
            },
        ];
        if record["kind"] == "camera" {
            let delta = |v| rotate(unrotate(v, original.rotation_degrees), t.rotation_degrees);
            let target = add(
                t.position,
                delta(sub(
                    vector(
                        &record["transform"]["target"],
                        add(original.position, [0.0, 0.0, -1.0]),
                    ),
                    original.position,
                )),
            );
            let up = delta(vector(&record["transform"]["up"], [0.0, 1.0, 0.0]));
            edits.push(SceneEdit::Set {
                entity: i,
                path: "/transform/target".into(),
                value: json!(target),
            });
            edits.push(SceneEdit::Set {
                entity: i,
                path: "/transform/up".into(),
                value: json!(up),
            });
        }
        let result = e
            .document
            .apply(e.document.revision(), &edits, SceneTypeRegistry::builtin());
        e.viewport.preview = None;
        e.viewport.preview_published = None;
        e.viewport.preview_published_at = None;
        result?;
        e.sync(&mut self.editor_text);
        self.status = "Transform committed · Ctrl+Z undoes the entire drag · Ctrl+S saves".into();
        self.invalidate_surfaces();
        Ok(())
    }
    pub fn viewport_action(&mut self, id: &str, payload: &Value) -> Result<(), String> {
        let e = self.scene_editor.as_mut().ok_or("No scene document")?;
        if e.viewport.preview.is_some() {
            return Ok(());
        }
        let g = &mut e.viewport.options.gizmo;
        match id {
            "content.viewport.light" => {
                e.viewport.options.preview_lighting = !e.viewport.options.preview_lighting
            }
            "content.viewport.tool" => {
                g.tool =
                    serde_json::from_value(payload["tool"].clone()).map_err(|e| e.to_string())?
            }
            "content.viewport.cycle" => {
                g.tool = match g.tool {
                    Tool::Translate => Tool::Rotate,
                    Tool::Rotate => Tool::Scale,
                    Tool::Scale => Tool::Translate,
                }
            }
            "content.viewport.space" => {
                g.space = if g.space == Space::World {
                    Space::Local
                } else {
                    Space::World
                }
            }
            "content.viewport.snap" => g.snap.enabled = !g.snap.enabled,
            "content.viewport.focus" => {
                e.viewport.options.focus_request = e.viewport.options.focus_request.wrapping_add(1)
            }
            "content.viewport.step" => {
                let cycle = |v: f64, steps: &[f64]| {
                    *steps.iter().find(|n| **n > v + 1e-9).unwrap_or(&steps[0])
                };
                match payload["kind"].as_str() {
                    Some("translation") => {
                        g.snap.translation =
                            cycle(g.snap.translation, &[0.01, 0.1, 0.5, 1.0, 5.0, 10.0, 100.0])
                    }
                    Some("rotation") => {
                        g.snap.rotation_degrees =
                            cycle(g.snap.rotation_degrees, &[1.0, 5.0, 15.0, 30.0, 45.0, 90.0])
                    }
                    Some("scale") => {
                        g.snap.scale = cycle(g.snap.scale, &[0.01, 0.05, 0.1, 0.25, 0.5, 1.0])
                    }
                    _ => return Err("Unknown snap step".into()),
                }
            }
            _ => return Err("Unknown viewport action".into()),
        }
        self.invalidate_surfaces();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manager() -> (PathBuf, ContentManager) {
        let root = std::env::temp_dir().join(format!(
            "newviso-viewport-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("scenes")).unwrap();
        let registry = SceneTypeRegistry::builtin();
        let scene = json!({"schema":"newviso.scene.v1","version":1,"entities":[registry.types["camera"].template,registry.types["cube"].template]});
        fs::write(root.join("scenes/main.json"), scene.to_string()).unwrap();
        fs::write(root.join("notes.txt"), "notes").unwrap();
        let mut m = ContentManager::open(&root).unwrap();
        m.select("scenes/main.json").unwrap();
        m.viewport_select(Some(2)).unwrap();
        (root, m)
    }
    #[test]
    fn preview_cannot_be_saved_and_commit_is_one_undo_with_exact_cancel() {
        let (root, mut m) = manager();
        let original = m.viewport_selection().unwrap().transform;
        let moved = Transform {
            position: [3.0, 2.0, 1.0],
            rotation_degrees: [20.0, 35.0, 70.0],
            scale: [2.0, 1.0, 0.5],
        };
        let rev = m.viewport_document_key().unwrap().1;
        m.viewport_preview(2, moved).unwrap();
        let ui_revision = m.surface_revision();
        for _ in 0..12 {
            m.viewport_preview(2, moved).unwrap();
        }
        assert_eq!(m.surface_revision(), ui_revision);
        assert_eq!(m.viewport_document_key().unwrap().1, rev);
        assert!(m.save().is_err());
        m.viewport_cancel_preview();
        assert_eq!(m.viewport_selection().unwrap().transform, original);
        assert!(!m.is_dirty());
        m.viewport_preview(2, moved).unwrap();
        m.viewport_commit(2, moved).unwrap();
        assert_eq!(m.viewport_document_key().unwrap().1, rev + 1);
        m.apply_dispatch(&json!({"actions":[{"action_id":"content.scene.undo"}]}))
            .unwrap();
        assert_eq!(m.viewport_selection().unwrap().transform, original);
        m.apply_dispatch(&json!({"actions":[{"action_id":"content.scene.redo"}]}))
            .unwrap();
        assert_eq!(m.viewport_selection().unwrap().transform, moved);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn saved_scene_keeps_its_session_when_browsing_other_assets() {
        let (root, mut m) = manager();
        m.apply_dispatch(&json!({"actions":[{"action_id":"content.scene.duplicate"}]}))
            .unwrap();
        let identities = m
            .viewport_entities()
            .iter()
            .map(|e| e.identity)
            .collect::<Vec<_>>();
        m.save().unwrap();
        m.select("notes.txt").unwrap();
        m.select("scenes/main.json").unwrap();
        assert_eq!(
            m.viewport_entities()
                .iter()
                .map(|e| e.identity)
                .collect::<Vec<_>>(),
            identities
        );
        assert!(m.scene_editor.as_ref().unwrap().document.can_undo());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn clicking_a_gizmo_without_moving_does_not_dirty_or_add_history() {
        let (root, mut m) = manager();
        let original = m.viewport_selection().unwrap().transform;
        let key = m.viewport_document_key();
        m.viewport_preview(2, original).unwrap();
        m.viewport_commit(2, original).unwrap();
        assert_eq!(m.viewport_document_key(), key);
        assert!(!m.is_dirty());
        assert!(!m.scene_editor.as_ref().unwrap().document.can_undo());
        fs::remove_dir_all(root).unwrap();
    }
}
