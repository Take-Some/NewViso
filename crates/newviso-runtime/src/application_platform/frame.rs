//! Frame orchestration. Platform lifecycle and window callbacks remain in
//! application_platform.rs; per-frame simulation/render sequencing lives here.
use super::*;

fn all_frame_perf_logging_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NEWVISO_PERF_LOG_EVERY_FRAME").as_deref() == Ok("1"))
}

impl EngineApplication {
    pub(super) fn step_frame(&mut self, dt: f32, surface: PlatformSurfaceMetricsV1) -> Result<bool, String> {
        if !self.ready {
            return Ok(false);
        }

        if !dt.is_finite() || dt < 0.0 {
            return Err("frame delta must be finite and non-negative".into());
        }

        if !self.startup_map_ready {
            self.pump_startup_map_loading(surface)?;
            return Ok(false);
        }

        self.world_save_allowed = false;
        self.elapsed_seconds += f64::from(dt);
        self.advance_climate_clock(dt)?;
        self.update_climate(dt)?;
        let perf_frame = self.asset_streamer.stats().frame;
        let perf_start = std::time::Instant::now();
        let mut perf_mark = perf_start;
        let perf_physics_ms;
        let perf_scripts_ms;
        let perf_streaming_ms;
        let perf_scene_render_ms;

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
            let (surface_wetness, surface_snow) = self.scene.surface_weather_state();
            self.vehicles
                .set_surface_weather(surface_wetness, surface_snow)?;
            let physics_dt = dt.min(self.settings.scheduling.max_physics_frame_seconds);
            let scene_solids = self
                .physics
                .as_ref()
                .and_then(PhysicsRuntime::scene_collider_interests)
                .map(|interests| self.scene.physics_static_solid_colliders_near(&interests))
                .unwrap_or_default();
            let (damage_contacts, activity_updates, pose_updates) = {
                let vehicles = &mut self.vehicles;
                let physics = self.physics.as_mut().expect("physics checked");
                physics.step_with_water(physics_dt, &scene_solids, vehicles, Some(self.scene.water_simulation_mut()))?;
                (
                    physics.damage_contacts(),
                    physics.scene_activity_updates(),
                    physics.scene_pose_updates(),
                )
            };

            // Static scene props remain cheap until a real collision or a
            // damage-carrying projectile reaches them. The scene owns health
            // and break policy; physics owns the dynamic body created at the
            // exact transition.
            self.apply_ped_wheel_runover()?;
            for contact in damage_contacts {
                if self.apply_ped_physics_damage(contact)? {
                    continue;
                }
                if self.vehicles.contains(contact.target) {
                    let _ = self.apply_vehicle_contact_damage(contact)?;
                    continue;
                }
                if let Some(activation) = self.scene.apply_entity_damage(
                    contact.target,
                    contact.direct_damage,
                    contact.contact_impulse,
                )? {
                    host::info(
                        "newviso.scene",
                        format!(
                            "destructible activated entity={} source={} damage={:.2} impulse={:.2}",
                            activation.entity,
                            contact.source,
                            contact.direct_damage,
                            contact.contact_impulse
                        ),
                    );
                    let physics = self.physics.as_mut().expect("physics checked");
                    physics.promote_scene_destructible(activation, contact)?;
                    self.scene
                        .set_physics_process_active(activation.entity, true)?;
                }
            }

            for activity in activity_updates {
                self.scene
                    .set_physics_process_active(activity.entity, activity.active)?;
            }
            for pose in pose_updates {
                // World-actor presentation samples ped bodies after character navigation.
                if self.is_ped_physics_entity(pose.entity) {
                    continue;
                }
                self.scene
                    .apply_physics_pose(pose.entity, pose.position, pose.rotation)?;
            }
            for (pickup, entity) in self.items.pickup_bodies() {
                if let Some(pose) = self
                    .physics
                    .as_ref()
                    .and_then(|physics| physics.pickup_body_pose(entity))
                {
                    self.items.sync_pickup_body(&pickup, pose)?;
                }
            }
        }

