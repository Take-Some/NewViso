use super::*;
use newviso_scene::SceneParticleStyle;

pub(super) fn property_value(property: Option<&Value>, t: f32) -> Option<[f32; 4]> {
    let keys = property?.get("keyframes")?.as_array()?;
    let first = keys.first()?;
    let time = |key: &Value| {
        key.get("time")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32
    };
    if t <= time(first) {
        return array4(first.get("value"));
    }
    for pair in keys.windows(2) {
        if t <= time(&pair[1]) {
            let a = array4(pair[0].get("value"))?;
            let b = array4(pair[1].get("value"))?;
            let ratio = ((t - time(&pair[0])) / (time(&pair[1]) - time(&pair[0])).max(1.0e-6))
                .clamp(0.0, 1.0);
            return Some(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * ratio));
        }
    }
    array4(keys.last()?.get("value"))
}

pub(super) fn curve_times(a: Option<&Value>, b: Option<&Value>) -> Vec<f32> {
    let mut times = a
        .into_iter()
        .chain(b)
        .flat_map(|p| {
            p.get("keyframes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|key| key.get("time")?.as_array()?.first()?.as_f64())
        .map(|t| (t as f32).clamp(0.0, 1.0))
        .filter(|t| t.is_finite())
        .collect::<Vec<_>>();
    times.sort_by(f32::total_cmp);
    times.dedup();
    times.truncate(128);
    times
}

pub(super) fn appearance(
    particle: &Value,
    tint: [f32; 4],
    size_multiplier: [f32; 3],
    rng: &mut Rng,
) -> SceneParticleStyle {
    let mut style = SceneParticleStyle::default();
    if let Some(color) = behaviour(particle, "colour") {
        let a = color.get("rgba_min");
        let b = color.get("rgba_max");
        let proportional = color.get("proportional").and_then(Value::as_bool) == Some(true);
        let random: [f32; 4] = if proportional {
            [rng.next(); 4]
        } else {
            std::array::from_fn(|_| rng.next())
        };
        let emissive = color.get("emissive_intensity");
        let emissive_random = rng.next();
        let mut times = curve_times(a, b);
        times.extend(curve_times(emissive, None));
        times.sort_by(f32::total_cmp);
        times.dedup();
        times.truncate(128);
        for t in times {
            let min = property_value(a, t).unwrap_or([1.0; 4]);
            let max = property_value(b, t).unwrap_or(min);
            let emission = property_value(emissive, t).unwrap_or([0.0; 4]);
            let gain = (emission[0] + (emission[1] - emission[0]) * emissive_random).max(1.0);
            style.color_keys.push((
                t,
                std::array::from_fn(|i| {
                    (min[i] + (max[i] - min[i]) * random[i]).max(0.0)
                        * tint[i]
                        * if i < 3 { gain } else { 1.0 }
                }),
            ));
        }
    }
    if let Some(size) = behaviour(particle, "size") {
        let a = size.get("whd_min");
        let b = size.get("whd_max");
        let random = rng.next();
        for t in curve_times(a, b) {
            let min = property_value(a, t).unwrap_or([1.0; 4]);
            let max = property_value(b, t).unwrap_or(min);
            style.size_keys.push((
                t,
                std::array::from_fn(|i| {
                    ((min[i] + (max[i] - min[i]) * random).abs() * size_multiplier[i]).max(0.002)
                }),
            ));
            style.depth_keys.push((
                t,
                [((min[2] + (max[2] - min[2]) * random).abs() * size_multiplier[2]).max(0.002)],
            ));
        }
    }
    style.texture_ref = particle
        .get("texture_ref")
        .and_then(Value::as_str)
        .map(str::to_owned);
    style.texture_grid = particle
        .get("texture_grid")
        .and_then(Value::as_array)
        .filter(|v| v.len() == 2)
        .map(|v| {
            [
                v[0].as_u64().unwrap_or(1) as u32,
                v[1].as_u64().unwrap_or(1) as u32,
            ]
        })
        .unwrap_or([1, 1]);
    style.diffuse_mode = particle
        .get("technique")
        .and_then(|t| t.get("diffuse_mode"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let first = particle
        .get("texture_frame_min")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let last = particle
        .get("texture_frame_max")
        .and_then(Value::as_u64)
        .unwrap_or(first as u64) as u32;
    style.first_frame = rng.range(first as f32, last.saturating_add(1) as f32) as u32;
    style.last_frame = style.first_frame;
    if let Some(animation) = behaviour(particle, "animate_texture") {
        let animated_last = animation
            .get("last_frame_id")
            .and_then(Value::as_i64)
            .unwrap_or(last as i64)
            .max(style.first_frame as i64) as u32;
        let rate = property_value(animation.get("animation_rate"), 0.0).unwrap_or([0.0; 4]);
        style.animation_rate = rng.range(rate[0], rate[1]).max(0.0);
        style.animate_over_life = animation
            .get("scaled_over_life")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        // Loop mode chooses the restart frame; only hold_last_frame disables looping.
        style.loop_animation =
            animation.get("hold_last_frame").and_then(Value::as_bool) != Some(true);
        let mode = animation
            .get("loop_mode")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        style.loop_start_frame = Some(
            match mode {
                1 => 0,
                2 => first,
                3 => last,
                _ => style.first_frame,
            }
            .min(animated_last),
        );
        if mode == 4 {
            style.loop_random_start = Some([first.min(animated_last), last.min(animated_last)]);
            style.animation_seed = rng.0;
        }
        if style.animation_rate > 0.0 || style.animate_over_life {
            style.last_frame = animated_last;
        }
    }
    style
}

pub(super) fn model_meshes(particle: &Value) -> Result<Vec<Arc<SceneParticleMesh>>, String> {
    particle
        .get("model_drawables")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|model| {
            let vertices = model
                .get("vertices")
                .and_then(Value::as_array)
                .ok_or("particle model has no vertices")?
                .iter()
                .map(|v| {
                    let values = v
                        .as_array()
                        .filter(|v| v.len() == 8)
                        .ok_or("invalid particle model vertex")?;
                    let mut vertex = [0.0; 8];
                    for i in 0..8 {
                        vertex[i] = values[i].as_f64().ok_or("invalid model vertex number")? as f32;
                    }
                    Ok(vertex)
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Arc::new(SceneParticleMesh {
                vertices,
                texture_ref: model
                    .get("texture_ref")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            }))
        })
        .collect()
}

pub(super) fn source_local_vector(
    local: [f32; 3],
    basis: Basis,
    scale: f32,
    z_up: bool,
) -> [f32; 3] {
    if z_up {
        // Source Z is the emission axis. The other axes retain a right-handed basis.
        add(
            add(
                mul(basis.right, local[0] * scale),
                mul(basis.up, -local[1] * scale),
            ),
            mul(basis.forward, local[2] * scale),
        )
    } else {
        local_to_world(local, basis, scale)
    }
}

pub(super) fn world_acceleration(
    particle: &Value,
    local: [f32; 3],
    basis: Basis,
    scale: f32,
    z_up: bool,
) -> [f32; 3] {
    let acceleration = behaviour(particle, "acceleration");
    let world = acceleration
        .and_then(|a| a.get("reference_space"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        == 0;
    let scale = if acceleration
        .and_then(|a| a.get("affected_by_zoom"))
        .and_then(Value::as_bool)
        == Some(false)
    {
        1.0
    } else {
        scale
    };
    let mut value = if z_up && world {
        [local[0] * scale, local[2] * scale, -local[1] * scale]
    } else {
        source_local_vector(local, basis, scale, z_up)
    };
    if acceleration
        .and_then(|a| a.get("enable_gravity"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        value[1] -= 9.81;
    }
    value
}

pub(super) fn motion_curves(
    style: &mut SceneParticleStyle,
    emitter: &Value,
    particle: &Value,
    basis: Basis,
    scale: f32,
    phase: f32,
    _playback: f32,
    rng: &mut Rng,
) {
    if let Some(a) = behaviour(particle, "acceleration") {
        let random = rng.next();
        let scalar = property_value(
            emitter_property(emitter, "ptxEmitterRule:m_accnScalarKFP"),
            phase,
        )
        .unwrap_or([100.0; 4]);
        for t in curve_times(a.get("xyz_min"), a.get("xyz_max")) {
            let min = property_value(a.get("xyz_min"), t).unwrap_or([0.0; 4]);
            let max = property_value(a.get("xyz_max"), t).unwrap_or(min);
            let local =
                std::array::from_fn(|i| (min[i] + (max[i] - min[i]) * random) * scalar[i] * 0.01);
            style
                .acceleration_keys
                .push((t, world_acceleration(particle, local, basis, scale, true)));
        }
    }
    if let Some(d) = behaviour(particle, "dampening") {
        let random = rng.next();
        let scalar = property_value(
            emitter_property(emitter, "ptxEmitterRule:m_dampeningScalarKFP"),
            phase,
        )
        .unwrap_or([100.0; 4]);
        for t in curve_times(d.get("xyz_min"), d.get("xyz_max")) {
            let min = property_value(d.get("xyz_min"), t).unwrap_or([0.0; 4]);
            let max = property_value(d.get("xyz_max"), t).unwrap_or(min);
            let local: [f32; 3] = std::array::from_fn(|i| {
                ((min[i] + (max[i] - min[i]) * random) * scalar[i] * 0.01).max(0.0)
            });
            let world = if d
                .get("reference_space")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                == 0
            {
                [local[0], local[2], local[1]]
            } else {
                std::array::from_fn(|i| {
                    basis.right[i].abs() * local[0]
                        + basis.up[i].abs() * local[1]
                        + basis.forward[i].abs() * local[2]
                })
            };
            style.drag_keys.push((t, world));
        }
    }
    if let Some(c) = behaviour(particle, "collision") {
        let chance = number(c.get("collision_chance"))
            .unwrap_or(100.0)
            .clamp(0.0, 100.0);
        if rng.next() * 100.0 < chance {
            let range = property_value(c.get("bounciness"), 0.0).unwrap_or([0.0; 4]);
            style.collision = Some([
                (rng.range(range[0], range[1]) * 0.01).clamp(0.0, 1.0),
                number(c.get("radius_multiplier")).unwrap_or(1.0).max(0.0),
                number(c.get("minimum_radius")).unwrap_or(0.0).max(0.0),
                number(c.get("rest_speed")).unwrap_or(0.0).max(0.02),
            ]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intermediate_opacity_key_is_preserved() {
        let particle = json!({"behaviours":[{"type":"colour","rgba_min":{"keyframes":[
            {"time":[0.0],"value":[1.0,1.0,1.0,0.0]}, {"time":[0.3],"value":[1.0,1.0,1.0,0.8]}, {"time":[1.0],"value":[1.0,1.0,1.0,0.0]}
        ]}}]});
        let style = appearance(&particle, [1.0; 4], [1.0; 3], &mut Rng(7));
        assert_eq!(style.color_keys.len(), 3);
        assert_eq!(style.color_keys[1].1[3], 0.8);
    }
    #[test]
    fn authored_width_height_and_depth_remain_independent() {
        let particle = json!({"behaviours":[{"type":"size","whd_min":{"keyframes":[{"time":[0.0],"value":[2.0,2.0,2.0,0.0]}]}}]});
        let style = appearance(&particle, [1.0; 4], [1.0, 0.2, 0.05], &mut Rng(7));
        assert_eq!(style.size_keys[0].1, [2.0, 0.4]);
        assert_eq!(style.depth_keys[0].1, [0.1]);
    }
    #[test]
    fn original_fire_emissive_curve_changes_rgb_without_changing_opacity() {
        let particle = json!({"behaviours":[{"type":"colour",
            "rgba_min":{"keyframes":[{"time":[0.0],"value":[1.0,0.5,0.25,0.1]}]},
            "emissive_intensity":{"keyframes":[{"time":[0.0],"value":[12.0,12.0,0.0,0.0]}]}
        }]});
        let style = appearance(&particle, [1.0; 4], [1.0; 3], &mut Rng(7));
        assert_eq!(style.color_keys[0].1, [12.0, 6.0, 3.0, 0.1]);
    }
    #[test]
    fn source_gravity_stays_down_for_sideways_impacts() {
        let basis = basis_from_forward([1.0, 0.0, 0.0]).unwrap();
        assert_eq!(
            world_acceleration(&json!({}), [0.0, 0.0, -9.81], basis, 1.0, true),
            [0.0, -9.81, -0.0]
        );
    }
}
