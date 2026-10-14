use super::*;

const LONGITUDINAL_PEAK: f32 = 0.12;
const SLIDING_RATIO: f32 = 2.5;
const MAX_CONTACT_STEP: f32 = 1.0 / 240.0;

pub(super) struct TireContactInput {
    pub angular_velocity: f32,
    pub forward_speed: f32,
    pub side_speed: f32,
    pub drive_force: f32,
    pub brake_force: f32,
    pub radius: f32,
    pub wheel_inertia: f32,
    pub supported_mass: f32,
    pub load: f32,
    pub grip: f32,
    pub lateral_peak: f32,
    pub maximum: f32,
    pub minimum: f32,
    pub dt: f32,
}

pub(super) struct TireContactOutput {
    pub longitudinal_force: f32,
    pub lateral_force: f32,
    pub angular_velocity: f32,
    pub longitudinal_slip: f32,
    pub lateral_slip_angle: f32,
    pub slip_intensity: f32,
    pub friction_utilization: f32,
    pub locked: bool,
    pub sliding_speed: f32,
    pub rotational_slip_ratio: f32,
}

/// Reference wheel.cpp combines both slip axes before evaluating its traction
/// curve. Falling past the peak therefore reduces the total available force,
/// rather than leaving longitudinal braking at the peak during a lateral skid.
fn combined_force(longitudinal: f32, lateral: f32, input: &TireContactInput) -> (f32, f32) {
    let x = longitudinal / LONGITUDINAL_PEAK;
    let y = lateral / input.lateral_peak.max(0.01);
    let ratio = x.hypot(y);
    if ratio <= EPSILON {
        return (0.0, 0.0);
    }
    let magnitude = traction_coefficient(ratio, 1.0, SLIDING_RATIO, input.maximum, input.minimum)
        * input.load.max(0.0)
        * input.grip.max(0.0);
    (x / ratio * magnitude, y / ratio * magnitude)
}

/// Dissipative wheel brake. A locked wheel stays at zero until road reaction or
/// drive torque exceeds the remaining brake torque; road speed never resets it.
pub(super) fn brake_spin(spin: f32, force: f32, radius: f32, inertia: f32, dt: f32) -> f32 {
    spin.signum() * (spin.abs() - force.max(0.0) * radius / inertia.max(0.01) * dt).max(0.0)
}

pub(super) fn solve_contact(input: TireContactInput) -> TireContactOutput {
    let count = (input.dt / MAX_CONTACT_STEP).ceil().clamp(1.0, 16.0) as usize;
    let h = input.dt / count as f32;
    let mass = input.supported_mass.max(0.01);
    let inertia = input.wheel_inertia.max(0.01);
    let radius = input.radius.max(0.01);
    let slip_mass = 1.0 / (1.0 / mass + radius * radius / inertia);
    let mut spin = input.angular_velocity;
    let mut forward = input.forward_speed;
    let mut side = input.side_speed;
    let mut longitudinal_impulse = 0.0;
    let mut lateral_impulse = 0.0;
    let mut long_slip = 0.0;
    let mut lat_slip = 0.0;
    let mut intensity = 0.0;

    for _ in 0..count {
        spin += input.drive_force * radius / inertia * h;
        let before_brake = spin;
        spin = brake_spin(spin, input.brake_force, radius, inertia, h);
        let remaining_brake = (input.brake_force.max(0.0) * h
            - (before_brake - spin).abs() * inertia / radius)
            .max(0.0);
        let wheel_speed = spin * radius;
        let difference = wheel_speed - forward;
        long_slip = difference / forward.abs().max(wheel_speed.abs()).max(1.0);
        lat_slip = (-side).atan2(forward.abs().max(1.0));
        intensity = (long_slip / LONGITUDINAL_PEAK).hypot(lat_slip / input.lateral_peak.max(0.01));
        let (mut long_force, mut side_force) = combined_force(long_slip, lat_slip, &input);
        if spin.abs() <= EPSILON && remaining_brake > 0.0 && intensity >= SLIDING_RATIO {
            // A sliding locked tyre opposes contact motion, as in the reference
            // braking branch, instead of manufacturing lateral steering grip.
            let speed = forward.hypot(side);
            let magnitude = long_force.hypot(side_force);
            if speed > EPSILON {
                long_force = -forward / speed * magnitude;
                side_force = -side / speed * magnitude;
            }
        }
        // Limit contact impulses to closing velocities. Substepping the wheel
        // and supported mass prevents high torque from reversing free-wheel slip
        // or injecting sideways energy at low speed.
        let held = spin.abs() <= EPSILON && remaining_brake > 0.0;
        let long_limit = if held {
            (difference.abs() * mass).min(remaining_brake + difference.abs() * slip_mass)
        } else {
            difference.abs() * slip_mass
        };
        let j_long = (long_force * h).clamp(-long_limit, long_limit);
        let side_limit = side.abs() * mass;
        let j_side = (side_force * h).clamp(-side_limit, side_limit);
        forward += j_long / mass;
        side += j_side / mass;
        spin -= j_long * radius / inertia;
        spin = brake_spin(spin, remaining_brake / h, radius, inertia, h);
        longitudinal_impulse += j_long;
        lateral_impulse += j_side;
    }
    let longitudinal_force = longitudinal_impulse / input.dt;
    let lateral_force = lateral_impulse / input.dt;
    let peak_capacity = input.maximum * input.load.max(0.0) * input.grip.max(0.0);
    TireContactOutput {
        longitudinal_force,
        lateral_force,
        angular_velocity: spin,
        longitudinal_slip: long_slip,
        lateral_slip_angle: lat_slip,
        slip_intensity: intensity.clamp(0.0, 12.0),
        friction_utilization: if peak_capacity > EPSILON {
            (longitudinal_force.hypot(lateral_force) / peak_capacity).clamp(0.0, 1.0)
        } else {
            0.0
        },
        locked: input.brake_force > EPSILON && spin.abs() <= EPSILON,
        sliding_speed: (spin * radius - forward).hypot(side),
        rotational_slip_ratio: (spin * radius - forward) / forward.abs().max(1.0),
    }
}

