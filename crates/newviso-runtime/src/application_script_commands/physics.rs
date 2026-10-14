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
            "physics.water.configure" => {
                self.script_physics(index)?.configure_water_from_script(command,index)?;
                let surface = self.physics.as_ref().and_then(|p|p.water_surface());
                self.scene.configure_water_surface(surface)?;
            }
            "physics.body.water.configure" => {
                let (resolved, _) = self.resolve_script_scene_entity_command(
                    command,index,"physics.body.water.configure")?;
                self.script_physics(index)?.configure_body_water_from_script(&resolved,index)?;
            }
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
                let mut origin = command_vec3(command, "origin", index)?;
                let mut direction = command_vec3(command, "direction", index)?;
                if let Some(aim) = command.get("aim_camera") {
                    let mode = aim.get("mode").and_then(Value::as_str).ok_or_else(|| "ballistic aim_camera requires mode".to_owned())?;
                    if mode != "first_person" && mode != "third_person" { return Err("invalid ballistic camera mode".to_owned()); }
                    let (camera_position, camera_forward, _) = self.scene.camera_pose_values();
                    let mut muzzle = origin;
                    if mode == "third_person" {
                        if let (Some(entity), Some(joint)) = (aim.get("muzzle_entity").and_then(Value::as_str), aim.get("muzzle_joint").and_then(Value::as_str)) {
                            if let Some(id) = self.scene.runtime_entity_stable_id(entity) {
                                if let Ok((position, rotation)) = self.scene.entity_joint_world_pose(id, joint) {
                                    let offset = aim.get("muzzle_offset").map(|_| command_vec3(aim, "muzzle_offset", index)).transpose()?.unwrap_or([0.0; 3]);
                                    let offset = rotate_local_vector_degrees(offset, rotation);
                                    muzzle = std::array::from_fn(|i| position[i] + offset[i]);
                                }
                            }
                        }
                    }
                    origin = camera_shot_origin(camera_position, camera_forward, muzzle, mode == "first_person");
                    if aim.get("spread_offset").is_some() {
                        let spread = command_vec3(aim, "spread_offset", index)?;
                        direction = camera_spread_direction(camera_forward, spread);
                    } else {
                        let spread = aim.get("spread_angles").map(|_| command_vec3(aim, "spread_angles", index)).transpose()?.unwrap_or([0.0; 3]);
                        let yaw = camera_forward[0].atan2(-camera_forward[2]) + spread[0];
                        let pitch = camera_forward[1].clamp(-1.0, 1.0).asin() + spread[1];
                        direction = [yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos()];
                    }
                }
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
                if max_distance <= 0.0
                    || damage < 0.0
                    || impulse < 0.0
                    || falloff_min < 0.0
                    || falloff_max < falloff_min
                    || !(0.0..=1.0).contains(&falloff_modifier)
                    || direction.iter().map(|v| v * v).sum::<f32>() <= 1.0e-8
                {
                    return Err("invalid ballistic shot parameters".into());
                }
                self.emit_ped_gunshot(command, origin)?;
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

/// Camera origin in FP; camera ray projected to muzzle depth minus 15 cm in TP.
/// This reproduces CalcFireVecFromAimCamera without using the render gun axis.
/// Spread is an offset divided by range, not an angle in degrees.
fn camera_spread_direction(forward: [f32; 3], spread: [f32; 3]) -> [f32; 3] {
    let horizontal = forward[0].hypot(forward[2]);
    let right = if horizontal > 1.0e-6 { [-forward[2] / horizontal, 0.0, forward[0] / horizontal] } else { [1.0, 0.0, 0.0] };
    let up = [-right[2] * forward[1], right[2] * forward[0] - right[0] * forward[2], right[0] * forward[1]];
    let result: [f32; 3] = std::array::from_fn(|i| forward[i] + right[i] * spread[0] + up[i] * spread[1]);
    let length = result.iter().map(|v| v * v).sum::<f32>().sqrt();
    result.map(|v| v / length.max(1.0e-6))
}

fn camera_shot_origin(camera: [f32; 3], forward: [f32; 3], muzzle: [f32; 3], first_person: bool) -> [f32; 3] {
    if first_person { return camera; }
    let depth = (0..3).map(|i| forward[i] * (muzzle[i] - camera[i])).sum::<f32>() - 0.15;
    std::array::from_fn(|i| camera[i] + depth * forward[i])
}

#[cfg(test)]
mod camera_shot_tests {
    use super::{camera_shot_origin, camera_spread_direction};

    #[test]
    fn spread_uses_a_cone_in_world_up_basis() {
        let direction = camera_spread_direction([0.0,0.0,-1.0], [0.01,0.02,0.0]);
        assert!(direction[0] > 0.0 && direction[1] > 0.0);
        assert!((direction[0] / -direction[2] - 0.01).abs() < 1e-6);
        assert!((direction[1] / -direction[2] - 0.02).abs() < 1e-6);
    }
    #[test]
    fn first_person_keeps_eye_origin_to_hit_near_walls() {
        assert_eq!(camera_shot_origin([1.0, 2.0, 3.0], [0.0, 0.0, -1.0], [1.0, 2.0, 1.0], true), [1.0, 2.0, 3.0]);
    }
    #[test]
    fn third_person_preserves_sight_line_and_advances_to_muzzle_depth() {
        let p = camera_shot_origin([0.0, 2.0, 5.0], [0.0, 0.0, -1.0], [0.4, 1.7, -0.5], false);
        assert!((p[2] + 0.35).abs() < 1.0e-5);
        assert_eq!(p[0], 0.0);
        assert_eq!(p[1], 2.0);
    }
}
