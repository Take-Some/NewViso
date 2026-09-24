#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod report;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    windows::run();
}
