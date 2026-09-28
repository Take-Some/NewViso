use std::{
    ffi::{c_void, OsStr},
    os::windows::ffi::OsStrExt,
};

pub(crate) type Hwnd = *mut c_void;
pub(crate) type Hinstance = *mut c_void;
pub(crate) type Hicon = *mut c_void;
pub(crate) type Hcursor = *mut c_void;
pub(crate) type Hbrush = *mut c_void;
pub(crate) type Hpen = *mut c_void;
pub(crate) type Hmenu = *mut c_void;
pub(crate) type Hglobal = *mut c_void;
pub(crate) type Hgdobj = *mut c_void;
pub(crate) type Hfont = *mut c_void;
pub(crate) type Hdc = *mut c_void;
pub(crate) type Lresult = isize;
pub(crate) type Wparam = usize;
pub(crate) type Lparam = isize;
pub(crate) type Dword = u32;
pub(crate) type Uint = u32;
pub(crate) type Bool = i32;
pub(crate) type Colorref = u32;

pub(crate) const WM_CREATE: Uint = 0x0001;
pub(crate) const WM_DESTROY: Uint = 0x0002;
pub(crate) const WM_CLOSE: Uint = 0x0010;
pub(crate) const WM_PAINT: Uint = 0x000F;
pub(crate) const WM_ERASEBKGND: Uint = 0x0014;
pub(crate) const WM_DRAWITEM: Uint = 0x002B;
pub(crate) const WM_SETFONT: Uint = 0x0030;
pub(crate) const WM_SETICON: Uint = 0x0080;
pub(crate) const WM_COMMAND: Uint = 0x0111;
pub(crate) const WM_CTLCOLOREDIT: Uint = 0x0133;
pub(crate) const WM_CTLCOLORBTN: Uint = 0x0135;
pub(crate) const WM_CTLCOLORSTATIC: Uint = 0x0138;

pub(crate) const EM_SETSEL: Uint = 0x00B1;
pub(crate) const EM_SETMARGINS: Uint = 0x00D3;
pub(crate) const EC_LEFTMARGIN: Wparam = 0x0001;
pub(crate) const EC_RIGHTMARGIN: Wparam = 0x0002;
pub(crate) const STM_SETICON: Uint = 0x0170;

pub(crate) const ICON_SMALL: Wparam = 0;
pub(crate) const ICON_BIG: Wparam = 1;

pub(crate) const WS_OVERLAPPEDWINDOW: Dword = 0x00CF0000;
pub(crate) const WS_VISIBLE: Dword = 0x10000000;
pub(crate) const WS_CHILD: Dword = 0x40000000;
pub(crate) const WS_TABSTOP: Dword = 0x00010000;
pub(crate) const WS_VSCROLL: Dword = 0x00200000;
pub(crate) const WS_CLIPCHILDREN: Dword = 0x02000000;
pub(crate) const ES_MULTILINE: Dword = 0x0004;
pub(crate) const ES_AUTOVSCROLL: Dword = 0x0040;
pub(crate) const ES_AUTOHSCROLL: Dword = 0x0080;
pub(crate) const ES_READONLY: Dword = 0x0800;
pub(crate) const ES_NOHIDESEL: Dword = 0x0100;
pub(crate) const BS_OWNERDRAW: Dword = 0x000B;
pub(crate) const SS_ICON: Dword = 0x00000003;

pub(crate) const ODS_SELECTED: Uint = 0x0001;
pub(crate) const ODS_DISABLED: Uint = 0x0004;
pub(crate) const ODS_FOCUS: Uint = 0x0010;

pub(crate) const DT_LEFT: Uint = 0x0000;
pub(crate) const DT_CENTER: Uint = 0x0001;
pub(crate) const DT_VCENTER: Uint = 0x0004;
pub(crate) const DT_SINGLELINE: Uint = 0x0020;
pub(crate) const DT_NOPREFIX: Uint = 0x0800;
pub(crate) const DT_END_ELLIPSIS: Uint = 0x8000;

pub(crate) const TRANSPARENT: i32 = 1;
pub(crate) const PS_SOLID: i32 = 0;

