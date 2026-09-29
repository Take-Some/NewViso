use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub(crate) struct UiState {
    pub(crate) report_folder: PathBuf,
    pub(crate) log_folder: Option<PathBuf>,
    pub(crate) recovery_executable: Option<PathBuf>,
    pub(crate) recovery_working_dir: Option<PathBuf>,
    pub(crate) recovery_args: Vec<String>,
    pub(crate) safe_mode_supported: bool,
    pub(crate) title: String,
    pub(crate) phase: String,
    pub(crate) issue_title: String,
    pub(crate) issue_summary: String,
    pub(crate) report_name: String,
    pub(crate) crash_id: String,
    pub(crate) build_summary: String,
    pub(crate) renderer_summary: String,
    pub(crate) gpu_summary: String,
    pub(crate) provider_summary: String,
    pub(crate) recovery_summary: String,
    pub(crate) overview_page: String,
    pub(crate) exception_page: String,
    pub(crate) stack_page: String,
    pub(crate) engine_page: String,
    pub(crate) system_page: String,
    pub(crate) files_page: String,
    pub(crate) copy_text: String,
    pub(crate) copy_error_text: String,
}

pub(crate) fn load_state(report_path: &Path) -> UiState {
    let raw = fs::read_to_string(report_path).unwrap_or_else(|error| {
        format!(
            "{{\"kind\":\"report_read_error\",\"message\":\"{}\"}}",
            error.to_string().replace('"', "'")
        )
    });
    let value = serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);

    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown_error");
    let phase = value
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("No error message was recorded.");
    let exception_code = value
        .get("exception_code")
        .and_then(Value::as_u64)
        .map(|code| format!("0x{code:08X}"))
        .unwrap_or_else(|| "N/A".to_owned());
    let exception_address = value
        .get("exception_address")
        .and_then(Value::as_str)
        .unwrap_or("N/A");
    let minidump = value
        .get("minidump")
        .and_then(Value::as_str)
        .unwrap_or("N/A");

    let context = value.get("context").and_then(Value::as_object);
    let issue_title = classify_issue_title(kind, message);
    let issue_summary = classify_issue_summary(kind, message);
    let dump_display = if minidump == "N/A" {
        "Not generated"
    } else {
        minidump
    };
    let report_name = report_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("crash.json")
        .to_owned();

    let crash_id = value
        .get("crash_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| legacy_id("LEGACY", &report_name));
    let session_id = value
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("Legacy report")
        .to_owned();

    let build_summary = build_summary(&value);
    let renderer_summary = renderer_summary(context);
    let gpu_summary = gpu_summary(context);
    let provider_summary = provider_summary(context);
    let safe_mode_active = context_string(context, "safe_mode")
        .is_some_and(|value| matches!(value, "true" | "1" | "yes"));
    let recovery_summary = if safe_mode_active {
        "Safe mode active".to_owned()
    } else {
        "Normal runtime".to_owned()
    };

    let recovery = value.get("recovery");
    let recovery_executable = recovery
        .and_then(|item| item.get("executable"))
        .and_then(Value::as_str)
        .or_else(|| value.get("executable").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);
    let recovery_working_dir = recovery
        .and_then(|item| item.get("working_dir"))
        .and_then(Value::as_str)
        .or_else(|| value.get("current_dir").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);
    let recovery_args = recovery
        .and_then(|item| item.get("args"))
        .or_else(|| value.get("args"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let safe_mode_supported = recovery
        .and_then(|item| item.get("safe_mode_supported"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let log_path = recovery
        .and_then(|item| item.get("log_path"))
        .and_then(Value::as_str)
        .or_else(|| context_string(context, "log_path"));
    let log_folder = log_path
        .map(PathBuf::from)
        .and_then(|path| path.parent().map(Path::to_path_buf));

    let breadcrumb_count = value
        .get("breadcrumbs")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let has_backtrace = value
        .get("backtrace")
        .and_then(Value::as_str)
        .is_some_and(|trace| !trace.trim().is_empty());

    let overview_page = build_overview_page(
        issue_title,
        issue_summary,
        &crash_id,
        &session_id,
        &build_summary,
        &renderer_summary,
        &gpu_summary,
        &provider_summary,
        phase,
        kind,
        &exception_code,
        exception_address,
        &report_name,
        dump_display,
        breadcrumb_count,
        has_backtrace,
        safe_mode_active,
    );
    let exception_page = build_exception_page(
        &value,
        message,
        phase,
        kind,
        &exception_code,
        exception_address,
        dump_display,
    );
    let stack_page = build_stack_page(&value);
    let engine_page = build_engine_page(
        &value,
        phase,
        &renderer_summary,
        &gpu_summary,
        &provider_summary,
        safe_mode_active,
    );
    let system_page = build_system_page(
        &value,
        &crash_id,
        &session_id,
        &build_summary,
        &renderer_summary,
        &gpu_summary,
    );
    let files_page = build_files_page(report_path, dump_display, log_path);

    let report_folder = report_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let copy_text = format!(
        "NewViso BugTrap\r\nCrash ID: {crash_id}\r\nSession: {session_id}\r\nReport: {}\r\nBuild: {build_summary}\r\nRenderer: {renderer_summary}\r\nGPU: {gpu_summary}\r\n\r\n{}\r\n\r\n{}",
        report_path.display(),
        exception_page,
        engine_page
    );
    let copy_error_text = format!(
        "{issue_title}\r\n{message}\r\nCrash ID: {crash_id}\r\nKind: {kind}\r\nPhase: {phase}\r\nException: {exception_code}\r\nAddress: {exception_address}"
    );

    UiState {
        report_folder,
        log_folder,
        recovery_executable,
        recovery_working_dir,
        recovery_args,
        safe_mode_supported,
        title: "NewViso BugTrap".to_owned(),
        phase: phase.to_owned(),
        issue_title: issue_title.to_owned(),
        issue_summary: issue_summary.to_owned(),
        report_name,
        crash_id,
        build_summary,
        renderer_summary,
        gpu_summary,
        provider_summary,
        recovery_summary,
        overview_page,
        exception_page,
        stack_page,
        engine_page,
        system_page,
        files_page,
        copy_text,
        copy_error_text,
    }
}

fn context_string<'a>(
    context: Option<&'a serde_json::Map<String, Value>>,
    key: &str,
) -> Option<&'a str> {
    context?.get(key)?.as_str()
}

fn legacy_id(prefix: &str, report_name: &str) -> String {
    let stem = report_name
        .strip_suffix(".json")
        .unwrap_or(report_name)
        .replace("crash-", "");
    format!("{prefix}-{stem}")
}

fn build_summary(value: &Value) -> String {
    let Some(build) = value.get("build") else {
        return "Legacy / unknown build".to_owned();
    };
    let version = build
        .get("engine_version")
        .and_then(Value::as_str)
        .unwrap_or("?");
    let profile = build.get("profile").and_then(Value::as_str).unwrap_or("?");
    let os = build
        .get("target_os")
        .and_then(Value::as_str)
        .unwrap_or("?");
    let arch = build
        .get("target_arch")
        .and_then(Value::as_str)
        .unwrap_or("?");
    let revision = build
        .get("revision")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());

    match revision {
        Some(revision) => format!("v{version} · {profile} · {os}/{arch} · {revision}"),
        None => format!("v{version} · {profile} · {os}/{arch}"),
    }
}

fn renderer_summary(context: Option<&serde_json::Map<String, Value>>) -> String {
    let name = context_string(context, "renderer_backend_name")
        .or_else(|| context_string(context, "renderer_provider_id"));
    let version = context_string(context, "renderer_backend_version")
        .or_else(|| context_string(context, "renderer_provider_version"));

    match (name, version) {
        (Some(name), Some(version)) => format!("{name} · {version}"),
        (Some(name), None) => name.to_owned(),
        _ => "Renderer metadata unavailable".to_owned(),
    }
}

fn gpu_summary(context: Option<&serde_json::Map<String, Value>>) -> String {
    let Some(raw) = context_string(context, "renderer_info_json") else {
        return "GPU metadata unavailable".to_owned();
    };
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return "GPU metadata unavailable".to_owned();
    };

    if let Some(device) = value.get("device").and_then(Value::as_object) {
        let name = device
            .get("name")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        if let Some(name) = name {
            let mut parts = vec![name.to_owned()];
            if let Some(vram) = device.get("dedicated_vram_mb").and_then(Value::as_u64) {
                if vram > 0 {
                    parts.push(format!("{vram} MiB"));
                }
            }
            if let Some(api) = device
                .get("api_version")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                parts.push(format!("Vulkan {api}"));
            }
            if let Some(driver) = device.get("driver_version").and_then(Value::as_u64) {
                if driver > 0 {
                    parts.push(format!("driver {driver}"));
                }
            }
            let vendor = device.get("vendor_id").and_then(Value::as_u64);
            let device_id = device.get("device_id").and_then(Value::as_u64);
            if let (Some(vendor), Some(device_id)) = (vendor, device_id) {
                if vendor > 0 || device_id > 0 {
                    parts.push(format!("PCI {vendor:04X}:{device_id:04X}"));
                }
            }
            return parts.join(" · ");
        }
    }

    for pointer in [
        "/gpu/name",
        "/gpu/device_name",
        "/device_name",
        "/gpu_name",
        "/adapter/name",
        "/adapter_name",
        "/physical_device/name",
    ] {
        if let Some(name) = value.pointer(pointer).and_then(Value::as_str) {
            if !name.trim().is_empty() {
                return name.to_owned();
            }
        }
    }

    value
        .get("debug_text")
        .and_then(Value::as_str)
        .filter(|text| {
            let lower = text.to_ascii_lowercase();
            lower.contains("gpu") || lower.contains("nvidia") || lower.contains("radeon")
        })
        .map(|text| text.lines().next().unwrap_or(text).to_owned())
        .unwrap_or_else(|| "GPU metadata unavailable".to_owned())
}

