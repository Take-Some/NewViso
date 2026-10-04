use super::*;

impl PhysicsRuntime {
    pub(crate) fn configure_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let mut next = self.settings;

        if command.get("fixed_hz").is_some() {
            next.fixed_hz = command_number(command, "fixed_hz", command_index)?;
        }
        if !(MIN_PHYSICS_HZ..=MAX_PHYSICS_HZ).contains(&next.fixed_hz) {
            return Err(format!(
                "script command[{command_index}] physics.world.configure fixed_hz must be in {MIN_PHYSICS_HZ}..={MAX_PHYSICS_HZ}"
            ));
        }

        if let Some(value) = command.get("max_steps_per_frame") {
            let value = value.as_u64().ok_or_else(|| {
                format!(
                    "script command[{command_index}] physics.world.configure max_steps_per_frame must be unsigned integer"
                )
            })?;
            let value = usize::try_from(value).map_err(|_| {
                format!(
                    "script command[{command_index}] physics.world.configure max_steps_per_frame out of range"
                )
            })?;
            if value == 0 || value > MAX_PHYSICS_STEPS_LIMIT {
                return Err(format!(
                    "script command[{command_index}] physics.world.configure max_steps_per_frame must be in 1..={MAX_PHYSICS_STEPS_LIMIT}"
                ));
            }
            next.max_steps_per_frame = value;
        }

        if command.get("gravity").is_some() {
            next.gravity = command_number(command, "gravity", command_index)?;
        }
        if !next.gravity.is_finite() || next.gravity.abs() > 1000.0 {
            return Err(format!(
                "script command[{command_index}] physics.world.configure gravity is invalid"
            ));
        }

        if command.get("contact_skin").is_some() {
            next.contact_skin = command_number(command, "contact_skin", command_index)?;
        }
        if !next.contact_skin.is_finite() || !(0.0..=1.0).contains(&next.contact_skin) {
            return Err(format!(
                "script command[{command_index}] physics.world.configure contact_skin must be in 0..=1"
            ));
        }

        if let Some(scene) = command.get("scene_colliders") {
            let scene = scene.as_object().ok_or_else(|| {
                format!(
                    "script command[{command_index}] physics.world.configure scene_colliders must be an object"
                )
            })?;
            if let Some(enabled) = scene.get("enabled") {
                next.scene_colliders_enabled = enabled.as_bool().ok_or_else(|| {
                    format!(
                        "script command[{command_index}] physics.world.configure scene_colliders.enabled must be boolean"
                    )
                })?;
            }
            if let Some(value) = scene.get("friction") {
                next.scene_material.friction = value.as_f64().map(|v| v as f32).filter(|v| v.is_finite()).ok_or_else(|| {
                    format!("script command[{command_index}] physics.world.configure scene_colliders.friction must be finite numeric")
                })?;
            }
            if let Some(value) = scene.get("restitution") {
                next.scene_material.restitution = value.as_f64().map(|v| v as f32).filter(|v| v.is_finite()).ok_or_else(|| {
                    format!("script command[{command_index}] physics.world.configure scene_colliders.restitution must be finite numeric")
                })?;
            }
            if let Some(value) = scene.get("density") {
                next.scene_material.density = value.as_f64().map(|v| v as f32).filter(|v| v.is_finite()).ok_or_else(|| {
                    format!("script command[{command_index}] physics.world.configure scene_colliders.density must be finite numeric")
                })?;
            }
            if let Some(value) = scene.get("participates_in_queries") {
                next.scene_participates_in_queries = value.as_bool().ok_or_else(|| {
                    format!("script command[{command_index}] physics.world.configure scene_colliders.participates_in_queries must be boolean")
                })?;
            }
            if let Some(value) = scene.get("casts_contacts") {
                next.scene_casts_contacts = value.as_bool().ok_or_else(|| {
                    format!("script command[{command_index}] physics.world.configure scene_colliders.casts_contacts must be boolean")
                })?;
            }
        }

        if next.scene_material.friction < 0.0
            || next.scene_material.restitution < 0.0
            || next.scene_material.density <= 0.0
        {
            return Err(format!(
                "script command[{command_index}] physics.world.configure scene collider material is invalid"
            ));
        }

