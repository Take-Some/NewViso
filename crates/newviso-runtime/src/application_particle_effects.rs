use newviso_assets_client::AssetClient;
use newviso_scene::{
    Scene3dRuntime, SceneParticleBlend, SceneParticleMesh, SceneParticleSpawnDesc,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

mod style;
use style::*;

const PARTICLE_RUNTIME_OUTPUT: &str = "particle_dictionary.runtime_v1";
const PARTICLE_RAW_BODY_OUTPUT: &str = "asset.list_file_body";
const PARTICLE_WIRE_MAGIC: &[u8; 4] = b"NVPT";
const MAX_EFFECT_PARTICLES_PER_COMMAND: usize = 512;
thread_local! {
    static DICTIONARIES: std::cell::RefCell<BTreeMap<String, (Arc<Value>, String)>> = std::cell::RefCell::new(BTreeMap::new());
}

#[derive(Clone, Copy)]
struct Basis {
    forward: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
}

#[derive(Clone, Copy)]
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 as f32 / (u32::MAX as f32 + 1.0)
    }

    fn range(&mut self, a: f32, b: f32) -> f32 {
        let lo = a.min(b);
        let hi = a.max(b);
        lo + (hi - lo) * self.next()
    }
}

pub(super) struct ParticleEffectSpawnReport {
    pub effect: String,
    pub emitted: usize,
    pub skipped_model: usize,
    pub skipped_trail: usize,
    pub source: String,
    pub textures: Vec<String>,
}

