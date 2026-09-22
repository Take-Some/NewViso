use abi_stable::std_types::{RResult, RString};
use newviso_compat_abi::{
    platform::{
        NativeWindowBackendV1, PlatformHdrModeV1, PlatformWindowModeV1, PlatformWindowReadyV1,
    },
    provider::{Blob, CapabilityId, EventSinkV1Dyn, HostApiV1, MethodName, ServiceV1Dyn},
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock, RwLock,
    },
};

type ServiceSlot = Arc<Mutex<ServiceV1Dyn<'static>>>;
type SinkSlot = Arc<Mutex<EventSinkV1Dyn<'static>>>;

const LOGGING_SERVICE_ID: &str = "logging.api";
const LOGGING_WRITE_METHOD: &str = "write_json";

#[derive(Clone, Copy, Debug)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Clone, Debug)]
struct PendingLog {
    level: LogLevel,
    target: String,
    message: String,
    fields: serde_json::Value,
}

#[derive(Default)]
struct HostState {
    services: HashMap<String, ServiceSlot>,
    aliases: HashMap<String, String>,
    sinks: Vec<SinkSlot>,
    platform_snapshot: Option<PlatformWindowReadyV1>,
    early_logs: Vec<PendingLog>,
}

static HOST_STATE: OnceLock<RwLock<HostState>> = OnceLock::new();
static EMITTED_EVENTS: AtomicU64 = AtomicU64::new(0);

fn state() -> &'static RwLock<HostState> {
    HOST_STATE.get_or_init(|| RwLock::new(HostState::default()))
}

pub fn reset() {
    let mut state = state().write().expect("NewViso host state poisoned");
    *state = HostState::default();
}

pub fn reset_event_count() {
    EMITTED_EVENTS.store(0, Ordering::Relaxed);
}

pub fn emitted_event_count() -> u64 {
    EMITTED_EVENTS.load(Ordering::Relaxed)
}

pub fn host_api() -> HostApiV1 {
    HostApiV1 {
        log_info,
        log_warn,
        log_error,
        register_service_v1,
        call_service_v1,
        emit_event_v1,
        subscribe_events_v1,
    }
}

pub fn set_platform_snapshot(snapshot: PlatformWindowReadyV1) {
    state()
        .write()
        .expect("NewViso host state poisoned")
        .platform_snapshot = Some(snapshot);
}

pub fn clear_platform_snapshot() {
    state()
        .write()
        .expect("NewViso host state poisoned")
        .platform_snapshot = None;
}

pub fn registered_service_ids() -> Vec<String> {
    let state = state().read().expect("NewViso host state poisoned");
    let mut ids = state.services.keys().cloned().collect::<Vec<_>>();
    ids.sort();
    ids
}

pub fn event_sink_count() -> usize {
    state()
        .read()
        .expect("NewViso host state poisoned")
        .sinks
        .len()
}

pub fn call_service(service_id: &str, method: &str, payload: &[u8]) -> Result<Vec<u8>, String> {
    match call_service_v1(
        CapabilityId::from(service_id),
        MethodName::from(method),
        Blob::from(payload.to_vec()),
    ) {
        RResult::ROk(bytes) => Ok(bytes.into_vec()),
        RResult::RErr(error) => Err(error.to_string()),
    }
}

pub fn call_json(
    service_id: &str,
    method: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let encoded = serde_json::to_vec(payload).map_err(|error| error.to_string())?;
    let bytes = call_service(service_id, method, &encoded)?;
    if bytes.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        format!("service '{service_id}' method '{method}' returned invalid JSON: {error}")
    })
}

pub fn log(level: LogLevel, target: impl Into<String>, message: impl Into<String>) {
    log_with_fields(level, target, message, serde_json::json!({}));
}

pub fn log_with_fields(
    level: LogLevel,
    target: impl Into<String>,
    message: impl Into<String>,
    fields: serde_json::Value,
) {
    let record = PendingLog {
        level,
        target: target.into(),
        message: message.into(),
        fields,
    };

    let logging_service = {
        let host = state().read().expect("NewViso host state poisoned");
        host.services.get(LOGGING_SERVICE_ID).cloned()
    };

    if let Some(service) = logging_service {
        let _ = write_log_record(&service, &record);
        return;
    }

    state()
        .write()
        .expect("NewViso host state poisoned")
        .early_logs
        .push(record);
}