/// CAutomobile keeps a short grip reduction after a handbrake turn, while its
/// handbrake torque decays from full force toward half force. Age is in seconds.
pub(super) fn handbrake_response(age: f32) -> (f32, f32) {
    (
        0.5 + 0.5 * (-2.5 * age.max(0.0)).exp(),
        0.65 + 0.35 * (age / 1.5).clamp(0.0, 1.0),
    )
}

/// CWheel::ProcessTyreTemp heat/cooling constants. The caller distinguishes
/// bounded rolling slip from intentional burnout slip; lateral scrub is not heat.
pub(super) fn tyre_temperature(
    previous: f32,
    rotational_slip_ratio: f32,
    heat: bool,
    slow_on_gas: bool,
    dt: f32,
) -> f32 {
    let heat_rate = if heat {
        1.5 * rotational_slip_ratio.abs().clamp(0.0, 100.0)
    } else {
        0.0
    };
    let cooling = if slow_on_gas { 0.12 } else { 0.25 };
    let decay = (-cooling * dt).exp();
    // Integrate cooling exponentially so burnout duration is independent of FPS.
    (previous * decay + heat_rate / cooling * (1.0 - decay)).clamp(0.0, TYRE_HEAT_BURST_THRESHOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_friction_does_not_increase_total_kinetic_energy() {
        for speed in [-20.0, -0.01, 0.0, 0.01, 20.0] {
            for side in [-12.0, 0.0, 12.0] {
                for spin in [-80.0, 0.0, 80.0] {
                    for brake in [0.0, 1000.0, 10000.0] {
                        let mut input = contact();
                        input.angular_velocity = spin;
                        input.forward_speed = speed;
                        input.side_speed = side;
                        input.brake_force = brake;
                        let before =
                            0.5 * 375.0 * (speed * speed + side * side) + 0.5 * 2.4 * spin * spin;
                        let result = solve_contact(input);
                        let forward = speed + result.longitudinal_force / 375.0 / 60.0;
                        let side = side + result.lateral_force / 375.0 / 60.0;
                        let after = 0.5 * 375.0 * (forward * forward + side * side)
                            + 0.5 * 2.4 * result.angular_velocity.powi(2);
                        assert!(
                            after <= before + 0.1,
                            "speed={speed} spin={spin} brake={brake} before={before} after={after}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn stationary_burnout_heats_tyre_and_airborne_spin_only_cools() {
        let mut temperature = 0.0;
        for _ in 0..600 {
            temperature = tyre_temperature(temperature, 12.0, true, true, 1.0 / 60.0);
        }
        assert_eq!(temperature, 59.0);
        assert!(tyre_temperature(temperature, 100.0, false, false, 1.0) < temperature);
    }

    #[test]
    fn tyre_heat_cooling_is_stable_across_step_rates() {
        let mut temperatures = Vec::new();
        for rate in [30, 60, 120] {
            let mut temperature = 10.0;
            for _ in 0..rate * 5 {
                temperature = tyre_temperature(temperature, 6.0, true, true, 1.0 / rate as f32);
            }
            temperatures.push(temperature);
        }
        assert!((temperatures[0] - temperatures[2]).abs() < 0.005);
    }

    #[test]
    fn ordinary_rotational_slip_cannot_reach_burst_temperature() {
        for slow_on_gas in [false, true] {
            let mut temperature = 0.0;
            for _ in 0..60 * 120 {
                temperature = tyre_temperature(temperature, 1.0, true, slow_on_gas, 1.0 / 120.0);
            }
            assert!(temperature <= 12.5);
        }
    }

    #[test]
    fn handbrake_transient_uses_reference_force_decay_and_grip_recovery() {
        let (force, grip) = handbrake_response(0.0);
        assert_eq!(force, 1.0);
        assert_eq!(grip, 0.65);
        let (later_force, later_grip) = handbrake_response(1.5);
        assert!(later_force > 0.5 && later_force < 0.6);
        assert_eq!(later_grip, 1.0);
    }

    fn contact() -> TireContactInput {
        TireContactInput {
            angular_velocity: 25.0,
            forward_speed: 10.0,
            side_speed: 0.0,
            drive_force: 0.0,
            brake_force: 0.0,
            radius: 0.4,
            wheel_inertia: 2.4,
            supported_mass: 375.0,
            load: 3678.75,
            grip: 1.0,
            lateral_peak: 0.15,
            maximum: 2.4,
            minimum: 1.8,
            dt: 1.0 / 60.0,
        }
    }

    #[test]
    fn combined_curve_uses_one_force_budget_and_falls_after_peak() {
        let input = contact();
        let peak = combined_force(0.12, 0.0, &input);
        let combined = combined_force(0.3, 0.375, &input);
        assert!((peak.0 - input.maximum * input.load).abs() < 0.01);
        assert!((combined.0.hypot(combined.1) - input.minimum * input.load).abs() < 0.01);
        assert!(combined.0.hypot(combined.1) < peak.0);
    }

    #[test]
    fn handbrake_keeps_wheel_locked_and_uses_sliding_friction() {
        let mut input = contact();
        input.angular_velocity = 0.0;
        input.brake_force = 20000.0;
        let result = solve_contact(input);
        assert!(result.locked);
        assert_eq!(result.angular_velocity, 0.0);
        assert!(result.longitudinal_slip < -0.9);
        assert!(result.longitudinal_force < 0.0);
        assert!(result.longitudinal_force.abs() <= 1.8 * 3678.75 + 0.01);
    }

    #[test]
    fn free_rolling_does_not_create_drive_or_brake_force() {
        let result = solve_contact(contact());
        assert_eq!(result.longitudinal_force, 0.0);
        assert_eq!(result.lateral_force, 0.0);
        assert_eq!(result.angular_velocity, 25.0);
    }

    #[test]
    fn contact_braking_dissipates_forward_and_reverse_motion() {
        for direction in [-1.0, 1.0] {
            let mut input = contact();
            input.angular_velocity *= direction;
            input.forward_speed *= direction;
            input.brake_force = 10000.0;
            let result = solve_contact(input);
            assert!(result.longitudinal_force * direction < 0.0);
            assert!(result.angular_velocity * direction >= 0.0);
            assert!(result.friction_utilization <= 1.0);
        }
    }

    #[test]
    fn ice_reduces_acceleration_and_increases_driven_wheel_spin() {
        let mut dry = contact();
        dry.drive_force = 12000.0;
        let dry_result = solve_contact(dry);
        let mut ice = contact();
        ice.drive_force = 12000.0;
        ice.grip = 0.12;
        let ice_result = solve_contact(ice);
        assert!(ice_result.longitudinal_force < dry_result.longitudinal_force);
        assert!(ice_result.angular_velocity > dry_result.angular_velocity);
    }

    #[test]
    fn lateral_contact_cannot_reverse_motion_or_add_energy() {
        let mut input = contact();
        input.side_speed = 0.001;
        let result = solve_contact(input);
        let after = 0.001 + result.lateral_force / 375.0 / 60.0;
        assert!(after >= -1.0e-7 && after <= 0.001);
        assert!(result.lateral_force <= 0.0);
    }

    #[test]
    fn brakes_stop_airborne_spin_without_reversing_it() {
        assert_eq!(brake_spin(2.0, 5000.0, 0.4, 2.4, 1.0 / 60.0), 0.0);
        assert_eq!(brake_spin(-2.0, 5000.0, 0.4, 2.4, 1.0 / 60.0), 0.0);
    }
}
