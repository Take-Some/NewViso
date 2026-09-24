use super::*;

impl EngineApplication {
    pub(super) fn apply_script_commands(&mut self, commands: &[Value]) -> Result<(), String> {
        for (index, command) in commands.iter().enumerate() {
            let op = command
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("script command[{index}] has no string 'op'"))?;

            match op {
                "events.emit" => {
                    let topic = command
                        .get("topic")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!("script command[{index}] events.emit requires string 'topic'")
                        })?;
                    let source = command
                        .get("source")
                        .and_then(Value::as_str)
                        .unwrap_or("game.script");
                    let payload = command.get("payload").cloned().unwrap_or(Value::Null);
                    let cancelable = command
                        .get("cancelable")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let phase = match command
                        .get("phase")
                        .and_then(Value::as_str)
                        .unwrap_or("observe")
                    {
                        "before" => EventPhase::Before,
                        "after" => EventPhase::After,
                        "observe" => EventPhase::Observe,
                        other => {
                            return Err(format!(
                                "script command[{index}] events.emit has invalid phase '{other}'"
                            ))
                        }
                    };

                    let mut metadata = command
                        .get("metadata")
                        .and_then(Value::as_object)
                        .map(|values| {
                            values
                                .iter()
                                .map(|(key, value)| (key.clone(), value.clone()))
                                .collect::<BTreeMap<_, _>>()
                        })
                        .unwrap_or_default();
                    metadata
                        .entry("script_echo".to_owned())
                        .or_insert(Value::Bool(false));

                    host::publish_event_json_with(
                        topic, source, payload, phase, cancelable, metadata,
                    )?;
                }
                "platform.cursor.set" => {
                    let captured = command
                        .get("captured")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] platform.cursor.set requires 'captured'"
                            )
                        })?;
                    let previous = self.cursor_captured;
                    self.cursor_captured = captured && self.window_focused;
                    if previous != self.cursor_captured {
                        host::publish_event_json(
                            event_topic::CURSOR_CAPTURE_CHANGED,
                            "newviso.runtime",
                            json!({
                                "captured": self.cursor_captured,
                                "previous": previous
                            }),
                        )?;
                    }
                }
                "physics.body.upsert" => {
                    self.physics
                        .as_mut()
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] requires engine.physics but no physics capability is active"
                            )
                        })?
                        .upsert_body_from_script(command, index)?;
                }
                "physics.body.destroy" => {
                    self.physics
                        .as_mut()
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] requires engine.physics but no physics capability is active"
                            )
                        })?
                        .destroy_body_from_script(command, index)?;
                }
                "physics.body.impulse" => {
                    self.physics
                        .as_mut()
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] requires engine.physics but no physics capability is active"
                            )
                        })?
                        .apply_impulse_from_script(command, index)?;
                }
                "scene.light.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] scene.light.upsert requires string 'id'")
                    })?;
                    let light_type = match command
                        .get("light_type")
                        .or_else(|| command.get("type"))
                        .and_then(Value::as_str)
                        .unwrap_or("point")
                        .trim()
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "directional" => SceneLightType::Directional,
                        "point" => SceneLightType::Point,
                        "spot" => SceneLightType::Spot,
                        "area" => SceneLightType::Area,
                        other => {
                            return Err(format!(
                                "script command[{index}] unknown light_type '{other}'"
                            ))
                        }
                    };
                    let mut desc = SceneLightDesc::default();
                    desc.light_type = light_type;
                    if command.get("color").is_some() {
                        desc.color = command_vec3(command, "color", index)?;
                    }
                    if command.get("intensity").is_some() {
                        desc.intensity = command_number(command, "intensity", index)?;
                    }
                    if command.get("range").is_some() {
                        desc.range = command_number(command, "range", index)?;
                    }
                    if command.get("cone_inner_degrees").is_some() {
                        desc.cone_inner_degrees =
                            command_number(command, "cone_inner_degrees", index)?;
                    }
                    if command.get("cone_outer_degrees").is_some() {
                        desc.cone_outer_degrees =
                            command_number(command, "cone_outer_degrees", index)?;
                    }
                    if let Some(value) = command.get("casts_shadows") {
                        desc.casts_shadows = value.as_bool().ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.light.upsert 'casts_shadows' must be boolean"
                            )
                        })?;
                    }
                    if command.get("shadow_bias").is_some() {
                        desc.shadow_bias = command_number(command, "shadow_bias", index)?;
                    }
                    if command.get("shadow_normal_bias").is_some() {
                        desc.shadow_normal_bias =
                            command_number(command, "shadow_normal_bias", index)?;
                    }
                    if let Some(value) = command.get("shadow_resolution") {
                        let resolution = value.as_u64().ok_or_else(|| {
                            format!(
                                "script command[{index}] scene.light.upsert 'shadow_resolution' must be unsigned integer"
                            )
                        })?;
                        desc.shadow_resolution = u32::try_from(resolution).map_err(|_| {
                            format!(
                                "script command[{index}] scene.light.upsert shadow_resolution out of range"
                            )
                        })?;
                    }
                    if command.get("shadow_distance").is_some() {
                        desc.shadow_distance = command_number(command, "shadow_distance", index)?;
                    }
                    self.scene.upsert_runtime_light(id, desc)?;
                }
                "scene.sky_visual.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] scene.sky_visual.upsert requires string 'id'"
                        )
                    })?;
                    let kind = match command
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("disc")
                        .trim()
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "disc" => SkyVisualKind::Disc,
                        "billboard" => SkyVisualKind::Billboard,
                        other => {
                            return Err(format!(
                                "script command[{index}] unknown sky visual kind '{other}'"
                            ))
                        }
                    };
                    let mut desc = SkyVisualDesc::default();
                    desc.kind = kind;
                    if command.get("color").is_some() {
                        desc.color = command_vec3(command, "color", index)?;
                    }
                    if command.get("intensity").is_some() {
                        desc.intensity = command_number(command, "intensity", index)?;
                    }
                    if command.get("angular_size_degrees").is_some() {
                        desc.angular_size_degrees =
                            command_number(command, "angular_size_degrees", index)?;
                    }
                    if command.get("halo_size_degrees").is_some() {
                        desc.halo_size_degrees =
                            command_number(command, "halo_size_degrees", index)?;
                    }
                    if command.get("halo_intensity").is_some() {
                        desc.halo_intensity = command_number(command, "halo_intensity", index)?;
                    }
                    self.scene.upsert_runtime_sky_visual(id, desc)?;
                }
                "scene.sky_visual.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] scene.sky_visual.remove requires string 'id'"
                        )
                    })?;
                    self.scene.remove_runtime_sky_visual(id)?;
                }
                "scene.lens_flare.upsert" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] scene.lens_flare.upsert requires string 'id'"
                        )
                    })?;
                    let source = command.get("source").and_then(Value::as_str).ok_or_else(|| {
                        format!("script command[{index}] scene.lens_flare.upsert requires string 'source'")
                    })?;
                    let enabled = command
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let intensity = if command.get("intensity").is_some() {
                        command_number(command, "intensity", index)?
                    } else {
                        1.0
                    };
                    let scale = if command.get("scale").is_some() {
                        command_number(command, "scale", index)?
                    } else {
                        1.0
                    };
                    let occlusion_test = command
                        .get("occlusion_test")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let elements_value = command
                        .get("elements")
                        .and_then(Value::as_array)
                        .ok_or_else(|| {
                            format!("script command[{index}] scene.lens_flare.upsert requires array 'elements'")
                        })?;
                    let mut elements = Vec::with_capacity(elements_value.len());
                    for (element_index, element) in elements_value.iter().enumerate() {
                        let kind = match element
                            .get("kind")
                            .and_then(Value::as_str)
                            .unwrap_or("ghost")
                            .trim()
                            .to_ascii_lowercase()
                            .as_str()
                        {
                            "halo" => LensFlareElementKind::Halo,
                            "ghost" => LensFlareElementKind::Ghost,
                            "streak" => LensFlareElementKind::Streak,
                            other => {
                                return Err(format!(
                                    "script command[{index}] flare element[{element_index}] unknown kind '{other}'"
                                ))
                            }
                        };
                        elements.push(LensFlareElementDesc {
                            kind,
                            offset: command_number(element, "offset", element_index)?,
                            size: command_number(element, "size", element_index)?,
                            color: command_vec3(element, "color", element_index)?,
                            alpha: command_number(element, "alpha", element_index)?,
                        });
                    }
                    self.scene.upsert_lens_flare(
                        id,
                        LensFlareDesc {
                            source: source.to_owned(),
                            enabled,
                            intensity,
                            scale,
                            occlusion_test,
                            elements,
                        },
                    )?;
                }
                "scene.lens_flare.remove" => {
                    let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                        format!(
                            "script command[{index}] scene.lens_flare.remove requires string 'id'"
                        )
                    })?;
                    self.scene.remove_lens_flare(id);
                }
                "scene.entity.transform.set" => {
                    let id = command
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
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
                    self.scene.remove_runtime_entity(id)?;
                }
                "scene.camera.set" => {
                    let position = command_vec3(command, "position", index)?;
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
                    self.scene.set_camera_pose(position, target, up, fov)?;
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
                other => {
                    return Err(format!(
                        "script command[{index}] uses unsupported engine command '{other}'"
                    ))
                }
            }
        }
        Ok(())
    }
}
