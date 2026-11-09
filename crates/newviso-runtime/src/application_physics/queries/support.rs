//! Shared footprint support enumeration. Mesh bounds never become fake floors.
use super::super::*;
use newviso_collision::SupportSurface;
impl PhysicsRuntime {
    pub(crate) fn character_support_surfaces(
        &self,
        min: [f32; 3],
        max: [f32; 3],
        ignore: u64,
        solids: &[(u64, [f32; 3], [f32; 3])],
        mut visit: impl FnMut(u64, &SupportSurface, [f32; 3]),
    ) {
        for (id, mesh) in &self.camera_collision_meshes {
            let Some(c) = self.streamed_colliders.get(id) else {
                continue;
            };
            if *id == ignore
                || c.flags.is_trigger
                || !c.flags.participates_in_queries
                || (0..3).any(|k| min[k] > c.bounds_max[k] || max[k] < c.bounds_min[k])
            {
                continue;
            }
            let local_min = std::array::from_fn(|i| min[i] - c.position[i]);
            let local_max = std::array::from_fn(|i| max[i] - c.position[i]);
            for surface in mesh.support_surfaces_near(local_min, local_max) {
                visit(*id, &surface, c.position);
            }
        }
        for b in self.bodies.values() {
            if b.entity == ignore
                || b.flags.is_trigger
                || !b.flags.participates_in_queries
                || (0..3).any(|k| min[k] > b.bounds_max[k] || max[k] < b.bounds_min[k])
            {
                continue;
            }
            // Curved shapes cannot support an entire planar sole by their enclosing AABB.
            let CollisionShape::Box { half_extents: h } = b.shape else {
                continue;
            };
            for sign in [-1.0, 1.0] {
                for axis in 0..3 {
                    let a = (axis + 1) % 3;
                    let z = (axis + 2) % 3;
                    let corners = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]].map(|s| {
                        let mut p = [0.0; 3];
                        p[axis] = sign * h[axis];
                        p[a] = s[0] * h[a];
                        p[z] = s[1] * h[z];
                        rotate_camera_vector(p, b.rotation)
                    });
                    if let Some(surface) = SupportSurface::from_quad(
                        (axis * 2 + usize::from(sign > 0.0)) as u32,
                        corners,
                    ) {
                        // Only outward-facing upper faces, not the underside seen from below.
                        let mut up = [0.0; 3];
                        up[axis] = sign;
                        if rotate_camera_vector(up, b.rotation)[1] > 0.1 {
                            visit(b.entity, &surface, b.position);
                        }
                    }
                }
            }
        }
        if self.settings.scene_colliders_enabled && self.settings.scene_participates_in_queries {
            for &(id, a, b) in solids {
                if id == ignore
                    || self.streamed_colliders.contains_key(&id)
                    || self.bodies.contains_key(&id)
                    || (0..3).any(|k| min[k] > b[k] || max[k] < a[k])
                {
                    continue;
                }
                let corners = [
                    [a[0], b[1], a[2]],
                    [b[0], b[1], a[2]],
                    [b[0], b[1], b[2]],
                    [a[0], b[1], b[2]],
                ];
                if let Some(surface) = SupportSurface::from_quad(0, corners) {
                    visit(id, &surface, [0.0; 3]);
                }
            }
        }
    }
}

impl PhysicsRuntime {
    pub(crate) fn character_foot_support_visible(
        &self,
        corners: [[f32; 3]; 4],
        ignore: u64,
        solids: &[(u64, [f32; 3], [f32; 3])],
        skin: f32,
    ) -> bool {
        let primitives = solids
            .iter()
            .filter(|(id, _, _)| {
                !self.streamed_colliders.contains_key(id) && !self.bodies.contains_key(id)
            })
            .map(|(_, a, b)| (*a, *b))
            .collect::<Vec<_>>();
        let skin = skin + self.settings.contact_skin;
        let probe = skin.max(0.001) * 2.0;
        corners.into_iter().all(|p| {
            let origin = [p[0], p[1] + probe, p[2]];
            self.character_sweep_sphere(
                origin,
                [0.0, -2.0 * probe, 0.0],
                0.0,
                Some(ignore),
                &primitives,
            )
            .is_some_and(|h| (probe - 2.0 * probe * h.fraction).abs() <= skin && h.normal[1] > 0.1)
        })
    }
}

impl PhysicsRuntime {
    /// Vertical non-penetration bound for the actual rolled sole, including an
    /// upcoming riser underneath a toe. Empty space is not reported as contact.
    pub(crate) fn character_foot_clearance(
        &self,
        corners: [[f32; 3]; 4],
        ignore: u64,
        solids: &[(u64, [f32; 3], [f32; 3])],
        cfg: newviso_character::CharacterConfig,
    ) -> Option<(f32, [f32; 3], Option<u64>, [f32; 3])> {
        let primitives = solids
            .iter()
            .filter(|(id, _, _)| {
                !self.streamed_colliders.contains_key(id) && !self.bodies.contains_key(id)
            })
            .map(|(_, a, b)| (*a, *b))
            .collect::<Vec<_>>();
        let body = self.vehicle_body_state(ignore)?;
        let base = body.position[1] - cfg.radius - cfg.half_height;
        let mut required: Option<(f32, [f32; 3], Option<u64>, [f32; 3])> = None;
        for p in corners {
            let origin = [p[0], p[1].max(base + cfg.step_height) + cfg.skin, p[2]];
            let down = origin[1] - (p[1].min(base) - cfg.ground_snap_distance - cfg.skin);
            if down <= 0.0 {
                continue;
            }
            if let Some(hit) = self
                .character_sweep_sphere(origin, [0.0, -down, 0.0], 0.0, Some(ignore), &primitives)
                .filter(|h| h.normal[1] >= cfg.max_slope_degrees.to_radians().cos())
            {
                let point = [p[0], origin[1] - down * hit.fraction, p[2]];
                let delta = point[1] - p[1];
                if required.is_none_or(|old| delta > old.0) {
                    required = Some((delta, hit.normal, hit.entity, point));
                }
            }
        }
        required
    }
}