fn provider_summary(context: Option<&serde_json::Map<String, Value>>) -> String {
    let Some(inventory) = context_string(context, "provider_inventory") else {
        return "Provider inventory unavailable".to_owned();
    };
    let count = inventory
        .split(';')
        .filter(|entry| !entry.trim().is_empty())
        .count();
    if count == 0 {
        "Provider inventory unavailable".to_owned()
    } else {
        format!("{count} providers discovered")
    }
}

#[allow(clippy::too_many_arguments)]
fn build_overview_page(
    issue_title: &str,
    issue_summary: &str,
    crash_id: &str,
    session_id: &str,
    build: &str,
    renderer: &str,
    gpu: &str,
    providers: &str,
    phase: &str,
    kind: &str,
    exception_code: &str,
    exception_address: &str,
    report_name: &str,
    minidump: &str,
    breadcrumb_count: usize,
    has_backtrace: bool,
    safe_mode_active: bool,
) -> String {
    let backtrace = if has_backtrace {
        "Captured"
    } else {
        "Not captured"
    };
    let dump = if minidump == "Not generated" {
        "Not generated"
    } else {
        "Captured"
    };
    let mode = if safe_mode_active {
        "Safe mode"
    } else {
        "Normal"
    };

    format!(
        "WHAT HAPPENED\r\n         =============\r\n         {issue_title}\r\n         {issue_summary}\r\n\r\n         INCIDENT IDENTITY\r\n         =================\r\n         Crash ID         : {crash_id}\r\n         Session ID       : {session_id}\r\n         Build            : {build}\r\n         Runtime mode     : {mode}\r\n\r\n         IMMEDIATE CONTEXT\r\n         =================\r\n         Engine phase     : {phase}\r\n         Diagnostic type  : {kind}\r\n         Exception code   : {exception_code}\r\n         Address          : {exception_address}\r\n         Renderer         : {renderer}\r\n         GPU              : {gpu}\r\n         Providers        : {providers}\r\n\r\n         CAPTURED EVIDENCE\r\n         =================\r\n         Crash report     : {report_name}\r\n         MiniDump         : {dump}\r\n         Rust backtrace   : {backtrace}\r\n         Breadcrumbs      : {breadcrumb_count}\r\n\r\n         RECOVERY\r\n         ========\r\n         Restart NewViso reproduces the original launch contract.\r\n         Restart in safe mode suppresses optional capabilities, project scripts/UI startup logic and world persistence.\r\n         Open logs opens the project log directory captured for this session.\r\n\r\n         The crash package remains local unless you explicitly copy or share it.\r\n"
    )
}

