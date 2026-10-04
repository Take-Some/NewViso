use super::*;

impl EngineApplication {
    pub(super) fn apply_effects_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "scene.particle_effect.spawn_at_joint" => {
                let stable_id = self.resolve_script_scene_entity(
                    command,
                    index,
                    "scene.particle_effect.spawn_at_joint",
                )?;
                let joint = command
                .get("joint")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "script command[{index}] scene.particle_effect.spawn_at_joint requires string 'joint'"
                    )
                })?;
                let (position, _) = match self.scene.entity_joint_world_pose(stable_id, joint) {
                    Ok(pose) => pose,
                    Err(error)
                        if error.contains("has no installed model skeleton")
                            || error.contains("is not an installed skinned model") =>
                    {
                        // Weapon/model streaming may materialize one frame after
                        // the script entity. The firing layer will retry naturally.
                        return Ok(());
                    }
                    Err(error) => return Err(error),
                };
                let mut resolved = command.clone();
                resolved["position"] = json!(position);
                resolved["op"] = Value::String("scene.particle_effect.spawn".to_owned());
                let report = application_particle_effects::spawn_particle_effect(
                    &mut self.scene,
                    &resolved,
                    index,
                )?;
                if report.skipped_model > 0 || report.skipped_trail > 0 {
                    host::warn(
                    "newviso.particles",
                    format!(
                        "joint particle effect '{}' source='{}' emitted={} skipped_model={} skipped_trail={} textures={:?}",
                        report.effect,
                        report.source,
                        report.emitted,
                        report.skipped_model,
                        report.skipped_trail,
                        report.textures,
                    ),
                );
                }
            }
            "scene.particle_effect.spawn" => {
                let report = application_particle_effects::spawn_particle_effect(
                    &mut self.scene,
                    command,
                    index,
                )?;
                if report.skipped_model > 0 || report.skipped_trail > 0 {
                    host::warn(
                    "newviso.particles",
                    format!(
                        "particle effect '{}' source='{}' emitted={} skipped_model={} skipped_trail={} textures={:?}; source model geometry must be included by the particle importer",
                        report.effect,
                        report.source,
                        report.emitted,
                        report.skipped_model,
                        report.skipped_trail,
                        report.textures,
                    ),
                );
                } else {
                    host::debug(
                        "newviso.particles",
                        format!(
                            "particle effect '{}' source='{}' emitted={} textures={:?}",
                            report.effect, report.source, report.emitted, report.textures,
                        ),
                    );
                }
            }
            "scene.particles.spawn" => {
                let joint_pose = if let Some(joint) = command
                    .get("joint")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    let stable_id =
                        self.resolve_script_scene_entity(command, index, "scene.particles.spawn")?;
                    match self.scene.entity_joint_world_pose(stable_id, joint) {
                        Ok(pose) => Some(pose),
                        Err(error)
                            if error.contains("has no installed model skeleton")
                                || error.contains("is not an installed skinned model") =>
                        {
                            return Ok(());
                        }
                        Err(error) => return Err(error),
                    }
                } else {
                    None
                };
                let items = command
                    .get("items")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        format!(
                            "script command[{index}] scene.particles.spawn requires array 'items'"
                        )
                    })?;
                let mut particles = Vec::with_capacity(items.len());
                for (item_index, item) in items.iter().enumerate() {
                    let blend = match item
                        .get("blend")
                        .and_then(Value::as_str)
                        .unwrap_or("alpha")
                        .trim()
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "alpha" => SceneParticleBlend::Alpha,
                        "additive" => SceneParticleBlend::Additive,
                        other => {
                            return Err(format!(
                            "script command[{index}] particle[{item_index}] has invalid blend '{other}'"
                        ));
                        }
                    };
                    let position = if item.get("position").is_some() {
                        command_vec3(item, "position", item_index)?
                    } else if let Some((origin, rotation)) = joint_pose {
                        let offset = item
                            .get("position_offset")
                            .map(|_| command_vec3(item, "position_offset", item_index))
                            .transpose()?
                            .unwrap_or([0.0; 3]);
                        let offset = rotate_local_vector_degrees(offset, rotation);
                        [
                            origin[0] + offset[0],
                            origin[1] + offset[1],
                            origin[2] + offset[2],
                        ]
                    } else {
                        return Err(format!(
                        "script command[{index}] particle[{item_index}] requires position or command joint"
                    ));
                    };
                    particles.push(SceneParticleSpawnDesc {
                        position,
                        velocity: item
                            .get("velocity")
                            .map(|_| command_vec3(item, "velocity", item_index))
                            .transpose()?
                            .unwrap_or([0.0; 3]),
                        acceleration: item
                            .get("acceleration")
                            .map(|_| command_vec3(item, "acceleration", item_index))
                            .transpose()?
                            .unwrap_or([0.0; 3]),
                        size: command_vector::<2>(item, "size", item_index)?,
                        end_size: item
                            .get("end_size")
                            .map(|_| command_vector::<2>(item, "end_size", item_index))
                            .transpose()?
                            .unwrap_or(command_vector::<2>(item, "size", item_index)?),
                        color: command_vec4(item, "color", item_index)?,
                        end_color: item
                            .get("end_color")
                            .map(|_| command_vec4(item, "end_color", item_index))
                            .transpose()?
                            .unwrap_or(command_vec4(item, "color", item_index)?),
                        lifetime_seconds: command_number(item, "lifetime_seconds", item_index)?,
                        rotation_degrees: item
                            .get("rotation_degrees")
                            .map(|_| command_number(item, "rotation_degrees", item_index))
                            .transpose()?
                            .unwrap_or(0.0),
                        angular_velocity_degrees: item
                            .get("angular_velocity_degrees")
                            .map(|_| command_number(item, "angular_velocity_degrees", item_index))
                            .transpose()?
                            .unwrap_or(0.0),
                        blend,
                        style: None,
                    });
                }
                self.scene.spawn_particles(particles)?;
            }
            "scene.particles.clear" => {
                self.scene.clear_particles();
            }
            "scene.surface_mark.add" => {
                let position = command_vec3(command, "position", index)?;
                let normal = command_vec3(command, "normal", index)?;
                let radius = command_number(command, "radius", index)?;
                let color = command
                    .get("color")
                    .map(|_| command_vec4(command, "color", index))
                    .transpose()?
                    .unwrap_or([0.025, 0.022, 0.018, 1.0]);
                self.scene.add_surface_mark(SceneSurfaceMark {
                    position,
                    normal,
                    radius,
                    color,
                })?;
            }
            "scene.surface_marks.clear" => {
                self.scene.clear_surface_marks();
            }
            "scene.transient_spheres.set" => {
                let items = command
                    .get("items")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        format!(
                        "script command[{index}] scene.transient_spheres.set requires array 'items'"
                    )
                    })?;
                let mut spheres = Vec::with_capacity(items.len());
                for (item_index, item) in items.iter().enumerate() {
                    spheres.push(SceneTransientSphere {
                        position: command_vec3(item, "position", item_index)?,
                        rotation_degrees: item
                            .get("rotation_degrees")
                            .map(|_| command_vec3(item, "rotation_degrees", item_index))
                            .transpose()?
                            .unwrap_or([0.0; 3]),
                        radius: command_number(item, "radius", item_index)?,
                        color: command_vec4(item, "color", item_index)?,
                        marker_color: item
                            .get("marker_color")
                            .map(|_| command_vec4(item, "marker_color", item_index))
                            .transpose()?,
                        marker_direction: if item.get("marker_color").is_some() {
                            command_vec3(item, "marker_direction", item_index)?
                        } else {
                            [0.0, 0.0, 1.0]
                        },
                        marker_threshold: if item.get("marker_color").is_some() {
                            command_number(item, "marker_threshold", item_index)?
                        } else {
                            1.0
                        },
                    });
                }
                self.scene.set_transient_spheres(spheres)?;
            }
            "scene.overlay_quads.set" => {
                let items = command
                    .get("items")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        format!(
                        "script command[{index}] scene.overlay_quads.set requires array 'items'"
                    )
                    })?;
                let mut quads = Vec::with_capacity(items.len());
                for (item_index, item) in items.iter().enumerate() {
                    quads.push(SceneOverlayQuad {
                        rect: command_vec4(item, "rect", item_index)?,
                        color: command_vec4(item, "color", item_index)?,
                    });
                }
                self.scene.set_overlay_quads(quads)?;
            }
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
