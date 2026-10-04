use super::*;

pub(super) const MAX_ENGINE_START_ATTEMPTS: u8 = 3;
pub(super) const ENGINE_START_THROTTLE_THRESHOLD: f32 = 0.01;
pub(super) const STARTING_IGNITION_MIN_HOLD: f32 = 0.3;
pub(super) const STARTING_IGNITION_MAX_HOLD: f32 = 0.6;
pub(super) const RETRY_IGNITION_MIN_HOLD: f32 = 0.8;
pub(super) const RETRY_IGNITION_MAX_HOLD: f32 = 1.3;

pub(super) fn supports_throttle_engine_start(class: VehicleClass) -> bool {
    matches!(class, VehicleClass::Automobile | VehicleClass::Bike)
}

pub(super) fn can_request_engine_start(vehicle: &VehicleInstance) -> bool {
    vehicle.enabled
        && vehicle
            .definition
            .specification
            .has_engine(vehicle.definition.class)
        && supports_throttle_engine_start(vehicle.definition.class)
        && !vehicle.engine_running
        && !vehicle.engine_starting
        && vehicle.damage.has_fuel()
        && !vehicle.damage.exploded
}

pub(super) fn can_retry_engine_start(vehicle: &VehicleInstance) -> bool {
    can_request_engine_start(vehicle)
        && vehicle.engine_operational()
        && !vehicle.damage.engine_dead
        && vehicle.damage.engine_health > ENGINE_DAMAGE_ON_FIRE
}

pub(super) fn begin_engine_start_attempt(
    entity: VehicleEntity,
    vehicle: &mut VehicleInstance,
) -> Option<f32> {
    if !can_request_engine_start(vehicle) {
        return None;
    }

    vehicle.engine_start_attempt_sequence = vehicle.engine_start_attempt_sequence.wrapping_add(1);
    let attempt = u64::from(vehicle.engine_start_attempt_sequence);

    let forced_failure = !vehicle.engine_operational()
        || vehicle.damage.engine_dead
        || vehicle.damage.engine_health <= ENGINE_DAMAGE_ON_FIRE
        || vehicle.engine_condition <= 0.0;

    let health_factor = (vehicle.damage.engine_health / ENGINE_HEALTH_MAX).clamp(0.0, 1.0);
    let failure_probability = if forced_failure {
        1.0
    } else if vehicle.failed_engine_start_attempts >= MAX_ENGINE_START_ATTEMPTS {
        // CVehicle::ComputeEngineWontStartProbability forces the next attempt to
        // succeed after three failures, provided the engine and fuel system are startable.
        0.0
    } else {
        1.0 - health_factor
    };

    let failure_roll = deterministic_unit(
        entity ^ attempt.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x454E_4749_4E45_5354,
    );
    vehicle.engine_start_will_fail = failure_roll < failure_probability;

    let first_successful_attempt =
        !vehicle.engine_start_will_fail && vehicle.failed_engine_start_attempts == 0;
    let (min_hold, max_hold) = if first_successful_attempt {
        (STARTING_IGNITION_MIN_HOLD, STARTING_IGNITION_MAX_HOLD)
    } else {
        (RETRY_IGNITION_MIN_HOLD, RETRY_IGNITION_MAX_HOLD)
    };
    let hold_roll = deterministic_unit(
        entity ^ attempt.wrapping_mul(0xD1B5_4A32_D192_ED03) ^ 0x4947_4E49_5449_4F4E,
    );
    let hold = min_hold + (max_hold - min_hold) * hold_roll;

    // SwitchEngineOn(false): reset transmission and enter the cranking state.
    vehicle.transmission = TransmissionState::default();
    vehicle.engine_running = false;
    vehicle.engine_starting = true;
    vehicle.engine_start_remaining = hold;
    Some(hold)
}

pub(super) fn advance_engine_start_attempt(
    entity: VehicleEntity,
    vehicle: &mut VehicleInstance,
    dt: f32,
    events: &mut Vec<VehicleEvent>,
) {
    if !vehicle.engine_starting {
        return;
    }

    vehicle.engine_start_remaining = (vehicle.engine_start_remaining - dt).max(0.0);
    if vehicle.engine_start_remaining > 0.0 {
        return;
    }

    if vehicle.engine_start_will_fail {
        vehicle.failed_engine_start_attempts =
            vehicle.failed_engine_start_attempts.saturating_add(1);
        vehicle.engine_starting = false;
        vehicle.engine_start_will_fail = false;
        vehicle.transmission.engine_speed = 0.0;

        let mut failed = VehicleEvent::new(0, entity, VehicleEventKind::EngineStartFailed);
        failed.hold_seconds = vehicle.engine_start_remaining;
        events.push(failed);

        // Reference behavior immediately retries after a failed ignition while
        // the engine remains startable. The retry does not require a new key edge.
        if can_retry_engine_start(vehicle) {
            let _ = begin_engine_start_attempt(entity, vehicle);
        }
        return;
    }

    vehicle.engine_running = true;
    vehicle.engine_starting = false;
    vehicle.engine_start_will_fail = false;
    vehicle.engine_start_remaining = 0.0;
    events.push(VehicleEvent::new(
        0,
        entity,
        VehicleEventKind::EngineStarted,
    ));
}
