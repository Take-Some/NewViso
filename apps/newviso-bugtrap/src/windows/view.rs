use std::ptr::null;

use super::{
    actions as recovery_actions,
    commands::*,
    ffi::*,
    state::{handles, install_handles, ui_state, Page, UiHandles},
    theme::*,
};

const BASE_DPI: i32 = 96;
const TAB_GAP: i32 = 6;
const ACTION_GAP: i32 = 10;

#[derive(Clone, Copy)]
struct Layout {
    dpi: i32,
    client_width: i32,
    client_height: i32,
    logical_content_width: i32,
    content_left: i32,
    content_right: i32,
    incident: Rect,
    section_y: i32,
    tabs_y: i32,
    details: Rect,
    footer_y: i32,
    status: Rect,
    wide_metrics: bool,
    compact_actions: bool,
}

impl Layout {
    unsafe fn from_window(hwnd: Hwnd) -> Self {
        let dpi = dpi_for_window(hwnd);
        let mut client = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        GetClientRect(hwnd, &mut client);

        let client_width = (client.right - client.left).max(1);
        let client_height = (client.bottom - client.top).max(1);
        let logical_width = unscale(client_width, dpi);
        let logical_height = unscale(client_height, dpi);
        let margin = if logical_width >= 920 { 28 } else { 18 };
        let logical_content_width = (logical_width - margin * 2).max(320);
        let content_left = scale(margin, dpi);
        let content_right = (client_width - scale(margin, dpi)).max(content_left + scale(320, dpi));
        let wide_metrics = logical_content_width >= 920;
        let compact_actions = logical_content_width < 980;

        let incident_bottom = if wide_metrics { 248 } else { 298 };
        let incident = Rect {
            left: content_left,
            top: scale(96, dpi),
            right: content_right,
            bottom: scale(incident_bottom, dpi),
        };

        let section_y = incident.bottom + scale(10, dpi);
        let tabs_y = incident.bottom + scale(34, dpi);
        let details_top = tabs_y + scale(44, dpi);
        let footer_reserved = if compact_actions { 104 } else { 58 };
        let footer_y = scale((logical_height - footer_reserved).max(568), dpi);
        let details_bottom = (footer_y - scale(12, dpi)).max(details_top + scale(120, dpi));

        let status_width = if logical_width >= 900 { 184 } else { 166 };
        let status = Rect {
            left: content_right - scale(status_width, dpi),
            top: scale(22, dpi),
            right: content_right,
            bottom: scale(56, dpi),
        };

        Self {
            dpi,
            client_width,
            client_height,
            logical_content_width,
            content_left,
            content_right,
            incident,
            section_y,
            tabs_y,
            details: Rect {
                left: content_left + scale(1, dpi),
                top: details_top,
                right: content_right - scale(1, dpi),
                bottom: details_bottom,
            },
            footer_y,
            status,
            wide_metrics,
            compact_actions,
        }
    }

    fn content_width(self) -> i32 {
        self.content_right - self.content_left
    }

    fn px(self, value: i32) -> i32 {
        scale(value, self.dpi)
    }
}