pub fn trace(target: impl Into<String>, message: impl Into<String>) {
    log(LogLevel::Trace, target, message);
}

pub fn debug(target: impl Into<String>, message: impl Into<String>) {
    log(LogLevel::Debug, target, message);
}

pub fn info(target: impl Into<String>, message: impl Into<String>) {
    log(LogLevel::Info, target, message);
}

pub fn warn(target: impl Into<String>, message: impl Into<String>) {
    log(LogLevel::Warn, target, message);
}

pub fn error(target: impl Into<String>, message: impl Into<String>) {
    log(LogLevel::Error, target, message);
}

fn write_log_record(service: &ServiceSlot, record: &PendingLog) -> Result<(), String> {
    let payload = serde_json::to_vec(&serde_json::json!({
        "level": record.level.wire_name(),
        "target": record.target,
        "message": record.message,
        "module_path": serde_json::Value::Null,
        "file": serde_json::Value::Null,
        "line": serde_json::Value::Null,
        "run_id": serde_json::Value::Null,
        "run_tag": serde_json::Value::Null,
        "event_id": serde_json::Value::Null,
        "fields": record.fields
    }))
    .map_err(|error| error.to_string())?;

    let service = service
        .lock()
        .map_err(|_| "NewViso logging service slot poisoned".to_owned())?;
    match service.call(MethodName::from(LOGGING_WRITE_METHOD), Blob::from(payload)) {
        RResult::ROk(_) => Ok(()),
        RResult::RErr(error) => Err(error.to_string()),
    }
}

fn flush_early_logs(service: &ServiceSlot, logs: Vec<PendingLog>) {
    for record in logs {
        let _ = write_log_record(service, &record);
    }
}

pub fn add_alias(alias: impl Into<String>, service_id: impl Into<String>) {
    state()
        .write()
        .expect("NewViso host state poisoned")
        .aliases
        .insert(alias.into(), service_id.into());
}

extern "C" fn log_info(message: RString) {
    info("provider", message.to_string());
}

extern "C" fn log_warn(message: RString) {
    warn("provider", message.to_string());
}

extern "C" fn log_error(message: RString) {
    error("provider", message.to_string());
}

extern "C" fn register_service_v1(service: ServiceV1Dyn<'static>) -> RResult<(), RString> {
    let id = service.id().to_string();
    if id.trim().is_empty() {
        return RResult::RErr(RString::from(
            "provider attempted to register an empty service id",
        ));
    }

    let slot = Arc::new(Mutex::new(service));
    let (logging_slot, pending_logs) = {
        let mut host = state().write().expect("NewViso host state poisoned");
        if host.services.contains_key(&id) {
            return RResult::RErr(RString::from(format!(
                "service '{id}' is already registered"
            )));
        }

        host.services.insert(id.clone(), slot.clone());

        match id.as_str() {
            "logging.api" => {
                host.aliases.insert("engine.logging".into(), id.clone());
            }
            "asset_manager.api" => {
                host.aliases.insert("engine.assets".into(), id.clone());
                host.aliases
                    .insert("engine.assets.streaming".into(), id.clone());
                host.aliases.insert("engine.assets.uid".into(), id.clone());
                host.aliases
                    .insert("engine.assets.dependencies".into(), id.clone());
                host.aliases
                    .insert("engine.assets.import_queue".into(), id.clone());
                host.aliases
                    .insert("engine.assets.package_writer".into(), id.clone());
            }
            "ecs.api" => {
                host.aliases.insert("engine.ecs".into(), id.clone());
            }
            "entity.api" => {
                host.aliases.insert("engine.entity".into(), id.clone());
            }
            "scene.api" => {
                host.aliases.insert("engine.scene".into(), id.clone());
            }
            "input.api" | "newengine.input.v1" => {
                host.aliases.insert("engine.input".into(), id.clone());
            }
            "render.api" => {
                host.aliases.insert("engine.render".into(), id.clone());
            }
            "physics.api" => {
                host.aliases.insert("engine.physics".into(), id.clone());
            }
            _ => {}
        }

        if id == LOGGING_SERVICE_ID {
            (Some(slot.clone()), std::mem::take(&mut host.early_logs))
        } else {
            (None, Vec::new())
        }
    };

    if let Some(logging) = logging_slot {
        flush_early_logs(&logging, pending_logs);
    }

    info("newviso.host", format!("registered service '{id}'"));
    RResult::ROk(())
}

