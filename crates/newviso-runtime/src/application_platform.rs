use super::*;

mod frame;
mod performance;

/// The native boot compositor yields to project-owned presentation.
fn project_ui_owns_loading_presentation(
    menu_visible: bool,
    launch_requested: bool,
    authored_loader_visible: bool,
) -> bool {
    menu_visible || launch_requested || authored_loader_visible
}

pub(super) fn benchmark_autostart_enabled() -> bool {
    std::env::var("NEWVISO_PERF_TRACE").as_deref() == Ok("1")
        && std::env::var("NEWVISO_BENCHMARK_AUTOSTART").as_deref() == Ok("1")
}

/// Startup argument recognized by the executable; all other initialization is unaffected.
fn has_no_intro_flag(args: impl IntoIterator<Item = std::ffi::OsString>) -> bool {
    args.into_iter()
        .any(|arg| matches!(arg.to_str(), Some("-noIntro" | "--no-intro")))
}

fn skip_intro_requested() -> bool {
    has_no_intro_flag(std::env::args_os().skip(1))
}

#[cfg(test)]
mod intro_launch_tests {
    use super::{has_no_intro_flag, project_ui_owns_loading_presentation};

    #[test]
    fn native_boot_presentation_yields_to_project_loading() {
        assert!(!project_ui_owns_loading_presentation(false, false, false));
        assert!(project_ui_owns_loading_presentation(true, false, false));
        assert!(project_ui_owns_loading_presentation(false, true, false));
        assert!(project_ui_owns_loading_presentation(false, false, true));
        assert!(project_ui_owns_loading_presentation(false, true, true));
    }

    #[test]
    fn skip_intro_matches_exact_switches_only() {
        for flag in ["-noIntro", "--no-intro"] {
            assert!(has_no_intro_flag(["--project", "FirstFPS", flag].map(Into::into)));
        }
        for flag in ["-noIntroExtra", "--intro", "-NOINTRO"] {
            assert!(!has_no_intro_flag([flag].map(Into::into)));
        }
    }
}

impl PlatformApplication for EngineApplication {
    fn on_window_ready(&mut self, ready: PlatformWindowReadyV1) -> Result<(), String> {
        // Platform has already created the permanent game HWND. Present the
        // optional startup movie directly on it, before Vulkan or map loading.
        // Missing/broken media is not fatal to the project.
        if !self.editor_mode && !benchmark_autostart_enabled() && !skip_intro_requested() {
            if let Err(error) = application_startup_intro::play_if_present(ready.handles.window) {
                host::warn(
                    "newviso.intro",
                    format!("same-window intro unavailable: {error}"),
                );
            }
        }
        bugtrap::checkpoint("render.provider.load");
        bugtrap::set_context(
            "renderer_provider_path",
            self.renderer_path.display().to_string(),
        );
        let mut renderer = RunningProvider::load_current(&self.renderer_path)?;
        bugtrap::checkpoint("render.provider.initialize");
        renderer.initialize()?;
        bugtrap::set_context("renderer_provider_runtime_id", renderer.id().to_owned());
        if let Ok(info) = host::call_json("engine.render", "info_json", &json!({})) {
            bugtrap::set_context("renderer_info_json", info.to_string());
            if let Some(value) = info.get("backend_name").and_then(Value::as_str) {
                bugtrap::set_context("renderer_backend_name", value.to_owned());
            }
            if let Some(value) = info.get("backend_version").and_then(Value::as_str) {
                bugtrap::set_context("renderer_backend_version", value.to_owned());
            }
            if let Some(value) = info.get("backend_id").and_then(Value::as_str) {
                bugtrap::set_context("renderer_backend_id", value.to_owned());
            }
        }

        host::info(
            "newviso.runtime",
            format!("renderer '{}' initialized on live window", renderer.id()),
        );

        self.renderer = Some(renderer);

        let startup_commands = self.settings.startup_commands.clone();
        self.apply_script_commands(&startup_commands)?;

        host::publish_event_json(
            event_topic::RUNTIME_STARTED,
            "newviso.runtime",
            json!({
                "scene": self.scene.title()
            }),
        )?;

        // Benchmark-only autostart takes the same residency-gated Play path
        // as the front-end button. Ordinary project behavior is unchanged.
        if benchmark_autostart_enabled() {
            if let Some(menu) = self.main_menu.as_mut() {
                menu.launch_requested = true;
                menu.visible = false;
                host::info("newviso.benchmark",
                    "autostart requested: waiting for ordinary map residency");
            }
        }
        if self.main_menu.is_none() {
            self.start_project_scripts()?;
        }

        bugtrap::checkpoint("scene.renderer.initialize");
        self.update_climate(0.0)?;
        self.scene.initialize_renderer()?;

        self.publish_bound_ui_if_changed()?;
        self.publish_content_manager_if_changed()?;

        self.ready = true;
        self.world_save_allowed = self.main_menu.is_none();

        host::info(
            "newviso.runtime",
            format!("3D scene '{}' initialized", self.scene.title()),
        );
        Ok(())
    }

