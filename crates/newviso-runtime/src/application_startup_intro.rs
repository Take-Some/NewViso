//! The startup MP4 is a stage of the main game window's presentation, not
//! another top-level splash. Called from PlatformApplication::on_window_ready,
//! before renderer initialization and the existing same-HWND PreStart overlay.
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
struct Intro {
    path: PathBuf,
    skippable: bool,
    volume: f32,
    duration: Duration,
    open_timeout: Duration,
    fullscreen: bool,
}

fn configured_intro(root: &Path) -> Result<Option<Intro>, String> {
    let config = root.join("intro.toml");
    let mut intro = Intro {
        path: root.join("intro.mp4"),
        skippable: true,
        volume: 0.7,
        duration: Duration::from_millis(12_000),
        open_timeout: Duration::from_millis(15_000),
        fullscreen: true,
    };
    if !config.is_file() {
        return Ok(intro.path.is_file().then_some(intro));
    }
    let source = std::fs::read_to_string(&config)
        .map_err(|error| format!("{}: {error}", config.display()))?;
    let manifest: toml::Value = toml::from_str(&source)
        .map_err(|error| format!("{}: {error}", config.display()))?;
    if manifest.get("schema").and_then(toml::Value::as_str)
        != Some("newengine.startup_intro.v1")
    {
        return Err(format!("{}: unsupported startup intro schema", config.display()));
    }
    if manifest.get("enabled").and_then(toml::Value::as_bool) == Some(false) {
        return Ok(None);
    }
    let Some(track) = manifest.get("sequence").and_then(toml::Value::as_array)
        .and_then(|tracks| tracks.iter().find(|track|
            track.get("enabled").and_then(toml::Value::as_bool) != Some(false)))
    else {
        return Ok(None);
    };
    let Some(name) = track.get("source").and_then(toml::Value::as_str) else {
        return Err("startup intro track lacks source".into());
    };
    let movie = PathBuf::from(name);
    intro.path = if movie.is_absolute() { movie } else { root.join(movie) };
    if !intro.path.is_file() {
        return Err(format!("intro file does not exist: {}", intro.path.display()));
    }
    intro.skippable = track.get("skippable").and_then(toml::Value::as_bool).unwrap_or(true);
    intro.volume = track.get("volume").and_then(toml::Value::as_float)
        .unwrap_or(0.7).clamp(0.0, 1.0) as f32;
    intro.duration = Duration::from_millis(
        track.get("max_duration_ms").and_then(toml::Value::as_integer)
            .unwrap_or(12_000).clamp(500, 120_000) as u64,
    );
    intro.fullscreen = match manifest.get("window").and_then(|v| v.get("mode"))
        .and_then(toml::Value::as_str).unwrap_or("fullscreen") {
            "fullscreen" | "borderless" => true,
            "windowed" => false,
            other => return Err(format!("invalid intro window.mode: {other}")),
        };
    intro.open_timeout = Duration::from_millis(
        manifest.get("window").and_then(|window| window.get("failure_timeout_ms"))
            .and_then(toml::Value::as_integer)
            .unwrap_or(15_000).clamp(1000, 60_000) as u64,
    );
    Ok(Some(intro))
}

/// Same HWND is supplied by Winit through the platform ABI; never open one.
pub(super) fn play_if_present(game_hwnd: u64) -> Result<(), String> {
    if game_hwnd == 0 { return Ok(()); }
    let Some(root) = std::env::var_os("NEWVISO_CONTENT_DIR").map(PathBuf::from) else {
        return Ok(());
    };
    let Some(intro) = configured_intro(&root)? else { return Ok(()); };
    #[cfg(windows)]
    {
        let outcome = native::play_in_game_window(game_hwnd, &intro)?;
        newviso_host::info("newviso.intro", format!(
            "same-HWND movie outcome={outcome} hwnd=0x{game_hwnd:x} path={}", intro.path.display()
        ));
    }
    #[cfg(not(windows))]
    {
        let _ = (intro, game_hwnd);
    }
    Ok(())
}

