use newviso_assets_client::AssetClient;
use newviso_scene::{Scene3dRuntime, SceneParticleBlend, SceneParticleSpawnDesc};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const PARTICLE_RUNTIME_OUTPUT: &str = "particle_dictionary.runtime_v1";
const PARTICLE_WIRE_MAGIC: &[u8; 4] = b"NVPT";
const MAX_EFFECT_PARTICLES_PER_COMMAND: usize = 512;

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
        let bytes = assets
            .decode(
                logical_path,
                PARTICLE_RUNTIME_OUTPUT,
                json!({"entry": effect_name}),
            )
            .map_err(|error| {
                format!(
                    "script command[{command_index}] native YPT decode failed path='{}': {error}",
                    logical_path
                )
            })?;
        (
            decode_particle_dictionary_wire(&bytes).map_err(|error| {
                format!(
                    "script command[{command_index}] particle dictionary '{}' decode failed: {error}",
                    logical_path
                )
            })?,
            format!("{logical_path}@{effect_name}"),
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
        (value, format!("{fallback}@{effect_name}"))
    };

    let effect = find_named(dictionary.get("effects"), effect_name).ok_or_else(|| {
        format!(
            "script command[{command_index}] particle effect '{}' is absent from '{}'",
            effect_name, source
        )
    })?;
    let emitters = named_map(dictionary.get("emitters"));
    let particles = named_map(dictionary.get("particles"));

    let effect_duration = positive_number(effect.get("duration_max"))
        .or_else(|| positive_number(effect.get("duration_min")))
        .unwrap_or(0.1)
        .clamp(0.001, 120.0);
    let events = effect
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            format!(
                "script command[{command_index}] particle effect '{}' has no event list",
                effect_name
            )
        })?;

    let mut output = Vec::new();
    let mut skipped_model = 0usize;
    let mut skipped_trail = 0usize;
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

        if behaviour_types.iter().any(|kind| *kind == "model") {
            skipped_model += 1;
            continue;
        }
        if behaviour_types.iter().any(|kind| *kind == "trail") {
            skipped_trail += 1;
            continue;
        }
        if !behaviour_types.iter().any(|kind| *kind == "sprite") {
            continue;
        }

        let spawn_rate = emitter_range(emitter, "ptxEmitterRule:m_spawnRateOverTimeKFP")
            .map(|range| (range.0 + range.1) * 0.5)
            .unwrap_or(1.0)
            .max(0.0);
        let one_shot = emitter
            .get("one_shot")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let event_window = {
            let start = number(event.get("start_ratio")).unwrap_or(0.0);
            let end = number(event.get("end_ratio")).unwrap_or(1.0);
            ((end - start).abs() * effect_duration).max(0.001)
        };
        let mut count = if one_shot {
            (spawn_rate * event_window).round().max(1.0) as usize
        } else {
            (spawn_rate * effect_duration).round().max(1.0) as usize
        };
        count = ((count as f32) * count_scale).round() as usize;
        count = count.min(128);

        for _ in 0..count {
            if output.len() >= MAX_EFFECT_PARTICLES_PER_COMMAND {
                break;
            }
            output.push(project_sprite_particle(
                emitter,
                particle,
                event,
                position,
                basis,
                scale,
                command_tint,
                &mut rng,
            )?);
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
) -> Result<SceneParticleSpawnDesc, String> {
    let lifetime_range =
        emitter_range(emitter, "ptxEmitterRule:m_particleLifeKFP").unwrap_or((0.1, 0.1));
    let lifetime_seconds = rng
        .range(lifetime_range.0, lifetime_range.1)
        .abs()
        .clamp(0.01, 120.0);

    let speed_range =
        emitter_range(emitter, "ptxEmitterRule:m_speedScalarKFP").unwrap_or((0.0, 0.0));
    let speed = rng.range(speed_range.0, speed_range.1);
    let emitter_size = emitter_range(emitter, "ptxEmitterRule:m_sizeScalarKFP")
        .map(|range| rng.range(range.0, range.1) * 0.01)
        .unwrap_or(1.0);
    let zoom = {
        let a = number(event.get("zoom_scalar_min")).unwrap_or(1.0);
        let b = number(event.get("zoom_scalar_max")).unwrap_or(a);
        rng.range(a, b).max(0.001)
    };

    let local_position = domain_spawn_position(emitter.get("creation_domain"), rng);
    let position = add(origin, local_to_world(local_position, basis, scale));

    let mut velocity = mul(basis.forward, speed * scale);
    let acceleration_local =
        behaviour_range3(particle, "acceleration", "xyz_min", "xyz_max", rng).unwrap_or([0.0; 3]);
    let acceleration = local_to_world(acceleration_local, basis, scale);

    // A small domain spread preserves the authored cone/cylinder origin without
    // fabricating a second velocity model that the RSC7 rule does not store.
    if let Some(domain) = emitter.get("creation_domain") {
        let outer = keyframe_value_at(domain.get("size_outer"), false);
        if let Some(outer) = outer {
            let lateral = (outer[1].abs() + outer[2].abs()) * 0.5 * scale;
            velocity = add(
                velocity,
                add(
                    mul(basis.right, rng.range(-lateral, lateral)),
                    mul(basis.up, rng.range(-lateral, lateral)),
                ),
            );
        }
    }

    let (size_start, size_end) = particle_size(particle, emitter_size * zoom * scale, rng);
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
        blend: if blend_set == 1 {
            SceneParticleBlend::Additive
        } else {
            SceneParticleBlend::Alpha
        },
    })
}

