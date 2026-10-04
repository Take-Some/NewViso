use super::*;

const DOOR_CLOSE_LATCH_RATIO: f32 = 0.018;
const DOOR_SWING_DAMPING: f32 = 2.4;

pub(super) fn drive_parameters(role: ModelFragmentPartRole, closing: bool) -> (f32, f32, f32, f32) {
    match (role, closing) {
        // Closing deliberately carries more momentum into the latch than
        // opening. The reference door drive lets the articulated joint hit the
        // latch with non-zero angular velocity; the latch, not the motor,
        // removes the remaining velocity.
        (ModelFragmentPartRole::Bonnet | ModelFragmentPartRole::Boot, true) => {
            (20.0, 5.8, 1.75, 14.0)
        }
        (ModelFragmentPartRole::Bonnet | ModelFragmentPartRole::Boot, false) => {
            (17.0, 7.4, 1.45, 11.0)
        }
        (_, true) => (30.0, 6.6, 3.05, 24.0),
        (_, false) => (26.0, 9.6, 2.45, 19.0),
    }
}

fn swing_drive(
    role: ModelFragmentPartRole,
    pivot: [f32; 3],
    local_acceleration: [f32; 3],
    local_angular_velocity: [f32; 3],
) -> f32 {
    match role {
        ModelFragmentPartRole::Door => {
            let side = if pivot[0] < 0.0 { -1.0 } else { 1.0 };
            // A side door pivots mostly around the local up axis. Vehicle
            // lateral/longitudinal acceleration and yaw create the dominant
            // inertial hinge moment, matching CCarDoor's "swinging" branch.
            -side * local_acceleration[0] * 0.0065
                + side * local_acceleration[2] * 0.0045
                + side * local_angular_velocity[1] * 0.18
        }
        ModelFragmentPartRole::Bonnet => {
            // Bonnet/boot use their gas-strut style hinge. Longitudinal/vertical
            // acceleration excites the hinge while a weak spring keeps an
            // already-open panel from hovering at arbitrary ratios forever.
            local_acceleration[2] * 0.004 - local_acceleration[1] * 0.003
        }
        ModelFragmentPartRole::Boot => {
            -local_acceleration[2] * 0.004 - local_acceleration[1] * 0.003
        }
        _ => 0.0,
    }
}

impl EngineApplication {
    pub(super) fn advance_vehicle_doors(&mut self, entity: u64) {
        let body = self
            .physics
            .as_ref()
            .and_then(|physics| physics.vehicle_body_state(entity));
        let rotation_degrees = self
            .scene
            .entity_transform_values(entity)
            .map(|(_, rotation, _)| rotation)
            .unwrap_or([0.0; 3]);
        let elapsed = self.elapsed_seconds;

        let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
            return;
        };
        let dt = (elapsed - binding.door_motion_runtime.last_update_seconds).clamp(0.0, 0.1) as f32;
        binding.door_motion_runtime.last_update_seconds = elapsed;
        if dt <= 0.0 {
            return;
        }

        let linear_velocity = body.map(|body| body.linear_velocity).unwrap_or([0.0; 3]);
        let angular_velocity = body.map(|body| body.angular_velocity).unwrap_or([0.0; 3]);
        let world_acceleration = std::array::from_fn(|axis| {
            (linear_velocity[axis] - binding.door_motion_runtime.last_linear_velocity[axis]) / dt
        });
        binding.door_motion_runtime.last_linear_velocity = linear_velocity;
        binding.door_motion_runtime.last_angular_velocity = angular_velocity;

        let local_acceleration = inverse_rotate_euler_xyz(world_acceleration, rotation_degrees);
        let local_angular_velocity = inverse_rotate_euler_xyz(angular_velocity, rotation_degrees);
        let mut audio_events = Vec::<(String, VehicleEventKind, f32)>::new();

