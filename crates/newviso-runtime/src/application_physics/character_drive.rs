//! Player input is an intent with finite traction; native impacts remain authoritative.
use super::*;
use newviso_character::GroundDrive;
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScriptDriveConfig {
    pub traction: f32,
    pub max_acceleration: f32,
    pub air_acceleration: f32,
    pub input_timeout_seconds: f32,
    #[serde(default)]
    pub stairs: newviso_character::CharacterConfig,
}
#[derive(Clone, Debug)]
pub(super) struct ScriptCharacterDrive {
    desired: [f32; 2],
    jump: Option<f32>,
    seen: f64,
    config: ScriptDriveConfig,
    previous: Option<GroundDrive>,
}
impl PhysicsRuntime {
    /// Physical pose ownership cancels upright intent without replacing momentum.
    pub(crate) fn suspend_character_drive(&mut self, entity: u64) {
        self.script_character_drives.remove(&entity);
        self.release_character_stairs(entity);
    }

    pub(crate) fn set_script_character_drive(
        &mut self,
        c: &Value,
        settings: &Value,
    ) -> Result<(), String> {
        let entity = c
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or("character drive needs a body")?;
        let cfg: ScriptDriveConfig = serde_json::from_value(settings.clone())
            .map_err(|e| format!("invalid ground drive: {e}"))?;
        cfg.stairs.validate()?;
        if ![
            cfg.traction,
            cfg.max_acceleration,
            cfg.air_acceleration,
            cfg.input_timeout_seconds,
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.0)
            || cfg.input_timeout_seconds == 0.0
        {
            return Err("invalid character traction or intent lifetime".into());
        }
        if self
            .bodies
            .get(&entity)
            .is_none_or(|b| !matches!(b.shape, CollisionShape::Capsule { .. }))
        {
            self.script_character_drives.remove(&entity);
            return Ok(());
        }
        let desired: [f32; 2] = serde_json::from_value(
            c.get("horizontal_velocity")
                .cloned()
                .ok_or("missing horizontal velocity")?,
        )
        .map_err(|e| e.to_string())?;
        let jump = c
            .get("jump_speed")
            .map(|v| v.as_f64().map(|x| x as f32).ok_or("invalid jump speed"))
            .transpose()?;
        if desired.iter().any(|v| !v.is_finite())
            || jump.is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 50.0)
        {
            return Err("invalid character drive intent".into());
        }
        let previous = self.script_character_drives.remove(&entity);
        self.script_character_drives.insert(
            entity,
            ScriptCharacterDrive {
                desired,
                jump: jump.or(previous.as_ref().and_then(|p| p.jump)),
                seen: self.fixed_seconds,
                config: cfg,
                previous: previous.and_then(|p| p.previous),
            },
        );
        Ok(())
    }
    pub(super) fn update_script_character_drives(&mut self, dt: f32) -> Result<(), String> {
        let entries = std::mem::take(&mut self.script_character_drives);
        for (entity, mut intent) in entries {
            let Some(body) = self.bodies.get(&entity) else {
                continue;
            };
            if !matches!(body.shape, CollisionShape::Capsule { .. }) {
                continue;
            }
            let velocity = body.linear_velocity;
            let grounded = self.character_has_support(entity) && velocity[1] <= 0.25;
            let support = self
                .last_output
                .events
                .iter()
                .filter_map(|event| {
                    let c = match event {
                        PhysicsEvent::ContactBegin(c) | PhysicsEvent::ContactPersist(c) => c,
                        _ => return None,
                    };
                    let own_a = c.a == entity;
                    let up = if own_a { c.normal[1] } else { -c.normal[1] };
                    if (!own_a && c.b != entity)
                        || up <= 0.45
                        || !self.current_contact_point(entity, c.point)
                    {
                        return None;
                    }
                    let other = if own_a { c.b } else { c.a };
                    let material = if own_a { c.materials.b } else { c.materials.a };
                    Some((
                        c.point[1],
                        other,
                        c.point,
                        material.map_or(self.surface_friction(Some(other)), |m| m.friction),
                        up,
                    ))
                })
                .max_by(|a, b| a.0.total_cmp(&b.0));
            let support_velocity = support
                .and_then(|(_, id, point, _, _)| {
                    self.vehicle_body_state(id).map(|s| {
                        let r = std::array::from_fn::<_, 3, _>(|i| point[i] - s.position[i]);
                        let w = s.angular_velocity;
                        [
                            s.linear_velocity[0] + w[1] * r[2] - w[2] * r[1],
                            s.linear_velocity[1] + w[2] * r[0] - w[0] * r[2],
                            s.linear_velocity[2] + w[0] * r[1] - w[1] * r[0],
                        ]
                    })
                })
                .unwrap_or([0.0; 3]);
            let expired =
                (self.fixed_seconds - intent.seen) as f32 > intent.config.input_timeout_seconds;
            let target = if expired { [0.0; 2] } else { intent.desired };
            let acceleration = if grounded {
                let friction = support.map_or(self.settings.scene_material.friction, |s| s.3);
                (intent.config.traction
                    * friction
                    * self.settings.gravity.abs()
                    * support.map_or(1.0, |s| s.4))
                .min(intent.config.max_acceleration)
            } else {
                intent.config.air_acceleration
            };
            let drive = GroundDrive::observe(
                intent.previous.as_ref(),
                entity,
                self.fixed_tick,
                dt,
                [
                    velocity[0] - support_velocity[0],
                    velocity[2] - support_velocity[2],
                ],
                target,
                acceleration,
            );
            let vy = intent
                .jump
                .take()
                .filter(|_| grounded && !expired)
                .map_or(velocity[1], |j| j + support_velocity[1]);
            self.drive_character_body(
                entity,
                [
                    drive.velocity[0] + support_velocity[0],
                    vy,
                    drive.velocity[1] + support_velocity[2],
                ],
                None,
            )?;
            self.register_character_stairs(
                entity,
                intent.config.stairs,
                [target[0], 0.0, target[1]],
            );
            intent.previous = Some(drive);
            self.script_character_drives.insert(entity, intent);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime() -> PhysicsRuntime {
        let mut p = PhysicsRuntime::empty_for_tests();
        p.upsert_body_from_script(
            &json!({"entity":3,"body_kind":"dynamic","position":[0,2,0],
  "shape":{"kind":"capsule","radius":0.3,"half_height":0.6},"linear_velocity":[0,-2,0]}),
            0,
        )
        .unwrap();
        p
    }
    fn intent(p: &mut PhysicsRuntime) {
        p.set_script_character_drive(&json!({"entity":3,"horizontal_velocity":[0,-4],"jump_speed":5}),
  &json!({"traction":0.9,"max_acceleration":6,"air_acceleration":0.5,"input_timeout_seconds":0.25})).unwrap();
    }
    #[test]
    fn airborne_input_is_bounded_cannot_jump_and_cannot_cancel_native_vertical_motion() {
        let mut p = runtime();
        intent(&mut p);
        p.update_script_character_drives(0.1).unwrap();
        assert!((p.bodies[&3].linear_velocity[2] + 0.05).abs() < 1e-5);
        assert_eq!(p.bodies[&3].linear_velocity[1], -2.0);
        assert!(p.script_character_drives[&3].jump.is_none());
    }
    #[test]
    fn stale_intent_brakes_and_explicit_handoff_cancels_drive() {
        let mut p = runtime();
        intent(&mut p);
        p.fixed_seconds = 0.3;
        p.update_script_character_drives(0.1).unwrap();
        assert_eq!(p.bodies[&3].linear_velocity, [0.0, -2.0, 0.0]);
        p.set_body_velocity_from_script(&json!({"entity":3,"velocity":[2,-3,4]}), 0)
            .unwrap();
        assert!(p.script_character_drives.is_empty());
        p.update_script_character_drives(0.1).unwrap();
        assert_eq!(p.bodies[&3].linear_velocity, [2.0, -3.0, 4.0]);
    }
    #[test]
    fn internal_pose_control_preserves_intent_but_external_handoff_clears_stairs() {
        let mut p = runtime();
        intent(&mut p);
        let passive_friction = p.bodies[&3].material.friction;
        p.register_character_stairs(
            3,
            newviso_character::CharacterConfig::default(),
            [1.0, 0.0, 0.0],
        );
        p.drive_character_body(3, [1.0, -2.0, 0.0], Some(2.1))
            .unwrap();
        assert!(p.script_character_drives.contains_key(&3));
        assert!(p.character_stairs.contains_key(&3));
        assert_eq!(p.bodies[&3].material.friction, 0.0);
        assert_eq!(p.character_traction_coefficient(3), passive_friction);
        p.set_body_pose_from_script(
            &json!({"entity":3,"position":[0,3,0],"rotation":[0,0,0,1]}),
            0,
        )
        .unwrap();
        assert!(!p.script_character_drives.contains_key(&3));
        assert!(!p.character_stairs.contains_key(&3));
        assert_eq!(p.bodies[&3].material.friction, passive_friction);
    }
    #[test]
    fn physical_pose_handoff_cancels_intent_preserving_collision_momentum() {
        let mut p = runtime();
        intent(&mut p);
        p.register_character_stairs(
            3,
            newviso_character::CharacterConfig::default(),
            [0.0, 0.0, -1.0],
        );
        let velocity = p.bodies[&3].linear_velocity;
        p.suspend_character_drive(3);
        assert!(p.script_character_drives.is_empty());
        assert!(p.character_stairs.is_empty());
        assert_eq!(p.bodies[&3].linear_velocity, velocity);
    }
}
