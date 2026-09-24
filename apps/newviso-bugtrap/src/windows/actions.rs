use std::ptr::null;

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
    let operation = wide("open");
    let folder = wide(ui_state().report_folder.as_os_str());
    ShellExecuteW(
        hwnd,
        operation.as_ptr(),
        folder.as_ptr(),
        null(),
        null(),
        SW_SHOWNORMAL,
    );
}
