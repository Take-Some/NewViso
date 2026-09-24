use super::*;

impl EngineApplication {
    pub(super) fn apply_script_commands(&mut self, commands: &[Value]) -> Result<(), String> {
        for (index, command) in commands.iter().enumerate() {
            let op = command
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("script command[{index}] has no string 'op'"))?;

            match op {
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
                    self.physics
                        .as_mut()
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] requires engine.physics but no physics capability is active"
                            )
                        })?
                        .upsert_body_from_script(command, index)?;
                }
                "physics.body.destroy" => {
                    let entity = command
                        .get("entity")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] physics.body.destroy requires unsigned integer 'entity'"
                            )
                        })?;
                    self.physics
                        .as_mut()
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] requires engine.physics but no physics capability is active"
                            )
                        })?
                        .destroy_body_from_script(command, index)?;
                    self.scene.set_physics_process_active(entity, false)?;
                }
                "physics.body.impulse" => {
                    self.physics
                        .as_mut()
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] requires engine.physics but no physics capability is active"
                            )
                        })?
                        .apply_impulse_from_script(command, index)?;
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
                    self.scene.configure_orbit(
                        command_number(command, "rotate_sensitivity", index)?,
                        command_number(command, "zoom_sensitivity", index)?,
                        command_number(command, "min_distance", index)?,
                        command_number(command, "max_distance", index)?,
                    )?;
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
                    self.scene.set_weather_backend(current, next, blend)?;
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
                "scene.entity.visibility.set" => {
                    let entity = command
                        .get("entity")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.visibility.set requires unsigned integer 'entity'"
                            )
                        })?;
                    let channel = command
                        .get("channel")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.visibility.set requires string 'channel'"
                            )
                        })?;
                    let visible = command
                        .get("visible")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.entity.visibility.set requires boolean 'visible'"
                            )
                        })?;
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
                    })?;
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
                        self.scene.remove_runtime_entity(id)?;
                    }
                }
                "scene.camera.set" => {
                    let position = command_vec3(command, "position", index)?;
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
