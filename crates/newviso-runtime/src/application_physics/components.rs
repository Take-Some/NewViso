use super::*;

impl PhysicsRuntime {
    pub(crate) fn remove_body_collision_components(
        &mut self,
        entity: u64,
        components: &[u32],
    ) -> bool {
        let Some((_, live)) = self.body_hull_components.get_mut(&entity) else {
            return false;
        };
        let Some(body) = self.bodies.get_mut(&entity) else {
            return false;
        };
        if live.len() != body.convex_hulls.len() {
            return false;
        }
        let mut hulls = Vec::new();
        let mut ids = Vec::new();
        for (hull, id) in body.convex_hulls.iter().zip(live.iter()) {
            if !components.contains(id) {
                hulls.push(hull.clone());
                ids.push(*id);
            }
        }
        if hulls.is_empty() {
            return false;
        }
        body.convex_hulls = hulls;
        *live = ids;
        refresh_bounds(body);
        true
    }
    pub(crate) fn ignore_collision_until_separated(&mut self, a: u64, b: u64) {
        if self.supports_collision_filters
            && a != b
            && self.bodies.contains_key(&a)
            && self.bodies.contains_key(&b)
        {
            self.separation_pairs
                .insert(if a < b { [a, b] } else { [b, a] });
        }
    }
    pub(super) fn refresh_separation_pairs(&mut self) {
        // Broadphase separation is conservative: world and every other vehicle
        // retain contacts throughout this source-only handoff.
        self.separation_pairs.retain(|[a, b]| {
            let (Some(a), Some(b)) = (self.bodies.get(a), self.bodies.get(b)) else {
                return false;
            };
            (0..3).all(|i| {
                a.bounds_min[i] <= b.bounds_max[i] + 0.03
                    && b.bounds_min[i] <= a.bounds_max[i] + 0.03
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_component_removal_and_repair_keep_identity_alignment() {
        let mut physics = PhysicsRuntime::empty_for_tests();
        let chassis = vec![
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [0.0, 1.0, -1.0],
            [0.0, 0.0, 1.0],
        ];
        let bonnet = chassis
            .iter()
            .map(|p| [p[0], p[1] + 2.0, p[2]])
            .collect::<Vec<_>>();
        physics
            .upsert_body_from_script(
                &json!({"entity":7,"body_kind":"dynamic",
            "shape":{"kind":"box","half_extents":[1,1,1]},"position":[0,0,0],
            "convex_hulls":[chassis,bonnet],"convex_hull_components":[0,9]}),
                0,
            )
            .unwrap();
        let original = physics.body_collision_hulls(7).unwrap();
        assert!(physics.remove_body_collision_components(7, &[9]));
        assert_eq!(physics.body_collision_hulls(7).unwrap().len(), 1);
        assert_eq!(physics.body_hull_components[&7].1, vec![0]);
        physics.restore_body_collision_hulls(7, original);
        assert_eq!(physics.body_hull_components[&7].1, vec![0, 9]);
    }
    #[test]
    fn source_filter_ends_after_separation_and_body_removal() {
        let mut physics = PhysicsRuntime::empty_for_tests();
        physics.supports_collision_filters = true;
        for id in [1, 2] {
            physics
                .upsert_body_from_script(
                    &json!({"entity":id,"body_kind":"dynamic",
            "shape":{"kind":"box","half_extents":[1,1,1]},"position":[0,0,0]}),
                    0,
                )
                .unwrap();
        }
        physics.ignore_collision_until_separated(1, 2);
        physics.refresh_separation_pairs();
        assert_eq!(physics.separation_pairs.len(), 1);
        physics.bodies.get_mut(&2).unwrap().position = [5.0, 0.0, 0.0];
        refresh_bounds(physics.bodies.get_mut(&2).unwrap());
        physics.refresh_separation_pairs();
        assert!(physics.separation_pairs.is_empty());
        physics.bodies.get_mut(&2).unwrap().position = [0.0; 3];
        refresh_bounds(physics.bodies.get_mut(&2).unwrap());
        physics.ignore_collision_until_separated(1, 2);
        physics
            .destroy_body_from_script(&json!({"entity":1}), 0)
            .unwrap();
        assert!(physics.separation_pairs.is_empty());
    }
}
