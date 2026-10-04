use super::*;

pub(super) const DOOR_AUDIO_OPEN_RATIO: f32 = 0.01;
pub(super) const DOOR_AUDIO_CLOSE_RATIO: f32 = 0.10;
pub(super) const DOOR_AUDIO_RETRIGGER_SECONDS: f64 = 0.25;

pub(super) fn vehicle_door_audio_transition(
    previous: f32,
    current: f32,
) -> Option<VehicleEventKind> {
    if previous < DOOR_AUDIO_OPEN_RATIO && current >= DOOR_AUDIO_OPEN_RATIO {
        Some(VehicleEventKind::DoorOpened)
    } else if previous >= DOOR_AUDIO_CLOSE_RATIO && current < DOOR_AUDIO_CLOSE_RATIO {
        Some(VehicleEventKind::DoorClosed)
    } else {
        None
    }
}

pub(super) fn rollable_window(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "window_lf" | "window_rf" | "window_lr" | "window_rr"
    )
}

pub(super) fn vehicle_damage_sample(
    entity: u64,
    part_index: u32,
    point: [f32; 3],
    salt: u32,
) -> f32 {
    let mut value = (entity as u32)
        ^ (entity >> 32) as u32
        ^ part_index.wrapping_mul(0x9E37_79B9)
        ^ salt.wrapping_mul(0x85EB_CA6B);
    for component in point {
        value ^= component.to_bits().wrapping_mul(0xC2B2_AE35);
        value ^= value >> 16;
        value = value.wrapping_mul(0x7FEB_352D);
        value ^= value >> 15;
    }
    value ^= value >> 16;
    value = value.wrapping_mul(0x846C_A68B);
    value ^= value >> 16;
    (value as f64 / u32::MAX as f64) as f32
}

pub(super) fn loose_panel_damage_gate(body_health: f32, damage: f32) -> bool {
    damage > 20.0 || (body_health < 600.0 && damage > 5.0)
}

pub(super) fn break_panel_damage_gate(body_health: f32, damage: f32, upside_down: bool) -> bool {
    damage > 120.0
        || (body_health < 700.0 && damage > 30.0)
        || (body_health < 500.0 && damage > 10.0)
        || (upside_down && damage > 5.0)
}

pub(super) fn damage_probability(base: f32, damage: f32, angular_multiplier: f32) -> f32 {
    let rate = ((damage - 40.0) / 60.0).clamp(0.0, 1.0);
    (base + (1.0 - base) * rate)
        .max(angular_multiplier.clamp(0.0, 1.0))
        .clamp(0.0, 1.0)
}

pub(super) fn damage_threshold(base: f32, angular_multiplier: f32) -> f32 {
    base * (1.0 - angular_multiplier.clamp(0.0, 1.0))
}

pub(super) fn loose_latched_ratio(role: ModelFragmentPartRole) -> f32 {
    match role {
        // Reference loose latch angles are 0.025 rad for side doors and
        // 0.05 rad for bonnet/boot. Convert to our normalized authored travel.
        ModelFragmentPartRole::Bonnet | ModelFragmentPartRole::Boot => 0.042,
        _ => 0.021,
    }
}

pub(super) fn vehicle_part_damage_detachable(
    role: ModelFragmentPartRole,
    name_lower: &str,
) -> bool {
    match role {
        ModelFragmentPartRole::BodyPanel => {
            name_lower.contains("bumper")
                || name_lower.contains("wing_")
                || name_lower.starts_with("wing")
                || name_lower.contains("fender")
        }
        ModelFragmentPartRole::Door
        | ModelFragmentPartRole::Bonnet
        | ModelFragmentPartRole::Boot
        | ModelFragmentPartRole::Wheel
        | ModelFragmentPartRole::Breakable
        | ModelFragmentPartRole::Extra
        | ModelFragmentPartRole::Spoiler
        | ModelFragmentPartRole::Roof => true,
        _ => false,
    }
}

