use abi_stable::std_types::{RResult, RString};
use libloading::{Library, Symbol};
use newviso_compat_abi::{
    platform::{
        NativeWindowBackendV1, PlatformAppConfigV1, PlatformCursorGrabModeV1, PlatformCursorPollV1,
        PlatformCursorStateV1, PlatformHostApiV1, PlatformHostJobCallbackV1,
        PlatformHostTaskRequestV1, PlatformHostTaskTicketV1, PlatformLoadingOverlayV1,
        PlatformStepResultV1, PlatformSurfaceMetricsV1, PlatformWindowReadyV1,
    },
    provider::HostApiV1,
};
use newviso_host as host;
use std::path::{Path, PathBuf};

pub const PLATFORM_RUNTIME_RUN_SYMBOL: &[u8] = b"newengine_platform_runtime_run_v1\0";

type PlatformRuntimeRunFnV1 =
    unsafe extern "C" fn(HostApiV1, PlatformHostApiV1, PlatformAppConfigV1) -> RResult<(), RString>;

pub trait PlatformApplication {
    fn on_window_ready(&mut self, _ready: PlatformWindowReadyV1) -> Result<(), String> {
        Ok(())
    }

    fn on_window_resized(&mut self, _surface: PlatformSurfaceMetricsV1) -> Result<(), String> {
        Ok(())
    }

    fn on_window_focused(&mut self, _focused: bool) -> Result<(), String> {
        Ok(())
    }

    fn on_close_requested(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// Returns true when the application itself requests exit.
    fn step(&mut self, dt: f32, surface: PlatformSurfaceMetricsV1) -> Result<bool, String>;

    fn cursor_state(&mut self) -> PlatformCursorPollV1 {
        PlatformCursorPollV1 {
            has_value: false,
            state: PlatformCursorStateV1 {
                visible: true,
                grab: PlatformCursorGrabModeV1::None,
            },
        }
    }

    fn shutdown(&mut self) {}
}

#[derive(Clone, Debug)]
pub struct PlatformRunConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub frame_limit: Option<u64>,
}

