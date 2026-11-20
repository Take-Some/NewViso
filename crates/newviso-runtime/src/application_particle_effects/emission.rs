use super::*;

pub(super) fn project_sprite_particle(
    emitter: &Value,
    particle: &Value,
    event: &Value,
    origin: [f32; 3],
    basis: Basis,
    scale: f32,
    command_tint: [f32; 4],
    rng: &mut Rng,
    source_z_up: bool,
    phase: f32,
    event_playback: f32,
) -> Result<SceneParticleSpawnDesc, String> {
    let lifetime_range = emitter_range_at(emitter, "ptxEmitterRule:m_particleLifeKFP", phase)
        .unwrap_or(if source_z_up { (1.0, 1.0) } else { (0.1, 0.1) });
    let playback = if source_z_up {
        property_value(
            emitter_property(emitter, "ptxEmitterRule:m_playbackRateScalarKFP"),
            phase,
        )
        .map(|v| v[0])
        .unwrap_or(1.0)
        .max(0.01)
            * event_playback
    } else {
        1.0
    };
    let lifetime_seconds =
        (rng.range(lifetime_range.0, lifetime_range.1).abs() / playback).clamp(0.01, 120.0);

    let speed_range = emitter_range_at(emitter, "ptxEmitterRule:m_speedScalarKFP", phase)
        .unwrap_or(if source_z_up { (1.0, 1.0) } else { (0.0, 0.0) });
    let speed = rng.range(speed_range.0, speed_range.1);
    let emitter_size = if source_z_up {
        emitter_property(emitter, "ptxEmitterRule:m_sizeScalarKFP")
            .and_then(|p| property_value(Some(p), phase))
            .map(|v| [v[0] * 0.01, v[1] * 0.01, v[2] * 0.01])
            .unwrap_or([1.0; 3])
    } else {
        let scalar = emitter_range(emitter, "ptxEmitterRule:m_sizeScalarKFP")
            .map(|range| rng.range(range.0, range.1) * 0.01)
            .unwrap_or(1.0);
        [scalar; 3]
    };
    let zoom = {
        let a = number(event.get("zoom_scalar_min")).unwrap_or(1.0);
        let b = number(event.get("zoom_scalar_max")).unwrap_or(a);
        rng.range(a, b).max(0.001)
    };

    let local_position = domain_spawn_position(emitter.get("creation_domain"), phase, rng);
    let position = add(
        origin,
        source_local_vector(local_position, basis, scale, source_z_up),
    );

    let mut velocity = mul(basis.forward, speed * scale);
    let acceleration_local =
        behaviour_range3(particle, "acceleration", "xyz_min", "xyz_max", rng).unwrap_or([0.0; 3]);
    let acceleration = world_acceleration(particle, acceleration_local, basis, scale, source_z_up);

    if emitter.get("target_domain").is_some() {
        let target = domain_spawn_position(emitter.get("target_domain"), phase, rng);
        let relative = source_z_up
            && emitter
                .get("target_domain")
                .and_then(|d| d.get("point_relative"))
                .and_then(Value::as_bool)
                == Some(true);
        let vector = if relative {
            target
        } else {
            std::array::from_fn(|axis| target[axis] - local_position[axis])
        };
        // The source target is an authored velocity vector, not a unit direction.
        let vector = if source_z_up {
            vector
        } else {
            normalize(vector).unwrap_or([0.0; 3])
        };
        velocity = source_local_vector(vector, basis, speed * scale, source_z_up);
    }

    let (size_start, size_end) =
        particle_size(particle, emitter_size.map(|v| v * zoom * scale), rng);
    let (mut color, mut end_color) = particle_color(particle, rng);
    let event_tint = event_color(event, rng);
    color = mul4(mul4(color, event_tint), command_tint);
    end_color = mul4(mul4(end_color, event_tint), command_tint);

    let (rotation_degrees, angular_velocity_degrees) = particle_rotation(particle, rng);
    let blend_set = particle
        .get("render_state")
        .and_then(|value| value.get("blend_set"))
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let mut appearance = appearance(
        particle,
        mul4(event_tint, command_tint),
        emitter_size.map(|v| v * zoom * scale),
        rng,
    );
    if source_z_up {
        appearance.motion_rate = Some(playback);
        motion_curves(
            &mut appearance,
            emitter,
            particle,
            basis,
            scale,
            phase,
            playback,
            rng,
        );
    }
    if !appearance.animate_over_life {
        appearance.animation_rate *= playback;
    }
    Ok(SceneParticleSpawnDesc {
        position,
        velocity,
        acceleration,
        size: size_start,
        end_size: size_end,
        color,
        end_color,
        lifetime_seconds,
        rotation_degrees,
        angular_velocity_degrees,
        style: Some(appearance),
        blend: if blend_set == 1 {
            SceneParticleBlend::Additive
        } else {
            SceneParticleBlend::Alpha
        },
    })
}

