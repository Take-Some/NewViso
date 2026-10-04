use super::*;

#[derive(Clone, Debug)]
pub struct ScenePhysicalParticleSpawn {
    pub entity: u64,
    pub position: [f32; 3],
    pub half_extents: [f32; 3],
    pub hull: Vec<[f32; 3]>,
    pub velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub mass: f32,
    pub restitution: f32,
    delay_seconds: f32,
}

#[derive(Clone, Debug)]
pub(super) struct PhysicalParticleGeometry {
    entity: u64,
    key: String,
    vertices: std::sync::Arc<[f32]>,
    rotation: Vec3,
    mass: f32,
}

impl PhysicalParticleGeometry {
    pub(super) fn append(&self, position: [f32; 3], out: &mut Vec<f32>) {
        let origin = Vec3::new(position[0], position[1], position[2]);
        let unit = Vec3::new(1.0, 1.0, 1.0);
        for vertex in self.vertices.chunks_exact(FLOATS_PER_VERTEX) {
            let mut v: [f32; FLOATS_PER_VERTEX] = vertex.try_into().unwrap();
            let p = transform_point(Vec3::new(v[0], v[1], v[2]), unit, self.rotation, origin);
            v[..3].copy_from_slice(&[p.x, p.y, p.z]);
            let n = transform_point(Vec3::new(v[4], v[5], v[6]), unit, self.rotation, Vec3::ZERO);
            v[4..7].copy_from_slice(&[n.x, n.y, n.z]);
            let t = transform_point(
                Vec3::new(v[13], v[14], v[15]),
                unit,
                self.rotation,
                Vec3::ZERO,
            );
            v[13..16].copy_from_slice(&[t.x, t.y, t.z]);
            out.extend_from_slice(&v);
        }
    }
}

impl Scene3dRuntime {
    pub fn enable_physical_particle_debris(&mut self, enabled: bool) {
        self.physical_particle_debris_enabled = enabled;
    }

    pub fn drain_physical_particle_spawns(&mut self) -> Vec<ScenePhysicalParticleSpawn> {
        let mut ready = Vec::new();
        for spawn in std::mem::take(&mut self.physical_particle_spawns) {
            if self.world.entity(SceneEntityId(spawn.entity)).is_none() {
                continue;
            }
            if spawn.delay_seconds <= 0.0 {
                ready.push(spawn);
            } else {
                self.physical_particle_spawns.push(spawn);
            }
        }
        ready
    }

