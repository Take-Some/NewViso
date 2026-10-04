use super::*;

impl EngineApplication {
    pub(super) fn apply_animation_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "scene.entity.main_view_meshes.set" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] main_view_meshes.set requires string 'id'")
                })?;
                let prefixes =
                    command
                        .get("hidden_mesh_prefixes")
                        .and_then(Value::as_array)
                        .ok_or_else(|| {
                            format!("script command[{index}] requires 'hidden_mesh_prefixes' array")
                        })?
                        .iter()
                        .map(|value| {
                            value.as_str().map(str::to_owned).ok_or_else(|| {
                    format!("script command[{index}] hidden_mesh_prefixes must contain strings")
                })
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                let stable_id = self.scene.runtime_entity_stable_id(id).ok_or_else(|| {
                    format!("script command[{index}] mesh visibility target '{id}' does not exist")
                })?;
                let roots = match command.get("hidden_joint_roots") {
                None => Vec::new(),
                Some(value) => value.as_array()
                    .ok_or_else(|| format!("script command[{index}] hidden_joint_roots must be an array"))?
                    .iter().map(|value| value.as_str().map(str::to_owned)
                        .ok_or_else(|| format!("script command[{index}] hidden_joint_roots must contain strings")))
                    .collect::<Result<Vec<_>, String>>()?,
            };
                self.scene
                    .set_entity_main_view_hidden_geometry(stable_id, prefixes, roots)?;
            }
            "scene.entity.joint_mesh.copy" => {
                let text = |field: &str| {
                    command.get(field).and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] joint_mesh.copy requires '{field}'")
                    })
                };
                let id = text("id")?;
                let source = text("source")?;
                let target_id = self
                    .scene
                    .runtime_entity_stable_id(id)
                    .ok_or_else(|| format!("joint mesh target '{id}' does not exist"))?;
                let source_id = self
                    .scene
                    .runtime_entity_stable_id(source)
                    .ok_or_else(|| format!("joint mesh source '{source}' does not exist"))?;
                let roots = command
                    .get("joint_roots")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        format!("script command[{index}] joint_mesh.copy requires joint_roots")
                    })?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| "joint_roots must contain strings".to_owned())
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                self.scene.copy_entity_joint_mesh(
                    source_id,
                    target_id,
                    &roots,
                    text("root_joint")?,
                    text("tip_joint")?,
                    command_vec3(command, "direction", index)?,
                    command_vec3(command, "up", index)?,
                )?;
            }
            "scene.entity.arm_ik.set" => {
                let stable_id = self.resolve_script_scene_entity(command, index, op)?;
                let arms = command
                    .get("arms")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        format!("script command[{index}] arm IK requires 'arms' array")
                    })?;
                let mut constraints = Vec::with_capacity(arms.len());
                for arm in arms {
                    let name = |key: &str| -> Result<String, String> {
                        arm.get(key)
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .ok_or_else(|| {
                                format!("script command[{index}] arm IK requires '{key}'")
                            })
                    };
                    constraints.push(newviso_scene::SceneArmIkConstraint {
                        upper_joint: name("upper_joint")?,
                        lower_joint: name("lower_joint")?,
                        hand_joint: name("hand_joint")?,
                        tip_joint: name("tip_joint")?,
                        target: command_vec3(arm, "target", index)?,
                        pole: command_vec3(arm, "pole", index)?,
                        rotation_degrees: command_vec3(arm, "rotation_degrees", index)?,
                        weight: command_number(arm, "weight", index)?,
                        hand_pose_weight: arm
                            .get("hand_pose_weight")
                            .map(|_| command_number(arm, "hand_pose_weight", index))
                            .transpose()?
                            .unwrap_or(command_number(arm, "weight", index)?),
                        joint_rotations: arm
                            .get("joint_rotations")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .map(|joint| {
                                let name = joint.get("joint").and_then(Value::as_str).ok_or_else(
                                    || {
                                        format!(
                                            "script command[{index}] hand pose requires joint name"
                                        )
                                    },
                                )?;
                                Ok((
                                    name.to_owned(),
                                    command_vec3(joint, "rotation_degrees", index)?,
                                ))
                            })
                            .collect::<Result<Vec<_>, String>>()?,
                    });
                }
                self.scene.set_entity_arm_ik(stable_id, constraints)?;
            }
            "scene.entity.animation.play" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.entity.animation.play requires string 'id'"
                    )
                })?;
                let clip_ref = command
                .get("clip_ref")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.entity.animation.play requires string 'clip_ref'"
                    )
                })?;
                let playback_rate = command
                    .get("playback_rate")
                    .map(|_| command_number(command, "playback_rate", index))
                    .transpose()?
                    .unwrap_or(1.0);
                let restart_if_same = command
                    .get("restart_if_same")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let start_time_seconds = command
                    .get("start_time_seconds")
                    .map(|_| command_number(command, "start_time_seconds", index))
                    .transpose()?
                    .unwrap_or(0.0);
                let blend_seconds = command
                    .get("blend_seconds")
                    .map(|_| command_number(command, "blend_seconds", index))
                    .transpose()?
                    .unwrap_or(0.18);
                let apply_mover = command
                    .get("apply_mover")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let stable_id = self.scene.runtime_entity_stable_id(id).ok_or_else(|| {
                    format!(
                        "script command[{index}] animation target '{}' does not exist",
                        id
                    )
                })?;
                let _ = self.bind_scene_entity_animation(
                    stable_id,
                    SceneAnimationBinding {
                        clip_ref: clip_ref.to_owned(),
                        playback_rate,
                        restart_if_same,
                        start_time_seconds,
                        blend_seconds,
                        bound_elapsed_seconds: 0.0,
                        apply_mover,
                    },
                )?;
            }
            "scene.entity.attach_to_joint" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.entity.attach_to_joint requires string 'id'"
                    )
                })?;
                let parent = command
                .get("parent")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.entity.attach_to_joint requires string 'parent'"
                    )
                })?;
                let joint = command
                .get("joint")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.entity.attach_to_joint requires string 'joint'"
                    )
                })?;
                let parent_stable_id =
                    self.scene.runtime_entity_stable_id(parent).ok_or_else(|| {
                        format!(
                            "script command[{index}] joint attachment parent '{}' does not exist",
                            parent
                        )
                    })?;
                if self.scene.runtime_entity_stable_id(id).is_none() {
                    return Err(format!(
                        "script command[{index}] joint attachment child '{}' does not exist",
                        id
                    ));
                }
                let (joint_position, joint_rotation) =
                    match self.scene.entity_joint_world_pose(parent_stable_id, joint) {
                        Ok(pose) => pose,
                        Err(error) if error.contains("not an installed skinned model") => {
                            // Streaming may materialize the child before the parent skin is
                            // installed. Attachment is retried by the project every frame.
                            return Ok(());
                        }
                        Err(error) => return Err(error),
                    };
                let offset = command
                    .get("position_offset")
                    .map(|_| command_vec3(command, "position_offset", index))
                    .transpose()?
                    .unwrap_or([0.0; 3]);
                let rotated_offset = rotate_local_vector_degrees(offset, joint_rotation);
                let rotation_offset = command
                    .get("rotation_offset_degrees")
                    .map(|_| command_vec3(command, "rotation_offset_degrees", index))
                    .transpose()?
                    .unwrap_or([0.0; 3]);
                self.scene.set_runtime_entity_transform(
                    id,
                    Some([
                        joint_position[0] + rotated_offset[0],
                        joint_position[1] + rotated_offset[1],
                        joint_position[2] + rotated_offset[2],
                    ]),
                    Some([
                        joint_rotation[0] + rotation_offset[0],
                        joint_rotation[1] + rotation_offset[1],
                        joint_rotation[2] + rotation_offset[2],
                    ]),
                    None,
                )?;
            }
            "scene.entity.attach_joint_to_joint" => {
                let id = command
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.entity.attach_joint_to_joint requires string 'id'"
                    )
                })?;
                let parent = command
                .get("parent")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.entity.attach_joint_to_joint requires string 'parent'"
                    )
                })?;
                let parent_joint = command
                .get("parent_joint")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.entity.attach_joint_to_joint requires string 'parent_joint'"
                    )
                })?;
                let child_joint = command
                .get("child_joint")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.entity.attach_joint_to_joint requires string 'child_joint'"
                    )
                })?;
                let Some(parent_stable_id) = self.scene.runtime_entity_stable_id(parent) else {
                    return Ok(());
                };
                let Some(child_stable_id) = self.scene.runtime_entity_stable_id(id) else {
                    return Ok(());
                };
                self.scene.set_entity_joint_attachment_offset(
                    child_stable_id,
                    parent_stable_id,
                    parent_joint,
                    child_joint,
                    command
                        .get("position_offset")
                        .map(|_| command_vec3(command, "position_offset", index))
                        .transpose()?
                        .unwrap_or([0.0; 3]),
                    command
                        .get("rotation_offset_degrees")
                        .map(|_| command_vec3(command, "rotation_offset_degrees", index))
                        .transpose()?
                        .unwrap_or([0.0; 3]),
                )?;
            }
            "scene.entity.animation.stop" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.entity.animation.stop requires string 'id'"
                    )
                })?;
                if let Some(stable_id) = self.scene.runtime_entity_stable_id(id) {
                    let _ = self.unbind_scene_entity_animation(stable_id)?;
                }
            }
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
