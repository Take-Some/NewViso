use super::*;

impl Scene3dRuntime {
    pub(super) fn vertex_capacity(&self) -> u32 {
        self.cubes.len() as u32 * CUBE_VERTEX_COUNT
            + MAX_TRANSIENT_SPHERES as u32 * geometry::SPHERE_VERTEX_COUNT
            + MAX_OVERLAY_QUADS as u32 * 6
    }
    pub(super) fn shadow_vertex_capacity(&self) -> u32 {
        self.cubes.len() as u32 * CUBE_VERTEX_COUNT
            + MAX_TRANSIENT_SPHERES as u32 * geometry::SPHERE_VERTEX_COUNT
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
