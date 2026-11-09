use super::*;
fn model() -> RecoveryModel {
    RecoveryModel {
        settling_energy_fraction: 0.05,
        minimum_support_cosine: 0.7,
        response_time_scale: 0.25,
        ready_confidence: 0.8,
        balance_reach: 1.0,
        airborne_clearance_fraction: 0.2,
        landing_absorption_height_fraction: 0.2,
        capacity_floor: 0.1,
    }
}
fn body(scale: f32, gravity: f32, capacity: f32, speed: f32) -> BodyObservation {
    BodyObservation {
        rotation: [0.0, 0.0, 0.0, 1.0],
        linear_velocity: [0.0, -speed, 0.0],
        angular_velocity: [0.0; 3],
        radius: 0.3 * scale,
        half_height: 0.6 * scale,
        gravity,
        capacity,
        motion_energy_per_mass: None,
        support: None,
        space_clear: true,
        down: false,
    }
}
#[test]
fn severe_descent_requests_physics_before_contact_at_all_observation_rates() {
    for hz in [30.0, 60.0, 144.0, 240.0] {
        let mut s = RecoveryState::default();
        for _ in 0..(hz as usize / 4) {
            s.observe(&model(), body(1.0, 9.81, 1.0, 5.0), 1.0 / hz);
        }
        assert!(s.falling && s.high_fall && !s.hard_landing && !s.support_available);
    }
}
#[test]
fn airborne_control_uses_the_same_capacity_and_energy_budget_as_landing() {
    for scale in [0.1, 1.0, 3.0] {
        for gravity in [1.62, 9.81, 19.62] {
            let budget = 2.0 * gravity * 1.8 * scale * model().landing_absorption_height_fraction;
            let mut low = RecoveryState::default();
            let mut high = RecoveryState::default();
            let mut injured = RecoveryState::default();
            let speed = (budget * 0.8_f32).sqrt();
            for _ in 0..120 {
                low.observe(&model(), body(scale, gravity, 1.0, speed), 1.0 / 60.0);
                high.observe(
                    &model(),
                    body(scale, gravity, 1.0, (budget * 1.2_f32).sqrt()),
                    1.0 / 60.0,
                );
                injured.observe(&model(), body(scale, gravity, 0.25, speed), 1.0 / 60.0);
            }
            assert!(!low.high_fall && high.high_fall && injured.high_fall);
        }
    }
}
#[test]
fn support_contact_ends_high_fall_and_preserves_the_landing_evidence() {
    let mut s = RecoveryState::default();
    let b = body(1.0, 9.81, 1.0, 5.0);
    for _ in 0..20 {
        s.observe(&model(), b, 1.0 / 60.0);
    }
    let mut supported = b;
    supported.linear_velocity = [0.0; 3];
    supported.support = Some(Support {
        normal: [0.0, 1.0, 0.0],
        velocity: [0.0; 3],
    });
    s.observe(&model(), supported, 1.0 / 60.0);
    assert!(!s.high_fall && !s.falling && s.hard_landing);
    s.observe(&model(), supported, 1.0 / 60.0);
    assert!(!s.hard_landing);
}
#[test]
fn invalid_observations_retire_airborne_history_instead_of_replaying_an_old_impact() {
    let mut s = RecoveryState::default();
    let mut b = body(1.0, 9.81, 1.0, 5.0);
    for _ in 0..20 {
        s.observe(&model(), b, 1.0 / 60.0);
    }
    b.linear_velocity[0] = f32::NAN;
    s.observe(&model(), b, 1.0 / 60.0);
    assert!(!s.high_fall && !s.falling && !s.ready && s.peak_descent_speed == 0.0);
    b.linear_velocity = [0.0; 3];
    b.support = Some(Support {
        normal: [0.0, 1.0, 0.0],
        velocity: [0.0; 3],
    });
    s.observe(&model(), b, 1.0 / 60.0);
    assert!(!s.hard_landing);
}
