//! Editor-owned context routing, including action-only legacy UI providers.
use super::*;
use serde_json::json;

impl ContentManager {
    /// Returns whether editor interaction currently owns camera navigation.
    pub fn complete_pointer_dispatch(
        &mut self,
        dispatch: &mut Value,
        input: &Value,
        viewport: [u32; 2],
    ) -> bool {
        if self.settings_visible {
            return true;
        }
        let Some(editor) = self.scene_editor.as_mut() else {
            return false;
        };
        let has = |field: &str, code: u64| {
            input[field]
                .as_array()
                .is_some_and(|v| v.iter().any(|n| n.as_u64() == Some(code)))
        };
        let pos = [
            input["mouse_pos"][0].as_f64().unwrap_or(0.0),
            input["mouse_pos"][1].as_f64().unwrap_or(0.0),
        ];
        let hovered = &dispatch["hovered_node"];
        let index = if hovered["surface_id"] == "newviso.content_manager.project" {
            hovered["node_id"]
                .as_str()
                .and_then(|id| {
                    id.strip_prefix("scene.entity.")
                        .or_else(|| id.strip_prefix("scene.more."))
                })
                .and_then(|s| s.parse::<usize>().ok())
        } else {
            None
        };
        // Older UI providers return actions without hover identities. Their context
        // menu operates on the explicitly selected object, just like the inspector.
        if has("mouse_pressed", 2) {
            editor.context_press = Some((index.unwrap_or_else(|| editor.selected_index()), pos));
        }
        if editor
            .context_press
            .is_some_and(|(_, start)| (pos[0] - start[0]).hypot(pos[1] - start[1]) > 6.0)
        {
            editor.context_press = None;
        }
        let open = if has("mouse_released", 2) {
            editor
                .context_press
                .take()
                .map(|p| p.0)
                .filter(|e| index.is_none_or(|i| i == *e))
        } else {
            None
        };
        let outside =
            editor.context_open() && has("mouse_pressed", 1) && !editor.context_contains(pos);
        let gate = editor.context_open() || editor.context_press.is_some() || open.is_some();
        let Some(object) = dispatch.as_object_mut() else {
            return gate;
        };
        let Some(actions) = object
            .entry("actions")
            .or_insert_with(|| json!([]))
            .as_array_mut()
        else {
            return gate;
        };
        if outside {
            actions.push(json!({"action_id":"content.scene.menu.close"}));
        }
        if let Some(entity) = open {
            if !actions
                .iter()
                .any(|a| a["action_id"] == "content.scene.menu.open")
            {
                actions.push(
                    json!({"action_id":"content.scene.menu.open","payload":{"entity":entity}}),
                );
            }
        }
        for action in actions
            .iter_mut()
            .filter(|a| a["action_id"] == "content.scene.menu.open")
        {
            if !action["payload"].is_object() {
                action["payload"] = json!({});
            }
            action["payload"]["position"] = json!(pos);
            action["payload"]["viewport"] = json!(viewport);
        }
        gate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_routing_requires_matching_press_release_and_deduplicates_provider_route() {
        let r = newviso_scene_document::SceneTypeRegistry::builtin();
        let scene = json!({"schema":"newviso.scene.v1","version":1,"entities":[r.types["camera"].template,r.types["cube"].template]});
        let root = std::env::temp_dir().join(format!("newviso-context-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let mut m = ContentManager::open(&root).unwrap();
        m.scene_editor = scene_editor::SceneEditor::parse(&scene.to_string()).unwrap();
        let hit = |index| json!({"hovered_node":{"surface_id":"newviso.content_manager.project","node_id":format!("scene.entity.{index}")},"actions":[]});
        m.complete_pointer_dispatch(&mut hit(0), &json!({"mouse_pressed":[2]}), [800, 600]);
        let mut release = hit(1);
        m.complete_pointer_dispatch(
            &mut release,
            &json!({"mouse_released":[2],"mouse_pos":[790,590]}),
            [800, 600],
        );
        assert!(release["actions"].as_array().unwrap().is_empty());
        m.complete_pointer_dispatch(
            &mut hit(1),
            &json!({"mouse_pressed":[2],"mouse_pos":[790,590]}),
            [800, 600],
        );
        let mut release = hit(1);
        m.complete_pointer_dispatch(
            &mut release,
            &json!({"mouse_released":[2],"mouse_pos":[790,590]}),
            [800, 600],
        );
        assert_eq!(release["actions"][0]["payload"]["entity"], 1);
        m.apply_dispatch(&release).unwrap();
        let menu = m.surfaces().pop().unwrap();
        assert_eq!(menu["metrics"]["overlay_x"], 500.0);
        assert_eq!(menu["metrics"]["overlay_y"], 220.0);
        let mut routed = hit(1);
        routed["actions"] = json!([{"action_id":"content.scene.menu.open","payload":{"entity":1}}]);
        m.complete_pointer_dispatch(
            &mut hit(1),
            &json!({"mouse_pressed":[2],"mouse_pos":[790,590]}),
            [800, 600],
        );
        m.complete_pointer_dispatch(
            &mut routed,
            &json!({"mouse_released":[2],"mouse_pos":[790,590]}),
            [800, 600],
        );
        assert_eq!(routed["actions"].as_array().unwrap().len(), 1);
        // Providers without hover still target the explicit selection; a drag cancels the menu.
        let mut no_hit = json!({"actions":[]});
        assert!(m.complete_pointer_dispatch(
            &mut no_hit,
            &json!({"mouse_pressed":[2],"mouse_pos":[20,20]}),
            [800, 600]
        ));
        m.complete_pointer_dispatch(
            &mut no_hit,
            &json!({"mouse_down":[2],"mouse_pos":[80,20]}),
            [800, 600],
        );
        let mut released = json!({"actions":[]});
        m.complete_pointer_dispatch(
            &mut released,
            &json!({"mouse_released":[2],"mouse_pos":[80,20]}),
            [800, 600],
        );
        assert!(released["actions"].as_array().unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