pub(super) fn particle_size(
    particle: &Value,
    multiplier: [f32; 3],
    rng: &mut Rng,
) -> ([f32; 2], [f32; 2]) {
    let Some(size) = behaviour(particle, "size") else {
        let s = [multiplier[0], multiplier[1]].map(|v| (0.1 * v).max(0.002));
        return (s, s);
    };
    let start_min = keyframe_value_at(size.get("whd_min"), false).unwrap_or([1.0; 4]);
    let start_max = keyframe_value_at(size.get("whd_max"), false).unwrap_or(start_min);
    let end_min = keyframe_value_at(size.get("whd_min"), true).unwrap_or(start_min);
    let end_max = keyframe_value_at(size.get("whd_max"), true).unwrap_or(start_max);
    let start = [
        (rng.range(start_min[0], start_max[0]).abs() * multiplier[0]).max(0.002),
        (rng.range(start_min[1], start_max[1]).abs() * multiplier[1]).max(0.002),
    ];
    let end = [
        (rng.range(end_min[0], end_max[0]).abs() * multiplier[0]).max(0.002),
        (rng.range(end_min[1], end_max[1]).abs() * multiplier[1]).max(0.002),
    ];
    (start, end)
}

pub(super) fn particle_color(particle: &Value, rng: &mut Rng) -> ([f32; 4], [f32; 4]) {
    let Some(color) = behaviour(particle, "colour") else {
        return ([1.0; 4], [1.0, 1.0, 1.0, 0.0]);
    };
    let start_min = keyframe_value_at(color.get("rgba_min"), false).unwrap_or([1.0; 4]);
    let start_max = keyframe_value_at(color.get("rgba_max"), false).unwrap_or(start_min);
    let end_min = keyframe_value_at(color.get("rgba_min"), true).unwrap_or(start_min);
    let end_max = keyframe_value_at(color.get("rgba_max"), true).unwrap_or(start_max);
    let pick = |a: [f32; 4], b: [f32; 4], rng: &mut Rng| {
        std::array::from_fn(|index| rng.range(a[index], b[index]).clamp(0.0, 64.0))
    };
    (pick(start_min, start_max, rng), pick(end_min, end_max, rng))
}

pub(super) fn particle_rotation(particle: &Value, rng: &mut Rng) -> (f32, f32) {
    let Some(rotation) = behaviour(particle, "rotation") else {
        return (0.0, 0.0);
    };
    let min = keyframe_value_at(rotation.get("initial_angle_min"), false)
        .map(|value| value[0])
        .unwrap_or(0.0);
    let max = keyframe_value_at(rotation.get("initial_angle_max"), false)
        .map(|value| value[0])
        .unwrap_or(min);
    let velocity_min = keyframe_value_at(rotation.get("angle_min"), false)
        .map(|value| value[0])
        .unwrap_or(0.0);
    let velocity_max = keyframe_value_at(rotation.get("angle_max"), false)
        .map(|value| value[0])
        .unwrap_or(velocity_min);
    (rng.range(min, max), rng.range(velocity_min, velocity_max))
}