        self.tick_cold(dt)?;
        perf_physics_ms = perf_mark.elapsed().as_secs_f64() * 1000.0;
        perf_mark = std::time::Instant::now();
        let scripts_detail_start = std::time::Instant::now();
        let mut scripts_detail_mark = scripts_detail_start;
        let perf_world_native_ms;
        let perf_presentations_ms;
        let perf_script_state_ms;
        let perf_quickjs_ms;
        let perf_script_commands_ms;
        let perf_scene_tick_ms;
        let mut final_camera_commands = Vec::new();

        bugtrap::set_phase("living_world.tick");
        let transient_observers = [self.scene.focus_position()];
        let observers = if self.settings.scheduling.scene_focus_observer {
            &transient_observers[..]
        } else {
            &[]
        };
        self.living_world.tick_frame(dt, observers);
        bugtrap::set_phase("agents.tick");
        self.tick_agents(dt)?;
        bugtrap::set_phase("characters.tick");
        self.tick_physical_characters(dt)?;
        perf_world_native_ms = scripts_detail_mark.elapsed().as_secs_f64() * 1000.0;
        scripts_detail_mark = std::time::Instant::now();

        self.sync_world_actor_presentations()?;
        perf_presentations_ms = scripts_detail_mark.elapsed().as_secs_f64() * 1000.0;
        scripts_detail_mark = std::time::Instant::now();

