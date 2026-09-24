use serde::Serialize;
use std::{
    backtrace::Backtrace,
    collections::{BTreeMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BREADCRUMBS: usize = 96;
static INSTALLED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static NATIVE_CAPTURE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static STATE: OnceLock<Mutex<BugTrapState>> = OnceLock::new();

#[derive(Clone, Debug, Serialize)]
pub struct Breadcrumb {
    pub timestamp_unix_ms: u128,
    pub phase: String,
    pub detail: String,
}

#[derive(Debug)]
struct BugTrapState {
    report_dir: PathBuf,
    phase: String,
    context: BTreeMap<String, String>,
    breadcrumbs: VecDeque<Breadcrumb>,
}

impl Default for BugTrapState {
    fn default() -> Self {
        Self {
            report_dir: PathBuf::from(".newviso").join("crash-reports"),
            phase: "process.start".to_owned(),
            context: BTreeMap::new(),
            breadcrumbs: VecDeque::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct CrashReport {
    schema: &'static str,
    kind: String,
    timestamp_unix_ms: u128,
    process_id: u32,
    thread_id: Option<u32>,
    phase: String,
    message: String,
    exception_code: Option<u32>,
    exception_address: Option<String>,
    panic_location: Option<String>,
    executable: Option<String>,
    current_dir: Option<String>,
    args: Vec<String>,
    context: BTreeMap<String, String>,
    breadcrumbs: Vec<Breadcrumb>,
    backtrace: Option<String>,
    minidump: Option<String>,
}

pub fn install(report_dir: impl Into<PathBuf>) -> Result<(), String> {
    configure_report_dir(report_dir)?;

    if INSTALLED.swap(true, Ordering::AcqRel) {
        return Ok(());
    }

    std::panic::set_hook(Box::new(|info| {
        let message = if let Some(value) = info.payload().downcast_ref::<&str>() {
            (*value).to_owned()
        } else if let Some(value) = info.payload().downcast_ref::<String>() {
            value.clone()
        } else {
            "Rust panic with non-string payload".to_owned()
        };
        let location = info.location().map(|location| {
            format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        });
        if let Ok(path) = write_report(
            "rust_panic",
            message,
            None,
            None,
            location,
            Some(format!("{:#}", Backtrace::force_capture())),
            None,
            None,
        ) {
            launch_reporter(&path);
        }
    }));

    #[cfg(windows)]
    unsafe {
        native::install_unhandled_exception_filter();
    }

    breadcrumb("bugtrap.install", "BugTrap installed");
    Ok(())
}

pub fn configure_report_dir(report_dir: impl Into<PathBuf>) -> Result<(), String> {
    let report_dir = report_dir.into();
    fs::create_dir_all(&report_dir).map_err(|error| {
        format!(
            "failed to create BugTrap report directory '{}': {error}",
            report_dir.display()
        )
    })?;
    let mut state = state()
        .lock()
        .map_err(|_| "BugTrap state poisoned".to_owned())?;
    state.report_dir = report_dir;
    Ok(())
}

pub fn set_context(key: impl Into<String>, value: impl Into<String>) {
    if let Ok(mut state) = state().lock() {
        state.context.insert(key.into(), value.into());
    }
}

pub fn set_phase(phase: impl Into<String>) {
    breadcrumb(phase, "");
}

pub fn checkpoint(phase: impl Into<String>) {
    breadcrumb(phase, "checkpoint");
}

pub fn native_boundary(phase: impl Into<String>) {
    let phase = phase.into();
    set_context("native_boundary", phase.clone());
    breadcrumb(phase, "native boundary");
    #[cfg(windows)]
    unsafe {
        native::refresh_unhandled_exception_filter();
    }
}

pub fn breadcrumb(phase: impl Into<String>, detail: impl Into<String>) {
    let phase = phase.into();
    let detail = detail.into();
    if let Ok(mut state) = state().lock() {
        state.phase = phase.clone();
        if state.breadcrumbs.len() >= MAX_BREADCRUMBS {
            state.breadcrumbs.pop_front();
        }
        state.breadcrumbs.push_back(Breadcrumb {
            timestamp_unix_ms: now_unix_ms(),
            phase,
            detail,
        });
    }
}

pub fn report_error(kind: impl Into<String>, message: impl Into<String>) -> Option<PathBuf> {
    let path = write_report(
        &kind.into(),
        message.into(),
        None,
        None,
        None,
        Some(format!("{:#}", Backtrace::force_capture())),
        None,
        None,
    )
    .ok()?;
    launch_reporter(&path);
    Some(path)
}

pub fn report_dir() -> PathBuf {
    state()
        .lock()
        .map(|state| state.report_dir.clone())
        .unwrap_or_else(|_| PathBuf::from(".newviso").join("crash-reports"))
}

pub fn clear_checkpoint() {
    let _ = fs::remove_file(report_dir().join("last-state.json"));
}

fn state() -> &'static Mutex<BugTrapState> {
    STATE.get_or_init(|| Mutex::new(BugTrapState::default()))
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default()
}

fn snapshot() -> (PathBuf, String, BTreeMap<String, String>, Vec<Breadcrumb>) {
    if let Ok(state) = state().try_lock() {
        (
            state.report_dir.clone(),
            state.phase.clone(),
            state.context.clone(),
            state.breadcrumbs.iter().cloned().collect(),
        )
    } else {
        (
            PathBuf::from(".newviso").join("crash-reports"),
            "bugtrap.state_unavailable".to_owned(),
            BTreeMap::new(),
            Vec::new(),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn write_report(
    kind: &str,
    message: String,
    thread_id: Option<u32>,
    exception_code: Option<u32>,
    panic_location: Option<String>,
    backtrace: Option<String>,
    exception_address: Option<String>,
    minidump_name: Option<String>,
) -> Result<PathBuf, String> {
    let timestamp = now_unix_ms();
    let process_id = std::process::id();
    let (report_dir, phase, context, breadcrumbs) = snapshot();
    fs::create_dir_all(&report_dir).map_err(|error| error.to_string())?;

    let stem = format!("crash-{timestamp}-{process_id}");
    let crash_path = report_dir.join(format!("{stem}.json"));

    let report = CrashReport {
        schema: "newviso.bugtrap.crash.v1",
        kind: kind.to_owned(),
        timestamp_unix_ms: timestamp,
        process_id,
        thread_id,
        phase,
        message,
        exception_code,
        exception_address,
        panic_location,
        executable: std::env::current_exe()
            .ok()
            .map(|path| path.display().to_string()),
        current_dir: std::env::current_dir()
            .ok()
            .map(|path| path.display().to_string()),
        args: std::env::args().collect(),
        context,
        breadcrumbs,
        backtrace,
        minidump: minidump_name,
    };

    let bytes = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    fs::write(&crash_path, bytes).map_err(|error| error.to_string())?;

    attach_known_log(&report_dir, &stem, &report.context);

    Ok(crash_path)
}

fn launch_reporter(report_path: &Path) {
    let Some(executable_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    else {
        return;
    };

    #[cfg(windows)]
    let reporter = executable_dir.join("newviso-bugtrap-ui.exe");
    #[cfg(not(windows))]
    let reporter = executable_dir.join("newviso-bugtrap-ui");

    if !reporter.is_file() {
        return;
    }

    let _ = Command::new(reporter).arg(report_path).spawn();
}

fn attach_known_log(report_dir: &Path, stem: &str, context: &BTreeMap<String, String>) {
    let Some(log_path) = context.get("log_path") else {
        return;
    };
    let source = Path::new(log_path);
    if !source.is_file() {
        return;
    }
    let destination = report_dir.join(format!("{stem}.ulog.ndjson"));
    let _ = fs::copy(source, destination);
}

#[cfg(windows)]
mod native {
    use super::{now_unix_ms, snapshot, write_report, NATIVE_CAPTURE_IN_PROGRESS};
    use std::sync::atomic::Ordering;
    use std::{
        ffi::c_void,
        fs::{self, File},
        os::windows::io::AsRawHandle,
    };

    type Handle = *mut c_void;

    #[repr(C)]
    struct ExceptionRecord {
        exception_code: u32,
        exception_flags: u32,
        exception_record: *mut ExceptionRecord,
        exception_address: *mut c_void,
        number_parameters: u32,
        alignment: u32,
        exception_information: [usize; 15],
    }

    #[repr(C)]
    pub(super) struct ExceptionPointers {
        exception_record: *mut ExceptionRecord,
        context_record: *mut c_void,
    }

    #[repr(C)]
    struct MinidumpExceptionInformation {
        thread_id: u32,
        exception_pointers: *mut ExceptionPointers,
        client_pointers: i32,
    }

    type UnhandledExceptionFilter = unsafe extern "system" fn(*mut ExceptionPointers) -> i32;

    const EXCEPTION_EXECUTE_HANDLER: i32 = 1;
    const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
    const EXCEPTION_ACCESS_VIOLATION: u32 = 0xC0000005;
    const EXCEPTION_ILLEGAL_INSTRUCTION: u32 = 0xC000001D;
    const EXCEPTION_STACK_OVERFLOW: u32 = 0xC00000FD;
    const EXCEPTION_INT_DIVIDE_BY_ZERO: u32 = 0xC0000094;
    const EXCEPTION_FLT_DIVIDE_BY_ZERO: u32 = 0xC000008E;
    const MINI_DUMP_WITH_UNLOADED_MODULES: u32 = 0x20;
    const MINI_DUMP_WITH_INDIRECTLY_REFERENCED_MEMORY: u32 = 0x40;
    const MINI_DUMP_WITH_THREAD_INFO: u32 = 0x1000;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetUnhandledExceptionFilter(
            filter: Option<UnhandledExceptionFilter>,
        ) -> Option<UnhandledExceptionFilter>;
        fn AddVectoredExceptionHandler(
            first: u32,
            handler: Option<UnhandledExceptionFilter>,
        ) -> *mut c_void;
        fn GetCurrentProcess() -> Handle;
        fn GetCurrentProcessId() -> u32;
        fn GetCurrentThreadId() -> u32;
    }

    #[link(name = "dbghelp")]
    extern "system" {
        fn MiniDumpWriteDump(
            process: Handle,
            process_id: u32,
            file: Handle,
            dump_type: u32,
            exception_param: *const MinidumpExceptionInformation,
            user_stream_param: *const c_void,
            callback_param: *const c_void,
        ) -> i32;
    }

    pub(super) unsafe fn install_unhandled_exception_filter() {
        let _ = AddVectoredExceptionHandler(1, Some(vectored_exception_handler));
        refresh_unhandled_exception_filter();
    }

    pub(super) unsafe fn refresh_unhandled_exception_filter() {
        SetUnhandledExceptionFilter(Some(unhandled_exception_filter));
    }

    unsafe extern "system" fn vectored_exception_handler(pointers: *mut ExceptionPointers) -> i32 {
        if pointers.is_null() || (*pointers).exception_record.is_null() {
            return EXCEPTION_CONTINUE_SEARCH;
        }

        let code = (*(*pointers).exception_record).exception_code;
        if !matches!(
            code,
            EXCEPTION_ACCESS_VIOLATION
                | EXCEPTION_ILLEGAL_INSTRUCTION
                | EXCEPTION_STACK_OVERFLOW
                | EXCEPTION_INT_DIVIDE_BY_ZERO
                | EXCEPTION_FLT_DIVIDE_BY_ZERO
        ) {
            return EXCEPTION_CONTINUE_SEARCH;
        }

        // First-chance VEH must stay async-minimal: do not allocate, log,
        // symbolize or call DbgHelp here. Reassert the final unhandled filter
        // because providers/CRTs may replace it after engine startup.
        SetUnhandledExceptionFilter(Some(unhandled_exception_filter));
        EXCEPTION_CONTINUE_SEARCH
    }

    unsafe extern "system" fn unhandled_exception_filter(pointers: *mut ExceptionPointers) -> i32 {
        if NATIVE_CAPTURE_IN_PROGRESS.swap(true, Ordering::AcqRel) {
            return EXCEPTION_EXECUTE_HANDLER;
        }
        capture_native_exception("native_unhandled_exception", pointers);
        EXCEPTION_EXECUTE_HANDLER
    }

    unsafe fn capture_native_exception(kind: &str, pointers: *mut ExceptionPointers) {
        let timestamp = now_unix_ms();
        let process_id = GetCurrentProcessId();
        let thread_id = GetCurrentThreadId();
        let (report_dir, _, _, _) = snapshot();
        let _ = fs::create_dir_all(&report_dir);

        let stem = format!("crash-{timestamp}-{process_id}");
        let dump_name = format!("{stem}.dmp");
        let dump_path = report_dir.join(&dump_name);

        let mut exception_code = None;
        let mut exception_address = None;
        if !pointers.is_null() && !(*pointers).exception_record.is_null() {
            let record = &*(*pointers).exception_record;
            exception_code = Some(record.exception_code);
            exception_address = Some(format!("{:p}", record.exception_address));
        }

        let minidump_written = File::create(&dump_path)
            .ok()
            .map(|file| {
                let exception = MinidumpExceptionInformation {
                    thread_id,
                    exception_pointers: pointers,
                    client_pointers: 0,
                };
                MiniDumpWriteDump(
                    GetCurrentProcess(),
                    process_id,
                    file.as_raw_handle() as Handle,
                    MINI_DUMP_WITH_UNLOADED_MODULES
                        | MINI_DUMP_WITH_INDIRECTLY_REFERENCED_MEMORY
                        | MINI_DUMP_WITH_THREAD_INFO,
                    &exception,
                    std::ptr::null(),
                    std::ptr::null(),
                ) != 0
            })
            .unwrap_or(false);

        if let Ok(path) = write_report(
            kind,
            exception_code
                .map(|code| format!("native exception 0x{code:08X}"))
                .unwrap_or_else(|| "native exception".to_owned()),
            Some(thread_id),
            exception_code,
            None,
            None,
            exception_address,
            minidump_written.then_some(dump_name),
        ) {
            super::launch_reporter(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_report_writes_structured_json() {
        let root = std::env::temp_dir().join(format!(
            "newviso-bugtrap-test-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        install(&root).unwrap();
        set_context("provider", "test.provider");
        set_phase("test.phase");

        let path = write_report(
            "test_error",
            "controlled failure".to_owned(),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();

        assert_eq!(value["schema"], "newviso.bugtrap.crash.v1");
        assert_eq!(value["kind"], "test_error");
        assert_eq!(value["phase"], "test.phase");
        assert_eq!(value["context"]["provider"], "test.provider");

        let _ = fs::remove_dir_all(root);
    }
}
