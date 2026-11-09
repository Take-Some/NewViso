//! Standing clearance must distinguish separating support from a new obstruction.
use super::super::*;

impl PhysicsRuntime {
    pub(crate) fn endorphin_getup_obstruction(
        &self,
        origin: [f32; 3],
        rise: f32,
        radius: f32,
        ignore: &[u64],
        solids: &[([f32; 3], [f32; 3])],
    ) -> Option<newviso_character::SweepHit> {
        self.character_sweep_sphere_ignoring_filtered(
            origin,
            [0.0, rise, 0.0],
            radius,
            ignore,
            solids,
            |hit| {
                // Any initial contact that the rising sphere separates from
                // can be released, including steep tilted objects near the feet.
                // Keep tangential walls (allow for rotation roundoff), ceilings,
                // and all later contacts, even behind an initial overlap.
                !(hit.fraction == 0.0
                    && hit.normal[1] > 8.0 * f32::EPSILON
                    && rise * hit.normal[1] > 0.0)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> PhysicsRuntime {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        runtime.settings.scene_colliders_enabled = true;
        runtime.settings.scene_participates_in_queries = true;
        runtime
    }

    fn obstruction(
        runtime: &PhysicsRuntime,
        solids: &[([f32; 3], [f32; 3])],
    ) -> Option<newviso_character::SweepHit> {
        runtime.endorphin_getup_obstruction([0.0, 0.29999, 0.0], 1.2, 0.3, &[], solids)
    }

    #[test]
    fn resting_floor_releases_but_does_not_hide_a_ceiling() {
        let runtime = runtime();
        let floor = ([-2.0, -0.1, -2.0], [2.0, 0.0, 2.0]);
        let ceiling = ([-2.0, 1.0, -2.0], [2.0, 1.1, 2.0]);
        assert!(
            runtime
                .character_sweep_sphere_ignoring(
                    [0.0, 0.29999, 0.0],
                    [0.0, 1.2, 0.0],
                    0.3,
                    &[],
                    &[floor]
                )
                .is_some(),
            "ordinary overlap queries retain their semantics"
        );
        assert!(obstruction(&runtime, &[floor]).is_none());
        for solids in [[floor, ceiling], [ceiling, floor]] {
            let hit = obstruction(&runtime, &solids).unwrap();
            assert!(hit.fraction > 0.0 && hit.fraction < 1.0);
            assert!(hit.normal[1] < -0.99);
        }
    }

    #[test]
    fn wall_and_initial_ceiling_overlap_still_block() {
        let runtime = runtime();
        let wall = ([0.29, -1.0, -2.0], [0.5, 3.0, 2.0]);
        let ceiling = ([-2.0, 0.5, -2.0], [2.0, 0.6, 2.0]);
        let side = obstruction(&runtime, &[wall]).unwrap();
        assert_eq!(side.fraction, 0.0);
        assert!(side.normal[0] < -0.99);
        let top = obstruction(&runtime, &[ceiling]).unwrap();
        assert_eq!(top.fraction, 0.0);
        assert!(top.normal[1] < -0.99);
    }

    #[test]
    fn loose_floor_object_releases_but_ignored_bodies_and_query_flags_stay_respected() {
        let mut runtime = runtime();
        runtime
            .upsert_body_from_script(
                &json!({"entity":7,"body_kind":"dynamic","position":[0,0.015,0],
                "shape":{"kind":"box","half_extents":[0.3,0.025,0.3]}}),
                0,
            )
            .unwrap();
        assert!(obstruction(&runtime, &[]).is_none());
        runtime.bodies.get_mut(&7).unwrap().position[1] = 1.0;
        assert_eq!(obstruction(&runtime, &[]).unwrap().entity, Some(7));
        assert!(runtime
            .endorphin_getup_obstruction([0.0, 0.29999, 0.0], 1.2, 0.3, &[7], &[])
            .is_none());
        runtime.bodies.get_mut(&7).unwrap().flags.is_trigger = true;
        assert!(obstruction(&runtime, &[]).is_none());
        let body = runtime.bodies.get_mut(&7).unwrap();
        body.flags.is_trigger = false;
        body.flags.participates_in_queries = false;
        assert!(obstruction(&runtime, &[]).is_none());
    }

    #[test]
    fn steep_separating_overlap_releases_while_a_tangential_wall_still_blocks() {
        let mut runtime = runtime();
        let angle = 80.0_f32.to_radians() * 0.5;
        runtime
            .upsert_body_from_script(
                &json!({"entity":8,"body_kind":"dynamic","position":[0,0.015,0],
                "rotation":[0.0,0.0,angle.sin(),angle.cos()],
                "shape":{"kind":"box","half_extents":[0.3,0.025,0.3]}}),
                0,
            )
            .unwrap();
        let hit = runtime
            .character_sweep_sphere_ignoring([0.0, 0.29999, 0.0], [0.0, 1.2, 0.0], 0.3, &[], &[])
            .unwrap();
        assert_eq!(hit.fraction, 0.0);
        assert!(hit.normal[1] > 0.1 && hit.normal[1] < 0.2);
        assert!(
            obstruction(&runtime, &[]).is_none(),
            "separation does not depend on whether a surface is walkable"
        );
        let body = runtime.bodies.get_mut(&8).unwrap();
        body.rotation = [
            0.0,
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        ];
        body.position[0] = 0.29;
        assert!(
            obstruction(&runtime, &[]).is_some(),
            "rotation roundoff must not turn a tangential wall into separation"
        );
    }

    #[test]
    fn descending_or_stationary_overlap_cannot_be_discarded_as_separation() {
        let runtime = runtime();
        let floor = [([-2.0, -0.1, -2.0], [2.0, 0.0, 2.0])];
        for rise in [-1.2, 0.0] {
            let hit = runtime
                .endorphin_getup_obstruction([0.0, 0.29999, 0.0], rise, 0.3, &[], &floor)
                .unwrap();
            assert_eq!(hit.fraction, 0.0);
        }
    }

    #[test]
    fn floor_and_ceiling_in_the_same_streamed_mesh_are_filtered_before_nearest_hit() {
        let mut runtime = runtime();
        let mesh = CollisionMeshResource {
            id: newviso_resource_runtime::AssetId(92),
            name: "getup.floor-and-ceiling".into(),
            bounds: newviso_collision::CollisionBounds {
                min: [-2.0, 0.0, -2.0],
                max: [2.0, 1.0, 2.0],
            },
            vertices: vec![
                [-2.0, 0.0, -2.0],
                [2.0, 0.0, -2.0],
                [-2.0, 0.0, 2.0],
                [2.0, 0.0, 2.0],
                [-2.0, 1.0, -2.0],
                [2.0, 1.0, -2.0],
                [-2.0, 1.0, 2.0],
                [2.0, 1.0, 2.0],
            ],
            triangles: vec![[0, 2, 1], [1, 2, 3], [4, 6, 5], [5, 6, 7]],
            material_indices: vec![0; 4],
        };
        runtime
            .install_streamed_collision(42, &mesh, [0.0; 3], [0.0; 3], [1.0; 3])
            .unwrap();
        let hit = obstruction(&runtime, &[]).unwrap();
        assert_eq!(hit.entity, Some(42));
        assert!(hit.fraction > 0.0 && hit.normal[1] < -0.99);
    }
}

impl PhysicsRuntime {
    /// TaskGetUp's 3x actor-mass object rule, restricted to registered pickups.
    /// Unknown objects/vehicles are never inferred to be movable from their shape.
    pub(crate) fn endorphin_recovery_pickup_yields(&self, entity: u64, actor_mass: f32) -> bool {
        if !actor_mass.is_finite() || actor_mass <= 0.0 {
            return false;
        }
        let Some(body) = self.bodies.get(&entity) else {
            return false;
        };
        if body.kind != PhysicsBodyKind::Dynamic
            || self
                .joints
                .values()
                .any(|joint| joint.parent == entity || joint.child == entity)
        {
            return false;
        }
        let mass = if let Some(properties) = &body.mass_properties {
            properties.mass
        } else {
            if !body.convex_hulls.is_empty() {
                return false;
            }
            let (min, max) = shape_bounds(body.shape, [0.0; 3]);
            // A conservative primitive-volume bound cannot make a heavy item light.
            (0..3).map(|i| max[i] - min[i]).product::<f32>() * body.material.density
        };
        mass.is_finite() && mass > 0.0 && f64::from(mass) <= 3.0 * f64::from(actor_mass)
    }
}
#[cfg(test)]
mod contextual_pickup_tests {
    use super::*;
    fn body(runtime: &mut PhysicsRuntime, mass: f32) {
        runtime
            .upsert_body_from_script(
                &json!({"entity":7,"body_kind":"dynamic","position":[0.29,0.3,0],
            "shape":{"kind":"box","half_extents":[0.025,0.3,0.3]},
            "mass_properties":{"mass":mass,"center_of_mass":[0,0,0],"inertia_diagonal":[1,1,1]}}),
                0,
            )
            .unwrap();
    }
    #[test]
    fn registered_loose_items_obey_the_reference_mass_boundary() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        body(&mut runtime, 180.0);
        assert!(runtime.endorphin_recovery_pickup_yields(7, 60.0));
        body(&mut runtime, 180.01);
        assert!(!runtime.endorphin_recovery_pickup_yields(7, 60.0));
        assert!(!runtime.endorphin_recovery_pickup_yields(7, f32::NAN));
        assert!(!runtime.endorphin_recovery_pickup_yields(900, 60.0));
    }
    #[test]
    fn fixed_kinematic_and_attached_items_remain_blocking_at_any_mass() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        body(&mut runtime, 1.0);
        for kind in [PhysicsBodyKind::Static, PhysicsBodyKind::Kinematic] {
            runtime.bodies.get_mut(&7).unwrap().kind = kind;
            assert!(!runtime.endorphin_recovery_pickup_yields(7, 60.0));
        }
        runtime.bodies.get_mut(&7).unwrap().kind = PhysicsBodyKind::Dynamic;
        runtime.joints.insert(
            1,
            PhysicsJointSnapshot {
                id: 1,
                parent: 8,
                child: 7,
                parent_anchor: [0.0; 3],
                child_anchor: [0.0; 3],
                parent_frame_rotation: [0.0, 0.0, 0.0, 1.0],
                child_frame_rotation: [0.0, 0.0, 0.0, 1.0],
                max_force: 1000.0,
                cone_limit: 1.0,
                angular_stiffness: 0.0,
                angular_damping: 0.0,
                collision_family: 0,
                angular_limits: None,
                motor: None,
            },
        );
        assert!(!runtime.endorphin_recovery_pickup_yields(7, 60.0));
    }
    #[test]
    fn yielding_a_known_pickup_cannot_hide_a_wall_or_ceiling_behind_it() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        body(&mut runtime, 1.0);
        runtime.settings.scene_colliders_enabled = true;
        runtime.settings.scene_participates_in_queries = true;
        assert!(runtime
            .endorphin_getup_obstruction([0.0, 0.3, 0.0], 1.2, 0.3, &[], &[])
            .is_some());
        assert!(runtime.endorphin_recovery_pickup_yields(7, 60.0));
        let ceiling = [([-2.0, 1.0, -2.0], [2.0, 1.1, 2.0])];
        let hit = runtime
            .endorphin_getup_obstruction([0.0, 0.3, 0.0], 1.2, 0.3, &[7], &ceiling)
            .unwrap();
        assert!(hit.fraction > 0.0 && hit.normal[1] < -0.99);
    }
}