pub(crate) unsafe fn create_controls(hwnd: Hwnd) {
    let instance = GetModuleHandleW(null());
    let icon = LoadIconW(instance, make_int_resource(1));
    let dpi = dpi_for_window(hwnd);
    let fonts = create_fonts(dpi);

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
        scale(42, dpi),
        scale(42, dpi),
        0,
    );
    SendMessageW(icon_view, STM_SETICON, icon as Wparam, 0);

    let tabs = [
        create_tab(
            hwnd,
            instance,
            fonts[FONT_LABEL],
            "Overview",
            ID_TAB_OVERVIEW,
        ),
        create_tab(
            hwnd,
            instance,
            fonts[FONT_LABEL],
            "Exception",
            ID_TAB_EXCEPTION,
        ),
        create_tab(
            hwnd,
            instance,
            fonts[FONT_LABEL],
            "Stack trace",
            ID_TAB_STACK,
        ),
        create_tab(hwnd, instance, fonts[FONT_LABEL], "Engine", ID_TAB_ENGINE),
        create_tab(hwnd, instance, fonts[FONT_LABEL], "System", ID_TAB_SYSTEM),
        create_tab(hwnd, instance, fonts[FONT_LABEL], "Files", ID_TAB_FILES),
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
    SendMessageW(details, WM_SETFONT, fonts[FONT_BODY] as Wparam, 1);
    SendMessageW(
        details,
        EM_SETMARGINS,
        EC_LEFTMARGIN | EC_RIGHTMARGIN,
        make_lparam(scale(16, dpi), scale(16, dpi)),
    );

    let actions = [
        create_action(
            hwnd,
            instance,
            fonts[FONT_LABEL],
            "Restart NewViso",
            ID_RESTART,
        ),
        create_action(
            hwnd,
            instance,
            fonts[FONT_LABEL],
            "Restart in safe mode",
            ID_RESTART_SAFE,
        ),
        create_action(hwnd, instance, fonts[FONT_LABEL], "Open logs", ID_OPEN_LOGS),
        create_action(
            hwnd,
            instance,
            fonts[FONT_LABEL],
            "Copy diagnostic",
            ID_COPY,
        ),
        create_action(
            hwnd,
            instance,
            fonts[FONT_LABEL],
            "Open report folder",
            ID_OPEN_FOLDER,
        ),
        create_action(hwnd, instance, fonts[FONT_LABEL], "Close", ID_CLOSE),
    ];

    EnableWindow(actions[0], recovery_actions::restart_available() as Bool);
    EnableWindow(
        actions[1],
        recovery_actions::safe_restart_available() as Bool,
    );
    EnableWindow(actions[2], recovery_actions::logs_available() as Bool);

    install_handles(UiHandles {
        icon: icon_view,
        details,
        tabs,
        actions,
        current_page: Page::Overview,
        brushes: vec![brush_bg, brush_code],
        fonts,
    });

    layout_controls(hwnd);
    switch_page(hwnd, Page::Overview);
}

