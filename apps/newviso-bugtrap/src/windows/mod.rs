mod actions;
mod app;
mod commands;
mod ffi;
mod state;
mod theme;
mod view;

use std::path::PathBuf;

pub(crate) fn run() {
    let report_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("crash.json"));

    state::install_ui_state(crate::report::load_state(&report_path));

    unsafe {
        app::run_window();
    }
}
