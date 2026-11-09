//! Available normal impact work at retained hardware, independent of solver reporting.
pub fn incident_energy(mass_kg: f32, approach_mps: f32, lever_fraction: f32) -> f32 {
    if ![mass_kg, approach_mps, lever_fraction]
        .iter()
        .all(|v| v.is_finite())
        || mass_kg <= 0.0
        || approach_mps <= 0.0
    {
        return 0.0;
    }
    let energy = 0.5 * mass_kg * (approach_mps * lever_fraction.clamp(0.0, 1.0)).powi(2);
    if energy.is_finite() {
        energy
    } else {
        f32::MAX
    }
}

/// Continue only the kinetic budget left after fracture; never add scripted velocity.
pub fn fracture_impulse(
    energy_joules: f32,
    inertia: f32,
    lever: f32,
    incoming_momentum: f32,
) -> f32 {
    if ![energy_joules, inertia, lever, incoming_momentum]
        .iter()
        .all(|v| v.is_finite())
        || energy_joules <= 0.0
        || inertia <= 0.0
        || lever <= f32::EPSILON
        || incoming_momentum <= 0.0
    {
        return 0.0;
    }
    ((2.0 * inertia * energy_joules).sqrt() / lever).min(incoming_momentum)
}