pub(super) fn spawn_particle_effect(
    scene: &mut Scene3dRuntime,
    command: &Value,
    command_index: usize,
) -> Result<ParticleEffectSpawnReport, String> {
    let asset_ref = required_string(command, "asset_ref", command_index)?;
    let (logical_path, effect_name) = split_asset_ref(asset_ref)?;
    let position = vec3(command, "position", command_index)?;
    let direction = optional_vec3(command, "direction", command_index)?.unwrap_or([0.0, 0.0, -1.0]);
    let basis = basis_from_forward(direction)?;
    let inherited_velocity =
        optional_vec3(command, "inherited_velocity", command_index)?.unwrap_or([0.0; 3]);
    let scale = optional_number(command, "scale", command_index)?
        .unwrap_or(1.0)
        .clamp(0.001, 1000.0);
    let count_scale = optional_number(command, "count_scale", command_index)?
        .unwrap_or(1.0)
        .clamp(0.0, 32.0);
    let command_tint = optional_vec4(command, "tint", command_index)?.unwrap_or([1.0; 4]);
    let seed = command
        .get("seed")
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .unwrap_or_else(|| joaat(effect_name) ^ joaat(logical_path));
    let mut rng = Rng(seed);

    let assets = AssetClient::new();
    let cache_key = format!(
        "{}|{}",
        logical_path,
        command
            .get("fallback_catalog")
            .and_then(Value::as_str)
            .unwrap_or("")
    );
    if command.get("reload").and_then(Value::as_bool) == Some(true) {
        DICTIONARIES.with(|cache| {
            cache.borrow_mut().remove(&cache_key);
        });
    }
    let cached = DICTIONARIES.with(|cache| cache.borrow().get(&cache_key).cloned());
    let (dictionary, source) = if let Some((dictionary, source)) = cached {
        (dictionary, format!("{source}@{effect_name}"))
    } else {
        let native_ypt_ready = assets
            .resolve_type(logical_path)
            .ok()
            .is_some_and(|type_info| {
                type_info.get("known").and_then(Value::as_bool) == Some(true)
                    && type_info
                        .get("descriptor")
                        .and_then(|descriptor| descriptor.get("asset_kind"))
                        .and_then(Value::as_str)
                        == Some("particle_dictionary")
                    && type_info
                        .get("descriptor")
                        .and_then(|descriptor| descriptor.get("outputs"))
                        .and_then(Value::as_array)
                        .is_some_and(|outputs| {
                            outputs
                                .iter()
                                .any(|value| value.as_str() == Some(PARTICLE_RUNTIME_OUTPUT))
                        })
            });

        let (dictionary, source) = if native_ypt_ready {
            let semantic_decode = assets.decode(
                logical_path,
                PARTICLE_RUNTIME_OUTPUT,
                json!({"entry": effect_name}),
            );
            let (bytes, decode_route) = match semantic_decode {
                Ok(bytes) => (bytes, PARTICLE_RUNTIME_OUTPUT),
                Err(semantic_error) => {
                    // Backward compatibility for older list-file codecs that can
                    // unwrap NEF8 but predate the semantic particle projection.
                    // The YPT NEF8 body is already the canonical NVPT runtime wire,
                    // which this module validates below before use.
                    let raw = assets.decode(
                        logical_path,
                        PARTICLE_RAW_BODY_OUTPUT,
                        json!({"entry": effect_name}),
                    );
                    match raw {
                        Ok(bytes) => (bytes, PARTICLE_RAW_BODY_OUTPUT),
                        Err(raw_error) => {
                            return Err(format!(
                            "script command[{command_index}] native YPT decode failed path='{}': semantic '{}': {}; raw-body '{}': {}",
                            logical_path,
                            PARTICLE_RUNTIME_OUTPUT,
                            semantic_error,
                            PARTICLE_RAW_BODY_OUTPUT,
                            raw_error
                        ));
                        }
                    }
                }
            };
            (
            decode_particle_dictionary_wire(&bytes).map_err(|error| {
                format!(
                    "script command[{command_index}] particle dictionary '{}' decode failed via '{}': {error}",
                    logical_path, decode_route
                )
            })?,
            logical_path.to_owned(),
        )
        } else {
            let fallback = command
            .get("fallback_catalog")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "script command[{command_index}] YPT '{}' is not registered as particle_dictionary and no fallback_catalog was supplied",
                    logical_path
                )
            })?;
            let value = assets.json(fallback).map_err(|fallback_error| {
            format!(
                "script command[{command_index}] YPT '{}' is not registered; fallback '{}' failed: {fallback_error}",
                logical_path, fallback
            )
        })?;
            (value, fallback.to_owned())
        };
        let dictionary = Arc::new(dictionary);
        DICTIONARIES.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= 32 {
                cache.clear();
            }
            cache.insert(cache_key, (dictionary.clone(), source.clone()));
        });
        (dictionary, format!("{source}@{effect_name}"))
    };

    let effect = find_named(dictionary.get("effects"), effect_name).ok_or_else(|| {
        format!(
            "script command[{command_index}] particle effect '{}' is absent from '{}'",
            effect_name, source
        )
    })?;
    let emitters = named_map(dictionary.get("emitters"));
    let particles = named_map(dictionary.get("particles"));

    let duration_min = positive_number(effect.get("duration_min")).unwrap_or(0.1);
    let effect_playback = playback_range(effect, &mut rng);
    let effect_duration = rng
        .range(
            duration_min,
            positive_number(effect.get("duration_max")).unwrap_or(duration_min),
        )
        .clamp(0.001, 120.0)
        / effect_playback;
    let events = effect
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            format!(
                "script command[{command_index}] particle effect '{}' has no event list",
                effect_name
            )
        })?;

    let emission_seconds =
        optional_number(command, "emission_seconds", command_index)?.map(|v| v.clamp(0.001, 1.0));
    let source_z_up =
        dictionary.get("coordinate_space").and_then(Value::as_str) == Some("rsc7_z_up_source");
    let rule_random = [rng.next(), rng.next()];
    let mut output = Vec::new();
    let mut skipped_model = 0usize;
    let skipped_trail = 0usize;
    let mut textures = Vec::<String>::new();

    for event in events {
        if event.get("event_type").and_then(Value::as_u64).unwrap_or(0) != 0 {
            continue;
        }
        let emitter_name = event
            .get("emitter_rule")
            .and_then(Value::as_str)
            .unwrap_or("");
        let particle_name = event
            .get("particle_rule")
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(emitter) = emitters.get(emitter_name).copied() else {
            return Err(format!(
                "particle effect '{}' references missing emitter '{}'",
                effect_name, emitter_name
            ));
        };
        let Some(particle) = particles.get(particle_name).copied() else {
            return Err(format!(
                "particle effect '{}' references missing particle '{}'",
                effect_name, particle_name
            ));
        };

        let behaviour_types = particle
            .get("behaviours")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.get("type").and_then(Value::as_str))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for texture in particle_texture_names(particle) {
            if !textures
                .iter()
                .any(|item| item.eq_ignore_ascii_case(&texture))
            {
                textures.push(texture);
            }
        }

        let model = behaviour_types.contains(&"model");
        let debris_density = physical_debris_density(particle, emitter_name, &behaviour_types);
        let trail = behaviour_types.contains(&"trail");
        let meshes = if model {
            model_meshes(particle)?
        } else {
            Vec::new()
        };
        if model && meshes.is_empty() {
            skipped_model += 1;
            continue;
        }
        if !model && !trail && !behaviour_types.contains(&"sprite") {
            continue;
        }

        let event_playback = effect_playback * playback_range(event, &mut rng);
        let births = emission_plan(
            emitter,
            event,
            effect_duration,
            emission_seconds,
            count_scale,
            event_playback,
            source_z_up,
            &mut rng,
        );
        if let Some(reference) = particle.get("texture_ref").and_then(Value::as_str) {
            if !scene.particle_texture_registered(reference) {
                let texture = super::load_sky_texture(&assets, reference, true)?;
                scene.register_particle_texture(reference, texture)?;
            }
        }
        for mesh in &meshes {
            if let Some(reference) = mesh.texture_ref.as_deref() {
                if !scene.particle_texture_registered(reference) {
                    scene.register_particle_texture(
                        reference,
                        super::load_sky_texture(&assets, reference, true)?,
                    )?;
                }
            }
        }

        for (delay, phase) in births {
            if output.len() >= MAX_EFFECT_PARTICLES_PER_COMMAND {
                break;
            }
            let start = number(event.get("start_ratio")).unwrap_or(0.0);
            let end = number(event.get("end_ratio")).unwrap_or(1.0);
            let effect_phase = (start + (end - start) * phase).clamp(0.0, 1.0);
            let source_zoom = if source_z_up {
                effect_zoom(effect, effect_phase, rule_random[0])
            } else {
                1.0
            };
            let rule_tint = if source_z_up {
                let a =
                    property_value(effect.get("colour_tint_min"), effect_phase).unwrap_or([1.0; 4]);
                let b = property_value(effect.get("colour_tint_max"), effect_phase).unwrap_or(a);
                std::array::from_fn(|i| a[i] + (b[i] - a[i]) * rule_random[1])
            } else {
                [1.0; 4]
            };
            let mut projected = project_sprite_particle(
                emitter,
                particle,
                event,
                position,
                basis,
                scale * source_zoom,
                mul4(command_tint, rule_tint),
                &mut rng,
                source_z_up,
                phase,
                event_playback,
            )?;
            let inherit = if source_z_up {
                emitter_range_at(emitter, "ptxEmitterRule:m_inheritVelocityKFP", phase)
                    .map(|(a, b)| rng.range(a, b) * 0.01)
                    .unwrap_or(0.0)
            } else {
                1.0
            };
            projected.velocity = add(projected.velocity, mul(inherited_velocity, inherit));
            if let Some(style) = projected.style.as_mut() {
                style.effect_name = Some(effect_name.to_owned());
                style.emitter_name = Some(emitter_name.to_owned());
                style.physical_debris_density = if projected.blend == SceneParticleBlend::Alpha {
                    debris_density
                } else {
                    None
                };
                style.trail = trail;
                if trail {
                    style.trail_group =
                        command.get("emitter_id").and_then(Value::as_u64).map(|id| {
                            id.wrapping_mul(0x9e3779b97f4a7c15)
                                ^ joaat(effect_name) as u64
                                ^ ((joaat(emitter_name) as u64) << 32)
                        });
                }
                if !meshes.is_empty() {
                    let index = (rng.next() * meshes.len() as f32) as usize;
                    let mesh = meshes[index.min(meshes.len() - 1)].clone();
                    style.texture_ref = mesh.texture_ref.clone().or(style.texture_ref.take());
                    style.texture_grid = [1, 1];
                    style.first_frame = 0;
                    style.last_frame = 0;
                    style.animation_rate = 0.0;
                    style.animate_over_life = false;
                    style.loop_animation = false;
                    style.loop_start_frame = None;
                    style.loop_random_start = None;
                    style.model = Some(mesh);
                    style.model_basis = if source_z_up {
                        [basis.right, mul(basis.up, -1.0), basis.forward]
                    } else {
                        [basis.right, basis.up, basis.forward]
                    };
                    if let Some(rotation) = behaviour(particle, "rotation") {
                        let min = property_value(rotation.get("initial_angle_min"), 0.0)
                            .unwrap_or([0.0; 4]);
                        let max =
                            property_value(rotation.get("initial_angle_max"), 0.0).unwrap_or(min);
                        let spin_min =
                            property_value(rotation.get("angle_min"), 0.0).unwrap_or([0.0; 4]);
                        let spin_max =
                            property_value(rotation.get("angle_max"), 0.0).unwrap_or(spin_min);
                        style.model_rotation = std::array::from_fn(|i| rng.range(min[i], max[i]));
                        style.model_spin = std::array::from_fn(|i| {
                            rng.range(spin_min[i], spin_max[i]) * style.motion_rate.unwrap_or(1.0)
                        });
                    }
                }
            }
            if let Some(style) = projected.style.as_mut() {
                style.delay_seconds = delay.min(120.0);
            }
            output.push(projected);
        }
        if output.len() >= MAX_EFFECT_PARTICLES_PER_COMMAND {
            break;
        }
    }

    let emitted = output.len();
    if !output.is_empty() {
        scene.spawn_particles(output)?;
    }

    Ok(ParticleEffectSpawnReport {
        effect: effect_name.to_owned(),
        emitted,
        skipped_model,
        skipped_trail,
        source,
        textures,
    })
}

