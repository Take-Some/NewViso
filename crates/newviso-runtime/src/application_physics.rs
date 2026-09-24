use super::*;

const STATIC_COLLIDER_ID_BASE: u64 = 1_u64 << 63;
const PHYSICS_FIXED_DT: f32 = 1.0 / 120.0;
const MAX_PHYSICS_STEPS_PER_FRAME: usize = 8;
const DEFAULT_GRAVITY: f32 = 9.81;
const DEFAULT_CONTACT_SKIN: f32 = 0.002;

#[derive(Debug)]
pub(super) struct PhysicsRuntime {
    client: PhysicsClient,
    bodies: BTreeMap<u64, PhysicsBodySnapshot>,
    pending_commands: Vec<PhysicsCommand>,
    frame_index: u64,
    fixed_tick: u64,
    next_command_seq: u64,
    accumulator: f32,
    last_output: PhysicsFrameOutput,
}

impl PhysicsRuntime {
    pub(super) fn connect() -> Result<Self, String> {
        let client = PhysicsClient::new();
        let negotiation = client.negotiate(
            vec![
                PhysicsFeature::StaticColliders,
                PhysicsFeature::DynamicBodies,
            ],
            vec![
                PhysicsFeature::Contacts,
                PhysicsFeature::Queries,
                PhysicsFeature::NativeBackend,
            ],
        )?;

        host::info(
            "newviso.physics",
            format!(
                "physics runtime connected version={}.{}.{} features={:?}",
                negotiation.backend_version.major,
                negotiation.backend_version.minor,
                negotiation.backend_version.patch,
                negotiation.enabled_features
            ),
        );

        Ok(Self {
            client,
            bodies: BTreeMap::new(),
            pending_commands: Vec::new(),
            frame_index: 0,
            fixed_tick: 0,
            next_command_seq: 1,
            accumulator: 0.0,
            last_output: PhysicsFrameOutput::default(),
        })
    }

    pub(super) fn step(
        &mut self,
        dt: f32,
        scene_solids: &[([f32; 3], [f32; 3])],
    ) -> Result<(), String> {
        if !dt.is_finite() || dt <= 0.0 {
            return Ok(());
        }

        self.frame_index = self.frame_index.wrapping_add(1);
        self.accumulator += dt.min(0.05);

        let mut steps = 0usize;
        while self.accumulator + 1.0e-7 >= PHYSICS_FIXED_DT && steps < MAX_PHYSICS_STEPS_PER_FRAME {
            self.fixed_tick = self.fixed_tick.wrapping_add(1);

            let mut bodies = static_scene_bodies(scene_solids);
            bodies.extend(self.bodies.values().cloned());

            let commands = if steps == 0 {
                std::mem::take(&mut self.pending_commands)
            } else {
                Vec::new()
            };

            let output = self.client.step_frame(PhysicsFrameInput {
                frame_index: self.frame_index,
                fixed_tick: self.fixed_tick,
                dt: PHYSICS_FIXED_DT,
                gravity: DEFAULT_GRAVITY,
                contact_skin: DEFAULT_CONTACT_SKIN,
                bodies,
                colliders: Vec::new(),
                commands,
                queries: Vec::new(),
            })?;

            self.apply_output(&output);
            self.last_output = output;
            self.accumulator -= PHYSICS_FIXED_DT;
            steps += 1;
        }

        if steps == MAX_PHYSICS_STEPS_PER_FRAME && self.accumulator >= PHYSICS_FIXED_DT {
            // Do not allow a long hitch to create an unbounded catch-up spiral.
            // Bodies stay alive and continue from the most recent simulated state.
            self.accumulator = self.accumulator.min(PHYSICS_FIXED_DT);
        }

        Ok(())
    }

    fn apply_output(&mut self, output: &PhysicsFrameOutput) {
        for pose in &output.pose_updates {
            let Some(body) = self.bodies.get_mut(&pose.entity) else {
                continue;
            };
            body.position = pose.position;
            body.rotation = pose.rotation;
            refresh_bounds(body);
        }

        for velocity in &output.velocity_updates {
            let Some(body) = self.bodies.get_mut(&velocity.entity) else {
                continue;
            };
            body.linear_velocity = velocity.linear_velocity;
            body.angular_velocity = velocity.angular_velocity;
        }
    }

