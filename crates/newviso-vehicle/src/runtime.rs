use crate::math::*;
use crate::types::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
struct WheelContact {
    distance: f32,
    normal: Vec3,
    surface_entity: Option<u64>,
    surface_id: Option<u32>,
    surface_class: VehicleSurfaceClass,
    grip_multiplier: f32,
}

#[derive(Clone, Debug, Default)]
struct WheelState {
    contact: Option<WheelContact>,
    compression: f32,
    old_compression: f32,
    angular_velocity: f32,
    rotation_angle: f32,
    telemetry: WheelTelemetry,
}

#[derive(Clone, Copy, Debug)]
struct TransmissionState {
    gear: i8,
    engine_speed: f32,
    clutch: f32,
    shift_timer: f32,
}

impl Default for TransmissionState {
    fn default() -> Self {
        Self {
            gear: 1,
            engine_speed: 0.0,
            clutch: 1.0,
            shift_timer: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
struct VehicleInstance {
    definition: VehicleDefinition,
    input: VehicleInput,
    enabled: bool,
    transmission: TransmissionState,
    wheels: Vec<WheelState>,
    speed_mps: f32,
    speed_forward_mps: f32,
}

#[derive(Clone, Copy, Debug)]
struct ProbeRoute {
    vehicle: VehicleEntity,
    wheel_index: usize,
}

#[derive(Clone, Debug)]
pub struct VehicleRuntime {
    vehicles: BTreeMap<VehicleEntity, VehicleInstance>,
    pending_probe_routes: BTreeMap<u64, ProbeRoute>,
    next_probe_seq: u64,
    surface_profiles: BTreeMap<u32, VehicleSurfaceProfile>,
    default_surface_profile: VehicleSurfaceProfile,
    surface_wetness: f32,
    surface_snow: f32,
}

impl Default for VehicleRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl VehicleRuntime {
    pub fn new() -> Self {
        Self {
            vehicles: BTreeMap::new(),
            pending_probe_routes: BTreeMap::new(),
            next_probe_seq: 0x5645_4800_0000_0001,
            surface_profiles: BTreeMap::new(),
            default_surface_profile: VehicleSurfaceProfile::default(),
            surface_wetness: 0.0,
            surface_snow: 0.0,
        }
    }

    pub fn upsert(
        &mut self,
        entity: VehicleEntity,
        definition: VehicleDefinition,
    ) -> Result<(), String> {
        definition.validate()?;
        let wheel_count = definition.wheels.len();
        match self.vehicles.get_mut(&entity) {
            Some(vehicle) => {
                let wheel_layout_changed = vehicle.wheels.len() != wheel_count;
                vehicle.definition = definition;
                if wheel_layout_changed {
                    vehicle.wheels = vec![WheelState::default(); wheel_count];
                }
            }
            None => {
                self.vehicles.insert(
                    entity,
                    VehicleInstance {
                        definition,
                        input: VehicleInput::default(),
                        enabled: true,
                        transmission: TransmissionState::default(),
                        wheels: vec![WheelState::default(); wheel_count],
                        speed_mps: 0.0,
                        speed_forward_mps: 0.0,
                    },
                );
            }
        }
        Ok(())
    }

    pub fn remove(&mut self, entity: VehicleEntity) -> bool {
        self.pending_probe_routes
            .retain(|_, route| route.vehicle != entity);
        self.vehicles.remove(&entity).is_some()
    }

    pub fn contains(&self, entity: VehicleEntity) -> bool {
        self.vehicles.contains_key(&entity)
    }

    pub fn set_enabled(&mut self, entity: VehicleEntity, enabled: bool) -> Result<(), String> {
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
        vehicle.enabled = enabled;
        Ok(())
    }

    pub fn set_input(&mut self, entity: VehicleEntity, input: VehicleInput) -> Result<(), String> {
        let vehicle = self
            .vehicles
            .get_mut(&entity)
            .ok_or_else(|| format!("unknown vehicle entity {entity}"))?;
        vehicle.input = input.sanitized();
        Ok(())
    }

    pub fn set_surface_profile(
        &mut self,
        surface_id: u32,
        profile: VehicleSurfaceProfile,
    ) -> Result<(), String> {
        self.surface_profiles
            .insert(surface_id, profile.validate()?);
        Ok(())
    }

    pub fn remove_surface_profile(&mut self, surface_id: u32) -> bool {
        self.surface_profiles.remove(&surface_id).is_some()
    }

    pub fn clear_surface_profiles(&mut self) {
        self.surface_profiles.clear();
    }

    pub fn set_default_surface_profile(
        &mut self,
        profile: VehicleSurfaceProfile,
    ) -> Result<(), String> {
        self.default_surface_profile = profile.validate()?;
        Ok(())
    }

    pub fn set_surface_weather(&mut self, wetness: f32, snow: f32) -> Result<(), String> {
        if !wetness.is_finite() || !snow.is_finite() {
            return Err("vehicle surface weather contains non-finite values".to_owned());
        }
        self.surface_wetness = wetness.clamp(0.0, 1.0);
        self.surface_snow = snow.clamp(0.0, 1.0);
        Ok(())
    }

    pub fn surface_profile(&self, surface_id: Option<u32>) -> VehicleSurfaceProfile {
        surface_id
            .and_then(|id| self.surface_profiles.get(&id).copied())
            .unwrap_or(self.default_surface_profile)
    }

    pub fn entity_ids(&self) -> impl Iterator<Item = VehicleEntity> + '_ {
        self.vehicles.keys().copied()
    }

    pub fn definition(&self, entity: VehicleEntity) -> Option<&VehicleDefinition> {
        self.vehicles
            .get(&entity)
            .map(|vehicle| &vehicle.definition)
    }

    pub fn prepare_frame(
        &mut self,
        dt: f32,
        gravity: f32,
        bodies: &BTreeMap<VehicleEntity, VehicleBodyState>,
    ) -> VehicleFramePlan {
        let dt = dt.clamp(1.0 / 1000.0, 0.1);
        let gravity = gravity.abs().max(0.01);
        let mut plan = VehicleFramePlan::default();
        let mut fresh_routes = BTreeMap::new();

        for (&entity, vehicle) in &mut self.vehicles {
            if !vehicle.enabled {
                continue;
            }
            let Some(body) = bodies.get(&entity).copied() else {
                continue;
            };

            let forward = normalize_or(
                rotate_vec(body.rotation, vehicle.definition.forward_local),
                [0.0, 0.0, -1.0],
            );
            let up = normalize_or(
                rotate_vec(body.rotation, vehicle.definition.up_local),
                WORLD_UP,
            );
            let right = normalize_or(cross(forward, up), [1.0, 0.0, 0.0]);

            vehicle.speed_mps = length(body.linear_velocity);
            vehicle.speed_forward_mps = dot(body.linear_velocity, forward);
            update_transmission(
                &vehicle.definition.handling,
                vehicle.input,
                vehicle.speed_forward_mps,
                dt,
                &mut vehicle.transmission,
            );

            match vehicle.definition.class {
                VehicleClass::Automobile
                | VehicleClass::Bike
                | VehicleClass::Train
                | VehicleClass::Trailer => {
                    simulate_ground_vehicle(
                        entity, vehicle, body, forward, up, dt, gravity, &mut plan,
                    );
                    append_wheel_probes(
                        &mut self.next_probe_seq,
                        entity,
                        vehicle,
                        body,
                        up,
                        &mut plan,
                        &mut fresh_routes,
                    );
                }
                VehicleClass::Plane => {
                    simulate_plane(
                        entity, vehicle, body, forward, up, right, dt, gravity, &mut plan,
                    );
                }
                VehicleClass::Helicopter => {
                    simulate_helicopter(
                        entity, vehicle, body, forward, up, right, dt, gravity, &mut plan,
                    );
                }
                VehicleClass::Boat => {
                    simulate_boat(
                        entity, vehicle, body, forward, up, right, dt, gravity, &mut plan,
                    );
                }
                VehicleClass::Submarine => {
                    simulate_submarine(
                        entity, vehicle, body, forward, up, right, dt, gravity, &mut plan,
                    );
                }
            }

            apply_drag_and_downforce(entity, vehicle, body, up, dt, gravity, &mut plan);
        }

        self.pending_probe_routes = fresh_routes;
        plan
    }

    pub fn accept_probe_hits(&mut self, hits: &[VehicleProbeHit]) {
        // Every pending query belongs to the last generated suspension batch.
        // Missing hits explicitly clear contact, so stale ground support cannot
        // survive after a wheel leaves a ledge.
        for route in self.pending_probe_routes.values().copied() {
            if let Some(wheel) = self
                .vehicles
                .get_mut(&route.vehicle)
                .and_then(|vehicle| vehicle.wheels.get_mut(route.wheel_index))
            {
                wheel.contact = None;
                wheel.telemetry.contact = false;
                wheel.telemetry.surface_entity = None;
                wheel.telemetry.surface_id = None;
                wheel.telemetry.surface_class = VehicleSurfaceClass::Default;
                wheel.telemetry.surface_grip_multiplier = 0.0;
            }
        }

        let mut nearest = BTreeMap::<u64, VehicleProbeHit>::new();
        for &hit in hits {
            if !hit.distance.is_finite()
                || hit.distance < 0.0
                || !self.pending_probe_routes.contains_key(&hit.seq)
            {
                continue;
            }
            nearest
                .entry(hit.seq)
                .and_modify(|current| {
                    if hit.distance < current.distance {
                        *current = hit;
                    }
                })
                .or_insert(hit);
        }

        for (seq, hit) in nearest {
            let Some(route) = self.pending_probe_routes.get(&seq).copied() else {
                continue;
            };
            let profile = self.surface_profile(hit.surface_id);
            let grip_multiplier = profile.grip(self.surface_wetness, self.surface_snow);
            let Some(wheel) = self
                .vehicles
                .get_mut(&route.vehicle)
                .and_then(|vehicle| vehicle.wheels.get_mut(route.wheel_index))
            else {
                continue;
            };
            wheel.contact = Some(WheelContact {
                distance: hit.distance,
                normal: normalize_or(hit.normal, WORLD_UP),
                surface_entity: hit.surface_entity,
                surface_id: hit.surface_id,
                surface_class: profile.class,
                grip_multiplier,
            });
            wheel.telemetry.contact = true;
            wheel.telemetry.surface_entity = hit.surface_entity;
            wheel.telemetry.surface_id = hit.surface_id;
            wheel.telemetry.surface_class = profile.class;
            wheel.telemetry.surface_grip_multiplier = grip_multiplier;
        }
    }

    pub fn telemetry(&self, entity: VehicleEntity) -> Option<VehicleTelemetry> {
        let vehicle = self.vehicles.get(&entity)?;
        Some(VehicleTelemetry {
            entity,
            class: vehicle.definition.class,
            speed_mps: vehicle.speed_mps,
            speed_forward_mps: vehicle.speed_forward_mps,
            gear: vehicle.transmission.gear,
            engine_speed: vehicle.transmission.engine_speed,
            clutch: vehicle.transmission.clutch,
            input: vehicle.input,
            wheels: vehicle.wheels.iter().map(|wheel| wheel.telemetry).collect(),
        })
    }

    pub fn runtime_state(&self) -> serde_json::Value {
        let vehicles = self
            .vehicles
            .iter()
            .map(|(&entity, vehicle)| {
                serde_json::to_value(VehicleTelemetry {
                    entity,
                    class: vehicle.definition.class,
                    speed_mps: vehicle.speed_mps,
                    speed_forward_mps: vehicle.speed_forward_mps,
                    gear: vehicle.transmission.gear,
                    engine_speed: vehicle.transmission.engine_speed,
                    clutch: vehicle.transmission.clutch,
                    input: vehicle.input,
                    wheels: vehicle.wheels.iter().map(|wheel| wheel.telemetry).collect(),
                })
                .expect("vehicle telemetry must serialize")
            })
            .collect::<Vec<_>>();

        serde_json::json!({
            "schema": "newviso.vehicle.runtime.v1",
            "count": vehicles.len(),
            "pending_probes": self.pending_probe_routes.len(),
            "surface_policy": {
                "mapped_surfaces": self.surface_profiles.len(),
                "wetness": self.surface_wetness,
                "snow": self.surface_snow,
                "default": self.default_surface_profile
            },
            "vehicles": vehicles,
        })
    }
}

fn update_transmission(
    handling: &HandlingData,
    input: VehicleInput,
    speed_forward: f32,
    dt: f32,
    state: &mut TransmissionState,
) {
    let gears = handling.initial_drive_gears.max(1) as i8;
    let speed = speed_forward.abs();
    let top = handling.max_gearing_velocity_mps.max(1.0);
    let reverse = input.throttle < -0.05 && speed_forward < 1.5;

    let target = if reverse {
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
    let target_engine = speed_ratio.max(input.throttle.abs() * 0.25);
    let response = (handling.drive_inertia * 8.0 * dt).clamp(0.0, 1.0);
    state.engine_speed += (target_engine - state.engine_speed) * response;
    state.engine_speed = state.engine_speed.clamp(0.0, 1.25);
}

#[allow(clippy::too_many_arguments)]
fn simulate_ground_vehicle(
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
    let wheel_count = vehicle.wheels.len().max(1);
    let mass_per_wheel = handling.mass / wheel_count as f32;
    let static_load = mass_per_wheel * gravity;

    let mut compression = vec![0.0; wheel_count];
    let mut suspension_velocity = vec![0.0; wheel_count];
    let mut normal_force = vec![0.0; wheel_count];

    for index in 0..wheel_count {
        let config = &vehicle.definition.wheels[index];
        let state = &mut vehicle.wheels[index];
        state.old_compression = state.compression;

        let Some(contact) = state.contact else {
            state.compression = 0.0;
            state.telemetry.contact = false;
            state.telemetry.normal_force = 0.0;
            continue;
        };

        let current_length = (contact.distance - config.radius - config.travel_up).max(0.0);
        let raw = config.rest_length - current_length + handling.suspension_raise;
        let current = raw.clamp(-config.travel_down, config.travel_up);
        state.compression = current;
        compression[index] = current;
        suspension_velocity[index] = (current - state.old_compression) / dt;

        let axle_bias = if config.front {
            handling.suspension_bias_front * 2.0
        } else {
            (1.0 - handling.suspension_bias_front) * 2.0
        }
        .max(0.05);

        let spring_rate =
            static_load * handling.suspension_force * axle_bias / config.rest_length.max(0.05);
        let damping_ratio = if suspension_velocity[index] >= 0.0 {
            handling.suspension_comp_damp
        } else {
            handling.suspension_rebound_damp
        };
        let critical_damping = 2.0 * (spring_rate * mass_per_wheel).sqrt();
        let damping_force = critical_damping * damping_ratio * suspension_velocity[index];

        let mut force = spring_rate * current.max(0.0) + damping_force;
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
        normal_force[index] = (normal_force[index] - transfer).max(0.0);
        normal_force[other] = (normal_force[other] + transfer).max(0.0);
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
        * vehicle.transmission.clutch;

    let handbrake_wheels = vehicle
        .definition
        .wheels
        .iter()
        .filter(|wheel| wheel.handbrake)
        .count()
        .max(1) as f32;

    for index in 0..wheel_count {
        let config = &vehicle.definition.wheels[index];
        let state = &mut vehicle.wheels[index];

        let Some(contact) = state.contact else {
            if config.driven {
                state.angular_velocity +=
                    total_drive_force.signum() * total_drive_force.abs() * 0.0007 * dt
                        / config.radius.max(0.05);
            }
            state.angular_velocity *= (1.0 - dt * 0.15).max(0.0);
            state.rotation_angle = wrap_angle(state.rotation_angle + state.angular_velocity * dt);
            state.telemetry.angular_velocity = state.angular_velocity;
            state.telemetry.rotation_angle = state.rotation_angle;
            continue;
        };

        let steer_angle = if config.steered {
            vehicle.input.steer * handling.steering_lock_rad
        } else {
            0.0
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
            state.angular_velocity = fwd_speed / config.radius;
        }
        let wheel_linear = state.angular_velocity * config.radius;
        let long_slip = (wheel_linear - fwd_speed) / fwd_speed.abs().max(1.0);
        let lat_slip = (-side_speed).atan2(fwd_speed.abs().max(1.0));

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
        let lateral_mu = traction_coefficient(
            lat_slip,
            peak_lat,
            end_lat,
            handling.traction_curve_max,
            handling.traction_curve_min,
        ) * traction_bias
            * traction_loss
            * config.grip_multiplier
            * contact.grip_multiplier;
        let longitudinal_mu = traction_coefficient(
            long_slip,
            0.12,
            0.42,
            handling.traction_curve_max,
            handling.traction_curve_min,
        ) * traction_bias
            * traction_loss
            * config.grip_multiplier
            * contact.grip_multiplier;

        let load = normal_force[index];
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

        let brake_direction = if fwd_speed.abs() > 0.2 {
            -fwd_speed.signum()
        } else {
            -drive_request.signum()
        };
        let requested_long = drive_request + brake_direction * (brake_request + handbrake_request);
        let slip_force = long_slip.signum() * longitudinal_mu * load * long_slip.abs().min(1.0);

        let mut long_force = requested_long + slip_force;
        let mut side_force = lat_slip.signum() * lateral_mu * load * lat_slip.abs().min(1.0);
        if side_force * side_speed > 0.0 {
            side_force = -side_force;
        }

        let capacity = load
            * handling.traction_curve_max
            * traction_bias
            * traction_loss
            * config.grip_multiplier
            * contact.grip_multiplier;
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
        plan.impulses.push(VehicleImpulse {
            vehicle: entity,
            impulse: mul(force, dt),
            point,
        });

        let wheel_inertia = (0.5 * mass_per_wheel * 0.08 * config.radius * config.radius).max(0.05);
        let reaction_torque = -long_force * config.radius;
        let drive_torque = drive_request * config.radius;
        let brake_torque = (brake_request + handbrake_request) * config.radius;
        let brake_sign = if state.angular_velocity.abs() > 0.1 {
            state.angular_velocity.signum()
        } else {
            fwd_speed.signum()
        };
        state.angular_velocity +=
            (drive_torque + reaction_torque - brake_sign * brake_torque) / wheel_inertia * dt;
        let max_spin = handling.max_gearing_velocity_mps * 2.0 / config.radius.max(0.05);
        state.angular_velocity = state.angular_velocity.clamp(-max_spin, max_spin);
        state.rotation_angle = wrap_angle(state.rotation_angle + state.angular_velocity * dt);

        state.telemetry = WheelTelemetry {
            contact: true,
            compression: compression[index],
            suspension_velocity: suspension_velocity[index],
            normal_force: load,
            longitudinal_slip: long_slip,
            lateral_slip_angle: lat_slip,
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

fn append_wheel_probes(
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
        let mount = add(body.position, rotate_vec(body.rotation, wheel.mount_local));
        let origin = add(mount, mul(up, wheel.travel_up));
        let max_distance = wheel.travel_up + wheel.rest_length + wheel.travel_down + wheel.radius;
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
            },
        );
    }
}

fn apply_drag_and_downforce(
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
fn simulate_plane(
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

    let thrust = vehicle.input.throttle.max(0.0) * mass * gravity * 2.4 * aero.thrust_multiplier;
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
fn simulate_helicopter(
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
    let rotor = mass * gravity * collective * 2.0 * aero.lift_multiplier;
    let cyclic = mass * gravity * 0.35;
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
                    (vehicle.input.roll * 2.2 - roll_rate * aero.roll_stabilize) * dt,
                ),
                add(
                    mul(
                        right,
                        (-vehicle.input.pitch * 2.2 - pitch_rate * aero.pitch_stabilize) * dt,
                    ),
                    mul(
                        up,
                        (vehicle.input.yaw * 1.8 - yaw_rate * aero.yaw_stabilize) * dt,
                    ),
                ),
            ),
        });
}

#[allow(clippy::too_many_arguments)]
fn simulate_boat(
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
    let thrust = vehicle.input.throttle * mass * gravity * 0.55 * water.thrust_multiplier;
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
fn simulate_submarine(
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

    fn body() -> VehicleBodyState {
        VehicleBodyState {
            position: [0.0, 1.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            linear_velocity: [0.0, 0.0, -10.0],
            angular_velocity: [0.0; 3],
        }
    }

    #[test]
    fn reference_handling_converts_authoring_units() {
        let handling = HandlingData::from_reference_units(ReferenceHandlingData {
            initial_drive_max_flat_vel: 180.0,
            steering_lock: 30.0,
            initial_drag_coeff: 10.0,
            suspension_comp_damp: 2.0,
            ..ReferenceHandlingData::default()
        });
        assert!((handling.max_flat_velocity_mps - 50.0).abs() < 1.0e-5);
        assert!((handling.max_gearing_velocity_mps - 60.0).abs() < 1.0e-5);
        assert!((handling.steering_lock_rad - 30.0_f32.to_radians()).abs() < 1.0e-6);
        assert!((handling.drag_coefficient - 0.001).abs() < 1.0e-7);
        assert!((handling.suspension_comp_damp - 0.2).abs() < 1.0e-6);
    }

    #[test]
    fn automobile_emits_four_suspension_queries() {
        let mut runtime = VehicleRuntime::new();
        runtime.upsert(42, VehicleDefinition::automobile()).unwrap();
        let bodies = BTreeMap::from([(42, body())]);
        let frame = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
        assert_eq!(frame.probes.len(), 4);
    }

    #[test]
    fn probe_hits_feed_suspension_forces() {
        let mut runtime = VehicleRuntime::new();
        runtime.upsert(7, VehicleDefinition::automobile()).unwrap();
        let bodies = BTreeMap::from([(7, body())]);
        let first = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
        let hits = first
            .probes
            .iter()
            .map(|probe| VehicleProbeHit {
                seq: probe.seq,
                position: add(probe.origin, mul(probe.direction, 0.48)),
                normal: WORLD_UP,
                distance: 0.48,
                surface_entity: Some(99),
                surface_id: None,
            })
            .collect::<Vec<_>>();
        runtime.accept_probe_hits(&hits);
        let second = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
        assert!(second
            .impulses
            .iter()
            .any(|impulse| impulse.impulse[1] > 0.0));
    }

    #[test]
    fn surface_policy_and_weather_reduce_wheel_grip() {
        let mut runtime = VehicleRuntime::new();
        runtime.upsert(7, VehicleDefinition::automobile()).unwrap();
        runtime
            .set_surface_profile(
                55,
                VehicleSurfaceProfile::for_class(VehicleSurfaceClass::Ice),
            )
            .unwrap();
        runtime.set_surface_weather(1.0, 0.0).unwrap();

        let bodies = BTreeMap::from([(7, body())]);
        let first = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
        runtime.accept_probe_hits(
            &first
                .probes
                .iter()
                .map(|probe| VehicleProbeHit {
                    seq: probe.seq,
                    position: add(probe.origin, mul(probe.direction, 0.45)),
                    normal: WORLD_UP,
                    distance: 0.45,
                    surface_entity: Some(99),
                    surface_id: Some(55),
                })
                .collect::<Vec<_>>(),
        );

        let telemetry = runtime.telemetry(7).expect("vehicle telemetry");
        assert!(telemetry.wheels.iter().all(|wheel| {
            wheel.contact
                && wheel.surface_id == Some(55)
                && wheel.surface_class == VehicleSurfaceClass::Ice
                && wheel.surface_grip_multiplier < 0.3
        }));
    }

    #[test]
    fn missing_next_hit_clears_contact() {
        let mut runtime = VehicleRuntime::new();
        runtime.upsert(7, VehicleDefinition::automobile()).unwrap();
        let bodies = BTreeMap::from([(7, body())]);
        let first = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
        runtime.accept_probe_hits(
            &first
                .probes
                .iter()
                .map(|probe| VehicleProbeHit {
                    seq: probe.seq,
                    position: probe.origin,
                    normal: WORLD_UP,
                    distance: 0.45,
                    surface_entity: Some(9),
                    surface_id: None,
                })
                .collect::<Vec<_>>(),
        );
        let _second = runtime.prepare_frame(1.0 / 60.0, 9.81, &bodies);
        runtime.accept_probe_hits(&[]);
        let state = runtime.runtime_state();
        let contacts = state["vehicles"][0]["wheels"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|wheel| wheel["contact"].as_bool() == Some(true))
            .count();
        assert_eq!(contacts, 0);
    }
}
