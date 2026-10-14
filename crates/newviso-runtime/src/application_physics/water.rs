//! Engine-level water forces run inside every fixed physics substep.
use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct WaterPhysics {
    pub surface: newviso_scene::water::WaterSurface,
    pub density_kg_m3: f32,
    pub linear_drag: f32,
    pub angular_drag: f32,
}

impl WaterPhysics {
    pub fn validate(&self) -> Result<(), String> {
        self.surface.validate().map_err(str::to_owned)?;
        if !self.density_kg_m3.is_finite()
            || !(1.0..=2500.0).contains(&self.density_kg_m3)
            || !self.linear_drag.is_finite()
            || !(0.0..=50.0).contains(&self.linear_drag)
            || !self.angular_drag.is_finite()
            || !(0.0..=50.0).contains(&self.angular_drag)
        {
            return Err("water physical density or damping outside supported range".into());
        }
        Ok(())
    }
}

pub(super) fn volume_and_extent(shape: CollisionShape) -> (f32, f32) {
    match shape {
        CollisionShape::Box { half_extents: h } => {
            ((8.0 * h[0] * h[1] * h[2]).max(0.0), (2.0 * h[1]).max(0.0))
        }
        CollisionShape::Sphere { radius: r } => {
            (4.0 * std::f32::consts::PI * r.powi(3) / 3.0, r * 2.0)
        }
        CollisionShape::Cylinder {
            radius: r,
            half_height: h,
        } => (std::f32::consts::PI * r * r * (2.0 * h), 2.0 * h),
        CollisionShape::Capsule {
            radius: r,
            half_height: h,
        } => (
            std::f32::consts::PI * r * r * (2.0 * h) + 4.0 * std::f32::consts::PI * r.powi(3) / 3.0,
            2.0 * (h + r),
        ),
    }
}

/// Newton-seconds over one fixed dt; always applied at the body's buoyancy
/// centre, never as a teleport / unconditional velocity overwrite.
#[cfg(test)]
pub(super) fn water_body_impulse(
    water: WaterPhysics,
    body: &PhysicsBodySnapshot,
    time_seconds: f32,
    gravity: f32,
    dt: f32,
) -> Option<([f32; 3], [f32; 3], f32)> {
    if body.kind != newviso_physics_client::PhysicsBodyKind::Dynamic || dt <= 0.0 {
        return None;
    }
    water_body_impulse_using(water, body, time_seconds, gravity, dt, None)
}

fn water_body_impulse_using(
    water: WaterPhysics,
    body: &PhysicsBodySnapshot,
    time_seconds: f32,
    gravity: f32,
    dt: f32,
    simulation: Option<&newviso_scene::water::WaterSimulation>,
) -> Option<([f32; 3], [f32; 3], f32)> {
    if body.kind != newviso_physics_client::PhysicsBodyKind::Dynamic || dt <= 0.0 {
        return None;
    }
    let (volume, _) = volume_and_extent(body.shape);
    // Rotated AABB height, not the unrotated box's local Y size.
    let height = (body.bounds_max[1] - body.bounds_min[1]).max(0.001);
    if volume <= 1e-5 || height <= 1e-5 {
        return None;
    }
    let bottom = body.position[1] - height * 0.5;
    let top = body.position[1] + height * 0.5;
    // Sample under the object's centre and at two opposite points of its
    // projected horizontal extent to respond to rolling wave crests.
    let positions = [
        [body.position[0], body.position[2]],
        [body.bounds_min[0], body.bounds_min[2]],
        [body.bounds_max[0], body.bounds_max[2]],
    ];
    let mut fraction = 0.0;
    let mut water_velocity = 0.0;
    for xy in positions {
        let sample = simulation
            .filter(|s| s.enabled)
            .map(|s| s.sample(xy))
            .unwrap_or_else(|| water.surface.sample(xy, time_seconds));
        fraction += ((sample.height - bottom) / (top - bottom)).clamp(0.0, 1.0);
        water_velocity += sample.vertical_velocity;
    }
    fraction /= positions.len() as f32;
    if fraction <= 0.0 {
        return None;
    }
    water_velocity /= positions.len() as f32;
    let mass = body
        .mass_properties
        .map(|p| p.mass)
        .unwrap_or_else(|| volume * body.material.density.max(1.0))
        .max(0.001);
    let lift = volume * fraction * water.density_kg_m3 * gravity.abs() * dt;
    // Linear resistance opposes relative velocity; cap impulse to prevent
    // reversing velocity under a large physics step.
    let drag_strength = (water.linear_drag * fraction * dt).clamp(0.0, 0.8);
    let impulse = [
        -body.linear_velocity[0] * mass * drag_strength,
        lift - (body.linear_velocity[1] - water_velocity) * mass * drag_strength,
        -body.linear_velocity[2] * mass * drag_strength,
    ];
    let point = [
        body.position[0],
        body.position[1]
            + (water.surface.level - body.position[1]).clamp(-height / 4.0, height / 4.0),
        body.position[2],
    ];
    Some((impulse, point, fraction))
}