        self.settings = next;
        self.accumulator = self.accumulator.min(next.fixed_dt());
        Ok(())
    }

    pub(crate) fn upsert_body_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let entity = command_u64(command, "entity", command_index)?;
        if entity >= STATIC_COLLIDER_ID_BASE {
            return Err(format!(
                "script command[{command_index}] physics body entity {entity} is in the reserved engine range"
            ));
        }

        let body_kind = match command
            .get("body_kind")
            .or_else(|| command.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("dynamic")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "static" => PhysicsBodyKind::Static,
            "dynamic" => PhysicsBodyKind::Dynamic,
            "kinematic" => PhysicsBodyKind::Kinematic,
            other => {
                return Err(format!(
                    "script command[{command_index}] physics.body.upsert has invalid body_kind '{other}'"
                ))
            }
        };

        let shape_value = command.get("shape").ok_or_else(|| {
            format!("script command[{command_index}] physics.body.upsert requires object 'shape'")
        })?;
        let shape_kind = shape_value
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "script command[{command_index}] physics.body.upsert shape requires string 'kind'"
                )
            })?
            .trim()
            .to_ascii_lowercase();

        let shape = match shape_kind.as_str() {
            "sphere" => CollisionShape::Sphere {
                radius: command_number(shape_value, "radius", command_index)?,
            },
            "box" => CollisionShape::Box {
                half_extents: command_vec3(shape_value, "half_extents", command_index)?,
            },
            "capsule" => CollisionShape::Capsule {
                radius: command_number(shape_value, "radius", command_index)?,
                half_height: command_number(shape_value, "half_height", command_index)?,
            },
            "cylinder" => CollisionShape::Cylinder {
                radius: command_number(shape_value, "radius", command_index)?,
                half_height: command_number(shape_value, "half_height", command_index)?,
            },
            other => {
                return Err(format!(
                    "script command[{command_index}] physics.body.upsert has invalid shape kind '{other}'"
                ))
            }
        };

        let mut position = command_vec3(command, "position", command_index)?;
        let rotation = command
            .get("rotation")
            .map(|_| command_vec4(command, "rotation", command_index))
            .transpose()?
            .unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let linear_velocity = command
            .get("linear_velocity")
            .map(|_| command_vec3(command, "linear_velocity", command_index))
            .transpose()?
            .unwrap_or([0.0; 3]);
        let angular_velocity = command
            .get("angular_velocity")
            .map(|_| command_vec3(command, "angular_velocity", command_index))
            .transpose()?
            .unwrap_or([0.0; 3]);

        if let Some(ground_snap) = command.get("ground_snap") {
            let required = optional_bool(ground_snap, "required", false)?;
            match self.snap_body_position_to_ground(
                shape,
                position,
                ground_snap,
                entity,
                command_index,
            )? {
                Some(snapped) => position = snapped,
                None if required => {
                    host::debug(
                        "newviso.physics",
                        format!(
                            "deferred physics.body.upsert entity={} because required ground support is not resident",
                            entity
                        ),
                    );
                    return Ok(());
                }
                None => {}
            }
        }

        let material = PhysicsMaterial {
            friction: optional_number(command, "friction", 0.55)?,
            restitution: optional_number(command, "restitution", 0.25)?,
            density: optional_number(command, "density", 1.0)?,
        };
        let flags = PhysicsBodyFlags {
            is_trigger: optional_bool(command, "is_trigger", false)?,
            participates_in_queries: optional_bool(command, "participates_in_queries", true)?,
            casts_contacts: optional_bool(command, "casts_contacts", true)?,
            continuous_collision: optional_bool(command, "continuous_collision", false)?,
        };

        let linear_damping = optional_nullable_number(command, "linear_damping")?;
        let angular_damping = optional_nullable_number(command, "angular_damping")?;
        let mass_properties = command
            .get("mass_properties")
            .map(|v| {
                serde_json::from_value::<newviso_physics_client::PhysicsMassProperties>(v.clone())
            })
            .transpose()
            .map_err(|e| format!("invalid mass_properties: {e}"))?;
        let convex_hulls = command
            .get("convex_hulls")
            .map(|v| serde_json::from_value::<Vec<Vec<[f32; 3]>>>(v.clone()))
            .transpose()
            .map_err(|e| format!("invalid convex_hulls: {e}"))?
            .unwrap_or_default();
        let (bounds_min, bounds_max) = shape_bounds(shape, position);

        let replacing = self.bodies.contains_key(&entity);
        self.bodies.insert(
            entity,
            PhysicsBodySnapshot {
                entity,
                kind: body_kind,
                shape,
                flags,
                material,
                position,
                rotation,
                linear_velocity,
                angular_velocity,
                linear_damping,
                angular_damping,
                mass_properties,
                convex_hulls,
                bounds_min,
                bounds_max,
            },
        );
        if replacing {
            let mut resets = vec![PhysicsCommandKind::SetBodyPose {
                entity,
                position,
                rotation,
            }];
            // Authored mass properties use the extended native vehicle contract.
            // Keep density-only bodies compatible with existing legacy providers.
            if mass_properties.is_some() {
                resets.push(PhysicsCommandKind::SetAngularVelocity {
                    entity,
                    velocity: angular_velocity,
                });
            }
            resets.push(PhysicsCommandKind::SetLinearVelocity {
                entity,
                velocity: linear_velocity,
            });
            for kind in resets {
                let seq = self.next_command_seq;
                self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
                self.pending_commands.push(PhysicsCommand { seq, kind });
            }
        }
        if let Some(body) = self.bodies.get_mut(&entity) {
            refresh_bounds(body);
        }
        match optional_nullable_number(command, "damage")? {
            Some(damage) if damage < 0.0 => {
                return Err(format!(
                "script command[{command_index}] physics.body.upsert damage must be non-negative"
            ))
            }
            Some(damage) if damage > 0.0 => {
                let kind =
                    PhysicsDamageKind::parse(command.get("damage_kind").and_then(Value::as_str))
                        .map_err(|error| {
                            format!("script command[{command_index}] physics.body.upsert {error}")
                        })?;
                self.damage_sources
                    .insert(entity, PhysicsDamageSource { damage, kind });
            }
            _ => {
                self.damage_sources.remove(&entity);
            }
        }
        Ok(())
    }

    pub(crate) fn destroy_body_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let entity = command_u64(command, "entity", command_index)?;
        self.bodies.remove(&entity);
        self.damage_sources.remove(&entity);
        self.scene_pose_offsets.remove(&entity);
        self.scene_rotation_offsets.remove(&entity);
        Ok(())
    }

    pub(crate) fn set_body_velocity_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let entity = command_u64(command, "entity", command_index)?;
        if !self.bodies.contains_key(&entity) {
            return Err(format!(
                "script command[{command_index}] physics.body.velocity.set references unknown body {entity}"
            ));
        }
        let velocity = command_vec3(command, "velocity", command_index)?;
        let seq = self.next_command_seq;
        self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
        self.pending_commands.push(PhysicsCommand {
            seq,
            kind: PhysicsCommandKind::SetLinearVelocity { entity, velocity },
        });
        Ok(())
    }

    pub(crate) fn set_body_pose_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let entity = command_u64(command, "entity", command_index)?;
        if !self.bodies.contains_key(&entity) {
            return Err(format!(
                "script command[{command_index}] physics.body.pose.set references unknown body {entity}"
            ));
        }
        let shape = self
            .bodies
            .get(&entity)
            .map(|body| body.shape)
            .expect("body existence checked above");
        let mut position = command_vec3(command, "position", command_index)?;
        if let Some(ground_snap) = command.get("ground_snap") {
            let required = optional_bool(ground_snap, "required", false)?;
            match self.snap_body_position_to_ground(
                shape,
                position,
                ground_snap,
                entity,
                command_index,
            )? {
                Some(snapped) => position = snapped,
                None if required => return Ok(()),
                None => {}
            }
        }
        let rotation = command
            .get("rotation")
            .map(|_| command_vec4(command, "rotation", command_index))
            .transpose()?
            .unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let seq = self.next_command_seq;
        self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
        self.pending_commands.push(PhysicsCommand {
            seq,
            kind: PhysicsCommandKind::SetBodyPose {
                entity,
                position,
                rotation,
            },
        });
        Ok(())
    }

    pub(crate) fn apply_impulse_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let entity = command_u64(command, "entity", command_index)?;
        let impulse = command_vec3(command, "impulse", command_index)?;
        let point = command
            .get("point")
            .map(|_| command_vec3(command, "point", command_index))
            .transpose()?
            .or_else(|| self.bodies.get(&entity).map(|body| body.position))
            .ok_or_else(|| {
                format!(
                    "script command[{command_index}] physics.body.impulse references unknown body {entity}"
                )
            })?;

        let seq = self.next_command_seq;
        self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
        self.pending_commands.push(PhysicsCommand {
            seq,
            kind: PhysicsCommandKind::ApplyImpulse {
                entity,
                impulse,
                point,
            },
        });
        Ok(())
    }
}
