use std::{path::Path, process::Command, ptr::null};

use super::{ffi::*, state::ui_state};

pub(crate) unsafe fn copy_report(hwnd: Hwnd) {
    copy_to_clipboard(hwnd, &ui_state().copy_text);
}

pub(crate) unsafe fn copy_error(hwnd: Hwnd) {
    copy_to_clipboard(hwnd, &ui_state().copy_error_text);
}

unsafe fn copy_to_clipboard(hwnd: Hwnd, text: &str) {
    let wide_text = wide(text);
    let bytes = wide_text.len() * std::mem::size_of::<u16>();
    let memory = GlobalAlloc(GMEM_MOVEABLE, bytes);
    if memory.is_null() {
        return;
    }

    let target = GlobalLock(memory) as *mut u16;
    if target.is_null() {
        GlobalFree(memory);
        return;
    }

    std::ptr::copy_nonoverlapping(wide_text.as_ptr(), target, wide_text.len());
    GlobalUnlock(memory);

    if OpenClipboard(hwnd) == 0 {
        GlobalFree(memory);
        return;
    }

    EmptyClipboard();
    if SetClipboardData(CF_UNICODETEXT, memory).is_null() {
        GlobalFree(memory);
    }
    CloseClipboard();
}

pub(crate) unsafe fn open_report_folder(hwnd: Hwnd) {
    open_folder(hwnd, &ui_state().report_folder);
}

pub(crate) unsafe fn open_project_logs(hwnd: Hwnd) {
    let Some(folder) = ui_state().log_folder.as_deref() else {
        return;
    };
    if folder.is_dir() {
        open_folder(hwnd, folder);
    }
}

unsafe fn open_folder(hwnd: Hwnd, folder: &Path) {
    let operation = wide("open");
    let folder = wide(folder.as_os_str());
    ShellExecuteW(
        hwnd,
        operation.as_ptr(),
        folder.as_ptr(),
        null(),
        null(),
        SW_SHOWNORMAL,
    );
}

pub(crate) unsafe fn restart_newviso(hwnd: Hwnd, safe_mode: bool) {
    let state = ui_state();
    let Some(executable) = state.recovery_executable.as_deref() else {
        return;
    };
    if !is_newviso_executable(executable) {
        return;
    }

    let mut command = Command::new(executable);
    if let Some(working_dir) = state
        .recovery_working_dir
        .as_deref()
        .filter(|path| path.is_dir())
    {
        command.current_dir(working_dir);
    }

    let mut args = recovery_arguments(
        executable,
        &state.recovery_args,
        safe_mode && state.safe_mode_supported,
    );
    if safe_mode && !state.safe_mode_supported {
        return;
    }
    command.args(args.drain(..));

    if command.spawn().is_ok() {
        DestroyWindow(hwnd);
    }
}

pub(crate) fn restart_available() -> bool {
    ui_state()
        .recovery_executable
        .as_deref()
        .is_some_and(is_newviso_executable)
}

pub(crate) fn safe_restart_available() -> bool {
    restart_available() && ui_state().safe_mode_supported
}

pub(crate) fn logs_available() -> bool {
    ui_state().log_folder.as_deref().is_some_and(Path::is_dir)
}

fn is_newviso_executable(path: &Path) -> bool {
    if !path.is_file()
        || !path
            .file_stem()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("newviso"))
    {
        return false;
    }

    let Some(reporter_dir) = std::env::current_exe()
        .ok()
        .and_then(|value| value.parent().map(Path::to_path_buf))
        .and_then(|value| std::fs::canonicalize(value).ok())
    else {
        return false;
    };
    let Some(target_dir) = path
        .parent()
        .and_then(|value| std::fs::canonicalize(value).ok())
    else {
        return false;
    };

    reporter_dir == target_dir
}

fn recovery_arguments(executable: &Path, original: &[String], safe_mode: bool) -> Vec<String> {
    let mut args = original.to_vec();
    if args.first().is_some_and(|first| {
        let first = Path::new(first);
        first == executable
            || first
                .file_stem()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("newviso"))
    }) {
        args.remove(0);
    }

    if safe_mode {
        args.retain(|arg| arg != "--normal-mode" && !arg.starts_with("--set=runtime.safe_mode="));
        let mut filtered = Vec::with_capacity(args.len() + 1);
        let mut index = 0;
        while index < args.len() {
            if args[index] == "--set"
                && args
                    .get(index + 1)
                    .is_some_and(|value| value.starts_with("runtime.safe_mode="))
            {
                index += 2;
                continue;
            }
            filtered.push(args[index].clone());
            index += 1;
        }
        args = filtered;
        if !args.iter().any(|arg| arg == "--safe-mode") {
            args.push("--safe-mode".to_owned());
        }
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_mode_preserves_launch_and_replaces_conflicting_mode() {
        let exe = Path::new(r"C:\NewViso\newviso.exe");
        let args = vec![
            r"C:\NewViso\newviso.exe".to_owned(),
            "--project".to_owned(),
            r"C:\Projects\FirstFPS".to_owned(),
            "--normal-mode".to_owned(),
            "--set=runtime.safe_mode=false".to_owned(),
        ];

        assert_eq!(
            recovery_arguments(exe, &args, true),
            vec![
                "--project".to_owned(),
                r"C:\Projects\FirstFPS".to_owned(),
                "--safe-mode".to_owned(),
            ]
        );
    }

    #[test]
    fn regular_restart_preserves_original_arguments() {
        let exe = Path::new(r"C:\NewViso\newviso.exe");
        let args = vec![
            r"C:\NewViso\newviso.exe".to_owned(),
            "--project".to_owned(),
            "FirstFPS".to_owned(),
        ];

        assert_eq!(
            recovery_arguments(exe, &args, false),
            vec!["--project".to_owned(), "FirstFPS".to_owned()]
        );
    }
}