impl PhysicsRuntime {
    pub(crate) fn water_surface(&self) -> Option<newviso_scene::water::WaterSurface> {
        self.water.map(|w| w.surface)
    }

    pub(crate) fn water_query_positions(&self) -> impl Iterator<Item = (u64, [f32; 2])> + '_ {
        self.bodies
            .values()
            .filter(|b| b.kind != newviso_physics_client::PhysicsBodyKind::Static)
            .map(|b| (b.entity, [b.position[0], b.position[2]]))
    }
    pub(crate) fn configure_water_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        if command.get("enabled").and_then(Value::as_bool) == Some(false) {
            self.water = None;
            return Ok(());
        }
        let previous = self.water.unwrap_or(WaterPhysics {
            surface: newviso_scene::water::WaterSurface::default(),
            density_kg_m3: 1025.0,
            linear_drag: 1.8,
            angular_drag: 0.6,
        });
        let mut next = previous;
        for (key, field) in [
            ("level", &mut next.surface.level),
            ("amplitude", &mut next.surface.amplitude),
            ("wavelength", &mut next.surface.wavelength),
            ("speed", &mut next.surface.speed),
            ("density_kg_m3", &mut next.density_kg_m3),
            ("linear_drag", &mut next.linear_drag),
            ("angular_drag", &mut next.angular_drag),
        ] {
            if command.get(key).is_some() {
                *field = command_number(command, key, index)?;
            }
        }
        if let Some(dir) = command.get("wind_direction") {
            let a = dir
                .as_array()
                .ok_or("water wind_direction must be a pair")?;
            if a.len() != 2 {
                return Err("water wind_direction must have two values".into());
            }
            next.surface.wind_direction = [
                a[0].as_f64().ok_or("invalid water wind x")? as f32,
                a[1].as_f64().ok_or("invalid water wind z")? as f32,
            ];
        }
        next.validate()?;
        self.water = Some(next);
        host::info(
            "newviso.physics.water",
            format!(
                "water enabled level={} amplitude={} wavelength={} density={}",
                next.surface.level,
                next.surface.amplitude,
                next.surface.wavelength,
                next.density_kg_m3
            ),
        );
        Ok(())
    }

    pub(crate) fn configure_body_water_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command_u64(command, "entity", index)?;
        match command
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("buoyant")
        {
            "buoyant" => {
                self.water_excluded.remove(&entity);
            }
            "swimmer" | "disabled" => {
                self.water_excluded.insert(entity);
            }
            other => return Err(format!("unsupported water body mode: {other}")),
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn push_water_forces(&mut self, fixed_dt: f32, commands: &mut Vec<PhysicsCommand>) {
        self.push_water_forces_using(fixed_dt, commands, None);
    }

    pub(super) fn push_water_forces_using(
        &mut self,
        fixed_dt: f32,
        commands: &mut Vec<PhysicsCommand>,
        simulation: Option<&newviso_scene::water::WaterSimulation>,
    ) {
        let Some(water) = self.water else {
            return;
        };
        let time = self.fixed_tick as f32 * fixed_dt;
        for body in self.bodies.values() {
            if self.water_excluded.contains(&body.entity) {
                continue;
            }
            let Some((impulse, _, fraction)) = water_body_impulse_using(
                water,
                body,
                time,
                self.settings.gravity,
                fixed_dt,
                simulation,
            ) else {
                continue;
            };
            let (volume, _) = volume_and_extent(body.shape);
            let height = (body.bounds_max[1] - body.bounds_min[1]).max(0.001);
            let mass = body
                .mass_properties
                .map(|v| v.mass)
                .unwrap_or(volume * body.material.density.max(1.0))
                .max(0.001);
            let lift =
                volume * fraction * water.density_kg_m3 * self.settings.gravity.abs() * fixed_dt;
            let center_drag = [impulse[0], impulse[1] - lift, impulse[2]];
            let center = body.position;

            let mut issue = |force: [f32; 3], point: [f32; 3]| {
                let seq = self.next_command_seq;
                self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
                commands.push(PhysicsCommand {
                    seq,
                    kind: PhysicsCommandKind::ApplyImpulse {
                        entity: body.entity,
                        impulse: force,
                        point,
                    },
                });
            };
            // Uniform flow resistance is a centre-of-mass force; no spin.
            if center_drag.iter().any(|v| v.abs() > 1e-6) {
                issue(center_drag, center);
            }

            // Four real buoyancy application points. The total lift is conserved,
            // while a wave crest underneath one corner creates natural pitch
            // and roll through Bullet's angular dynamics.
            let hx = ((body.bounds_max[0] - body.bounds_min[0]) * 0.32).max(0.05);
            let hz = ((body.bounds_max[2] - body.bounds_min[2]) * 0.32).max(0.05);
            let corners = [[-hx, -hz], [-hx, hz], [hx, -hz], [hx, hz]];
            let mut weights = [0.0; 4];
            let bottom = center[1] - height * 0.5;
            let top = center[1] + height * 0.5;
            for (i, corner) in corners.iter().enumerate() {
                let position = [center[0] + corner[0], center[2] + corner[1]];
                let sample = simulation
                    .filter(|s| s.enabled)
                    .map(|s| s.sample(position))
                    .unwrap_or_else(|| water.surface.sample(position, time));
                weights[i] = ((sample.height - bottom) / (top - bottom)).clamp(0.0, 1.0);
            }
            let sum = weights.iter().sum::<f32>();
            if sum > 1e-5 {
                for (i, corner) in corners.iter().enumerate() {
                    if weights[i] <= 0.0 {
                        continue;
                    }
                    let weight = weights[i] / sum;
                    let point = [
                        center[0] + corner[0],
                        center[1] - height * 0.22,
                        center[2] + corner[1],
                    ];
                    issue([0.0, lift * weight, 0.0], point);
                }
            } else {
                issue([0.0, lift, 0.0], center);
            }

            // Gravitas supports ApplyImpulse but not SetAngularVelocity.
            // A pair of equal/opposite tangential forces creates angular
            // drag without changing the body's overall linear momentum.
            let damping = (water.angular_drag * fraction * fixed_dt).clamp(0.0, 0.2);
            if damping > 1e-5 {
                for offset in [[hx, 0.0, 0.0], [0.0, 0.0, hz]] {
                    let omega = body.angular_velocity;
                    let tangent = [
                        omega[1] * offset[2] - omega[2] * offset[1],
                        omega[2] * offset[0] - omega[0] * offset[2],
                        omega[0] * offset[1] - omega[1] * offset[0],
                    ];
                    let pair = [
                        -tangent[0] * mass * damping * 0.20,
                        -tangent[1] * mass * damping * 0.20,
                        -tangent[2] * mass * damping * 0.20,
                    ];
                    if pair.iter().any(|v| v.abs() > 1e-5) {
                        issue(pair, std::array::from_fn(|i| center[i] + offset[i]));
                        issue(
                            pair.map(|v| -v),
                            std::array::from_fn(|i| center[i] - offset[i]),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn water_lifts_low_density_object_without_teleporting() {
        let water = WaterPhysics {
            surface: newviso_scene::water::WaterSurface {
                amplitude: 0.0,
                ..Default::default()
            },
            density_kg_m3: 1025.0,
            linear_drag: 1.0,
            angular_drag: 0.5,
        };
        let mut runtime = PhysicsRuntime::empty_for_tests();
        let body = PhysicsBodySnapshot {
            entity: 42,
            kind: newviso_physics_client::PhysicsBodyKind::Dynamic,
            shape: CollisionShape::Box {
                half_extents: [0.5; 3],
            },
            flags: Default::default(),
            material: PhysicsMaterial {
                friction: 0.4,
                restitution: 0.1,
                density: 400.0,
            },
            position: [0.0, -0.25, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            linear_velocity: [0.0; 3],
            angular_velocity: [0.0; 3],
            linear_damping: None,
            angular_damping: None,
            mass_properties: None,
            convex_hulls: vec![],
            bounds_min: [-0.5, -0.75, -0.5],
            bounds_max: [0.5, 0.25, 0.5],
        };
        let (impulse, _, fraction) =
            water_body_impulse(water, &body, 0.0, 9.81, 1.0 / 60.0).unwrap();
        assert!(impulse[1] > 0.0);
        assert!(fraction > 0.6);
        runtime.water = Some(water);
        runtime.bodies.insert(body.entity, body);
        runtime.settings.gravity = 9.81;
        runtime.fixed_tick = 10;
        let mut commands = Vec::new();
        runtime.push_water_forces(1.0 / 60.0, &mut commands);
        assert!(commands
            .iter()
            .any(|c| matches!(c.kind, PhysicsCommandKind::ApplyImpulse { entity: 42, .. })));
        assert!(!commands
            .iter()
            .any(|c| matches!(c.kind, PhysicsCommandKind::SetAngularVelocity { .. })));
    }
}