fn project_sprite_particle(
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

fn particle_size(particle: &Value, multiplier: [f32; 3], rng: &mut Rng) -> ([f32; 2], [f32; 2]) {
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

fn particle_color(particle: &Value, rng: &mut Rng) -> ([f32; 4], [f32; 4]) {
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

fn particle_rotation(particle: &Value, rng: &mut Rng) -> (f32, f32) {
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

fn event_color(event: &Value, rng: &mut Rng) -> [f32; 4] {
    let min = array4(event.get("colour_tint_min")).unwrap_or([1.0; 4]);
    let max = array4(event.get("colour_tint_max")).unwrap_or(min);
    std::array::from_fn(|index| rng.range(min[index], max[index]).clamp(0.0, 64.0))
}

fn domain_spawn_position(domain: Option<&Value>, phase: f32, rng: &mut Rng) -> [f32; 3] {
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

fn rotate_domain(mut p: [f32; 3], degrees: [f32; 3]) -> [f32; 3] {
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

fn effect_zoom(effect: &Value, phase: f32, random: f32) -> f32 {
    let range = property_value(effect.get("zoom_scalar"), phase).unwrap_or([100.0; 4]);
    ((range[0] + (range[1] - range[0]) * random) * 0.01).max(0.0)
}

fn playback_range(value: &Value, rng: &mut Rng) -> f32 {
    let a = number(value.get("playback_rate_scalar_min")).unwrap_or(1.0);
    let b = number(value.get("playback_rate_scalar_max")).unwrap_or(a);
    rng.range(a, b).max(0.01)
}

/// Burst rates are counts; continuous rates are particles per playback second.
/// Birth times retain the authored event envelope instead of aging every sprite together.
fn physical_debris_density(particle: &Value, emitter: &str, behaviours: &[&str]) -> Option<f32> {
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

fn emission_plan(
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

fn behaviour_range3(
    particle: &Value,
    kind: &str,
    min_key: &str,
    max_key: &str,
    rng: &mut Rng,
) -> Option<[f32; 3]> {
    let value = behaviour(particle, kind)?;
    let min = keyframe_value_at(value.get(min_key), false)?;
    let max = keyframe_value_at(value.get(max_key), false).unwrap_or(min);
    Some(std::array::from_fn(|index| {
        rng.range(min[index], max[index])
    }))
}

fn behaviour<'a>(particle: &'a Value, kind: &str) -> Option<&'a Value> {
    particle
        .get("behaviours")?
        .as_array()?
        .iter()
        .find(|value| value.get("type").and_then(Value::as_str) == Some(kind))
}

fn emitter_property<'a>(emitter: &'a Value, name: &str) -> Option<&'a Value> {
    emitter
        .get("keyframe_properties")?
        .as_array()?
        .iter()
        .find(|value| value.get("name").and_then(Value::as_str) == Some(name))
}
fn emitter_range(emitter: &Value, name: &str) -> Option<(f32, f32)> {
    emitter_range_at(emitter, name, 0.0)
}
fn emitter_range_at(emitter: &Value, name: &str, phase: f32) -> Option<(f32, f32)> {
    let value = property_value(emitter_property(emitter, name), phase)?;
    Some((value[0], value[1]))
}

fn keyframe_value_at(property: Option<&Value>, last: bool) -> Option<[f32; 4]> {
    let values = property?.get("keyframes")?.as_array()?;
    let keyframe = if last {
        values.last()?
    } else {
        values.first()?
    };
    array4(keyframe.get("value"))
}

fn particle_texture_names(particle: &Value) -> Vec<String> {
    particle
        .get("shader_vars")
        .and_then(Value::as_array)
        .map(|vars| {
            vars.iter()
                .filter(|value| value.get("type").and_then(Value::as_str) == Some("texture"))
                .filter_map(|value| value.get("texture_name").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn named_map(value: Option<&Value>) -> BTreeMap<String, &Value> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.trim();
                    (!name.is_empty()).then_some((name.to_owned(), item))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn find_named<'a>(value: Option<&'a Value>, name: &str) -> Option<&'a Value> {
    value?.as_array()?.iter().find(|item| {
        item.get("name")
            .and_then(Value::as_str)
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
    })
}

fn decode_particle_dictionary_wire(bytes: &[u8]) -> Result<Value, String> {
    if bytes.starts_with(b"{") {
        return serde_json::from_slice(bytes)
            .map_err(|error| format!("particle dictionary JSON decode failed: {error}"));
    }
    if bytes.len() < 20 || &bytes[..4] != PARTICLE_WIRE_MAGIC {
        return Err(format!(
            "expected NVPT semantic wire, got {} bytes magic={:?}",
            bytes.len(),
            bytes.get(..4)
        ));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != 1 {
        return Err(format!("unsupported NVPT semantic version {version}"));
    }
    let meta_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let payload_len = usize::try_from(u64::from_le_bytes(bytes[12..20].try_into().unwrap()))
        .map_err(|_| "NVPT payload length exceeds usize".to_owned())?;
    let meta_end = 20usize
        .checked_add(meta_len)
        .ok_or_else(|| "NVPT metadata range overflow".to_owned())?;
    let total = meta_end
        .checked_add(payload_len)
        .ok_or_else(|| "NVPT payload range overflow".to_owned())?;
    if total != bytes.len() {
        return Err(format!(
            "NVPT size mismatch declared={total} actual={}",
            bytes.len()
        ));
    }
    serde_json::from_slice(&bytes[20..meta_end])
        .map_err(|error| format!("NVPT metadata JSON decode failed: {error}"))
}

fn split_asset_ref(value: &str) -> Result<(&str, &str), String> {
    let (path, entry) = value
        .rsplit_once('@')
        .ok_or_else(|| format!("particle effect asset_ref requires @effect: '{value}'"))?;
    let path = path.trim();
    let entry = entry.trim();
    if path.is_empty() || entry.is_empty() {
        return Err(format!("invalid particle effect asset_ref '{value}'"));
    }
    Ok((path, entry))
}

fn required_string<'a>(value: &'a Value, key: &str, index: usize) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("script command[{index}] requires non-empty string '{key}'"))
}

fn optional_number(value: &Value, key: &str, index: usize) -> Result<Option<f32>, String> {
    let Some(raw) = value.get(key) else {
        return Ok(None);
    };
    let result = raw
        .as_f64()
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("script command[{index}] '{key}' must be finite numeric"))?;
    Ok(Some(result))
}

fn vec3(value: &Value, key: &str, index: usize) -> Result<[f32; 3], String> {
    array3(value.get(key))
        .ok_or_else(|| format!("script command[{index}] '{key}' must contain three numbers"))
}

fn optional_vec3(value: &Value, key: &str, index: usize) -> Result<Option<[f32; 3]>, String> {
    if value.get(key).is_none() {
        return Ok(None);
    }
    vec3(value, key, index).map(Some)
}

fn optional_vec4(value: &Value, key: &str, index: usize) -> Result<Option<[f32; 4]>, String> {
    if value.get(key).is_none() {
        return Ok(None);
    }
    array4(value.get(key))
        .ok_or_else(|| format!("script command[{index}] '{key}' must contain four numbers"))
        .map(Some)
}

fn array3(value: Option<&Value>) -> Option<[f32; 3]> {
    let values = value?.as_array()?;
    if values.len() != 3 {
        return None;
    }
    Some([
        values[0].as_f64()? as f32,
        values[1].as_f64()? as f32,
        values[2].as_f64()? as f32,
    ])
}

fn array4(value: Option<&Value>) -> Option<[f32; 4]> {
    let values = value?.as_array()?;
    if values.len() != 4 {
        return None;
    }
    Some([
        values[0].as_f64()? as f32,
        values[1].as_f64()? as f32,
        values[2].as_f64()? as f32,
        values[3].as_f64()? as f32,
    ])
}

fn number(value: Option<&Value>) -> Option<f32> {
    value?
        .as_f64()
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
}

fn positive_number(value: Option<&Value>) -> Option<f32> {
    number(value).filter(|value| *value > 0.0)
}

fn basis_from_forward(direction: [f32; 3]) -> Result<Basis, String> {
    let forward =
        normalize(direction).ok_or_else(|| "particle effect direction is zero".to_owned())?;
    let world_up = [0.0, 1.0, 0.0];
    let right = normalize(cross(forward, world_up))
        .or_else(|| normalize(cross(forward, [1.0, 0.0, 0.0])))
        .ok_or_else(|| "particle effect could not derive orientation basis".to_owned())?;
    let up = normalize(cross(right, forward)).unwrap_or(world_up);
    Ok(Basis { forward, right, up })
}

fn local_to_world(local: [f32; 3], basis: Basis, scale: f32) -> [f32; 3] {
    add(
        add(
            mul(basis.forward, local[0] * scale),
            mul(basis.right, local[1] * scale),
        ),
        mul(basis.up, local[2] * scale),
    )
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: [f32; 3], scalar: f32) -> [f32; 3] {
    [a[0] * scalar, a[1] * scalar, a[2] * scalar]
}
fn mul4(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|index| a[index] * b[index])
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(value, value).sqrt();
    if !length.is_finite() || length <= 1.0e-6 {
        None
    } else {
        Some(mul(value, 1.0 / length))
    }
}

fn joaat(value: &str) -> u32 {
    let mut hash = 0u32;
    for byte in value.to_ascii_lowercase().bytes() {
        hash = hash.wrapping_add(byte as u32);
        hash = hash.wrapping_add(hash << 10);
        hash ^= hash >> 6;
    }
    hash = hash.wrapping_add(hash << 3);
    hash ^= hash >> 11;
    hash.wrapping_add(hash << 15)
}

#[cfg(test)]
mod physical_debris_tests {
    use super::*;
    #[test]
    fn solid_source_rules_are_separated_from_fire_smoke_dust_and_sparks() {
        for name in [
            "ent_brk_metal_frag_debris",
            "veh_tyre_burst_rubber",
            "glass_side_shards_PC",
            "glass_side_panes_PC",
            "glass_side_nuggets",
            "glass_shards",
        ] {
            assert!(
                physical_debris_density(&json!({"name":name}), name, &["sprite"]).is_some(),
                "{name}"
            );
        }
        for name in [
            "ent_brk_metal_frag_sparks",
            "ent_brk_metal_frag_lit_2",
            "glass_shards_dust",
            "glass_smash_dots",
            "veh_tyre_burst_resid",
            "fire_wrecked_car_flames",
            "fire_wrecked_car_ash",
            "veh_exhaust_car",
            "engine_smoke",
        ] {
            assert!(
                physical_debris_density(&json!({"name":name}), name, &["sprite"]).is_none(),
                "{name}"
            );
        }
        assert!(
            physical_debris_density(&json!({"name":"chunk_model"}), "chunk", &["model"]).is_some()
        );
        assert!(
            physical_debris_density(&json!({"name":"metal_frag_trail"}), "chunk", &["trail"])
                .is_none()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvpt_wire_parser_rejects_wrong_magic() {
        assert!(decode_particle_dictionary_wire(b"NOPE").is_err());
    }

    #[test]
    fn legacy_listfile_particle_fallback_uses_raw_body_output() {
        assert_eq!(PARTICLE_RAW_BODY_OUTPUT, "asset.list_file_body");
        assert_ne!(PARTICLE_RAW_BODY_OUTPUT, PARTICLE_RUNTIME_OUTPUT);
    }

    #[test]
    fn default_basis_faces_minus_z() {
        let basis = basis_from_forward([0.0, 0.0, -1.0]).unwrap();
        assert_eq!(basis.forward, [0.0, 0.0, -1.0]);
        assert!((basis.right[0] - 1.0).abs() < 1e-6);
        assert!((basis.up[1] - 1.0).abs() < 1e-6);
    }
}

#[cfg(test)]
mod source_emission_tests {
    use super::*;
    fn emitter(rate: [f32; 4], one_shot: bool) -> Value {
        json!({"one_shot":one_shot,"keyframe_properties":[{"name":"ptxEmitterRule:m_spawnRateOverTimeKFP",
            "keyframes":[{"time":[0.0],"value":rate}]}]})
    }
    #[test]
    fn original_bursts_count_particles_independently_of_event_duration() {
        let e = emitter([25.0, 25.0, 0.0, 0.0], true);
        for duration in [0.1, 2.0] {
            let births = emission_plan(&e, &json!({}), duration, None, 1.0, 1.0, true, &mut Rng(7));
            assert_eq!(births.len(), 25);
            assert!(births.iter().all(|(delay, _)| *delay == 0.0));
        }
    }
    #[test]
    fn carmetal_model_shards_obey_original_eight_particle_cap() {
        let births = emission_plan(
            &emitter([222.0, 225.0, 8.0, 0.0], true),
            &json!({}),
            2.0,
            None,
            1.0,
            1.0,
            true,
            &mut Rng(7),
        );
        assert_eq!(births.len(), 8);
    }
    #[test]
    fn disabled_source_emitters_do_not_create_an_extra_giant_sprite() {
        let births = emission_plan(
            &emitter([0.0; 4], true),
            &json!({}),
            0.1,
            None,
            1.0,
            1.0,
            true,
            &mut Rng(7),
        );
        assert!(births.is_empty());
    }
    #[test]
    fn continuous_emission_preserves_the_authored_time_envelope() {
        let births = emission_plan(
            &emitter([20.0, 20.0, 0.0, 0.0], false),
            &json!({"start_ratio":0.2,"end_ratio":0.5}),
            2.0,
            None,
            1.0,
            1.0,
            true,
            &mut Rng(7),
        );
        assert_eq!(births.len(), 12);
        assert!(births.first().unwrap().0 >= 0.4);
        assert!(births.last().unwrap().0 > 0.9 && births.last().unwrap().0 <= 1.0);
    }
    #[test]
    fn target_point_relative_keeps_its_authored_speed_and_playback_lifetime() {
        let e = json!({"keyframe_properties":[
            {"name":"ptxEmitterRule:m_particleLifeKFP","keyframes":[{"time":[0.0],"value":[6.0,6.0,0.0,0.0]}]},
            {"name":"ptxEmitterRule:m_playbackRateScalarKFP","keyframes":[{"time":[0.0],"value":[2.0,0.0,0.0,0.0]}]},
            {"name":"ptxEmitterRule:m_speedScalarKFP","keyframes":[{"time":[0.0],"value":[10.0,10.0,0.0,0.0]}]}],
            "creation_domain":{"position":{"keyframes":[{"time":[0.0],"value":[2.0,0.0,0.0,0.0]}]}},
            "target_domain":{"point_relative":true,"position":{"keyframes":[{"time":[0.0],"value":[0.0,0.0,0.2,0.0]}]}}});
        let p = project_sprite_particle(
            &e,
            &json!({}),
            &json!({}),
            [0.0; 3],
            basis_from_forward([0.0, 1.0, 0.0]).unwrap(),
            1.0,
            [1.0; 4],
            &mut Rng(7),
            true,
            0.0,
            1.0,
        )
        .unwrap();
        assert_eq!(p.lifetime_seconds, 3.0);
        assert!((p.velocity[1] - 2.0).abs() < 1.0e-5);
        assert!(p.velocity[0].abs() < 1.0e-5);
    }
    #[test]
    fn source_cylinder_uses_elliptical_cross_section_and_domain_rotation() {
        let d = json!({"type":"cylinder","size_outer":{"keyframes":[{"value":[0.5,0.0,0.5,0.0]}]},
            "rotation":{"keyframes":[{"value":[0.0,90.0,0.0,0.0]}]}});
        let mut rng = Rng(7);
        let samples = (0..40)
            .map(|_| domain_spawn_position(Some(&d), 0.0, &mut rng))
            .collect::<Vec<_>>();
        assert!(samples
            .iter()
            .all(|p| p[1].abs() < 1.0e-6 && dot(*p, *p) <= 0.251));
        assert!(samples.iter().any(|p| p[2].abs() > 0.1));
    }
}

#[cfg(test)]
mod effect_rule_tests {
    use super::*;
    #[test]
    fn source_effect_zoom_uses_percent_before_emitter_and_particle_sizes() {
        let e = json!({"zoom_scalar":{"keyframes":[{"time":[0.0],"value":[9.0,12.0,0.0,0.0]}]}});
        assert!((effect_zoom(&e, 0.0, 0.5) - 0.105).abs() < 1.0e-6);
        assert_eq!(effect_zoom(&json!({}), 0.0, 0.5), 1.0);
    }
}
