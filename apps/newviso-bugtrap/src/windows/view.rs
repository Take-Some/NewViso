use std::ptr::null;

use super::{
    commands::*,
    ffi::*,
    state::{handles, install_handles, ui_state, Page, UiHandles},
    theme::*,
};

pub(crate) unsafe fn create_controls(hwnd: Hwnd) {
    let state = ui_state();
    let instance = GetModuleHandleW(null());
    let icon = LoadIconW(instance, make_int_resource(1));

    let font_title = create_font(23, FW_BOLD, "Segoe UI");
    let font_subtitle = create_font(11, FW_NORMAL, "Segoe UI");
    let font_label = create_font(10, FW_SEMIBOLD, "Segoe UI");
    let font_body = create_font(11, FW_NORMAL, "Segoe UI");
    let font_heading = create_font(17, FW_SEMIBOLD, "Segoe UI");
    let font_mono = create_font(10, FW_NORMAL, "Consolas");

    let brush_bg = CreateSolidBrush(RGB_BG);
    let brush_panel = CreateSolidBrush(RGB_PANEL);

    let icon_view = create_control(
        hwnd,
        instance,
        "STATIC",
        "",
        WS_CHILD | WS_VISIBLE | SS_ICON,
        28,
        24,
        40,
        40,
        0,
    );
    SendMessageW(icon_view, 0x0170, icon as Wparam, 0);

    let title = create_control(
        hwnd,
        instance,
        "STATIC",
        "NewViso BugTrap",
        WS_CHILD | WS_VISIBLE,
        82,
        20,
        330,
        32,
        0,
    );
    SendMessageW(title, WM_SETFONT, font_title as Wparam, 1);

    let subtitle = create_control(
        hwnd,
        instance,
        "STATIC",
        "Crash diagnostics & recovery",
        WS_CHILD | WS_VISIBLE,
        84,
        52,
        420,
        22,
        0,
    );
    SendMessageW(subtitle, WM_SETFONT, font_subtitle as Wparam, 1);

    let status = create_control(
        hwnd,
        instance,
        "STATIC",
        "●  Crash captured",
        WS_CHILD | WS_VISIBLE | SS_CENTER,
        842,
        30,
        156,
        24,
        0,
    );
    SendMessageW(status, WM_SETFONT, font_label as Wparam, 1);

    let headline = create_control(
        hwnd,
        instance,
        "STATIC",
        &state.issue_title,
        WS_CHILD | WS_VISIBLE,
        28,
        94,
        690,
        30,
        0,
    );
    SendMessageW(headline, WM_SETFONT, font_heading as Wparam, 1);

    let summary = create_control(
        hwnd,
        instance,
        "STATIC",
        &state.issue_summary,
        WS_CHILD | WS_VISIBLE,
        28,
        126,
        900,
        24,
        0,
    );
    SendMessageW(summary, WM_SETFONT, font_body as Wparam, 1);

    create_metric_card(
        hwnd,
        instance,
        font_label,
        font_body,
        "ENGINE PHASE",
        &state.phase,
        28,
    );
    create_metric_card(
        hwnd,
        instance,
        font_label,
        font_body,
        "ERROR TYPE",
        &state.kind,
        278,
    );
    create_metric_card(
        hwnd,
        instance,
        font_label,
        font_body,
        "ADDRESS",
        &state.exception_address,
        528,
    );
    create_metric_card(
        hwnd,
        instance,
        font_label,
        font_body,
        "DUMP",
        &state.minidump,
        778,
    );

    let tabs = [
        create_tab(hwnd, instance, font_label, "Overview", ID_TAB_OVERVIEW, 28),
        create_tab(
            hwnd,
            instance,
            font_label,
            "Exception",
            ID_TAB_EXCEPTION,
            154,
        ),
        create_tab(hwnd, instance, font_label, "Stack Trace", ID_TAB_STACK, 280),
        create_tab(hwnd, instance, font_label, "Engine", ID_TAB_ENGINE, 406),
        create_tab(hwnd, instance, font_label, "System", ID_TAB_SYSTEM, 532),
        create_tab(hwnd, instance, font_label, "Files", ID_TAB_FILES, 658),
    ];

    let details = create_control(
        hwnd,
        instance,
        "EDIT",
        &state.overview_page,
        WS_CHILD
            | WS_VISIBLE
            | WS_VSCROLL
            | ES_MULTILINE
            | ES_AUTOVSCROLL
            | ES_AUTOHSCROLL
            | ES_READONLY
            | ES_NOHIDESEL,
        28,
        288,
        970,
        330,
        0,
    );
    SendMessageW(details, WM_SETFONT, font_mono as Wparam, 1);

    let copy_error = create_control(
        hwnd,
        instance,
        "BUTTON",
        "Copy error",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON | BS_FLAT,
        28,
        646,
        126,
        36,
        ID_COPY_ERROR,
    );
    SendMessageW(copy_error, WM_SETFONT, font_label as Wparam, 1);

    let copy = create_control(
        hwnd,
        instance,
        "BUTTON",
        "Copy diagnostic",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON | BS_FLAT,
        166,
        646,
        150,
        36,
        ID_COPY,
    );
    SendMessageW(copy, WM_SETFONT, font_label as Wparam, 1);

    let open = create_control(
        hwnd,
        instance,
        "BUTTON",
        "Open report folder",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON | BS_FLAT,
        328,
        646,
        170,
        36,
        ID_OPEN_FOLDER,
    );
    SendMessageW(open, WM_SETFONT, font_label as Wparam, 1);

    let close = create_control(
        hwnd,
        instance,
        "BUTTON",
        "Close",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON | BS_FLAT,
        878,
        646,
        120,
        36,
        ID_CLOSE,
    );
    SendMessageW(close, WM_SETFONT, font_label as Wparam, 1);

    install_handles(UiHandles {
        details,
        tabs,
        current_page: Page::Overview,
        brushes: vec![brush_bg, brush_panel],
        fonts: vec![
            font_title,
            font_subtitle,
            font_label,
            font_body,
            font_heading,
            font_mono,
        ],
    });

    switch_page(hwnd, Page::Overview);
}