    fn step(&mut self, dt: f32, surface: PlatformSurfaceMetricsV1) -> Result<bool, String> {
        self.step_frame(dt, surface)
    }

    fn loading_overlay(&self) -> PlatformLoadingOverlayV1 {
        // Single-HWND presentation ownership: an authored project loading
        // screen (or a pending Play action) preempts the Winit boot compositor.
        // Otherwise two separate threads present alternating fullscreen frames
        // to the same window during map streaming, producing visible flicker.
        if self.ready || project_ui_owns_loading_presentation(
            self.main_menu.as_ref().is_some_and(|menu| menu.visible),
            self.main_menu.as_ref().is_some_and(|menu| menu.launch_requested),
            self.loading_ui_visible,
        ) {
            return PlatformLoadingOverlayV1::default();
        }
        PlatformLoadingOverlayV1::default()
    }

    fn on_window_focused(&mut self, focused: bool) -> Result<(), String> {
        let previous_focus = self.window_focused;
        let previous_capture = self.cursor_captured;

        self.window_focused = focused;
        if !focused {
            self.cursor_captured = false;
        }

        if previous_focus != focused {
            host::publish_event_json(
                event_topic::WINDOW_FOCUS_CHANGED,
                "newviso.platform",
                json!({
                    "focused": focused,
                    "previous": previous_focus
                }),
            )?;
        }
        if previous_capture != self.cursor_captured {
            host::publish_event_json(
                event_topic::CURSOR_CAPTURE_CHANGED,
                "newviso.runtime",
                json!({
                    "captured": self.cursor_captured,
                    "previous": previous_capture,
                    "reason": "window_focus"
                }),
            )?;
        }
        Ok(())
    }

    fn cursor_state(&mut self) -> PlatformCursorPollV1 {
        let captured = self.ready && self.window_focused && self.cursor_captured;
        PlatformCursorPollV1 {
            has_value: true,
            state: PlatformCursorStateV1 {
                visible: !captured,
                grab: if captured {
                    PlatformCursorGrabModeV1::Locked
                } else {
                    PlatformCursorGrabModeV1::None
                },
            },
        }
    }

    fn shutdown(&mut self) {
        bugtrap::set_phase("runtime.shutdown");
        if let Err(error) = host::publish_event_json(
            event_topic::RUNTIME_SHUTDOWN,
            "newviso.runtime",
            json!({
                "elapsed_seconds": self.elapsed_seconds
            }),
        ) {
            host::warn(
                "newviso.events",
                format!("failed to publish runtime shutdown event: {error}"),
            );
        }

        self.cursor_captured = false;

        // Capture diagnostics while scene streaming ownership is still intact.
        // Releasing claims first erased the very dependency state needed to
        // diagnose shutdown-time residency/materialization problems.
        host::info(
            "newviso.runtime",
            format!("pre-shutdown runtime state: {}", self.runtime_state()),
        );

        let claims = std::mem::take(&mut self.scene_stream_claims);
        for (stable_id, address) in claims {
            self.asset_streamer
                .release(Self::scene_stream_owner(stable_id), &address);
        }
        let shutdown_control = if let Some(scripts) = self.scripts.as_mut() {
            match scripts.shutdown(&self.project_context) {
                Ok(control) => Some(control),
                Err(error) => {
                    host::warn(
                        "newviso.scripting",
                        format!("script shutdown hook failed: {error}"),
                    );
                    self.world_save_allowed = false;
                    None
                }
            }
        } else {
            None
        };
        if let Some(control) = shutdown_control {
            if let Err(error) = self.apply_script_commands(&control.commands) {
                host::warn(
                    "newviso.scripting",
                    format!("script shutdown commands failed: {error}"),
                );
                self.world_save_allowed = false;
            }
        }

        self.stop_script_audio_loops();
        self.save_world_on_shutdown();
        host::info(
            "newviso.runtime",
            format!("final runtime state: {}", self.runtime_state()),
        );

        // Provider event sinks are ABI trait objects whose vtables live in the
        // subscribing DLLs. The renderer is the first transient provider that
        // will be unloaded, so release every sink now while renderer, input and
        // all bootstrap providers are still resident.
        bugtrap::checkpoint("host.event_sinks.release");
        let released_event_sinks = host::clear_event_sinks();
        host::debug(
            "newviso.host",
            format!("released event sinks before provider unload count={released_event_sinks}"),
        );

        self.scene.shutdown_renderer();
        if let Some(mut renderer) = self.renderer.take() {
            bugtrap::checkpoint("render.provider.shutdown");
            renderer.shutdown();

            // render.api is implemented by the renderer DLL. Release the ABI
            // object while that DLL is still resident.
            host::unregister_service("render.api");
            drop(renderer);
        }
        self.ready = false;
    }
}
