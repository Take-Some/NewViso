//! Reproducible opt-in microbenchmark against the pre-refactor BTreeMap path.
use super::*;
use newviso_audio_client::{ImpactMaterial, PhysicsAudioContact};
use newviso_physics_client::PhysicsContactEvent;
use std::hint::black_box;
use std::time::Instant;

fn former_audio_contacts(runtime: &PhysicsRuntime, seconds: f64) -> Vec<PhysicsAudioContact> {
    let mut peaks = BTreeMap::<(u64, u64, u32), PhysicsAudioContact>::new();
    for event in &runtime.last_output.events {
        let contact = match event {
            PhysicsEvent::ContactBegin(c) | PhysicsEvent::ContactPersist(c) => c,
            _ => continue,
        };
        if !contact.impulse.is_finite() || contact.impulse <= 0.0 {
            continue;
        }
        let a = contact.a.min(contact.b);
        let b = contact.a.max(contact.b);
        let mass_for = |id| {
            runtime
                .bodies
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
        let normal_speed_mps = contact.relative_velocity.map(|v| {
            v.iter()
                .zip(contact.normal)
                .map(|(x, n)| x * n)
                .sum::<f32>()
                .abs()
        });
        let input = PhysicsAudioContact {
            body_a: a,
            body_b: b,
            contact_group: contact.surface_id.unwrap_or(0),
            position: contact.point,
            normal_impulse: contact.impulse,
            effective_mass_kg,
            normal_speed_mps,
            material: ImpactMaterial::Generic,
            seconds,
        };
        let key = (a, b, input.contact_group);
        let replace = peaks
            .get(&key)
            .is_none_or(|previous| input.normal_impulse > previous.normal_impulse);
        if replace {
            peaks.insert(key, input);
        }
    }
    peaks.into_values().collect()
}

#[test]
#[ignore = "opt-in performance profiling; no timing assertions"]
fn compare_audio_contact_coalescing_legacy() {
    let mut runtime = PhysicsRuntime::empty_for_tests();
    // Thousands of *persistent* Bullet manifold samples across 700 pairs.
    for i in 0u64..700 {
        let mut contact = PhysicsContactEvent {
            a: i + 11,
            b: 1,
            point: [i as f32, 0.0, 1.0],
            normal: [0.0, 1.0, 0.0],
            impulse: 1.0,
            relative_velocity: Some([1.0, -3.0, 0.0]),
            materials: Default::default(),
            surface_id: Some((i % 16) as u32),
            surface_entity: Some(1),
            surface_triangle: None,
        };
        for step in 0..35 {
            contact.impulse = 10.0 + step as f32;
            runtime
                .last_output
                .events
                .push(PhysicsEvent::ContactPersist(contact));
        }
    }
    let expected = former_audio_contacts(&runtime, 1.0);
    let actual = runtime.audio_contacts(1.0);
    assert_eq!(expected.len(), actual.len());
    for (a, b) in expected.iter().zip(&actual) {
        assert_eq!(
            (a.body_a, a.body_b, a.contact_group),
            (b.body_a, b.body_b, b.contact_group)
        );
        assert_eq!(a.normal_impulse, b.normal_impulse);
        assert_eq!(a.normal_speed_mps, b.normal_speed_mps);
        assert_eq!(a.effective_mass_kg, b.effective_mass_kg);
    }
    // Warm both paths, then measure repeated runs under the same process.
    black_box(former_audio_contacts(&runtime, 1.0));
    black_box(runtime.audio_contacts(1.0));
    let n = 20;
    let start = Instant::now();
    for _ in 0..n {
        black_box(former_audio_contacts(black_box(&runtime), black_box(1.0)));
    }
    let legacy = start.elapsed();
    let start = Instant::now();
    for _ in 0..n {
        black_box(runtime.audio_contacts(black_box(1.0)));
    }
    let optimized = start.elapsed();
    println!(
        "contact_audio: {} events, {} grouped, {} iterations | legacy {:.3} ms | optimized {:.3} ms | speedup {:.2}x",
        runtime.last_output.events.len(), actual.len(), n,
        legacy.as_secs_f64() * 1000.0,
        optimized.as_secs_f64() * 1000.0,
        legacy.as_secs_f64() / optimized.as_secs_f64()
    );
}
