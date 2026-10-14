//! physics geometry.
use super::*;

impl Scene3dRuntime {
    pub fn focus_position(&self) -> [f32; 3] {
        let focus = self.world.focus().position;
        [focus.x, focus.y, focus.z]
    }

    pub fn solid_aabbs(&self) -> Vec<([f32; 3], [f32; 3])> {
        self.world
            .solid_bounds()
            .map(|bounds| {
                (
                    [bounds.min.x, bounds.min.y, bounds.min.z],
                    [bounds.max.x, bounds.max.y, bounds.max.z],
                )
            })
            .collect()
    }

    pub fn physics_static_solid_colliders_near(
        &self,
        interests: &[([f32; 3], [f32; 3])],
    ) -> Vec<(u64, [f32; 3], [f32; 3])> {
        let interests = interests
            .iter()
            .map(|(min, max)| SceneBounds {
                min: Vec3::new(min[0], min[1], min[2]),
                max: Vec3::new(max[0], max[1], max[2]),
            })
            .collect::<Vec<_>>();
        self.world
            .static_solid_colliders_near(&interests)
            .into_iter()
            .map(|(id, bounds)| {
                (
                    id.0,
                    [bounds.min.x, bounds.min.y, bounds.min.z],
                    [bounds.max.x, bounds.max.y, bounds.max.z],
                )
            })
            .collect()
    }

    pub fn physics_static_solid_aabbs_near(
        &self,
        interests: &[([f32; 3], [f32; 3])],
    ) -> Vec<([f32; 3], [f32; 3])> {
        self.physics_static_solid_colliders_near(interests)
            .into_iter()
            .map(|(_, min, max)| (min, max))
            .collect()
    }

    /// Imported collision meshes have their own streamed nav tiles. Only authored
    /// solid cubes may contribute their primitive surfaces here: a model's AABB
    /// must never become a fictitious floor or bridge.
    pub fn physics_static_primitive_navigation_near(
        &self,
        interests: &[([f32; 3], [f32; 3])],
    ) -> Vec<SceneNavigationGeometry> {
        self.physics_static_solid_colliders_near(interests)
            .into_iter()
            .filter_map(|(id, _, _)| {
                let entity = self.world.entity(SceneEntityId(id))?;
                (entity.asset_ref.is_none()
                    && entity.render_slot.is_some()
                    && entity.kind == SceneEntityKind::StaticMesh)
                    .then(|| primitive_navigation_geometry(id, entity.transform))
            })
            .collect()
    }
}

fn primitive_navigation_geometry(
    entity: u64,
    transform: SceneTransform,
) -> SceneNavigationGeometry {
    let corners = [
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, 0.5],
    ];
    let vertices = corners
        .into_iter()
        .map(|[x, y, z]| {
            let p = transform_point(
                Vec3::new(x, y, z),
                transform.scale,
                transform.rotation_degrees,
                transform.position,
            );
            [p.x, p.y, p.z]
        })
        .collect();
    let mut triangles = vec![
        [3, 7, 6],
        [3, 6, 2],
        [0, 1, 5],
        [0, 5, 4],
        [0, 3, 2],
        [0, 2, 1],
        [4, 5, 6],
        [4, 6, 7],
        [0, 4, 7],
        [0, 7, 3],
        [1, 2, 6],
        [1, 6, 5],
    ];
    if transform.scale.x * transform.scale.y * transform.scale.z < 0.0 {
        for triangle in &mut triangles {
            triangle.swap(1, 2);
        }
    }
    SceneNavigationGeometry {
        entity,
        vertices,
        triangles,
    }
}

#[cfg(test)]
mod primitive_navigation_tests {
    use super::*;

    #[test]
    fn cube_navigation_preserves_authored_rotation_and_outward_winding() {
        // Rotating the box 90 degrees about Z makes its right face the floor.
        // An axis-aligned bounding box would lose this actual source geometry.
        let geometry = primitive_navigation_geometry(
            42,
            SceneTransform {
                position: Vec3::new(10.0, 2.0, 3.0),
                rotation_degrees: Vec3::new(0.0, 0.0, 90.0),
                scale: Vec3::new(4.0, 2.0, 6.0),
            },
        );
        assert_eq!(geometry.entity, 42);
        let p = geometry.vertices[6];
        assert!((p[0] - 9.0).abs() < 1.0e-5 && (p[1] - 4.0).abs() < 1.0e-5);
        assert!((p[2] - 6.0).abs() < 1.0e-5);
        let [a, b, c] = geometry.triangles[10].map(|i| geometry.vertices[i as usize]);
        let up = (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]);
        assert!(up > 0.0);
    }

    #[test]
    fn mirrored_cube_keeps_walkable_surface_winding() {
        let geometry = primitive_navigation_geometry(
            1,
            SceneTransform {
                position: Vec3::ZERO,
                rotation_degrees: Vec3::ZERO,
                scale: Vec3::new(-4.0, 1.0, 4.0),
            },
        );
        let [a, b, c] = geometry.triangles[0].map(|i| geometry.vertices[i as usize]);
        let up = (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]);
        assert!(up > 0.0);
    }
}