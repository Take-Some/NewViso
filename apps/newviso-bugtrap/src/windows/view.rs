use std::ptr::null;

use super::{
    commands::*,
    ffi::*,
    state::{handles, install_handles, ui_state, Page, UiHandles},
    theme::*,
};

const HEADER_TOP: i32 = 18;
const INCIDENT_TOP: i32 = 96;
const TAB_GAP: i32 = 6;
const ACTION_GAP: i32 = 10;

#[derive(Clone, Copy)]
struct Layout {
    client_width: i32,
    client_height: i32,
    content_left: i32,
    content_right: i32,
    incident: Rect,
    section_y: i32,
    tabs_y: i32,
    details: Rect,
    footer_y: i32,
    status: Rect,
    wide_metrics: bool,
}

impl Layout {
    unsafe fn from_window(hwnd: Hwnd) -> Self {
        let mut client = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        GetClientRect(hwnd, &mut client);

        let client_width = (client.right - client.left).max(1);
        let client_height = (client.bottom - client.top).max(1);
        let margin = if client_width >= 920 { 28 } else { 18 };
        let content_left = margin;
        let content_right = (client_width - margin).max(content_left + 320);
        let content_width = content_right - content_left;
        let wide_metrics = content_width >= 920;

        let incident_bottom = if wide_metrics { 248 } else { 298 };
        let incident = Rect {
            left: content_left,
            top: INCIDENT_TOP,
            right: content_right,
            bottom: incident_bottom,
        };

        let section_y = incident.bottom + 10;
        let tabs_y = incident.bottom + 34;
        let details_top = tabs_y + 44;
        let footer_y = (client_height - 58).max(details_top + 138);
        let details_bottom = (footer_y - 12).max(details_top + 120);

        let status_width = if client_width >= 900 { 184 } else { 166 };
        let status = Rect {
            left: content_right - status_width,
            top: 22,
            right: content_right,
            bottom: 56,
        };

        Self {
            client_width,
            client_height,
            content_left,
            content_right,
            incident,
            section_y,
            tabs_y,
            details: Rect {
                left: content_left + 1,
                top: details_top,
                right: content_right - 1,
                bottom: details_bottom,
            },
            footer_y,
            status,
            wide_metrics,
        }
    }

