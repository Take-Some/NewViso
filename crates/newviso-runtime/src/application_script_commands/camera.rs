use super::audio::sync_audio_listener_if_available;
use super::*;

impl EngineApplication {
    fn constrain_script_camera(
        &self, command: &Value, position: [f32; 3], index: usize,
    ) -> Result<[f32; 3], String> {
        let Some(collision) = command.get("collision") else { return Ok(position); };
        let origin = command_vec3(collision, "origin", index)?;
        let radius = command_number(collision, "radius", index)?;
        if !(0.01..=2.0).contains(&radius) {
            return Err(format!("script command[{index}] camera collision radius must be in 0.01..=2"));
        }
        let ignore = collision.get("ignore_entity")
            .map(|_| command_u64(collision, "ignore_entity", index)).transpose()?;
        let min = std::array::from_fn(|i| origin[i].min(position[i]) - radius);
        let max = std::array::from_fn(|i| origin[i].max(position[i]) + radius);
        let solids = self.scene.physics_static_solid_aabbs_near(&[(min, max)]);
        let physics = self.physics.as_ref().ok_or_else(||
            format!("script command[{index}] camera collision requires physics"))?;
        Ok(physics.constrain_camera(origin, position, radius, ignore, &solids))
    }

    pub(super) fn apply_camera_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "scene.camera.set_relative_to_entity" => {
                if command
                    .get("late_update")
                    .is_some_and(|value| !value.is_boolean())
                {
                    return Err(format!(
                        "script command[{index}] camera 'late_update' must be a boolean"
                    ));
                }
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
                let stable_id = self.scene.runtime_entity_stable_id(entity).ok_or_else(|| {
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
                let mut anchor_position = parent_position;
                let fallback_world = command.get("fallback_world_position")
                    .map(|_| command_vec3(command, "fallback_world_position", index)).transpose()?;
                let mut joint_available = false;
                let mut local_position = command_vec3(command, "local_position", index)?;
                if let Some(joint) = command.get("joint") {
                    let joint = joint
                        .as_str()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] camera 'joint' must be a non-empty string"
                            )
                        })?;
                    match self.scene.entity_joint_world_position(stable_id, joint) {
                        Ok(position) => { anchor_position = position; joint_available = true; },
                        Err(error)
                            if error.contains("has no installed model skeleton")
                                || error.contains("is not an installed skinned model") =>
                        {
                            // The controller can request a head anchor before the
                            // streamed character/animation is materialized.
                            if let Some(position) = fallback_world {
                                anchor_position = position;
                            } else {
                                local_position = command_vec3(command, "joint_fallback_position", index)?;
                            }
                        }
                        Err(error) => {
                            return Err(format!("script command[{index}] camera joint: {error}"));
                        }
                    }
                }
                if joint_available {
                    if let Some(fallback) = fallback_world {
                        let blend = command.get("anchor_blend")
                            .map(|_| command_number(command, "anchor_blend", index)).transpose()?.unwrap_or(1.0);
                        if !(0.0..=1.0).contains(&blend) { return Err("camera anchor_blend must be in 0..=1".to_owned()); }
                        anchor_position = std::array::from_fn(|i| fallback[i] + (anchor_position[i] - fallback[i]) * blend);
                    }
                }
                let local_forward = command_vec3(command, "local_forward", index)?;
                let local_up = command
                    .get("local_up")
                    .map(|_| command_vec3(command, "local_up", index))
                    .transpose()?
                    .unwrap_or([0.0, 1.0, 0.0]);
                let position_offset = rotate_local_vector_degrees(local_position, parent_rotation);
                let forward = command.get("world_forward")
                    .map(|_| command_vec3(command, "world_forward", index)).transpose()?
                    .unwrap_or_else(|| rotate_local_vector_degrees(local_forward, parent_rotation));
                let up = command.get("world_up")
                    .map(|_| command_vec3(command, "world_up", index)).transpose()?
                    .unwrap_or_else(|| rotate_local_vector_degrees(local_up, parent_rotation));
                let world_offset = command.get("world_position_offset")
                    .map(|_| command_vec3(command, "world_position_offset", index)).transpose()?.unwrap_or([0.0; 3]);
                let position = std::array::from_fn(|i| anchor_position[i] + position_offset[i] + world_offset[i]);
                let position = self.constrain_script_camera(command, position, index)?;
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
                if command.get("near_clip").is_some() {
                    self.scene.set_camera_near_clip(command_number(command, "near_clip", index)?)?;
                }
                self.scene
                    .set_camera_pose(position, target, Some(up), fov)?;
                let listener = json!({
                    "position": position,
                    "forward": forward,
                    "up": up,
                    "velocity": [0.0, 0.0, 0.0]
                });
                sync_audio_listener_if_available(index, &listener)?;
            }
            "scene.camera.set" => {
                let position = self.constrain_script_camera(command, command_vec3(command, "position", index)?, index)?;
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
                if command.get("near_clip").is_some() {
                    self.scene.set_camera_near_clip(command_number(command, "near_clip", index)?)?;
                }
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
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}

