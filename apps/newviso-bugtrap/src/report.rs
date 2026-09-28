use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub(crate) struct UiState {
    pub(crate) report_folder: PathBuf,
    pub(crate) title: String,
    pub(crate) phase: String,
    pub(crate) exception_address: String,
    pub(crate) kind: String,
    pub(crate) issue_title: String,
    pub(crate) issue_summary: String,
    pub(crate) evidence_summary: String,
    pub(crate) report_name: String,
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

    let breadcrumb_count = value
        .get("breadcrumbs")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let has_backtrace = value
        .get("backtrace")
        .and_then(Value::as_str)
        .is_some_and(|trace| !trace.trim().is_empty());
    let has_dump = minidump != "N/A";

    let evidence_summary = match (has_dump, has_backtrace, breadcrumb_count) {
        (true, true, count) if count > 0 => format!("Dump · stack · {count} crumbs"),
        (true, true, _) => "Dump · stack".to_owned(),
        (true, false, count) if count > 0 => format!("Dump · {count} crumbs"),
        (false, true, count) if count > 0 => format!("Stack · {count} crumbs"),
        (true, false, _) => "MiniDump captured".to_owned(),
        (false, true, _) => "Stack captured".to_owned(),
        (false, false, count) if count > 0 => format!("{count} breadcrumbs"),
        _ => "Basic report only".to_owned(),
    };

    let overview_page = build_overview_page(
        issue_title,
        issue_summary,
        phase,
        kind,
        &exception_code,
        exception_address,
        &report_name,
        dump_display,
        breadcrumb_count,
        has_backtrace,
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
    let engine_page = build_engine_page(&value, phase);
    let system_page = build_system_page(&value);
    let files_page = build_files_page(report_path, dump_display);

    let report_folder = report_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let copy_text = format!(
        "NewViso BugTrap\r\nReport: {}\r\n\r\n{}\r\n\r\n{}",
        report_path.display(),
        exception_page,
        engine_page
    );
    let copy_error_text = format!(
        "{issue_title}\r\n{message}\r\nKind: {kind}\r\nPhase: {phase}\r\nException: {exception_code}\r\nAddress: {exception_address}"
    );

    UiState {
        report_folder,
        title: "NewViso BugTrap".to_owned(),
        phase: phase.to_owned(),
        exception_address: exception_address.to_owned(),
        kind: kind.to_owned(),
        issue_title: issue_title.to_owned(),
        issue_summary: issue_summary.to_owned(),
        evidence_summary,
        report_name,
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

#[allow(clippy::too_many_arguments)]
fn build_overview_page(
    issue_title: &str,
    issue_summary: &str,
    phase: &str,
    kind: &str,
    exception_code: &str,
    exception_address: &str,
    report_name: &str,
    minidump: &str,
    breadcrumb_count: usize,
    has_backtrace: bool,
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

    format!(
        "WHAT HAPPENED\r\n         =============\r\n         {issue_title}\r\n         {issue_summary}\r\n\r\n         IMMEDIATE CONTEXT\r\n         =================\r\n         Engine phase     : {phase}\r\n         Diagnostic type  : {kind}\r\n         Exception code   : {exception_code}\r\n         Address          : {exception_address}\r\n\r\n         CAPTURED EVIDENCE\r\n         =================\r\n         Crash report     : {report_name}\r\n         MiniDump         : {dump}\r\n         Rust backtrace   : {backtrace}\r\n         Breadcrumbs      : {breadcrumb_count}\r\n\r\n         TRIAGE\r\n         ======\r\n         1. Open Exception for the failure reason, code and crash context.\r\n         2. Open Stack trace for symbolic call frames when available.\r\n         3. Open Engine for the phase and recent engine breadcrumbs.\r\n         4. Use Copy diagnostic when attaching the report to a NewViso issue.\r\n\r\n         The crash package remains local unless you explicitly copy or share it.\r\n"
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

fn build_engine_page(value: &Value, phase: &str) -> String {
    let mut page = String::new();
    page.push_str("ENGINE CONTEXT\r\n==============\r\n");
    page.push_str(&format!("Last recorded phase: {phase}\r\n"));

    let Some(breadcrumbs) = value.get("breadcrumbs").and_then(Value::as_array) else {
        page.push_str("\r\nNo engine breadcrumbs were captured.\r\n");
        return page;
    };

    page.push_str(&format!("Breadcrumbs captured: {}\r\n", breadcrumbs.len()));
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

fn build_system_page(value: &Value) -> String {
    format!(
        "SYSTEM / PROCESS\r\n================\r\nExecutable        : {}\r\nWorking directory : {}\r\nArguments         : {}\r\n",
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

fn build_files_page(report_path: &Path, minidump: &str) -> String {
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

    format!(
        "CRASH PACKAGE\r\n=============\r\nReport   : {}\r\nMiniDump : {}\r\n\r\nUse “Open report folder” to inspect or attach the complete crash package.\r\n",
        report_path.display(),
        dump_path
    )
}

fn classify_issue_title(kind: &str, message: &str) -> &'static str {
    if kind == "report_read_error" && message.contains("os error 2") {
        "Required diagnostic file could not be found"
    } else if kind == "panic" {
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