fn particle_size(particle: &Value, multiplier: f32, rng: &mut Rng) -> ([f32; 2], [f32; 2]) {
    let Some(size) = behaviour(particle, "size") else {
        let s = (0.1 * multiplier).max(0.002);
        return ([s, s], [s, s]);
    };
    let start_min = keyframe_value_at(size.get("whd_min"), false).unwrap_or([1.0; 4]);
    let start_max = keyframe_value_at(size.get("whd_max"), false).unwrap_or(start_min);
    let end_min = keyframe_value_at(size.get("whd_min"), true).unwrap_or(start_min);
    let end_max = keyframe_value_at(size.get("whd_max"), true).unwrap_or(start_max);
    let start = [
        (rng.range(start_min[0], start_max[0]).abs() * multiplier).max(0.002),
        (rng.range(start_min[1], start_max[1]).abs() * multiplier).max(0.002),
    ];
    let end = [
        (rng.range(end_min[0], end_max[0]).abs() * multiplier).max(0.002),
        (rng.range(end_min[1], end_max[1]).abs() * multiplier).max(0.002),
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

fn domain_spawn_position(domain: Option<&Value>, rng: &mut Rng) -> [f32; 3] {
    let Some(domain) = domain else {
        return [0.0; 3];
    };
    let center = keyframe_value_at(domain.get("position"), false).unwrap_or([0.0; 4]);
    let outer = keyframe_value_at(domain.get("size_outer"), false).unwrap_or([0.0; 4]);
    let inner = keyframe_value_at(domain.get("size_inner"), false).unwrap_or([0.0; 4]);
    let kind = domain.get("type").and_then(Value::as_str).unwrap_or("box");
    match kind {
        "sphere" => {
            let radius = rng.range(inner[0].abs(), outer[0].abs());
            let z = rng.range(-1.0, 1.0);
            let a = rng.range(0.0, std::f32::consts::TAU);
            let r = (1.0 - z * z).max(0.0).sqrt() * radius;
            [
                center[0] + r * a.cos(),
                center[1] + r * a.sin(),
                center[2] + z * radius,
            ]
        }
        "cylinder" => {
            let radius = rng.range(inner[1].abs(), outer[1].abs());
            let a = rng.range(0.0, std::f32::consts::TAU);
            [
                center[0] + rng.range(-outer[0].abs(), outer[0].abs()),
                center[1] + radius * a.cos(),
                center[2] + radius * a.sin(),
            ]
        }
        _ => [
            center[0] + rng.range(-outer[0].abs(), outer[0].abs()),
            center[1] + rng.range(-outer[1].abs(), outer[1].abs()),
            center[2] + rng.range(-outer[2].abs(), outer[2].abs()),
        ],
    }
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

fn emitter_range(emitter: &Value, name: &str) -> Option<(f32, f32)> {
    let property = emitter
        .get("keyframe_properties")?
        .as_array()?
        .iter()
        .find(|value| value.get("name").and_then(Value::as_str) == Some(name))?;
    let value = keyframe_value_at(Some(property), false)?;
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
mod tests {
    use super::*;

    #[test]
    fn nvpt_wire_parser_rejects_wrong_magic() {
        assert!(decode_particle_dictionary_wire(b"NOPE").is_err());
    }

    #[test]
    fn default_basis_faces_minus_z() {
        let basis = basis_from_forward([0.0, 0.0, -1.0]).unwrap();
        assert_eq!(basis.forward, [0.0, 0.0, -1.0]);
        assert!((basis.right[0] - 1.0).abs() < 1e-6);
        assert!((basis.up[1] - 1.0).abs() < 1e-6);
    }
}
