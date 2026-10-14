use super::*;

pub(super) fn update_transmission(
    handling: &HandlingData,
    input: VehicleInput,
    manual_gear: Option<i8>,
    speed_forward: f32,
    drive_wheels_loaded: bool,
    dt: f32,
    state: &mut TransmissionState,
) {
    let gears = handling.initial_drive_gears.max(1) as i8;
    let speed = speed_forward.abs();
    let top = handling.max_gearing_velocity_mps.max(1.0);
    let reverse = input.throttle < -0.05 && speed_forward < 1.5;

    let target = if let Some(gear) = manual_gear {
        gear
    } else if reverse {
        -1
    } else {
        let normalized = (speed / top).clamp(0.0, 0.999);
        (1 + (normalized * gears as f32).floor() as i8).clamp(1, gears)
    };

    if target != state.gear && state.shift_timer <= 0.0 {
        let rate = if target > state.gear {
            handling.clutch_change_rate_up_shift
        } else {
            handling.clutch_change_rate_down_shift
        }
        .max(0.1);
        state.shift_timer = 1.0 / rate;
        state.gear = target;
    }

    if state.shift_timer > 0.0 {
        state.shift_timer = (state.shift_timer - dt).max(0.0);
        let rate = if state.gear >= 1 {
            handling.clutch_change_rate_up_shift
        } else {
            handling.clutch_change_rate_down_shift
        }
        .max(0.1);
        let duration = 1.0 / rate;
        let phase = (state.shift_timer / duration).clamp(0.0, 1.0);
        state.clutch = (phase * 2.0 - 1.0).abs();
    } else {
        state.clutch = 1.0;
    }

    let gear = state.gear.unsigned_abs().max(1) as f32;
    let count = gears as f32;
    let low = ((gear - 1.0) / count) * top;
    let high = (gear / count) * top;
    let speed_ratio = ((speed - low) / (high - low).max(1.0)).clamp(0.0, 1.25);
    // In neutral or without ground load the engine can rev freely, even when
    // all wheels are missing. Wheel forces remain limited by actual contacts.
    let target_engine = if state.gear == 0 || !drive_wheels_loaded {
        input.throttle.abs()
    } else {
        speed_ratio.max(input.throttle.abs() * 0.25)
    };
    let response = (handling.drive_inertia * 8.0 * dt).clamp(0.0, 1.0);
    state.engine_speed += (target_engine - state.engine_speed) * response;
    state.engine_speed = state.engine_speed.clamp(0.0, 1.25);
}

fn steering_angle_for_wheel(
    handling: &HandlingData,
    wheels: &[WheelConfig],
    config: &WheelConfig,
    steer: f32,
    speed_mps: f32,
) -> f32 {
    if !config.steered || steer.abs() <= EPSILON {
        return 0.0;
    }

    // Keep manoeuvring lock at parking speeds, then progressively reduce the
    // road-wheel angle. This prevents high-speed steering from becoming an
    // instantaneous yaw command while preserving the authored steering lock.
    let speed_ratio = speed_mps.abs() / 38.0;
    let speed_scale = (1.0 / (1.0 + speed_ratio * speed_ratio * 0.78)).clamp(0.38, 1.0);
    // Input is positive for right. In our Y-up, -Z-forward coordinates a
    // right turn needs negative yaw, for both tyre forces and wheel meshes.
    let base = -steer.clamp(-1.0, 1.0) * handling.steering_lock_rad * speed_scale;

    // Proper Ackermann geometry: the inside tyre turns further than the
    // outside tyre. Bikes and single-track layouts naturally fall back to base.
    let mut front_z = 0.0f32;
    let mut rear_z = 0.0f32;
    let mut front_count = 0usize;
    let mut rear_count = 0usize;
    let mut front_min_x = f32::INFINITY;
    let mut front_max_x = f32::NEG_INFINITY;
    for wheel in wheels {
        if wheel.front {
            front_z += wheel.mount_local[2];
            front_count += 1;
            front_min_x = front_min_x.min(wheel.mount_local[0]);
            front_max_x = front_max_x.max(wheel.mount_local[0]);
        } else {
            rear_z += wheel.mount_local[2];
            rear_count += 1;
        }
    }
    if front_count < 2 || rear_count == 0 {
        return base;
    }
    let wheelbase = (front_z / front_count as f32 - rear_z / rear_count as f32).abs();
    let track = (front_max_x - front_min_x).abs();
    if wheelbase < 0.25 || track < 0.15 || base.abs() < 0.01 {
        return base;
    }

    let radius = wheelbase / base.abs().tan().abs().max(0.02);
    let half_track = track * 0.5;
    if radius <= half_track + 0.05 {
        return base;
    }
    let inner = (wheelbase / (radius - half_track)).atan();
    let outer = (wheelbase / (radius + half_track)).atan();
    // Positive yaw rotates local forward (-Z) toward -X, so left is inside.
    let inside = if base > 0.0 {
        config.left
    } else {
        !config.left
    };
    base.signum() * if inside { inner } else { outer }
}

