use super::ffi::*;

pub(crate) const RGB_BG: Colorref = rgb(245, 246, 248);
pub(crate) const RGB_PANEL: Colorref = rgb(255, 255, 255);
pub(crate) const RGB_TEXT: Colorref = rgb(17, 24, 39);

pub(crate) const FW_NORMAL: i32 = 400;
pub(crate) const FW_SEMIBOLD: i32 = 600;
pub(crate) const FW_BOLD: i32 = 700;

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