pub(crate) const CW_USEDEFAULT: i32 = i32::MIN;
pub(crate) const SW_SHOW: i32 = 5;
pub(crate) const SW_SHOWNORMAL: i32 = 1;
pub(crate) const IDC_ARROW: usize = 32512;
pub(crate) const CF_UNICODETEXT: Uint = 13;
pub(crate) const GMEM_MOVEABLE: Uint = 0x0002;

pub(crate) const DEFAULT_CHARSET: Dword = 1;
pub(crate) const OUT_DEFAULT_PRECIS: Dword = 0;
pub(crate) const CLIP_DEFAULT_PRECIS: Dword = 0;
pub(crate) const CLEARTYPE_QUALITY: Dword = 5;
pub(crate) const DEFAULT_PITCH: Dword = 0;
pub(crate) const DWMWA_USE_IMMERSIVE_DARK_MODE: Dword = 20;

#[repr(C)]
pub(crate) struct Point {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Rect {
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) right: i32,
    pub(crate) bottom: i32,
}

#[repr(C)]
pub(crate) struct PaintStruct {
    pub(crate) hdc: Hdc,
    pub(crate) erase: Bool,
    pub(crate) rc_paint: Rect,
    pub(crate) restore: Bool,
    pub(crate) inc_update: Bool,
    pub(crate) rgb_reserved: [u8; 32],
}

#[repr(C)]
pub(crate) struct Msg {
    pub(crate) hwnd: Hwnd,
    pub(crate) message: Uint,
    pub(crate) w_param: Wparam,
    pub(crate) l_param: Lparam,
    pub(crate) time: Dword,
    pub(crate) pt: Point,
    pub(crate) l_private: Dword,
}

#[repr(C)]
pub(crate) struct DrawItemStruct {
    pub(crate) ctl_type: Uint,
    pub(crate) ctl_id: Uint,
    pub(crate) item_id: Uint,
    pub(crate) item_action: Uint,
    pub(crate) item_state: Uint,
    pub(crate) hwnd_item: Hwnd,
    pub(crate) hdc: Hdc,
    pub(crate) rc_item: Rect,
    pub(crate) item_data: usize,
}

pub(crate) type WndProc = unsafe extern "system" fn(Hwnd, Uint, Wparam, Lparam) -> Lresult;

#[repr(C)]
pub(crate) struct WndClassExW {
    pub(crate) cb_size: Uint,
    pub(crate) style: Uint,
    pub(crate) lpfn_wnd_proc: Option<WndProc>,
    pub(crate) cb_cls_extra: i32,
    pub(crate) cb_wnd_extra: i32,
    pub(crate) h_instance: Hinstance,
    pub(crate) h_icon: Hicon,
    pub(crate) h_cursor: Hcursor,
    pub(crate) hbr_background: Hbrush,
    pub(crate) lpsz_menu_name: *const u16,
    pub(crate) lpsz_class_name: *const u16,
    pub(crate) h_icon_sm: Hicon,
}

