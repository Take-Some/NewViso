use crate::report::UiState;
use std::sync::{Mutex, OnceLock};

use super::ffi::{Hbrush, Hfont, Hwnd};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Page {
    Overview,
    Exception,
    Stack,
    Engine,
    System,
    Files,
}

pub(crate) struct UiHandles {
    pub(crate) details: Hwnd,
    pub(crate) tabs: [Hwnd; 6],
    pub(crate) current_page: Page,
    pub(crate) brushes: Vec<Hbrush>,
    pub(crate) fonts: Vec<Hfont>,
}

unsafe impl Send for UiHandles {}

static UI_STATE: OnceLock<UiState> = OnceLock::new();
static UI_HANDLES: OnceLock<Mutex<UiHandles>> = OnceLock::new();

pub(crate) fn install_ui_state(state: UiState) {
    let _ = UI_STATE.set(state);
}

pub(crate) fn ui_state() -> &'static UiState {
    UI_STATE.get().expect("BugTrap UI state")
}

pub(crate) fn install_handles(handles: UiHandles) {
    let _ = UI_HANDLES.set(Mutex::new(handles));
}

pub(crate) fn handles() -> Option<&'static Mutex<UiHandles>> {
    UI_HANDLES.get()
}