    pub(super) fn runtime_state(&self) -> Value {
        let bodies = self
            .bodies
            .values()
            .map(|body| {
                json!({
                    "entity": body.entity,
                    "kind": match body.kind {
                        PhysicsBodyKind::Static => "static",
                        PhysicsBodyKind::Dynamic => "dynamic",
                        PhysicsBodyKind::Kinematic => "kinematic",
                    },
                    "position": body.position,
                    "rotation": body.rotation,
                    "linear_velocity": body.linear_velocity,
                    "angular_velocity": body.angular_velocity,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "enabled": true,
            "fixed_hz": 120,
            "fixed_tick": self.fixed_tick,
            "bodies": bodies,
            "events": self.last_output.events,
            "report": self.last_output.report,
        })
    }

    pub(super) fn upsert_body_from_script(
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

        let position = command_vec3(command, "position", command_index)?;
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
        let (bounds_min, bounds_max) = shape_bounds(shape, position);

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
                bounds_min,
                bounds_max,
            },
        );
        Ok(())
    }

    pub(super) fn destroy_body_from_script(
        &mut self,
        command: &Value,
        command_index: usize,
    ) -> Result<(), String> {
        let entity = command_u64(command, "entity", command_index)?;
        self.bodies.remove(&entity);
        Ok(())
    }

    pub(super) fn apply_impulse_from_script(
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

fn static_scene_bodies(solids: &[([f32; 3], [f32; 3])]) -> Vec<PhysicsBodySnapshot> {
    solids
        .iter()
        .enumerate()
        .map(|(index, (min, max))| {
            let center = [
                (min[0] + max[0]) * 0.5,
                (min[1] + max[1]) * 0.5,
                (min[2] + max[2]) * 0.5,
            ];
            let half_extents = [
                ((max[0] - min[0]) * 0.5).max(0.001),
                ((max[1] - min[1]) * 0.5).max(0.001),
                ((max[2] - min[2]) * 0.5).max(0.001),
            ];
            PhysicsBodySnapshot {
                entity: STATIC_COLLIDER_ID_BASE | index as u64,
                kind: PhysicsBodyKind::Static,
                shape: CollisionShape::Box { half_extents },
                flags: PhysicsBodyFlags {
                    is_trigger: false,
                    participates_in_queries: true,
                    casts_contacts: true,
                    continuous_collision: false,
                },
                material: PhysicsMaterial {
                    friction: 0.72,
                    restitution: 0.04,
                    density: 1.0,
                },
                position: center,
                rotation: [0.0, 0.0, 0.0, 1.0],
                linear_velocity: [0.0; 3],
                angular_velocity: [0.0; 3],
                linear_damping: Some(0.0),
                angular_damping: Some(0.0),
                bounds_min: *min,
                bounds_max: *max,
            }
        })
        .collect()
}

fn shape_bounds(shape: CollisionShape, position: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let half = match shape {
        CollisionShape::Box { half_extents } => half_extents,
        CollisionShape::Sphere { radius } => [radius; 3],
        CollisionShape::Capsule {
            radius,
            half_height,
        }
        | CollisionShape::Cylinder {
            radius,
            half_height,
        } => [radius, radius + half_height, radius],
    };
    (
        [
            position[0] - half[0],
            position[1] - half[1],
            position[2] - half[2],
        ],
        [
            position[0] + half[0],
            position[1] + half[1],
            position[2] + half[2],
        ],
    )
}

fn refresh_bounds(body: &mut PhysicsBodySnapshot) {
    let (min, max) = shape_bounds(body.shape, body.position);
    body.bounds_min = min;
    body.bounds_max = max;
}

fn command_u64(value: &Value, key: &str, index: usize) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("script command[{index}] requires unsigned integer '{key}'"))
}

fn optional_number(value: &Value, key: &str, default: f32) -> Result<f32, String> {
    match value.get(key) {
        Some(value) => value
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("physics field '{key}' must be finite numeric")),
        None => Ok(default),
    }
}

fn optional_nullable_number(value: &Value, key: &str) -> Result<Option<f32>, String> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .map(Some)
            .ok_or_else(|| format!("physics field '{key}' must be finite numeric or null")),
    }
}

fn optional_bool(value: &Value, key: &str, default: bool) -> Result<bool, String> {
    match value.get(key) {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("physics field '{key}' must be boolean")),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_scene_body_uses_reserved_id_and_box_bounds() {
        let bodies = static_scene_bodies(&[([-2.0, 0.0, -3.0], [2.0, 1.0, 3.0])]);
        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0].entity, STATIC_COLLIDER_ID_BASE);
        assert_eq!(bodies[0].position, [0.0, 0.5, 0.0]);
        assert_eq!(
            bodies[0].shape,
            CollisionShape::Box {
                half_extents: [2.0, 0.5, 3.0]
            }
        );
    }

    #[test]
    fn sphere_bounds_follow_physics_pose() {
        let (min, max) = shape_bounds(CollisionShape::Sphere { radius: 0.25 }, [1.0, 2.0, 3.0]);
        assert_eq!(min, [0.75, 1.75, 2.75]);
        assert_eq!(max, [1.25, 2.25, 3.25]);
    }
}