pub(super) fn vehicle_part_detachable(role: ModelFragmentPartRole) -> bool {
    matches!(
        role,
        ModelFragmentPartRole::Door
            | ModelFragmentPartRole::Bonnet
            | ModelFragmentPartRole::Boot
            | ModelFragmentPartRole::Wheel
            | ModelFragmentPartRole::BodyPanel
            | ModelFragmentPartRole::Breakable
            | ModelFragmentPartRole::Extra
            | ModelFragmentPartRole::Spoiler
            | ModelFragmentPartRole::Roof
    )
}

pub(super) fn tire_condition_from_damage(damage: f32) -> TireCondition {
    if damage >= 1.0 {
        TireCondition::Missing
    } else if damage >= 0.65 {
        TireCondition::Rim
    } else if damage >= 0.15 {
        TireCondition::Punctured
    } else {
        TireCondition::Intact
    }
}

pub(super) fn detached_fragment_velocity(
    linear: [f32; 3],
    angular: [f32; 3],
    offset: [f32; 3],
) -> [f32; 3] {
    [
        linear[0] + angular[1] * offset[2] - angular[2] * offset[1],
        linear[1] + angular[2] * offset[0] - angular[0] * offset[2],
        linear[2] + angular[0] * offset[1] - angular[1] * offset[0],
    ]
}

pub(super) fn apply_vehicle_parent_articulation(
    pose: &mut SceneModelPartPose,
    parent: &VehiclePresentationPart,
) {
    match parent.role {
        ModelFragmentPartRole::Door => {
            let lower = parent.name.to_ascii_lowercase();
            let direction =
                if lower.contains("pside") || lower.contains("_rf") || lower.contains("_rr") {
                    1.0
                } else {
                    -1.0
                };
            let damage = parent.damage.clamp(0.0, 1.0);
            pose.rotation_degrees[1] += direction * 70.0 * parent.open;
            pose.rotation_degrees[2] += direction * damage * 4.5;
            pose.translation[0] += direction * damage * 0.035;
            pose.translation[1] -= damage * 0.018;
        }
        ModelFragmentPartRole::Bonnet => {
            let damage = parent.damage.clamp(0.0, 1.0);
            pose.rotation_degrees[0] += 58.0 * parent.open - damage * 5.0;
            pose.translation[1] -= damage * 0.025;
        }
        ModelFragmentPartRole::Boot => {
            let damage = parent.damage.clamp(0.0, 1.0);
            pose.rotation_degrees[0] += -68.0 * parent.open + damage * 4.0;
            pose.translation[1] -= damage * 0.018;
        }
        ModelFragmentPartRole::BodyPanel => {
            let damage = parent.damage.clamp(0.0, 1.0);
            let side = if parent.pivot[0] < 0.0 { -1.0 } else { 1.0 };
            pose.scale = [
                1.0 - damage * 0.045,
                1.0 - damage * 0.09,
                1.0 - damage * 0.025,
            ];
            pose.translation[0] -= side * damage * 0.035;
            pose.translation[1] -= damage * 0.025;
            pose.rotation_degrees[2] += side * damage * 2.5;
        }
        _ => {}
    }
}

pub(super) fn inverse_rotate_euler_xyz(value: [f32; 3], rotation_degrees: [f32; 3]) -> [f32; 3] {
    let [rx, ry, rz] = rotation_degrees.map(f32::to_radians);
    let (sx, cx) = (-rx).sin_cos();
    let (sy, cy) = (-ry).sin_cos();
    let (sz, cz) = (-rz).sin_cos();

    // Forward transform is X -> Y -> Z, therefore the inverse is -Z -> -Y -> -X.
    let after_z = [
        value[0] * cz - value[1] * sz,
        value[0] * sz + value[1] * cz,
        value[2],
    ];
    let after_y = [
        after_z[0] * cy + after_z[2] * sy,
        after_z[1],
        -after_z[0] * sy + after_z[2] * cy,
    ];
    [
        after_y[0],
        after_y[1] * cx - after_y[2] * sx,
        after_y[1] * sx + after_y[2] * cx,
    ]
}