#[allow(clippy::too_many_arguments)]
fn build_exception_page(
    value: &Value,
    message: &str,
    phase: &str,
    kind: &str,
    exception_code: &str,
    exception_address: &str,
    minidump: &str,
) -> String {
    let mut details = String::new();
    details.push_str("EXCEPTION\r\n=========\r\n");
    details.push_str(message);
    details.push_str("\r\n\r\nCRASH CONTEXT\r\n=============\r\n");
    details.push_str(&format!("Phase      : {phase}\r\n"));
    details.push_str(&format!("Kind       : {kind}\r\n"));
    details.push_str(&format!("Exception  : {exception_code}\r\n"));
    details.push_str(&format!("Address    : {exception_address}\r\n"));
    details.push_str(&format!("MiniDump   : {minidump}\r\n"));

    if let Some(location) = value.get("panic_location").and_then(Value::as_str) {
        details.push_str(&format!("Panic at   : {location}\r\n"));
    }

    if let Some(context) = value.get("context").and_then(Value::as_object) {
        details.push_str("\r\nCONTEXT VALUES\r\n==============\r\n");
        for (key, value) in context {
            details.push_str(&format!("{key}: {}\r\n", display_json(value)));
        }
    }

    details
}

fn build_stack_page(value: &Value) -> String {
    value
        .get("backtrace")
        .and_then(Value::as_str)
        .filter(|trace| !trace.trim().is_empty())
        .map(|backtrace| format!("STACK TRACE\r\n===========\r\n{backtrace}\r\n"))
        .unwrap_or_else(|| {
            "STACK TRACE\r\n===========\r\nNo symbolic Rust backtrace was captured. For a native crash, inspect the MiniDump from the Files tab.\r\n".to_owned()
        })
}

