use abi_stable::{
    sabi_trait::TD_Opaque,
    std_types::{RString, RVec},
};
use newviso_assets_client::AssetClient;
use newviso_compat_abi::provider::{EventSinkV1, EventSinkV1_TO};
use newviso_events::{decode_host_event, topic, EventEnvelope};
use newviso_host as host;
pub use newviso_script_client::ScriptPermission;
use newviso_script_client::{
    ScriptClient, ScriptInvocation, ScriptModuleLoad, ScriptResponseStatus,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, Weak},
};

#[derive(Clone, Debug)]
pub struct ScriptModuleSpec {
    pub asset: String,
    pub on_start: Option<String>,
    pub on_frame: Option<String>,
    pub on_event: Option<String>,
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

const MAX_SCRIPT_EVENT_QUEUE: usize = 4096;

#[derive(Default)]
struct ScriptEventQueue {
    events: VecDeque<EventEnvelope>,
    dropped: u64,
}

struct ScriptEventSink {
    queue: Weak<Mutex<ScriptEventQueue>>,
}

impl EventSinkV1 for ScriptEventSink {
    fn on_event(&mut self, topic_name: RString, payload: RVec<u8>) {
        let event = decode_host_event(topic_name.as_str(), payload.as_slice());
        if !event.script_echo_enabled() {
            return;
        }

        let Some(queue) = self.queue.upgrade() else {
            return;
        };
        let Ok(mut queue) = queue.lock() else {
            return;
        };

        if queue.events.len() >= MAX_SCRIPT_EVENT_QUEUE {
            queue.events.pop_front();
            queue.dropped = queue.dropped.saturating_add(1);
        }
        queue.events.push_back(event);
    }
}

pub struct ScriptRuntime {
    modules: Vec<LoadedModule>,
    request_counter: u64,
    event_queue: Arc<Mutex<ScriptEventQueue>>,
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

        let event_queue = Arc::new(Mutex::new(ScriptEventQueue::default()));
        let sink = EventSinkV1_TO::from_value(
            ScriptEventSink {
                queue: Arc::downgrade(&event_queue),
            },
            TD_Opaque,
        );
        host::subscribe_event_sink(sink)?;

        Ok(Self {
            modules: loaded,
            request_counter: 0,
            event_queue,
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
        self.start_with_runtime(project_context, &Value::Null)
    }

    pub fn start_with_runtime(&mut self, project_context: &Value, runtime_state: &Value) -> Result<ScriptControl, String> {
        let mut control = ScriptControl::default();
        for event in self.drain_events() {
            let event_control = self.invoke_event(&event)?;
            absorb_control(&mut control, event_control);
        }

        let payload = json!({
            "event": "start",
            "project": project_context,
            "runtime": runtime_state
        });
        let start_control = self.invoke_lifecycle("start", &payload, false)?;
        absorb_control(&mut control, start_control);
        Ok(control)
    }

    /// Runs one project-script frame from engine-neutral snapshots.
    ///
    /// Host/provider events are delivered to the optional on_event hook before
    /// on_frame and are also exposed as an observational batch in payload.events.
    pub fn frame(
        &mut self,
        delta_seconds: f32,
        elapsed_seconds: f64,
        project_context: &Value,
        runtime_state: &Value,
        frame_context: &Value,
    ) -> Result<ScriptControl, String> {
        let events = self.drain_events();
        let mut control = ScriptControl::default();

        for event in &events {
            let event_control = self.invoke_event(event)?;
            absorb_control(&mut control, event_control);
        }

        let payload = json!({
            "event": "frame",
            "delta_seconds": delta_seconds,
            "elapsed_seconds": elapsed_seconds,
            "project": project_context,
            "runtime": runtime_state,
            "frame": frame_context,
            "events": events
        });

        let frame_control = self.invoke_lifecycle("frame", &payload, true)?;
        absorb_control(&mut control, frame_control);
        Ok(control)
    }

    pub fn shutdown(&mut self, project_context: &Value) -> Result<ScriptControl, String> {
        let mut control = ScriptControl::default();
        for event in self.drain_events() {
            let event_control = self.invoke_event(&event)?;
            absorb_control(&mut control, event_control);
        }

        let payload = json!({
            "event": "shutdown",
            "project": project_context
        });
        let shutdown_control = self.invoke_lifecycle("shutdown", &payload, false)?;
        absorb_control(&mut control, shutdown_control);
        Ok(control)
    }

    fn drain_events(&mut self) -> Vec<EventEnvelope> {
        let Ok(mut queue) = self.event_queue.lock() else {
            return Vec::new();
        };

        let dropped = std::mem::take(&mut queue.dropped);
        let mut events = queue.events.drain(..).collect::<Vec<_>>();
        drop(queue);

        if dropped > 0 {
            let synthetic = EventEnvelope::new(
                0,
                topic::SCRIPT_QUEUE_DROPPED,
                "newviso.scripting",
                json!({
                    "dropped": dropped,
                    "queue_capacity": MAX_SCRIPT_EVENT_QUEUE
                }),
            )
            .expect("static scripting event topic must be valid");
            events.insert(0, synthetic);
        }

        events
    }

    fn invoke_event(&mut self, event: &EventEnvelope) -> Result<ScriptControl, String> {
        let client = ScriptClient::new();
        let mut control = ScriptControl::default();
        let payload = serde_json::to_value(event)
            .map_err(|error| format!("script event payload encode failed: {error}"))?;

        for module_index in 0..self.modules.len() {
            let operation = self.modules[module_index].spec.on_event.clone();
            let Some(operation) = operation else {
                continue;
            };

            self.request_counter = self.request_counter.wrapping_add(1);
            let request_id = format!("newviso-event-{}", self.request_counter);
            let spec = &self.modules[module_index].spec;

            let request = ScriptInvocation {
                request_id: &request_id,
                script_ref: &spec.asset,
                operation: &operation,
                payload: &payload,
                context_bytes: &[],
                permissions: &spec.permissions,
                metadata: BTreeMap::from([
                    ("phase".to_owned(), "event".to_owned()),
                    ("topic".to_owned(), event.topic.clone()),
                    ("payload_format".to_owned(), "json".to_owned()),
                ]),
            };

            let response = client.invoke(&request)?;
            match response.status {
                ScriptResponseStatus::Ok | ScriptResponseStatus::Empty => {}
                other => {
                    return Err(format!(
                        "script '{}' event operation '{}' topic='{}' returned status {:?}: {}",
                        spec.asset,
                        operation,
                        event.topic,
                        other,
                        if response.diagnostics.is_empty() {
                            "no provider diagnostics".to_owned()
                        } else {
                            response.diagnostics.join(" | ")
                        }
                    ))
                }
            }

            if let Some(value) = response.payload_json()? {
                merge_control(&mut control, &value);
            }
        }

        Ok(control)
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
                        "script '{}' operation '{}' returned status {:?}: {}",
                        spec.asset,
                        operation,
                        other,
                        if response.diagnostics.is_empty() {
                            "no provider diagnostics".to_owned()
                        } else {
                            response.diagnostics.join(" | ")
                        }
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

fn absorb_control(target: &mut ScriptControl, mut source: ScriptControl) {
    target.exit_requested |= source.exit_requested;
    target.ui_bindings.append(&mut source.ui_bindings);
    target.commands.append(&mut source.commands);
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