pub(super) fn vehicle_local_point(
    position: [f32; 3],
    rotation_degrees: [f32; 3],
    scale: [f32; 3],
    local: [f32; 3],
) -> [f32; 3] {
    let scaled = [
        local[0] * scale[0],
        local[1] * scale[1],
        local[2] * scale[2],
    ];
    let rotated = rotate_euler_xyz(scaled, rotation_degrees);
    [
        position[0] + rotated[0],
        position[1] + rotated[1],
        position[2] + rotated[2],
    ]
}

pub(super) fn split_runtime_wheel_meshes(
    model: &ModelResource,
    mesh_names: &[String],
    pivot: [f32; 3],
) -> Option<(Vec<String>, Vec<String>)> {
    if mesh_names.len() < 2 {
        return None;
    }

    fn mesh_mean_radius(mesh: &newviso_model::MeshResource, pivot: [f32; 3]) -> Option<f32> {
        let positions = mesh.vertex_streams.iter().find(|stream| {
            stream.semantic == newviso_model::VertexSemantic::Position
                && stream.format == newviso_model::VertexFormat::Float32x3
        })?;
        let stride = usize::try_from(positions.stride).ok()?;
        if stride < 12 {
            return None;
        }

        let mut indices = std::collections::BTreeSet::<usize>::new();
        for primitive in &mesh.primitives {
            let first = usize::try_from(primitive.first_index).ok()?;
            let count = usize::try_from(primitive.index_count).ok()?;
            let end = first.checked_add(count)?;
            for raw_index in first..end {
                let source_index = match mesh.index_buffer.format {
                    newviso_model::IndexFormat::U16 => {
                        let offset = raw_index.checked_mul(2)?;
                        let bytes = mesh.index_buffer.data.get(offset..offset + 2)?;
                        u16::from_le_bytes([bytes[0], bytes[1]]) as i64
                    }
                    newviso_model::IndexFormat::U32 => {
                        let offset = raw_index.checked_mul(4)?;
                        let bytes = mesh.index_buffer.data.get(offset..offset + 4)?;
                        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64
                    }
                };
                let vertex = source_index + i64::from(primitive.base_vertex);
                if vertex >= 0 {
                    indices.insert(vertex as usize);
                }
            }
        }
        if indices.is_empty() {
            return None;
        }

        let mut total = 0.0f32;
        let mut count = 0usize;
        for index in indices {
            let offset = index.checked_mul(stride)?;
            let bytes = positions.data.get(offset..offset + 12)?;
            let y = f32::from_le_bytes(bytes[4..8].try_into().ok()?);
            let z = f32::from_le_bytes(bytes[8..12].try_into().ok()?);
            let dy = y - pivot[1];
            let dz = z - pivot[2];
            let radius = (dy * dy + dz * dz).sqrt();
            if radius.is_finite() {
                total += radius;
                count += 1;
            }
        }
        (count > 0).then_some(total / count as f32)
    }

    let measured = mesh_names
        .iter()
        .filter_map(|name| {
            model
                .meshes
                .iter()
                .find(|mesh| mesh.name == *name)
                .and_then(|mesh| mesh_mean_radius(mesh, pivot))
                .map(|radius| (name.clone(), radius))
        })
        .collect::<Vec<_>>();
    if measured.len() != mesh_names.len() {
        return None;
    }

    let outer_mean = measured
        .iter()
        .map(|(_, radius)| *radius)
        .fold(0.0f32, f32::max);
    if outer_mean <= 0.0 {
        return None;
    }
    let split_radius = outer_mean * 0.94;
    let mut tyre = Vec::new();
    let mut rim = Vec::new();
    for (name, radius) in measured {
        if radius < split_radius {
            rim.push(name);
        } else {
            tyre.push(name);
        }
    }
    (!tyre.is_empty() && !rim.is_empty()).then_some((tyre, rim))
}

