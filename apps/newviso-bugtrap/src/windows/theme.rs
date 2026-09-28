use super::ffi::*;

pub(crate) const RGB_BG: Colorref = rgb(246, 248, 251);
pub(crate) const RGB_PANEL: Colorref = rgb(255, 255, 255);
pub(crate) const RGB_SURFACE: Colorref = rgb(249, 250, 252);
pub(crate) const RGB_CODE: Colorref = rgb(248, 250, 252);
pub(crate) const RGB_BORDER: Colorref = rgb(225, 229, 236);
pub(crate) const RGB_BORDER_STRONG: Colorref = rgb(211, 217, 226);
pub(crate) const RGB_TEXT: Colorref = rgb(27, 32, 40);
pub(crate) const RGB_TEXT_MUTED: Colorref = rgb(91, 101, 116);
pub(crate) const RGB_TEXT_DIM: Colorref = rgb(128, 138, 151);
pub(crate) const RGB_ACCENT: Colorref = rgb(52, 104, 246);
pub(crate) const RGB_ACCENT_PRESSED: Colorref = rgb(39, 84, 210);
pub(crate) const RGB_DANGER: Colorref = rgb(211, 47, 61);
pub(crate) const RGB_DANGER_SOFT: Colorref = rgb(255, 243, 244);
pub(crate) const RGB_DANGER_BORDER: Colorref = rgb(250, 209, 214);

pub(crate) const FW_NORMAL: i32 = 400;
pub(crate) const FW_MEDIUM: i32 = 500;
pub(crate) const FW_SEMIBOLD: i32 = 600;
pub(crate) const FW_BOLD: i32 = 700;

pub(crate) const FONT_BRAND: usize = 0;
pub(crate) const FONT_META: usize = 1;
pub(crate) const FONT_LABEL: usize = 2;
pub(crate) const FONT_BODY: usize = 3;
pub(crate) const FONT_HEADING: usize = 4;
pub(crate) const FONT_METRIC: usize = 6;

pub(crate) fn create_font(height: i32, weight: i32, face: &str) -> Hfont {
    let face = wide(face);
    unsafe {
        CreateFontW(
            -height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH,
            face.as_ptr(),
        )
    }
}

pub(crate) unsafe fn fill_round_rect(
    hdc: Hdc,
    rect: &Rect,
    radius: i32,
    fill: Colorref,
    border: Colorref,
) {
    let brush = CreateSolidBrush(fill);
    let pen = CreatePen(PS_SOLID, 1, border);
    let previous_brush = SelectObject(hdc, brush as Hgdobj);
    let previous_pen = SelectObject(hdc, pen as Hgdobj);
    RoundRect(
        hdc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius,
        radius,
    );
    SelectObject(hdc, previous_brush);
    SelectObject(hdc, previous_pen);
    DeleteObject(brush as Hgdobj);
    DeleteObject(pen as Hgdobj);
}

pub(crate) unsafe fn fill_rect_color(hdc: Hdc, rect: &Rect, color: Colorref) {
    let brush = CreateSolidBrush(color);
    FillRect(hdc, rect, brush);
    DeleteObject(brush as Hgdobj);
}

pub(crate) unsafe fn draw_text(
    hdc: Hdc,
    font: Hfont,
    text: &str,
    mut rect: Rect,
    color: Colorref,
    flags: Uint,
) {
    let text = wide(text);
    let previous_font = SelectObject(hdc, font as Hgdobj);
    SetTextColor(hdc, color);
    SetBkMode(hdc, TRANSPARENT);
    DrawTextW(hdc, text.as_ptr(), -1, &mut rect, flags);
    SelectObject(hdc, previous_font);
}