#[cfg(windows)]
mod native {
    use super::Intro;
    use std::{os::windows::ffi::OsStrExt, thread, time::Instant};
    use windows::{
        core::{PCWSTR, w},
        Win32::{
            Foundation::{HWND, RECT, COLORREF},
            Graphics::Dwm::DwmFlush,
            Graphics::Gdi::{                MonitorFromWindow, GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTONEAREST,
                GetDC, ReleaseDC, FillRect, CreateSolidBrush, DeleteObject, HGDIOBJ,
                GdiFlush,
            },
            Media::MediaFoundation::{
                MFPCreateMediaPlayer, IMFPMediaPlayer, IMFPMediaPlayerCallback,
                MFP_OPTION_NONE, MFP_MEDIAPLAYER_STATE_PLAYING,
                MFP_MEDIAPLAYER_STATE_STOPPED, MFStartup, MFShutdown, MF_VERSION, MFSTARTUP_FULL,
            },
            System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
            UI::WindowsAndMessaging::{
                DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
                WM_LBUTTONDOWN, WM_KEYDOWN, WM_CLOSE, WM_QUIT,
                SetWindowPos, HWND_NOTOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_NOACTIVATE,
                SWP_SHOWWINDOW, SWP_FRAMECHANGED, SWP_NOCOPYBITS,
                GetWindowRect, GetClientRect, GetWindowLongPtrW, SetWindowLongPtrW,
                CreateWindowExW, DestroyWindow, IsChild, WINDOW_EX_STYLE,
                WS_CHILD, WS_VISIBLE, WS_CLIPSIBLINGS,
                GWL_STYLE, WS_POPUP, WS_OVERLAPPEDWINDOW,
            },
        },
    };

    /// Temporary borderless fullscreen presentation on the already existing
    /// game HWND. On drop, restore the EXACT original window style/rectangle;
    /// Vulkan and gameplay subsequently use the unchanged HWND.
    struct IntroWindowFrame {
        hwnd: HWND,
        original_style: isize,
        original_rect: RECT,
        changed: bool,
    }

