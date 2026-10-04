use super::*;

impl EngineApplication {
    pub(super) fn apply_world_simulation_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
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
                    format!("script command[{index}] world.observer.upsert requires string 'id'")
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
                    format!("script command[{index}] world.observer.remove requires string 'id'")
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
                    .map(|value| {
                        value.as_bool().ok_or_else(|| {
                    format!("script command[{index}] world.actor.upsert 'enabled' must be boolean")
                })
                    })
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
                    ));
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
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
