//! Swept head/torso hazards and pre-solver impact evidence for Endorphin.
use super::*;
use newviso_endorphin::{math as em, ImpactThreat};
mod support;
impl PhysicsRuntime {
    pub(crate) fn protective_threat(
        &self,
        origin: [f32; 3],
        velocity: [f32; 3],
        acceleration: [f32; 3],
        radius: f32,
        reach: f32,
        horizon: f32,
        family: &[u64],
        scene_solids: &[([f32; 3], [f32; 3])],
        supported_landing: Option<support::PredictedSupport>,
    ) -> Option<ImpactThreat> {
        if !origin
            .iter()
            .chain(&velocity)
            .chain(&acceleration)
            .all(|v| v.is_finite())
            || ![radius, reach, horizon]
                .iter()
                .all(|v| v.is_finite() && *v > 0.0)
        {
            return None;
        }
        let mut best: Option<ImpactThreat> = None;
        let mut consider = |candidate: ImpactThreat| {
            if candidate.valid()
                && !supported_landing.is_some_and(|s| s.intercepts(&candidate))
                && best
                    .as_ref()
                    .is_none_or(|old| candidate.time_to_impact < old.time_to_impact)
            {
                best = Some(candidate);
            }
        };
        let mut static_ignore = family.to_vec();
        static_ignore.extend(
            self.bodies
                .values()
                .filter(|b| {
                    b.kind != PhysicsBodyKind::Static
                        || em::length(b.linear_velocity) > f32::EPSILON
                        || em::length(b.angular_velocity) > f32::EPSILON
                })
                .map(|b| b.entity),
        );
        for segment in 0..2 {
            let start = horizon * segment as f32 / 2.0;
            let finish = horizon * (segment + 1) as f32 / 2.0;
            let at = |t: f32| {
                em::add(
                    origin,
                    std::array::from_fn(|i| velocity[i] * t + 0.5 * acceleration[i] * t * t),
                )
            };
            let from = at(start);
            let delta = em::sub(at(finish), from);
            if let Some(hit) = self.character_sweep_sphere_ignoring(
                from,
                delta,
                radius,
                &static_ignore,
                scene_solids,
            ) {
                let time = start + (finish - start) * hit.fraction;
                let point = em::sub(
                    em::add(from, delta.map(|v| v * hit.fraction)),
                    hit.normal.map(|v| v * radius),
                );
                let impact_velocity = em::add(velocity, acceleration.map(|v| v * time));
                consider(ImpactThreat {
                    entity: hit.entity,
                    point,
                    normal: hit.normal,
                    surface_velocity: [0.0; 3],
                    closing_speed: -em::dot(impact_velocity, hit.normal),
                    time_to_impact: time,
                });
            }
        }
        for body in self.bodies.values().filter(|b| {
            !family.contains(&b.entity)
                && !b.flags.is_trigger
                && b.flags.participates_in_queries
                && (b.kind != PhysicsBodyKind::Static
                    || em::length(b.linear_velocity) > f32::EPSILON
                    || em::length(b.angular_velocity) > f32::EPSILON)
        }) {
            let surface = em::add(
                body.linear_velocity,
                em::cross(body.angular_velocity, em::sub(origin, body.position)),
            );
            let relative = em::sub(velocity, surface);
            for segment in 0..2 {
                let start = horizon * segment as f32 / 2.0;
                let finish = horizon * (segment + 1) as f32 / 2.0;
                let at = |t: f32| {
                    em::add(
                        origin,
                        std::array::from_fn(|i| relative[i] * t + 0.5 * acceleration[i] * t * t),
                    )
                };
                let from = at(start);
                let delta = em::sub(at(finish), from);
                if let Some(hit) = super::queries::sweep_body_box(body, from, delta, radius) {
                    let normal = hit.normal;
                    let point = em::sub(
                        em::add(from, delta.map(|v| v * hit.fraction)),
                        normal.map(|v| v * radius),
                    );
                    let speed = em::add(
                        body.linear_velocity,
                        em::cross(body.angular_velocity, em::sub(point, body.position)),
                    );
                    let time = start + (finish - start) * hit.fraction;
                    consider(ImpactThreat {
                        entity: Some(body.entity),
                        point,
                        normal,
                        surface_velocity: speed,
                        closing_speed: -em::dot(
                            em::sub(em::add(velocity, acceleration.map(|v| v * time)), speed),
                            normal,
                        ),
                        time_to_impact: time,
                    });
                }
            }
        }
        for event in &self.last_output.events {
            let c = match event {
                PhysicsEvent::ContactBegin(c) | PhysicsEvent::ContactPersist(c) => c,
                _ => continue,
            };
            let a = family.contains(&c.a);
            let b = family.contains(&c.b);
            if a == b {
                continue;
            }
            let normal = if a { c.normal } else { c.normal.map(|v| -v) };
            if em::dot(em::sub(origin, c.point), normal).abs() > reach {
                continue;
            }
            let other = if a { c.b } else { c.a };
            let surface = self.vehicle_body_state(other).map_or([0.0; 3], |p| {
                em::add(
                    p.linear_velocity,
                    em::cross(p.angular_velocity, em::sub(c.point, p.position)),
                )
            });
            let relative = c
                .relative_velocity
                .map(|v| if a { v } else { v.map(|v| -v) })
                .unwrap_or_else(|| em::sub(velocity, surface));
            consider(ImpactThreat {
                entity: Some(other),
                point: c.point,
                normal,
                surface_velocity: surface,
                closing_speed: -em::dot(relative, normal),
                time_to_impact: 0.0,
            });
        }
        best
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incoming_surfaces_are_predicted_for_a_stationary_character_and_receding_ones_are_ignored() {
        let mut p = PhysicsRuntime::empty_for_tests();
        p.upsert_body_from_script(
            &json!({"entity":8,"body_kind":"kinematic","position":[0,1,1],
            "shape":{"kind":"box","half_extents":[1,1,0.05]},"linear_velocity":[0,0,-3]}),
            0,
        )
        .unwrap();
        let t = p
            .protective_threat(
                [0.0, 1.0, 0.0],
                [0.0; 3],
                [0.0; 3],
                0.1,
                0.7,
                0.5,
                &[],
                &[],
                None,
            )
            .unwrap();
        assert_eq!(t.entity, Some(8));
        assert!(t.time_to_impact > 0.2 && t.time_to_impact < 0.4);
        p.bodies.get_mut(&8).unwrap().linear_velocity = [0.0, 0.0, 3.0];
        assert!(p
            .protective_threat(
                [0.0, 1.0, 0.0],
                [0.0; 3],
                [0.0; 3],
                0.1,
                0.7,
                0.5,
                &[],
                &[],
                None
            )
            .is_none());
    }
    #[test]
    fn static_walls_self_filters_and_ballistic_ground_approaches_are_distinct() {
        let mut p = PhysicsRuntime::empty_for_tests();
        p.upsert_body_from_script(
            &json!({"entity":8,"body_kind":"static","position":[0,1,1],
            "shape":{"kind":"box","half_extents":[1,1,0.05]}}),
            0,
        )
        .unwrap();
        assert!(p
            .protective_threat(
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 3.0],
                [0.0; 3],
                0.1,
                0.7,
                0.5,
                &[8],
                &[],
                None,
            )
            .is_none());
        let t = p
            .protective_threat(
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 3.0],
                [0.0; 3],
                0.1,
                0.7,
                0.5,
                &[],
                &[],
                None,
            )
            .unwrap();
        assert!(t.normal[2] < -0.9 && t.closing_speed > 2.9);
        p.bodies.clear();
        p.upsert_body_from_script(&json!({"entity":9,"body_kind":"static","position":[0,-0.1,0],"shape":{"kind":"box","half_extents":[2,0.1,2]}}),0).unwrap();
        let floor = ([-2.0, -0.2, -2.0], [2.0, 0.0, 2.0]);
        let t = p
            .protective_threat(
                [0.0, 0.5, 0.0],
                [0.0, -2.0, 0.0],
                [0.0, -9.81, 0.0],
                0.1,
                0.7,
                0.4,
                &[],
                &[floor],
                None,
            )
            .unwrap();
        assert!(t.normal[1] > 0.9 && t.closing_speed > 2.0);
    }
}
