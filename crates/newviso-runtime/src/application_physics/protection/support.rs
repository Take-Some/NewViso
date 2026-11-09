//! Upright feet intercept a predicted landing before the head can reach that plane.
use super::*;
#[derive(Clone, Copy)]
pub(crate) struct PredictedSupport {
    pub time: f32,
    pub friction: f32,
}
impl PredictedSupport {
    pub(super) fn intercepts(&self, t: &ImpactThreat) -> bool {
        t.time_to_impact >= self.time && supports_gravity(t.normal, self.friction)
    }
}
fn supports_gravity(normal: [f32; 3], friction: f32) -> bool {
    let up = normal[1];
    up > 0.0 && (1.0 - up * up).max(0.0) <= (friction * up).powi(2)
}
impl PhysicsRuntime {
    pub(crate) fn protective_support(
        &self,
        entity: u64,
        velocity: [f32; 3],
        gravity: f32,
        horizon: f32,
        scene_solids: &[([f32; 3], [f32; 3])],
    ) -> Option<PredictedSupport> {
        let body = self.bodies.get(&entity)?;
        let CollisionShape::Capsule {
            radius,
            half_height,
        } = body.shape
        else {
            return None;
        };
        let sole = em::add(
            body.position,
            rotate_camera_vector([0.0, -half_height, 0.0], body.rotation),
        );
        for segment in 0..2 {
            let start = horizon * segment as f32 / 2.0;
            let finish = horizon * (segment + 1) as f32 / 2.0;
            let at = |t: f32| {
                em::add(
                    sole,
                    [
                        velocity[0] * t,
                        velocity[1] * t - 0.5 * gravity * t * t,
                        velocity[2] * t,
                    ],
                )
            };
            let from = at(start);
            let delta = em::sub(at(finish), from);
            let Some(hit) =
                self.character_sweep_sphere_ignoring(from, delta, radius, &[entity], scene_solids)
            else {
                continue;
            };
            let time = start + (finish - start) * hit.fraction;
            let friction = hit
                .entity
                .and_then(|e| self.bodies.get(&e))
                .map_or(body.material.friction, |other| {
                    body.material.friction * other.material.friction
                });
            let impact = [velocity[0], velocity[1] - gravity * time, velocity[2]];
            if supports_gravity(hit.normal, friction) && -em::dot(impact, hit.normal) > 0.0 {
                return Some(PredictedSupport { time, friction });
            }
        }
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_supported_capsule_landing_does_not_predict_a_head_impact_but_keeps_incoming_walls() {
        let mut p = PhysicsRuntime::empty_for_tests();
        p.upsert_body_from_script(
            &json!({"entity":1,"body_kind":"dynamic","position":[0,1.1,0],
            "shape":{"kind":"capsule","radius":0.3,"half_height":0.6},"friction":0.7}),
            0,
        )
        .unwrap();
        p.upsert_body_from_script(
            &json!({"entity":2,"body_kind":"static","position":[0,-0.1,0],
            "shape":{"kind":"box","half_extents":[3,0.1,3]},"friction":0.7}),
            0,
        )
        .unwrap();
        let landing = p.protective_support(1, [0.0; 3], 9.81, 1.0, &[]).unwrap();
        let args = ([0.0, 1.6, 0.0], [0.0; 3], [0.0, -9.81, 0.0]);
        assert!(p
            .protective_threat(args.0, args.1, args.2, 0.1, 0.7, 1.0, &[1], &[], None)
            .is_some());
        assert!(p
            .protective_threat(
                args.0,
                args.1,
                args.2,
                0.1,
                0.7,
                1.0,
                &[1],
                &[],
                Some(landing)
            )
            .is_none());
        p.upsert_body_from_script(
            &json!({"entity":3,"body_kind":"kinematic","position":[0,1.6,1],
            "shape":{"kind":"box","half_extents":[1,1,0.05]},"linear_velocity":[0,0,-3]}),
            0,
        )
        .unwrap();
        let t = p
            .protective_threat(
                args.0,
                args.1,
                args.2,
                0.1,
                0.7,
                1.0,
                &[1],
                &[],
                Some(landing),
            )
            .unwrap();
        assert_eq!(t.entity, Some(3));
        p.bodies.remove(&2);
        assert!(p.protective_support(1, [0.0; 3], 9.81, 1.0, &[]).is_none());
        assert!(
            !supports_gravity([0.0, -1.0, 0.0], 0.7)
                && !supports_gravity([0.0, 0.1, 0.9949874], 0.7)
        );
    }
}
