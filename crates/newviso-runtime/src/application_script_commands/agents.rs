use super::*;

impl EngineApplication {
    pub(super) fn apply_agents_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "character.world_actor.bind" => {
                self.bind_physical_character_from_script(command, index)?;
            }
            "character.world_actor.jump" => {
                let actor_id = command
                .get("actor_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] character.world_actor.jump requires string 'actor_id'"
                    )
                })?;
                let jump_speed = command
                    .get("jump_speed")
                    .map(|_| command_number(command, "jump_speed", index))
                    .transpose()?
                    .unwrap_or(5.0);
                let _ = self.physical_characters.jump(actor_id, jump_speed)?;
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
                    application_agents::apply_agent_commands(&mut self.living_world, vec![cancel])?;
                }
            }
            "agent.remove" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] agent.remove requires string 'id'")
                })?;
                if let Some(cancel) = self.agents.remove_agent(id) {
                    application_agents::apply_agent_commands(&mut self.living_world, vec![cancel])?;
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
            "agent.tasks.clear" => {
                let agent_id = command.get("agent_id").and_then(Value::as_str)
                    .ok_or_else(|| "agent.tasks.clear requires agent_id".to_owned())?;
                if let Some(cancel) = self.agents.clear_tasks(agent_id)? {
                    application_agents::apply_agent_commands(&mut self.living_world, vec![cancel])?;
                }
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
                let task_id = command
                    .get("task_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!(
                            "script command[{index}] agent.task.clear requires string 'task_id'"
                        )
                    })?;
                if let Some(cancel) = self.agents.clear_task(agent_id, task_id)? {
                    application_agents::apply_agent_commands(&mut self.living_world, vec![cancel])?;
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
                    format!("script command[{index}] agent.blackboard.set requires string 'key'")
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
                    format!("script command[{index}] agent.blackboard.remove requires string 'key'")
                })?;
                self.agents.remove_blackboard(agent_id, key)?;
            }
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
