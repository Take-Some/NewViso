use super::*;

/// Source-neutral sprite appearance. Curves use normalized particle age.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneParticleStyle {
    /// Solid debris is promoted to an independent rigid body when physics is active.
    pub physical_debris_density: Option<f32>,
    pub effect_name: Option<String>,
    pub emitter_name: Option<String>,
    pub model: Option<std::sync::Arc<SceneParticleMesh>>,
    pub model_basis: [[f32; 3]; 3],
    pub model_rotation: [f32; 3],
    pub model_spin: [f32; 3],
    pub depth_keys: Vec<(f32, [f32; 1])>,
    pub trail: bool,
    pub trail_group: Option<u64>,
    pub texture_ref: Option<String>,
    pub texture_grid: [u32; 2],
    pub first_frame: u32,
    pub last_frame: u32,
    pub animation_rate: f32,
    pub animate_over_life: bool,
    pub loop_animation: bool,
    pub loop_start_frame: Option<u32>,
    pub loop_random_start: Option<[u32; 2]>,
    pub animation_seed: u32,
    pub diffuse_mode: u32,
    pub delay_seconds: f32,
    pub color_keys: Vec<(f32, [f32; 4])>,
    pub size_keys: Vec<(f32, [f32; 2])>,
    pub motion_rate: Option<f32>,
    pub acceleration_keys: Vec<(f32, [f32; 3])>,
    pub drag_keys: Vec<(f32, [f32; 3])>,
    /// Restitution, radius multiplier, minimum radius and rest speed.
    pub collision: Option<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneParticleMesh {
    /// Source-space positions normalized to the authored bounds, normals and UVs.
    pub vertices: Vec<[f32; 8]>,
    pub texture_ref: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct ParticleDrawBatch {
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub blend: SceneParticleBlend,
    pub material: Option<(String, u32)>,
}

pub(super) fn sample_particle_curve<const N: usize>(
    keys: &[(f32, [f32; N])],
    t: f32,
    fallback: [f32; N],
) -> [f32; N] {
    let Some(first) = keys.first() else {
        return fallback;
    };
    if t <= first.0 {
        return first.1;
    }
    for pair in keys.windows(2) {
        if t <= pair[1].0 {
            let alpha = ((t - pair[0].0) / (pair[1].0 - pair[0].0).max(1.0e-6)).clamp(0.0, 1.0);
            return std::array::from_fn(|i| pair[0].1[i] + (pair[1].1[i] - pair[0].1[i]) * alpha);
        }
    }
    keys.last().unwrap().1
}

impl SceneParticleStyle {
    pub(super) fn valid(&self) -> bool {
        fn curve<const N: usize>(keys: &[(f32, [f32; N])]) -> bool {
            keys.len() <= 128
                && keys.iter().all(|(t, v)| {
                    t.is_finite() && (0.0..=1.0).contains(t) && v.iter().all(|x| x.is_finite())
                })
                && keys.windows(2).all(|pair| pair[0].0 < pair[1].0)
        }
        self.animation_rate.is_finite()
            && self
                .physical_debris_density
                .is_none_or(|v| v.is_finite() && v > 0.0 && v <= 20000.0)
            && self.animation_rate >= 0.0
            && self.delay_seconds.is_finite()
            && (0.0..=120.0).contains(&self.delay_seconds)
            && self.first_frame <= self.last_frame
            && self.loop_start_frame.is_none_or(|f| f <= self.last_frame)
            && self
                .loop_random_start
                .is_none_or(|r| r[0] <= r[1] && r[1] <= self.last_frame)
            && curve(&self.color_keys)
            && curve(&self.size_keys)
            && curve(&self.depth_keys)
            && curve(&self.acceleration_keys)
            && curve(&self.drag_keys)
            && self.motion_rate.is_none_or(|r| r.is_finite() && r > 0.0)
            && self
                .collision
                .is_none_or(|c| c.iter().all(|v| v.is_finite() && *v >= 0.0) && c[0] <= 1.0)
            && self
                .model_basis
                .iter()
                .flatten()
                .chain(self.model_rotation.iter())
                .chain(self.model_spin.iter())
                .all(|v| v.is_finite())
            && self.model.as_ref().is_none_or(|mesh| {
                !mesh.vertices.is_empty()
                    && mesh.vertices.len() <= 4096
                    && mesh.vertices.len() % 3 == 0
                    && mesh.vertices.iter().flatten().all(|v| v.is_finite())
            })
    }

    pub(super) fn uv(&self, uv: [f32; 2], age: f32, lifetime: f32) -> [f32; 2] {
        let columns = self.texture_grid[0].max(1);
        let rows = self.texture_grid[1].max(1);
        let count = self
            .last_frame
            .saturating_sub(self.first_frame)
            .saturating_add(1)
            .max(1);
        let advance = if self.animate_over_life {
            (age / lifetime.max(0.001) * count as f32).max(0.0) as u32
        } else {
            (age * self.animation_rate).max(0.0) as u32
        };
        let mut frame = self.first_frame.saturating_add(advance.min(count - 1));
        if self.loop_animation && advance >= count {
            let start = self
                .loop_start_frame
                .unwrap_or(self.first_frame)
                .min(self.last_frame);
            let mut remaining = advance - count;
            if let Some([min, max]) = self.loop_random_start {
                let mut seed = self.animation_seed;
                // Bounded by validated particle lifetime/rate; protect malformed assets too.
                for _ in 0..8192 {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let start = min + seed % (max - min + 1);
                    let span = self.last_frame - start + 1;
                    frame = start + remaining.min(span - 1);
                    if remaining < span {
                        break;
                    }
                    remaining -= span;
                }
            } else {
                frame = start + remaining % (self.last_frame - start + 1);
            }
        }
        let frame = frame.min(columns.saturating_mul(rows).saturating_sub(1));
        [
            (frame % columns) as f32 / columns as f32 + uv[0] / columns as f32,
            (frame / columns) as f32 / rows as f32 + uv[1] / rows as f32,
        ]
    }
}

impl Scene3dRuntime {
    pub(super) fn particle_runtime_state(&self) -> Value {
        let mut effects = BTreeMap::<String, Value>::new();
        for p in &self.particles {
            let Some(style) = p.desc.style.as_ref() else {
                continue;
            };
            let Some(name) = style.effect_name.as_ref() else {
                continue;
            };
            let stats = effects.entry(name.clone()).or_insert_with(|| {
                json!({"alive":0,"active":0,"models":0,
                "max_width":0.0,"max_height":0.0,"max_lifetime":0.0,"oldest_seconds":0.0,"resting":0})
            });
            stats["alive"] = json!(stats["alive"].as_u64().unwrap_or(0) + 1);
            if p.age_seconds >= 0.0 {
                stats["active"] = json!(stats["active"].as_u64().unwrap_or(0) + 1);
            }
            if style.collision.is_some()
                && p.age_seconds > 0.25
                && p.desc.velocity.iter().map(|v| v * v).sum::<f32>() < 0.0001
            {
                stats["resting"] = json!(stats["resting"].as_u64().unwrap_or(0) + 1);
            }
            if style.model.is_some() {
                stats["models"] = json!(stats["models"].as_u64().unwrap_or(0) + 1);
            }
            let age = (p.age_seconds / p.desc.lifetime_seconds).clamp(0.0, 1.0);
            let fallback = std::array::from_fn(|i| {
                p.desc.size[i] + (p.desc.end_size[i] - p.desc.size[i]) * age
            });
            let size = sample_particle_curve(&style.size_keys, age, fallback);
            for (key, value) in [
                ("max_width", size[0]),
                ("max_height", size[1]),
                ("max_lifetime", p.desc.lifetime_seconds),
                ("oldest_seconds", p.age_seconds.max(0.0)),
            ] {
                stats[key] = json!((stats[key].as_f64().unwrap_or(0.0) as f32).max(value));
            }
        }
        json!({"alive": self.particles.len(), "textures": self.particle_textures.len(),
        "models": self.particles.iter().filter(|p| p.desc.style.as_ref().is_some_and(|s| s.model.is_some())).count(),
        "trails": self.particles.iter().filter(|p| p.desc.style.as_ref().is_some_and(|s| s.trail)).count(),
        "effects": effects,
        "physical_debris": self.physical_particle_runtime_state(),
        "interior_occlusion": {
            "volumes":self.particle_interiors.iter().filter(|(id,_)|self.world.entity(SceneEntityId(**id)).is_some())
                .map(|(id,b)|json!({"entity":id,"min":b.min,"max":b.max,"extra_planes":b.planes})).collect::<Vec<_>>(),
            "clipped_triangles":self.particle_interior_stats.clipped_triangles,
            "rendered_triangles":self.particle_interior_stats.rendered_triangles,
            "interior_triangles":self.particle_interior_stats.interior_triangles,
            "swept_contacts":self.particle_interior_contacts
        }})
    }

    pub(super) fn ensure_particle_vertex_capacity(
        &mut self,
        render: &RenderClient,
        slot: usize,
        required: usize,
    ) -> Result<(), String> {
        let mut gpu = self.gpu.ok_or("particle GPU resources missing")?;
        if required > gpu.particle_vertex_capacities[slot] as usize {
            let next = u32::try_from(required)
                .map_err(|_| "particle vertex count overflow")?
                .checked_next_power_of_two()
                .ok_or("particle capacity overflow")?;
            let buffer = render.create_frame_buffer(
                slot,
                &format!("newviso.particles.grown.slot.{slot}"),
                next as u64 * VERTEX_STRIDE,
                "Vertex",
                "CpuToGpu",
            )?;
            render.destroy_buffer(gpu.particle_vertex_buffers[slot]);
            gpu.particle_vertex_buffers[slot] = buffer;
            gpu.particle_vertex_capacities[slot] = next;
            self.gpu = Some(gpu);
        }
        Ok(())
    }
    pub fn particle_texture_registered(&self, reference: &str) -> bool {
        self.particle_textures.contains_key(reference)
    }

    pub fn register_particle_texture(
        &mut self,
        reference: &str,
        texture: SkyTextureResources,
    ) -> Result<(), String> {
        if texture.width == 0
            || texture.height == 0
            || texture.rgba8.len() != texture.width as usize * texture.height as usize * 4
        {
            return Err(format!("particle texture '{reference}' has invalid pixels"));
        }
        self.particle_textures
            .entry(reference.to_owned())
            .or_insert(texture);
        Ok(())
    }

    pub(super) fn sync_particle_material_gpu(&mut self) -> Result<(), String> {
        let Some(gpu) = self.gpu else { return Ok(()) };
        let render = RenderClient::new();
        let materials = self
            .particles
            .iter()
            .filter_map(|p| {
                let style = p.desc.style.as_ref()?;
                Some((style.texture_ref.clone()?, style.diffuse_mode))
            })
            .collect::<BTreeSet<_>>();
        for key in materials {
            if self.particle_gpu_materials.contains_key(&key) {
                continue;
            }
            let texture = if let Some(&handle) = self.particle_gpu_textures.get(&key.0) {
                handle
            } else {
                let source = self
                    .particle_textures
                    .get(&key.0)
                    .ok_or_else(|| format!("particle texture '{}' was not registered", key.0))?;
                let handle = upload_sky_texture(
                    &render,
                    &format!("newviso.particle.{}", source.name),
                    source,
                )?;
                self.particle_gpu_textures.insert(key.0.clone(), handle);
                handle
            };
            let uniform_buffer =
                render.create_buffer("newviso.particle.material", 48, "Uniform", "CpuToGpu")?;
            // Particle bit selects unlit textured shading in the scene shader.
            let params = [
                0.0,
                0.0,
                1.0,
                0.0,
                0.0,
                0.0,
                (16_384 | 16 | 64) as f32,
                1.0,
                0.0,
                key.1 as f32,
                0.0,
                0.0,
            ];
            render.write_buffer_f32(uniform_buffer, 0, &params)?;
            let bind_group = match render.create_bind_group6(
                "newviso.particle.material.bind",
                gpu.material_bind_group_layout,
                [
                    Some(texture),
                    Some(gpu.default_normal_texture),
                    Some(gpu.default_specular_texture),
                    Some(gpu.default_emissive_texture),
                    Some(gpu.default_environment_texture),
                    None,
                ],
                Some(gpu.material_sampler),
                Some((uniform_buffer, 0, 48)),
            ) {
                Ok(value) => value,
                Err(error) => {
                    render.destroy_buffer(uniform_buffer);
                    return Err(error);
                }
            };
            self.particle_gpu_materials.insert(
                key,
                GpuAssetMaterial {
                    uniform_buffer,
                    bind_group,
                },
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smoke_remains_visible_between_transparent_endpoints() {
        let keys = [(0.0, [0.0]), (0.25, [0.8]), (1.0, [0.0])];
        assert_eq!(sample_particle_curve(&keys, 0.25, [0.0]), [0.8]);
        assert!(sample_particle_curve(&keys, 0.5, [0.0])[0] > 0.5);
    }
    #[test]
    fn atlas_animation_clamps_instead_of_restarting() {
        let style = SceneParticleStyle {
            texture_grid: [4, 2],
            first_frame: 2,
            last_frame: 7,
            animation_rate: 4.0,
            ..Default::default()
        };
        assert_eq!(style.uv([0.0, 0.0], 100.0, 2.0), [0.75, 0.5]);
        assert_eq!(style.uv([0.0, 0.0], 0.0, 2.0), [0.5, 0.0]);
    }
}

#[cfg(test)]
mod source_atlas_tests {
    use super::*;
    #[test]
    fn zero_loop_mode_returns_to_frame_zero_after_initial_random_frame() {
        let style = SceneParticleStyle {
            texture_grid: [4, 2],
            first_frame: 2,
            last_frame: 7,
            animation_rate: 4.0,
            loop_animation: true,
            loop_start_frame: Some(0),
            ..Default::default()
        };
        assert_eq!(style.uv([0.0, 0.0], 1.5, 4.0), [0.0, 0.0]);
        assert_eq!(style.uv([0.0, 0.0], 1.75, 4.0), [0.25, 0.0]);
        assert_eq!(style.uv([0.0, 0.0], 3.5, 4.0), [0.0, 0.0]);
    }
}

/// Swept sphere against resident solid bounds: contact surfaces stop fast shards
/// from crossing the ground in one frame. Static track boxes remain exact here.
pub(super) fn particle_surface_contact(
    from: [f32; 3],
    to: [f32; 3],
    radius: f32,
    solids: &[SceneBounds],
) -> Option<([f32; 3], [f32; 3])> {
    let delta: [f32; 3] = std::array::from_fn(|i| to[i] - from[i]);
    let mut best = None::<(f32, [f32; 3])>;
    for bounds in solids {
        let min = [
            bounds.min.x - radius,
            bounds.min.y - radius,
            bounds.min.z - radius,
        ];
        let max = [
            bounds.max.x + radius,
            bounds.max.y + radius,
            bounds.max.z + radius,
        ];
        if (0..3).all(|i| from[i] > min[i] + 0.0001 && from[i] < max[i] - 0.0001) {
            continue;
        }
        let mut enter = 0.0f32;
        let mut exit = 1.0f32;
        let mut normal = [0.0; 3];
        let mut miss = false;
        for axis in 0..3 {
            if delta[axis].abs() < 1.0e-8 {
                if from[axis] < min[axis] || from[axis] > max[axis] {
                    miss = true;
                    break;
                }
                continue;
            }
            let a = (min[axis] - from[axis]) / delta[axis];
            let b = (max[axis] - from[axis]) / delta[axis];
            let near = a.min(b);
            let far = a.max(b);
            if near >= enter {
                enter = near;
                normal = [0.0; 3];
                normal[axis] = if delta[axis] > 0.0 { -1.0 } else { 1.0 };
            }
            exit = exit.min(far);
            if enter > exit {
                miss = true;
                break;
            }
        }
        if !miss
            && (0.0..=1.0).contains(&enter)
            && normal.iter().any(|v| *v != 0.0)
            && best.is_none_or(|(t, _)| enter < t)
        {
            best = Some((enter, normal));
        }
    }
    best.map(|(time, normal)| {
        (
            std::array::from_fn(|i| from[i] + delta[i] * time + normal[i] * 0.0001),
            normal,
        )
    })
}

#[cfg(test)]
mod particle_collision_tests {
    use super::*;
    #[test]
    fn fast_shard_sweep_hits_ground_before_it_crosses_the_floor() {
        let floor = SceneBounds {
            min: Vec3::new(-10.0, -1.0, -10.0),
            max: Vec3::new(10.0, 0.0, 10.0),
        };
        let (hit, normal) =
            particle_surface_contact([0.0, 2.0, 0.0], [0.0, -4.0, 0.0], 0.02, &[floor]).unwrap();
        assert!(hit[1] >= 0.02 && hit[1] < 0.021);
        assert_eq!(normal, [0.0, 1.0, 0.0]);
    }
    #[test]
    fn shard_outside_surface_bounds_does_not_hit_an_infinite_plane() {
        let floor = SceneBounds {
            min: Vec3::new(-1.0, -1.0, -1.0),
            max: Vec3::new(1.0, 0.0, 1.0),
        };
        assert!(
            particle_surface_contact([3.0, 2.0, 0.0], [3.0, -4.0, 0.0], 0.02, &[floor]).is_none()
        );
    }
}