#[link(name = "user32")]
extern "system" {
    pub(crate) fn RegisterClassExW(class: *const WndClassExW) -> u16;
    pub(crate) fn CreateWindowExW(
        ex_style: Dword,
        class_name: *const u16,
        window_name: *const u16,
        style: Dword,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: Hmenu,
        instance: Hinstance,
        param: *mut c_void,
    ) -> Hwnd;
    pub(crate) fn DefWindowProcW(
        hwnd: Hwnd,
        msg: Uint,
        w_param: Wparam,
        l_param: Lparam,
    ) -> Lresult;
    pub(crate) fn DestroyWindow(hwnd: Hwnd) -> Bool;
    pub(crate) fn PostQuitMessage(exit_code: i32);
    pub(crate) fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min_filter: Uint, max_filter: Uint)
        -> i32;
    pub(crate) fn TranslateMessage(msg: *const Msg) -> Bool;
    pub(crate) fn DispatchMessageW(msg: *const Msg) -> Lresult;
    pub(crate) fn ShowWindow(hwnd: Hwnd, command: i32) -> Bool;
    pub(crate) fn UpdateWindow(hwnd: Hwnd) -> Bool;
    pub(crate) fn SendMessageW(hwnd: Hwnd, msg: Uint, w_param: Wparam, l_param: Lparam) -> Lresult;
    pub(crate) fn SetWindowTextW(hwnd: Hwnd, text: *const u16) -> Bool;
    pub(crate) fn InvalidateRect(hwnd: Hwnd, rect: *const Rect, erase: Bool) -> Bool;
    pub(crate) fn BeginPaint(hwnd: Hwnd, paint: *mut PaintStruct) -> Hdc;
    pub(crate) fn EndPaint(hwnd: Hwnd, paint: *const PaintStruct) -> Bool;
    pub(crate) fn FillRect(hdc: Hdc, rect: *const Rect, brush: Hbrush) -> i32;
    pub(crate) fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> Bool;
    pub(crate) fn SetTextColor(hdc: Hdc, color: Colorref) -> Colorref;
    pub(crate) fn SetBkColor(hdc: Hdc, color: Colorref) -> Colorref;
    pub(crate) fn SetBkMode(hdc: Hdc, mode: i32) -> i32;
    pub(crate) fn DrawTextW(
        hdc: Hdc,
        text: *const u16,
        count: i32,
        rect: *mut Rect,
        format: Uint,
    ) -> i32;
    pub(crate) fn LoadIconW(instance: Hinstance, icon_name: *const u16) -> Hicon;
    pub(crate) fn LoadCursorW(instance: Hinstance, cursor_name: *const u16) -> Hcursor;
    pub(crate) fn OpenClipboard(owner: Hwnd) -> Bool;
    pub(crate) fn EmptyClipboard() -> Bool;
    pub(crate) fn SetClipboardData(format: Uint, memory: Hglobal) -> Hglobal;
    pub(crate) fn CloseClipboard() -> Bool;
}

#[link(name = "kernel32")]
extern "system" {
    pub(crate) fn GetModuleHandleW(module_name: *const u16) -> Hinstance;
    pub(crate) fn GlobalAlloc(flags: Uint, bytes: usize) -> Hglobal;
    pub(crate) fn GlobalLock(memory: Hglobal) -> *mut c_void;
    pub(crate) fn GlobalUnlock(memory: Hglobal) -> Bool;
    pub(crate) fn GlobalFree(memory: Hglobal) -> Hglobal;
}

#[link(name = "gdi32")]
extern "system" {
    pub(crate) fn CreateSolidBrush(color: Colorref) -> Hbrush;
    pub(crate) fn CreatePen(style: i32, width: i32, color: Colorref) -> Hpen;
    pub(crate) fn SelectObject(hdc: Hdc, object: Hgdobj) -> Hgdobj;
    pub(crate) fn RoundRect(
        hdc: Hdc,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        width: i32,
        height: i32,
    ) -> Bool;
    pub(crate) fn DeleteObject(object: Hgdobj) -> Bool;
    pub(crate) fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: Dword,
        underline: Dword,
        strike_out: Dword,
        char_set: Dword,
        out_precision: Dword,
        clip_precision: Dword,
        quality: Dword,
        pitch_and_family: Dword,
        face: *const u16,
    ) -> Hfont;
}

#[link(name = "shell32")]
extern "system" {
    pub(crate) fn ShellExecuteW(
        hwnd: Hwnd,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show_command: i32,
    ) -> *mut c_void;
}

#[link(name = "dwmapi")]
extern "system" {
    pub(crate) fn DwmSetWindowAttribute(
        hwnd: Hwnd,
        attribute: Dword,
        value: *const c_void,
        value_size: Dword,
    ) -> i32;
}

pub(crate) fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value
        .as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub(crate) const fn rgb(r: u8, g: u8, b: u8) -> Colorref {
    (r as Colorref) | ((g as Colorref) << 8) | ((b as Colorref) << 16)
}

pub(crate) fn make_int_resource(id: usize) -> *const u16 {
    id as *const u16
}

pub(crate) const fn make_lparam(low: i32, high: i32) -> Lparam {
    ((low as u32 & 0xFFFF) | ((high as u32 & 0xFFFF) << 16)) as Lparam
}
