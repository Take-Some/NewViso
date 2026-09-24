use super::*;

impl PlatformApplication for EngineApplication {
    fn on_window_ready(&mut self, _ready: PlatformWindowReadyV1) -> Result<(), String> {
        bugtrap::checkpoint("render.provider.load");
        bugtrap::set_context(
            "renderer_provider_path",
            self.renderer_path.display().to_string(),
        );
        let mut renderer = RunningProvider::load_current(&self.renderer_path)?;
        bugtrap::checkpoint("render.provider.initialize");
        renderer.initialize()?;

        host::info(
            "newviso.runtime",
            format!("renderer '{}' initialized on live window", renderer.id()),
        );

        self.renderer = Some(renderer);

        bugtrap::checkpoint("scene.renderer.initialize");
        self.scene.initialize_renderer()?;

        host::publish_event_json(
            event_topic::RUNTIME_STARTED,
            "newviso.runtime",
            json!({
                "scene": self.scene.title()
            }),
        )?;

        if self.scripts.is_some() {
            bugtrap::set_phase("scripts.start");
            let control = {
                let scripts = self
                    .scripts
                    .as_mut()
                    .ok_or_else(|| "script runtime disappeared during start".to_owned())?;
                scripts.start(&self.project_context)?
            };
            self.exit_requested |= control.exit_requested;
            self.ui_bindings.extend(control.ui_bindings);
            self.apply_script_commands(&control.commands)?;
        }

        self.publish_bound_ui_if_changed()?;
        self.publish_content_manager_if_changed()?;

        self.ready = true;

        host::info(
            "newviso.runtime",
            format!("3D scene '{}' initialized", self.scene.title()),
        );
        Ok(())
    }