pub(super) fn infer_wheel_layout_from_model(
    model: &ModelResource,
    handling: &HandlingData,
) -> Vec<WheelConfig> {
    let Some(fragment) = model.fragment.as_ref() else {
        return Vec::new();
    };
    let ordered_slots = [
        ModelWheelSlot::FrontLeft,
        ModelWheelSlot::FrontRight,
        ModelWheelSlot::RearLeft,
        ModelWheelSlot::RearRight,
    ];
    let mut wheels = Vec::<WheelConfig>::new();
    for slot in ordered_slots {
        let Some(part) = fragment.parts.iter().find(|part| {
            part.role == ModelFragmentPartRole::Wheel && part.wheel_slot == Some(slot)
        }) else {
            continue;
        };

        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut found_bounds = false;
        for mesh_name in &part.mesh_names {
            let Some(mesh) = model.meshes.iter().find(|mesh| mesh.name == *mesh_name) else {
                continue;
            };
            for axis in 0..3 {
                min[axis] = min[axis].min(mesh.bounds.min[axis]);
                max[axis] = max[axis].max(mesh.bounds.max[axis]);
            }
            found_bounds = true;
        }
        let radius = if found_bounds {
            (((max[1] - min[1]).abs().max((max[2] - min[2]).abs())) * 0.5).clamp(0.15, 1.5)
        } else {
            0.34
        };
        let width = if found_bounds {
            (max[0] - min[0]).abs().clamp(0.06, 0.9)
        } else {
            0.24
        };
        let front = matches!(slot, ModelWheelSlot::FrontLeft | ModelWheelSlot::FrontRight);
        let left = matches!(slot, ModelWheelSlot::FrontLeft | ModelWheelSlot::RearLeft);
        let front_drive = handling.front_drive_weight();
        let driven = if front {
            front_drive > 0.001
        } else {
            (1.0 - front_drive) > 0.001
        };
        let rest_length = (radius * 0.82).clamp(0.18, 0.48);
        wheels.push(WheelConfig {
            name: part.name.clone(),
            mount_local: [
                part.rest_position[0],
                part.rest_position[1] + rest_length,
                part.rest_position[2],
            ],
            radius,
            width,
            rest_length,
            travel_up: handling.suspension_upper_limit.max(0.08).min(0.35),
            travel_down: handling.suspension_lower_limit.max(0.08).min(0.40),
            steered: front,
            driven,
            handbrake: !front,
            front,
            left,
            opposite_index: None,
            grip_multiplier: 1.0,
        });
    }

    for index in 0..wheels.len() {
        let target_front = wheels[index].front;
        let target_left = !wheels[index].left;
        wheels[index].opposite_index = wheels
            .iter()
            .position(|candidate| candidate.front == target_front && candidate.left == target_left);
    }
    wheels
}

pub(super) fn parse_surface_profile(
    value: &Value,
    index: usize,
    label: &str,
) -> Result<VehicleSurfaceProfile, String> {
    let class = value
        .get("class")
        .cloned()
        .map(serde_json::from_value::<VehicleSurfaceClass>)
        .transpose()
        .map_err(|error| {
            format!(
                "script command[{index}] vehicle.surface_policy.set {label} invalid class: {error}"
            )
        })?
        .unwrap_or(VehicleSurfaceClass::Default);
    let mut profile = VehicleSurfaceProfile::for_class(class);
    macro_rules! coefficient {
        ($field:ident, $key:literal) => {
            if let Some(raw) = value.get($key) {
                profile.$field = raw
                    .as_f64()
                    .map(|value| value as f32)
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| {
                        format!(
                            "script command[{index}] vehicle.surface_policy.set {label}.{} must be finite numeric",
                            $key
                        )
                    })?;
            }
        };
    }
    coefficient!(dry_grip, "dry_grip");
    coefficient!(wet_grip, "wet_grip");
    coefficient!(snow_grip, "snow_grip");
    coefficient!(rolling_resistance, "rolling_resistance");
    coefficient!(fx_response, "fx_response");
    profile.validate().map_err(|error| {
        format!("script command[{index}] vehicle.surface_policy.set {label}: {error}")
    })
}

