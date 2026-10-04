use super::*;

pub(super) fn advance_vehicle_powertrain(
    entity: VehicleEntity,
    vehicle: &mut VehicleInstance,
    dt: f32,
    upside_down: bool,
    consume_petrol: bool,
    frame_events: &mut Vec<VehicleEvent>,
) {
    let before = vehicle.damage.status();
    let wheels = &vehicle.wheels;
    let seed_base = entity
        ^ (vehicle.speed_mps.to_bits() as u64).rotate_left(13)
        ^ (vehicle.transmission.gear as i64 as u64).rotate_left(29);
    let damage_signals = process_damage_frame_with_specification(
        &mut vehicle.damage,
        vehicle.definition.class,
        &vehicle.definition.specification,
        dt,
        |index| {
            wheels
                .get(index)
                .map(|wheel| {
                    wheel.telemetry.angular_velocity.abs()
                        * wheel.telemetry.effective_radius.max(0.0)
                })
                .unwrap_or_default()
        },
        |index| {
            wheels
                .get(index)
                .is_some_and(|wheel| wheel.telemetry.contact)
        },
        |index| deterministic_unit(seed_base ^ (index as u64).wrapping_mul(0x9E37_79B1_85EB_CA87)),
    );
    for signal in damage_signals {
        if let Some(index) = signal.wheel_index {
            if let (Some(wheel), Some(wheel_damage), Some(config)) = (
                vehicle.wheels.get_mut(index),
                vehicle.damage.wheels.get(index),
                vehicle.definition.wheels.get(index),
            ) {
                wheel.tire_condition = wheel_damage.tyre_condition;
                wheel.telemetry.tire_condition = wheel_damage.tyre_condition;
                wheel.telemetry.tire_rubber_remaining = wheel_damage.tyre_rubber_remaining;
                wheel.telemetry.effective_radius = config.radius
                    * wheel_damage
                        .tyre_condition
                        .radius_multiplier_with_rubber(wheel_damage.tyre_rubber_remaining);
                wheel.telemetry.tire_grip_multiplier = wheel_damage
                    .tyre_condition
                    .grip_multiplier_with_rubber(wheel_damage.tyre_rubber_remaining);
                if wheel_damage.tyre_condition == TireCondition::Missing {
                    wheel.contact = None;
                    wheel.compression = 0.0;
                    wheel.angular_velocity = 0.0;
                }
            }
        }
        let mut event = VehicleEvent::new(0, entity, signal.kind);
        event.wheel_index = signal.wheel_index;
        event.magnitude = signal.magnitude;
        event.speed_mps = vehicle.speed_mps;
        frame_events.push(event);
    }

    let powertrain_signals = process_powertrain_with_specification(
        &mut vehicle.damage,
        vehicle.definition.class,
        &vehicle.definition.specification,
        VehicleDamageFrameContext {
            engine_running: vehicle.engine_running,
            rev_ratio: vehicle.transmission.engine_speed,
            forward_speed_mps: vehicle.speed_forward_mps,
            player_driver: vehicle.player_driver,
            upside_down,
            allow_fire_damage: !vehicle.damage_policy.fire_proof
                && !vehicle.damage_policy.invincible
                && !vehicle.definition.specification.damage.indestructible,
            allow_explosion: !vehicle.damage_policy.explosion_proof
                && !vehicle.damage_policy.invincible
                && !vehicle.definition.specification.damage.indestructible,
            allow_oil_damage: !vehicle.damage_policy.invincible
                && !vehicle.definition.specification.damage.indestructible,
            fuel_consumption_rate: if consume_petrol {
                vehicle.definition.handling.petrol_consumption_rate
            } else {
                0.0
            },
        },
        dt,
        |index| {
            deterministic_unit(
                seed_base
                    ^ 0xD4A6_E5E1_3C2B_91F7
                    ^ (index as u64).wrapping_mul(0xA24B_AED4_963E_E407),
            )
        },
    );
    for signal in powertrain_signals {
        let mut event = VehicleEvent::new(0, entity, signal.kind);
        event.magnitude = signal.magnitude;
        event.speed_mps = vehicle.speed_mps;
        event.details = serde_json::json!({"before": before, "after": vehicle.damage.status()});
        frame_events.push(event);
    }

    if vehicle.damage.exploded {
        vehicle.damage.overall_health = 0.0;
        vehicle.damage.body_health = 0.0;
        vehicle.damage.engine_health = ENGINE_DAMAGE_FINISHED;
    }

    if (!vehicle.engine_operational() || !vehicle.damage.has_fuel() || vehicle.damage.engine_dead)
        && vehicle.engine_running
    {
        vehicle.engine_running = false;
        vehicle.engine_starting = false;
        vehicle.engine_start_will_fail = false;
        vehicle.engine_start_remaining = 0.0;
        vehicle.transmission.engine_speed = 0.0;
        frame_events.push(VehicleEvent::new(
            0,
            entity,
            VehicleEventKind::EngineStopped,
        ));
    }
    vehicle.engine_condition = vehicle.damage.engine_condition();
}