pub(super) fn event_color(event: &Value, rng: &mut Rng) -> [f32; 4] {
    let min = array4(event.get("colour_tint_min")).unwrap_or([1.0; 4]);
    let max = array4(event.get("colour_tint_max")).unwrap_or(min);
    std::array::from_fn(|index| rng.range(min[index], max[index]).clamp(0.0, 64.0))
}

pub(super) fn domain_spawn_position(domain: Option<&Value>, phase: f32, rng: &mut Rng) -> [f32; 3] {
    let Some(domain) = domain else {
        return [0.0; 3];
    };
    let center = property_value(domain.get("position"), phase).unwrap_or([0.0; 4]);
    let outer = property_value(domain.get("size_outer"), phase)
        .unwrap_or([0.0; 4])
        .map(f32::abs);
    let inner = property_value(domain.get("size_inner"), phase)
        .unwrap_or([0.0; 4])
        .map(f32::abs);
    let kind = domain.get("type").and_then(Value::as_str).unwrap_or("box");
    let offset = match kind {
        "sphere" => {
            let direction =
                normalize(std::array::from_fn(|_| rng.range(-1.0, 1.0))).unwrap_or([0.0, 0.0, 1.0]);
            mul(direction, rng.range(inner[0], outer[0]))
        }
        "cylinder" => {
            // Source cylinders run along Y, with an elliptical X/Z section.
            let y = rng.range(-outer[1], outer[1]);
            let angle = rng.range(0.0, std::f32::consts::TAU);
            let edge = [outer[0] * angle.cos(), 0.0, outer[2] * angle.sin()];
            let radius = dot(edge, edge).sqrt();
            let inner_radius = if y.abs() <= inner[1] {
                ((inner[0] * angle.cos()).powi(2) + (inner[2] * angle.sin()).powi(2)).sqrt()
            } else {
                0.0
            };
            let ratio = if radius > 1.0e-6 {
                rng.range(inner_radius.min(radius), radius) / radius
            } else {
                0.0
            };
            [edge[0] * ratio, y, edge[2] * ratio]
        }
        _ => {
            let mut value = std::array::from_fn(|i| rng.range(-outer[i], outer[i]));
            if (0..3).all(|i| inner[i] > 0.0 && value[i].abs() < inner[i]) {
                let axis = (rng.next() * 3.0) as usize;
                let axis = axis.min(2);
                value[axis] = rng.range(inner[axis].min(outer[axis]), outer[axis])
                    * if value[axis] < 0.0 { -1.0 } else { 1.0 };
            }
            value
        }
    };
    let rotation = property_value(domain.get("rotation"), phase).unwrap_or([0.0; 4]);
    let offset = rotate_domain(offset, [rotation[0], rotation[1], rotation[2]]);
    add([center[0], center[1], center[2]], offset)
}

pub(super) fn rotate_domain(mut p: [f32; 3], degrees: [f32; 3]) -> [f32; 3] {
    for axis in 0..3 {
        let (sin, cos) = degrees[axis].to_radians().sin_cos();
        let a = (axis + 1) % 3;
        let b = (axis + 2) % 3;
        let va = p[a];
        let vb = p[b];
        p[a] = va * cos - vb * sin;
        p[b] = va * sin + vb * cos;
    }
    p
}

pub(super) fn effect_zoom(effect: &Value, phase: f32, random: f32) -> f32 {
    let range = property_value(effect.get("zoom_scalar"), phase).unwrap_or([100.0; 4]);
    ((range[0] + (range[1] - range[0]) * random) * 0.01).max(0.0)
}

pub(super) fn playback_range(value: &Value, rng: &mut Rng) -> f32 {
    let a = number(value.get("playback_rate_scalar_min")).unwrap_or(1.0);
    let b = number(value.get("playback_rate_scalar_max")).unwrap_or(a);
    rng.range(a, b).max(0.01)
}