    fn content_width(self) -> i32 {
        self.content_right - self.content_left
    }
}

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
        0,
        0,
        42,
        42,
        0,
    );
    SendMessageW(icon_view, STM_SETICON, icon as Wparam, 0);

    let tabs = [
        create_tab(hwnd, instance, font_label, "Overview", ID_TAB_OVERVIEW),
        create_tab(hwnd, instance, font_label, "Exception", ID_TAB_EXCEPTION),
        create_tab(hwnd, instance, font_label, "Stack trace", ID_TAB_STACK),
        create_tab(hwnd, instance, font_label, "Engine", ID_TAB_ENGINE),
        create_tab(hwnd, instance, font_label, "System", ID_TAB_SYSTEM),
        create_tab(hwnd, instance, font_label, "Files", ID_TAB_FILES),
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
        0,
        0,
        100,
        100,
        0,
    );
    SendMessageW(details, WM_SETFONT, font_body as Wparam, 1);
    SendMessageW(
        details,
        EM_SETMARGINS,
        EC_LEFTMARGIN | EC_RIGHTMARGIN,
        make_lparam(16, 16),
    );

    let actions = [
        create_action(hwnd, instance, font_label, "Copy error", ID_COPY_ERROR),
        create_action(hwnd, instance, font_label, "Copy diagnostic", ID_COPY),
        create_action(
            hwnd,
            instance,
            font_label,
            "Open report folder",
            ID_OPEN_FOLDER,
        ),
        create_action(hwnd, instance, font_label, "Close", ID_CLOSE),
    ];

    install_handles(UiHandles {
        icon: icon_view,
        details,
        tabs,
        actions,
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

    layout_controls(hwnd);
    switch_page(hwnd, Page::Overview);
}

pub(crate) unsafe fn layout_controls(hwnd: Hwnd) {
    let layout = Layout::from_window(hwnd);
    let Some(handles_mutex) = handles() else {
        return;
    };

    let (icon, details, tabs, actions) = {
        let Ok(handles) = handles_mutex.lock() else {
            return;
        };
        (handles.icon, handles.details, handles.tabs, handles.actions)
    };

    MoveWindow(icon, layout.content_left + 2, HEADER_TOP + 4, 42, 42, 1);

    let tab_width =
        ((layout.content_width() - TAB_GAP * (tabs.len() as i32 - 1)) / tabs.len() as i32).max(70);
    for (index, tab) in tabs.into_iter().enumerate() {
        let x = layout.content_left + index as i32 * (tab_width + TAB_GAP);
        MoveWindow(tab, x, layout.tabs_y, tab_width, 34, 1);
    }

    MoveWindow(
        details,
        layout.details.left,
        layout.details.top,
        (layout.details.right - layout.details.left).max(80),
        (layout.details.bottom - layout.details.top).max(80),
        1,
    );

    if layout.content_width() >= 820 {
        let widths = [126, 152, 174, 126];
        let mut x = layout.content_left;
        for (index, action) in actions.into_iter().enumerate().take(3) {
            MoveWindow(action, x, layout.footer_y, widths[index], 40, 1);
            x += widths[index] + ACTION_GAP;
        }
        MoveWindow(
            actions[3],
            layout.content_right - widths[3],
            layout.footer_y,
            widths[3],
            40,
            1,
        );
    } else {
        let button_width =
            ((layout.content_width() - ACTION_GAP * 3) / actions.len() as i32).max(92);
        for (index, action) in actions.into_iter().enumerate() {
            let x = layout.content_left + index as i32 * (button_width + ACTION_GAP);
            MoveWindow(action, x, layout.footer_y, button_width, 40, 1);
        }
    }

    InvalidateRect(hwnd, null(), 1);
}

pub(crate) unsafe fn switch_page(hwnd: Hwnd, page: Page) {
    let state = ui_state();
    let text = match page {
        Page::Overview => &state.overview_page,
        Page::Exception => &state.exception_page,
        Page::Stack => &state.stack_page,
        Page::Engine => &state.engine_page,
        Page::System => &state.system_page,
        Page::Files => &state.files_page,
    };

    let Some(handles_mutex) = handles() else {
        return;
    };

    let (details, tabs, font) = {
        let Ok(mut handles) = handles_mutex.lock() else {
            return;
        };
        handles.current_page = page;
        let font = if page == Page::Overview {
            handles.fonts[FONT_BODY]
        } else {
            handles.fonts[FONT_MONO]
        };
        (handles.details, handles.tabs, font)
    };

    let text = wide(text);
    SetWindowTextW(details, text.as_ptr());
    SendMessageW(details, WM_SETFONT, font as Wparam, 1);
    SendMessageW(details, EM_SETSEL, 0, 0);

    for tab in tabs {
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
    let layout = Layout::from_window(hwnd);

    let client = Rect {
        left: 0,
        top: 0,
        right: layout.client_width,
        bottom: layout.client_height,
    };
    fill_rect_color(hdc, &client, RGB_BG);

    let brand_left = layout.content_left + 54;
    draw_text(
        hdc,
        fonts[FONT_BRAND],
        "NEWVISO",
        Rect {
            left: brand_left,
            top: 16,
            right: brand_left + 190,
            bottom: 45,
        },
        RGB_TEXT,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        fonts[FONT_META],
        "BUGTRAP  /  CRASH REPORT",
        Rect {
            left: brand_left + 2,
            top: 45,
            right: brand_left + 330,
            bottom: 67,
        },
        RGB_TEXT_MUTED,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    fill_round_rect(hdc, &layout.status, 16, RGB_DANGER_SOFT, RGB_DANGER_BORDER);
    fill_round_rect(
        hdc,
        &Rect {
            left: layout.status.left + 14,
            top: 35,
            right: layout.status.left + 22,
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
            left: layout.status.left + 32,
            top: layout.status.top,
            right: layout.status.right - 12,
            bottom: layout.status.bottom,
        },
        RGB_DANGER,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    fill_rect_color(
        hdc,
        &Rect {
            left: layout.content_left,
            top: 78,
            right: layout.content_right,
            bottom: 79,
        },
        RGB_BORDER,
    );

    fill_round_rect(hdc, &layout.incident, 18, RGB_PANEL, RGB_BORDER);
    fill_round_rect(
        hdc,
        &Rect {
            left: layout.incident.left,
            top: layout.incident.top,
            right: layout.incident.left + 6,
            bottom: layout.incident.bottom,
        },
        6,
        RGB_DANGER,
        RGB_DANGER,
    );

    let state = ui_state();
    let inner_left = layout.incident.left + 24;
    let inner_right = layout.incident.right - 24;

    draw_text(
        hdc,
        fonts[FONT_LABEL],
        "RUNTIME FAILURE",
        Rect {
            left: inner_left,
            top: 109,
            right: inner_left + 190,
            bottom: 132,
        },
        RGB_DANGER,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        fonts[FONT_HEADING],
        &state.issue_title,
        Rect {
            left: inner_left,
            top: 132,
            right: inner_right,
            bottom: 163,
        },
        RGB_TEXT,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );

    let metric_top = if layout.wide_metrics {
        layout.incident.bottom - 48
    } else {
        layout.incident.bottom - 90
    };
    draw_text(
        hdc,
        fonts[FONT_BODY],
        &state.issue_summary,
        Rect {
            left: inner_left,
            top: 162,
            right: inner_right,
            bottom: metric_top - 7,
        },
        RGB_TEXT_MUTED,
        DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
    );

    paint_metrics(hdc, fonts, layout, state);

    draw_text(
        hdc,
        fonts[FONT_META],
        "DIAGNOSTIC DATA",
        Rect {
            left: layout.content_left,
            top: layout.section_y,
            right: layout.content_left + 190,
            bottom: layout.section_y + 22,
        },
        RGB_TEXT_DIM,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        fonts[FONT_META],
        &format!("{}  ·  local crash package", state.report_name),
        Rect {
            left: layout.content_left + 200,
            top: layout.section_y,
            right: layout.content_right,
            bottom: layout.section_y + 22,
        },
        RGB_TEXT_DIM,
        DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );

    let code_frame = Rect {
        left: layout.content_left,
        top: layout.details.top - 1,
        right: layout.content_right,
        bottom: layout.details.bottom + 1,
    };
    fill_round_rect(hdc, &code_frame, 12, RGB_CODE, RGB_BORDER);

    let divider_y = layout.footer_y - 12;
    fill_rect_color(
        hdc,
        &Rect {
            left: layout.content_left,
            top: divider_y,
            right: layout.content_right,
            bottom: divider_y + 1,
        },
        RGB_BORDER,
    );

    if layout.content_width() >= 920 {
        draw_text(
            hdc,
            fonts[FONT_META],
            "Local by default. Share only when you choose to.",
            Rect {
                left: layout.content_left + 520,
                top: layout.footer_y,
                right: layout.content_right - 144,
                bottom: layout.footer_y + 40,
            },
            RGB_TEXT_DIM,
            DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    }
}

unsafe fn paint_metrics(hdc: Hdc, fonts: &[Hfont], layout: Layout, state: &crate::report::UiState) {
    let left = layout.incident.left + 24;
    let right = layout.incident.right - 24;
    let available = right - left;
    let gap = 10;

    let items = [
        ("ENGINE PHASE", state.phase.as_str()),
        ("FAILURE TYPE", state.kind.as_str()),
        ("ADDRESS", state.exception_address.as_str()),
        ("CAPTURED", state.evidence_summary.as_str()),
    ];

    if layout.wide_metrics {
        let width = (available - gap * 3) / 4;
        let y = layout.incident.bottom - 48;
        for (index, (label, value)) in items.into_iter().enumerate() {
            paint_metric(
                hdc,
                fonts,
                label,
                value,
                left + index as i32 * (width + gap),
                y,
                width,
            );
        }
    } else {
        let width = (available - gap) / 2;
        let y = layout.incident.bottom - 90;
        for (index, (label, value)) in items.into_iter().enumerate() {
            let column = index as i32 % 2;
            let row = index as i32 / 2;
            paint_metric(
                hdc,
                fonts,
                label,
                value,
                left + column * (width + gap),
                y + row * 42,
                width,
            );
        }
    }
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

    let selected = page_to_tab_id(handles.current_page) == item.ctl_id as usize;
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

    let width = item.rc_item.right - item.rc_item.left;
    draw_text(
        item.hdc,
        handles.fonts[FONT_LABEL],
        tab_label(item.ctl_id as usize, width),
        item.rc_item,
        if selected { RGB_TEXT } else { RGB_TEXT_MUTED },
        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
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
        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
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

unsafe fn create_tab(hwnd: Hwnd, instance: Hinstance, font: Hfont, text: &str, id: usize) -> Hwnd {
    let tab = create_control(
        hwnd,
        instance,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        0,
        0,
        100,
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
    id: usize,
) -> Hwnd {
    let button = create_control(
        hwnd,
        instance,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW,
        0,
        0,
        120,
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

fn tab_label(id: usize, width: i32) -> &'static str {
    match id {
        ID_TAB_OVERVIEW => "Overview",
        ID_TAB_EXCEPTION if width < 100 => "Error",
        ID_TAB_EXCEPTION => "Exception",
        ID_TAB_STACK if width < 100 => "Stack",
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