        for part in &mut binding.parts {
            let Some(motion) = part.door_motion.as_mut() else {
                continue;
            };
            let previous_ratio = part.open;
            motion.just_latched = false;
            if part.detached_entity.is_some() || !part.visible {
                motion.current_speed = 0.0;
                motion.driven = false;
                motion.swinging = false;
                motion.latched = false;
                continue;
            }

            // Cabin controls are persistent driven targets. Direct part.set also
            // writes motion.target_ratio, so both API surfaces share one state.
            if let Some(target) = binding.cabin.targets.get(&part.name_lower).copied() {
                motion.target_ratio = target.clamp(0.0, 1.0);
                motion.driven = true;
                motion.swinging = false;
            }

            if !part.loose && motion.target_ratio > DOOR_CLOSE_LATCH_RATIO + 0.01 {
                motion.latched = false;
            }

            // SetLooseLatch in the reference keeps the panel retained by the
            // latch while rendering it at a tiny authored opening angle.
            if part.loose && motion.latched && !motion.driven && !motion.swinging {
                part.open = part.open.max(loose_latched_ratio(part.role));
                motion.current_speed = 0.0;
                motion.target_ratio = part.open;
            }

            if motion.driven {
                let error = motion.target_ratio - part.open;
                let closing = motion.target_ratio < part.open;
                let (stiffness, damping, max_speed, max_accel) =
                    drive_parameters(part.role, closing);
                let latch_pull = if closing && motion.target_ratio <= DOOR_CLOSE_LATCH_RATIO {
                    // Keep a small closing torque through the final travel so
                    // the panel arrives at the latch with audible/visible mass.
                    -2.4 * (1.0 - part.open.clamp(0.0, 1.0)).powi(2)
                } else {
                    0.0
                };
                let acceleration = (error * stiffness - motion.current_speed * damping
                    + latch_pull)
                    .clamp(-max_accel, max_accel);
                motion.current_speed =
                    (motion.current_speed + acceleration * dt).clamp(-max_speed, max_speed);
                let before_step = part.open;
                part.open = (part.open + motion.current_speed * dt).clamp(0.0, 1.0);
                let crossed_target = (motion.target_ratio - part.open).signum()
                    != (motion.target_ratio - before_step).signum();
                let settled = (motion.target_ratio - part.open).abs() <= 0.002
                    && motion.current_speed.abs() <= 0.055;
                // Do not pre-brake a closing door into the zero target. Let the
                // latch block below absorb its remaining angular velocity.
                if (crossed_target || settled)
                    && !(closing && motion.target_ratio <= DOOR_CLOSE_LATCH_RATIO)
                {
                    part.open = motion.target_ratio;
                    motion.current_speed = 0.0;
                }
            } else if motion.swinging && !motion.latched {
                let drive = swing_drive(
                    part.role,
                    part.pivot,
                    local_acceleration,
                    local_angular_velocity,
                );
                motion.current_speed += drive * dt;
                motion.current_speed *= (-DOOR_SWING_DAMPING * dt).exp();

                // Damaged panels become progressively easier to swing.
                let damage_gain = 0.8 + part.damage.clamp(0.0, 1.0) * 1.35;
                let attempted_open = part.open + motion.current_speed * dt * damage_gain;
                part.open = attempted_open;

                // CCarDoor breaks an unlatched articulated panel when it is
                // driven 45 degrees beyond its authored stop, or when it stays
                // over ~2 degrees beyond the stop for roughly one second.
                if attempted_open > 1.0 {
                    let max_angle_deg = match part.role {
                        ModelFragmentPartRole::Door => 70.0,
                        ModelFragmentPartRole::Bonnet => 68.0,
                        ModelFragmentPartRole::Boot => 74.0,
                        _ => 70.0,
                    };
                    let over_limit_deg = (attempted_open - 1.0) * max_angle_deg;
                    if over_limit_deg > 2.0 {
                        motion.over_limit_seconds += dt;
                    } else {
                        motion.over_limit_seconds = 0.0;
                    }
                    motion.break_stress = (motion.break_stress
                        + (over_limit_deg / 45.0).max(0.0) * dt)
                        .clamp(0.0, 2.0);
                    if over_limit_deg > 45.0
                        || motion.over_limit_seconds > 1.0
                        || (part.damage >= 0.90 && motion.break_stress >= 1.0)
                    {
                        part.damage = 1.0;
                        part.loose = false;
                    }
                } else {
                    motion.over_limit_seconds = (motion.over_limit_seconds - dt * 2.0).max(0.0);
                }

                if matches!(
                    part.role,
                    ModelFragmentPartRole::Bonnet | ModelFragmentPartRole::Boot
                ) {
                    // Weak gas-strut semantics: panels near the open side prefer
                    // staying open, panels near closed prefer returning to latch.
                    let equilibrium = if part.open >= 0.52 { 1.0 } else { 0.0 };
                    motion.current_speed += (equilibrium - part.open) * 0.65 * dt;
                }

                if part.open >= 1.0 {
                    part.open = 1.0;
                    motion.current_speed = -motion.current_speed.abs() * 0.08;
                } else if part.open <= 0.0 {
                    part.open = 0.0;
                    motion.current_speed = motion.current_speed.abs() * 0.08;
                }
            }

            if part.role == ModelFragmentPartRole::Bonnet
                && !motion.latched
                && !motion.driven
                && part.open >= 0.95
            {
                let local_velocity = inverse_rotate_euler_xyz(linear_velocity, rotation_degrees);
                let forward_speed = (-local_velocity[2]).max(0.0);
                if forward_speed > 20.0 {
                    part.damage = 1.0;
                    part.loose = false;
                }
            }

            if motion.auto_reset
                && !motion.driven
                && !motion.swinging
                && !motion.latched
                && part.open < 0.18
            {
                motion.target_ratio = 0.0;
                motion.driven = true;
            }

            if part.open <= DOOR_CLOSE_LATCH_RATIO
                && motion.target_ratio <= DOOR_CLOSE_LATCH_RATIO
                && motion.current_speed <= 0.0
            {
                let was_latched = motion.latched;
                // Capture the impact before the latch removes the remaining
                // velocity. This is also useful for close-sound intensity.
                let latch_impact_speed = motion.current_speed.abs();
                part.open = 0.0;
                motion.current_speed = 0.0;
                motion.latched = true;
                motion.swinging = false;
                motion.driven = false;
                if !was_latched {
                    motion.just_latched = true;
                    if motion.last_audio_seconds < 0.0
                        || elapsed - motion.last_audio_seconds >= DOOR_AUDIO_RETRIGGER_SECONDS
                    {
                        motion.last_audio_seconds = elapsed;
                        audio_events.push((
                            part.name.clone(),
                            VehicleEventKind::DoorClosed,
                            latch_impact_speed.max(0.35),
                        ));
                    }
                }
            }

            if let Some(kind) = vehicle_door_audio_transition(previous_ratio, part.open) {
                if kind == VehicleEventKind::DoorClosed {
                    // Closing sound is emitted by the physical latch above so
                    // its intensity can follow actual impact speed.
                    part.presentation_override = true;
                    continue;
                }
                if motion.last_audio_seconds < 0.0
                    || elapsed - motion.last_audio_seconds >= DOOR_AUDIO_RETRIGGER_SECONDS
                {
                    motion.last_audio_seconds = elapsed;
                    audio_events.push((
                        part.name.clone(),
                        kind,
                        (part.open - previous_ratio)
                            .abs()
                            .max(motion.current_speed.abs()),
                    ));
                }
            }
            part.presentation_override = true;
        }

        for (part_name, kind, magnitude) in audio_events {
            let mut event = VehicleEvent::new(0, entity, kind);
            event.part = Some(part_name);
            event.magnitude = magnitude;
            self.vehicles.emit_event(event);
        }
    }
}
