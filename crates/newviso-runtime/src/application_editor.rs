//! Explicit editor mode only. Game input and first-person cameras never enter here.
use super::*;
use newviso_content_manager::ContentManager;
use newviso_editor_viewport::*;
use newviso_input_client::key;
mod draw;
#[derive(Default)]
pub(super) struct EditorRuntime {
    document: Option<(String, u64)>,
    bindings: BTreeMap<u64, u64>,
    applied: BTreeMap<u64, Transform>,
    drag: Option<(u64, u64, Drag)>,
    gizmo: Gizmo,
    hover: Option<Handle>,
    navigation: Option<u64>,
    fly_active: bool,
    navigation_travel: f64,
    viewport_focus: bool,
    last_pos: Option<[f64; 2]>,
    speed: f64,
    focus_request: u64,
    ui_test: Option<([f64; 2], [u32; 2], u64, bool)>,
    original_environment: Option<newviso_scene::SceneEnvironmentDesc>,
    preview_light: Option<bool>,
    preview_lights: Vec<u64>,
}
fn camera(scene: &Scene3dRuntime, size: [u32; 2]) -> Camera {
    let c = scene.authoring_camera();
    Camera {
        position: c.position.map(f64::from),
        target: c.target.map(f64::from),
        up: c.up.map(f64::from),
        fov_y_degrees: c.fov_y_degrees as f64,
        near: c.near as f64,
        far: c.far as f64,
        size,
    }
}
fn bounds(scene: &Scene3dRuntime, id: u64) -> Option<Bounds> {
    scene.authoring_bounds(id).map(|(min, max)| Bounds {
        min: min.map(f64::from),
        max: max.map(f64::from),
    })
}
fn apply(scene: &mut Scene3dRuntime, id: u64, t: Transform) -> Result<(), String> {
    scene.set_authoring_transform(
        id,
        t.position.map(|v| v as f32),
        t.rotation_degrees.map(|v| v as f32),
        t.scale.map(|v| v as f32),
    )
}
fn report(m: &mut ContentManager, error: String) {
    host::warn("newviso.editor", &error);
    m.report_status(error);
}
impl EditorRuntime {
    pub(super) fn capture_cursor(&self) -> bool {
        self.fly_active && self.navigation == Some(2)
    }
    pub(super) fn release_navigation(&mut self) {
        self.navigation = None;
        self.fly_active = false;
        self.navigation_travel = 0.0;
    }
    fn synchronize(
        &mut self,
        scene: &mut Scene3dRuntime,
        m: &ContentManager,
    ) -> Result<(), String> {
        let key = m.viewport_document_key();
        if self.document == key {
            return Ok(());
        }
        if key.is_none() || !m.viewport_is_active_scene() {
            self.document = key;
            return Ok(());
        }
        if self.document.as_ref().map(|k| &k.0) != key.as_ref().map(|k| &k.0) {
            self.bindings.clear();
            self.applied.clear();
            for e in m.viewport_entities() {
                if let Some(id) = scene.authoring_binding(e.identity.saturating_sub(1) as usize) {
                    self.bindings.insert(e.identity, id);
                }
            }
        }
        let entities = m.viewport_entities();
        for e in &entities {
            if !self.bindings.contains_key(&e.identity) {
                use newviso_content_manager::ViewportVisual;
                let mut desc = newviso_scene::SceneRuntimeEntityDesc {
                    position: e.transform.position.map(|v| v as f32),
                    rotation_degrees: e.transform.rotation_degrees.map(|v| v as f32),
                    scale: e.transform.scale.map(|v| v as f32),
                    ..Default::default()
                };
                match &e.visual {
                    ViewportVisual::Cube { color } => {
                        desc.visual = newviso_scene::SceneRuntimeVisualKind::Cube;
                        desc.base_color = *color;
                    }
                    ViewportVisual::Asset {
                        reference,
                        texture_dictionary,
                    } => {
                        desc.asset_ref = Some(reference.clone());
                        desc.texture_dictionary = texture_dictionary.clone();
                    }
                    ViewportVisual::Proxy => {
                        desc.bounds_half_extent = [0.2; 3];
                    }
                    ViewportVisual::Unsupported => continue,
                }
                let id = scene.spawn_authoring_entity(e.identity, desc)?;
                self.bindings.insert(e.identity, id);
                self.applied.insert(e.identity, e.transform);
            }
        }
        let present = entities.iter().map(|e| e.identity).collect::<BTreeSet<_>>();
        for (identity, id) in &self.bindings {
            scene.set_entity_visibility(*id, "editor_document", present.contains(identity))?;
        }
        for e in entities {
            if let Some(id) = self.bindings.get(&e.identity).copied() {
                if e.transformable && self.applied.get(&e.identity) != Some(&e.transform) {
                    apply(scene, id, e.transform)?;
                    self.applied.insert(e.identity, e.transform);
                }
            }
        }
        self.document = key;
        Ok(())
    }
    pub(super) fn step(
        &mut self,
        scene: &mut Scene3dRuntime,
        m: &mut ContentManager,
        input: &InputSnapshot,
        ui: &newviso_ui_client::UiFrameOutput,
        size: [u32; 2],
        dpi: f32,
        dt: f32,
    ) -> Vec<Value> {
        let mut actions = vec![];
        if !m.viewport_is_active_scene() {
            self.release_navigation();
            return actions;
        }
        if let Err(e) = self.synchronize(scene, m) {
            self.release_navigation();
            report(m, e);
            return actions;
        }
        let frame = input.ui_input_frame();
        let pos = [
            frame["mouse_pos"][0].as_f64().unwrap_or(0.0),
            frame["mouse_pos"][1].as_f64().unwrap_or(0.0),
        ];
        let delta = self
            .last_pos
            .map(|p| [pos[0] - p[0], pos[1] - p[1]])
            .unwrap_or([0.0; 2]);
        self.last_pos = Some(pos);
        let has = |field: &str, code: u64| {
            frame[field]
                .as_array()
                .is_some_and(|v| v.iter().any(|n| n.as_u64() == Some(code)))
        };
        let ctrl = input.key_down(key::CONTROL_LEFT) || input.key_down(key::CONTROL_RIGHT);
        let shift = input.key_down(key::SHIFT_LEFT) || input.key_down(key::SHIFT_RIGHT);
        let alt = input.key_down(key::ALT_LEFT) || input.key_down(key::ALT_RIGHT);
        let mut c = camera(scene, size);
        let options = match m.viewport_options() {
            Some(o) => o,
            None => return actions,
        };
        if let Err(e) = self.lighting(scene, options.preview_lighting) {
            report(m, e);
        }
        self.gizmo = options.gizmo;
        self.gizmo.dpi = f64::from(dpi.max(0.25));
        if let Some((identity, id, mut drag)) = self.drag.take() {
            self.gizmo.active = Some(drag.handle);
            if input.key_pressed(key::ESCAPE) {
                let _ = apply(scene, id, drag.original);
                m.viewport_cancel_preview();
                self.applied.insert(identity, drag.original);
                self.gizmo.active = None;
                host::info("newviso.editor", "viewport drag cancelled");
            } else if has("mouse_down", 1) {
                let mut snap = self.gizmo.snap;
                snap.enabled |= ctrl;
                let previous = drag.current;
                let next = drag.update(pos, snap);
                let update = if next != previous {
                    apply(scene, id, next)
                } else {
                    Ok(())
                };
                if let Err(e) = update.and_then(|_| m.viewport_preview(identity, next)) {
                    let _ = apply(scene, id, drag.original);
                    m.viewport_cancel_preview();
                    report(m, e);
                    self.gizmo.active = None;
                } else {
                    self.drag = Some((identity, id, drag));
                }
            } else {
                let current = drag.current;
                if let Err(e) = m.viewport_commit(identity, current) {
                    let _ = apply(scene, id, drag.original);
                    m.viewport_cancel_preview();
                    report(m, e);
                } else {
                    self.applied.insert(identity, current);
                    host::info("newviso.editor",format!("viewport transform committed identity={identity} position={:?} rotation={:?} scale={:?}",current.position,current.rotation_degrees,current.scale));
                }
                self.gizmo.active = None;
            }
            return actions;
        }
        self.gizmo.active = None;
        let inside =
            pos[0] >= 0.0 && pos[1] >= 0.0 && pos[0] < size[0] as f64 && pos[1] < size[1] as f64;
        let revision = m.surface_revision();
        let ui_hit = if let Some((p, s, r, hit)) = self.ui_test {
            if p == pos && s == size && r == revision {
                hit
            } else {
                inside && draw::ui_hit(&ui.draw_list, pos, dpi)
            }
        } else {
            inside && draw::ui_hit(&ui.draw_list, pos, dpi)
        };
        self.ui_test = Some((pos, size, revision, ui_hit));
        let blocked = m.viewport_blocked();
        let available = inside && !ui_hit && !blocked;
        if available && has("mouse_pressed", 2) {
            let ray = c.ray(pos);
            let bindings = self
                .bindings
                .iter()
                .map(|(a, b)| (*a, *b))
                .collect::<Vec<_>>();
            let pick = scene.authoring_pick(
                &bindings,
                ray.origin.map(|v| v as f32),
                ray.direction.map(|v| v as f32),
            );
            if pick.is_some() {
                if let Err(e) = m.viewport_select(pick) {
                    report(m, e);
                } else {
                    m.viewport_arm_context(pos);
                }
            } else {
                m.viewport_cancel_context_gesture();
            }
        }
        for button in [1, 2, 3] {
            if has("mouse_pressed", button) {
                self.viewport_focus = available;
                if available && button != 1 {
                    self.navigation = Some(button);
                    self.fly_active = false;
                    self.navigation_travel = 0.0;
                }
            }
        }
        if blocked {
            self.release_navigation();
            self.hover = None;
            return actions;
        }
        if options.focus_request != self.focus_request {
            self.focus_request = options.focus_request;
            if let Some(selected) = m
                .viewport_selection()
                .and_then(|e| self.bindings.get(&e.identity))
                .and_then(|id| bounds(scene, *id))
            {
                let rect = draw::viewport_rect(&ui.draw_list, size, dpi);
                c.focus_in_rect(selected, rect);
                host::debug("newviso.editor", format!("viewport focused rect={rect:?}"));
                self.set_camera(scene, m, c);
            }
        }
        if input.key_pressed(key::ESCAPE) && self.navigation.is_some() {
            self.release_navigation();
            m.viewport_cancel_context_gesture();
            return actions;
        }
        let mut navigated = false;
        let wheel = input.mouse_wheel_y() as f64;
        if self.navigation == Some(2) && has("mouse_down", 2) {
            let movement = [
                (if input.key_down(key::D) { 1.0 } else { 0.0 })
                    - (if input.key_down(key::A) { 1.0 } else { 0.0 }),
                (if input.key_down(key::E) { 1.0 } else { 0.0 })
                    - (if input.key_down(key::Q) { 1.0 } else { 0.0 }),
                (if input.key_down(key::W) { 1.0 } else { 0.0 })
                    - (if input.key_down(key::S) { 1.0 } else { 0.0 }),
            ];
            if self.speed <= 0.0 {
                self.speed = 8.0;
            }
            if wheel != 0.0 {
                self.speed = (self.speed * (wheel * 0.15).exp()).clamp(0.1, 10000.0);
            }
            let raw = input.mouse_delta().map(f64::from);
            let raw = if has("mouse_pressed", 2) {
                [0.0; 2]
            } else {
                raw
            };
            self.navigation_travel += raw[0].hypot(raw[1]);
            self.fly_active |= self.navigation_travel > 6.0 || movement != [0.0; 3];
            if self.fly_active {
                m.viewport_cancel_context_gesture();
                c.fly(
                    raw,
                    movement,
                    dt as f64,
                    self.speed * if shift { 4.0 } else { 1.0 },
                );
                navigated = true;
            }
        } else if self.navigation == Some(3) && has("mouse_down", 3) {
            let distance = m
                .viewport_selection()
                .and_then(|e| self.bindings.get(&e.identity))
                .and_then(|id| bounds(scene, *id))
                .map(|b| length(sub(b.center(), c.position)))
                .unwrap_or(length(sub(c.target, c.position)).max(5.0));
            c.pan(delta, distance);
            navigated = delta != [0.0; 2];
        }
        if has("mouse_pressed", 1) && available && alt {
            self.navigation = Some(1);
        }
        if self.navigation == Some(1) && has("mouse_down", 1) && alt {
            let pivot = m
                .viewport_selection()
                .and_then(|e| self.bindings.get(&e.identity))
                .and_then(|id| bounds(scene, *id))
                .map(Bounds::center)
                .unwrap_or(c.target);
            c.orbit(pivot, delta);
            navigated = delta != [0.0; 2];
        }
        if self
            .navigation
            .is_some_and(|button| !has("mouse_down", button))
        {
            if self.fly_active {
                host::info(
                    "newviso.editor",
                    format!(
                        "viewport flight ended position={:?} target={:?}",
                        c.position, c.target
                    ),
                );
            }
            self.release_navigation();
        }
        if available && self.navigation.is_none() && wheel != 0.0 {
            c.dolly(wheel);
            navigated = true;
        }
        if navigated {
            self.set_camera(scene, m, c);
        }
        if self.navigation.is_some() || navigated {
            return actions;
        }
        if self.viewport_focus && !ui_hit {
            if input.key_pressed(key::ESCAPE) {
                let _ = m.viewport_select(None);
            } else if ctrl {
                let id = if input.key_pressed(key::Z) {
                    Some(if shift {
                        "content.scene.redo"
                    } else {
                        "content.scene.undo"
                    })
                } else if input.key_pressed(key::Y) {
                    Some("content.scene.redo")
                } else {
                    None
                };
                if let Some(id) = id {
                    actions.push(json!({"action_id":id}));
                }
            } else {
                for (code, tool) in [(key::W, "translate"), (key::E, "rotate"), (key::R, "scale")] {
                    if input.key_pressed(code) {
                        let _ = m.viewport_action("content.viewport.tool", &json!({"tool":tool}));
                    }
                }
                if input.key_pressed(key::SPACE) {
                    let _ = m.viewport_action("content.viewport.cycle", &Value::Null);
                }
                if input.key_pressed(key::F) {
                    let _ = m.viewport_action("content.viewport.focus", &Value::Null);
                }
            }
        }
        let selected = m.viewport_selection();
        self.hover = if available {
            selected.as_ref().filter(|e| e.transformable).and_then(|e| {
                self.bindings
                    .contains_key(&e.identity)
                    .then(|| self.gizmo.hit(c, e.transform, pos))
                    .flatten()
            })
        } else {
            None
        };
        if available && has("mouse_pressed", 1) && !alt {
            if let Some(e) = selected {
                if let (Some(handle), Some(id)) =
                    (self.hover, self.bindings.get(&e.identity).copied())
                {
                    if let Some(drag) = self.gizmo.begin(c, e.transform, pos, handle) {
                        self.drag = Some((e.identity, id, drag));
                        let _ = m.viewport_preview(e.identity, e.transform);
                        return actions;
                    }
                }
            }
            let ray = c.ray(pos);
            let bindings = self
                .bindings
                .iter()
                .map(|(a, b)| (*a, *b))
                .collect::<Vec<_>>();
            let pick = scene.authoring_pick(
                &bindings,
                ray.origin.map(|v| v as f32),
                ray.direction.map(|v| v as f32),
            );
            if let Err(e) = m.viewport_select(pick) {
                report(m, e);
            } else {
                host::info(
                    "newviso.editor",
                    format!("viewport selection identity={pick:?}"),
                );
                if let Some(e) = m.viewport_selection() {
                    if !e.transformable {
                        m.report_status("Heightfield selected. Its baked world grid is edited through terrain source properties; mesh gizmos are disabled.");
                    }
                }
            }
        }
        actions
    }
    fn lighting(&mut self, scene: &mut Scene3dRuntime, enabled: bool) -> Result<(), String> {
        if self.preview_light == Some(enabled) {
            return Ok(());
        }
        let original = *self
            .original_environment
            .get_or_insert_with(|| scene.scene_environment());
        if enabled {
            let mut environment = original;
            environment.ambient_color = [1.0; 3];
            environment.ambient_intensity = original.ambient_intensity.max(0.55);
            scene.set_scene_environment(environment)?;
            if self.preview_lights.is_empty() {
                for (index, rotation, intensity) in
                    [(0, [45.0, -35.0, 0.0], 1.6), (1, [-35.0, 135.0, 0.0], 0.65)]
                {
                    let key = format!("newviso.editor.preview_light.{index}");
                    let id = scene.upsert_runtime_light(
                        &key,
                        newviso_scene::SceneLightDesc {
                            light_type: newviso_scene::SceneLightType::Directional,
                            intensity,
                            ..Default::default()
                        },
                    )?;
                    scene.set_entity_transform(id, [0.0; 3], rotation, [1.0; 3])?;
                    self.preview_lights.push(id);
                }
            }
        } else {
            scene.set_scene_environment(original)?;
        }
        for id in &self.preview_lights {
            scene.set_entity_visibility(*id, "editor_preview", enabled)?;
        }
        self.preview_light = Some(enabled);
        Ok(())
    }
    fn set_camera(&self, scene: &mut Scene3dRuntime, m: &mut ContentManager, c: Camera) {
        if let Err(e) = scene.set_camera_pose(
            c.position.map(|v| v as f32),
            c.target.map(|v| v as f32),
            Some(c.up.map(|v| v as f32)),
            None,
        ) {
            report(m, e);
        }
    }
    pub(super) fn overlay(
        &self,
        scene: &Scene3dRuntime,
        m: &ContentManager,
        draw: &mut Value,
        size: [u32; 2],
        dpi: f32,
    ) -> Result<(), String> {
        if !m.viewport_is_active_scene() {
            return Ok(());
        }
        let Some(e) = m.viewport_selection() else {
            return Ok(());
        };
        let Some(id) = self.bindings.get(&e.identity) else {
            return Ok(());
        };
        let c = camera(scene, size);
        let mut lines = bounds(scene, *id)
            .map(|b| bounds_lines(c, b))
            .unwrap_or_default();
        if e.transformable {
            lines.extend(
                self.gizmo
                    .geometry(c, e.transform, self.hover)
                    .into_iter()
                    .map(|(line, _)| line),
            );
        }
        draw::prepend_lines(draw, &lines, size, dpi)
    }
}

