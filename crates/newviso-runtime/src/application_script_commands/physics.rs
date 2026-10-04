use super::*;

impl EngineApplication {
    fn script_physics(&mut self, index: usize) -> Result<&mut PhysicsRuntime, String> {
        self.physics.as_mut().ok_or_else(|| format!("script command[{index}] requires engine.physics but no physics capability is active"))
    }

    pub(crate) fn apply_physics_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "physics.world.configure" => {
                self.script_physics(index)?
                    .configure_from_script(command, index)?;
            }
            "physics.body.upsert" => {
                let (resolved, entity) = self.resolve_script_scene_entity_command(
                    command,
                    index,
                    "physics.body.upsert",
                )?;
                self.script_physics(index)?
                    .upsert_body_from_script(&resolved, index)?;
                self.scene.set_physics_process_active(entity, true)?;
            }
            "physics.body.destroy" => {
                let (resolved, entity) = self.resolve_script_scene_entity_command(
                    command,
                    index,
                    "physics.body.destroy",
                )?;
                self.script_physics(index)?
                    .destroy_body_from_script(&resolved, index)?;
                self.scene.set_physics_process_active(entity, false)?;
            }
            "physics.body.velocity.set" => {
                let (resolved, _) = self.resolve_script_scene_entity_command(
                    command,
                    index,
                    "physics.body.velocity.set",
                )?;
                self.script_physics(index)?
                    .set_body_velocity_from_script(&resolved, index)?;
            }
            "physics.body.pose.set" => {
                let (resolved, _) = self.resolve_script_scene_entity_command(
                    command,
                    index,
                    "physics.body.pose.set",
                )?;
                self.script_physics(index)?
                    .set_body_pose_from_script(&resolved, index)?;
            }
            "physics.body.impulse" => {
                let (resolved, _) = self.resolve_script_scene_entity_command(
                    command,
                    index,
                    "physics.body.impulse",
                )?;
                self.script_physics(index)?
                    .apply_impulse_from_script(&resolved, index)?;
            }
            "physics.ballistic.fire" => {
                let source = command
                .get("source")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] physics.ballistic.fire requires unsigned integer 'source'"
                    )
                })?;
                let origin = command_vec3(command, "origin", index)?;
                let direction = command_vec3(command, "direction", index)?;
                let max_distance = command_number(command, "max_distance", index)?;
                let damage = command_number(command, "damage", index)?;
                let impulse = command
                    .get("impulse")
                    .map(|_| command_number(command, "impulse", index))
                    .transpose()?
                    .unwrap_or(0.0);
                let ignore_entity = command
                    .get("ignore_entity")
                    .map(|_| command_u64(command, "ignore_entity", index))
                    .transpose()?;
                let max_hits = command
                .get("max_hits")
                .map(|value| {
                    value
                        .as_u64()
                        .and_then(|value| u16::try_from(value).ok())
                        .filter(|value| *value > 0)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] physics.ballistic.fire 'max_hits' must be in 1..={}",
                                u16::MAX
                            )
                        })
                })
                .transpose()?
                .unwrap_or(1);
                let falloff_min = command
                    .get("falloff_min")
                    .map(|_| command_number(command, "falloff_min", index))
                    .transpose()?
                    .unwrap_or(max_distance);
                let falloff_max = command
                    .get("falloff_max")
                    .map(|_| command_number(command, "falloff_max", index))
                    .transpose()?
                    .unwrap_or(max_distance);
                let falloff_modifier = command
                    .get("falloff_modifier")
                    .map(|_| command_number(command, "falloff_modifier", index))
                    .transpose()?
                    .unwrap_or(1.0);
                // Validate before emitting sound or mutating ped health.
                if max_distance <= 0.0 || damage < 0.0 || impulse < 0.0
                    || falloff_min < 0.0 || falloff_max < falloff_min || !(0.0..=1.0).contains(&falloff_modifier)
                    || direction.iter().map(|v| v * v).sum::<f32>() <= 1.0e-8 {
                    return Err("invalid ballistic shot parameters".into());
                }
                if self.try_ped_ballistic(command, origin, direction, max_distance, damage,
                    ignore_entity, falloff_min, falloff_max, falloff_modifier)? { return Ok(()); }
                self.script_physics(index)?.queue_ballistic_ray(
                    source,
                    origin,
                    direction,
                    max_distance,
                    damage,
                    impulse,
                    ignore_entity,
                    max_hits,
                    falloff_min,
                    falloff_max,
                    falloff_modifier,
                )?;
            }
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