        if self.scripts.is_some() {
            let runtime_state = self.script_frame_state();
            perf_script_state_ms = scripts_detail_mark.elapsed().as_secs_f64() * 1000.0;
            scripts_detail_mark = std::time::Instant::now();
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
                    runtime_state,
                    &frame_context,
                )?
            };
            perf_quickjs_ms = scripts_detail_mark.elapsed().as_secs_f64() * 1000.0;
            scripts_detail_mark = std::time::Instant::now();

            self.exit_requested |= control.exit_requested;
            self.ui_bindings.extend(control.ui_bindings);
            // Joint cameras need the final animated/attached actor pose. Keep
            // the last camera request and any later explicit audio listener in
            // their original order, so ordinary camera/listener overrides work.
            for command in &control.commands {
                match command.get("op").and_then(Value::as_str) {
                    Some("scene.camera.set_relative_to_entity")
                        if command.get("late_update").and_then(Value::as_bool) == Some(true) =>
                    {
                        final_camera_commands.clear();
                        final_camera_commands.push(command.clone());
                    }
                    Some("scene.camera.set" | "scene.camera.set_relative_to_entity") => {
                        final_camera_commands.clear();
                    }
                    Some("audio.listener.set") if !final_camera_commands.is_empty() => {
                        final_camera_commands.truncate(1);
                        final_camera_commands.push(command.clone());
                    }
                    _ => {}
                }
            }
            self.apply_script_commands(&control.commands)?;
            perf_script_commands_ms = scripts_detail_mark.elapsed().as_secs_f64() * 1000.0;
            scripts_detail_mark = std::time::Instant::now();

            bugtrap::set_phase("scene.tick");
            self.update_climate(0.0)?;
            self.scene.tick(dt)?;
            perf_scene_tick_ms = scripts_detail_mark.elapsed().as_secs_f64() * 1000.0;
        } else {
            // Native orbit is an engine/editor navigation fallback, not gameplay.
            self.scene.update_native_input_from_snapshot(
                &input,
                dt,
                camera_navigation_enabled && self.settings.scheduling.native_navigation_enabled,
            )?;
            perf_script_state_ms = 0.0;
            perf_quickjs_ms = 0.0;
            perf_script_commands_ms = 0.0;
            perf_scene_tick_ms = 0.0;
        }

        // Contact audio uses the just-updated skeleton phase, after scripts
        // and scene tick but before frame submission to the renderer.
        self.flush_character_motion_audio()?;

        bugtrap::set_phase("vehicles.presentation");
        self.sync_vehicle_presentations()?;
        self.sync_physical_particle_bodies()?;

        for mutation in self.scene.drain_entity_mutations() {
            host::publish_event_json(event_topic::SCENE_ENTITY_MUTATED, "newviso.scene", mutation)?;
        }

        // The scene contributes only generic asset interest. Residency, dependency
        // closure, retry and eviction remain owned by newviso-resource-runtime.
        perf_scripts_ms = perf_mark.elapsed().as_secs_f64() * 1000.0;
        perf_mark = std::time::Instant::now();
        let streaming_interest_started = std::time::Instant::now();
        self.sync_scene_streaming_interests()?;
        let streaming_interest_ms = streaming_interest_started.elapsed().as_secs_f64() * 1000.0;
        let streaming_pump_started = std::time::Instant::now();
        self.pump_asset_streaming()?;
        let streaming_pump_ms = streaming_pump_started.elapsed().as_secs_f64() * 1000.0;
        // Animation sampling, vehicle occupant transforms and newly streamed
        // skeletons are now final for the rendered frame. Resolve the eye here
        // to prevent speed-dependent lag behind the driver's seat.
        if !final_camera_commands.is_empty() {
            self.apply_script_commands(&final_camera_commands)?;
        }
        // Probe resident collision triangles after the final eye and streamed
        // geometry are ready. Render bounds of terrain/props are not rain roofs.
        let weather_eye = self.scene.camera_pose_values().0;
        let weather_shelter = self.physics.as_ref()
            .is_some_and(|physics| physics.weather_camera_is_sheltered(weather_eye));
        self.scene.set_weather_physical_shelter(weather_shelter);
        let rain_physics = self.physics.as_ref();
        self.scene.refresh_weather_collision_map(|x, z, top| {
            rain_physics.and_then(|physics| physics.weather_surface_height(x, z, top))
        });
        perf_streaming_ms = perf_mark.elapsed().as_secs_f64() * 1000.0;
        if perf_streaming_ms >= 4.0 {
            host::debug(
                "newviso.perf",
                format!(
                    "streaming_breakdown frame={} interests_ms={:.3} pump_ms={:.3} total_ms={:.3}",
                    self.ui_frame_index,
                    streaming_interest_ms,
                    streaming_pump_ms,
                    perf_streaming_ms
                ),
            );
        }
        let publish_start=std::time::Instant::now();
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

        self.world_save_allowed = true;
        self.autosave_world(dt);
        if self.exit_requested {
            return Ok(true);
        }

        let perf_publish_ms=publish_start.elapsed().as_secs_f64()*1000.0;
        let scene_render_start=std::time::Instant::now();
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

        perf_scene_render_ms = scene_render_start.elapsed().as_secs_f64() * 1000.0;
        perf_mark = std::time::Instant::now();
        if let Some(renderer) = self.renderer.as_mut() {
            bugtrap::native_boundary("render.provider.update");
            renderer.update(dt)?;
            bugtrap::native_boundary("render.provider.render");
            renderer.render(dt)?;
        }
        let perf_provider_ms = perf_mark.elapsed().as_secs_f64() * 1000.0;
        let perf_total_ms = perf_start.elapsed().as_secs_f64() * 1000.0;
        if perf_total_ms >= 25.0 || perf_frame % 60 == 0 || all_frame_perf_logging_enabled() {
            let message=format!(
                    "frame={} total_ms={:.2} physics_ms={:.2} scripts_world_ms={:.2} world_native_ms={:.2} presentations_ms={:.2} script_state_ms={:.2} quickjs_ms={:.2} script_commands_ms={:.2} scene_tick_ms={:.2} streaming_ms={:.2} publish_ms={:.2} scene_render_ms={:.2} provider_ms={:.2}",
                    perf_frame,
                    perf_total_ms,
                    perf_physics_ms,
                    perf_scripts_ms,
                    perf_world_native_ms,
                    perf_presentations_ms,
                    perf_script_state_ms,
                    perf_quickjs_ms,
                    perf_script_commands_ms,
                    perf_scene_tick_ms,
                    perf_streaming_ms,
                    perf_publish_ms,
                    perf_scene_render_ms,
                    perf_provider_ms
                );
            if all_frame_perf_logging_enabled() {
                host::info("newviso.perf",message);
            } else {
                host::trace("newviso.perf",message);
            }
        }
        bugtrap::set_phase("frame.idle");

        Ok(false)
    }

}
