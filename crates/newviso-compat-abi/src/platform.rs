use abi_stable::std_types::{ROption, RResult, RString, RVec};

pub const PLATFORM_RUNTIME_RUN_SYMBOL: &[u8] = b"newengine_platform_runtime_run_v1\0";

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformWindowPlacementKindV1 {
    OsDefault = 0,
    Centered = 1,
    Absolute = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PlatformWindowPlacementV1 {
    pub kind: PlatformWindowPlacementKindV1,
    pub x: i32,
    pub y: i32,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformWindowModeV1 {
    Windowed = 0,
    Borderless = 1,
    ExclusiveFullscreen = 2,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformHdrModeV1 {
    Auto = 0,
    Enabled = 1,
    Disabled = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PlatformDisplayConfigV1 {
    pub monitor_index: i32,
    pub window_mode: PlatformWindowModeV1,
    pub vsync: bool,
    pub refresh_rate_millihz: u32,
    pub render_scale: f32,
    pub hdr: PlatformHdrModeV1,
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct PlatformAppIconV1 {
    pub rgba: RVec<u8>,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct PlatformAppConfigV1 {
    pub title: RString,
    pub width: u32,
    pub height: u32,
    pub placement: PlatformWindowPlacementV1,
    pub icon: ROption<PlatformAppIconV1>,
    pub display: PlatformDisplayConfigV1,
}

impl PlatformAppConfigV1 {
    pub fn new(title: impl Into<RString>, width: u32, height: u32) -> Self {
        Self {
            title: title.into(),
            width,
            height,
            placement: PlatformWindowPlacementV1 {
                kind: PlatformWindowPlacementKindV1::Centered,
                x: 0,
                y: 0,
            },
            icon: ROption::RNone,
            display: PlatformDisplayConfigV1 {
                monitor_index: -1,
                window_mode: PlatformWindowModeV1::Windowed,
                vsync: false,
                refresh_rate_millihz: 0,
                render_scale: 1.0,
                hdr: PlatformHdrModeV1::Auto,
            },
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeWindowBackendV1 {
    Unknown = 0,
    Win32 = 1,
    Wayland = 2,
    Xlib = 3,
    Xcb = 4,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeWindowHandlesV1 {
    pub backend: NativeWindowBackendV1,
    pub window: u64,
    pub display: u64,
    pub reserved0: u64,
    pub reserved1: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PlatformSurfaceMetricsV1 {
    pub width: u32,
    pub height: u32,
    pub pixels_per_point: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PlatformWindowReadyV1 {
    pub handles: NativeWindowHandlesV1,
    pub surface: PlatformSurfaceMetricsV1,
    pub display: PlatformDisplayConfigV1,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformCursorGrabModeV1 {
    None = 0,
    Confined = 1,
    Locked = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PlatformCursorStateV1 {
    pub visible: bool,
    pub grab: PlatformCursorGrabModeV1,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PlatformCursorPollV1 {
    pub has_value: bool,
    pub state: PlatformCursorStateV1,
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct PlatformLoadingOverlayV1 {
    pub active: bool,
    pub progress_01: f32,
    pub spinner_phase: u32,
    pub title: RString,
    pub status: RString,
    pub detail: RString,
    pub view_json: RString,
}

impl Default for PlatformLoadingOverlayV1 {
    fn default() -> Self {
        Self {
            active: false,
            progress_01: 0.0,
            spinner_phase: 0,
            title: RString::new(),
            status: RString::new(),
            detail: RString::new(),
            view_json: RString::new(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct PlatformStepResultV1 {
    pub exit_requested: bool,
    pub loading_overlay: PlatformLoadingOverlayV1,
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct PlatformHostTaskRequestV1 {
    pub label: RString,
    pub source: RString,
    pub owner: RString,
    pub category: RString,
    pub lane: RString,
    pub priority: RString,
    pub task_id: RString,
    pub can_cancel: bool,
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct PlatformHostTaskTicketV1 {
    pub accepted: bool,
    pub job_id: RString,
    pub status: RString,
    pub detail: RString,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PlatformHostJobCallbackV1 {
    pub callback_addr: usize,
}

#[repr(C)]
#[derive(Clone)]
pub struct PlatformHostApiV1 {
    pub user_data: usize,
    pub on_window_ready_v1: extern "C" fn(usize, PlatformWindowReadyV1) -> RResult<(), RString>,
    pub on_window_resized_v1:
        extern "C" fn(usize, PlatformSurfaceMetricsV1) -> RResult<(), RString>,
    pub on_window_focused_v1: extern "C" fn(usize, bool) -> RResult<(), RString>,
    pub on_close_requested_v1: extern "C" fn(usize) -> RResult<(), RString>,
    pub step_v1: extern "C" fn(usize, f32) -> RResult<PlatformStepResultV1, RString>,
    pub poll_cursor_state_v1: extern "C" fn(usize) -> PlatformCursorPollV1,
    pub submit_job_v1: extern "C" fn(
        usize,
        PlatformHostTaskRequestV1,
        PlatformHostJobCallbackV1,
        usize,
    ) -> PlatformHostTaskTicketV1,
}