extern "C" fn call_service_v1(
    service_id: CapabilityId,
    method: MethodName,
    payload: Blob,
) -> RResult<Blob, RString> {
    let requested = service_id.to_string();

    if requested == "engine.platform" {
        return call_platform_service(method.as_str());
    }

    let slot = {
        let host = state().read().expect("NewViso host state poisoned");
        let resolved = host
            .aliases
            .get(&requested)
            .cloned()
            .unwrap_or_else(|| requested.clone());
        host.services.get(&resolved).cloned()
    };

    let Some(slot) = slot else {
        return RResult::RErr(RString::from(format!(
            "NewViso host: service '{requested}' is not registered"
        )));
    };

    let service = slot.lock().expect("NewViso service slot poisoned");
    service.call(method, payload)
}

fn call_platform_service(method: &str) -> RResult<Blob, RString> {
    match method {
        "window_snapshot_json_v1" => {
            let snapshot = state()
                .read()
                .expect("NewViso host state poisoned")
                .platform_snapshot;

            let Some(snapshot) = snapshot else {
                return RResult::RErr(RString::from(
                    "NewViso host: platform window snapshot is not ready",
                ));
            };

            let backend = match snapshot.handles.backend {
                NativeWindowBackendV1::Unknown => "Unknown",
                NativeWindowBackendV1::Win32 => "Win32",
                NativeWindowBackendV1::Wayland => "Wayland",
                NativeWindowBackendV1::Xlib => "Xlib",
                NativeWindowBackendV1::Xcb => "Xcb",
            };
            let window_mode = match snapshot.display.window_mode {
                PlatformWindowModeV1::Windowed => "Windowed",
                PlatformWindowModeV1::Borderless => "Borderless",
                PlatformWindowModeV1::ExclusiveFullscreen => "ExclusiveFullscreen",
            };
            let hdr = match snapshot.display.hdr {
                PlatformHdrModeV1::Auto => "Auto",
                PlatformHdrModeV1::Enabled => "Enabled",
                PlatformHdrModeV1::Disabled => "Disabled",
            };

            let value = serde_json::json!({
                "handles": {
                    "backend": backend,
                    "window": snapshot.handles.window,
                    "display": snapshot.handles.display,
                    "reserved0": snapshot.handles.reserved0,
                    "reserved1": snapshot.handles.reserved1,
                },
                "surface": {
                    "width": snapshot.surface.width,
                    "height": snapshot.surface.height,
                    "pixels_per_point": snapshot.surface.pixels_per_point,
                },
                "display": {
                    "monitor_index": snapshot.display.monitor_index,
                    "window_mode": window_mode,
                    "vsync": snapshot.display.vsync,
                    "refresh_rate_millihz": snapshot.display.refresh_rate_millihz,
                    "render_scale": snapshot.display.render_scale,
                    "hdr": hdr,
                }
            });

            match serde_json::to_vec(&value) {
                Ok(bytes) => RResult::ROk(bytes.into()),
                Err(error) => RResult::RErr(RString::from(error.to_string())),
            }
        }
        "info_json" => {
            let value = serde_json::json!({
                "protocol": "newengine.platform-api/v1",
                "provider": "NewViso host + external platform runtime",
                "methods": [
                    "info_json",
                    "window_snapshot_json_v1"
                ]
            });
            RResult::ROk(serde_json::to_vec(&value).unwrap_or_default().into())
        }
        other => RResult::RErr(RString::from(format!(
            "NewViso host: unsupported engine.platform method '{other}'"
        ))),
    }
}

extern "C" fn emit_event_v1(topic: RString, payload: Blob) -> RResult<(), RString> {
    EMITTED_EVENTS.fetch_add(1, Ordering::Relaxed);
    let sinks = state()
        .read()
        .expect("NewViso host state poisoned")
        .sinks
        .clone();

    for sink in sinks {
        let mut sink = sink.lock().expect("NewViso event sink poisoned");
        sink.on_event(topic.clone(), payload.clone());
    }

    RResult::ROk(())
}

extern "C" fn subscribe_events_v1(sink: EventSinkV1Dyn<'static>) -> RResult<(), RString> {
    state()
        .write()
        .expect("NewViso host state poisoned")
        .sinks
        .push(Arc::new(Mutex::new(sink)));
    info("newviso.host", "event sink subscribed");
    RResult::ROk(())
}