fn relaxed_slip(previous: f32, target: f32, dt: f32, speed_mps: f32) -> f32 {
    // Contact-patch relaxation removes one-frame spikes without hiding a real
    // breakaway. Response becomes quicker with road speed.
    let rate = (10.0 + speed_mps.abs() * 0.45).clamp(10.0, 34.0);
    let blend = 1.0 - (-rate * dt.max(0.0)).exp();
    previous + (target - previous) * blend.clamp(0.0, 1.0)
}

fn combined_slip_intensity(longitudinal: f32, lateral: f32, lateral_peak: f32) -> f32 {
    let long = longitudinal.abs() / 0.12;
    let lat = lateral.abs() / lateral_peak.max(0.03);
    (long * long + lat * lat).sqrt().clamp(0.0, 12.0)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn simulate_ground_vehicle(
    entity: VehicleEntity,
    vehicle: &mut VehicleInstance,
    body: VehicleBodyState,
    forward: Vec3,
    up: Vec3,
    dt: f32,
    gravity: f32,
    plan: &mut VehicleFramePlan,
) {
    let handling = &vehicle.definition.handling;
    let wheel_count = vehicle.wheels.len();
    if wheel_count == 0 {
        return;
    }
    let mass_per_wheel = handling.mass / wheel_count as f32;
    let static_load = mass_per_wheel * gravity;

    let mut compression = vec![0.0; wheel_count];
    let mut suspension_velocity = vec![0.0; wheel_count];
    let mut normal_force = vec![0.0; wheel_count];

    for index in 0..wheel_count {
        let config = &vehicle.definition.wheels[index];
        let rubber_remaining = vehicle
            .damage
            .wheels
            .get(index)
            .map_or(1.0, |damage| damage.tyre_rubber_remaining);
        let state = &mut vehicle.wheels[index];
        state.telemetry.tire_rubber_remaining = rubber_remaining;
        state.old_compression = state.compression;
        if state.tire_condition == TireCondition::Missing {
            continue;
        }

        let Some(contact) = state.contact else {
            state.compression = 0.0;
            state.telemetry.contact = false;
            state.telemetry.normal_force = 0.0;
            state.telemetry.longitudinal_slip = 0.0;
            state.telemetry.lateral_slip_angle = 0.0;
            state.telemetry.slip_intensity = 0.0;
            continue;
        };

        // The hit belongs to the previous probe. Measure its world support plane
        // against the current mount, rather than reusing a stale ray distance.
        let mount_offset = rotate_vec(body.rotation, config.mount_local);
        let mount = add(body.position, mount_offset);
        let current_length = (dot(sub(mount, contact.position), up)
            - config.radius
                * state
                    .tire_condition
                    .radius_multiplier_with_rubber(rubber_remaining))
        .max(0.0);
        let raw = config.rest_length - current_length + handling.suspension_raise;
        let current = raw.clamp(-config.travel_down, config.travel_up);
        state.compression = current;
        compression[index] = current;
        let mount_velocity = add(
            body.linear_velocity,
            cross(body.angular_velocity, mount_offset),
        );
        suspension_velocity[index] = -dot(mount_velocity, up);

        let axle_bias = if config.front {
            handling.suspension_bias_front * 2.0
        } else {
            (1.0 - handling.suspension_bias_front) * 2.0
        }
        .max(0.05);

        let suspension_damage = &vehicle.damage.wheels[index];
        let spring_rate = static_load
            * handling.suspension_force
            * axle_bias
            * suspension_damage.suspension_spring_multiplier()
            / config.rest_length.max(0.05);
        let damping_ratio = (if suspension_velocity[index] >= 0.0 {
            handling.suspension_comp_damp
        } else {
            handling.suspension_rebound_damp
        }) * suspension_damage.suspension_damping_multiplier();
        let critical_damping = 2.0 * (spring_rate * mass_per_wheel).sqrt();
        let damping = critical_damping * damping_ratio;
        // Solve spring/damper velocity implicitly. Include gravity in the predicted
        // compression so the equilibrium load stays m*g while damping cannot inject
        // energy through an explicit step on stiff, highly damped reference profiles.
        let step_coefficient = damping * dt + spring_rate * dt * dt;
        let gravity_along_suspension = gravity * dot(up, WORLD_UP).max(0.0);
        let numerator = spring_rate * current.max(0.0)
            + (damping + spring_rate * dt) * suspension_velocity[index]
            + step_coefficient * gravity_along_suspension;
        let mut force = numerator / (1.0 + step_coefficient / mass_per_wheel);
        if config.travel_up > EPSILON {
            let bump_start = config.travel_up * 0.8;
            if current > bump_start {
                let ratio = ((current - bump_start) / (config.travel_up - bump_start).max(0.01))
                    .clamp(0.0, 2.0);
                force += static_load * ratio * ratio * 2.5;
            }
        }
        normal_force[index] = force.max(0.0);
    }

    // Paired-wheel anti-roll load transfer.
    for index in 0..wheel_count {
        let Some(other) = vehicle.definition.wheels[index].opposite_index else {
            continue;
        };
        if other <= index
            || other >= wheel_count
            || vehicle.wheels[index].contact.is_none()
            || vehicle.wheels[other].contact.is_none()
        {
            continue;
        }
        let front = vehicle.definition.wheels[index].front;
        let bias = if front {
            handling.anti_roll_bar_bias_front * 2.0
        } else {
            (1.0 - handling.anti_roll_bar_bias_front) * 2.0
        };
        let transfer = (compression[index] - compression[other])
            * handling.anti_roll_bar_force
            * bias
            * handling.mass
            * gravity
            * 0.5;
        normal_force[index] = (normal_force[index] + transfer).max(0.0);
        normal_force[other] = (normal_force[other] - transfer).max(0.0);
    }

    let driven_front = vehicle
        .definition
        .wheels
        .iter()
        .filter(|wheel| wheel.driven && wheel.front)
        .count()
        .max(1) as f32;
    let driven_rear = vehicle
        .definition
        .wheels
        .iter()
        .filter(|wheel| wheel.driven && !wheel.front)
        .count()
        .max(1) as f32;

    let gear = vehicle.transmission.gear.unsigned_abs().max(1) as f32;
    let gears = handling.initial_drive_gears.max(1) as f32;
    let gear_torque = 1.0 + 1.35 * (1.0 - ((gear - 1.0) / gears));
    let throttle_direction = if vehicle.transmission.gear < 0 {
        -vehicle.input.throttle.abs()
    } else {
        vehicle.input.throttle.max(0.0)
    };
    let total_drive_force = throttle_direction
        * handling.initial_drive_force
        * handling.mass
        * gravity
        * gear_torque
        * vehicle.transmission.clutch
        * vehicle.motor_output()
        * if vehicle.engine_running && vehicle.transmission.gear != 0 {
            1.0
        } else {
            0.0
        };

    let handbrake_wheels = vehicle
        .definition
        .wheels
        .iter()
        .filter(|wheel| wheel.handbrake)
        .count()
        .max(1) as f32;

    let (handbrake_force_scale, handbrake_grip) =
        if vehicle.definition.class == VehicleClass::Automobile {
            vehicle
                .event_state
                .handbrake_age
                .map_or((1.0, 1.0), tires::handbrake_response)
        } else {
            (1.0, 1.0)
        };

    for index in 0..wheel_count {
        let config = &vehicle.definition.wheels[index];
        let rubber_remaining = vehicle
            .damage
            .wheels
            .get(index)
            .map_or(1.0, |damage| damage.tyre_rubber_remaining);
        let state = &mut vehicle.wheels[index];
        state.telemetry.tire_rubber_remaining = rubber_remaining;
        if state.tire_condition == TireCondition::Missing {
            continue;
        }
        let radius = config.radius
            * state
                .tire_condition
                .radius_multiplier_with_rubber(rubber_remaining);
        let tire_grip = state
            .tire_condition
            .grip_multiplier_with_rubber(rubber_remaining);

        let front_drive = handling.front_drive_weight();
        let drive_bias = if config.front {
            front_drive
        } else {
            1.0 - front_drive
        };
        let drive_divisor = if config.front {
            driven_front
        } else {
            driven_rear
        };
        // CWheel::SetHandBrakeForce disconnects drive torque at the braked axle.
        let drive_request = if config.driven && !(config.handbrake && vehicle.input.handbrake > 0.1)
        {
            // Keep explicitly-authored driven wheels useful even when the handling
            // bias is exactly 0/1, while still preserving front/rear distribution.
            let authored_floor = if drive_bias <= 0.001 { 0.0 } else { 0.05 };
            total_drive_force * drive_bias.max(authored_floor) / drive_divisor
        } else {
            0.0
        };

        let brake_bias = if config.front {
            handling.brake_bias_front
        } else {
            1.0 - handling.brake_bias_front
        };
        let brake_request =
            handling.brake_force * handling.mass * gravity * vehicle.input.brake * brake_bias
                / (wheel_count as f32 * 0.5).max(1.0);
        let handbrake_request = if config.handbrake {
            handling.handbrake_force * handbrake_force_scale * handling.mass * gravity * vehicle.input.handbrake
                // Reference assigns handbrake force at each rear wheel. Preserve
                // that two-wheel axle scale for explicitly authored multi-axle layouts.
                * 2.0
                / handbrake_wheels
        } else {
            0.0
        };

        let wheel_inertia = (0.5 * mass_per_wheel * 0.08 * radius * radius).max(0.05);
        let Some(contact) = state.contact else {
            state.angular_velocity += drive_request * radius / wheel_inertia * dt;
            state.angular_velocity = tires::brake_spin(
                state.angular_velocity,
                brake_request + handbrake_request,
                radius,
                wheel_inertia,
                dt,
            );
            state.rotation_initialized |= state.angular_velocity.abs() > EPSILON;
            state.angular_velocity *= (-dt * 0.15).exp();
            state.rotation_angle = wrap_angle(state.rotation_angle + state.angular_velocity * dt);
            state.telemetry.angular_velocity = state.angular_velocity;
            state.telemetry.rotation_angle = state.rotation_angle;
            state.telemetry.longitudinal_force = 0.0;
            state.telemetry.lateral_force = 0.0;
            state.telemetry.friction_utilization = 0.0;
            state.telemetry.wheel_locked = false;
            state.telemetry.sliding_speed_mps = 0.0;
            continue;
        };

        let mut steering_config = config.clone();
        steering_config.steered = match vehicle.definition.specification.controls.steering {
            VehicleSteeringMode::Configured => config.steered,
            VehicleSteeringMode::Rear => !config.front,
            VehicleSteeringMode::All => true,
            VehicleSteeringMode::HandbrakeRear => {
                config.steered || !config.front && vehicle.input.handbrake > 0.1
            }
        };
        let steer_angle = steering_angle_for_wheel(
            handling,
            &vehicle.definition.wheels,
            &steering_config,
            vehicle.input.steer,
            vehicle.speed_mps,
        ) * if !config.front
            && vehicle.definition.specification.controls.steering != VehicleSteeringMode::Configured
        {
            -1.0
        } else {
            1.0
        };
        let normal = normalize_or(contact.normal, up);
        let mut tyre_forward = normalize_or(project_on_plane(forward, normal), forward);
        if steer_angle.abs() > EPSILON {
            tyre_forward = normalize_or(
                rotate_around_axis(tyre_forward, normal, steer_angle),
                tyre_forward,
            );
        }
        let tyre_side = normalize_or(cross(tyre_forward, normal), [1.0, 0.0, 0.0]);

        let mount = add(body.position, rotate_vec(body.rotation, config.mount_local));
        let suspension_down = normalize_or(
            rotate_vec(body.rotation, [0.0, -1.0, 0.0]),
            [0.0, -1.0, 0.0],
        );
        let probe_origin = add(mount, mul(up, config.travel_up));
        // Reproject the previous support plane under the current suspension ray.
        // Reusing the old query distance offsets tyre forces and marks after motion.
        let denominator = dot(suspension_down, normal);
        let distance = if denominator.abs() > 0.05 {
            dot(sub(contact.position, probe_origin), normal) / denominator
        } else {
            contact.distance
        };
        let point = add(probe_origin, mul(suspension_down, distance.max(0.0)));
        let offset = sub(point, body.position);
        let point_velocity = add(body.linear_velocity, cross(body.angular_velocity, offset));
        let fwd_speed = dot(point_velocity, tyre_forward);
        let side_speed = dot(point_velocity, tyre_side);

        if !state.rotation_initialized {
            state.angular_velocity = fwd_speed / radius;
            state.rotation_initialized = true;
        }

        let traction_bias = if config.front {
            handling.traction_bias_front * 2.0
        } else {
            (1.0 - handling.traction_bias_front) * 2.0
        }
        .max(0.05);

        // Reference reduces launch grip only for a driven tyre at full throttle.
        // Applying it to the front brakes also destroyed a rear-drive burnout.
        let low_speed_loss = if handling.low_speed_traction_loss_mult > 0.0
            && drive_request.abs() > EPSILON
            && vehicle.input.throttle.abs() >= 0.95
        {
            let mult = handling.low_speed_traction_loss_mult;
            let floor = (0.25 / mult).min(1.0);
            let speed_term = floor + (0.08 / mult * fwd_speed.abs()).min(1.0 - floor);
            let material_compensation =
                1.0 + (1.0 - contact.grip_multiplier).max(0.0) * handling.traction_loss_mult * 5.0;
            let pitch_compensation =
                1.0 + forward[1].abs().clamp(0.0, 1.0).asin() / std::f32::consts::PI * 5.0;
            (speed_term * material_compensation * pitch_compensation).clamp(0.01, 1.0)
        } else {
            1.0
        };
        // Source traction_loss_mult controls material loss. It is not a direct
        // grip multiplier: increasing it must not increase dry-road friction.
        let material_loss =
            (1.0 + (contact.grip_multiplier - 1.0) * handling.traction_loss_mult).max(0.001);
        let peak_lat = (handling.traction_curve_lateral_rad / 2.5).max(0.01);
        let load = normal_force[index];
        let load_sensitivity = (static_load / load.max(static_load * 0.25))
            .powf(0.065)
            .clamp(0.82, 1.12);
        let grip = traction_bias
            * handbrake_grip
            * low_speed_loss
            * material_loss
            * config.grip_multiplier
            * tire_grip
            * load_sensitivity;
        let resistance = state
            .tire_condition
            .rolling_resistance_with_rubber(rubber_remaining)
            * load;
        let response = tires::solve_contact(tires::TireContactInput {
            angular_velocity: state.angular_velocity,
            forward_speed: fwd_speed,
            side_speed,
            drive_force: drive_request,
            brake_force: brake_request + handbrake_request + resistance,
            radius,
            wheel_inertia,
            supported_mass: mass_per_wheel,
            load,
            grip,
            lateral_peak: peak_lat * material_loss,
            maximum: handling.traction_curve_max,
            minimum: handling.traction_curve_min,
            dt,
        });
        let long_force = response.longitudinal_force;
        let side_force = response.lateral_force;
        // Brake-held, powered wheelspin is the intentional burnout mode. A
        // full-throttle launch or a lateral skid must not arm thermal punctures.
        let drive_direction = drive_request.signum();
        state.burnout_active = config.driven
            && drive_request.abs() > EPSILON
            && vehicle.input.brake > 0.1
            && vehicle.speed_mps < 2.0
            && load > EPSILON
            && !response.locked
            && response.rotational_slip_ratio * drive_direction > 2.0;
        state.tyre_heat_slip_ratio = if state.burnout_active {
            response.rotational_slip_ratio
        } else {
            response.longitudinal_slip.clamp(-1.0, 1.0)
        };
        let long_slip = relaxed_slip(
            state.telemetry.longitudinal_slip,
            response.longitudinal_slip,
            dt,
            vehicle.speed_mps,
        );
        let lat_slip = relaxed_slip(
            state.telemetry.lateral_slip_angle,
            response.lateral_slip_angle,
            dt,
            vehicle.speed_mps,
        );

        let force = add(
            mul(normal, load),
            add(mul(tyre_forward, long_force), mul(tyre_side, side_force)),
        );
        // GTA wheel.cpp applies tyre forces at the axle roll centre to reduce
        // body roll, pitch and dive. Suspension load still uses the true contact;
        // shifting vertically preserves its lever arm on a flat road.
        let roll_centre = if config.front {
            handling.roll_centre_height_front
        } else {
            handling.roll_centre_height_rear
        };
        plan.impulses.push(VehicleImpulse {
            vehicle: entity,
            impulse: mul(force, dt),
            point: add(point, mul(up, roll_centre.clamp(0.0, 1.0))),
        });

        state.angular_velocity = response.angular_velocity;
        let max_spin = handling.max_gearing_velocity_mps * 2.0 / radius.max(0.05);
        state.angular_velocity = state.angular_velocity.clamp(-max_spin, max_spin);
        state.rotation_angle = wrap_angle(state.rotation_angle + state.angular_velocity * dt);

        state.telemetry = WheelTelemetry {
            tire_condition: state.tire_condition,
            tire_rubber_remaining: rubber_remaining,
            effective_radius: radius,
            tire_grip_multiplier: tire_grip,
            contact: true,
            // The probe hit belongs to the previous query. point re-evaluates
            // that support plane from the current wheel mount so tracks do not
            // lag one physics frame behind the vehicle.
            contact_position: Some(point),
            contact_normal: Some(normal),
            compression: compression[index],
            suspension_velocity: suspension_velocity[index],
            normal_force: load,
            longitudinal_slip: long_slip,
            lateral_slip_angle: lat_slip,
            slip_intensity: combined_slip_intensity(long_slip, lat_slip, peak_lat)
                .max(response.slip_intensity),
            longitudinal_force: long_force,
            lateral_force: side_force,
            friction_utilization: response.friction_utilization,
            wheel_locked: response.locked,
            sliding_speed_mps: response.sliding_speed,
            tire_temperature: vehicle.damage.wheels[index].tyre_temperature,
            angular_velocity: state.angular_velocity,
            rotation_angle: state.rotation_angle,
            steer_angle,
            surface_entity: contact.surface_entity,
            surface_id: contact.surface_id,
            surface_class: contact.surface_class,
            surface_grip_multiplier: contact.grip_multiplier,
        };
    }

    if vehicle.definition.class == VehicleClass::Bike {
        let speed_gain = (vehicle.speed_mps / 8.0).clamp(0.0, 1.0);
        let lean_error = dot(cross(up, WORLD_UP), forward);
        let roll_rate = dot(body.angular_velocity, forward);
        let desired_lean = -vehicle.input.steer * (vehicle.speed_mps / 20.0).clamp(0.0, 0.7);
        let correction =
            (desired_lean - lean_error) * 3.5 * speed_gain - roll_rate * 1.8 * speed_gain;
        plan.angular_velocity_deltas
            .push(VehicleAngularVelocityDelta {
                vehicle: entity,
                delta: mul(forward, correction * dt),
            });
    }
}

pub(super) fn append_wheel_probes(
    next_probe_seq: &mut u64,
    entity: VehicleEntity,
    vehicle: &VehicleInstance,
    body: VehicleBodyState,
    up: Vec3,
    plan: &mut VehicleFramePlan,
    routes: &mut BTreeMap<u64, ProbeRoute>,
) {
    let down = mul(up, -1.0);
    for (wheel_index, wheel) in vehicle.definition.wheels.iter().enumerate() {
        let condition = vehicle.wheels[wheel_index].tire_condition;
        let rubber_remaining = vehicle
            .damage
            .wheels
            .get(wheel_index)
            .map_or(1.0, |damage| damage.tyre_rubber_remaining);
        if condition == TireCondition::Missing {
            continue;
        }
        let mount = add(body.position, rotate_vec(body.rotation, wheel.mount_local));
        let origin = add(mount, mul(up, wheel.travel_up));
        let max_distance = wheel.travel_up
            + wheel.rest_length
            + wheel.travel_down
            + wheel.radius * condition.radius_multiplier_with_rubber(rubber_remaining);
        let seq = *next_probe_seq;
        *next_probe_seq = next_probe_seq.wrapping_add(1).max(0x5645_4800_0000_0001);
        plan.probes.push(VehicleProbe {
            seq,
            vehicle: entity,
            wheel_index,
            origin,
            direction: down,
            max_distance,
        });
        routes.insert(
            seq,
            ProbeRoute {
                vehicle: entity,
                wheel_index,
                direction: down,
            },
        );
    }
}

pub(super) fn apply_drag_and_downforce(
    entity: VehicleEntity,
    vehicle: &VehicleInstance,
    body: VehicleBodyState,
    up: Vec3,
    dt: f32,
    gravity: f32,
    plan: &mut VehicleFramePlan,
) {
    let handling = &vehicle.definition.handling;
    let speed = length(body.linear_velocity);
    if speed > 0.01 {
        let drag_force = handling.drag_coefficient * handling.mass * speed * speed * 0.65;
        plan.impulses.push(VehicleImpulse {
            vehicle: entity,
            impulse: mul(normalize(body.linear_velocity), -drag_force * dt),
            point: body.position,
        });
    }

    if matches!(
        vehicle.definition.class,
        VehicleClass::Automobile | VehicleClass::Bike
    ) && handling.downforce_modifier > 0.0
    {
        let speed_ratio = (speed / handling.max_flat_velocity_mps.max(1.0)).clamp(0.0, 1.5);
        let force = handling.mass
            * gravity
            * handling.downforce_modifier
            * speed_ratio
            * speed_ratio
            * 0.35;
        plan.impulses.push(VehicleImpulse {
            vehicle: entity,
            impulse: mul(up, -force * dt),
            point: body.position,
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn simulate_plane(
    entity: VehicleEntity,
    vehicle: &VehicleInstance,
    body: VehicleBodyState,
    forward: Vec3,
    up: Vec3,
    right: Vec3,
    dt: f32,
    gravity: f32,
    plan: &mut VehicleFramePlan,
) {
    let mass = vehicle.definition.handling.mass;
    let aero = &vehicle.definition.aero;
    let forward_speed = dot(body.linear_velocity, forward).max(0.0);
    let lateral_speed = dot(body.linear_velocity, right);
    let top = vehicle.definition.handling.max_flat_velocity_mps.max(20.0);
    let ratio = (forward_speed / top).clamp(0.0, 2.0);

    let thrust = vehicle.input.throttle.max(0.0)
        * mass
        * gravity
        * 2.4
        * aero.thrust_multiplier
        * vehicle.motor_output();
    let lift =
        mass * gravity * aero.lift_multiplier * ratio * ratio * (1.0 + vehicle.input.pitch * 0.15);
    let side_force = -lateral_speed * mass * aero.side_slip_multiplier * 0.8;
    plan.impulses.push(VehicleImpulse {
        vehicle: entity,
        impulse: mul(
            add(
                mul(forward, thrust),
                add(mul(up, lift), mul(right, side_force)),
            ),
            dt,
        ),
        point: body.position,
    });

    let roll_rate = dot(body.angular_velocity, forward);
    let pitch_rate = dot(body.angular_velocity, right);
    let yaw_rate = dot(body.angular_velocity, up);
    let gain = (0.25 + ratio).clamp(0.25, 1.5);
    plan.angular_velocity_deltas
        .push(VehicleAngularVelocityDelta {
            vehicle: entity,
            delta: add(
                mul(
                    forward,
                    (vehicle.input.roll * aero.roll_multiplier * gain
                        - roll_rate * aero.roll_stabilize)
                        * dt,
                ),
                add(
                    mul(
                        right,
                        (vehicle.input.pitch * aero.pitch_multiplier * gain
                            - pitch_rate * aero.pitch_stabilize)
                            * dt,
                    ),
                    mul(
                        up,
                        (vehicle.input.yaw * aero.yaw_multiplier * gain
                            - yaw_rate * aero.yaw_stabilize)
                            * dt,
                    ),
                ),
            ),
        });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn simulate_helicopter(
    entity: VehicleEntity,
    vehicle: &VehicleInstance,
    body: VehicleBodyState,
    forward: Vec3,
    up: Vec3,
    right: Vec3,
    dt: f32,
    gravity: f32,
    plan: &mut VehicleFramePlan,
) {
    let mass = vehicle.definition.handling.mass;
    let aero = &vehicle.definition.aero;
    let collective = if vehicle.input.collective > 0.0 {
        vehicle.input.collective
    } else {
        ((vehicle.input.throttle + 1.0) * 0.5).clamp(0.0, 1.0)
    };
    let power = vehicle.motor_output();
    let rotor = mass * gravity * collective * 2.0 * aero.lift_multiplier * power;
    let cyclic = mass * gravity * 0.35 * power;
    plan.impulses.push(VehicleImpulse {
        vehicle: entity,
        impulse: mul(
            add(
                mul(up, rotor),
                add(
                    mul(forward, -vehicle.input.pitch * cyclic),
                    mul(right, vehicle.input.roll * cyclic),
                ),
            ),
            dt,
        ),
        point: body.position,
    });

    let roll_rate = dot(body.angular_velocity, forward);
    let pitch_rate = dot(body.angular_velocity, right);
    let yaw_rate = dot(body.angular_velocity, up);
    plan.angular_velocity_deltas
        .push(VehicleAngularVelocityDelta {
            vehicle: entity,
            delta: add(
                mul(
                    forward,
                    (vehicle.input.roll * 2.2 * power - roll_rate * aero.roll_stabilize) * dt,
                ),
                add(
                    mul(
                        right,
                        (-vehicle.input.pitch * 2.2 * power - pitch_rate * aero.pitch_stabilize)
                            * dt,
                    ),
                    mul(
                        up,
                        (vehicle.input.yaw * 1.8 * power - yaw_rate * aero.yaw_stabilize) * dt,
                    ),
                ),
            ),
        });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn simulate_boat(
    entity: VehicleEntity,
    vehicle: &VehicleInstance,
    body: VehicleBodyState,
    forward: Vec3,
    up: Vec3,
    right: Vec3,
    dt: f32,
    gravity: f32,
    plan: &mut VehicleFramePlan,
) {
    let mass = vehicle.definition.handling.mass;
    let water = &vehicle.definition.water;
    let thrust = vehicle.input.throttle
        * mass
        * gravity
        * 0.55
        * water.thrust_multiplier
        * vehicle.motor_output();
    let buoyancy = mass * gravity * water.buoyancy_ratio;
    plan.impulses.push(VehicleImpulse {
        vehicle: entity,
        impulse: mul(add(mul(forward, thrust), mul(up, buoyancy)), dt),
        point: body.position,
    });

    let speed_gain = (length(body.linear_velocity) / 8.0).clamp(0.0, 2.0);
    let yaw_rate = dot(body.angular_velocity, up);
    plan.angular_velocity_deltas
        .push(VehicleAngularVelocityDelta {
            vehicle: entity,
            delta: mul(
                up,
                (vehicle.input.steer * water.rudder_force * speed_gain - yaw_rate * 0.35) * dt,
            ),
        });

    let local = [
        dot(body.linear_velocity, right),
        dot(body.linear_velocity, up),
        dot(body.linear_velocity, forward),
    ];
    let resistance = add(
        mul(right, -local[0] * water.move_resistance[0] * mass * 0.1),
        add(
            mul(up, -local[1] * water.move_resistance[1] * mass * 0.1),
            mul(forward, -local[2] * water.move_resistance[2] * mass * 0.1),
        ),
    );
    plan.impulses.push(VehicleImpulse {
        vehicle: entity,
        impulse: mul(resistance, dt),
        point: body.position,
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn simulate_submarine(
    entity: VehicleEntity,
    vehicle: &VehicleInstance,
    body: VehicleBodyState,
    forward: Vec3,
    up: Vec3,
    right: Vec3,
    dt: f32,
    gravity: f32,
    plan: &mut VehicleFramePlan,
) {
    simulate_boat(entity, vehicle, body, forward, up, right, dt, gravity, plan);
    let mass = vehicle.definition.handling.mass;
    plan.impulses.push(VehicleImpulse {
        vehicle: entity,
        impulse: mul(up, -vehicle.input.pitch * mass * gravity * 0.25 * dt),
        point: body.position,
    });
    plan.angular_velocity_deltas
        .push(VehicleAngularVelocityDelta {
            vehicle: entity,
            delta: add(
                mul(right, vehicle.input.pitch * 0.8 * dt),
                mul(forward, vehicle.input.roll * 0.5 * dt),
            ),
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ackermann_inside_wheel_turns_more_than_outside() {
        let definition = VehicleDefinition::automobile();
        let left = &definition.wheels[0];
        let right = &definition.wheels[1];
        let left_angle =
            steering_angle_for_wheel(&definition.handling, &definition.wheels, left, 1.0, 4.0);
        let right_angle =
            steering_angle_for_wheel(&definition.handling, &definition.wheels, right, 1.0, 4.0);
        assert!(left_angle < 0.0 && right_angle < 0.0);
        assert!(right_angle.abs() > left_angle.abs());
        let opposite_left =
            steering_angle_for_wheel(&definition.handling, &definition.wheels, left, -1.0, 4.0);
        let opposite_right =
            steering_angle_for_wheel(&definition.handling, &definition.wheels, right, -1.0, 4.0);
        assert!(opposite_left > opposite_right && opposite_right > 0.0);
        assert!((opposite_left + right_angle).abs() < 1.0e-6);
        assert!((opposite_right + left_angle).abs() < 1.0e-6);
    }

    #[test]
    fn steering_lock_reduces_progressively_with_speed() {
        let definition = VehicleDefinition::automobile();
        let wheel = &definition.wheels[0];
        let slow =
            steering_angle_for_wheel(&definition.handling, &definition.wheels, wheel, 1.0, 2.0);
        let fast =
            steering_angle_for_wheel(&definition.handling, &definition.wheels, wheel, 1.0, 50.0);
        assert!(fast.abs() < slow.abs());
        assert!(fast.abs() > definition.handling.steering_lock_rad * 0.25);
    }

    #[test]
    fn combined_slip_is_one_at_longitudinal_peak() {
        let intensity = combined_slip_intensity(0.12, 0.0, 0.10);
        assert!((intensity - 1.0).abs() < 1.0e-6);
        assert!(combined_slip_intensity(0.12, 0.10, 0.10) > 1.4);
    }

    #[test]
    fn slip_relaxation_moves_toward_target_without_overshoot() {
        let next = relaxed_slip(0.0, 1.0, 1.0 / 60.0, 20.0);
        assert!(next > 0.0 && next < 1.0);
        let reverse = relaxed_slip(next, -1.0, 1.0 / 60.0, 20.0);
        assert!(reverse < next && reverse > -1.0);
    }
}