pub(super) fn resolve_wheel_index(
    definition: &VehicleDefinition,
    slot: Option<ModelWheelSlot>,
) -> Option<usize> {
    let slot = slot?;
    definition.wheels.iter().position(|wheel| match slot {
        ModelWheelSlot::FrontLeft => wheel.front && wheel.left,
        ModelWheelSlot::FrontRight => wheel.front && !wheel.left,
        ModelWheelSlot::RearLeft => !wheel.front && wheel.left,
        ModelWheelSlot::RearRight => !wheel.front && !wheel.left,
    })
}

pub(super) fn infer_wheel_slot_from_name(name: &str) -> Option<ModelWheelSlot> {
    let name = name.to_ascii_lowercase();
    if name.contains("_lf") || name.ends_with("lf") {
        Some(ModelWheelSlot::FrontLeft)
    } else if name.contains("_rf") || name.ends_with("rf") {
        Some(ModelWheelSlot::FrontRight)
    } else if name.contains("_lr") || name.ends_with("lr") {
        Some(ModelWheelSlot::RearLeft)
    } else if name.contains("_rr") || name.ends_with("rr") {
        Some(ModelWheelSlot::RearRight)
    } else {
        None
    }
}

pub(super) fn wheel_part_rotation_degrees(rotation_angle: f32, steer_angle: f32) -> [f32; 3] {
    // Physics wheel speed is positive along vehicle forward (-Z). A mesh
    // spinning about +X moves its top toward +Z, so negate spin for rendering.
    // Tyre and rim use the same pose, including when only the rim remains.
    [-rotation_angle.to_degrees(), steer_angle.to_degrees(), 0.0]
}

pub(super) fn rotate_euler_xyz(value: [f32; 3], rotation_degrees: [f32; 3]) -> [f32; 3] {
    let [rx, ry, rz] = rotation_degrees.map(f32::to_radians);
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    let after_x = [
        value[0],
        value[1] * cx - value[2] * sx,
        value[1] * sx + value[2] * cx,
    ];
    let after_y = [
        after_x[0] * cy + after_x[2] * sy,
        after_x[1],
        -after_x[0] * sy + after_x[2] * cy,
    ];
    [
        after_y[0] * cz - after_y[1] * sz,
        after_y[0] * sz + after_y[1] * cz,
        after_y[2],
    ]
}

pub(super) fn parse_vec3(value: &Value, index: usize, label: &str) -> Result<[f32; 3], String> {
    serde_json::from_value::<[f32; 3]>(value.clone())
        .map_err(|error| format!("script command[{index}] {label} must be [x,y,z]: {error}"))
}

pub(super) fn parse_vec4(value: &Value, index: usize, label: &str) -> Result<[f32; 4], String> {
    serde_json::from_value::<[f32; 4]>(value.clone())
        .map_err(|error| format!("script command[{index}] {label} must be [x,y,z,w]: {error}"))
}

pub(super) fn optional_number(
    value: &Value,
    key: &str,
    default: f32,
    index: usize,
) -> Result<f32, String> {
    match value.get(key) {
        Some(raw) => raw
            .as_f64()
            .map(|number| number as f32)
            .filter(|number| number.is_finite())
            .ok_or_else(|| {
                format!("script command[{index}] vehicle field {key} must be finite numeric")
            }),
        None => Ok(default),
    }
}
