use super::*;

// Script frames summarize fixed-step manifolds. Native damage processing and
// full diagnostics retain every sample in last_output.events.
pub(super) fn compact_frame_events(events: &[PhysicsEvent]) -> Vec<PhysicsEvent> {
    let mut output = Vec::with_capacity(events.len());
    let mut contacts = std::collections::HashMap::with_capacity(events.len().min(4096));
    for event in events {
        match event {
            PhysicsEvent::ContactBegin(contact) | PhysicsEvent::ContactPersist(contact) => {
                let (a, b) = (contact.a.min(contact.b), contact.a.max(contact.b));
                let axis = (0..3)
                    .max_by(|&a, &b| contact.normal[a].abs().total_cmp(&contact.normal[b].abs()))
                    .unwrap_or(1);
                let direction = contact.normal[axis] * if contact.a == a { 1.0 } else { -1.0 };
                let key = (
                    matches!(event, PhysicsEvent::ContactBegin(_)),
                    a,
                    b,
                    axis,
                    direction.is_sign_negative(),
                    contact.surface_entity,
                    contact.surface_id,
                );
                if let Some(&index) = contacts.get(&key) {
                    let prior = match &output[index] {
                        PhysicsEvent::ContactBegin(c) | PhysicsEvent::ContactPersist(c) => c,
                        _ => unreachable!(),
                    };
                    if contact.impulse > prior.impulse {
                        output[index] = *event;
                    }
                } else {
                    contacts.insert(key, output.len());
                    output.push(*event);
                }
            }
            PhysicsEvent::ContactEnd { a, b } => {
                let pair = ((*a).min(*b), (*a).max(*b));
                contacts.retain(|key, _| (key.1, key.2) != pair);
                output.push(*event);
            }
            PhysicsEvent::BodyDestroyed { entity } => {
                contacts.retain(|key, _| key.1 != *entity && key.2 != *entity);
                output.push(*event);
            }
            PhysicsEvent::BodyCreated { .. } => output.push(*event),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use newviso_physics_client::PhysicsContactEvent;

    fn contact(impulse: f32, normal: [f32; 3]) -> PhysicsContactEvent {
        PhysicsContactEvent {
            a: 7,
            b: 8,
            point: [impulse, 0.0, 0.0],
            normal,
            impulse,
            relative_velocity: None,
            materials: Default::default(),
            surface_id: Some(4),
            surface_entity: Some(8),
            surface_triangle: None,
        }
    }

    #[test]
    fn script_contact_peaks_keep_transitions_surfaces_and_support_normals() {
        let floor = [0.0, 1.0, 0.0];
        let mut other_surface = contact(1.0, floor);
        other_surface.surface_id = Some(5);
        let events = [
            PhysicsEvent::BodyCreated { entity: 7 },
            PhysicsEvent::ContactBegin(contact(2.0, floor)),
            PhysicsEvent::ContactBegin(contact(5.0, floor)),
            PhysicsEvent::ContactPersist(contact(1.0, floor)),
            PhysicsEvent::ContactPersist(contact(9.0, floor)),
            PhysicsEvent::ContactPersist(contact(3.0, [1.0, 0.0, 0.0])),
            PhysicsEvent::ContactPersist(other_surface),
            PhysicsEvent::ContactEnd { a: 8, b: 7 },
            PhysicsEvent::ContactBegin(contact(4.0, floor)),
            PhysicsEvent::BodyDestroyed { entity: 7 },
        ];
        let compact = compact_frame_events(&events);
        assert_eq!(compact.len(), 8);
        assert_eq!(compact[1], PhysicsEvent::ContactBegin(contact(5.0, floor)));
        assert_eq!(
            compact[2],
            PhysicsEvent::ContactPersist(contact(9.0, floor))
        );
        assert_eq!(compact[5], PhysicsEvent::ContactEnd { a: 8, b: 7 });
        assert_eq!(compact[6], PhysicsEvent::ContactBegin(contact(4.0, floor)));

        let mut runtime = PhysicsRuntime::empty_for_tests();
        runtime.last_output.events = events.to_vec();
        runtime.damage_sources.insert(
            7,
            PhysicsDamageSource {
                damage: 5.0,
                kind: PhysicsDamageKind::Melee,
            },
        );
        runtime
            .ballistic_damage_contacts
            .push(PhysicsDamageContact {
                source: 9,
                target: 7,
                damage_kind: PhysicsDamageKind::Bullet,
                direct_damage: 20.0,
                contact_impulse: 3.0,
                point: [0.0; 3],
                impulse_direction: [1.0, 0.0, 0.0],
            });
        assert_eq!(
            runtime.runtime_state()["events"].as_array().unwrap().len(),
            10
        );
        let script = runtime.script_frame_state(&BTreeSet::new());
        assert_eq!(script["events"].as_array().unwrap().len(), 8);
        assert_eq!(script["contact_sampling"], "frame_peaks");
        assert_eq!(script["damage_contacts"].as_array().unwrap().len(), 6);
        assert_eq!(script["damage_contact_sampling"], "direct_sources");
        assert_eq!(runtime.last_output.events.len(), 10);
        assert_eq!(runtime.damage_contacts().len(), 15);
    }
    #[test]
    fn native_particle_persist_sampling_preserves_gameplay_contacts_and_native_damage() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        runtime
            .upsert_body_from_script(
                &json!({
                    "entity":10, "body_kind":"dynamic",
                    "shape":{"kind":"box","half_extents":[0.5,0.5,0.5]}, "position":[0.0,1.0,0.0]
                }),
                0,
            )
            .unwrap();
        let floor = contact(50.0, [0.0, 1.0, 0.0]);
        let mut particles = floor;
        particles.b = 9;
        let mut car = floor;
        car.b = 10;
        let mut player_ground = floor;
        player_ground.a = 10;
        runtime.last_output.events = vec![
            PhysicsEvent::BodyCreated { entity: 7 },
            PhysicsEvent::ContactBegin(floor),
            PhysicsEvent::ContactPersist(floor),
            PhysicsEvent::ContactEnd { a: 7, b: 8 },
            PhysicsEvent::ContactBegin(particles),
            PhysicsEvent::ContactPersist(particles),
            PhysicsEvent::ContactPersist(car),
            PhysicsEvent::ContactPersist(player_ground),
            PhysicsEvent::BodyDestroyed { entity: 9 },
        ];
        let native_contacts = runtime.damage_contacts();
        let script = runtime.script_frame_state(&BTreeSet::from([7, 9]));
        let events = script["events"].as_array().unwrap();
        assert_eq!(events.len(), 7);
        assert_eq!(script["particle_persist_suppressed"], 2);
        assert!(events.iter().any(|e| e["ContactBegin"]["b"] == 8));
        assert!(events.iter().any(|e| e["ContactEnd"]["b"] == 8));
        assert!(events.iter().any(|e| e["ContactPersist"]["b"] == 10));
        assert!(events.iter().any(|e| e["ContactPersist"]["a"] == 10));
        assert_eq!(
            runtime.runtime_state()["events"].as_array().unwrap().len(),
            9
        );
        assert_eq!(runtime.damage_contacts().len(), native_contacts.len());
        assert_eq!(
            runtime
                .damage_contacts()
                .iter()
                .map(|c| c.contact_impulse)
                .sum::<f32>(),
            native_contacts
                .iter()
                .map(|c| c.contact_impulse)
                .sum::<f32>()
        );
        runtime.damage_sources.insert(
            7,
            PhysicsDamageSource {
                damage: 5.0,
                kind: PhysicsDamageKind::Melee,
            },
        );
        let damaging = runtime.script_frame_state(&BTreeSet::from([7, 9]));
        assert_eq!(damaging["particle_persist_suppressed"], 0);
        assert!(!damaging["damage_contacts"].as_array().unwrap().is_empty());
    }
}
