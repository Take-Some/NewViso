//! particles.
use super::*;

impl Scene3dRuntime {
    pub fn spawn_particles(
        &mut self,
        particles: Vec<SceneParticleSpawnDesc>,
    ) -> Result<(), String> {
        let transient = particles
            .iter()
            .filter(|p| {
                !self.physical_particle_debris_enabled
                    || p.style
                        .as_ref()
                        .is_none_or(|s| s.physical_debris_density.is_none())
            })
            .count();
        if self.particles.len().saturating_add(transient) > self.render_policy.particle_capacity {
            return Err(format!(
                "scene.particles.spawn exceeds the configured limit of {}",
                self.render_policy.particle_capacity
            ));
        }
        for desc in particles {
            let finite = desc
                .position
                .iter()
                .chain(desc.velocity.iter())
                .chain(desc.acceleration.iter())
                .chain(desc.size.iter())
                .chain(desc.end_size.iter())
                .chain(desc.color.iter())
                .chain(desc.end_color.iter())
                .all(|value| value.is_finite())
                && desc.lifetime_seconds.is_finite()
                && desc.rotation_degrees.is_finite()
                && desc.angular_velocity_degrees.is_finite();
            if !finite
                || desc.lifetime_seconds <= 0.0
                || desc.lifetime_seconds > 120.0
                || desc.size.iter().any(|value| *value <= 0.0)
                || desc.end_size.iter().any(|value| *value <= 0.0)
                || desc.style.as_ref().is_some_and(|style| !style.valid())
            {
                return Err("scene.particles.spawn contains invalid particle data".to_owned());
            }
            let age_seconds = -desc.style.as_ref().map_or(0.0, |style| style.delay_seconds);
            if self.physical_particle_debris_enabled
                && desc
                    .style
                    .as_ref()
                    .is_some_and(|s| s.physical_debris_density.is_some())
            {
                self.spawn_physical_particle(desc)?;
                continue;
            }
            let trail_history = if desc.style.as_ref().is_some_and(|s| s.trail) {
                vec![desc.position]
            } else {
                Vec::new()
            };
            self.particles.push(SceneRuntimeParticle {
                desc,
                age_seconds,
                trail_history,
                physical: None,
            });
        }
        Ok(())
    }

    pub fn clear_particles(&mut self) {
        self.particles.clear();
        self.particle_interior_stats = ParticleInteriorStats::default();
        self.particle_interior_contacts = 0;
    }

    pub(super) fn update_particles(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.update_physical_particles(dt);
        let solids = if self
            .particles
            .iter()
            .any(|p| p.desc.style.as_ref().is_some_and(|s| s.collision.is_some()))
        {
            self.world.solid_bounds().collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let interiors = self.particle_interior_volumes();
        for particle in &mut self.particles {
            particle.age_seconds += dt;
            let active_dt = dt.min(particle.age_seconds.max(0.0));
            if active_dt <= 0.0 {
                continue;
            }
            let before = particle.desc.position;
            let style = particle.desc.style.as_ref();
            let t = (particle.age_seconds / particle.desc.lifetime_seconds).clamp(0.0, 1.0);
            let simulation_dt = active_dt * style.and_then(|s| s.motion_rate).unwrap_or(1.0);
            let acceleration = style.map_or(particle.desc.acceleration, |s| {
                sample_particle_curve(&s.acceleration_keys, t, particle.desc.acceleration)
            });
            let drag = style.map_or([0.0; 3], |s| {
                sample_particle_curve(&s.drag_keys, t, [0.0; 3])
            });
            for axis in 0..3 {
                particle.desc.velocity[axis] += acceleration[axis] * simulation_dt;
                particle.desc.velocity[axis] *= (1.0 - drag[axis] * simulation_dt).max(0.0);
                particle.desc.position[axis] += particle.desc.velocity[axis] * simulation_dt;
            }
            if let Some([bounce, radius_multiplier, min_radius, rest_speed]) =
                style.and_then(|s| s.collision)
            {
                let size = sample_particle_curve(&style.unwrap().size_keys, t, particle.desc.size);
                let radius = (size[0].min(size[1]) * 0.5 * radius_multiplier)
                    .max(min_radius)
                    .max(0.001);
                if let Some((position, normal)) =
                    particle_surface_contact(before, particle.desc.position, radius, &solids)
                {
                    particle.desc.position = position;
                    let dot = (0..3)
                        .map(|i| particle.desc.velocity[i] * normal[i])
                        .sum::<f32>();
                    particle.desc.velocity = std::array::from_fn(|i| {
                        (particle.desc.velocity[i] - 2.0 * dot * normal[i]) * bounce
                    });
                    if particle.desc.velocity.iter().map(|v| v * v).sum::<f32>()
                        < rest_speed * rest_speed
                    {
                        particle.desc.velocity = [0.0; 3];
                        particle.desc.angular_velocity_degrees = 0.0;
                        if let Some(style) = particle.desc.style.as_mut() {
                            style.model_rotation = std::array::from_fn(|i| {
                                style.model_rotation[i] + style.model_spin[i] * particle.age_seconds
                            });
                            style.model_spin = [0.0; 3];
                        }
                    }
                }
            }
            for volume in &interiors {
                if let Some((position, normal)) = volume.contact(before, particle.desc.position) {
                    particle.desc.position = position;
                    let inward_speed = (0..3)
                        .map(|i| particle.desc.velocity[i] * normal[i])
                        .sum::<f32>()
                        .min(0.0);
                    for i in 0..3 {
                        particle.desc.velocity[i] -= inward_speed * normal[i];
                    }
                    self.particle_interior_contacts += 1;
                }
            }
            particle.desc.rotation_degrees +=
                particle.desc.angular_velocity_degrees * simulation_dt;
            if particle.desc.style.as_ref().is_some_and(|s| s.trail) {
                particle.trail_history.push(particle.desc.position);
                if particle.trail_history.len() > 16 {
                    particle.trail_history.remove(0);
                }
            }
        }
        self.particles
            .retain(|particle| particle.age_seconds < particle.desc.lifetime_seconds);
    }

    /// A fixed-size, age-limited set of impact wakes for the GPU ocean shader.
    pub fn add_water_impact(&mut self, position:[f32;3], intensity:f32) -> Result<(),String> {
        if position.iter().any(|v|!v.is_finite()) || !intensity.is_finite()
            || !(0.05..=8.0).contains(&intensity) {
            return Err("scene.water.impact requires finite position and intensity 0.05..8".to_owned());
        }
        let now=self.water_simulation.seconds() as f32;
        self.water_simulation.impact([position[0],position[2]],intensity);
        self.water_impacts.retain(|impact|now-impact.start_time < 2.8);
        if self.water_impacts.len()>=8 {self.water_impacts.remove(0);}
        self.water_impacts.push(WaterImpact {
            x:position[0],z:position[2],start_time:now,intensity
        });
        Ok(())
    }
}
