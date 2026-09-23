use newviso_assets_client::AssetClient;
pub use newviso_script_client::ScriptPermission;
use newviso_script_client::{
    ScriptClient, ScriptInvocation, ScriptModuleLoad, ScriptResponseStatus,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct ScriptModuleSpec {
    pub asset: String,
    pub on_start: Option<String>,
    pub on_frame: Option<String>,
    pub on_shutdown: Option<String>,
    pub permissions: Vec<ScriptPermission>,
}

/// Generic script output collected across all project modules.
///
/// Commands are deliberately opaque to the scripting layer. The runtime routes
/// them to engine capabilities; this crate does not know gameplay concepts.
#[derive(Clone, Debug, Default)]
pub struct ScriptControl {
    pub exit_requested: bool,
    pub ui_bindings: BTreeMap<String, Value>,
    pub commands: Vec<Value>,
}

#[derive(Debug)]
struct LoadedModule {
    spec: ScriptModuleSpec,
}

#[derive(Debug)]
pub struct ScriptRuntime {
    modules: Vec<LoadedModule>,
    request_counter: u64,
}

impl ScriptRuntime {
    pub fn load(modules: Vec<ScriptModuleSpec>) -> Result<Self, String> {
        let assets = AssetClient::new();
        let scripting = ScriptClient::new();
        let mut loaded = Vec::with_capacity(modules.len());

        for spec in modules {
            let module_bytes = assets.raw_bytes(&spec.asset)?;
            scripting.load_module(&ScriptModuleLoad {
                reference: &spec.asset,
                module_bytes: &module_bytes,
                permissions: &spec.permissions,
                metadata: BTreeMap::from([
                    ("source".to_owned(), "newviso-project-vfs".to_owned()),
                    ("logical_path".to_owned(), spec.asset.clone()),
                ]),
            })?;
            loaded.push(LoadedModule { spec });
        }

        Ok(Self {
            modules: loaded,
            request_counter: 0,
        })
    }

    /// Rebuilds the complete script graph after any project script asset changes.
    ///
    /// Child imports are owned by the scripting provider, so NewViso intentionally
    /// does not parse or track the dependency graph. Reloading the root lets the
    /// provider resolve the new graph through engine.assets/VFS transactionally.
    pub fn reload_graph_after_asset_change(
        &mut self,
        changed_reference: &str,
    ) -> Result<(), String> {
        let assets = AssetClient::new();
        let scripting = ScriptClient::new();

        for module in &self.modules {
            let spec = &module.spec;
            let module_bytes = assets.raw_bytes(&spec.asset)?;
            scripting.load_module(&ScriptModuleLoad {
                reference: &spec.asset,
                module_bytes: &module_bytes,
                permissions: &spec.permissions,
                metadata: BTreeMap::from([
                    ("source".to_owned(), "newviso-content-manager".to_owned()),
                    ("logical_path".to_owned(), spec.asset.clone()),
                    ("reload".to_owned(), "true".to_owned()),
                    ("changed_asset".to_owned(), changed_reference.to_owned()),
                ]),
            })?;
        }

        Ok(())
    }

    pub fn start(&mut self, project_context: &Value) -> Result<ScriptControl, String> {
        let payload = json!({
            "event": "start",
            "project": project_context
        });
        self.invoke_lifecycle("start", &payload, false)
    }

    /// Runs one project-script frame from engine-neutral snapshots.
    pub fn frame(
        &mut self,
        delta_seconds: f32,
        elapsed_seconds: f64,
        project_context: &Value,
        runtime_state: &Value,
        frame_context: &Value,
    ) -> Result<ScriptControl, String> {
        let payload = json!({
            "event": "frame",
            "delta_seconds": delta_seconds,
            "elapsed_seconds": elapsed_seconds,
            "project": project_context,
            "runtime": runtime_state,
            "frame": frame_context
        });
        self.invoke_lifecycle("frame", &payload, true)
    }

    pub fn shutdown(&mut self, project_context: &Value) -> Result<ScriptControl, String> {
        let payload = json!({
            "event": "shutdown",
            "project": project_context
        });
        self.invoke_lifecycle("shutdown", &payload, false)
    }

    fn invoke_lifecycle(
        &mut self,
        phase: &str,
        payload: &Value,
        frame: bool,
    ) -> Result<ScriptControl, String> {
        let client = ScriptClient::new();
        let mut control = ScriptControl::default();

        for module_index in 0..self.modules.len() {
            let operation = {
                let spec = &self.modules[module_index].spec;
                match phase {
                    "start" => spec.on_start.as_deref(),
                    "frame" => spec.on_frame.as_deref(),
                    "shutdown" => spec.on_shutdown.as_deref(),
                    _ => None,
                }
                .map(str::to_owned)
            };

            let Some(operation) = operation else {
                continue;
            };

            self.request_counter = self.request_counter.wrapping_add(1);
            let request_id = format!("newviso-{phase}-{}", self.request_counter);
            let spec = &self.modules[module_index].spec;

            let request = ScriptInvocation {
                request_id: &request_id,
                script_ref: &spec.asset,
                operation: &operation,
                payload,
                context_bytes: &[],
                permissions: &spec.permissions,
                metadata: BTreeMap::from([
                    ("phase".to_owned(), phase.to_owned()),
                    ("payload_format".to_owned(), "json".to_owned()),
                ]),
            };

            let response = if frame {
                client.frame(&request)?
            } else {
                client.invoke(&request)?
            };

            match response.status {
                ScriptResponseStatus::Ok | ScriptResponseStatus::Empty => {}
                other => {
                    return Err(format!(
                        "script '{}' operation '{}' returned status {:?}",
                        spec.asset, operation, other
                    ))
                }
            }

            if let Some(value) = response.payload_json()? {
                merge_control(&mut control, &value);
            }
        }

        Ok(control)
    }
}

fn merge_control(control: &mut ScriptControl, value: &Value) {
    let direct = value
        .get("exit_requested")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let nested = value
        .get("runtime")
        .and_then(|runtime| runtime.get("exit_requested"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    control.exit_requested |= direct || nested;

    if let Some(bindings) = value
        .get("ui")
        .and_then(|ui| ui.get("bindings"))
        .and_then(Value::as_object)
    {
        for (key, value) in bindings {
            control.ui_bindings.insert(key.clone(), value.clone());
        }
    }

    if let Some(commands) = value.get("commands").and_then(Value::as_array) {
        control.commands.extend(commands.iter().cloned());
    }
    if let Some(commands) = value
        .get("scene")
        .and_then(|scene| scene.get("commands"))
        .and_then(Value::as_array)
    {
        control.commands.extend(commands.iter().cloned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_merges_nested_exit_request() {
        let mut control = ScriptControl::default();
        merge_control(&mut control, &json!({"runtime": {"exit_requested": true}}));
        assert!(control.exit_requested);
    }

    #[test]
    fn control_collects_ui_bindings() {
        let mut control = ScriptControl::default();
        merge_control(
            &mut control,
            &json!({
                "ui": {
                    "bindings": {
                        "camera.yaw": "32.10 deg",
                        "camera.x": "4.200"
                    }
                }
            }),
        );
        assert_eq!(
            control
                .ui_bindings
                .get("camera.yaw")
                .and_then(Value::as_str),
            Some("32.10 deg")
        );
    }

    #[test]
    fn control_collects_nested_scene_commands() {
        let mut control = ScriptControl::default();
        merge_control(
            &mut control,
            &json!({
                "scene": {
                    "commands": [
                        {"op": "scene.light.upsert", "id": "world.light"},
                        {"op": "scene.entity.transform.set", "id": "world.light"}
                    ]
                }
            }),
        );
        assert_eq!(control.commands.len(), 2);
        assert_eq!(
            control.commands[0].get("op").and_then(Value::as_str),
            Some("scene.light.upsert")
        );
    }

    #[test]
    fn control_collects_generic_command_buffer_in_order() {
        let mut control = ScriptControl::default();
        merge_control(
            &mut control,
            &json!({
                "commands": [
                    {"op": "scene.camera.set", "position": [0, 1, 2]},
                    {"op": "platform.cursor.set", "captured": true}
                ]
            }),
        );
        assert_eq!(control.commands.len(), 2);
        assert_eq!(
            control.commands[0].get("op").and_then(Value::as_str),
            Some("scene.camera.set")
        );
        assert_eq!(
            control.commands[1].get("op").and_then(Value::as_str),
            Some("platform.cursor.set")
        );
    }
}