pub(crate) unsafe fn switch_page(hwnd: Hwnd, page: Page) {
    let state = ui_state();
    let Some(handles_mutex) = handles() else {
        return;
    };
    let Ok(mut handles) = handles_mutex.lock() else {
        return;
    };

    let text = match page {
        Page::Overview => &state.overview_page,
        Page::Exception => &state.exception_page,
        Page::Stack => &state.stack_page,
        Page::Engine => &state.engine_page,
        Page::System => &state.system_page,
        Page::Files => &state.files_page,
    };

    let text = wide(text);
    SetWindowTextW(handles.details, text.as_ptr());
    SendMessageW(handles.details, EM_SETSEL, 0, 0);
    handles.current_page = page;

    let labels = [
        (Page::Overview, "Overview"),
        (Page::Exception, "Exception"),
        (Page::Stack, "Stack Trace"),
        (Page::Engine, "Engine"),
        (Page::System, "System"),
        (Page::Files, "Files"),
    ];

    for (index, (tab_page, label)) in labels.iter().enumerate() {
        let caption = if *tab_page == page {
            format!("● {label}")
        } else {
            (*label).to_owned()
        };
        let caption = wide(caption);
        SetWindowTextW(handles.tabs[index], caption.as_ptr());
        InvalidateRect(handles.tabs[index], null(), 1);
    }

    InvalidateRect(hwnd, null(), 0);
}

unsafe fn create_metric_card(
    hwnd: Hwnd,
    instance: Hinstance,
    font_label: Hfont,
    font_value: Hfont,
    label: &str,
    value: &str,
    x: i32,
) {
    let card_label = create_control(
        hwnd,
        instance,
        "STATIC",
        label,
        WS_CHILD | WS_VISIBLE,
        x,
        166,
        220,
        18,
        0,
    );
    SendMessageW(card_label, WM_SETFONT, font_label as Wparam, 1);

    let display = if value.chars().count() > 30 {
        format!("{}…", value.chars().take(29).collect::<String>())
    } else {
        value.to_owned()
    };

    let card_value = create_control(
        hwnd,
        instance,
        "STATIC",
        &display,
        WS_CHILD | WS_VISIBLE,
        x,
        188,
        220,
        32,
        0,
    );
    SendMessageW(card_value, WM_SETFONT, font_value as Wparam, 1);
}

unsafe fn create_tab(
    hwnd: Hwnd,
    instance: Hinstance,
    font: Hfont,
    text: &str,
    id: usize,
    x: i32,
) -> Hwnd {
    let tab = create_control(
        hwnd,
        instance,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON | BS_FLAT,
        x,
        236,
        116,
        34,
        id,
    );
    SendMessageW(tab, WM_SETFONT, font as Wparam, 1);
    tab
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_control(
    parent: Hwnd,
    instance: Hinstance,
    class_name: &str,
    text: &str,
    style: Dword,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    id: usize,
) -> Hwnd {
    let class_name = wide(class_name);
    let text = wide(text);

    CreateWindowExW(
        0,
        class_name.as_ptr(),
        text.as_ptr(),
        style,
        x,
        y,
        width,
        height,
        parent,
        id as Hmenu,
        instance,
        std::ptr::null_mut(),
    )
}
