use super::*;

impl EngineApplication {
    pub(super) fn apply_world_navigation_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
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
                let actor_id =
                    command
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
                let actor_id =
                    command
                        .get("actor_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                        "script command[{index}] world.travel.cancel requires string 'actor_id'"
                    )
                        })?;
                self.living_world.cancel_travel(actor_id);
            }
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
