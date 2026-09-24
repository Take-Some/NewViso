use std::{
    ffi::c_void,
    ptr::{null, null_mut},
};

use super::{
    actions,
    commands::*,
    ffi::*,
    state::{handles, ui_state, Page},
    theme::{RGB_BG, RGB_PANEL, RGB_TEXT},
    view,
};

pub(crate) unsafe fn run_window() {
    let instance = GetModuleHandleW(null());
    let class_name = wide("NewVisoBugTrapWindow");
    let icon = LoadIconW(instance, make_int_resource(1));
    let cursor = LoadCursorW(null_mut(), make_int_resource(IDC_ARROW));
    let background = CreateSolidBrush(RGB_BG);

    let class = WndClassExW {
        cb_size: std::mem::size_of::<WndClassExW>() as Uint,
        style: 0,
        lpfn_wnd_proc: Some(window_proc),
        cb_cls_extra: 0,
        cb_wnd_extra: 0,
        h_instance: instance,
        h_icon: icon,
        h_cursor: cursor,
        hbr_background: background,
        lpsz_menu_name: null(),
        lpsz_class_name: class_name.as_ptr(),
        h_icon_sm: icon,
    };

    if RegisterClassExW(&class) == 0 {
        return;
    }

    let title = wide(&ui_state().title);
    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        title.as_ptr(),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        1040,
        760,
        null_mut(),
        null_mut(),
        instance,
        null_mut(),
    );
    if hwnd.is_null() {
        return;
    }

    set_light_title_bar(hwnd);

    SendMessageW(hwnd, WM_SETICON, ICON_BIG, icon as Lparam);
    SendMessageW(hwnd, WM_SETICON, ICON_SMALL, icon as Lparam);
    ShowWindow(hwnd, SW_SHOW);
    UpdateWindow(hwnd);

    let mut msg = std::mem::zeroed::<Msg>();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe fn set_light_title_bar(hwnd: Hwnd) {
    let dark: i32 = 0;
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        &dark as *const _ as *const c_void,
        std::mem::size_of_val(&dark) as Dword,
    );
}

unsafe extern "system" fn window_proc(
    hwnd: Hwnd,
    msg: Uint,
    w_param: Wparam,
    l_param: Lparam,
) -> Lresult {
    match msg {
        WM_CREATE => {
            view::create_controls(hwnd);
            0
        }
        WM_COMMAND => {
            handle_command(hwnd, w_param & 0xFFFF);
            0
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN => control_color(w_param as Hdc),
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint_background(hwnd);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            release_ui_resources();
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w_param, l_param),
    }
}

unsafe fn handle_command(hwnd: Hwnd, id: usize) {
    match id {
        ID_COPY => actions::copy_report(hwnd),
        ID_COPY_ERROR => actions::copy_error(hwnd),
        ID_OPEN_FOLDER => actions::open_report_folder(hwnd),
        ID_CLOSE => {
            DestroyWindow(hwnd);
        }
        ID_TAB_OVERVIEW => view::switch_page(hwnd, Page::Overview),
        ID_TAB_EXCEPTION => view::switch_page(hwnd, Page::Exception),
        ID_TAB_STACK => view::switch_page(hwnd, Page::Stack),
        ID_TAB_ENGINE => view::switch_page(hwnd, Page::Engine),
        ID_TAB_SYSTEM => view::switch_page(hwnd, Page::System),
        ID_TAB_FILES => view::switch_page(hwnd, Page::Files),
        _ => {}
    }
}

unsafe fn control_color(hdc: Hdc) -> Lresult {
    SetTextColor(hdc, RGB_TEXT);
    SetBkColor(hdc, RGB_PANEL);
    SetBkMode(hdc, 1);

    let Some(handles_mutex) = handles() else {
        return 0;
    };
    let Ok(handles) = handles_mutex.lock() else {
        return 0;
    };
    let Some(brush) = handles.brushes.get(1) else {
        return 0;
    };

    *brush as Lresult
}

unsafe fn paint_background(hwnd: Hwnd) {
    let mut paint = std::mem::zeroed::<PaintStruct>();
    let hdc = BeginPaint(hwnd, &mut paint);

    if let Some(handles_mutex) = handles() {
        if let Ok(handles) = handles_mutex.lock() {
            if let Some(brush) = handles.brushes.first() {
                let mut rect = std::mem::zeroed::<Rect>();
                GetClientRect(hwnd, &mut rect);
                FillRect(hdc, &rect, *brush);
            }
        }
    }

    EndPaint(hwnd, &paint);
}

unsafe fn release_ui_resources() {
    let Some(handles_mutex) = handles() else {
        return;
    };
    let Ok(handles) = handles_mutex.lock() else {
        return;
    };

    for brush in &handles.brushes {
        DeleteObject(*brush as Hgdobj);
    }
    for font in &handles.fonts {
        DeleteObject(*font as Hgdobj);
    }
}
