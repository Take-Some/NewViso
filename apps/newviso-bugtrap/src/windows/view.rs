use std::ptr::null;

use super::{
    commands::*,
    ffi::*,
    state::{handles, install_handles, ui_state, Page, UiHandles},
    theme::*,
};

const CONTENT_LEFT: i32 = 28;
const CONTENT_RIGHT: i32 = 1090;
const CONTENT_WIDTH: i32 = CONTENT_RIGHT - CONTENT_LEFT;

pub(crate) unsafe fn create_controls(hwnd: Hwnd) {
    let instance = GetModuleHandleW(null());
    let icon = LoadIconW(instance, make_int_resource(1));

    let font_brand = create_font(21, FW_BOLD, "Segoe UI");
    let font_meta = create_font(10, FW_MEDIUM, "Segoe UI");
    let font_label = create_font(10, FW_SEMIBOLD, "Segoe UI");
    let font_body = create_font(11, FW_NORMAL, "Segoe UI");
    let font_heading = create_font(18, FW_SEMIBOLD, "Segoe UI");
    let font_mono = create_font(10, FW_NORMAL, "Cascadia Mono");
    let font_metric = create_font(11, FW_SEMIBOLD, "Segoe UI");

    let brush_bg = CreateSolidBrush(RGB_BG);
    let brush_code = CreateSolidBrush(RGB_CODE);

    let icon_view = create_control(
        hwnd,
        instance,
        "STATIC",
        "",
        WS_CHILD | WS_VISIBLE | SS_ICON,
        30,
        22,
        42,
        42,
        0,
    );
    SendMessageW(icon_view, STM_SETICON, icon as Wparam, 0);

    let tabs = [
        create_tab(hwnd, instance, font_label, "Overview", ID_TAB_OVERVIEW, 28),
        create_tab(
            hwnd,
            instance,
            font_label,
            "Exception",
            ID_TAB_EXCEPTION,
            148,
        ),
        create_tab(hwnd, instance, font_label, "Stack trace", ID_TAB_STACK, 268),
        create_tab(hwnd, instance, font_label, "Engine", ID_TAB_ENGINE, 388),
        create_tab(hwnd, instance, font_label, "System", ID_TAB_SYSTEM, 508),
        create_tab(hwnd, instance, font_label, "Files", ID_TAB_FILES, 628),
    ];

    let details = create_control(
        hwnd,
        instance,
        "EDIT",
        &ui_state().overview_page,
        WS_CHILD
            | WS_VISIBLE
            | WS_VSCROLL
            | ES_MULTILINE
            | ES_AUTOVSCROLL
            | ES_AUTOHSCROLL
            | ES_READONLY
            | ES_NOHIDESEL,
        CONTENT_LEFT + 1,
        324,
        CONTENT_WIDTH - 2,
        366,
        0,
    );
    SendMessageW(details, WM_SETFONT, font_mono as Wparam, 1);
    SendMessageW(
        details,
        EM_SETMARGINS,
        EC_LEFTMARGIN | EC_RIGHTMARGIN,
        make_lparam(16, 16),
    );

    let copy_error = create_action(
        hwnd,
        instance,
        font_label,
        "Copy error",
        28,
        714,
        126,
        ID_COPY_ERROR,
    );
    let copy = create_action(
        hwnd,
        instance,
        font_label,
        "Copy diagnostic",
        164,
        714,
        152,
        ID_COPY,
    );
    let open = create_action(
        hwnd,
        instance,
        font_label,
        "Open report folder",
        326,
        714,
        174,
        ID_OPEN_FOLDER,
    );
    let close = create_action(hwnd, instance, font_label, "Close", 964, 714, 126, ID_CLOSE);

    for control in [copy_error, copy, open, close] {
        SendMessageW(control, WM_SETFONT, font_label as Wparam, 1);
    }

    install_handles(UiHandles {
        details,
        tabs,
        current_page: Page::Overview,
        brushes: vec![brush_bg, brush_code],
        fonts: vec![
            font_brand,
            font_meta,
            font_label,
            font_body,
            font_heading,
            font_mono,
            font_metric,
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

    for tab in handles.tabs {
        InvalidateRect(tab, null(), 1);
    }
    InvalidateRect(hwnd, null(), 0);
}

pub(crate) unsafe fn paint_shell(hwnd: Hwnd, hdc: Hdc) {
    let Some(handles_mutex) = handles() else {
        return;
    };
    let Ok(handles) = handles_mutex.lock() else {
        return;
    };
    if handles.fonts.len() <= FONT_METRIC {
        return;
    }

    let fonts = &handles.fonts;
    let mut client = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    GetClientRect(hwnd, &mut client);
    fill_rect_color(hdc, &client, RGB_BG);

    draw_text(
        hdc,
        fonts[FONT_BRAND],
        "NEWVISO",
        Rect {
            left: 82,
            top: 16,
            right: 260,
            bottom: 45,
        },
        RGB_TEXT,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        fonts[FONT_META],
        "BUGTRAP  /  CRASH DIAGNOSTICS",
        Rect {
            left: 84,
            top: 45,
            right: 390,
            bottom: 67,
        },
        RGB_TEXT_MUTED,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let status_rect = Rect {
        left: 906,
        top: 22,
        right: 1090,
        bottom: 56,
    };
    fill_round_rect(hdc, &status_rect, 16, RGB_DANGER_SOFT, RGB_DANGER_BORDER);
    fill_round_rect(
        hdc,
        &Rect {
            left: 920,
            top: 35,
            right: 928,
            bottom: 43,
        },
        8,
        RGB_DANGER,
        RGB_DANGER,
    );
    draw_text(
        hdc,
        fonts[FONT_LABEL],
        "CRASH CAPTURED",
        Rect {
            left: 938,
            top: 22,
            right: 1074,
            bottom: 56,
        },
        RGB_DANGER,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    fill_rect_color(
        hdc,
        &Rect {
            left: CONTENT_LEFT,
            top: 78,
            right: CONTENT_RIGHT,
            bottom: 79,
        },
        RGB_BORDER,
    );

    let incident = Rect {
        left: CONTENT_LEFT,
        top: 98,
        right: CONTENT_RIGHT,
        bottom: 241,
    };
    fill_round_rect(hdc, &incident, 18, RGB_PANEL, RGB_BORDER);
    fill_round_rect(
        hdc,
        &Rect {
            left: 28,
            top: 98,
            right: 34,
            bottom: 241,
        },
        6,
        RGB_DANGER,
        RGB_DANGER,
    );

    let state = ui_state();
    draw_text(
        hdc,
        fonts[FONT_LABEL],
        "RUNTIME FAILURE",
        Rect {
            left: 52,
            top: 111,
            right: 240,
            bottom: 133,
        },
        RGB_DANGER,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        fonts[FONT_HEADING],
        &state.issue_title,
        Rect {
            left: 52,
            top: 133,
            right: 1038,
            bottom: 165,
        },
        RGB_TEXT,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        fonts[FONT_BODY],
        &state.issue_summary,
        Rect {
            left: 52,
            top: 163,
            right: 1038,
            bottom: 188,
        },
        RGB_TEXT_MUTED,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );

    paint_metric(hdc, fonts, "ENGINE PHASE", &state.phase, 52, 194, 238);
    paint_metric(hdc, fonts, "ERROR TYPE", &state.kind, 302, 194, 238);
    paint_metric(
        hdc,
        fonts,
        "ADDRESS",
        &state.exception_address,
        552,
        194,
        238,
    );
    paint_metric(hdc, fonts, "MINIDUMP", &state.minidump, 802, 194, 238);

    draw_text(
        hdc,
        fonts[FONT_META],
        "DIAGNOSTIC DATA",
        Rect {
            left: 28,
            top: 254,
            right: 190,
            bottom: 276,
        },
        RGB_TEXT_DIM,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let code_frame = Rect {
        left: 28,
        top: 323,
        right: 1090,
        bottom: 691,
    };
    fill_round_rect(hdc, &code_frame, 12, RGB_CODE, RGB_BORDER);

    fill_rect_color(
        hdc,
        &Rect {
            left: 28,
            top: 702,
            right: 1090,
            bottom: 703,
        },
        RGB_BORDER,
    );

    draw_text(
        hdc,
        fonts[FONT_META],
        "Crash package stays local until you choose to share it.",
        Rect {
            left: 520,
            top: 714,
            right: 944,
            bottom: 754,
        },
        RGB_TEXT_DIM,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
}

pub(crate) unsafe fn paint_owner_draw(item: &DrawItemStruct) {
    if (ID_TAB_OVERVIEW..=ID_TAB_FILES).contains(&(item.ctl_id as usize)) {
        paint_tab(item);
    } else {
        paint_action(item);
    }
}

unsafe fn paint_tab(item: &DrawItemStruct) {
    let Some(handles_mutex) = handles() else {
        return;
    };
    let Ok(handles) = handles_mutex.lock() else {
        return;
    };
    let selected_id = page_to_tab_id(handles.current_page);
    let selected = selected_id == item.ctl_id as usize;
    let pressed = item.item_state & ODS_SELECTED != 0;

    let fill = if selected {
        RGB_PANEL
    } else if pressed {
        RGB_SURFACE
    } else {
        RGB_BG
    };
    let border = if selected { RGB_BORDER_STRONG } else { RGB_BG };
    fill_round_rect(item.hdc, &item.rc_item, 12, fill, border);

    if selected {
        fill_round_rect(
            item.hdc,
            &Rect {
                left: item.rc_item.left + 18,
                top: item.rc_item.bottom - 4,
                right: item.rc_item.right - 18,
                bottom: item.rc_item.bottom - 1,
            },
            3,
            RGB_ACCENT,
            RGB_ACCENT,
        );
    }

    let text = tab_label(item.ctl_id as usize);
    draw_text(
        item.hdc,
        handles.fonts[FONT_LABEL],
        text,
        item.rc_item,
        if selected { RGB_TEXT } else { RGB_TEXT_MUTED },
        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
}

unsafe fn paint_action(item: &DrawItemStruct) {
    let Some(handles_mutex) = handles() else {
        return;
    };
    let Ok(handles) = handles_mutex.lock() else {
        return;
    };

    let id = item.ctl_id as usize;
    let pressed = item.item_state & ODS_SELECTED != 0;
    let disabled = item.item_state & ODS_DISABLED != 0;
    let primary = id == ID_COPY;

    let (fill, border, text_color) = if primary {
        (
            if pressed {
                RGB_ACCENT_PRESSED
            } else {
                RGB_ACCENT
            },
            if pressed {
                RGB_ACCENT_PRESSED
            } else {
                RGB_ACCENT
            },
            RGB_PANEL,
        )
    } else if pressed {
        (RGB_SURFACE, RGB_BORDER_STRONG, RGB_TEXT)
    } else {
        (RGB_PANEL, RGB_BORDER_STRONG, RGB_TEXT)
    };

    fill_round_rect(item.hdc, &item.rc_item, 12, fill, border);

    if item.item_state & ODS_FOCUS != 0 {
        let focus = Rect {
            left: item.rc_item.left + 3,
            top: item.rc_item.top + 3,
            right: item.rc_item.right - 3,
            bottom: item.rc_item.bottom - 3,
        };
        fill_round_rect(
            item.hdc,
            &focus,
            9,
            fill,
            if primary { RGB_PANEL } else { RGB_ACCENT },
        );
    }

    draw_text(
        item.hdc,
        handles.fonts[FONT_LABEL],
        action_label(id),
        item.rc_item,
        if disabled { RGB_TEXT_DIM } else { text_color },
        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
}

unsafe fn paint_metric(
    hdc: Hdc,
    fonts: &[Hfont],
    label: &str,
    value: &str,
    x: i32,
    y: i32,
    width: i32,
) {
    let rect = Rect {
        left: x,
        top: y,
        right: x + width,
        bottom: y + 37,
    };
    fill_round_rect(hdc, &rect, 10, RGB_SURFACE, RGB_BORDER);

    draw_text(
        hdc,
        fonts[FONT_META],
        label,
        Rect {
            left: x + 11,
            top: y + 3,
            right: x + width - 10,
            bottom: y + 18,
        },
        RGB_TEXT_DIM,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        fonts[FONT_METRIC],
        value,
        Rect {
            left: x + 11,
            top: y + 17,
            right: x + width - 10,
            bottom: y + 35,
        },
        RGB_TEXT,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
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
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        x,
        280,
        110,
        34,
        id,
    );
    SendMessageW(tab, WM_SETFONT, font as Wparam, 1);
    tab
}

unsafe fn create_action(
    hwnd: Hwnd,
    instance: Hinstance,
    font: Hfont,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    id: usize,
) -> Hwnd {
    let button = create_control(
        hwnd,
        instance,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        x,
        y,
        width,
        40,
        id,
    );
    SendMessageW(button, WM_SETFONT, font as Wparam, 1);
    button
}

fn page_to_tab_id(page: Page) -> usize {
    match page {
        Page::Overview => ID_TAB_OVERVIEW,
        Page::Exception => ID_TAB_EXCEPTION,
        Page::Stack => ID_TAB_STACK,
        Page::Engine => ID_TAB_ENGINE,
        Page::System => ID_TAB_SYSTEM,
        Page::Files => ID_TAB_FILES,
    }
}

fn tab_label(id: usize) -> &'static str {
    match id {
        ID_TAB_OVERVIEW => "Overview",
        ID_TAB_EXCEPTION => "Exception",
        ID_TAB_STACK => "Stack trace",
        ID_TAB_ENGINE => "Engine",
        ID_TAB_SYSTEM => "System",
        ID_TAB_FILES => "Files",
        _ => "",
    }
}

fn action_label(id: usize) -> &'static str {
    match id {
        ID_COPY_ERROR => "Copy error",
        ID_COPY => "Copy diagnostic",
        ID_OPEN_FOLDER => "Open report folder",
        ID_CLOSE => "Close",
        _ => "",
    }
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
