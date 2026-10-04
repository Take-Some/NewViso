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

        let Some(contact) = state.contact else {
            if config.driven {
                state.angular_velocity +=
                    total_drive_force.signum() * total_drive_force.abs() * 0.0007 * dt
                        / radius.max(0.05);
            }
            state.angular_velocity *= (1.0 - dt * 0.15).max(0.0);
            state.rotation_angle = wrap_angle(state.rotation_angle + state.angular_velocity * dt);
            state.telemetry.angular_velocity = state.angular_velocity;
            state.telemetry.rotation_angle = state.rotation_angle;
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
        let point = add(probe_origin, mul(suspension_down, contact.distance));
        let offset = sub(point, body.position);
        let point_velocity = add(body.linear_velocity, cross(body.angular_velocity, offset));
        let fwd_speed = dot(point_velocity, tyre_forward);
        let side_speed = dot(point_velocity, tyre_side);

        if state.angular_velocity.abs() < 0.01 {
            state.angular_velocity = fwd_speed / radius;
        }
        let wheel_linear = state.angular_velocity * radius;
        // Use wheel and road speed in the denominator so launch and burnout
        // slip stays finite around zero vehicle speed.
        let reference_speed = fwd_speed.abs().max(wheel_linear.abs()).max(1.0);
        let raw_long_slip = (wheel_linear - fwd_speed) / reference_speed;
        let raw_lat_slip = (-side_speed).atan2(fwd_speed.abs().max(0.75));
        let long_slip = relaxed_slip(
            state.telemetry.longitudinal_slip,
            raw_long_slip,
            dt,
            vehicle.speed_mps,
        );
        let lat_slip = relaxed_slip(
            state.telemetry.lateral_slip_angle,
            raw_lat_slip,
            dt,
            vehicle.speed_mps,
        );

        let traction_bias = if config.front {
            handling.traction_bias_front * 2.0
        } else {
            (1.0 - handling.traction_bias_front) * 2.0
        }
        .max(0.05);

        let low_speed_loss = if handling.low_speed_traction_loss_mult > 0.0 {
            let ratio = (vehicle.speed_mps / 8.0).clamp(0.0, 1.0);
            (1.0 - (1.0 - ratio)
                * handling.low_speed_traction_loss_mult.clamp(0.0, 1.0)
                * vehicle.input.throttle.abs())
            .max(0.2)
        } else {
            1.0
        };
        let traction_loss = (handling.traction_loss_mult * low_speed_loss).max(0.05);

        let peak_lat = (handling.traction_curve_lateral_rad * 0.45).max(0.03);
        let end_lat = handling.traction_curve_lateral_rad.max(peak_lat + 0.01);
        let load = normal_force[index];
        // Loaded tyres gain force sub-linearly. Mild load sensitivity keeps the
        // outside tyre dominant without making the inside tyre unrealistically sticky.
        let load_sensitivity = (static_load / load.max(static_load * 0.25))
            .powf(0.065)
            .clamp(0.82, 1.12);
        let handbrake_lateral = if config.handbrake {
            (1.0 - vehicle.input.handbrake * 0.58).clamp(0.36, 1.0)
        } else {
            1.0
        };
        let lateral_mu = traction_coefficient(
            lat_slip,
            peak_lat,
            end_lat,
            handling.traction_curve_max,
            handling.traction_curve_min,
        ) * traction_bias
            * traction_loss
            * config.grip_multiplier
            * contact.grip_multiplier
            * tire_grip
            * load_sensitivity
            * handbrake_lateral;
        let longitudinal_mu = traction_coefficient(
            long_slip,
            0.12,
            0.42,
            handling.traction_curve_max,
            handling.traction_curve_min,
        ) * traction_bias
            * traction_loss
            * config.grip_multiplier
            * contact.grip_multiplier
            * tire_grip
            * load_sensitivity;

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
        let drive_request = if config.driven {
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
            handling.handbrake_force * handling.mass * gravity * vehicle.input.handbrake
                / handbrake_wheels
        } else {
            0.0
        };

        // A brake dissipates motion; it must never accelerate through zero.
        // The old low-speed dead zone let residual wheel spin push parked cars,
        // and the explicit torque step could reverse a locked wheel each tick.
        let braking = brake_request + handbrake_request;
        let brake_force = (-fwd_speed * mass_per_wheel / dt).clamp(-braking, braking);
        // Apply extra rolling drag at this wheel's contact patch. The off-centre
        // impulse changes yaw naturally; cap it so it cannot reverse motion.
        let resistance = state
            .tire_condition
            .rolling_resistance_with_rubber(rubber_remaining)
            * normal_force[index];
        let rolling_drag = (-fwd_speed * mass_per_wheel / dt).clamp(-resistance, resistance);
        let requested_long = drive_request + brake_force + rolling_drag;
        let wheel_inertia = (0.5 * mass_per_wheel * 0.08 * radius * radius).max(0.05);
        let slip_mass = 1.0 / (1.0 / mass_per_wheel + radius * radius / wheel_inertia);
        let slip_limit = (wheel_linear - fwd_speed).abs() * slip_mass / dt;
        let slip_force = if braking > EPSILON {
            0.0
        } else {
            long_slip.signum() * (longitudinal_mu * load).min(slip_limit)
        };

        let mut long_force = requested_long + slip_force;
        // traction_coefficient already includes the build-up to the peak.
        // Multiplying by slip again made the old response quadratic and caused
        // delayed steering followed by snap breakaway.
        let mut side_force = lat_slip.signum() * lateral_mu * load;
        if side_force * side_speed > 0.0 {
            side_force = -side_force;
        }

        let capacity = load
            * handling.traction_curve_max
            * traction_bias
            * traction_loss
            * config.grip_multiplier
            * contact.grip_multiplier
            * tire_grip;
        let combined = (long_force * long_force + side_force * side_force).sqrt();
        if combined > capacity && combined > EPSILON {
            let scale = capacity / combined;
            long_force *= scale;
            side_force *= scale;
        }

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

        let reaction_torque = -long_force * radius;
        let drive_torque = drive_request * radius;
        let brake_torque = (brake_request + handbrake_request) * radius;
        state.angular_velocity += (drive_torque + reaction_torque) / wheel_inertia * dt;
        let spin = state.angular_velocity;
        let brake_step = brake_torque / wheel_inertia * dt;
        state.angular_velocity = spin.signum() * (spin.abs() - brake_step).max(0.0);
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
            slip_intensity: combined_slip_intensity(long_slip, lat_slip, peak_lat),
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