    impl IntroWindowFrame {
        fn enter(hwnd: HWND, fullscreen: bool) -> Result<Self, String> {
            let mut rect = RECT::default();
            unsafe { GetWindowRect(hwnd, &mut rect) }
                .map_err(|e| format!("GetWindowRect: {e}"))?;
            let original_style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
            let mut frame = Self { hwnd, original_style, original_rect: rect, changed: false };
            // Winit temporarily elevates the game window for bootstrap; the
            // authored intro config requires a normal window Z-level.
            let _ = unsafe { SetWindowPos(
                hwnd, Some(HWND_NOTOPMOST), 0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            ) };
            if !fullscreen {
                return Ok(frame);
            }

            let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default() };
            if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
                return Err("GetMonitorInfoW failed for intro fullscreen".into());
            }
            let bounds = info.rcMonitor;
            let width = bounds.right - bounds.left;
            let height = bounds.bottom - bounds.top;
            if width <= 0 || height <= 0 {
                return Err("fullscreen monitor bounds are empty".into());
            }
            let fullscreen_style = (original_style & !(WS_OVERLAPPEDWINDOW.0 as isize))
                | WS_POPUP.0 as isize;
            unsafe { SetWindowLongPtrW(hwnd, GWL_STYLE, fullscreen_style) };
            frame.changed = true;
            unsafe { SetWindowPos(
                hwnd, Some(HWND_NOTOPMOST),
                bounds.left, bounds.top, width, height,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOACTIVATE | SWP_NOCOPYBITS,
            ) }.map_err(|e| format!("fullscreen intro window: {e}"))?;
            newviso_host::info("newviso.intro",format!(
                "fullscreen same-HWND hwnd=0x{:x} monitor={}x{} origin={},{}",
                hwnd.0 as usize, width, height, bounds.left, bounds.top,
            ));
            Ok(frame)
        }

        fn restore(&mut self) {
            if !self.changed { return; }
            unsafe { SetWindowLongPtrW(self.hwnd, GWL_STYLE, self.original_style) };
            let original = self.original_rect;
            let _ = unsafe { SetWindowPos(
                self.hwnd, Some(HWND_NOTOPMOST), original.left, original.top,
                original.right - original.left,
                original.bottom - original.top,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOACTIVATE | SWP_NOCOPYBITS,
            ) };
            self.changed = false;
            newviso_host::info("newviso.intro", format!(
                "game window frame restored hwnd=0x{:x} rect={}x{}",
                self.hwnd.0 as usize,
                original.right - original.left,
                original.bottom - original.top,
            ));
        }
    }

    impl Drop for IntroWindowFrame {
        fn drop(&mut self) { self.restore(); }
    }

    /// EVR redirects the HWND passed to MFPlay through its own compositor.
    /// Attaching it directly to the game HWND leaves a terminal redirected
    /// video frame *above* Vulkan after Shutdown. A child surface isolates that
    /// redirection and is destroyed before the engine reuses its game HWND.
    /// This is not a second top-level window: it always fills the game frame.
    struct IntroVideoSurface {
        hwnd: HWND,
        owner: HWND,
    }

    impl IntroVideoSurface {
        fn create(owner: HWND) -> Result<Self, String> {
            let mut bounds = RECT::default();
            unsafe { GetClientRect(owner, &mut bounds) }
                .map_err(|error| format!("intro parent client rect: {error}"))?;
            let width = bounds.right - bounds.left;
            let height = bounds.bottom - bounds.top;
            if width <= 0 || height <= 0 {
                return Err("intro cannot create video surface on empty game frame".into());
            }
            let hwnd = unsafe { CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"), w!(""),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
                0, 0, width, height,
                Some(owner), None, None, None,
            ) }.map_err(|error| format!("same-frame video surface: {error}"))?;
            newviso_host::info("newviso.intro", format!(
                "intro child surface attached owner=0x{:x} video=0x{:x} extent={}x{}",
                owner.0 as usize, hwnd.0 as usize, width, height,
            ));
            Ok(Self { hwnd, owner })
        }
    }

    impl Drop for IntroVideoSurface {
        fn drop(&mut self) {
            // Shutdown and release MFPlay first. Destroying only this child
            // forces DWM to discard the redirected EVR target; the parent's
            // Vulkan-compatible native HWND and Winit window remain intact.
            if let Err(error) = unsafe { DestroyWindow(self.hwnd) } {
                newviso_host::warn("newviso.intro", format!(
                    "failed to destroy retired video surface: {error}"
                ));
            }
            newviso_host::info("newviso.intro", format!(
                "intro video surface destroyed owner=0x{:x} video=0x{:x}",
                self.owner.0 as usize, self.hwnd.0 as usize,
            ));
        }
    }

    /// Replace the last EVR surface with a defined black image after MFPlay
    /// Shutdown, including on left-click/ESC skip. No second top-level window.
    fn clear_old_video_frame(hwnd: HWND) -> Result<(), String> {
        let mut rect = RECT::default();
        unsafe { GetClientRect(hwnd, &mut rect) }
            .map_err(|e| format!("GetClientRect: {e}"))?;
        let dc = unsafe { GetDC(Some(hwnd)) };
        if dc.is_invalid() {
            return Err("GetDC failed during intro clear".into());
        }
        let brush = unsafe { CreateSolidBrush(COLORREF(0)) };
        let painted = unsafe { FillRect(dc, &rect, brush) };
        unsafe {
            let _ = GdiFlush();
            let _ = DeleteObject(HGDIOBJ(brush.0));
            ReleaseDC(Some(hwnd), dc);
        }
        if painted == 0 {
            return Err("FillRect failed during intro clear".into());
        }
        // DWM/EVR can retain a redirected video surface across native resize.
        // Flush the first black replacement BEFORE handing this HWND to
        // PreStart's DXGI loading swapchain (which has its own present barrier).
        let _ = unsafe { DwmFlush() };
        newviso_host::info("newviso.intro",format!(
            "old movie frame cleared hwnd=0x{:x} rect={}x{}",
            hwnd.0 as usize, rect.right - rect.left, rect.bottom - rect.top,
        ));
        Ok(())
    }

    pub(super) fn play_in_game_window(hwnd_value: u64, intro: &Intro) -> Result<&'static str, String> {
        // Runs on Winit's actual game HWND thread. Pump native messages during
        // video playback so left-click/ESC can skip without hanging the window.
        let hwnd = HWND(hwnd_value as usize as *mut core::ffi::c_void);
        let mut frame = IntroWindowFrame::enter(hwnd, intro.fullscreen)?;
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok().map_err(|error| format!("CoInitializeEx: {error}"))?;
        let result = (|| {
            unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }
                .map_err(|error| format!("MFStartup: {error}"))?;
            let outcome = match IntroVideoSurface::create(hwnd) {
                Ok(video) => {
                    let result = play(intro, video.hwnd, hwnd);
                    drop(video);
                    result
                }
                Err(error) => Err(error),
            };
            let _ = unsafe { MFShutdown() };
            outcome
        })();
        unsafe { CoUninitialize() };
        // MFPlay / EVR owns its own presentation buffers: never leave its
        // terminal frame resident after a skip or natural completion.
        let cleared_fullscreen = clear_old_video_frame(hwnd);
        frame.restore();
        let cleared_game_frame = clear_old_video_frame(hwnd);
        for result in [cleared_fullscreen, cleared_game_frame] {
            if let Err(error) = result {
                newviso_host::warn("newviso.intro", format!(
                    "same-HWND video clear was incomplete: {error}"
                ));
            }
        }
        result
    }

    fn play(intro: &Intro, hwnd: HWND, game_hwnd: HWND) -> Result<&'static str, String> {
        let file: Vec<u16> = intro.path.as_os_str().encode_wide().chain([0]).collect();
        let mut player: Option<IMFPMediaPlayer> = None;
        let callback: Option<&IMFPMediaPlayerCallback> = None;
        unsafe {
            MFPCreateMediaPlayer(
                PCWSTR(file.as_ptr()), true, MFP_OPTION_NONE,
                callback, Some(hwnd), Some(&mut player),
            )
        }.map_err(|error| format!("MFPlay existing game HWND: {error}"))?;
        let result = (|| {
            let player = player.as_ref().ok_or("MFPlay returned no player")?;
            let _ = unsafe { player.SetVolume(intro.volume) };
            let opened_at = Instant::now();
            let mut playing_at: Option<Instant> = None;
            loop {
                let mut message = MSG::default();
                for _ in 0..64 {
                    if !unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {break;}
                    if message.message == WM_CLOSE || message.message == WM_QUIT {
                        return Ok("window_closed");
                    }
                    if intro.skippable && (message.hwnd == hwnd
                        || message.hwnd == game_hwnd
                        || unsafe { IsChild(hwnd, message.hwnd) }.as_bool())
                    {
                        if message.message == WM_LBUTTONDOWN ||
                            (message.message == WM_KEYDOWN && message.wParam.0 == 0x1b)
                        {
                            // The video child captures the click. Consume the
                            // gesture without forwarding it to game input.
                            return Ok("skipped_by_user");
                        }
                    }
                    let _ = unsafe { TranslateMessage(&message) };
                    unsafe { DispatchMessageW(&message) };
                }
                if let Ok(state) = unsafe { player.GetState() } {
                    if state == MFP_MEDIAPLAYER_STATE_PLAYING && playing_at.is_none() {
                        playing_at = Some(Instant::now());
                        let _ = unsafe { player.SetVolume(intro.volume) };
                        // Don't letterbox the clip in a small centered area:
                        // EVR must fill the fullscreen game's entire client
                        // frame, independently of the source aspect ratio.
                        if let Err(error) = unsafe { player.SetAspectRatioMode(0) } {
                            newviso_host::warn("newviso.intro", format!(
                                "video fill-mode unavailable: {error}"
                            ));
                        }
                        let _ = unsafe { player.UpdateVideo() };
                        newviso_host::info("newviso.intro",format!(
                            "fullscreen intro video active video_hwnd=0x{:x} game_hwnd=0x{:x} aspect='stretch-to-client'",
                            hwnd.0 as usize, game_hwnd.0 as usize,
                        ));
                    }
                    if state == MFP_MEDIAPLAYER_STATE_STOPPED && playing_at.is_some() {
                        return Ok("completed");
                    }
                }
                if let Some(started) = playing_at {
                    if started.elapsed() >= intro.duration { return Ok("duration_elapsed"); }
                } else if opened_at.elapsed() >= intro.open_timeout {
                    return Err("media did not start before timeout".into());
                }
                thread::sleep(std::time::Duration::from_millis(8));
            }
        })();
        if let Some(player) = player.as_ref() {
            let _ = unsafe { player.Shutdown() };
        }
        drop(player);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_config_and_movie_are_optional() {
        assert!(configured_intro(Path::new("__newviso_nonexistent_intro_directory__")).unwrap().is_none());
    }

    #[test]
    fn existing_intro_manifest_is_respected_and_can_be_disabled() {
        let directory = std::env::temp_dir()
            .join(format!("newviso-same-hwnd-intro-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("intro.mp4"), b"dummy").unwrap();
        std::fs::write(directory.join("intro.toml"), r#"
schema = "newengine.startup_intro.v1"
enabled = true
[window]
topmost = false
failure_timeout_ms = 15000
[[sequence]]
source = "intro.mp4"
enabled = true
skippable = true
max_duration_ms = 12000
volume = 0.7
"#).unwrap();
        let intro = configured_intro(&directory).unwrap().unwrap();
        assert_eq!(intro.path, directory.join("intro.mp4"));
        assert!(intro.skippable);
        assert!(intro.fullscreen);
        assert_eq!(intro.duration, Duration::from_millis(12_000));
        std::fs::write(directory.join("intro.toml"),
            "schema = \"newengine.startup_intro.v1\"\nenabled = false\n").unwrap();
        assert!(configured_intro(&directory).unwrap().is_none());
        let _ = std::fs::remove_dir_all(directory);
    }
}
