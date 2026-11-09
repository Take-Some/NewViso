//! Geometry evidence for admission of a proposed get-up placement.
use super::super::*;

impl PhysicsRuntime {
    pub(crate) fn endorphin_getup_ground(
        &self,
        origin: [f32; 3],
        drop: f32,
        ignore: &[u64],
        solids: &[([f32; 3], [f32; 3])],
        minimum_cosine: f32,
    ) -> Option<f32> {
        if !origin.iter().all(|v| v.is_finite())
            || !drop.is_finite()
            || drop <= 0.0
            || !minimum_cosine.is_finite()
            || !(0.0..=1.0).contains(&minimum_cosine)
        {
            return None;
        }
        let hit =
            self.character_sweep_sphere_ignoring(origin, [0.0, -drop, 0.0], 0.0, ignore, solids)?;
        (hit.normal[1] >= minimum_cosine && hit.fraction.is_finite())
            .then_some(origin[1] - drop * hit.fraction)
    }

    pub(crate) fn endorphin_getup_root_path_clear(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        ignore: &[u64],
        solids: &[([f32; 3], [f32; 3])],
    ) -> bool {
        if !from.iter().chain(&to).all(|v| v.is_finite()) {
            return false;
        }
        let delta: [f32; 3] = std::array::from_fn(|i| to[i] - from[i]);
        let length = delta.iter().map(|v| v * v).sum::<f32>().sqrt();
        if !length.is_finite() {
            return false;
        }
        if length <= f32::EPSILON {
            return true;
        }
        let epsilon = 8.0 * f32::EPSILON * length;
        self.character_sweep_sphere_ignoring_filtered(from, delta, 0.0, ignore, solids, |hit| {
            !(hit.fraction == 0.0
                && (0..3).map(|i| delta[i] * hit.normal[i]).sum::<f32>() > epsilon)
        })
        .is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime() -> PhysicsRuntime {
        let mut r = PhysicsRuntime::empty_for_tests();
        r.settings.scene_colliders_enabled = true;
        r.settings.scene_participates_in_queries = true;
        r
    }
    #[test]
    fn proposed_placement_requires_a_real_walkable_floor() {
        let r = runtime();
        let floor = [([-2.0, -0.1, -2.0], [2.0, 0.0, 2.0])];
        assert_eq!(
            r.endorphin_getup_ground([0.0, 1.0, 0.0], 2.0, &[], &floor, 0.65),
            Some(0.0)
        );
        assert!(r
            .endorphin_getup_ground([3.0, 1.0, 0.0], 2.0, &[], &floor, 0.65)
            .is_none());
        let wall = [([0.0, -1.0, -1.0], [0.1, 2.0, 1.0])];
        assert!(r
            .endorphin_getup_ground([0.05, 1.0, 0.0], 2.0, &[], &wall, 0.65)
            .is_none());
        assert!(r
            .endorphin_getup_ground([0.0, 1.0, 0.0], f32::NAN, &[], &floor, 0.65)
            .is_none());
    }
    #[test]
    fn a_clear_capsule_destination_does_not_allow_crossing_a_root_barrier() {
        let mut r = runtime();
        r.upsert_body_from_script(
            &json!({"entity":7,"body_kind":"static",
            "position":[0,1,0],"shape":{"kind":"box","half_extents":[0.1,1,1]}}),
            0,
        )
        .unwrap();
        assert!(!r.endorphin_getup_root_path_clear([-1.0, 0.5, 0.0], [1.0, 0.5, 0.0], &[], &[]));
        assert!(r.endorphin_getup_root_path_clear([-1.0, 0.5, 0.0], [1.0, 0.5, 0.0], &[7], &[]));
        r.bodies.get_mut(&7).unwrap().flags.is_trigger = true;
        assert!(r.endorphin_getup_root_path_clear([-1.0, 0.5, 0.0], [1.0, 0.5, 0.0], &[], &[]));
    }
    #[test]
    fn separating_initial_root_contact_cannot_hide_a_later_wall() {
        let r = runtime();
        let initial = ([-1.0, 0.0, -1.0], [0.0, 1.0, 1.0]);
        let wall = ([0.5, 0.0, -1.0], [0.6, 1.0, 1.0]);
        let from = [-0.0001, 0.5, 0.0];
        let to = [1.0, 0.5, 0.0];
        assert!(r.endorphin_getup_root_path_clear(from, to, &[], &[initial]));
        assert!(!r.endorphin_getup_root_path_clear(from, to, &[], &[initial, wall]));
        assert!(!r.endorphin_getup_root_path_clear([f32::NAN, 0.0, 0.0], to, &[], &[]));
    }
}

impl PhysicsRuntime {
    pub(crate) fn endorphin_recovery_support_frame(
        &self,
        entity: u64,
    ) -> Option<newviso_endorphin::BodyPose> {
        let b = self.bodies.get(&entity)?;
        if b.flags.is_trigger || !b.flags.participates_in_queries {
            return None;
        }
        Some(newviso_endorphin::BodyPose {
            position: b.position,
            rotation: b.rotation,
            linear_velocity: b.linear_velocity,
            angular_velocity: b.angular_velocity,
        })
    }
    pub(crate) fn endorphin_getup_surface(
        &self,
        origin: [f32; 3],
        drop: f32,
        ignore: &[u64],
        solids: &[([f32; 3], [f32; 3])],
        minimum_cosine: f32,
    ) -> Option<newviso_endorphin::StepGround> {
        if !origin.iter().all(|v| v.is_finite())
            || !drop.is_finite()
            || drop <= 0.0
            || !minimum_cosine.is_finite()
            || !(0.0..=1.0).contains(&minimum_cosine)
        {
            return None;
        }
        let hit =
            self.character_sweep_sphere_ignoring(origin, [0.0, -drop, 0.0], 0.0, ignore, solids)?;
        if hit.normal[1] < minimum_cosine || !hit.fraction.is_finite() {
            return None;
        }
        let ground = newviso_endorphin::StepGround {
            position: [origin[0], origin[1] - drop * hit.fraction, origin[2]],
            normal: hit.normal,
            velocity: [0.0; 3],
            support: None,
        };
        if let Some((entity, frame)) = hit
            .entity
            .and_then(|e| self.endorphin_recovery_support_frame(e).map(|p| (e, p)))
        {
            ground.bind(entity, frame, 0.0)
        } else {
            Some(ground)
        }
    }
    /// Sweep the entire standing capsule envelope, not just its destination.
    pub(crate) fn endorphin_getup_carry_clear(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        radius: f32,
        half_height: f32,
        ignore: &[u64],
        solids: &[([f32; 3], [f32; 3])],
    ) -> bool {
        let delta: [f32; 3] = std::array::from_fn(|i| to[i] - from[i]);
        let length = newviso_endorphin::math::length(delta);
        if !from.iter().chain(&to).all(|v| v.is_finite())
            || !length.is_finite()
            || !radius.is_finite()
            || radius <= 0.0
            || !half_height.is_finite()
            || half_height < 0.0
        {
            return false;
        }
        if length <= f32::EPSILON {
            return true;
        }
        let skin = self.contact_skin();
        let epsilon = 8.0 * f32::EPSILON * length;
        let query_radius = (radius - skin).max(f32::EPSILON);
        let clear = |height, swept_radius| {
            self.character_sweep_sphere_ignoring_filtered(
                [from[0], from[1] + radius + skin + height, from[2]],
                delta,
                swept_radius,
                ignore,
                solids,
                |hit| {
                    !(hit.fraction == 0.0
                        && newviso_endorphin::math::dot(delta, hit.normal) > epsilon)
                },
            )
            .is_none()
        };
        if !clear(0.0, query_radius) || !clear(2.0 * half_height, query_radius) {
            return false;
        }
        if half_height <= f32::EPSILON {
            return true;
        }
        // Cover the entire cylinder between the two spherical caps. Each
        // interval's midpoint sphere has enough extra radius to contain the
        // cylinder's full cross-section even at the interval endpoints.
        // Three uninflated spine probes leave gaps around thin ledges.
        let intervals = ((2.0 * half_height / (query_radius * 0.5)).ceil() as usize).clamp(1, 64);
        let step = 2.0 * half_height / intervals as f32;
        let cover_radius = query_radius.hypot(0.5 * step);
        if !step.is_finite() || !cover_radius.is_finite() {
            return false;
        }
        (0..intervals).all(|i| clear((i as f32 + 0.5) * step, cover_radius))
    }
}

#[cfg(test)]
mod moving_recovery_tests {
    use super::*;
    #[test]
    fn recovery_surface_binds_the_real_body_and_measures_angular_point_velocity() {
        let mut r = PhysicsRuntime::empty_for_tests();
        r.upsert_body_from_script(
            &json!({"entity":7,"body_kind":"kinematic","position":[0,2.9,0],
            "linear_velocity":[1,0.2,0.3],"angular_velocity":[0,0.5,0],
            "shape":{"kind":"box","half_extents":[4,0.1,4]}}),
            0,
        )
        .unwrap();
        let g = r
            .endorphin_getup_surface([1.0, 4.0, 0.0], 2.0, &[], &[], 0.65)
            .unwrap();
        assert_eq!(g.support.as_ref().unwrap().entity, 7);
        assert!((g.position[1] - 3.0).abs() < 0.00001);
        let frame = r.endorphin_recovery_support_frame(7).unwrap();
        let expected = newviso_endorphin::math::add(
            frame.linear_velocity,
            newviso_endorphin::math::cross(
                frame.angular_velocity,
                newviso_endorphin::math::sub(g.position, frame.position),
            ),
        );
        assert!(
            newviso_endorphin::math::length(newviso_endorphin::math::sub(g.velocity, expected))
                < 0.00001
        );
        r.bodies.get_mut(&7).unwrap().flags.participates_in_queries = false;
        assert!(r.endorphin_recovery_support_frame(7).is_none());
        assert!(r
            .endorphin_getup_surface([1.0, 4.0, 0.0], 2.0, &[], &[], 0.65)
            .is_none());
    }
    #[test]
    fn a_clear_carried_destination_cannot_teleport_the_capsule_through_a_thin_wall() {
        let mut r = PhysicsRuntime::empty_for_tests();
        r.settings.scene_colliders_enabled = true;
        r.settings.scene_participates_in_queries = true;
        let wall = [([-0.02, 0.0, -2.0], [0.02, 2.0, 2.0])];
        assert!(r
            .endorphin_getup_obstruction([1.0, 0.302, 0.0], 1.2, 0.298, &[], &wall)
            .is_none());
        assert!(!r.endorphin_getup_carry_clear(
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            0.3,
            0.6,
            &[],
            &wall
        ));
        assert!(r.endorphin_getup_carry_clear(
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 1.0],
            0.3,
            0.6,
            &[],
            &wall
        ));
    }
    #[test]
    fn carried_capsule_cannot_miss_a_thin_ledge_between_probe_heights() {
        let mut r = PhysicsRuntime::empty_for_tests();
        r.settings.scene_colliders_enabled = true;
        r.settings.scene_participates_in_queries = true;
        // This strip intersects the standing capsule cylinder, while missing
        // spheres placed only at the bottom, middle and top of its spine.
        let skin = r.contact_skin();
        assert!(skin > 0.0 && skin < 0.3);
        let y = 0.6 + skin;
        let ledge = [(
            [-0.02, y - 0.25 * skin, 0.27],
            [0.02, y + 0.25 * skin, 0.29],
        )];
        assert!(!r.endorphin_getup_carry_clear(
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            0.3,
            0.6,
            &[],
            &ledge
        ));
        // A strip genuinely outside the capsule remains clear.
        let outside = [([-0.02, y - 0.25 * skin, 0.4], [0.02, y + 0.25 * skin, 0.42])];
        assert!(r.endorphin_getup_carry_clear(
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            0.3,
            0.6,
            &[],
            &outside
        ));
    }
    #[test]
    fn support_ignore_never_removes_an_unrelated_obstacle_or_accepts_bad_motion() {
        let mut r = PhysicsRuntime::empty_for_tests();
        for (entity, x) in [(7, 0.0), (8, 0.5)] {
            r.upsert_body_from_script(
                &json!({"entity":entity,"body_kind":"static","position":[x,1,0],
                "shape":{"kind":"box","half_extents":[0.05,1,1]}}),
                0,
            )
            .unwrap();
        }
        assert!(!r.endorphin_getup_carry_clear(
            [-0.2, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            0.1,
            0.4,
            &[7],
            &[]
        ));
        assert!(r.endorphin_getup_carry_clear(
            [-0.2, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            0.1,
            0.4,
            &[7, 8],
            &[]
        ));
        assert!(!r.endorphin_getup_carry_clear([0.0; 3], [f32::NAN, 0.0, 0.0], 0.3, 0.6, &[], &[]));
    }
}
