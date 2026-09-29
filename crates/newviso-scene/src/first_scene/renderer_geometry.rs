use super::*;

impl Scene3dRuntime {
    pub(super) fn build_particle_vertices(&self) -> (Vec<f32>, u32, u32) {
        if self.particles.is_empty() {
            return (Vec::new(), 0, 0);
        }

        let forward = self.camera.target.sub(self.camera.position).normalized();
        let camera_right = forward.cross(self.camera.up).normalized();
        let camera_up = camera_right.cross(forward).normalized();
        let normal = forward.mul(-1.0);

        let mut alpha = self
            .particles
            .iter()
            .filter(|particle| particle.desc.blend == SceneParticleBlend::Alpha)
            .collect::<Vec<_>>();
        alpha.sort_by(|a, b| {
            let da = Vec3::new(a.desc.position[0], a.desc.position[1], a.desc.position[2])
                .sub(self.camera.position)
                .dot(forward);
            let db = Vec3::new(b.desc.position[0], b.desc.position[1], b.desc.position[2])
                .sub(self.camera.position)
                .dot(forward);
            db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
        });
        let additive = self
            .particles
            .iter()
            .filter(|particle| particle.desc.blend == SceneParticleBlend::Additive)
            .collect::<Vec<_>>();

        let mut out = Vec::with_capacity(self.particles.len() * 6 * FLOATS_PER_VERTEX);
        let append = |particle: &SceneRuntimeParticle, out: &mut Vec<f32>| {
            let t = (particle.age_seconds / particle.desc.lifetime_seconds).clamp(0.0, 1.0);
            let size = [
                particle.desc.size[0] + (particle.desc.end_size[0] - particle.desc.size[0]) * t,
                particle.desc.size[1] + (particle.desc.end_size[1] - particle.desc.size[1]) * t,
            ];
            let color = std::array::from_fn(|i| {
                particle.desc.color[i] + (particle.desc.end_color[i] - particle.desc.color[i]) * t
            });
            let angle = particle.desc.rotation_degrees.to_radians();
            let c = angle.cos();
            let s = angle.sin();
            let right = camera_right.mul(c).add(camera_up.mul(s));
            let up = camera_up.mul(c).sub(camera_right.mul(s));
            let center = Vec3::new(
                particle.desc.position[0],
                particle.desc.position[1],
                particle.desc.position[2],
            );
            let hx = size[0] * 0.5;
            let hy = size[1] * 0.5;
            let corners = [
                center.sub(right.mul(hx)).sub(up.mul(hy)),
                center.add(right.mul(hx)).sub(up.mul(hy)),
                center.add(right.mul(hx)).add(up.mul(hy)),
                center.sub(right.mul(hx)).add(up.mul(hy)),
            ];
            for (corner, uv) in [
                (0usize, [0.0, 1.0]),
                (1, [1.0, 1.0]),
                (2, [1.0, 0.0]),
                (0, [0.0, 1.0]),
                (2, [1.0, 0.0]),
                (3, [0.0, 0.0]),
            ] {
                geometry::append_particle_vertex(out, corners[corner], normal, color, uv);
            }
        };

        for particle in &alpha {
            append(particle, &mut out);
        }
        let alpha_vertices = u32::try_from(alpha.len().saturating_mul(6)).unwrap_or(u32::MAX);
        for particle in &additive {
            append(particle, &mut out);
        }
        let additive_vertices = u32::try_from(additive.len().saturating_mul(6)).unwrap_or(u32::MAX);
        (out, alpha_vertices, additive_vertices)
    }

    pub(super) fn build_lens_flare_vertices(&self, aspect: f32) -> Vec<f32> {
        let mut out = Vec::new();
        if self.lens_flares.is_empty() {
            return out;
        }

        let forward = self.camera.target.sub(self.camera.position).normalized();
        let right = forward.cross(self.camera.up).normalized();
        let up = right.cross(forward).normalized();
        let inv_tan = 1.0
            / (self.camera.fov_y_degrees.to_radians() * 0.5)
                .tan()
                .max(0.0001);

        for flare in self.lens_flares.values() {
            if !flare.enabled || flare.intensity <= 0.0 {
                continue;
            }
            if !self.sky_visuals.contains_key(&flare.source) {
                continue;
            }
            let Some(id) = self
                .runtime_entity_ids
                .get(&flare.source)
                .copied()
                .map(SceneEntityId)
                .or_else(|| self.world.entity_id_by_name(&flare.source))
            else {
                continue;
            };
            let Some(entity) = self.world.entity(id) else {
                continue;
            };
            if entity.lifecycle != SceneLifecycle::Active {
                continue;
            }

            let direction = sky_visual_direction(entity.transform.rotation_degrees);
            let view_z = forward.dot(direction);
            if view_z <= 0.0001 {
                continue;
            }
            let source_x = (right.dot(direction) * inv_tan / aspect.max(0.0001)) / view_z;
            let source_y = (-up.dot(direction) * inv_tan) / view_z;
            if source_x.abs() > 1.05 || source_y.abs() > 1.05 {
                continue;
            }
            if flare.occlusion_test && self.direction_is_occluded(direction) {
                continue;
            }

            let edge_visibility = 1.0 - source_x.abs().max(source_y.abs()).clamp(0.0, 1.0);
            let visibility = edge_visibility * flare.intensity;
            if visibility <= 0.0001 {
                continue;
            }

            for element in flare
                .elements
                .iter()
                .take(self.render_policy.flare_element_capacity)
            {
                let center = [
                    source_x * (1.0 - element.offset),
                    source_y * (1.0 - element.offset),
                ];
                let half_y = element.size * flare.scale;
                let half_x = half_y / aspect.max(0.0001);
                append_flare_quad(
                    &mut out,
                    center,
                    [half_x, half_y],
                    element.color,
                    element.alpha * visibility,
                    match element.kind {
                        LensFlareElementKind::Halo => 0.0,
                        LensFlareElementKind::Ghost => 1.0,
                        LensFlareElementKind::Streak => 2.0,
                    },
                );
            }
        }

        out
    }