#[cfg(test)]
mod natural_foot_tests {
    use super::*;
    #[test]
    fn trailing_sole_can_keep_a_lower_tread_beyond_capsule_snap_distance() {
        let mut physics = PhysicsRuntime::empty_for_tests();
        physics.settings.scene_colliders_enabled = true;
        physics.settings.scene_participates_in_queries = true;
        physics
            .upsert_body_from_script(
                &json!({"entity":3,"body_kind":"dynamic","position":[0,1.5,0],
            "shape":{"kind":"capsule","radius":0.3,"half_height":0.6}}),
                0,
            )
            .unwrap();
        let cfg = newviso_character::CharacterConfig {
            radius: 0.3,
            half_height: 0.6,
            ..Default::default()
        };
        let corners = [
            [-0.05, 0.18, -0.1],
            [0.05, 0.18, -0.1],
            [0.05, 0.18, 0.1],
            [-0.05, 0.18, 0.1],
        ];
        let bound = physics
            .character_foot_clearance(corners, 3, &[(1, [-1.0, 0.0, -1.0], [1.0, 0.18, 1.0])], cfg)
            .unwrap();
        assert!(bound.0.abs() < 1e-6);
        assert!((bound.3[1] - 0.18).abs() < 1e-6);
    }
}

impl PhysicsRuntime {
    /// Continuous contact of the sole's moving corners. A riser blocks forward
    /// travel while leaving upward swing available; it never becomes a request
    /// to teleport the ankle to the top of the step.
    pub(crate) fn character_foot_sweep(
        &self,
        previous: [[f32; 3]; 4],
        proposed: [[f32; 3]; 4],
        ignore: u64,
        solids: &[(u64, [f32; 3], [f32; 3])],
        iterations: u32,
    ) -> [f32; 3] {
        let primitives = solids
            .iter()
            .filter(|(id, _, _)| {
                !self.streamed_colliders.contains_key(id) && !self.bodies.contains_key(id)
            })
            .map(|(_, a, b)| (*a, *b))
            .collect::<Vec<_>>();
        let skin = self.settings.contact_skin.max(0.0001);
        if previous
            .iter()
            .zip(proposed)
            .all(|(a, b)| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>() < 1e-14)
        {
            return [0.0; 3];
        }
        let mut correction = [0.0; 3];
        for _ in 0..iterations {
            let mut best: Option<(newviso_character::SweepHit, [f32; 3])> = None;
            for (old, new) in previous.into_iter().zip(proposed) {
                let origin = [old[0], old[1] + skin, old[2]];
                let delta = std::array::from_fn(|k| new[k] + correction[k] - old[k]);
                if let Some(hit) = self
                    .character_sweep_sphere(origin, delta, 0.0, Some(ignore), &primitives)
                    .filter(|hit| (0..3).map(|k| delta[k] * hit.normal[k]).sum::<f32>() < -1e-7)
                {
                    if best.is_none_or(|(old, _)| hit.fraction < old.fraction) {
                        best = Some((hit, delta));
                    }
                }
            }
            let Some((hit, delta)) = best else { break };
            let depth = -(0..3).map(|k| delta[k] * hit.normal[k]).sum::<f32>()
                * (1.0 - hit.fraction)
                + skin;
            for k in 0..3 {
                correction[k] += hit.normal[k] * depth;
            }
        }
        correction
    }
}
#[cfg(test)]
mod sole_sweep_tests {
    use super::*;
    #[test]
    fn a_riser_blocks_horizontal_swing_without_lifting_it_to_the_tread() {
        let mut physics = PhysicsRuntime::empty_for_tests();
        physics.settings.scene_colliders_enabled = true;
        physics.settings.scene_participates_in_queries = true;
        let previous = [
            [-0.05, 0.10, 0.02],
            [0.05, 0.10, 0.02],
            [0.05, 0.10, 0.22],
            [-0.05, 0.10, 0.22],
        ];
        let proposed = previous.map(|p| [p[0], p[1], p[2] - 0.04]);
        let solids = [(1, [-1.0, 0.0, -1.0], [1.0, 0.18, 0.0])];
        let correction = physics.character_foot_sweep(previous, proposed, 3, &solids, 5);
        assert!(correction[2] > 0.02);
        assert!(correction[1].abs() < 1e-6);
        assert!(proposed.iter().all(|p| p[2] + correction[2] >= 0.0));
        // A sub-millimetre toe crossing still hits the full riser. Query skin
        // is a separation margin, never a minimum swept travel distance.
        let edge = previous.map(|p| [p[0], p[1], 0.00005]);
        let crossed = edge.map(|p| [p[0], p[1], -0.00005]);
        let tiny = physics.character_foot_sweep(edge, crossed, 3, &solids, 5);
        assert!(tiny[2] > 0.00005 && tiny[1].abs() < 1e-6);
        let clear = previous.map(|p| [p[0], 0.3, p[2]]);
        let next = clear.map(|p| [p[0], p[1], p[2] - 0.04]);
        assert_eq!(
            physics.character_foot_sweep(clear, next, 3, &solids, 5),
            [0.0; 3]
        );
    }
}