impl Default for PlatformRunConfig {
    fn default() -> Self {
        Self {
            title: "NewViso".to_owned(),
            width: 1280,
            height: 720,
            frame_limit: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlatformRunReport {
    pub path: PathBuf,
    pub frames: u64,
    pub emitted_events: u64,
    pub window_ready: bool,
    pub backend: Option<NativeWindowBackendV1>,
    pub width: u32,
    pub height: u32,
    pub close_requested: bool,
    pub app_error: Option<String>,
}

struct PlatformState {
    app: Box<dyn PlatformApplication>,
    frame_limit: Option<u64>,
    frames: u64,
    window_ready: bool,
    backend: Option<NativeWindowBackendV1>,
    surface: PlatformSurfaceMetricsV1,
    close_requested: bool,
    app_error: Option<String>,
    shutdown_called: bool,
}

impl PlatformState {
    fn fail(&mut self, error: String) {
        if self.app_error.is_none() {
            self.app_error = Some(error);
        }
        self.close_requested = true;
    }

    fn shutdown_app(&mut self) {
        if !self.shutdown_called {
            self.app.shutdown();
            self.shutdown_called = true;
        }
    }
}

pub fn run_platform(
    path: impl AsRef<Path>,
    config: PlatformRunConfig,
    app: Box<dyn PlatformApplication>,
) -> Result<PlatformRunReport, String> {
    let path = path.as_ref();
    host::reset_event_count();

    let library = unsafe { Library::new(path) }
        .map_err(|error| format!("platform DLL load failed: {error}"))?;
    let run: Symbol<PlatformRuntimeRunFnV1> = unsafe {
        library
            .get(PLATFORM_RUNTIME_RUN_SYMBOL)
            .map_err(|error| format!("platform runtime entrypoint missing: {error}"))?
    };

    let mut state = Box::new(PlatformState {
        app,
        frame_limit: config.frame_limit,
        frames: 0,
        window_ready: false,
        backend: None,
        surface: PlatformSurfaceMetricsV1 {
            width: config.width,
            height: config.height,
            pixels_per_point: 1.0,
        },
        close_requested: false,
        app_error: None,
        shutdown_called: false,
    });

    let api = PlatformHostApiV1 {
        user_data: (&mut *state as *mut PlatformState) as usize,
        on_window_ready_v1: on_window_ready,
        on_window_resized_v1: on_window_resized,
        on_window_focused_v1: on_window_focused,
        on_close_requested_v1: on_close_requested,
        step_v1: step,
        poll_cursor_state_v1: poll_cursor_state,
        submit_job_v1: submit_job,
    };

    let plugin_host = host::host_api();
    let app_config = PlatformAppConfigV1::new(config.title, config.width, config.height);

    let result = unsafe { run(plugin_host, api, app_config) };
    state.shutdown_app();
    host::clear_platform_snapshot();

    if let RResult::RErr(error) = result {
        return Err(format!("platform runtime failed: {error}"));
    }

    Ok(PlatformRunReport {
        path: path.to_path_buf(),
        frames: state.frames,
        emitted_events: host::emitted_event_count(),
        window_ready: state.window_ready,
        backend: state.backend,
        width: state.surface.width,
        height: state.surface.height,
        close_requested: state.close_requested,
        app_error: state.app_error.clone(),
    })
}

extern "C" fn on_window_ready(
    user_data: usize,
    ready: PlatformWindowReadyV1,
) -> RResult<(), RString> {
    let state = unsafe { &mut *(user_data as *mut PlatformState) };
    state.window_ready = true;
    state.backend = Some(ready.handles.backend);
    state.surface = ready.surface;
    host::set_platform_snapshot(ready);

    if let Err(error) = state.app.on_window_ready(ready) {
        state.fail(error);
    }
    RResult::ROk(())
}

extern "C" fn on_window_resized(
    user_data: usize,
    surface: PlatformSurfaceMetricsV1,
) -> RResult<(), RString> {
    let state = unsafe { &mut *(user_data as *mut PlatformState) };
    state.surface = surface;
    if let Err(error) = state.app.on_window_resized(surface) {
        state.fail(error);
    }
    RResult::ROk(())
}

extern "C" fn on_window_focused(user_data: usize, focused: bool) -> RResult<(), RString> {
    let state = unsafe { &mut *(user_data as *mut PlatformState) };
    if let Err(error) = state.app.on_window_focused(focused) {
        state.fail(error);
    }
    RResult::ROk(())
}

extern "C" fn on_close_requested(user_data: usize) -> RResult<(), RString> {
    let state = unsafe { &mut *(user_data as *mut PlatformState) };
    state.close_requested = true;
    if let Err(error) = state.app.on_close_requested() {
        state.fail(error);
    }
    RResult::ROk(())
}

extern "C" fn step(user_data: usize, dt: f32) -> RResult<PlatformStepResultV1, RString> {
    let state = unsafe { &mut *(user_data as *mut PlatformState) };
    state.frames = state.frames.saturating_add(1);

    let app_exit = match state.app.step(dt, state.surface) {
        Ok(exit) => exit,
        Err(error) => {
            state.fail(error);
            true
        }
    };

    let frame_limit_reached = state
        .frame_limit
        .is_some_and(|limit| state.frames >= limit.max(1));
    let exit_requested = state.close_requested || app_exit || frame_limit_reached;

    if exit_requested {
        state.shutdown_app();
    }

    RResult::ROk(PlatformStepResultV1 {
        exit_requested,
        loading_overlay: PlatformLoadingOverlayV1::default(),
    })
}

extern "C" fn poll_cursor_state(user_data: usize) -> PlatformCursorPollV1 {
    let state = unsafe { &mut *(user_data as *mut PlatformState) };
    state.app.cursor_state()
}

extern "C" fn submit_job(
    _user_data: usize,
    request: PlatformHostTaskRequestV1,
    _callback: PlatformHostJobCallbackV1,
    _callback_user_data: usize,
) -> PlatformHostTaskTicketV1 {
    PlatformHostTaskTicketV1 {
        accepted: false,
        job_id: request.task_id,
        status: RString::from("not-submitted"),
        detail: RString::from("NewViso platform layer does not own the job scheduler"),
    }
}