/// Burst rates are counts; continuous rates are particles per playback second.
/// Birth times retain the authored event envelope instead of aging every sprite together.
pub(super) fn physical_debris_density(
    particle: &Value,
    emitter: &str,
    behaviours: &[&str],
) -> Option<f32> {
    let name = format!(
        "{} {emitter}",
        particle.get("name").and_then(Value::as_str).unwrap_or("")
    )
    .to_ascii_lowercase();
    // Inspect the individual source rule: a metal break effect also contains
    // sparks, glowing dots and dust, which must keep their authored lifetime.
    if [
        "dust", "smoke", "spark", "glow", "_lit", "_dots", "mist", "resid", "fire", "flame",
        "cinder", "_ash", "heathaze",
    ]
    .iter()
    .any(|token| name.contains(token))
    {
        return None;
    }
    let solid = behaviours.contains(&"model")
        || [
            "rubber", "shard", "pane", "nugget", "debris", "fragment", "_frag",
        ]
        .iter()
        .any(|token| name.contains(token));
    if !solid || behaviours.contains(&"trail") {
        return None;
    }
    Some(if name.contains("glass") {
        2500.0
    } else if name.contains("rubber") {
        1100.0
    } else if name.contains("metal") {
        7800.0
    } else if name.contains("wood") {
        700.0
    } else {
        1000.0
    })
}

pub(super) fn emission_plan(
    emitter: &Value,
    event: &Value,
    duration: f32,
    slice: Option<f32>,
    density: f32,
    playback: f32,
    source: bool,
    rng: &mut Rng,
) -> Vec<(f32, f32)> {
    let start = number(event.get("start_ratio"))
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let end = number(event.get("end_ratio"))
        .unwrap_or(1.0)
        .clamp(start, 1.0);
    let window = slice.unwrap_or((end - start) * duration).max(0.001);
    let one_shot = emitter.get("one_shot").and_then(Value::as_bool) == Some(true);
    let initial = property_value(
        emitter_property(emitter, "ptxEmitterRule:m_spawnRateOverTimeKFP"),
        0.0,
    )
    .unwrap_or([1.0, 1.0, 0.0, 0.0]);
    let cap = if initial[2] > 0.0 {
        (initial[2] as usize).min(128)
    } else {
        128
    };
    if density <= 0.0 || cap == 0 {
        return Vec::new();
    }
    if one_shot {
        let count = rng.range(initial[0], initial[1]).max(0.0) * density;
        // Streaming commands request a time slice of a repeatedly driven effect.
        let count = if slice.is_some() {
            let expected = count * window;
            expected.floor()
                + if rng.next() < expected.fract() {
                    1.0
                } else {
                    0.0
                }
        } else {
            count.floor()
        };
        return (0..(count as usize).min(cap))
            .map(|_| {
                (
                    if slice.is_some() {
                        0.0
                    } else {
                        start * duration
                    },
                    0.0,
                )
            })
            .collect();
    }
    let mut births = Vec::new();
    let steps = (window * 120.0).ceil().clamp(1.0, 14400.0) as usize;
    let dt = window / steps as f32;
    let mut accumulator = rng.next();
    for step in 0..steps {
        let phase = (step as f32 + 0.5) / steps as f32;
        let value = property_value(
            emitter_property(emitter, "ptxEmitterRule:m_spawnRateOverTimeKFP"),
            phase,
        )
        .unwrap_or(initial);
        let rate = rng.range(value[0], value[1]).max(0.0);
        accumulator += rate * dt * if source { playback } else { 1.0 } * density;
        while accumulator >= 1.0 && births.len() < cap {
            accumulator -= 1.0;
            births.push((
                if slice.is_some() {
                    0.0
                } else {
                    start * duration + phase * window
                },
                phase,
            ));
        }
        if births.len() >= cap {
            break;
        }
    }
    births
}