    fn step(&mut self, dt: f32, surface: PlatformSurfaceMetricsV1) -> Result<bool, String> {
        if !self.ready {
            return Ok(false);
        }

        self.elapsed_seconds += f64::from(dt);

        bugtrap::set_phase("frame.input");
        let input = InputSnapshot::sample()?;
        host::publish_event_json(
            event_topic::RUNTIME_FRAME_BEGIN,
            "newviso.runtime",
            json!({
                "delta_seconds": dt,
                "elapsed_seconds": self.elapsed_seconds
            }),
        )?;
        self.publish_input_events(&input)?;

        let surface_size = [surface.width.max(1), surface.height.max(1)];
        let has_ui = self.ui_template.is_some() || self.content_manager.is_some();

        let mut ui_frame = None;
        if has_ui {
            let ui = UiClient::new();
            let dispatch = ui.dispatch_input(
                self.ui_frame_index,
                &input.ui_input_frame(),
                surface_size,
                surface.pixels_per_point,
            )?;
            self.apply_content_dispatch(&dispatch)?;

            ui_frame = Some(ui.frame(
                self.ui_frame_index,
                dt,
                surface_size,
                surface.pixels_per_point,
            )?);
        }

        let camera_navigation_enabled = ui_frame
            .as_ref()
            .and_then(|frame| frame.input_capture.get("camera_navigation_gated"))
            .and_then(Value::as_bool)
            != Some(true);

        if self.physics.is_some() {
            bugtrap::set_phase("physics.step");
            let scene_solids = self.scene.solid_aabbs();
            self.physics
                .as_mut()
                .expect("physics checked")
                .step(dt, &scene_solids)?;
        }

        if self.scripts.is_some() {
            let mut runtime_state = self.scene.runtime_state();
            if let Some(physics) = self.physics.as_ref() {
                runtime_state
                    .as_object_mut()
                    .expect("scene runtime state must be a JSON object")
                    .insert("physics".to_owned(), physics.runtime_state());
            }
            let frame_context = json!({
                "input": {
                    "state": &input.state,
                    "text": &input.text,
                    "ime_commit": &input.ime_commit
                },
                "surface": {
                    "width": surface.width.max(1),
                    "height": surface.height.max(1),
                    "pixels_per_point": surface.pixels_per_point
                },
                "platform": {
                    "focused": self.window_focused,
                    "cursor_captured": self.cursor_captured
                },
                "camera_navigation_enabled": camera_navigation_enabled
            });

            bugtrap::set_phase("scripts.frame");
            let control = {
                let scripts = self
                    .scripts
                    .as_mut()
                    .ok_or_else(|| "script runtime disappeared".to_owned())?;
                scripts.frame(
                    dt,
                    self.elapsed_seconds,
                    &self.project_context,
                    &runtime_state,
                    &frame_context,
                )?
            };
            self.exit_requested |= control.exit_requested;
            self.ui_bindings.extend(control.ui_bindings);
            self.apply_script_commands(&control.commands)?;
            bugtrap::set_phase("scene.tick");
            self.scene.tick(dt)?;
        } else {
            // Native orbit is an engine/editor navigation fallback, not gameplay.
            self.scene
                .update_native_input_from_snapshot(&input, dt, camera_navigation_enabled)?;
        }

        // The scene contributes only generic asset interest. Residency, dependency
        // closure, retry and eviction remain owned by newviso-resource-runtime.
        self.sync_scene_streaming_interests()?;
        self.pump_asset_streaming()?;

        self.publish_bound_ui_if_changed()?;
        self.publish_content_manager_if_changed()?;

        host::publish_event_json(
            event_topic::RUNTIME_FRAME_END,
            "newviso.runtime",
            json!({
                "delta_seconds": dt,
                "elapsed_seconds": self.elapsed_seconds,
                "exit_requested": self.exit_requested
            }),
        )?;

        if self.exit_requested {
            return Ok(true);
        }

        if let Some(ui) = ui_frame {
            if self.ui_frame_index == 0 {
                let vertices = ui
                    .draw_list
                    .pointer("/mesh/vertices")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                let indices = ui
                    .draw_list
                    .pointer("/mesh/indices")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                let cmds = ui
                    .draw_list
                    .pointer("/mesh/cmds")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                let textures = ui
                    .draw_list
                    .pointer("/texture_delta/set")
                    .and_then(Value::as_object)
                    .map_or(0, serde_json::Map::len);

                host::info(
                    "newviso.ui",
                    format!(
                        "first UI draw list screen={} ppp={} vertices={vertices} indices={indices} cmds={cmds} textures={textures} first_vertex={} first_cmd={}",
                        ui.draw_list
                            .get("screen_size_px")
                            .cloned()
                            .unwrap_or(Value::Null),
                        ui.draw_list
                            .get("pixels_per_point")
                            .cloned()
                            .unwrap_or(Value::Null),
                        ui.draw_list
                            .pointer("/mesh/vertices/0")
                            .cloned()
                            .unwrap_or(Value::Null),
                        ui.draw_list
                            .pointer("/mesh/cmds/0")
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                );
                if let Some(set) = ui
                    .draw_list
                    .pointer("/texture_delta/set")
                    .and_then(Value::as_object)
                {
                    host::info(
                        "newviso.ui",
                        format!("first UI texture ids={:?}", set.keys().collect::<Vec<_>>()),
                    );

                    if let Some(texture) = set.get("1") {
                        let alpha = texture
                            .get("rgba8")
                            .and_then(Value::as_array)
                            .map(|bytes| {
                                bytes
                                    .iter()
                                    .skip(3)
                                    .step_by(4)
                                    .filter_map(Value::as_u64)
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        let alpha_min = alpha.iter().copied().min().unwrap_or(0);
                        let alpha_max = alpha.iter().copied().max().unwrap_or(0);
                        host::info(
                            "newviso.ui",
                            format!(
                                "first UI atlas size={} alpha_range={}..{}",
                                texture.get("size").cloned().unwrap_or(Value::Null),
                                alpha_min,
                                alpha_max
                            ),
                        );
                    }
                }
            }
            self.ui_frame_index = self.ui_frame_index.wrapping_add(1);
            bugtrap::native_boundary("render.scene");
            self.scene
                .render_frame_with_overlay(surface.width, surface.height, move || {
                    RenderClient::new().set_ui_draw_list(ui.draw_list)
                })?;
        } else {
            bugtrap::native_boundary("render.scene");
            self.scene.render_frame(surface.width, surface.height)?;
        }

        if let Some(renderer) = self.renderer.as_mut() {
            bugtrap::native_boundary("render.provider.update");
            renderer.update(dt)?;
            bugtrap::native_boundary("render.provider.render");
            renderer.render(dt)?;
        }
        bugtrap::set_phase("frame.idle");

        Ok(false)
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
            }
        }

        host::info(
            "newviso.scene",
            format!("final runtime state: {}", self.scene.runtime_state()),
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
