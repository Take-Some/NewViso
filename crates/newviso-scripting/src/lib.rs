use abi_stable::{
    sabi_trait::TD_Opaque,
    std_types::{RString, RVec},
};
use newviso_assets_client::AssetClient;
use newviso_compat_abi::provider::{EventSinkV1, EventSinkV1_TO};
use newviso_events::{decode_host_event, topic, EventEnvelope};
use newviso_host as host;
pub use newviso_script_client::ScriptPermission;
use newviso_script_client::{ScriptClient, ScriptInvocation, ScriptResponseStatus};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, Weak},
};

mod imports;
mod snapshot_delta;
use snapshot_delta::SnapshotDelta;

#[derive(Clone, Debug)]
pub struct ScriptModuleSpec {
    pub asset: String,
    pub runtime_delta: bool,
    pub project_context_cache: bool,
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
    snapshot_revision: Option<u64>,
    project_cache_valid: bool,
}

fn frame_project_cache_ready(modules: &[LoadedModule]) -> bool {
    modules
        .iter()
        .filter(|module| module.spec.on_frame.is_some())
        .all(|module| module.spec.project_context_cache && module.project_cache_valid)
}

#[derive(Default)]
struct ScriptEventQueue {
    capacity: usize,
    events: VecDeque<EventEnvelope>,
    dropped: u64,
}

impl ScriptEventQueue {
    fn resize(&mut self, capacity: usize) {
        self.capacity = capacity;
        while self.events.len() > capacity {
            self.events.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
    }
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

        if queue.events.len() >= queue.capacity {
            queue.events.pop_front();
            queue.dropped = queue.dropped.saturating_add(1);
        }
        queue.events.push_back(event);
    }
}

pub struct ScriptRuntime {
    modules: Vec<LoadedModule>,
    request_counter: u64,
    snapshot_delta: SnapshotDelta,
    event_queue: Arc<Mutex<ScriptEventQueue>>,
}

