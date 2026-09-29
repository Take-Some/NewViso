use super::*;

impl EngineApplication {
    fn resolve_script_scene_entity(
        &self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<u64, String> {
        if let Some(entity) = command.get("entity").and_then(Value::as_u64) {
            return Ok(entity);
        }
        let id = command
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                format!(
                    "script command[{index}] {op} requires unsigned integer 'entity' or string 'id'"
                )
            })?;
        self.scene
            .runtime_entity_stable_id(id)
            .ok_or_else(|| format!("script command[{index}] {op} target '{id}' does not exist"))
    }

    fn resolve_script_scene_entity_command(
        &self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(Value, u64), String> {
        let entity = self.resolve_script_scene_entity(command, index, op)?;
        let mut resolved = command.clone();
        resolved["entity"] = json!(entity);
        Ok((resolved, entity))
    }

    pub(super) fn apply_script_commands(&mut self, commands: &[Value]) -> Result<(), String> {
        for (index, command) in commands.iter().enumerate() {
            let op = command
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("script command[{index}] has no string 'op'"))?;

            match op {
                "scene.render.configure" => {
                    let patch = command.get("settings").ok_or_else(|| {
                        format!("script command[{index}] scene.render.configure requires settings")
                    })?;
                    self.scene.configure_render_policy(patch)?;
                }
                "runtime.configure" => {
                    let patch = command.get("settings").ok_or_else(|| {
                        format!("script command[{index}] runtime.configure requires settings")
                    })?;
                    self.configure_runtime(patch)?;
                }
                "console.write" => {
                    let message =
                        command
                            .get("message")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                format!(
                                "script command[{index}] console.write requires string 'message'"
                            )
                            })?;
                    let level = command
                        .get("level")
                        .and_then(Value::as_str)
                        .unwrap_or("info")
                        .trim()
                        .to_ascii_lowercase();
                    let raw_target = command
                        .get("target")
                        .and_then(Value::as_str)
                        .unwrap_or("script")
                        .trim()
                        .to_ascii_lowercase();
                    let clean_target = raw_target
                        .chars()
                        .map(|ch| {
                            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':' | '-') {
                                ch
                            } else {
                                '_'
                            }
                        })
                        .take(96)
                        .collect::<String>();
                    let target = if clean_target.is_empty() || clean_target == "script" {
                        "script".to_owned()
                    } else {
                        format!("script.{clean_target}")
                    };
                    let bounded_message = message.chars().take(16_384).collect::<String>();
                    match level.as_str() {
                        "trace" => host::trace(target, bounded_message),
                        "debug" => host::debug(target, bounded_message),
                        "info" => host::info(target, bounded_message),
                        "warn" | "warning" => host::warn(target, bounded_message),
                        "error" => host::error(target, bounded_message),
                        other => {
                            return Err(format!(
                                "script command[{index}] console.write has invalid level '{other}'"
                            ))
                        }
                    }
                }
                "audio.cue.preload" => {
                    let cue = command
                        .get("cue")
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] audio.cue.preload requires string 'cue'"
                            )
                        })?;
                    let request = json!({
                        "cue": { "logical_path": cue }
                    });
                    invoke_audio_service(index, "preload_cue_json_v1", &request)?;
                }
                "audio.cue.play" => {
                    let cue = command
                        .get("cue")
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| {
                            format!("script command[{index}] audio.cue.play requires string 'cue'")
                        })?;
                    let gain = command
                        .get("gain")
                        .map(|_| command_number(command, "gain", index))
                        .transpose()?
                        .unwrap_or(1.0)
                        .clamp(0.0, 4.0);
                    let pitch = command
                        .get("pitch")
                        .map(|_| command_number(command, "pitch", index))
                        .transpose()?
                        .unwrap_or(1.0)
                        .clamp(0.05, 4.0);
                    let mut request = json!({
                        "version": 1,
                        "cue": { "logical_path": cue },
                        "gain": gain,
                        "pitch": pitch
                    });
                    if command.get("position").is_some() {
                        request["position"] = json!(command_vec3(command, "position", index)?);
                    }
                    if let Some(route) = command.get("route").and_then(Value::as_str) {
                        request["route"] = Value::String(route.to_owned());
                    }
                    if let Some(seed) = command.get("seed").and_then(Value::as_u64) {
                        request["seed"] = json!(seed);
                    }
                    if let Some(scope_id) = command.get("scope_id").and_then(Value::as_str) {
                        request["scope_id"] = Value::String(scope_id.to_owned());
                    }
                    invoke_audio_service(index, "play_cue_json_v1", &request)?;
                }
                "audio.listener.set" => {
                    let request = json!({
                        "position": command_vec3(command, "position", index)?,
                        "forward": command
                            .get("forward")
                            .map(|_| command_vec3(command, "forward", index))
                            .transpose()?
                            .unwrap_or([0.0, 0.0, -1.0]),
                        "up": command
                            .get("up")
                            .map(|_| command_vec3(command, "up", index))
                            .transpose()?
                            .unwrap_or([0.0, 1.0, 0.0]),
                        "velocity": command
                            .get("velocity")
                            .map(|_| command_vec3(command, "velocity", index))
                            .transpose()?
                            .unwrap_or([0.0, 0.0, 0.0])
                    });
                    invoke_audio_service(index, "set_listener_json_v1", &request)?;
                }
                "events.emit" => {
                    let topic = command
                        .get("topic")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!("script command[{index}] events.emit requires string 'topic'")
                        })?;
                    let source = command
                        .get("source")
                        .and_then(Value::as_str)
                        .unwrap_or("project.script");
                    let payload = command.get("payload").cloned().unwrap_or(Value::Null);
                    let cancelable = command
                        .get("cancelable")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let phase = match command
                        .get("phase")
                        .and_then(Value::as_str)
                        .unwrap_or("observe")
                    {
                        "before" => EventPhase::Before,
                        "after" => EventPhase::After,
                        "observe" => EventPhase::Observe,
                        other => {
                            return Err(format!(
                                "script command[{index}] events.emit has invalid phase '{other}'"
                            ))
                        }
                    };

                    let mut metadata = command
                        .get("metadata")
                        .and_then(Value::as_object)
                        .map(|values| {
                            values
                                .iter()
                                .map(|(key, value)| (key.clone(), value.clone()))
                                .collect::<BTreeMap<_, _>>()
                        })
                        .unwrap_or_default();
                    metadata
                        .entry("script_echo".to_owned())
                        .or_insert(Value::Bool(false));

                    host::publish_event_json_with(
                        topic, source, payload, phase, cancelable, metadata,
                    )?;
                }
                "platform.cursor.set" => {
                    let captured = command
                        .get("captured")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] platform.cursor.set requires 'captured'"
                            )
                        })?;
                    let previous = self.cursor_captured;
                    self.cursor_captured = captured && self.window_focused;
                    if previous != self.cursor_captured {
                        host::publish_event_json(
                            event_topic::CURSOR_CAPTURE_CHANGED,
                            "newviso.runtime",
                            json!({
                                "captured": self.cursor_captured,
                                "previous": previous
                            }),
                        )?;
                    }
                }
                "physics.world.configure" => {
                    self.physics
                        .as_mut()
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] requires engine.physics but no physics capability is active"
                            )
                        })?
                        .configure_from_script(command, index)?;
                }
                "physics.body.upsert" => {
                    let (resolved, entity) = self.resolve_script_scene_entity_command(
                        command,
                        index,
                        "physics.body.upsert",
                    )?;
                    self.physics
                        .as_mut()
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] requires engine.physics but no physics capability is active"
                            )
                        })?
                        .upsert_body_from_script(&resolved, index)?;
                    self.scene.set_physics_process_active(entity, true)?;
                }
                "physics.body.destroy" => {
                    let (resolved, entity) = self.resolve_script_scene_entity_command(
                        command,
                        index,
                        "physics.body.destroy",
                    )?;
                    self.physics
                        .as_mut()
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] requires engine.physics but no physics capability is active"
                            )
                        })?
                        .destroy_body_from_script(&resolved, index)?;
                    self.scene.set_physics_process_active(entity, false)?;
                }
                "physics.body.velocity.set" => {
                    let (resolved, _) = self.resolve_script_scene_entity_command(
                        command,
                        index,
                        "physics.body.velocity.set",
                    )?;
                    self.physics
                        .as_mut()
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] requires engine.physics but no physics capability is active"
                            )
                        })?
                        .set_body_velocity_from_script(&resolved, index)?;
                }
                "physics.body.pose.set" => {
                    let (resolved, _) = self.resolve_script_scene_entity_command(
                        command,
                        index,
                        "physics.body.pose.set",
                    )?;
                    self.physics
                        .as_mut()
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] requires engine.physics but no physics capability is active"
                            )
                        })?
                        .set_body_pose_from_script(&resolved, index)?;
                }
                "physics.body.impulse" => {
                    let (resolved, _) = self.resolve_script_scene_entity_command(
                        command,
                        index,
                        "physics.body.impulse",
                    )?;
                    self.physics
                        .as_mut()
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] requires engine.physics but no physics capability is active"
                            )
                        })?
                        .apply_impulse_from_script(&resolved, index)?;
                }
                "vehicle.surface_policy.set" => {
                    self.set_vehicle_surface_policy_from_script(command, index)?;
                }
                "vehicle.upsert" => {
                    let (resolved, _) =
                        self.resolve_script_scene_entity_command(command, index, "vehicle.upsert")?;
                    self.upsert_vehicle_from_script(&resolved, index)?;
                }
                "vehicle.remove" => {
                    let (resolved, _) =
                        self.resolve_script_scene_entity_command(command, index, "vehicle.remove")?;
                    self.remove_vehicle_from_script(&resolved, index)?;
                }
                "vehicle.input.set" => {
                    let (resolved, _) = self.resolve_script_scene_entity_command(
                        command,
                        index,
                        "vehicle.input.set",
                    )?;
                    self.set_vehicle_input_from_script(&resolved, index)?;
                }
                "vehicle.enabled.set" => {
                    let (resolved, _) = self.resolve_script_scene_entity_command(
                        command,
                        index,
                        "vehicle.enabled.set",
                    )?;
                    self.set_vehicle_enabled_from_script(&resolved, index)?;
                }
                "vehicle.part.set" => {
                    let (resolved, _) = self.resolve_script_scene_entity_command(
                        command,
                        index,
                        "vehicle.part.set",
                    )?;
                    self.set_vehicle_part_from_script(&resolved, index)?;
                }
                "vehicle.lights.set" => {
                    let (resolved, _) = self.resolve_script_scene_entity_command(
                        command,
                        index,
                        "vehicle.lights.set",
                    )?;
                    self.set_vehicle_lights_from_script(&resolved, index)?;
                }
                "vehicle.audio_fx.configure" => {
                    let (resolved, _) = self.resolve_script_scene_entity_command(
                        command,
                        index,
                        "vehicle.audio_fx.configure",
                    )?;
                    self.set_vehicle_audio_fx_from_script(&resolved, index)?;
                }
                "vehicle.occupant.set" => {
                    let (resolved, _) = self.resolve_script_scene_entity_command(
                        command,
                        index,
                        "vehicle.occupant.set",
                    )?;
                    self.set_vehicle_occupant_from_script(&resolved, index)?;
                }
                "scene.clear_color.set" => {
                    self.scene
                        .set_clear_color(command_vec4(command, "color", index)?);
                }
                "scene.environment.set" => {
                    let mut desc = self.scene.scene_environment();
                    if command.get("ambient_color").is_some() {
                        desc.ambient_color = command_vec3(command, "ambient_color", index)?;
                    }
                    if command.get("ambient_intensity").is_some() {
                        desc.ambient_intensity =
                            command_number(command, "ambient_intensity", index)?;
                    }

                    if let Some(fog) = command.get("fog") {
                        if !fog.is_object() {
                            return Err(format!(
                                "script command[{index}] scene.environment.set 'fog' must be an object"
                            ));
                        }
                        if let Some(value) = fog.get("enabled") {
                            desc.fog_enabled = value.as_bool().ok_or_else(|| {
                                format!(
                                    "script command[{index}] scene.environment.set fog.enabled must be boolean"
                                )
                            })?;
                        }
                        if fog.get("color").is_some() {
                            desc.fog_color = command_vec3(fog, "color", index)?;
                        }
                        if fog.get("density").is_some() {
                            desc.fog_density = command_number(fog, "density", index)?;
                        }
                        if fog.get("start_distance").is_some() {
                            desc.fog_start_distance = command_number(fog, "start_distance", index)?;
                        }
                        if fog.get("height_falloff").is_some() {
                            desc.fog_height_falloff = command_number(fog, "height_falloff", index)?;
                        }
                        if fog.get("base_height").is_some() {
                            desc.fog_base_height = command_number(fog, "base_height", index)?;
                        }
                        if fog.get("max_opacity").is_some() {
                            desc.fog_max_opacity = command_number(fog, "max_opacity", index)?;
                        }
                    }

                    if let Some(haze) = command.get("haze") {
                        if !haze.is_object() {
                            return Err(format!(
                                "script command[{index}] scene.environment.set 'haze' must be an object"
                            ));
                        }
                        if haze.get("color").is_some() {
                            desc.haze_color = command_vec3(haze, "color", index)?;
                        }
                        if haze.get("density").is_some() {
                            desc.haze_density = command_number(haze, "density", index)?;
                        }
                        if haze.get("start_distance").is_some() {
                            desc.haze_start_distance =
                                command_number(haze, "start_distance", index)?;
                        }
                    }

                    self.scene.set_scene_environment(desc)?;
                }
                "scene.orbit.configure" => {
                    // Preserve the legacy command while keeping the live settings
                    // snapshot synchronized with the actual camera policy.
                    self.configure_runtime(&json!({"camera": {
                        "rotate_sensitivity": command_number(command, "rotate_sensitivity", index)?,
                        "zoom_sensitivity": command_number(command, "zoom_sensitivity", index)?,
                        "min_distance": command_number(command, "min_distance", index)?,
                        "max_distance": command_number(command, "max_distance", index)?
                    }}))?;
                }
                "scene.sky.time.set" => {
                    self.scene
                        .set_sky_time_seconds(command_number(command, "seconds", index)?)?;
                }
                "scene.sky.time_scale.set" => {
                    self.scene
                        .set_sky_time_scale(command_number(command, "scale", index)?)?;
                }
                "scene.timecycle.state.set" => {
                    let cycle_seconds = command_number(command, "cycle_seconds", index)?;
                    let phase = command_number(command, "phase", index)?;
                    let rate = command_number(command, "rate", index)?;
                    let duration_seconds = command_number(command, "duration_seconds", index)?;
                    self.scene.set_timecycle_backend(
                        cycle_seconds,
                        phase,
                        rate,
                        duration_seconds,
                    )?;
                }
                "scene.weather.state.set" => {
                    let current = command
                        .get("current")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.weather.state.set requires string 'current'"
                            )
                        })?;
                    let next = command
                        .get("next")
                        .and_then(Value::as_str)
                        .unwrap_or(current);
                    let blend = command_number(command, "blend", index)?;

                    let resolved_effects = application_weather::resolve_shared_weather_effects(
                        current,
                        next,
                        blend,
                        self.scene.weather_effects(),
                    )?
                    .unwrap_or_else(|| self.scene.weather_effects().clone());
                    let mut effects =
                        parse_weather_effects(command.get("effects"), &resolved_effects, index)?;
                    if let Some(value) =
                        command.get("current_cloud_variant").and_then(Value::as_str)
                    {
                        effects.current_cloud_variant = value.to_owned();
                    }
                    if let Some(value) = command.get("next_cloud_variant").and_then(Value::as_str) {
                        effects.next_cloud_variant = value.to_owned();
                    }

                    self.scene.set_weather_backend(current, next, blend)?;
                    self.scene.set_weather_effects(effects)?;
                }
                "character.world_actor.bind" => {
                    self.bind_physical_character_from_script(command, index)?;
                }
                "character.world_actor.unbind" => {
                    let actor_id =
                        command
                            .get("actor_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                format!(
                                    "script command[{index}] character.world_actor.unbind requires string 'actor_id'"
                                )
                            })?;
                    self.physical_characters.unbind(actor_id);
                    self.living_world.release_actor_external_motion(actor_id);
                }
                "navigation.configure" => {
                    self.configure_navigation_from_script(command, index)?;
                }
                "navigation.obstacle.upsert" => {
                    self.upsert_navigation_obstacle_from_script(command, index)?;
                }
                "navigation.obstacle.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] navigation.obstacle.remove requires string 'id'"
                        )
                    })?;
                    self.physical_characters.remove_obstacle(id);
                }
                "navigation.off_mesh_link.upsert" => {
                    self.upsert_off_mesh_link_from_script(command, index)?;
                }
                "navigation.off_mesh_link.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] navigation.off_mesh_link.remove requires string 'id'"
                        )
                    })?;
                    self.physical_characters.remove_off_mesh_link(id);
                }
                "agent.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] agent.upsert requires string 'id'")
                    })?;
                    let actor_id =
                        command
                            .get("actor_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                format!(
                                "script command[{index}] agent.upsert requires string 'actor_id'"
                            )
                            })?;
                    let enabled = command
                        .get("enabled")
                        .map(|value| {
                            value.as_bool().ok_or_else(|| {
                                format!(
                                    "script command[{index}] agent.upsert 'enabled' must be boolean"
                                )
                            })
                        })
                        .transpose()?
                        .unwrap_or(true);
                    let perception = command
                        .get("perception")
                        .filter(|value| !value.is_null())
                        .map(|value| {
                            serde_json::from_value::<AgentPerceptionPolicy>(value.clone()).map_err(
                                |error| {
                                    format!(
                                        "script command[{index}] agent.upsert invalid perception: {error}"
                                    )
                                },
                            )
                        })
                        .transpose()?
                        .unwrap_or_default();
                    let thinking = command
                        .get("thinking")
                        .filter(|value| !value.is_null())
                        .map(|value| {
                            serde_json::from_value::<AgentThinkingPolicy>(value.clone()).map_err(
                                |error| {
                                    format!(
                                        "script command[{index}] agent.upsert invalid thinking: {error}"
                                    )
                                },
                            )
                        })
                        .transpose()?
                        .unwrap_or_default();
                    let blackboard = command
                        .get("blackboard")
                        .map(|value| {
                            serde_json::from_value::<BTreeMap<String, Value>>(value.clone()).map_err(
                                |error| {
                                    format!(
                                        "script command[{index}] agent.upsert invalid blackboard: {error}"
                                    )
                                },
                            )
                        })
                        .transpose()?
                        .unwrap_or_default();
                    if let Some(cancel) = self.agents.upsert_agent(AgentDesc {
                        id: id.to_owned(),
                        actor_id: actor_id.to_owned(),
                        enabled,
                        perception,
                        thinking,
                        blackboard,
                    })? {
                        application_agents::apply_agent_commands(
                            &mut self.living_world,
                            vec![cancel],
                        )?;
                    }
                }
                "agent.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] agent.remove requires string 'id'")
                    })?;
                    if let Some(cancel) = self.agents.remove_agent(id) {
                        application_agents::apply_agent_commands(
                            &mut self.living_world,
                            vec![cancel],
                        )?;
                    }
                }
                "agent.task.set" => {
                    let agent_id =
                        command
                            .get("agent_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                format!(
                                "script command[{index}] agent.task.set requires string 'agent_id'"
                            )
                            })?;
                    let task = command.get("task").ok_or_else(|| {
                        format!("script command[{index}] agent.task.set requires object 'task'")
                    })?;
                    let task =
                        serde_json::from_value::<AgentTaskDesc>(task.clone()).map_err(|error| {
                            format!("script command[{index}] agent.task.set invalid task: {error}")
                        })?;
                    self.agents.set_task(agent_id, task)?;
                }
                "agent.task.clear" => {
                    let agent_id =
                        command
                            .get("agent_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                format!(
                                    "script command[{index}] agent.task.clear requires string 'agent_id'"
                                )
                            })?;
                    let task_id =
                        command
                            .get("task_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                format!(
                                    "script command[{index}] agent.task.clear requires string 'task_id'"
                                )
                            })?;
                    if let Some(cancel) = self.agents.clear_task(agent_id, task_id)? {
                        application_agents::apply_agent_commands(
                            &mut self.living_world,
                            vec![cancel],
                        )?;
                    }
                }
                "agent.blackboard.set" => {
                    let agent_id =
                        command
                            .get("agent_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                format!(
                                    "script command[{index}] agent.blackboard.set requires string 'agent_id'"
                                )
                            })?;
                    let key = command.get("key").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] agent.blackboard.set requires string 'key'"
                        )
                    })?;
                    self.agents.set_blackboard(
                        agent_id,
                        key,
                        command.get("value").cloned().unwrap_or(Value::Null),
                    )?;
                }
                "agent.blackboard.remove" => {
                    let agent_id =
                        command
                            .get("agent_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                format!(
                                    "script command[{index}] agent.blackboard.remove requires string 'agent_id'"
                                )
                            })?;
                    let key = command.get("key").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] agent.blackboard.remove requires string 'key'"
                        )
                    })?;
                    self.agents.remove_blackboard(agent_id, key)?;
                }
                "items.definition.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] items.definition.upsert requires string 'id'"
                        )
                    })?;
                    let display_name = command
                        .get("display_name")
                        .and_then(Value::as_str)
                        .unwrap_or(id)
                        .to_owned();
                    let category = command
                        .get("category")
                        .and_then(Value::as_str)
                        .unwrap_or("misc")
                        .to_owned();
                    let max_stack = command
                        .get("max_stack")
                        .map(|_| command_u32(command, "max_stack", index))
                        .transpose()?
                        .unwrap_or(1);
                    let tags = command
                        .get("tags")
                        .and_then(Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .map(|value| {
                                    value.as_str().map(str::to_owned).ok_or_else(|| {
                                        format!(
                                            "script command[{index}] items.definition.upsert tags must contain strings"
                                        )
                                    })
                                })
                                .collect::<Result<Vec<_>, String>>()
                        })
                        .transpose()?
                        .unwrap_or_default();
                    self.items.upsert_definition(ItemDefinition {
                        id: id.to_owned(),
                        display_name,
                        category,
                        max_stack,
                        tags,
                        metadata: command.get("metadata").cloned().unwrap_or(Value::Null),
                    })?;
                }
                "items.inventory.ensure" => {
                    let owner_id = command
                        .get("owner_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] items.inventory.ensure requires string 'owner_id'"
                            )
                        })?;
                    self.items.ensure_inventory(owner_id)?;
                }
                "items.inventory.add" => {
                    let owner_id = command
                        .get("owner_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] items.inventory.add requires string 'owner_id'"
                            )
                        })?;
                    let item_id = command
                        .get("item_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] items.inventory.add requires string 'item_id'"
                            )
                        })?;
                    let quantity = command_u32(command, "quantity", index)?;
                    let accepted = self.items.add_inventory(owner_id, item_id, quantity)?;
                    host::publish_event_json(
                        "gameplay.inventory.changed",
                        "newviso.items",
                        json!({
                            "operation": "add",
                            "owner_id": owner_id,
                            "item_id": item_id,
                            "requested_quantity": quantity,
                            "applied_quantity": accepted,
                            "quantity": self.items.inventory_quantity(owner_id, item_id)
                        }),
                    )?;
                }
                "items.inventory.remove" => {
                    let owner_id = command
                        .get("owner_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] items.inventory.remove requires string 'owner_id'"
                            )
                        })?;
                    let item_id = command
                        .get("item_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] items.inventory.remove requires string 'item_id'"
                            )
                        })?;
                    let quantity = command_u32(command, "quantity", index)?;
                    let removed = self.items.remove_inventory(owner_id, item_id, quantity)?;
                    host::publish_event_json(
                        "gameplay.inventory.changed",
                        "newviso.items",
                        json!({
                            "operation": "remove",
                            "owner_id": owner_id,
                            "item_id": item_id,
                            "requested_quantity": quantity,
                            "applied_quantity": removed,
                            "quantity": self.items.inventory_quantity(owner_id, item_id)
                        }),
                    )?;
                }
                "items.pickup.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] items.pickup.upsert requires string 'id'")
                    })?;
                    let item_id = command
                        .get("item_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] items.pickup.upsert requires string 'item_id'"
                            )
                        })?;
                    let quantity = command
                        .get("quantity")
                        .map(|_| command_u32(command, "quantity", index))
                        .transpose()?
                        .unwrap_or(1);
                    let collection_radius = command
                        .get("collection_radius")
                        .map(|_| command_number(command, "collection_radius", index))
                        .transpose()?
                        .unwrap_or(2.0);
                    let requires_interact = command
                        .get("requires_interact")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    self.items.upsert_pickup(WorldPickup {
                        id: id.to_owned(),
                        item_id: item_id.to_owned(),
                        quantity,
                        position: command_vec3(command, "position", index)?,
                        collection_radius,
                        requires_interact,
                        collected: false,
                    })?;
                }
                "items.pickup.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] items.pickup.remove requires string 'id'")
                    })?;
                    self.items.remove_pickup(id);
                }
                "items.pickup.collect" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] items.pickup.collect requires string 'id'")
                    })?;
                    let owner_id = command
                        .get("owner_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] items.pickup.collect requires string 'owner_id'"
                            )
                        })?;
                    let outcome = self.items.collect(
                        id,
                        owner_id,
                        command_vec3(command, "position", index)?,
                    )?;
                    host::publish_event_json(
                        "gameplay.pickup.collection",
                        "newviso.items",
                        serde_json::to_value(&outcome).map_err(|error| error.to_string())?,
                    )?;
                }
                "world.clock.configure" => {
                    self.living_world.configure_clock(WorldClockPolicyDesc {
                        fixed_hz: command_number(command, "fixed_hz", index)?,
                        time_scale: command_number(command, "time_scale", index)?,
                        max_steps_per_frame: command_u32(command, "max_steps_per_frame", index)?,
                    })?;
                }
                "world.clock.set" => {
                    self.living_world.set_world_seconds(command_f64(
                        command,
                        "world_seconds",
                        index,
                    )?)?;
                }
                "world.simulation.configure" => {
                    let simulation_defaults = WorldSimulationPolicyDesc::default();
                    self.living_world
                        .configure_simulation(WorldSimulationPolicyDesc {
                            transient_full_radius: command_number(
                                command,
                                "transient_full_radius",
                                index,
                            )?,
                            transient_reduced_radius: command_number(
                                command,
                                "transient_reduced_radius",
                                index,
                            )?,
                            tier_hysteresis_radius: command
                                .get("tier_hysteresis_radius")
                                .map(|_| command_number(command, "tier_hysteresis_radius", index))
                                .transpose()?
                                .unwrap_or(simulation_defaults.tier_hysteresis_radius),
                            full_interval_seconds: command_number(
                                command,
                                "full_interval_seconds",
                                index,
                            )?,
                            reduced_interval_seconds: command_number(
                                command,
                                "reduced_interval_seconds",
                                index,
                            )?,
                            background_interval_seconds: command_number(
                                command,
                                "background_interval_seconds",
                                index,
                            )?,
                            max_actor_updates_per_step: command_u32(
                                command,
                                "max_actor_updates_per_step",
                                index,
                            )?,
                        })?;
                }
                "world.observer.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.observer.upsert requires string 'id'"
                        )
                    })?;
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    self.living_world.upsert_observer(WorldObserverDesc {
                        id: id.to_owned(),
                        position: command_vec3(command, "position", index)?,
                        full_radius: command_number(command, "full_radius", index)?,
                        reduced_radius: command_number(command, "reduced_radius", index)?,
                        importance: command
                            .get("importance")
                            .map(|_| command_number(command, "importance", index))
                            .transpose()?
                            .unwrap_or(1.0),
                        tags,
                    })?;
                }
                "world.observer.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.observer.remove requires string 'id'"
                        )
                    })?;
                    self.living_world.remove_observer(id);
                }
                "world.actor.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] world.actor.upsert requires string 'id'")
                    })?;
                    let kind = command.get("kind").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] world.actor.upsert requires string 'kind'")
                    })?;
                    let group = command
                        .get("group")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let channel = command
                        .get("channel")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    let enabled = command
                        .get("enabled")
                        .map(|value| value.as_bool().ok_or_else(|| {
                            format!("script command[{index}] world.actor.upsert 'enabled' must be boolean")
                        }))
                        .transpose()?
                        .unwrap_or(true);
                    self.living_world.upsert_actor(WorldActorDesc {
                        id: id.to_owned(),
                        kind: kind.to_owned(),
                        position: command_vec3(command, "position", index)?,
                        group,
                        channel,
                        enabled,
                        tags,
                        parameters: command_float_map(command, "parameters", index)?,
                        state: command.get("state").cloned().unwrap_or(Value::Null),
                    })?;
                }
                "world.actor.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] world.actor.remove requires string 'id'")
                    })?;
                    self.living_world.remove_actor(id);
                }
                "world.process.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] world.process.upsert requires string 'id'")
                    })?;
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    let enabled = command
                        .get("enabled")
                        .map(|value| value.as_bool().ok_or_else(|| {
                            format!("script command[{index}] world.process.upsert 'enabled' must be boolean")
                        }))
                        .transpose()?
                        .unwrap_or(true);
                    self.living_world.upsert_process(WorldProcessDesc {
                        id: id.to_owned(),
                        interval_seconds: command_f64(command, "interval_seconds", index)?,
                        phase_seconds: command
                            .get("phase_seconds")
                            .map(|_| command_f64(command, "phase_seconds", index))
                            .transpose()?
                            .unwrap_or(0.0),
                        enabled,
                        priority: command
                            .get("priority")
                            .map(|_| command_i32(command, "priority", index))
                            .transpose()?
                            .unwrap_or(0),
                        tags,
                        payload: command.get("payload").cloned().unwrap_or(Value::Null),
                    })?;
                }
                "world.process.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] world.process.remove requires string 'id'")
                    })?;
                    self.living_world.remove_process(id);
                }
                "world.fact.set" => {
                    let key = command.get("key").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] world.fact.set requires string 'key'")
                    })?;
                    self.living_world
                        .set_fact_with_cause(
                            key,
                            command.get("value").cloned().unwrap_or(Value::Null),
                            command.get("cause").filter(|v| !v.is_null()).map(|v| {
                                v.as_str().map(str::to_owned).ok_or_else(|| {
                                    format!("script command[{index}] world.fact.set 'cause' must be a string")
                                })
                            }).transpose()?,
                        )?;
                }
                "world.fact.remove" => {
                    let key = command.get("key").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] world.fact.remove requires string 'key'")
                    })?;
                    self.living_world.remove_fact(key);
                }
                "world.event.schedule" => {
                    let kind = command.get("kind").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.event.schedule requires string 'kind'"
                        )
                    })?;
                    let source = command
                        .get("source")
                        .and_then(Value::as_str)
                        .unwrap_or("project.script");
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    self.living_world.schedule_event(WorldScheduledEventDesc {
                        id: command
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        kind: kind.to_owned(),
                        source: source.to_owned(),
                        cause: command.get("cause").filter(|v| !v.is_null()).map(|v| {
                            v.as_str().map(str::to_owned).ok_or_else(|| {
                                format!("script command[{index}] world.event.schedule 'cause' must be a string")
                            })
                        }).transpose()?,
                        delay_seconds: command_f64(command, "delay_seconds", index)?,
                        ttl_seconds: command_f64(command, "ttl_seconds", index)?,
                        priority: command
                            .get("priority")
                            .map(|_| command_i32(command, "priority", index))
                            .transpose()?
                            .unwrap_or(0),
                        position: command_optional_vec3(command, "position", index)?,
                        tags,
                        payload: command.get("payload").cloned().unwrap_or(Value::Null),
                    })?;
                }
                "world.event.cancel" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] world.event.cancel requires string 'id'")
                    })?;
                    self.living_world.cancel_event(id);
                }
                "world.reality.record" => {
                    let kind = command.get("kind").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.reality.record requires string 'kind'"
                        )
                    })?;
                    let source = command
                        .get("source")
                        .and_then(Value::as_str)
                        .unwrap_or("project.script");
                    let cause = command
                        .get("cause")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let participants = if command.get("participants").is_some() {
                        command_strings(command, "participants", index)?
                    } else {
                        Vec::new()
                    };
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    self.living_world
                        .record_reality_event(WorldRealityEventDesc {
                            id: command
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned(),
                            kind: kind.to_owned(),
                            source: source.to_owned(),
                            cause,
                            participants,
                            position: command_optional_vec3(command, "position", index)?,
                            importance: command
                                .get("importance")
                                .map(|_| command_number(command, "importance", index))
                                .transpose()?
                                .unwrap_or(1.0),
                            tags,
                            payload: command.get("payload").cloned().unwrap_or(Value::Null),
                        })?;
                }
                "world.scenario.reserve" => {
                    let scenario_point_id = command
                        .get("scenario_point_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.scenario.reserve requires string 'scenario_point_id'"
                            )
                        })?;
                    let actor_id = command
                        .get("actor_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.scenario.reserve requires string 'actor_id'"
                            )
                        })?;
                    self.living_world.reserve_scenario(WorldScenarioReservationDesc {
                        id: command
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        scenario_point_id: scenario_point_id.to_owned(),
                        actor_id: actor_id.to_owned(),
                        delay_seconds: command
                            .get("delay_seconds")
                            .map(|_| command_f64(command, "delay_seconds", index))
                            .transpose()?
                            .unwrap_or(0.0),
                        duration_seconds: command_f64(command, "duration_seconds", index)?,
                        priority: command
                            .get("priority")
                            .map(|_| command_i32(command, "priority", index))
                            .transpose()?
                            .unwrap_or(0),
                        exclusive: command
                            .get("exclusive")
                            .map(|value| {
                                value.as_bool().ok_or_else(|| {
                                    format!(
                                        "script command[{index}] world.scenario.reserve 'exclusive' must be boolean"
                                    )
                                })
                            })
                            .transpose()?
                            .unwrap_or(true),
                        payload: command.get("payload").cloned().unwrap_or(Value::Null),
                    })?;
                }
                "world.scenario.release" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.scenario.release requires string 'id'"
                        )
                    })?;
                    self.living_world.release_scenario_reservation(id);
                }
                "world.navigation.node.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.navigation.node.upsert requires string 'id'"
                        )
                    })?;
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    self.living_world.upsert_nav_node(WorldNavNodeDesc {
                        id: id.to_owned(),
                        position: command_vec3(command, "position", index)?,
                        tags,
                        parameters: command_float_map(command, "parameters", index)?,
                    })?;
                }
                "world.navigation.node.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.navigation.node.remove requires string 'id'"
                        )
                    })?;
                    self.living_world.remove_nav_node(id)?;
                }
                "world.navigation.edge.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.navigation.edge.upsert requires string 'id'"
                        )
                    })?;
                    let from = command.get("from").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.navigation.edge.upsert requires string 'from'"
                        )
                    })?;
                    let to = command.get("to").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.navigation.edge.upsert requires string 'to'"
                        )
                    })?;
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    self.living_world.upsert_nav_edge(WorldNavEdgeDesc {
                        id: id.to_owned(),
                        from: from.to_owned(),
                        to: to.to_owned(),
                        bidirectional: command
                            .get("bidirectional")
                            .map(|value| {
                                value.as_bool().ok_or_else(|| {
                                    format!(
                                        "script command[{index}] world.navigation.edge.upsert 'bidirectional' must be boolean"
                                    )
                                })
                            })
                            .transpose()?
                            .unwrap_or(true),
                        distance: command
                            .get("distance")
                            .filter(|value| !value.is_null())
                            .map(|_| command_number(command, "distance", index))
                            .transpose()?,
                        cost_scale: command
                            .get("cost_scale")
                            .map(|_| command_number(command, "cost_scale", index))
                            .transpose()?
                            .unwrap_or(1.0),
                        enabled: command
                            .get("enabled")
                            .map(|value| {
                                value.as_bool().ok_or_else(|| {
                                    format!(
                                        "script command[{index}] world.navigation.edge.upsert 'enabled' must be boolean"
                                    )
                                })
                            })
                            .transpose()?
                            .unwrap_or(true),
                        tags,
                        parameters: command_float_map(command, "parameters", index)?,
                    })?;
                }
                "world.navigation.edge.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.navigation.edge.remove requires string 'id'"
                        )
                    })?;
                    self.living_world.remove_nav_edge(id);
                }
                "world.travel.start" => {
                    let actor_id = command
                        .get("actor_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.travel.start requires string 'actor_id'"
                            )
                        })?;
                    let destination_node = command
                        .get("destination_node")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.travel.start requires string 'destination_node'"
                            )
                        })?;
                    let start_node = command
                        .get("start_node")
                        .filter(|value| !value.is_null())
                        .map(|value| {
                            value.as_str().map(str::to_owned).ok_or_else(|| {
                                format!(
                                    "script command[{index}] world.travel.start 'start_node' must be a string"
                                )
                            })
                        })
                        .transpose()?;
                    let mode = command
                        .get("mode")
                        .and_then(Value::as_str)
                        .unwrap_or("generic");
                    self.living_world.start_travel(WorldTravelRequestDesc {
                        actor_id: actor_id.to_owned(),
                        start_node,
                        destination_node: destination_node.to_owned(),
                        speed: command_number(command, "speed", index)?,
                        mode: mode.to_owned(),
                        payload: command.get("payload").cloned().unwrap_or(Value::Null),
                    })?;
                }
                "world.travel.cancel" => {
                    let actor_id = command
                        .get("actor_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.travel.cancel requires string 'actor_id'"
                            )
                        })?;
                    self.living_world.cancel_travel(actor_id);
                }
                "world.actor.presentation.bind" => {
                    let actor_id = command
                        .get("actor_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.actor.presentation.bind requires string 'actor_id'"
                            )
                        })?;
                    let scene_key = command
                        .get("scene_key")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("world.actor.{actor_id}"));
                    let visual = match command
                        .get("visual")
                        .and_then(Value::as_str)
                        .unwrap_or("none")
                        .trim()
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "none" => SceneRuntimeVisualKind::None,
                        "cube" => SceneRuntimeVisualKind::Cube,
                        other => {
                            return Err(format!(
                                "script command[{index}] world.actor.presentation.bind unknown visual '{other}'"
                            ))
                        }
                    };
                    let materialized_representations =
                        if command.get("materialized_representations").is_some() {
                            command_strings(command, "materialized_representations", index)?
                                .into_iter()
                                .map(|value| value.trim().to_ascii_lowercase())
                                .collect()
                        } else {
                            vec!["physical".to_owned()]
                        };
                    let solid = command
                        .get("solid")
                        .map(|value| {
                            value.as_bool().ok_or_else(|| {
                                format!(
                                    "script command[{index}] world.actor.presentation.bind 'solid' must be boolean"
                                )
                            })
                        })
                        .transpose()?
                        .unwrap_or(false);
                    let locomotion = command
                        .get("locomotion")
                        .filter(|value| !value.is_null())
                        .map(|value| {
                            serde_json::from_value::<WorldActorLocomotionBinding>(value.clone())
                                .map_err(|error| {
                                    format!(
                                        "script command[{index}] world.actor.presentation.bind invalid locomotion: {error}"
                                    )
                                })
                        })
                        .transpose()?;
                    self.bind_world_actor_presentation(
                        actor_id.to_owned(),
                        WorldActorPresentationBinding {
                            scene_key,
                            visual,
                            asset_ref: command
                                .get("asset_ref")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            position_offset: command
                                .get("position_offset")
                                .map(|_| command_vec3(command, "position_offset", index))
                                .transpose()?
                                .unwrap_or([0.0; 3]),
                            rotation_degrees: command
                                .get("rotation_degrees")
                                .map(|_| command_vec3(command, "rotation_degrees", index))
                                .transpose()?
                                .unwrap_or([0.0; 3]),
                            scale: command
                                .get("scale")
                                .map(|_| command_vec3(command, "scale", index))
                                .transpose()?
                                .unwrap_or([1.0; 3]),
                            bounds_half_extent: command
                                .get("bounds_half_extent")
                                .map(|_| command_vec3(command, "bounds_half_extent", index))
                                .transpose()?
                                .unwrap_or([0.5; 3]),
                            base_color: command
                                .get("base_color")
                                .map(|_| command_vec4(command, "base_color", index))
                                .transpose()?
                                .unwrap_or([1.0; 4]),
                            solid,
                            visible_distance: command
                                .get("visible_distance")
                                .map(|_| command_number(command, "visible_distance", index))
                                .transpose()?
                                .unwrap_or(f32::INFINITY),
                            stream_distance: command
                                .get("stream_distance")
                                .map(|_| command_number(command, "stream_distance", index))
                                .transpose()?
                                .unwrap_or(f32::INFINITY),
                            fade_range: command
                                .get("fade_range")
                                .map(|_| command_number(command, "fade_range", index))
                                .transpose()?
                                .unwrap_or(0.0),
                            materialized_representations,
                            locomotion,
                        },
                    )?;
                }
                "world.actor.presentation.unbind" => {
                    let actor_id = command
                        .get("actor_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.actor.presentation.unbind requires string 'actor_id'"
                            )
                        })?;
                    self.unbind_world_actor_presentation(actor_id)?;
                }
                "world.population.channel.upsert" => {
                    let id = command
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.population.channel.upsert requires string 'id'"
                            )
                        })?;
                    let model_set = command
                        .get("model_set")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    self.living_world
                        .upsert_population_channel(PopulationChannelDesc {
                            id: id.to_owned(),
                            density: command_number(command, "density", index)?,
                            max_active: command_u32(command, "max_active", index)?,
                            spawn_radius: command_number(command, "spawn_radius", index)?,
                            despawn_radius: command_number(command, "despawn_radius", index)?,
                            creation_budget_per_tick: command
                                .get("creation_budget_per_tick")
                                .map(|_| command_u32(command, "creation_budget_per_tick", index))
                                .transpose()?
                                .unwrap_or(1),
                            removal_budget_per_tick: command
                                .get("removal_budget_per_tick")
                                .map(|_| command_u32(command, "removal_budget_per_tick", index))
                                .transpose()?
                                .unwrap_or(4),
                            update_interval_seconds: command
                                .get("update_interval_seconds")
                                .map(|_| command_number(command, "update_interval_seconds", index))
                                .transpose()?
                                .unwrap_or(0.0),
                            model_set,
                            tags,
                            parameters: command_float_map(command, "parameters", index)?,
                        })?;
                }
                "world.population.channel.remove" => {
                    let id = command
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.population.channel.remove requires string 'id'"
                            )
                        })?;
                    self.living_world.remove_population_channel(id);
                }
                "world.population.streaming.configure" => {
                    let fallback_set = command
                        .get("fallback_set")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    self.living_world.set_population_streaming_policy(
                        PopulationStreamingPolicyDesc {
                            max_resident_sets: command_u32(command, "max_resident_sets", index)?,
                            request_budget_per_tick: command_u32(
                                command,
                                "request_budget_per_tick",
                                index,
                            )?,
                            eviction_budget_per_tick: command_u32(
                                command,
                                "eviction_budget_per_tick",
                                index,
                            )?,
                            fallback_set,
                        },
                    )?;
                }
                "world.zone.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] world.zone.upsert requires string 'id'")
                    })?;
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    self.living_world.upsert_zone(LivingWorldZoneDesc {
                        id: id.to_owned(),
                        min: command_vec3(command, "min", index)?,
                        max: command_vec3(command, "max", index)?,
                        priority: command
                            .get("priority")
                            .map(|_| command_i32(command, "priority", index))
                            .transpose()?
                            .unwrap_or(0),
                        tags,
                        parameters: command_float_map(command, "parameters", index)?,
                    })?;
                }
                "world.zone.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] world.zone.remove requires string 'id'")
                    })?;
                    self.living_world.remove_zone(id);
                }
                "world.model_set.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.model_set.upsert requires string 'id'"
                        )
                    })?;
                    let category = command
                        .get("category")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.model_set.upsert requires string 'category'"
                            )
                        })?;
                    let assets = command_strings(command, "assets", index)?;
                    let weights = if let Some(values) =
                        command.get("weights").and_then(Value::as_array)
                    {
                        let mut out = Vec::with_capacity(values.len());
                        for (weight_index, value) in values.iter().enumerate() {
                            let weight = value.as_f64().ok_or_else(|| {
                                format!(
                                    "script command[{index}] world.model_set.upsert weights[{weight_index}] must be numeric"
                                )
                            })? as f32;
                            if !weight.is_finite() {
                                return Err(format!(
                                    "script command[{index}] world.model_set.upsert weights[{weight_index}] must be finite"
                                ));
                            }
                            out.push(weight);
                        }
                        out
                    } else {
                        Vec::new()
                    };
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    self.living_world.upsert_model_set(AmbientModelSetDesc {
                        id: id.to_owned(),
                        category: category.to_owned(),
                        assets,
                        weights,
                        tags,
                    })?;
                }
                "world.model_set.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.model_set.remove requires string 'id'"
                        )
                    })?;
                    self.living_world.remove_model_set(id);
                }
                "world.scenario_point.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.scenario_point.upsert requires string 'id'"
                        )
                    })?;
                    let kind = command.get("kind").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.scenario_point.upsert requires string 'kind'"
                        )
                    })?;
                    let group = command
                        .get("group")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let model_set = command
                        .get("model_set")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    self.living_world.upsert_scenario_point(ScenarioPointDesc {
                        id: id.to_owned(),
                        kind: kind.to_owned(),
                        group,
                        position: command_vec3(command, "position", index)?,
                        heading_degrees: command
                            .get("heading_degrees")
                            .map(|_| command_number(command, "heading_degrees", index))
                            .transpose()?
                            .unwrap_or(0.0),
                        radius: command
                            .get("radius")
                            .map(|_| command_number(command, "radius", index))
                            .transpose()?
                            .unwrap_or(1.0),
                        probability: command
                            .get("probability")
                            .map(|_| command_number(command, "probability", index))
                            .transpose()?
                            .unwrap_or(1.0),
                        model_set,
                        enabled: command
                            .get("enabled")
                            .map(|value| {
                                value.as_bool().ok_or_else(|| {
                                    format!(
                                        "script command[{index}] world.scenario_point.upsert 'enabled' must be boolean"
                                    )
                                })
                            })
                            .transpose()?
                            .unwrap_or(true),
                        tags,
                        parameters: command_float_map(command, "parameters", index)?,
                    })?;
                }
                "world.scenario_point.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.scenario_point.remove requires string 'id'"
                        )
                    })?;
                    self.living_world.remove_scenario_point(id);
                }
                "world.relationship.set" => {
                    let source_group = command
                        .get("source_group")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.relationship.set requires string 'source_group'"
                            )
                        })?;
                    let target_group = command
                        .get("target_group")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.relationship.set requires string 'target_group'"
                            )
                        })?;
                    let relation = command
                        .get("relation")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.relationship.set requires string 'relation'"
                            )
                        })?;
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    let desc = RelationshipRuleDesc {
                        source_group: source_group.to_owned(),
                        target_group: target_group.to_owned(),
                        relation: relation.to_owned(),
                        weight: command
                            .get("weight")
                            .map(|_| command_number(command, "weight", index))
                            .transpose()?
                            .unwrap_or(1.0),
                        tags,
                    };
                    let reciprocal = command
                        .get("reciprocal")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    self.living_world.upsert_relationship(desc.clone())?;
                    if reciprocal && source_group != target_group {
                        self.living_world
                            .upsert_relationship(RelationshipRuleDesc {
                                source_group: target_group.to_owned(),
                                target_group: source_group.to_owned(),
                                ..desc
                            })?;
                    }
                }
                "world.relationship.remove" => {
                    let source_group = command
                        .get("source_group")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.relationship.remove requires string 'source_group'"
                            )
                        })?;
                    let target_group = command
                        .get("target_group")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] world.relationship.remove requires string 'target_group'"
                            )
                        })?;
                    self.living_world
                        .remove_relationship(source_group, target_group);
                    if command
                        .get("reciprocal")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        self.living_world
                            .remove_relationship(target_group, source_group);
                    }
                }
                "world.stimulus.emit" => {
                    let id = command.get("id").and_then(Value::as_str).unwrap_or("");
                    let kind = command.get("kind").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] world.stimulus.emit requires string 'kind'"
                        )
                    })?;
                    let source = command
                        .get("source")
                        .and_then(Value::as_str)
                        .unwrap_or("project.script");
                    let tags = if command.get("tags").is_some() {
                        command_strings(command, "tags", index)?
                    } else {
                        Vec::new()
                    };
                    self.living_world.emit_stimulus(WorldStimulusDesc {
                        id: id.to_owned(),
                        kind: kind.to_owned(),
                        source: source.to_owned(),
                        position: command_vec3(command, "position", index)?,
                        radius: command_number(command, "radius", index)?,
                        intensity: command_number(command, "intensity", index)?,
                        lifetime_seconds: command_number(command, "lifetime_seconds", index)?,
                        tags,
                        payload: command.get("payload").cloned().unwrap_or(Value::Null),
                    })?;
                }
                "world.stimulus.clear" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] world.stimulus.clear requires string 'id'")
                    })?;
                    self.living_world.clear_stimulus(id);
                }
                "scene.entity.process_claim.set" => {
                    let entity = command
                        .get("entity")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.process_claim.set requires unsigned integer 'entity'"
                            )
                        })?;
                    let reason = command
                        .get("reason")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.process_claim.set requires string 'reason'"
                            )
                        })?;
                    let active = command
                        .get("active")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.process_claim.set requires boolean 'active'"
                            )
                        })?;
                    self.scene.set_entity_process_claim(
                        entity,
                        "project.script",
                        reason,
                        active,
                    )?;
                }
                "scene.entity.process_rate.set" => {
                    let entity = command
                        .get("entity")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.process_rate.set requires unsigned integer 'entity'"
                            )
                        })?;
                    let reason = command
                        .get("reason")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.process_rate.set requires string 'reason'"
                            )
                        })?;
                    let hz = command_number(command, "hz", index)?;
                    self.scene.set_entity_process_rate_hz(entity, reason, hz)?;
                }
                "scene.entity.visibility.set" => {
                    let visible = command
                        .get("visible")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.visibility.set requires boolean 'visible'"
                            )
                        })?;
                    let channel = command
                        .get("channel")
                        .and_then(Value::as_str)
                        .unwrap_or("project.script");
                    let entity = self.resolve_script_scene_entity(
                        command,
                        index,
                        "scene.entity.visibility.set",
                    )?;
                    self.scene.set_entity_visibility(entity, channel, visible)?;
                }
                "scene.light.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] scene.light.upsert requires string 'id'")
                    })?;
                    let light_type = match command
                        .get("light_type")
                        .or_else(|| command.get("type"))
                        .and_then(Value::as_str)
                        .unwrap_or("point")
                        .trim()
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "directional" => SceneLightType::Directional,
                        "point" => SceneLightType::Point,
                        "spot" => SceneLightType::Spot,
                        "area" => SceneLightType::Area,
                        other => {
                            return Err(format!(
                                "script command[{index}] unknown light_type '{other}'"
                            ))
                        }
                    };
                    let mut desc = SceneLightDesc::default();
                    desc.light_type = light_type;
                    if command.get("color").is_some() {
                        desc.color = command_vec3(command, "color", index)?;
                    }
                    if command.get("intensity").is_some() {
                        desc.intensity = command_number(command, "intensity", index)?;
                    }
                    if command.get("range").is_some() {
                        desc.range = command_number(command, "range", index)?;
                    }
                    if command.get("cone_inner_degrees").is_some() {
                        desc.cone_inner_degrees =
                            command_number(command, "cone_inner_degrees", index)?;
                    }
                    if command.get("cone_outer_degrees").is_some() {
                        desc.cone_outer_degrees =
                            command_number(command, "cone_outer_degrees", index)?;
                    }
                    if let Some(value) = command.get("casts_shadows") {
                        desc.casts_shadows = value.as_bool().ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.light.upsert 'casts_shadows' must be boolean"
                            )
                        })?;
                    }
                    if command.get("shadow_bias").is_some() {
                        desc.shadow_bias = command_number(command, "shadow_bias", index)?;
                    }
                    if command.get("shadow_normal_bias").is_some() {
                        desc.shadow_normal_bias =
                            command_number(command, "shadow_normal_bias", index)?;
                    }
                    if let Some(value) = command.get("shadow_resolution") {
                        let resolution = value.as_u64().ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.light.upsert 'shadow_resolution' must be unsigned integer"
                            )
                        })?;
                        desc.shadow_resolution = u32::try_from(resolution).map_err(|_| {
                            format!(
                                "script command[{index}] scene.light.upsert shadow_resolution out of range"
                            )
                        })?;
                    }
                    if command.get("shadow_distance").is_some() {
                        desc.shadow_distance = command_number(command, "shadow_distance", index)?;
                    }
                    self.scene.upsert_runtime_light(id, desc)?;
                }
                "scene.sky_visual.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] scene.sky_visual.upsert requires string 'id'"
                        )
                    })?;
                    let kind = match command
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("disc")
                        .trim()
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "disc" => SkyVisualKind::Disc,
                        "billboard" => SkyVisualKind::Billboard,
                        other => {
                            return Err(format!(
                                "script command[{index}] unknown sky visual kind '{other}'"
                            ))
                        }
                    };
                    let mut desc = SkyVisualDesc::default();
                    desc.kind = kind;
                    if command.get("color").is_some() {
                        desc.color = command_vec3(command, "color", index)?;
                    }
                    if command.get("intensity").is_some() {
                        desc.intensity = command_number(command, "intensity", index)?;
                    }
                    if command.get("angular_size_degrees").is_some() {
                        desc.angular_size_degrees =
                            command_number(command, "angular_size_degrees", index)?;
                    }
                    if command.get("halo_size_degrees").is_some() {
                        desc.halo_size_degrees =
                            command_number(command, "halo_size_degrees", index)?;
                    }
                    if command.get("halo_intensity").is_some() {
                        desc.halo_intensity = command_number(command, "halo_intensity", index)?;
                    }
                    if let Some(value) = command.get("atmosphere_driver") {
                        desc.atmosphere_driver = value.as_bool().ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.sky_visual.upsert 'atmosphere_driver' must be boolean"
                            )
                        })?;
                    }
                    self.scene.upsert_runtime_sky_visual(id, desc)?;
                }
                "scene.sky_visual.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] scene.sky_visual.remove requires string 'id'"
                        )
                    })?;
                    self.scene.remove_runtime_sky_visual(id)?;
                }
                "scene.sky.atmosphere.set" => {
                    self.scene.set_sky_atmosphere(SkyAtmosphereDesc {
                        twilight_altitudes: command_vec4(command, "twilight_altitudes", index)?,
                        daylight_altitudes: command_vector::<2>(
                            command,
                            "daylight_altitudes",
                            index,
                        )?,
                        horizon_power: command_number(command, "horizon_power", index)?,
                        tonemap_shoulder: command_number(command, "tonemap_shoulder", index)?,
                        night_zenith: command_vec3(command, "night_zenith", index)?,
                        night_horizon: command_vec3(command, "night_horizon", index)?,
                        astronomical_zenith: command_vec3(command, "astronomical_zenith", index)?,
                        astronomical_horizon: command_vec3(command, "astronomical_horizon", index)?,
                        nautical_zenith: command_vec3(command, "nautical_zenith", index)?,
                        nautical_horizon: command_vec3(command, "nautical_horizon", index)?,
                        civil_zenith: command_vec3(command, "civil_zenith", index)?,
                        civil_horizon: command_vec3(command, "civil_horizon", index)?,
                        day_zenith: command_vec3(command, "day_zenith", index)?,
                        day_horizon: command_vec3(command, "day_horizon", index)?,
                        sunset_tint: command_vec3(command, "sunset_tint", index)?,
                        sunset_strength: command_number(command, "sunset_strength", index)?,
                        cloud_night: command_vec3(command, "cloud_night", index)?,
                        cloud_twilight_shadow: command_vec3(
                            command,
                            "cloud_twilight_shadow",
                            index,
                        )?,
                        cloud_twilight_light: command_vec3(command, "cloud_twilight_light", index)?,
                        cloud_day_shadow: command_vec3(command, "cloud_day_shadow", index)?,
                        cloud_day_light: command_vec3(command, "cloud_day_light", index)?,
                        star_tint: command_vec3(command, "star_tint", index)?,
                        star_intensity: command_number(command, "star_intensity", index)?,
                        star_visibility_altitudes: command_vector::<2>(
                            command,
                            "star_visibility_altitudes",
                            index,
                        )?,
                        cloud_occlusion: command_number(command, "cloud_occlusion", index)?,
                        silver_lining_tint: command_vec3(command, "silver_lining_tint", index)?,
                        silver_lining_strength: command_number(
                            command,
                            "silver_lining_strength",
                            index,
                        )?,
                        cloud_alpha_range: command_vector::<2>(
                            command,
                            "cloud_alpha_range",
                            index,
                        )?,
                    })?;
                }
                "scene.cloudhat.keyframe.set" => {
                    let current = self.scene.cloudhat_keyframe();
                    let enabled = command
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let optional_vec4 =
                        |key: &str, fallback: [f32; 4]| -> Result<[f32; 4], String> {
                            command
                                .get(key)
                                .map(|_| command_vec4(command, key, index))
                                .transpose()
                                .map(|value| value.unwrap_or(fallback))
                        };
                    self.scene.set_cloudhat_keyframe(CloudHatKeyframeState {
                        enabled,
                        cloud_color: optional_vec4("cloud_color", current.cloud_color)?,
                        cloud_light_color: optional_vec4(
                            "cloud_light_color",
                            current.cloud_light_color,
                        )?,
                        cloud_ambient_color: optional_vec4(
                            "cloud_ambient_color",
                            current.cloud_ambient_color,
                        )?,
                        cloud_sky_color: optional_vec4("cloud_sky_color", current.cloud_sky_color)?,
                        cloud_bounce_color: optional_vec4(
                            "cloud_bounce_color",
                            current.cloud_bounce_color,
                        )?,
                        cloud_east_color: optional_vec4(
                            "cloud_east_color",
                            current.cloud_east_color,
                        )?,
                        cloud_west_color: optional_vec4(
                            "cloud_west_color",
                            current.cloud_west_color,
                        )?,
                        scale_fill_colors: optional_vec4(
                            "scale_fill_colors",
                            current.scale_fill_colors,
                        )?,
                        density_shift_scale_scattering: optional_vec4(
                            "density_shift_scale_scattering",
                            current.density_shift_scale_scattering,
                        )?,
                        piercing_light: optional_vec4("piercing_light", current.piercing_light)?,
                        scale_diffuse_fill_ambient_wrap: optional_vec4(
                            "scale_diffuse_fill_ambient_wrap",
                            current.scale_diffuse_fill_ambient_wrap,
                        )?,
                    })?;
                }
                "scene.sky_clouds.set" => {
                    let current = self.scene.sky_clouds();
                    let enabled = command
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(current.enabled);
                    let coverage = command
                        .get("coverage")
                        .map(|_| command_number(command, "coverage", index))
                        .transpose()?
                        .unwrap_or(current.coverage);
                    let density = command
                        .get("density")
                        .map(|_| command_number(command, "density", index))
                        .transpose()?
                        .unwrap_or(current.density);
                    let softness = command
                        .get("softness")
                        .map(|_| command_number(command, "softness", index))
                        .transpose()?
                        .unwrap_or(current.softness);
                    let scale = command
                        .get("scale")
                        .map(|_| command_number(command, "scale", index))
                        .transpose()?
                        .unwrap_or(current.scale);
                    let detail_scale = command
                        .get("detail_scale")
                        .map(|_| command_number(command, "detail_scale", index))
                        .transpose()?
                        .unwrap_or(current.detail_scale);
                    let speed = command
                        .get("speed")
                        .map(|_| command_vector::<2>(command, "speed", index))
                        .transpose()?
                        .unwrap_or(current.speed);
                    let horizon_fade = command
                        .get("horizon_fade")
                        .map(|_| command_number(command, "horizon_fade", index))
                        .transpose()?
                        .unwrap_or(current.horizon_fade);
                    let macro_scale = command
                        .get("macro_scale")
                        .map(|_| command_number(command, "macro_scale", index))
                        .transpose()?
                        .unwrap_or(current.macro_scale);
                    let macro_strength = command
                        .get("macro_strength")
                        .map(|_| command_number(command, "macro_strength", index))
                        .transpose()?
                        .unwrap_or(current.macro_strength);
                    let detail_strength = command
                        .get("detail_strength")
                        .map(|_| command_number(command, "detail_strength", index))
                        .transpose()?
                        .unwrap_or(current.detail_strength);
                    let micro_strength = command
                        .get("micro_strength")
                        .map(|_| command_number(command, "micro_strength", index))
                        .transpose()?
                        .unwrap_or(current.micro_strength);
                    let erosion_strength = command
                        .get("erosion_strength")
                        .map(|_| command_number(command, "erosion_strength", index))
                        .transpose()?
                        .unwrap_or(current.erosion_strength);
                    let warp_strength = command
                        .get("warp_strength")
                        .map(|_| command_number(command, "warp_strength", index))
                        .transpose()?
                        .unwrap_or(current.warp_strength);
                    let shape_contrast = command
                        .get("shape_contrast")
                        .map(|_| command_number(command, "shape_contrast", index))
                        .transpose()?
                        .unwrap_or(current.shape_contrast);
                    let shear_speed = command
                        .get("shear_speed")
                        .map(|_| command_vector::<2>(command, "shear_speed", index))
                        .transpose()?
                        .unwrap_or(current.shear_speed);
                    let seed_offset = command
                        .get("seed_offset")
                        .map(|_| command_vector::<2>(command, "seed_offset", index))
                        .transpose()?
                        .unwrap_or(current.seed_offset);
                    let large_speed = command
                        .get("large_speed")
                        .map(|_| command_number(command, "large_speed", index))
                        .transpose()?
                        .unwrap_or(current.large_speed);
                    let small_speed = command
                        .get("small_speed")
                        .map(|_| command_number(command, "small_speed", index))
                        .transpose()?
                        .unwrap_or(current.small_speed);
                    let overall_detail_speed = command
                        .get("overall_detail_speed")
                        .map(|_| command_number(command, "overall_detail_speed", index))
                        .transpose()?
                        .unwrap_or(current.overall_detail_speed);
                    let edge_detail_speed = command
                        .get("edge_detail_speed")
                        .map(|_| command_number(command, "edge_detail_speed", index))
                        .transpose()?
                        .unwrap_or(current.edge_detail_speed);
                    let noise_phase_scale = command
                        .get("noise_phase_scale")
                        .map(|_| command_number(command, "noise_phase_scale", index))
                        .transpose()?
                        .unwrap_or(current.noise_phase_scale);

                    self.scene.set_sky_clouds(SkyCloudDesc {
                        enabled,
                        coverage,
                        density,
                        softness,
                        scale,
                        detail_scale,
                        speed,
                        horizon_fade,
                        macro_scale,
                        macro_strength,
                        detail_strength,
                        micro_strength,
                        erosion_strength,
                        warp_strength,
                        shape_contrast,
                        shear_speed,
                        seed_offset,
                        large_speed,
                        small_speed,
                        overall_detail_speed,
                        edge_detail_speed,
                        noise_phase_scale,
                    })?;
                }
                "scene.volumetric_clouds.set" => {
                    let mut desc = self.scene.volumetric_clouds().unwrap_or_default();
                    if let Some(value) = command.get("enabled") {
                        desc.enabled = value.as_bool().ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.volumetric_clouds.set 'enabled' must be boolean"
                            )
                        })?;
                    }
                    macro_rules! optional_cloud_number {
                        ($field:ident, $name:literal) => {
                            if command.get($name).is_some() {
                                desc.$field = command_number(command, $name, index)?;
                            }
                        };
                    }
                    optional_cloud_number!(base_altitude, "base_altitude");
                    optional_cloud_number!(top_altitude, "top_altitude");
                    optional_cloud_number!(max_distance, "max_distance");
                    optional_cloud_number!(resolution_scale, "resolution_scale");
                    optional_cloud_number!(coverage, "coverage");
                    optional_cloud_number!(density, "density");
                    optional_cloud_number!(shape_scale, "shape_scale");
                    optional_cloud_number!(detail_scale, "detail_scale");
                    optional_cloud_number!(detail_strength, "detail_strength");
                    optional_cloud_number!(erosion_strength, "erosion_strength");
                    optional_cloud_number!(extinction, "extinction");
                    optional_cloud_number!(scattering, "scattering");
                    optional_cloud_number!(ambient, "ambient");
                    optional_cloud_number!(phase_forward, "phase_forward");
                    optional_cloud_number!(powder_strength, "powder_strength");
                    optional_cloud_number!(temporal_blend, "temporal_blend");
                    optional_cloud_number!(jitter_strength, "jitter_strength");
                    if let Some(value) = command.get("ray_steps") {
                        let value = value.as_u64().ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.volumetric_clouds.set 'ray_steps' must be unsigned integer"
                            )
                        })?;
                        desc.ray_steps = u32::try_from(value).map_err(|_| {
                            format!(
                                "script command[{index}] scene.volumetric_clouds.set ray_steps out of range"
                            )
                        })?;
                    }
                    if let Some(value) = command.get("light_steps") {
                        let value = value.as_u64().ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.volumetric_clouds.set 'light_steps' must be unsigned integer"
                            )
                        })?;
                        desc.light_steps = u32::try_from(value).map_err(|_| {
                            format!(
                                "script command[{index}] scene.volumetric_clouds.set light_steps out of range"
                            )
                        })?;
                    }
                    self.scene.set_volumetric_clouds(desc)?;
                }
                "scene.atmospheric_cloud_layer.target.set" => {
                    let layer = command
                        .get("layer")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] atmospheric cloud target requires string 'layer'"
                            )
                        })?;
                    let alpha = command_number(command, "alpha", index)?;
                    let transition_seconds = command
                        .get("transition_seconds")
                        .map(|_| command_number(command, "transition_seconds", index))
                        .transpose()?
                        .unwrap_or(5.0);
                    self.scene.set_atmospheric_cloud_layer_target(
                        layer,
                        alpha,
                        transition_seconds,
                    )?;
                }
                "scene.atmospheric_cloud_layer.target.clear" => {
                    let layer = command
                        .get("layer")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] atmospheric cloud clear requires string 'layer'"
                            )
                        })?;
                    self.scene.clear_atmospheric_cloud_layer_target(layer)?;
                }
                "scene.lens_flare.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] scene.lens_flare.upsert requires string 'id'"
                        )
                    })?;
                    let source = command.get("source").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] scene.lens_flare.upsert requires string 'source'")
                    })?;
                    let enabled = command
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let intensity = if command.get("intensity").is_some() {
                        command_number(command, "intensity", index)?
                    } else {
                        1.0
                    };
                    let scale = if command.get("scale").is_some() {
                        command_number(command, "scale", index)?
                    } else {
                        1.0
                    };
                    let occlusion_test = command
                        .get("occlusion_test")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let elements_value = command
                        .get("elements")
                        .and_then(Value::as_array)
                        .ok_or_else(|| {
                            format!("script command[{index}] scene.lens_flare.upsert requires array 'elements'")
                        })?;
                    let mut elements = Vec::with_capacity(elements_value.len());
                    for (element_index, element) in elements_value.iter().enumerate() {
                        let kind = match element
                            .get("kind")
                            .and_then(Value::as_str)
                            .unwrap_or("ghost")
                            .trim()
                            .to_ascii_lowercase()
                            .as_str()
                        {
                            "halo" => LensFlareElementKind::Halo,
                            "ghost" => LensFlareElementKind::Ghost,
                            "streak" => LensFlareElementKind::Streak,
                            other => {
                                return Err(format!(
                                    "script command[{index}] flare element[{element_index}] unknown kind '{other}'"
                                ))
                            }
                        };
                        elements.push(LensFlareElementDesc {
                            kind,
                            offset: command_number(element, "offset", element_index)?,
                            size: command_number(element, "size", element_index)?,
                            color: command_vec3(element, "color", element_index)?,
                            alpha: command_number(element, "alpha", element_index)?,
                        });
                    }
                    self.scene.upsert_lens_flare(
                        id,
                        LensFlareDesc {
                            source: source.to_owned(),
                            enabled,
                            intensity,
                            scale,
                            occlusion_test,
                            elements,
                        },
                    )?;
                }
                "scene.lens_flare.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] scene.lens_flare.remove requires string 'id'"
                        )
                    })?;
                    self.scene.remove_lens_flare(id);
                }
                "scene.mass_instance.debug.populate_grid" => {
                    let layer = command
                        .get("layer")
                        .and_then(Value::as_str)
                        .unwrap_or("debug.mass_instances");
                    let count = command
                        .get("count")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] mass-instance debug grid requires unsigned integer 'count'"
                            )
                        })?;
                    let count = u32::try_from(count).map_err(|_| {
                        format!(
                            "script command[{index}] mass-instance debug grid count exceeds u32"
                        )
                    })?;
                    let spacing = command
                        .get("spacing")
                        .map(|_| command_number(command, "spacing", index))
                        .transpose()?
                        .unwrap_or(0.25);
                    let base_color = command
                        .get("base_color")
                        .map(|_| command_vec4(command, "base_color", index))
                        .transpose()?
                        .unwrap_or([0.62, 0.35, 0.105, 1.0]);
                    self.scene
                        .populate_mass_instance_debug_grid(layer, count, spacing, base_color)?;
                }
                "scene.mass_instance.upsert" => {
                    let layer = command
                        .get("layer")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.mass_instance.upsert requires string 'layer'"
                            )
                        })?;
                    let slot = command
                        .get("slot")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.mass_instance.upsert requires unsigned integer 'slot'"
                            )
                        })?;
                    let slot = u32::try_from(slot).map_err(|_| {
                        format!(
                            "script command[{index}] scene.mass_instance.upsert slot exceeds u32"
                        )
                    })?;
                    self.scene.upsert_mass_instance(
                        layer,
                        slot,
                        SceneMassInstanceDesc {
                            position: command_vec3(command, "position", index)?,
                            rotation_degrees: command
                                .get("rotation_degrees")
                                .map(|_| command_vec3(command, "rotation_degrees", index))
                                .transpose()?
                                .unwrap_or([0.0; 3]),
                            scale: command_vec3(command, "scale", index)?,
                            base_color: command
                                .get("base_color")
                                .map(|_| command_vec4(command, "base_color", index))
                                .transpose()?
                                .unwrap_or([1.0; 4]),
                            visible: command
                                .get("visible")
                                .and_then(Value::as_bool)
                                .unwrap_or(true),
                        },
                    )?;
                }
                "scene.mass_instance.visible.set" => {
                    let layer = command
                        .get("layer")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.mass_instance.visible.set requires string 'layer'"
                            )
                        })?;
                    let slot = command
                        .get("slot")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.mass_instance.visible.set requires unsigned integer 'slot'"
                            )
                        })?;
                    let slot = u32::try_from(slot).map_err(|_| {
                        format!(
                            "script command[{index}] scene.mass_instance.visible.set slot exceeds u32"
                        )
                    })?;
                    let visible = command
                        .get("visible")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.mass_instance.visible.set requires boolean 'visible'"
                            )
                        })?;
                    let _ = self.scene.set_mass_instance_visible(layer, slot, visible)?;
                }
                "scene.mass_instance.clear" => {
                    let layer = command
                        .get("layer")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.mass_instance.clear requires string 'layer'"
                            )
                        })?;
                    let _ = self.scene.clear_mass_instance_layer(layer)?;
                }
                "scene.dynamic_entity.upsert" => {
                    let id = command
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.dynamic_entity.upsert requires string 'id'"
                            )
                        })?;
                    let visual = match command
                        .get("visual")
                        .and_then(Value::as_str)
                        .unwrap_or("none")
                        .trim()
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "none" => SceneRuntimeVisualKind::None,
                        "cube" => SceneRuntimeVisualKind::Cube,
                        other => {
                            return Err(format!(
                                "script command[{index}] scene.dynamic_entity.upsert has invalid visual '{other}'"
                            ))
                        }
                    };
                    let asset_ref = command
                        .get("asset_ref")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let texture_dictionary = command
                        .get("texture_dictionary")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let solid = command
                        .get("solid")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    self.scene.upsert_runtime_dynamic_entity(
                        id,
                        SceneRuntimeEntityDesc {
                            visual,
                            asset_ref,
                            texture_dictionary,
                            position: command_vec3(command, "position", index)?,
                            rotation_degrees: command_vec3(command, "rotation_degrees", index)?,
                            scale: command_vec3(command, "scale", index)?,
                            bounds_half_extent: command_vec3(command, "bounds_half_extent", index)?,
                            base_color: command_vec4(command, "base_color", index)?,
                            solid,
                            visible_distance: command_number(command, "visible_distance", index)?,
                            stream_distance: command_number(command, "stream_distance", index)?,
                            fade_range: command_number(command, "fade_range", index)?,
                        },
                    )?;
                }
                "scene.entity.main_view_meshes.set" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] main_view_meshes.set requires string 'id'")
                    })?;
                    let prefixes = command.get("hidden_mesh_prefixes")
                        .and_then(Value::as_array)
                        .ok_or_else(|| format!("script command[{index}] requires 'hidden_mesh_prefixes' array"))?
                        .iter()
                        .map(|value| value.as_str().map(str::to_owned).ok_or_else(|| {
                            format!("script command[{index}] hidden_mesh_prefixes must contain strings")
                        }))
                        .collect::<Result<Vec<_>, String>>()?;
                    let stable_id = self.scene.runtime_entity_stable_id(id).ok_or_else(|| {
                        format!(
                            "script command[{index}] mesh visibility target '{id}' does not exist"
                        )
                    })?;
                    self.scene
                        .set_entity_main_view_hidden_meshes(stable_id, prefixes)?;
                }
                "scene.entity.animation.play" => {
                    let id = command
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.animation.play requires string 'id'"
                            )
                        })?;
                    let clip_ref = command
                        .get("clip_ref")
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.animation.play requires string 'clip_ref'"
                            )
                        })?;
                    let playback_rate = command
                        .get("playback_rate")
                        .map(|_| command_number(command, "playback_rate", index))
                        .transpose()?
                        .unwrap_or(1.0);
                    let restart_if_same = command
                        .get("restart_if_same")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let stable_id = self.scene.runtime_entity_stable_id(id).ok_or_else(|| {
                        format!(
                            "script command[{index}] animation target '{}' does not exist",
                            id
                        )
                    })?;
                    let _ = self.bind_scene_entity_animation(
                        stable_id,
                        SceneAnimationBinding {
                            clip_ref: clip_ref.to_owned(),
                            playback_rate,
                            restart_if_same,
                        },
                    )?;
                }
                "scene.entity.animation.stop" => {
                    let id = command
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.animation.stop requires string 'id'"
                            )
                        })?;
                    if let Some(stable_id) = self.scene.runtime_entity_stable_id(id) {
                        let _ = self.unbind_scene_entity_animation(stable_id)?;
                    }
                }
                "scene.entity.damage" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] scene.entity.damage requires string 'id'")
                    })?;
                    let stable_id = self.scene.runtime_entity_stable_id(id).ok_or_else(|| {
                        format!(
                            "script command[{index}] damage target '{}' does not exist",
                            id
                        )
                    })?;
                    let direct_damage = command
                        .get("damage")
                        .map(|_| command_number(command, "damage", index))
                        .transpose()?
                        .unwrap_or(0.0)
                        .max(0.0);
                    let contact_impulse = command
                        .get("impulse")
                        .map(|_| command_number(command, "impulse", index))
                        .transpose()?
                        .unwrap_or(0.0)
                        .max(0.0);
                    let fallback_point = self
                        .scene
                        .entity_transform_values(stable_id)
                        .map(|(position, _, _)| position)
                        .unwrap_or([0.0; 3]);
                    let point = command
                        .get("point")
                        .map(|_| command_vec3(command, "point", index))
                        .transpose()?
                        .unwrap_or(fallback_point);
                    let impulse_direction = command
                        .get("impulse_direction")
                        .map(|_| command_vec3(command, "impulse_direction", index))
                        .transpose()?
                        .unwrap_or([0.0, 1.0, 0.0]);
                    let source = command
                        .get("source_entity")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);

                    if let Some(activation) =
                        self.scene
                            .apply_entity_damage(stable_id, direct_damage, contact_impulse)?
                    {
                        let physics = self.physics.as_mut().ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.damage requires physics runtime"
                            )
                        })?;
                        physics.promote_scene_destructible(
                            activation,
                            application_physics::PhysicsDamageContact {
                                source,
                                target: stable_id,
                                direct_damage,
                                contact_impulse,
                                point,
                                impulse_direction,
                            },
                        )?;
                        self.scene.set_physics_process_active(stable_id, true)?;
                    }
                }
                "scene.entity.transform.set" => {
                    let id = command
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.transform.set requires string 'id'"
                            )
                        })?;
                    let position = command
                        .get("position")
                        .map(|_| command_vec3(command, "position", index))
                        .transpose()?;
                    let rotation = command
                        .get("rotation_degrees")
                        .map(|_| command_vec3(command, "rotation_degrees", index))
                        .transpose()?;
                    let scale = command
                        .get("scale")
                        .map(|_| command_vec3(command, "scale", index))
                        .transpose()?;
                    self.scene
                        .set_runtime_entity_transform(id, position, rotation, scale)?;
                }
                "scene.entity.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] scene.entity.remove requires string 'id'")
                    })?;
                    let if_exists = command
                        .get("if_exists")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if !if_exists || self.scene.runtime_entity_exists(id) {
                        if let Some(stable_id) = self.scene.runtime_entity_stable_id(id) {
                            self.scene_animation_bindings.remove(&stable_id);
                        }
                        self.scene.remove_runtime_entity(id)?;
                    }
                }
                "scene.camera.set_relative_to_entity" => {
                    let entity = command
                        .get("entity")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.camera.set_relative_to_entity requires non-empty string 'entity'"
                            )
                        })?;
                    let stable_id = self
                        .scene
                        .runtime_entity_stable_id(entity)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] camera parent entity '{entity}' does not exist"
                            )
                        })?;
                    let (parent_position, parent_rotation, _) = self
                        .scene
                        .entity_transform_values(stable_id)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] camera parent entity '{entity}' lost its transform"
                            )
                        })?;
                    let local_position = command_vec3(command, "local_position", index)?;
                    let local_forward = command_vec3(command, "local_forward", index)?;
                    let local_up = command
                        .get("local_up")
                        .map(|_| command_vec3(command, "local_up", index))
                        .transpose()?
                        .unwrap_or([0.0, 1.0, 0.0]);
                    let position_offset =
                        rotate_local_vector_degrees(local_position, parent_rotation);
                    let forward = rotate_local_vector_degrees(local_forward, parent_rotation);
                    let up = rotate_local_vector_degrees(local_up, parent_rotation);
                    let position = [
                        parent_position[0] + position_offset[0],
                        parent_position[1] + position_offset[1],
                        parent_position[2] + position_offset[2],
                    ];
                    let target = [
                        position[0] + forward[0],
                        position[1] + forward[1],
                        position[2] + forward[2],
                    ];
                    let fov = command
                        .get("fov_y_degrees")
                        .map(|value| {
                            value.as_f64().map(|v| v as f32).ok_or_else(|| {
                                format!(
                                    "script command[{index}] scene.camera.set_relative_to_entity 'fov_y_degrees' must be numeric"
                                )
                            })
                        })
                        .transpose()?;
                    self.scene.set_camera_pose(position, target, Some(up), fov)?;
                    let listener = json!({
                        "position": position,
                        "forward": forward,
                        "up": up,
                        "velocity": [0.0, 0.0, 0.0]
                    });
                    sync_audio_listener_if_available(index, &listener)?;
                }
                "scene.camera.set" => {
                    let mut position = command_vec3(command, "position", index)?;
                    if let Some(collision) = command.get("collision") {
                        let origin = command_vec3(collision, "origin", index)?;
                        let radius = command_number(collision, "radius", index)?;
                        if !(0.01..=2.0).contains(&radius) {
                            return Err(format!("script command[{index}] camera collision radius must be in 0.01..=2"));
                        }
                        let ignore = collision
                            .get("ignore_entity")
                            .map(|_| command_u64(collision, "ignore_entity", index))
                            .transpose()?;
                        let min = std::array::from_fn(|i| origin[i].min(position[i]) - radius);
                        let max = std::array::from_fn(|i| origin[i].max(position[i]) + radius);
                        let solids = self.scene.physics_static_solid_aabbs_near(&[(min, max)]);
                        let physics = self.physics.as_ref().ok_or_else(|| {
                            format!("script command[{index}] camera collision requires physics")
                        })?;
                        position =
                            physics.constrain_camera(origin, position, radius, ignore, &solids);
                    }
                    let target = command_vec3(command, "target", index)?;
                    let up = command
                        .get("up")
                        .map(|_| command_vec3(command, "up", index))
                        .transpose()?;
                    let fov = command
                        .get("fov_y_degrees")
                        .map(|value| {
                            value.as_f64().map(|v| v as f32).ok_or_else(|| {
                                format!(
                                    "script command[{index}] scene.camera.set 'fov_y_degrees' must be numeric"
                                )
                            })
                        })
                        .transpose()?;
                    self.scene.set_camera_pose(position, target, up, fov)?;

                    // The active scene camera is the engine-default spatial audio listener.
                    // A later explicit audio.listener.set command in the same script batch may
                    // override this for games with a listener independent from the render camera.
                    let listener = json!({
                        "position": position,
                        "forward": [
                            target[0] - position[0],
                            target[1] - position[1],
                            target[2] - position[2]
                        ],
                        "up": up.unwrap_or([0.0, 1.0, 0.0]),
                        "velocity": [0.0, 0.0, 0.0]
                    });
                    sync_audio_listener_if_available(index, &listener)?;
                }
                "scene.particle_effect.spawn" => {
                    let report = application_particle_effects::spawn_particle_effect(
                        &mut self.scene,
                        command,
                        index,
                    )?;
                    if report.skipped_model > 0 || report.skipped_trail > 0 {
                        host::warn(
                            "newviso.particles",
                            format!(
                                "particle effect '{}' source='{}' emitted={} skipped_model={} skipped_trail={} textures={:?}; model/trail branches remain preserved in YPT metadata but are not projected into billboard particles",
                                report.effect,
                                report.source,
                                report.emitted,
                                report.skipped_model,
                                report.skipped_trail,
                                report.textures,
                            ),
                        );
                    } else {
                        host::debug(
                            "newviso.particles",
                            format!(
                                "particle effect '{}' source='{}' emitted={} textures={:?}",
                                report.effect, report.source, report.emitted, report.textures,
                            ),
                        );
                    }
                }
                "scene.particles.spawn" => {
                    let items = command
                        .get("items")
                        .and_then(Value::as_array)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.particles.spawn requires array 'items'"
                            )
                        })?;
                    let mut particles = Vec::with_capacity(items.len());
                    for (item_index, item) in items.iter().enumerate() {
                        let blend = match item
                            .get("blend")
                            .and_then(Value::as_str)
                            .unwrap_or("alpha")
                            .trim()
                            .to_ascii_lowercase()
                            .as_str()
                        {
                            "alpha" => SceneParticleBlend::Alpha,
                            "additive" => SceneParticleBlend::Additive,
                            other => return Err(format!(
                                "script command[{index}] particle[{item_index}] has invalid blend '{other}'"
                            )),
                        };
                        particles.push(SceneParticleSpawnDesc {
                            position: command_vec3(item, "position", item_index)?,
                            velocity: item
                                .get("velocity")
                                .map(|_| command_vec3(item, "velocity", item_index))
                                .transpose()?
                                .unwrap_or([0.0; 3]),
                            acceleration: item
                                .get("acceleration")
                                .map(|_| command_vec3(item, "acceleration", item_index))
                                .transpose()?
                                .unwrap_or([0.0; 3]),
                            size: command_vector::<2>(item, "size", item_index)?,
                            end_size: item
                                .get("end_size")
                                .map(|_| command_vector::<2>(item, "end_size", item_index))
                                .transpose()?
                                .unwrap_or(command_vector::<2>(item, "size", item_index)?),
                            color: command_vec4(item, "color", item_index)?,
                            end_color: item
                                .get("end_color")
                                .map(|_| command_vec4(item, "end_color", item_index))
                                .transpose()?
                                .unwrap_or(command_vec4(item, "color", item_index)?),
                            lifetime_seconds: command_number(item, "lifetime_seconds", item_index)?,
                            rotation_degrees: item
                                .get("rotation_degrees")
                                .map(|_| command_number(item, "rotation_degrees", item_index))
                                .transpose()?
                                .unwrap_or(0.0),
                            angular_velocity_degrees: item
                                .get("angular_velocity_degrees")
                                .map(|_| {
                                    command_number(item, "angular_velocity_degrees", item_index)
                                })
                                .transpose()?
                                .unwrap_or(0.0),
                            blend,
                        });
                    }
                    self.scene.spawn_particles(particles)?;
                }
                "scene.particles.clear" => {
                    self.scene.clear_particles();
                }
                "scene.transient_spheres.set" => {
                    let items = command
                        .get("items")
                        .and_then(Value::as_array)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.transient_spheres.set requires array 'items'"
                            )
                        })?;
                    let mut spheres = Vec::with_capacity(items.len());
                    for (item_index, item) in items.iter().enumerate() {
                        spheres.push(SceneTransientSphere {
                            position: command_vec3(item, "position", item_index)?,
                            rotation_degrees: item
                                .get("rotation_degrees")
                                .map(|_| command_vec3(item, "rotation_degrees", item_index))
                                .transpose()?
                                .unwrap_or([0.0; 3]),
                            radius: command_number(item, "radius", item_index)?,
                            color: command_vec4(item, "color", item_index)?,
                            marker_color: item
                                .get("marker_color")
                                .map(|_| command_vec4(item, "marker_color", item_index))
                                .transpose()?,
                            marker_direction: if item.get("marker_color").is_some() {
                                command_vec3(item, "marker_direction", item_index)?
                            } else {
                                [0.0, 0.0, 1.0]
                            },
                            marker_threshold: if item.get("marker_color").is_some() {
                                command_number(item, "marker_threshold", item_index)?
                            } else {
                                1.0
                            },
                        });
                    }
                    self.scene.set_transient_spheres(spheres)?;
                }
                "scene.overlay_quads.set" => {
                    let items = command
                        .get("items")
                        .and_then(Value::as_array)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.overlay_quads.set requires array 'items'"
                            )
                        })?;
                    let mut quads = Vec::with_capacity(items.len());
                    for (item_index, item) in items.iter().enumerate() {
                        quads.push(SceneOverlayQuad {
                            rect: command_vec4(item, "rect", item_index)?,
                            color: command_vec4(item, "color", item_index)?,
                        });
                    }
                    self.scene.set_overlay_quads(quads)?;
                }
                other => {
                    return Err(format!(
                        "script command[{index}] uses unsupported engine command '{other}'"
                    ))
                }
            }
        }
        Ok(())
    }
}