    pub fn drain_removed_physical_particles(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.removed_physical_particles)
    }

    pub(super) fn spawn_physical_particle(
        &mut self,
        mut desc: SceneParticleSpawnDesc,
    ) -> Result<(), String> {
        let style = desc
            .style
            .as_mut()
            .ok_or("physical particle style missing")?;
        let density = style
            .physical_debris_density
            .ok_or("physical particle density missing")?;
        let delay = style.delay_seconds;
        let motion_rate = style.motion_rate.unwrap_or(1.0);
        // A solid keeps its authored size and visible material. Its source rule's
        // end-of-life fade/shrink belongs to the transient representation only.
        if let Some((_, color)) = style
            .color_keys
            .iter()
            .max_by(|a, b| a.1[3].total_cmp(&b.1[3]))
        {
            desc.color = *color;
        } else if desc.end_color[3] > desc.color[3] {
            desc.color = desc.end_color;
        }
        if let Some((_, size)) = style
            .size_keys
            .iter()
            .find(|(_, s)| s.iter().all(|v| *v > 0.001))
        {
            desc.size = *size;
        }
        style.color_keys.clear();
        style.size_keys.clear();
        desc.end_color = desc.color;
        desc.end_size = desc.size;
        let restitution = style.collision.map_or(0.12, |c| c[0].clamp(0.0, 1.0));
        let model = style.model.is_some();
        let model_spin = style.model_spin;
        let model_basis = style.model_basis;
        let mut particle = SceneRuntimeParticle {
            desc,
            age_seconds: 0.0,
            trail_history: Vec::new(),
            physical: None,
        };
        let forward = self.camera.target.sub(self.camera.position).normalized();
        let right = forward.cross(self.camera.up).normalized();
        let up = right.cross(forward).normalized();
        let mut vertices = Vec::new();
        particle_geometry::append_particle_visual(
            &particle,
            right,
            up,
            forward,
            None,
            &mut vertices,
        );
        if vertices.is_empty() {
            return Err("physical particle has no geometry".to_owned());
        }
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for v in vertices.chunks_exact(FLOATS_PER_VERTEX) {
            for i in 0..3 {
                min[i] = min[i].min(v[i]);
                max[i] = max[i].max(v[i]);
            }
        }
        let center = std::array::from_fn(|i| (min[i] + max[i]) * 0.5);
        let thickness = particle.desc.size[0]
            .min(particle.desc.size[1])
            .mul_add(0.06, 0.0)
            .clamp(0.002, 0.012);
        let mut hull = Vec::new();
        let mut keys = BTreeSet::new();
        for v in vertices.chunks_exact_mut(FLOATS_PER_VERTEX) {
            for i in 0..3 {
                v[i] -= center[i];
            }
            for side in [-0.5, 0.5] {
                let point = std::array::from_fn(|i| v[i] + v[i + 4] * thickness * side);
                if keys.insert(point.map(f32::to_bits)) {
                    hull.push(point);
                }
            }
        }
        let half_extents =
            std::array::from_fn(|i| hull.iter().map(|p| p[i].abs()).fold(0.002f32, f32::max));
        let volume = if model {
            8.0 * half_extents[0] * half_extents[1] * half_extents[2]
        } else {
            particle.desc.size[0] * particle.desc.size[1] * thickness
        };
        let mass = (density * volume).clamp(0.001, 30.0);
        let key = loop {
            self.next_physical_particle_serial = self
                .next_physical_particle_serial
                .checked_add(1)
                .ok_or("particle debris identity space exhausted")?;
            let k = format!(
                "world.particle_debris.{}",
                self.next_physical_particle_serial
            );
            if self.runtime_entity_stable_id(&k).is_none() {
                break k;
            }
        };
        let entity = self.upsert_runtime_dynamic_entity(
            &key,
            SceneRuntimeEntityDesc {
                position: center,
                bounds_half_extent: half_extents,
                ..Default::default()
            },
        )?;
        let angular_velocity = if model {
            std::array::from_fn(|i| {
                (0..3)
                    .map(|j| model_basis[j][i] * model_spin[j].to_radians())
                    .sum()
            })
        } else {
            let n = forward.mul(-particle.desc.angular_velocity_degrees.to_radians() * motion_rate);
            [n.x, n.y, n.z]
        };
        self.physical_particle_spawns
            .push(ScenePhysicalParticleSpawn {
                entity,
                position: center,
                half_extents,
                hull,
                velocity: particle.desc.velocity.map(|v| v * motion_rate),
                angular_velocity,
                mass,
                restitution,
                delay_seconds: delay,
            });
        particle.desc.position = center;
        particle.age_seconds = -delay;
        particle.physical = Some(PhysicalParticleGeometry {
            entity,
            key,
            vertices: std::sync::Arc::from(vertices),
            rotation: Vec3::ZERO,
            mass,
        });
        self.physical_particles.push(particle);
        Ok(())
    }

    pub(super) fn update_physical_particles(&mut self, dt: f32) {
        for spawn in &mut self.physical_particle_spawns {
            spawn.delay_seconds -= dt;
        }
        for particle in &mut self.physical_particles {
            particle.age_seconds += dt;
            let physical = particle.physical.as_mut().unwrap();
            if let Some(entity) = self.world.entity(SceneEntityId(physical.entity)) {
                let p = entity.transform.position;
                particle.desc.position = [p.x, p.y, p.z];
                physical.rotation = entity.transform.rotation_degrees;
            } else {
                self.removed_physical_particles.push(physical.entity);
            }
        }
        self.physical_particles.retain(|p| {
            self.world
                .entity(SceneEntityId(p.physical.as_ref().unwrap().entity))
                .is_some()
        });
    }

    pub(super) fn physical_particle_runtime_state(&self) -> Value {
        let entities=self.physical_particles.iter().map(|p| {
            let physical=p.physical.as_ref().unwrap();let style=p.desc.style.as_ref().unwrap();
            json!({"entity":physical.entity,"scene_key":physical.key,"effect":style.effect_name,"emitter":style.emitter_name,
                "mass":physical.mass,"size":p.desc.size,"position":p.desc.position,"age_seconds":p.age_seconds,
                "source_lifetime_seconds":p.desc.lifetime_seconds,"persistent":true,"model":style.model.is_some()})
        }).collect::<Vec<_>>();
        json!({"count":entities.len(),"entities":entities})
    }
}