    pub(super) fn direction_is_occluded(&self, direction: Vec3) -> bool {
        let origin = self.camera.position;
        self.world
            .solid_bounds()
            .any(|bounds| ray_hits_aabb(origin, direction, bounds, self.camera.near.max(0.02)))
    }

    pub(super) fn vertex_capacity(&self) -> u32 {
        (self.cubes.len() + self.render_policy.runtime_cube_capacity) as u32 * CUBE_VERTEX_COUNT
            + self.render_policy.transient_sphere_capacity as u32 * geometry::SPHERE_VERTEX_COUNT
            + self.render_policy.overlay_quad_capacity as u32 * 6
    }
    pub(super) fn shadow_vertex_capacity(&self) -> u32 {
        (self.cubes.len() + self.render_policy.runtime_cube_capacity) as u32 * CUBE_VERTEX_COUNT
            + self.render_policy.transient_sphere_capacity as u32 * geometry::SPHERE_VERTEX_COUNT
    }
    pub(super) fn vertex_count(&self) -> u32 {
        self.frame_plan.visible_render_slots.len() as u32 * CUBE_VERTEX_COUNT
            + self.transient_spheres.len() as u32 * geometry::SPHERE_VERTEX_COUNT
            + self.overlay_quads.len() as u32 * 6
    }
    pub(super) fn shadow_vertex_count(&self) -> u32 {
        self.world.active_render_slots().len() as u32 * CUBE_VERTEX_COUNT
            + self.transient_spheres.len() as u32 * geometry::SPHERE_VERTEX_COUNT
    }
    pub(super) fn build_shadow_vertices(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.shadow_vertex_count() as usize * FLOATS_PER_VERTEX);
        for slot in self.world.active_render_slots() {
            if let Some(cube) = self.cubes.get(slot) {
                cube.append_vertices(&mut out);
            }
        }
        for sphere in &self.transient_spheres {
            geometry::append_sphere_vertices(
                Vec3::new(sphere.position[0], sphere.position[1], sphere.position[2]),
                sphere.radius,
                Vec3::new(
                    sphere.rotation_degrees[0],
                    sphere.rotation_degrees[1],
                    sphere.rotation_degrees[2],
                ),
                sphere.color,
                sphere.marker_color,
                Vec3::new(
                    sphere.marker_direction[0],
                    sphere.marker_direction[1],
                    sphere.marker_direction[2],
                ),
                sphere.marker_threshold,
                &mut out,
            );
        }
        out
    }
    pub(super) fn build_cube_vertices(&self, _aspect: f32) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.vertex_count() as usize * FLOATS_PER_VERTEX);
        for slot in &self.frame_plan.visible_render_slots {
            if let Some(cube) = self.cubes.get(*slot) {
                cube.append_vertices(&mut out);
            }
        }

        for sphere in &self.transient_spheres {
            geometry::append_sphere_vertices(
                Vec3::new(sphere.position[0], sphere.position[1], sphere.position[2]),
                sphere.radius,
                Vec3::new(
                    sphere.rotation_degrees[0],
                    sphere.rotation_degrees[1],
                    sphere.rotation_degrees[2],
                ),
                sphere.color,
                sphere.marker_color,
                Vec3::new(
                    sphere.marker_direction[0],
                    sphere.marker_direction[1],
                    sphere.marker_direction[2],
                ),
                sphere.marker_threshold,
                &mut out,
            );
        }

        for quad in &self.overlay_quads {
            let [x0, y0, x1, y1] = quad.rect;
            for [x, y] in [[x0, y0], [x1, y0], [x1, y1], [x0, y0], [x1, y1], [x0, y1]] {
                geometry::append_overlay_vertex(&mut out, x, y, quad.color);
            }
        }

        out
    }
}