fn rotate_local_vector_degrees(
    point: [f32; 3],
    rotation_degrees: [f32; 3],
) -> [f32; 3] {
    let mut p = point;
    let rx = rotation_degrees[0].to_radians();
    let ry = rotation_degrees[1].to_radians();
    let rz = rotation_degrees[2].to_radians();

    p = [
        p[0],
        p[1] * rx.cos() - p[2] * rx.sin(),
        p[1] * rx.sin() + p[2] * rx.cos(),
    ];
    p = [
        p[0] * ry.cos() + p[2] * ry.sin(),
        p[1],
        -p[0] * ry.sin() + p[2] * ry.cos(),
    ];
    [
        p[0] * rz.cos() - p[1] * rz.sin(),
        p[0] * rz.sin() + p[1] * rz.cos(),
        p[2],
    ]
}

fn parse_weather_effects(
    value: Option<&Value>,
    current: &WeatherEffectsState,
    index: usize,
) -> Result<WeatherEffectsState, String> {
    let Some(value) = value else {
        return Ok(current.clone());
    };
    if !value.is_object() {
        return Err(format!(
            "script command[{index}] weather 'effects' must be an object"
        ));
    }

    let mut state = current.clone();
    macro_rules! string_field {
        ($field:ident, $key:literal) => {
            if let Some(raw) = value.get($key) {
                state.$field = raw
                    .as_str()
                    .ok_or_else(|| {
                        format!(
                            "script command[{index}] weather effects '{}' must be a string",
                            $key
                        )
                    })?
                    .to_owned();
            }
        };
    }
    macro_rules! number_field {
        ($field:ident, $key:literal) => {
            if value.get($key).is_some() {
                state.$field = command_number(value, $key, index)?;
            }
        };
    }

    string_field!(current_cloud_settings, "current_cloud_settings");
    string_field!(next_cloud_settings, "next_cloud_settings");
    string_field!(current_timecycle, "current_timecycle");
    string_field!(next_timecycle, "next_timecycle");
    string_field!(current_drop_setting, "current_drop_setting");
    string_field!(next_drop_setting, "next_drop_setting");
    string_field!(current_mist_setting, "current_mist_setting");
    string_field!(next_mist_setting, "next_mist_setting");
    string_field!(current_ground_setting, "current_ground_setting");
    string_field!(next_ground_setting, "next_ground_setting");
    string_field!(current_cloud_variant, "current_cloud_variant");
    string_field!(next_cloud_variant, "next_cloud_variant");

    number_field!(sun, "sun");
    number_field!(cloud, "cloud");
    number_field!(wind_min, "wind_min");
    number_field!(wind_max, "wind_max");
    number_field!(wind_speed, "wind_speed");
    number_field!(rain, "rain");
    number_field!(snow, "snow");
    number_field!(snow_mist, "snow_mist");
    number_field!(fog, "fog");
    number_field!(ripple_bumpiness, "ripple_bumpiness");
    number_field!(ripple_min_bumpiness, "ripple_min_bumpiness");
    number_field!(ripple_max_bumpiness, "ripple_max_bumpiness");
    number_field!(ripple_bumpiness_wind_scale, "ripple_bumpiness_wind_scale");
    number_field!(ripple_scale, "ripple_scale");
    number_field!(ripple_speed, "ripple_speed");
    number_field!(ripple_velocity_transfer, "ripple_velocity_transfer");
    number_field!(ocean_bumpiness, "ocean_bumpiness");
    number_field!(deep_ocean_scale, "deep_ocean_scale");
    number_field!(ocean_noise_min_amplitude, "ocean_noise_min_amplitude");
    number_field!(ocean_wave_amplitude, "ocean_wave_amplitude");
    number_field!(shore_wave_amplitude, "shore_wave_amplitude");
    number_field!(ocean_wave_wind_scale, "ocean_wave_wind_scale");
    number_field!(shore_wave_wind_scale, "shore_wave_wind_scale");
    number_field!(ocean_wave_min_amplitude, "ocean_wave_min_amplitude");
    number_field!(shore_wave_min_amplitude, "shore_wave_min_amplitude");
    number_field!(ocean_wave_max_amplitude, "ocean_wave_max_amplitude");
    number_field!(shore_wave_max_amplitude, "shore_wave_max_amplitude");
    number_field!(ocean_foam_intensity, "ocean_foam_intensity");
    number_field!(ocean_foam_scale, "ocean_foam_scale");
    number_field!(ripple_disturb, "ripple_disturb");
    number_field!(lightning, "lightning");
    number_field!(sandstorm, "sandstorm");

    if value.get("wind_direction").is_some() {
        state.wind_direction = command_vector::<2>(value, "wind_direction", index)?;
    }
    Ok(state)
}

fn sync_audio_listener_if_available(index: usize, request: &Value) -> Result<(), String> {
    let encoded = serde_json::to_vec(request).map_err(|error| {
        format!("script command[{index}] audio listener encode failed: {error}")
    })?;
    match host::call_service("engine.audio", "set_listener_json_v1", &encoded) {
        Ok(_) => Ok(()),
        Err(error) if error.contains("is not registered") => Ok(()),
        Err(error) => Err(format!(
            "script command[{index}] default engine.audio listener sync failed: {error}"
        )),
    }
}

fn invoke_audio_service(index: usize, method: &str, request: &Value) -> Result<(), String> {
    let encoded = serde_json::to_vec(request)
        .map_err(|error| format!("script command[{index}] audio request encode failed: {error}"))?;
    match host::call_service("engine.audio", method, &encoded) {
        Ok(_) => Ok(()),
        Err(error) if error.contains("is not registered") => {
            host::warn(
                "newviso.audio",
                format!(
                    "script command[{index}] skipped because engine.audio is unavailable method='{method}'"
                ),
            );
            Ok(())
        }
        Err(error) => Err(format!(
            "script command[{index}] engine.audio method '{method}' failed: {error}"
        )),
    }
}
