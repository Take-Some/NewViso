use super::*;

impl EngineApplication {
    pub(super) fn apply_entity_transform_batch(&mut self, command: &Value, index: usize) -> Result<(), String> {
        let entries = command.get("transforms").and_then(Value::as_array)
            .ok_or_else(|| format!("script command[{index}] scene.entities.transform.batch requires array 'transforms'"))?;
        for (batch_index, entry) in entries.iter().enumerate() {
            let id = entry.get("id").and_then(Value::as_str).filter(|id| !id.is_empty())
                .ok_or_else(|| format!("script command[{index}] transform[{batch_index}] requires string 'id'"))?;
            let position = entry.get("position").map(|_| command_vec3(entry, "position", batch_index)).transpose()?;
            let rotation = entry.get("rotation_degrees").map(|_| command_vec3(entry, "rotation_degrees", batch_index)).transpose()?;
            let scale = entry.get("scale").map(|_| command_vec3(entry, "scale", batch_index)).transpose()?;
            self.scene.set_runtime_entity_transform(id, position, rotation, scale)
                .map_err(|e| format!("script command[{index}] transform[{batch_index}]: {e}"))?;
        }
        Ok(())
    }
    pub(super) fn apply_entities_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "scene.render.configure" => {
                let patch = command.get("settings").ok_or_else(|| {
                    format!("script command[{index}] scene.render.configure requires settings")
                })?;
                self.scene.configure_render_policy(patch)?;
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
                self.scene
                    .set_entity_process_claim(entity, "project.script", reason, active)?;
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
                    format!("script command[{index}] mass-instance debug grid count exceeds u32")
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
                    format!("script command[{index}] scene.mass_instance.upsert slot exceeds u32")
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
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
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
                    ));
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
                let damage_kind = application_physics::PhysicsDamageKind::parse(
                    command.get("damage_kind").and_then(Value::as_str),
                )
                .map_err(|error| format!("script command[{index}] scene.entity.damage {error}"))?;

                if self.vehicles.contains(stable_id) {
                    self.apply_vehicle_damage_transaction(
                        application_physics::PhysicsDamageContact {
                            source,
                            target: stable_id,
                            damage_kind,
                            direct_damage,
                            contact_impulse,
                            point,
                            impulse_direction,
                        },
                        command
                            .get("point")
                            .is_none()
                            .then_some(newviso_vehicle::VehicleDamageComponent::Body),
                    )?;
                    return Ok(());
                }
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
                            damage_kind,
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
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
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
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
