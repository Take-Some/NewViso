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

/// Resolve and cache the dictionary without emitting particles.
pub(super) fn preload_particle_effect(command: &Value, command_index: usize) -> Result<(), String> {
    let asset_ref = required_string(command, "asset_ref", command_index)?;
    let (logical_path, effect_name) = split_asset_ref(asset_ref)?;
    let (dictionary, source) =
        load_particle_dictionary(command, logical_path, effect_name, command_index)?;
    if find_named(dictionary.get("effects"), effect_name).is_none() {
        return Err(format!("script command[{command_index}] particle effect '{effect_name}' is absent from '{source}'"));
    }
    Ok(())
}

pub(super) fn preload_particle_effect_resources(
    scene: &mut Scene3dRuntime,
    command: &Value,
    command_index: usize,
) -> Result<(), String> {
    preload_particle_effect(command, command_index)?;
    let (path, effect) = split_asset_ref(required_string(command, "asset_ref", command_index)?)?;
    let (dictionary, _) = load_particle_dictionary(command, path, effect, command_index)?;
    let assets = AssetClient::new();
    for reference in effect_texture_references(&dictionary, effect)? {
        if !scene.particle_texture_registered(&reference) {
            scene.register_particle_texture(
                &reference,
                super::load_sky_texture(&assets, &reference, true)?,
            )?;
        }
    }
    Ok(())
}

fn collect_texture_references(value: &Value, references: &mut std::collections::BTreeSet<String>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if key == "texture_ref" {
                    if let Some(reference) = value.as_str() {
                        references.insert(reference.to_owned());
                    }
                } else {
                    collect_texture_references(value, references);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_texture_references(value, references);
            }
        }
        _ => {}
    }
}

fn effect_texture_references(
    dictionary: &Value,
    name: &str,
) -> Result<std::collections::BTreeSet<String>, String> {
    let effect = find_named(dictionary.get("effects"), name)
        .ok_or_else(|| format!("particle effect '{name}' is absent"))?;
    let events = effect
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("particle effect '{name}' has no event list"))?;
    let mut references = std::collections::BTreeSet::new();
    for event in events {
        if event.get("event_type").and_then(Value::as_u64).unwrap_or(0) != 0 {
            continue;
        }
        let name = event
            .get("particle_rule")
            .and_then(Value::as_str)
            .unwrap_or("");
        let particle = find_named(dictionary.get("particles"), name)
            .ok_or_else(|| format!("particle effect references missing particle '{name}'"))?;
        collect_texture_references(particle, &mut references);
    }
    Ok(references)
}

fn load_particle_dictionary(
    command: &Value,
    logical_path: &str,
    effect_name: &str,
    command_index: usize,
) -> Result<(Arc<Value>, String), String> {
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
            let particle_wire = match semantic_decode {
                Ok(bytes) => Ok((bytes, PARTICLE_RUNTIME_OUTPUT)),
                Err(semantic_error) => {
                    // A registered YPT format does not imply the native decoder
                    // worker is deployed. Try a canonical embedded wire body.
                    let raw = assets.decode(
                        logical_path,
                        PARTICLE_RAW_BODY_OUTPUT,
                        json!({"entry": effect_name}),
                    );
                    match raw {
                        Ok(bytes) => Ok((bytes, PARTICLE_RAW_BODY_OUTPUT)),
                        Err(raw_error) => Err((semantic_error, raw_error)),
                    }
                }
            };
            match particle_wire {
                Ok((bytes, decode_route)) => (
                    decode_particle_dictionary_wire(&bytes).map_err(|error| {
                        format!(
                            "script command[{command_index}] particle dictionary '{}' decode failed via '{}': {error}",
                            logical_path, decode_route
                        )
                    })?,
                    logical_path.to_owned(),
                ),
                Err((semantic_error, raw_error)) => {
                    // Only absent codecs may use an explicitly authored
                    // fallback. Actual parse/corruption failures remain fatal.
                    let codec_missing = semantic_error.contains("no registered codec accepted")
                        && raw_error.contains("no registered codec accepted");
                    let fallback = command
                        .get("fallback_catalog")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|path| !path.is_empty());
                    if !codec_missing || fallback.is_none() {
                        return Err(format!(
                            "script command[{command_index}] native YPT decode failed path='{}': semantic '{}': {}; raw-body '{}': {}",
                            logical_path, PARTICLE_RUNTIME_OUTPUT, semantic_error,
                            PARTICLE_RAW_BODY_OUTPUT, raw_error
                        ));
                    }
                    let fallback = fallback.expect("validated authored fallback path");
                    let value = assets.json(fallback).map_err(|error| {
                        format!(
                            "script command[{command_index}] YPT codec unavailable and authored fallback '{}' failed: {error}",
                            fallback
                        )
                    })?;
                    if find_named(value.get("effects"), effect_name).is_none() {
                        return Err(format!(
                            "script command[{command_index}] authored fallback '{}' has no particle effect '{}'",
                            fallback, effect_name
                        ));
                    }
                    newviso_host::warn(
                        "newviso.particles",
                        format!("native YPT codec unavailable for '{}'; using authored particle catalog '{}'", logical_path, fallback),
                    );
                    (value, fallback.to_owned())
                }
            }
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
    Ok((dictionary, source))
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
    let (dictionary, source) =
        load_particle_dictionary(command, logical_path, effect_name, command_index)?;

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

mod emission;
mod catalog;
use emission::*;
use catalog::*;

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
    fn preload_closure_includes_sprite_and_model_textures_without_unrelated_particles() {
        let dictionary = json!({
            "effects":[{"name":"water","events":[{"event_type":0,"particle_rule":"splash"},
                {"event_type":0,"particle_rule":"mesh"},{"event_type":0,"particle_rule":"splash"}]}],
            "particles":[{"name":"splash","texture_ref":"textures@splash"},
                {"name":"mesh","meshes":[{"texture_ref":"textures@mesh"}]},
                {"name":"unrelated","texture_ref":"textures@unused"}]
        });
        assert_eq!(
            effect_texture_references(&dictionary, "water")
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["textures@mesh", "textures@splash"]
        );
        assert!(effect_texture_references(&dictionary, "missing").is_err());
    }

    #[test]
    fn preload_reuses_the_dictionary_and_validates_the_requested_effect() {
        let key = "tests/preloaded.ypt|".to_owned();
        let dictionary = Arc::new(json!({"effects":[{"name":"water_splash_generic"}]}));
        DICTIONARIES.with(|cache| {
            cache.borrow_mut().insert(
                key.clone(),
                (dictionary.clone(), "tests/preloaded.ypt".into()),
            );
        });
        let command = json!({"asset_ref":"tests/preloaded.ypt@water_splash_generic"});
        preload_particle_effect(&command, 0).unwrap();
        preload_particle_effect(&command, 0).unwrap();
        DICTIONARIES.with(|cache| {
            assert!(Arc::ptr_eq(&dictionary, &cache.borrow()[&key].0));
        });
        let missing = json!({"asset_ref":"tests/preloaded.ypt@missing_effect"});
        assert!(preload_particle_effect(&missing, 0)
            .unwrap_err()
            .contains("is absent"));
        DICTIONARIES.with(|cache| {
            cache.borrow_mut().remove(&key);
        });
    }

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