impl ScriptRuntime {
    pub fn load(
        modules: Vec<ScriptModuleSpec>,
        event_queue_capacity: usize,
    ) -> Result<Self, String> {
        if event_queue_capacity == 0 {
            return Err("event queue capacity must be greater than zero".into());
        }
        let assets = AssetClient::new();
        let scripting = ScriptClient::new();
        let mut loaded = Vec::with_capacity(modules.len());

        for spec in modules {
            imports::load_module_graph(
                &assets,
                &scripting,
                &spec.asset,
                &spec.permissions,
                BTreeMap::from([("source".to_owned(), "newviso-project-vfs".to_owned())]),
            )?;
            loaded.push(LoadedModule {
                spec,
                snapshot_revision: None,
                project_cache_valid: false,
            });
        }

        let event_queue = Arc::new(Mutex::new(ScriptEventQueue {
            capacity: event_queue_capacity,
            ..Default::default()
        }));
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
            snapshot_delta: SnapshotDelta::default(),
            event_queue,
        })
    }

    pub fn configure_event_queue(&mut self, capacity: usize) -> Result<(), String> {
        if capacity == 0 {
            return Err("event queue capacity must be greater than zero".into());
        }
        let mut queue = self
            .event_queue
            .lock()
            .map_err(|_| "script event queue lock poisoned")?;
        queue.resize(capacity);
        Ok(())
    }

    /// Reload configured root scripts, resolving relative dependencies via VFS.
    ///
    /// The V8 provider preserves imported module identities. Until transactional
    /// graph replacement exists, changing an imported child requires restarting
    /// the NewViso process; silently retaining stale imported values is forbidden.
    pub fn reload_graph_after_asset_change(
        &mut self,
        changed_reference: &str,
    ) -> Result<(), String> {
        if !self
            .modules
            .iter()
            .any(|module| module.spec.asset.eq_ignore_ascii_case(changed_reference))
        {
            return Err(format!(
                "Imported YSC asset '{changed_reference}' changed. Restart NewViso to rebuild the linked V8 module graph safely."
            ));
        }
        let assets = AssetClient::new();
        let scripting = ScriptClient::new();

        for module in &mut self.modules {
            // A failed later reload also leaves earlier modules needing a full base.
            module.snapshot_revision = None;
            module.project_cache_valid = false;
            let spec = &module.spec;
            imports::load_module_graph(
                &assets,
                &scripting,
                &spec.asset,
                &spec.permissions,
                BTreeMap::from([
                    ("source".to_owned(), "newviso-content-manager".to_owned()),
                    ("reload".to_owned(), "true".to_owned()),
                    ("changed_asset".to_owned(), changed_reference.to_owned()),
                ]),
            )?;
        }

        Ok(())
    }

    pub fn start(&mut self, project_context: &Value) -> Result<ScriptControl, String> {
        self.start_with_runtime(project_context, &Value::Null)
    }

    pub fn start_with_runtime(
        &mut self,
        project_context: &Value,
        runtime_state: &Value,
    ) -> Result<ScriptControl, String> {
        let mut control = ScriptControl::default();
        for event in self.drain_events() {
            let event_control = self.invoke_event(&event)?;
            absorb_control(&mut control, event_control);
        }

        let payload = json!({
            "event": "start",
            "project": project_context
        });
        let start_control =
            self.invoke_lifecycle("start", &payload, false, Some(runtime_state.clone()))?;
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
        runtime_state: Value,
        frame_context: &Value,
    ) -> Result<ScriptControl, String> {
        let events = self.drain_events();
        let mut control = ScriptControl::default();

        for event in &events {
            let event_control = self.invoke_event(event)?;
            absorb_control(&mut control, event_control);
        }

        // When every frame hook opted into the project-cache protocol and
        // already consumed a complete project config, omit its immutable
        // 300-KiB+ tree. The module's decoder reinstates identical project data.
        let compact = frame_project_cache_ready(&self.modules);
        let payload = if compact {
            json!({
                "event": "frame",
                "delta_seconds": delta_seconds,
                "elapsed_seconds": elapsed_seconds,
                "__project_cached": true,
                "frame": frame_context,
                "events": events
            })
        } else {
            json!({
                "event": "frame",
                "delta_seconds": delta_seconds,
                "elapsed_seconds": elapsed_seconds,
                "project": project_context,
                "frame": frame_context,
                "events": events
            })
        };

        let frame_control = self.invoke_lifecycle("frame", &payload, true, Some(runtime_state))?;
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
        let shutdown_control = self.invoke_lifecycle("shutdown", &payload, false, None)?;
        absorb_control(&mut control, shutdown_control);
        Ok(control)
    }

    fn drain_events(&mut self) -> Vec<EventEnvelope> {
        let Ok(mut queue) = self.event_queue.lock() else {
            return Vec::new();
        };

        let capacity = queue.capacity;
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
                    "queue_capacity": capacity
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
        runtime: Option<Value>,
    ) -> Result<ScriptControl, String> {
        let client = ScriptClient::new();
        let mut control = ScriptControl::default();
        static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let trace = frame
            && *TRACE.get_or_init(|| std::env::var("NEWVISO_PERF_TRACE").as_deref() == Ok("1"));
        let delta_started = trace.then(std::time::Instant::now);
        let use_delta = runtime.is_some() && self.modules.iter().any(|m| m.spec.runtime_delta);
        let revision = self.snapshot_delta.next_revision();
        let delta = use_delta.then(|| self.snapshot_delta.encode(runtime.as_ref().unwrap()));
        let delta_ms = delta_started.map_or(0.0, |mark| mark.elapsed().as_secs_f64() * 1000.0);
        let mut provider_ms = 0.0;
        let mut merge_ms = 0.0;
        let mut script_stage_us = [0u64; 7];
        let mut full_payload = None;
        let mut delta_payload = None;
        let mut reset_payload = None;
        // Merge results in the original module order; only the transport and
        // worker-channel hop are batched. Lifecycle effects remain sequential.
        let batch_frames = frame
            && self
                .modules
                .iter()
                .filter(|module| module.spec.on_frame.is_some())
                .take(2)
                .count()
                == 2;
        let mut batch_requests: Vec<Vec<u8>> = Vec::new();
        let mut batch_modules: Vec<(usize, String, bool, bool)> = Vec::new();
        // Each snapshot flavor is serialized at most once per batch/frame.
        let mut batch_json: [Option<Vec<u8>>; 3] = std::array::from_fn(|_| None);

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
            let module = &self.modules[module_index];
            let spec = &module.spec;
            let project_context_cache = spec.project_context_cache;
            let (request_payload, payload_kind) = if spec.runtime_delta && use_delta {
                if module.snapshot_revision == self.snapshot_delta.revision() {
                    (
                        delta_payload.get_or_insert_with(|| {
                            let mut value = payload.clone();
                            value["__runtime_snapshot"] = delta.as_ref().unwrap().clone();
                            value
                        }),
                        1usize,
                    )
                } else {
                    (
                        reset_payload.get_or_insert_with(|| {
                            let mut value = payload.clone();
                            value["__runtime_snapshot"] =
                                SnapshotDelta::full(runtime.as_ref().unwrap(), revision);
                            value
                        }),
                        2usize,
                    )
                }
            } else {
                (
                    full_payload.get_or_insert_with(|| {
                        let mut value = payload.clone();
                        if let Some(runtime) = runtime.as_ref() {
                            value["runtime"] = runtime.clone();
                        }
                        value
                    }),
                    0usize,
                )
            };

            let request = ScriptInvocation {
                request_id: &request_id,
                script_ref: &spec.asset,
                operation: &operation,
                payload: request_payload,
                context_bytes: &[],
                permissions: &spec.permissions,
                metadata: BTreeMap::from([
                    ("phase".to_owned(), phase.to_owned()),
                    ("payload_format".to_owned(), "json".to_owned()),
                ]),
            };

            if batch_frames {
                if batch_json[payload_kind].is_none() {
                    batch_json[payload_kind] =
                        Some(serde_json::to_vec(request_payload).map_err(|e| e.to_string())?);
                }
                batch_requests.push(client.prepare_frame_request_with_payload(
                    &request,
                    batch_json[payload_kind].as_deref().unwrap(),
                ));
                batch_modules.push((
                    module_index,
                    operation,
                    spec.runtime_delta && use_delta,
                    project_context_cache && payload.get("project").is_some(),
                ));
                continue;
            }

            let provider_started = trace.then(std::time::Instant::now);
            let response = if frame {
                client.frame(&request)?
            } else {
                client.invoke(&request)?
            };
            if let Some(started) = provider_started {
                provider_ms += started.elapsed().as_secs_f64() * 1000.0;
            }
            if trace {
                const KEYS: [&str; 7] = [
                    "perf.host_encode_us",
                    "perf.host_gateway_us",
                    "perf.host_decode_us",
                    "perf.provider_roundtrip_us",
                    "perf.v8_input_bridge_us",
                    "perf.v8_javascript_us",
                    "perf.v8_output_bridge_us",
                ];
                for (index, key) in KEYS.iter().enumerate() {
                    let duration = response
                        .metadata
                        .get(*key)
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or(0);
                    script_stage_us[index] = script_stage_us[index].saturating_add(duration);
                }
            }

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

            let merge_started = trace.then(std::time::Instant::now);
            if let Some(value) = response.payload_json()? {
                merge_control(&mut control, &value);
            }
            if let Some(started) = merge_started {
                merge_ms += started.elapsed().as_secs_f64() * 1000.0;
            }
            if spec.runtime_delta && use_delta {
                self.modules[module_index].snapshot_revision = Some(revision);
            }
            if project_context_cache && payload.get("project").is_some() {
                // Only after a successful script invocation; if it failed
                // or was hot-reloaded, the next call transmits a fresh base.
                self.modules[module_index].project_cache_valid = true;
            }
        }
        if batch_frames {
            let mut processed = 0usize;
            for chunk in batch_requests.chunks(64) {
                let batch_started = trace.then(std::time::Instant::now);
                let responses = client.frame_batch_encoded(chunk)?;
                if let Some(started) = batch_started {
                    provider_ms += started.elapsed().as_secs_f64() * 1000.0;
                }
                let received = responses.len();
                for ((module_index, operation, set_delta, set_cache), response) in batch_modules
                    [processed..processed + received]
                    .iter()
                    .zip(responses)
                {
                    if trace {
                        const KEYS: [&str; 7] = [
                            "perf.host_encode_us",
                            "perf.host_gateway_us",
                            "perf.host_decode_us",
                            "perf.provider_roundtrip_us",
                            "perf.v8_input_bridge_us",
                            "perf.v8_javascript_us",
                            "perf.v8_output_bridge_us",
                        ];
                        for (index, key) in KEYS.iter().enumerate() {
                            let duration = response
                                .metadata
                                .get(*key)
                                .and_then(|value| value.parse::<u64>().ok())
                                .unwrap_or(0);
                            script_stage_us[index] =
                                script_stage_us[index].saturating_add(duration);
                        }
                    }
                    match response.status {
                        ScriptResponseStatus::Ok | ScriptResponseStatus::Empty => {}
                        other => {
                            return Err(format!(
                                "script '{}' operation '{}' returned status {:?}: {}",
                                self.modules[*module_index].spec.asset,
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
                    let merge_started = trace.then(std::time::Instant::now);
                    if let Some(value) = response.payload_json()? {
                        merge_control(&mut control, &value);
                    }
                    if let Some(started) = merge_started {
                        merge_ms += started.elapsed().as_secs_f64() * 1000.0;
                    }
                    if *set_delta {
                        self.modules[*module_index].snapshot_revision = Some(revision);
                    }
                    if *set_cache {
                        self.modules[*module_index].project_cache_valid = true;
                    }
                }
                if received != chunk.len() {
                    return Err("YSC batch stopped without a script error response".into());
                }
                processed += chunk.len();
            }
        }
        if use_delta {
            self.snapshot_delta.commit(runtime.unwrap());
        }
        if trace && self.request_counter % 30 == 0 {
            let json_bytes =
                |value: &Value| serde_json::to_vec(value).map_or(0, |bytes| bytes.len());
            let project_bytes = payload.get("project").map_or(0, json_bytes);
            let delta_bytes = delta.as_ref().map_or(0, json_bytes);
            let full_payload_bytes = full_payload.as_ref().map_or(0, json_bytes);
            let delta_payload_bytes = delta_payload.as_ref().map_or(0, json_bytes);
            host::info("newviso.perf.trace.script", format!(
                "script_frame={} delta_ms={:.4} provider_ms={:.4} response_merge_ms={:.4} project_bytes={} delta_bytes={} full_payload_bytes={} delta_payload_bytes={} commands={}",
                self.request_counter, delta_ms, provider_ms, merge_ms,
                project_bytes, delta_bytes, full_payload_bytes,
                delta_payload_bytes, control.commands.len()
            ));
            host::info("newviso.perf.trace.script.stages", format!(
                "script_frame={} host_encode_us={} host_gateway_us={} host_decode_us={} provider_roundtrip_us={} v8_input_bridge_us={} v8_javascript_us={} v8_output_bridge_us={}",
                self.request_counter, script_stage_us[0], script_stage_us[1],
                script_stage_us[2], script_stage_us[3], script_stage_us[4],
                script_stage_us[5], script_stage_us[6]
            ));
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

    fn cache_module(opt_in: bool, cached: bool, frame: bool) -> LoadedModule {
        LoadedModule {
            spec: ScriptModuleSpec {
                asset: "scripts/test.ysc".into(),
                runtime_delta: opt_in,
                project_context_cache: opt_in,
                on_start: Some("on_start".into()),
                on_frame: frame.then(|| "on_frame".into()),
                on_event: None,
                on_shutdown: None,
                permissions: Vec::new(),
            },
            snapshot_revision: None,
            project_cache_valid: cached,
        }
    }

    #[test]
    fn project_cache_requires_all_active_frame_modules_to_opt_in_and_be_primed() {
        assert!(!frame_project_cache_ready(&[
            cache_module(true, true, true),
            cache_module(false, false, true),
        ]));
        assert!(!frame_project_cache_ready(&[
            cache_module(true, true, true),
            cache_module(true, false, true),
        ]));
        assert!(frame_project_cache_ready(&[
            cache_module(true, true, true),
            cache_module(true, true, true),
            cache_module(false, false, false),
        ]));
    }

    #[test]
    fn resizing_event_queue_preserves_newest_events_and_accounts_drops() {
        let mut queue = ScriptEventQueue {
            capacity: 4,
            ..Default::default()
        };
        for n in 0..4 {
            queue
                .events
                .push_back(EventEnvelope::new(n, "test.event", "test", json!({})).unwrap());
        }
        queue.resize(2);
        assert_eq!(queue.events.len(), 2);
        assert_eq!(queue.events.front().unwrap().sequence, 2);
        assert_eq!(queue.dropped, 2);
        queue.resize(8);
        assert_eq!(queue.events.len(), 2);
        assert_eq!(queue.dropped, 2);
    }

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