fn build_engine_page(
    value: &Value,
    phase: &str,
    renderer: &str,
    gpu: &str,
    providers: &str,
    safe_mode_active: bool,
) -> String {
    let mut page = String::new();
    page.push_str("ENGINE CONTEXT\r\n==============\r\n");
    page.push_str(&format!("Last recorded phase : {phase}\r\n"));
    page.push_str(&format!(
        "Runtime mode        : {}\r\n",
        if safe_mode_active {
            "Safe mode"
        } else {
            "Normal"
        }
    ));
    page.push_str(&format!("Renderer            : {renderer}\r\n"));
    page.push_str(&format!("GPU                 : {gpu}\r\n"));
    page.push_str(&format!("Providers           : {providers}\r\n"));

    if let Some(context) = value.get("context").and_then(Value::as_object) {
        if let Some(inventory) = context.get("provider_inventory").and_then(Value::as_str) {
            page.push_str("\r\nPROVIDER INVENTORY\r\n==================\r\n");
            for provider in inventory.split(';').filter(|item| !item.trim().is_empty()) {
                page.push_str(provider);
                page.push_str("\r\n");
            }
        }
    }

    let Some(breadcrumbs) = value.get("breadcrumbs").and_then(Value::as_array) else {
        page.push_str("\r\nNo engine breadcrumbs were captured.\r\n");
        return page;
    };

    page.push_str(&format!(
        "\r\nBreadcrumbs captured: {}\r\n",
        breadcrumbs.len()
    ));
    page.push_str("\r\nRECENT BREADCRUMBS\r\n==================\r\n");

    for breadcrumb in breadcrumbs.iter().rev().take(64).rev() {
        let phase = breadcrumb
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or("?");
        let detail = breadcrumb
            .get("detail")
            .and_then(Value::as_str)
            .unwrap_or("");
        let timestamp = breadcrumb
            .get("timestamp_unix_ms")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        page.push_str(&format!("{timestamp:<14} {phase:<24} {detail}\r\n"));
    }

    page
}

