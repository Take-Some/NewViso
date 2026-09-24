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
    pub(crate) minidump: String,
    pub(crate) kind: String,
    pub(crate) issue_title: String,
    pub(crate) issue_summary: String,
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

    let details = build_exception_details(
        &value,
        report_path,
        message,
        phase,
        kind,
        &exception_code,
        exception_address,
        minidump,
    );

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
        .unwrap_or("crash.json");

    let overview_page = format!(
        "{issue_title}\r\n\r\n{issue_summary}\r\n\r\nEngine phase\r\n{phase}\r\n\r\nDiagnostic type\r\n{kind}\r\n\r\nCrash report\r\n{report_name}\r\n\r\nRecovery\r\nDiagnostic data was collected. Review the technical tabs for stack, engine and system details.\r\n\r\nTechnical details\r\nUse Exception for the raw diagnostic record, or Files to locate the crash package.\r\n"
    );

    let stack_page = build_stack_page(&value);
    let system_page = build_system_page(&value);
    let files_page = format!(
        "CRASH PACKAGE\r\n=============\r\nReport: {}\r\nMiniDump: {}\r\n",
        report_path.display(),
        dump_display
    );

    let report_folder = report_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let copy_text = format!(
        "NewViso BugTrap\r\n{}\r\n\r\n{}",
        report_path.display(),
        details
    );
    let copy_error_text = format!("{issue_title}\r\n{message}\r\nKind: {kind}\r\nPhase: {phase}");

    UiState {
        report_folder,
        title: "NewViso BugTrap".to_owned(),
        phase: phase.to_owned(),
        exception_address: exception_address.to_owned(),
        minidump: dump_display.to_owned(),
        kind: kind.to_owned(),
        issue_title: issue_title.to_owned(),
        issue_summary: issue_summary.to_owned(),
        overview_page,
        exception_page: details.clone(),
        stack_page,
        engine_page: details,
        system_page,
        files_page,
        copy_text,
        copy_error_text,
    }
}

fn build_exception_details(
    value: &Value,
    report_path: &Path,
    message: &str,
    phase: &str,
    kind: &str,
    exception_code: &str,
    exception_address: &str,
    minidump: &str,
) -> String {
    let mut details = String::new();
    details.push_str("Exception Reason\r\n");
    details.push_str("================\r\n");
    details.push_str(message);
    details.push_str("\r\n\r\n");
    details.push_str("Crash Context\r\n");
    details.push_str("=============\r\n");
    details.push_str(&format!("Phase: {phase}\r\n"));
    details.push_str(&format!("Kind: {kind}\r\n"));
    details.push_str(&format!("Exception: {exception_code}\r\n"));
    details.push_str(&format!("Address: {exception_address}\r\n"));
    details.push_str(&format!("MiniDump: {minidump}\r\n"));

    if let Some(location) = value.get("panic_location").and_then(Value::as_str) {
        details.push_str(&format!("Panic location: {location}\r\n"));
    }

    if let Some(context) = value.get("context").and_then(Value::as_object) {
        details.push_str("\r\nContext\r\n=======\r\n");
        for (key, value) in context {
            details.push_str(&format!("{key}: {}\r\n", display_json(value)));
        }
    }

    append_breadcrumbs(&mut details, value);
    append_backtrace(&mut details, value);

    details.push_str("\r\nReport Files\r\n============\r\n");
    details.push_str(&format!("Report: {}\r\n", report_path.display()));
    if minidump != "N/A" {
        details.push_str(&format!(
            "MiniDump: {}\r\n",
            report_path
                .parent()
                .unwrap_or(Path::new("."))
                .join(minidump)
                .display()
        ));
    }

    details
}

fn append_breadcrumbs(details: &mut String, value: &Value) {
    let Some(breadcrumbs) = value.get("breadcrumbs").and_then(Value::as_array) else {
        return;
    };

    details.push_str("\r\nEngine Breadcrumbs\r\n==================\r\n");
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
        details.push_str(&format!("{timestamp}  {phase}  {detail}\r\n"));
    }
}

fn append_backtrace(details: &mut String, value: &Value) {
    let Some(backtrace) = value.get("backtrace").and_then(Value::as_str) else {
        return;
    };

    details.push_str("\r\nBacktrace\r\n=========\r\n");
    details.push_str(backtrace);
    details.push_str("\r\n");
}

fn build_stack_page(value: &Value) -> String {
    value
        .get("backtrace")
        .and_then(Value::as_str)
        .map(|backtrace| format!("STACK TRACE\r\n===========\r\n{backtrace}\r\n"))
        .unwrap_or_else(|| {
            "STACK TRACE\r\n===========\r\nNo symbolic Rust backtrace was captured. For native crashes, use the MiniDump from the Files tab.\r\n".to_owned()
        })
}

fn build_system_page(value: &Value) -> String {
    format!(
        "SYSTEM / PROCESS\r\n================\r\nExecutable: {}\r\nWorking directory: {}\r\nArguments: {}\r\n",
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

fn classify_issue_title(kind: &str, message: &str) -> &'static str {
    if kind == "report_read_error" && message.contains("os error 2") {
        "Required file could not be found"
    } else if kind == "panic" {
        "NewViso encountered an internal error"
    } else {
        "NewViso encountered an error"
    }
}

fn classify_issue_summary<'a>(kind: &str, message: &'a str) -> &'a str {
    if kind == "report_read_error" && message.contains("os error 2") {
        "The diagnostic report could not be read because a required file was not found."
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