pub(crate) unsafe fn refresh_dpi_resources(hwnd: Hwnd) {
    let dpi = dpi_for_window(hwnd);
    let new_fonts = create_fonts(dpi);
    if let Some(handles_mutex) = handles() {
        if let Ok(mut handles) = handles_mutex.lock() {
            let old_fonts = std::mem::replace(&mut handles.fonts, new_fonts);
            let details_font = if handles.current_page == Page::Overview {
                handles.fonts[FONT_BODY]
            } else {
                handles.fonts[FONT_MONO]
            };
            SendMessageW(handles.details, WM_SETFONT, details_font as Wparam, 1);
            for control in handles.tabs.into_iter().chain(handles.actions) {
                SendMessageW(control, WM_SETFONT, handles.fonts[FONT_LABEL] as Wparam, 1);
            }
            SendMessageW(
                handles.details,
                EM_SETMARGINS,
                EC_LEFTMARGIN | EC_RIGHTMARGIN,
                make_lparam(scale(16, dpi), scale(16, dpi)),
            );
            for font in old_fonts {
                DeleteObject(font as Hgdobj);
            }
        }
    }
    InvalidateRect(hwnd, null(), 1);
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

    MoveWindow(
        icon,
        layout.content_left + layout.px(2),
        layout.px(22),
        layout.px(42),
        layout.px(42),
        1,
    );

    let tab_gap = layout.px(TAB_GAP);
    let tab_width = ((layout.content_width() - tab_gap * (tabs.len() as i32 - 1))
        / tabs.len() as i32)
        .max(layout.px(70));
    for (index, tab) in tabs.into_iter().enumerate() {
        let x = layout.content_left + index as i32 * (tab_width + tab_gap);
        MoveWindow(tab, x, layout.tabs_y, tab_width, layout.px(34), 1);
    }

    MoveWindow(
        details,
        layout.details.left,
        layout.details.top,
        (layout.details.right - layout.details.left).max(layout.px(80)),
        (layout.details.bottom - layout.details.top).max(layout.px(80)),
        1,
    );

    let action_gap = layout.px(ACTION_GAP);
    if !layout.compact_actions {
        let logical_widths = [138, 168, 106, 142, 164, 90];
        let widths = logical_widths.map(|width| layout.px(width));
        let total = widths.iter().sum::<i32>() + action_gap * 5;
        let squeeze = (total - layout.content_width()).max(0);
        let per_button_squeeze = (squeeze / 6) + i32::from(squeeze % 6 != 0);
        let widths = widths.map(|width| (width - per_button_squeeze).max(layout.px(82)));

        let mut x = layout.content_left;
        for (index, action) in actions.into_iter().enumerate() {
            MoveWindow(action, x, layout.footer_y, widths[index], layout.px(40), 1);
            x += widths[index] + action_gap;
        }
    } else {
        let columns = 3;
        let width =
            ((layout.content_width() - action_gap * (columns - 1)) / columns).max(layout.px(92));
        for (index, action) in actions.into_iter().enumerate() {
            let column = index as i32 % columns;
            let row = index as i32 / columns;
            MoveWindow(
                action,
                layout.content_left + column * (width + action_gap),
                layout.footer_y + row * layout.px(46),
                width,
                layout.px(40),
                1,
            );
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

    let brand_left = layout.content_left + layout.px(54);
    draw_text(
        hdc,
        fonts[FONT_BRAND],
        "NEWVISO",
        Rect {
            left: brand_left,
            top: layout.px(16),
            right: brand_left + layout.px(190),
            bottom: layout.px(45),
        },
        RGB_TEXT,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        fonts[FONT_META],
        "BUGTRAP  /  RECOVERY & DIAGNOSTICS",
        Rect {
            left: brand_left + layout.px(2),
            top: layout.px(45),
            right: brand_left + layout.px(360),
            bottom: layout.px(67),
        },
        RGB_TEXT_MUTED,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    fill_round_rect(
        hdc,
        &layout.status,
        layout.px(16),
        RGB_DANGER_SOFT,
        RGB_DANGER_BORDER,
    );
    fill_round_rect(
        hdc,
        &Rect {
            left: layout.status.left + layout.px(14),
            top: layout.status.top + layout.px(13),
            right: layout.status.left + layout.px(22),
            bottom: layout.status.top + layout.px(21),
        },
        layout.px(8),
        RGB_DANGER,
        RGB_DANGER,
    );
    draw_text(
        hdc,
        fonts[FONT_LABEL],
        "CRASH CAPTURED",
        Rect {
            left: layout.status.left + layout.px(32),
            top: layout.status.top,
            right: layout.status.right - layout.px(12),
            bottom: layout.status.bottom,
        },
        RGB_DANGER,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    fill_rect_color(
        hdc,
        &Rect {
            left: layout.content_left,
            top: layout.px(78),
            right: layout.content_right,
            bottom: layout.px(79),
        },
        RGB_BORDER,
    );

    fill_round_rect(hdc, &layout.incident, layout.px(18), RGB_PANEL, RGB_BORDER);
    fill_round_rect(
        hdc,
        &Rect {
            left: layout.incident.left,
            top: layout.incident.top,
            right: layout.incident.left + layout.px(6),
            bottom: layout.incident.bottom,
        },
        layout.px(6),
        RGB_DANGER,
        RGB_DANGER,
    );

    let state = ui_state();
    let inner_left = layout.incident.left + layout.px(24);
    let inner_right = layout.incident.right - layout.px(24);

    draw_text(
        hdc,
        fonts[FONT_LABEL],
        "RUNTIME FAILURE",
        Rect {
            left: inner_left,
            top: layout.incident.top + layout.px(13),
            right: inner_left + layout.px(190),
            bottom: layout.incident.top + layout.px(36),
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
            top: layout.incident.top + layout.px(36),
            right: inner_right,
            bottom: layout.incident.top + layout.px(67),
        },
        RGB_TEXT,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );

    let metric_top = if layout.wide_metrics {
        layout.incident.bottom - layout.px(48)
    } else {
        layout.incident.bottom - layout.px(90)
    };
    draw_text(
        hdc,
        fonts[FONT_BODY],
        &state.issue_summary,
        Rect {
            left: inner_left,
            top: layout.incident.top + layout.px(66),
            right: inner_right,
            bottom: metric_top - layout.px(7),
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
            right: layout.content_left + layout.px(190),
            bottom: layout.section_y + layout.px(22),
        },
        RGB_TEXT_DIM,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        fonts[FONT_META],
        &format!("{}  ·  {}", state.crash_id, state.report_name),
        Rect {
            left: layout.content_left + layout.px(200),
            top: layout.section_y,
            right: layout.content_right,
            bottom: layout.section_y + layout.px(22),
        },
        RGB_TEXT_DIM,
        DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );

    let code_frame = Rect {
        left: layout.content_left,
        top: layout.details.top - layout.px(1),
        right: layout.content_right,
        bottom: layout.details.bottom + layout.px(1),
    };
    fill_round_rect(hdc, &code_frame, layout.px(12), RGB_CODE, RGB_BORDER);

    let divider_y = layout.footer_y - layout.px(12);
    fill_rect_color(
        hdc,
        &Rect {
            left: layout.content_left,
            top: divider_y,
            right: layout.content_right,
            bottom: divider_y + layout.px(1),
        },
        RGB_BORDER,
    );

    if layout.logical_content_width >= 1180 {
        draw_text(
            hdc,
            fonts[FONT_META],
            &format!("{}  ·  {}", state.provider_summary, state.recovery_summary),
            Rect {
                left: layout.content_left + layout.px(820),
                top: layout.footer_y,
                right: layout.content_right,
                bottom: layout.footer_y + layout.px(40),
            },
            RGB_TEXT_DIM,
            DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    }
}

unsafe fn paint_metrics(hdc: Hdc, fonts: &[Hfont], layout: Layout, state: &crate::report::UiState) {
    let left = layout.incident.left + layout.px(24);
    let right = layout.incident.right - layout.px(24);
    let available = right - left;
    let gap = layout.px(10);

    let items = [
        ("ENGINE PHASE", state.phase.as_str()),
        ("BUILD", state.build_summary.as_str()),
        ("RENDERER", state.renderer_summary.as_str()),
        ("GPU", state.gpu_summary.as_str()),
    ];

    if layout.wide_metrics {
        let width = (available - gap * 3) / 4;
        let y = layout.incident.bottom - layout.px(48);
        for (index, (label, value)) in items.into_iter().enumerate() {
            paint_metric(
                hdc,
                fonts,
                layout,
                label,
                value,
                left + index as i32 * (width + gap),
                y,
                width,
            );
        }
    } else {
        let width = (available - gap) / 2;
        let y = layout.incident.bottom - layout.px(90);
        for (index, (label, value)) in items.into_iter().enumerate() {
            let column = index as i32 % 2;
            let row = index as i32 / 2;
            paint_metric(
                hdc,
                fonts,
                layout,
                label,
                value,
                left + column * (width + gap),
                y + row * layout.px(42),
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

    let dpi = dpi_for_window(item.hwnd_item);
    fill_round_rect(item.hdc, &item.rc_item, scale(12, dpi), fill, border);

    if selected {
        fill_round_rect(
            item.hdc,
            &Rect {
                left: item.rc_item.left + scale(18, dpi),
                top: item.rc_item.bottom - scale(4, dpi),
                right: item.rc_item.right - scale(18, dpi),
                bottom: item.rc_item.bottom - scale(1, dpi),
            },
            scale(3, dpi),
            RGB_ACCENT,
            RGB_ACCENT,
        );
    }

    let width = unscale(item.rc_item.right - item.rc_item.left, dpi);
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
    let primary = id == ID_RESTART;
    let recovery_secondary = id == ID_RESTART_SAFE;

    let (fill, border, text_color) = if disabled {
        (RGB_SURFACE, RGB_BORDER, RGB_TEXT_DIM)
    } else if primary {
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
    } else if recovery_secondary {
        (
            if pressed { RGB_SURFACE } else { RGB_PANEL },
            RGB_ACCENT,
            RGB_ACCENT,
        )
    } else if pressed {
        (RGB_SURFACE, RGB_BORDER_STRONG, RGB_TEXT)
    } else {
        (RGB_PANEL, RGB_BORDER_STRONG, RGB_TEXT)
    };

    let dpi = dpi_for_window(item.hwnd_item);
    fill_round_rect(item.hdc, &item.rc_item, scale(12, dpi), fill, border);

    if item.item_state & ODS_FOCUS != 0 && !disabled {
        let focus = Rect {
            left: item.rc_item.left + scale(3, dpi),
            top: item.rc_item.top + scale(3, dpi),
            right: item.rc_item.right - scale(3, dpi),
            bottom: item.rc_item.bottom - scale(3, dpi),
        };
        fill_round_rect(
            item.hdc,
            &focus,
            scale(9, dpi),
            fill,
            if primary { RGB_PANEL } else { RGB_ACCENT },
        );
    }

    draw_text(
        item.hdc,
        handles.fonts[FONT_LABEL],
        action_label(id),
        item.rc_item,
        text_color,
        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
}

unsafe fn paint_metric(
    hdc: Hdc,
    fonts: &[Hfont],
    layout: Layout,
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
        bottom: y + layout.px(37),
    };
    fill_round_rect(hdc, &rect, layout.px(10), RGB_SURFACE, RGB_BORDER);

    draw_text(
        hdc,
        fonts[FONT_META],
        label,
        Rect {
            left: x + layout.px(11),
            top: y + layout.px(3),
            right: x + width - layout.px(10),
            bottom: y + layout.px(18),
        },
        RGB_TEXT_DIM,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    draw_text(
        hdc,
        fonts[FONT_METRIC],
        value,
        Rect {
            left: x + layout.px(11),
            top: y + layout.px(17),
            right: x + width - layout.px(10),
            bottom: y + layout.px(35),
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
        ID_RESTART => "Restart NewViso",
        ID_RESTART_SAFE => "Restart in safe mode",
        ID_OPEN_LOGS => "Open logs",
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

unsafe fn dpi_for_window(hwnd: Hwnd) -> i32 {
    GetDpiForWindow(hwnd).max(BASE_DPI as Uint) as i32
}

fn scale(value: i32, dpi: i32) -> i32 {
    ((value as i64 * dpi as i64 + 48) / 96) as i32
}

fn unscale(value: i32, dpi: i32) -> i32 {
    ((value as i64 * 96 + (dpi as i64 / 2)) / dpi as i64) as i32
}

fn create_fonts(dpi: i32) -> Vec<Hfont> {
    vec![
        create_font(scale(21, dpi), FW_BOLD, "Segoe UI"),
        create_font(scale(10, dpi), FW_MEDIUM, "Segoe UI"),
        create_font(scale(10, dpi), FW_SEMIBOLD, "Segoe UI"),
        create_font(scale(11, dpi), FW_NORMAL, "Segoe UI"),
        create_font(scale(18, dpi), FW_SEMIBOLD, "Segoe UI"),
        create_font(scale(10, dpi), FW_NORMAL, "Cascadia Mono"),
        create_font(scale(11, dpi), FW_SEMIBOLD, "Segoe UI"),
    ]
}