fn build_system_page(
    value: &Value,
    crash_id: &str,
    session_id: &str,
    build: &str,
    renderer: &str,
    gpu: &str,
) -> String {
    format!(
        "SYSTEM / PROCESS\r\n================\r\nCrash ID          : {crash_id}\r\nSession ID        : {session_id}\r\nBuild             : {build}\r\nRenderer          : {renderer}\r\nGPU               : {gpu}\r\nExecutable        : {}\r\nWorking directory : {}\r\nArguments         : {}\r\n",
        value
            .get("executable")
            .and_then(Value::as_str)
            .unwrap_or("N/A"),
        value
            .get("current_dir")
            .and_then(Value::as_str)
            .unwrap_or("N/A"),
        value
            .get("args")
            .map(Value::to_string)
            .unwrap_or_else(|| "[]".to_owned())
    )
}

fn build_files_page(report_path: &Path, minidump: &str, log_path: Option<&str>) -> String {
    let dump_path = if minidump == "Not generated" {
        "Not generated".to_owned()
    } else {
        report_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(minidump)
            .display()
            .to_string()
    };
    let log_path = log_path.unwrap_or("Not captured");

    format!(
        "CRASH PACKAGE\r\n=============\r\nReport   : {}\r\nMiniDump : {}\r\nLog      : {}\r\n\r\nOpen report folder shows the complete crash package. Open logs jumps to the project log directory captured for this session.\r\n",
        report_path.display(),
        dump_path,
        log_path
    )
}

fn classify_issue_title(kind: &str, message: &str) -> &'static str {
    if kind == "report_read_error" && message.contains("os error 2") {
        "Required diagnostic file could not be found"
    } else if kind == "rust_panic" || kind == "panic" {
        "NewViso stopped after an internal error"
    } else if kind.contains("exception") || kind.contains("crash") {
        "NewViso stopped after a runtime exception"
    } else {
        "NewViso stopped after an unexpected error"
    }
}

fn classify_issue_summary<'a>(kind: &str, message: &'a str) -> &'a str {
    if kind == "report_read_error" && message.contains("os error 2") {
        "BugTrap could not read the requested crash report because a required file was not found."
    } else {
        message
    }
}

fn display_json(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_summary_uses_renderer_device_contract() {
        let info = serde_json::json!({
            "backend_id": "engine.render.vulkan",
            "device": {
                "name": "NVIDIA GeForce GTX 1080 Ti",
                "vendor_id": 4318,
                "device_id": 6918,
                "driver_version": 123456,
                "api_version": "1.3.0",
                "dedicated_vram_mb": 11264
            }
        });
        let context = serde_json::Map::from_iter([(
            "renderer_info_json".to_owned(),
            Value::String(info.to_string()),
        )]);

        assert_eq!(
            gpu_summary(Some(&context)),
            "NVIDIA GeForce GTX 1080 Ti · 11264 MiB · Vulkan 1.3.0 · driver 123456 · PCI 10DE:1B06"
        );
    }

    #[test]
    fn gpu_summary_remains_compatible_with_old_renderer_info() {
        let context = serde_json::Map::from_iter([(
            "renderer_info_json".to_owned(),
            Value::String(
                serde_json::json!({
                    "backend_id": "engine.render.vulkan",
                    "backend_name": "Vulkan Renderer",
                    "backend_version": "0.33.8"
                })
                .to_string(),
            ),
        )]);

        assert_eq!(gpu_summary(Some(&context)), "GPU metadata unavailable");
    }
}