/// Save belongs to the editor document even while the inspector owns keyboard
/// focus. Text-field undo remains provider-owned; viewport undo is separately gated.
pub(super) fn complete_global_shortcuts(dispatch: &mut Value, input: &InputSnapshot) {
    if !(input.key_down(key::CONTROL_LEFT) || input.key_down(key::CONTROL_RIGHT))
        || !input.key_pressed(key::S)
    {
        return;
    }
    let Some(object) = dispatch.as_object_mut() else {
        return;
    };
    let Some(actions) = object
        .entry("actions")
        .or_insert_with(|| json!([]))
        .as_array_mut()
    else {
        return;
    };
    if !actions.iter().any(|a| a["action_id"] == "content.save") {
        actions.push(json!({"action_id":"content.save"}));
    }
}
#[cfg(test)]
mod shortcut_tests {
    use super::*;
    #[test]
    fn native_control_save_works_without_viewport_focus_and_deduplicates_provider_action() {
        let input = InputSnapshot {
            state: json!({"keys":{"down":[62,37],"pressed":[37]}}),
            text: String::new(),
            ime_commit: String::new(),
        };
        let mut dispatch =
            json!({"actions":[],"focused_node":{"surface_id":"inspector","node_id":"position"}});
        complete_global_shortcuts(&mut dispatch, &input);
        complete_global_shortcuts(&mut dispatch, &input);
        assert_eq!(dispatch["actions"], json!([{"action_id":"content.save"}]));
    }
}
