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

impl PhysicsRuntime {
    /// Raw post-Bullet contact observations for sound; independent from
    /// health/damage filtering. Coalesce fixed-step manifolds to their highest
    /// normal impulse per body pair and surface within the rendered frame.
    pub(crate) fn audio_contacts(
        &self,
        seconds: f64,
    ) -> Vec<newviso_audio_client::PhysicsAudioContact> {
        use newviso_audio_client::{ImpactMaterial, PhysicsAudioContact};
        // Resolve only the strongest contact per body/surface before paying
        // for mass lookups and contact velocity transforms. Bullet emits many
        // persistent manifold samples per render frame.
        let mut peaks = std::collections::HashMap::<
            (u64, u64, u32),
            newviso_physics_client::PhysicsContactEvent,
        >::with_capacity(self.last_output.events.len().min(256));
        for event in &self.last_output.events {
            let contact = match event {
                PhysicsEvent::ContactBegin(c) | PhysicsEvent::ContactPersist(c) => c,
                _ => continue,
            };
            if !contact.impulse.is_finite() || contact.impulse <= 0.0 {
                continue;
            }
            let a = contact.a.min(contact.b);
            let b = contact.a.max(contact.b);
            let key = (a, b, contact.surface_id.unwrap_or(0));
            peaks
                .entry(key)
                .and_modify(|previous| {
                    if contact.impulse > previous.impulse {
                        *previous = *contact;
                    }
                })
                .or_insert(*contact);
        }

        // Keep stable ordering for tests, diagnostics and playback seeds.
        let mut strongest = peaks.into_iter().collect::<Vec<_>>();
        strongest.sort_unstable_by_key(|(key, _)| *key);
        let mut output = Vec::with_capacity(strongest.len());
        for ((a, b, contact_group), contact) in strongest {
            // Friction/density are not an acoustic material taxonomy.
            let material = ImpactMaterial::Generic;
            let mass_for = |id| {
                self.bodies
                    .get(&id)
                    .and_then(|body| body.mass_properties.as_ref())
                    .map(|mass| mass.mass)
                    .filter(|mass| mass.is_finite() && *mass > 0.0)
            };
            let effective_mass_kg = match (mass_for(a), mass_for(b)) {
                (Some(x), Some(y)) => x * y / (x + y),
                (Some(m), None) | (None, Some(m)) => m,
                (None, None) => 1.0,
            };
            let normal_speed_mps = contact.relative_velocity.map(|velocity| {
                velocity
                    .iter()
                    .zip(contact.normal)
                    .map(|(x, n)| x * n)
                    .sum::<f32>()
                    .abs()
            });
            output.push(PhysicsAudioContact {
                body_a: a,
                body_b: b,
                contact_group,
                position: contact.point,
                normal_impulse: contact.impulse,
                effective_mass_kg,
                normal_speed_mps,
                material,
                seconds,
            });
        }
        output
    }
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
    fn undamaged_bullet_contacts_are_visible_to_audio_pipeline() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        runtime.last_output.events = vec![
            PhysicsEvent::ContactBegin(contact(40.0, [0.0, 1.0, 0.0])),
            PhysicsEvent::ContactPersist(contact(60.0, [0.0, 1.0, 0.0])),
        ];
        // Nothing is marked as damage source, yet a physical event exists.
        assert!(runtime.damage_sources.is_empty());
        let contacts = runtime.audio_contacts(0.5);
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].normal_impulse, 60.0);
        assert_eq!(contacts[0].body_a, 7);
        assert_eq!(contacts[0].body_b, 8);
    }

    #[test]
    fn repeated_manifolds_emit_one_stable_peak_per_body_pair_and_surface() {
        let mut runtime = PhysicsRuntime::empty_for_tests();
        let mut events = Vec::new();
        for id in (1..=32).rev() {
            let mut event = contact(10.0, [0.0, 1.0, 0.0]);
            event.a = 100;
            event.b = id;
            event.surface_id = Some(id as u32);
            events.push(PhysicsEvent::ContactBegin(event));
            for impulse in [2.0, 80.0, 20.0] {
                event.impulse = impulse;
                events.push(PhysicsEvent::ContactPersist(event));
            }
        }
        runtime.last_output.events = events;
        let contacts = runtime.audio_contacts(0.75);
        assert_eq!(contacts.len(), 32);
        assert!(contacts.iter().all(|c| c.normal_impulse == 80.0));
        assert!(contacts.windows(2).all(|p| {
            (p[0].body_a, p[0].body_b, p[0].contact_group)
                < (p[1].body_a, p[1].body_b, p[1].contact_group)
        }));
        let again = runtime.audio_contacts(0.75);
        assert!(contacts.iter().zip(again).all(|(x, y)| {
            x.body_a == y.body_a
                && x.body_b == y.body_b
                && x.contact_group == y.contact_group
                && x.normal_impulse == y.normal_impulse
        }));
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

#[cfg(test)]
mod benchmarks;
