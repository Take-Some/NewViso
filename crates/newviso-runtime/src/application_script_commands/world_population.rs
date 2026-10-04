use super::*;

impl EngineApplication {
    pub(super) fn apply_world_population_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
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
                    format!("script command[{index}] world.model_set.upsert requires string 'id'")
                })?;
                let category =
                    command
                        .get("category")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                        "script command[{index}] world.model_set.upsert requires string 'category'"
                    )
                        })?;
                let assets = command_strings(command, "assets", index)?;
                let weights = if let Some(values) = command.get("weights").and_then(Value::as_array)
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
                    format!("script command[{index}] world.model_set.remove requires string 'id'")
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
                let relation =
                    command
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
                    format!("script command[{index}] world.stimulus.emit requires string 'kind'")
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
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
