use super::*;

impl EngineApplication {
    pub(super) fn apply_world_events_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
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
                    format!("script command[{index}] world.event.schedule requires string 'kind'")
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
                    format!("script command[{index}] world.reality.record requires string 'kind'")
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
                let actor_id =
                    command
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
                    format!("script command[{index}] world.scenario.release requires string 'id'")
                })?;
                self.living_world.release_scenario_reservation(id);
            }
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
