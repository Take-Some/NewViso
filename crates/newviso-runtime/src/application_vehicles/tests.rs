use super::*;

#[test]
fn rendered_wheel_rolls_with_vehicle_in_forward_and_reverse() {
    let radius = 0.4;
    let dt = 0.001;
    for speed in [-8.0, 8.0] {
        let pose = wheel_part_rotation_degrees(speed / radius * dt, 0.0);
        let top = rotate_euler_xyz([0.0, radius, 0.0], pose);
        let bottom = rotate_euler_xyz([0.0, -radius, 0.0], pose);
        // Top tread travels in the vehicle's direction. At the ground the
        // mesh motion cancels chassis travel instead of adding to it.
        assert!(top[2] * -speed > 0.0, "speed={speed} top={top:?}");
        assert!(
            (bottom[2] / dt - speed).abs() < 0.002,
            "speed={speed} ground_velocity={}",
            bottom[2] / dt - speed
        );
    }
    for (steer, side) in [(-0.3, 1.0), (0.3, -1.0)] {
        let pose = wheel_part_rotation_degrees(0.0, steer);
        let forward = rotate_euler_xyz([0.0, 0.0, -1.0], pose);
        assert!(forward[0] * side > 0.0);
    }
}

#[test]
fn wheel_slot_name_inference_covers_vehicle_corner_names() {
    assert_eq!(
        infer_wheel_slot_from_name("suspension_lf"),
        Some(ModelWheelSlot::FrontLeft)
    );
    assert_eq!(
        infer_wheel_slot_from_name("hub_rf"),
        Some(ModelWheelSlot::FrontRight)
    );
    assert_eq!(
        infer_wheel_slot_from_name("spring_lr"),
        Some(ModelWheelSlot::RearLeft)
    );
    assert_eq!(
        infer_wheel_slot_from_name("hub_rr"),
        Some(ModelWheelSlot::RearRight)
    );
    assert_eq!(infer_wheel_slot_from_name("engine"), None);
}

#[test]
fn legacy_combined_wheel_meshes_recover_separate_tyre_and_rim_geometry() {
    fn wheel_mesh(name: &str, radius: f32) -> newviso_model::MeshResource {
        let positions = [
            [0.0f32, radius, 0.0],
            [0.0f32, 0.0, radius],
            [0.0f32, -radius, 0.0],
        ];
        let position_bytes = positions
            .into_iter()
            .flatten()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        let index_bytes = [0u16, 1, 2]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        newviso_model::MeshResource {
            name: name.to_owned(),
            vertex_streams: vec![newviso_model::VertexStream {
                semantic: newviso_model::VertexSemantic::Position,
                format: newviso_model::VertexFormat::Float32x3,
                stride: 12,
                vertex_count: 3,
                data: std::sync::Arc::from(position_bytes),
            }],
            index_buffer: newviso_model::IndexBuffer {
                format: newviso_model::IndexFormat::U16,
                index_count: 3,
                data: std::sync::Arc::from(index_bytes),
            },
            primitives: vec![newviso_model::MeshPrimitive {
                first_index: 0,
                index_count: 3,
                base_vertex: 0,
                material_slot: None,
            }],
            bounds: newviso_model::Bounds3 {
                min: [0.0, -radius, -radius],
                max: [0.0, radius, radius],
            },
        }
    }

    let model = ModelResource {
        id: newviso_resource_runtime::AssetId(1),
        name: "legacy_wheel".to_owned(),
        bounds: newviso_model::Bounds3 {
            min: [-1.0; 3],
            max: [1.0; 3],
        },
        meshes: vec![wheel_mesh("tyre_mesh", 0.40), wheel_mesh("rim_mesh", 0.30)],
        material_slots: Vec::new(),
        skin_source_to_model: [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ],
        skeleton: None,
        animations: Vec::new(),
        fragment: None,
    };
    let (tyre, rim) = split_runtime_wheel_meshes(
        &model,
        &["tyre_mesh".to_owned(), "rim_mesh".to_owned()],
        [0.0; 3],
    )
    .expect("legacy wheel meshes should split");
    assert_eq!(tyre, vec!["tyre_mesh"]);
    assert_eq!(rim, vec!["rim_mesh"]);
}

#[test]
fn vehicle_local_rotation_round_trips() {
    let value = [1.25, -0.5, 3.0];
    let rotation = [17.0, -43.0, 29.0];
    let rotated = rotate_euler_xyz(value, rotation);
    let restored = inverse_rotate_euler_xyz(rotated, rotation);
    for axis in 0..3 {
        assert!(
            (restored[axis] - value[axis]).abs() < 1.0e-4,
            "axis={axis} restored={restored:?} source={value:?}"
        );
    }
}

#[test]
fn glass_inherits_parent_door_articulation() {
    let parent = VehiclePresentationPart {
        index: 2,
        name: "door_dside_f".to_owned(),
        name_lower: "door_dside_f".to_owned(),
        rollable_window: false,
        door_motion: Some(VehicleDoorMotionState::default()),
        role: ModelFragmentPartRole::Door,
        parent_part_index: None,
        mesh_names: Vec::new(),
        pivot: [-0.85, 0.7, 0.15],
        wheel_slot: None,
        open: 1.0,
        locked: false,
        visible: true,
        damage: 0.0,
        glass_hit_uv: [0.5; 2],
        loose: false,
        detach_velocity_boost: [0.0; 3],
        fire_intensity: 0.0,
        fire_remaining_seconds: 0.0,
        presentation_override: false,
        detached_entity: None,
    };
    let mut pose = SceneModelPartPose::default();
    pose.pivot = parent.pivot;
    apply_vehicle_parent_articulation(&mut pose, &parent);
    assert_eq!(pose.pivot, parent.pivot);
    assert!((pose.rotation_degrees[1] + 70.0).abs() < 1.0e-5);
    assert_eq!(pose.translation, [0.0; 3]);
}

#[test]
fn rear_glass_inherits_boot_articulation_and_damage() {
    let parent = VehiclePresentationPart {
        index: 12,
        name: "boot".to_owned(),
        name_lower: "boot".to_owned(),
        rollable_window: false,
        door_motion: Some(VehicleDoorMotionState::default()),
        role: ModelFragmentPartRole::Boot,
        parent_part_index: None,
        mesh_names: Vec::new(),
        pivot: [0.0, 0.75, 1.8],
        wheel_slot: None,
        open: 0.5,
        locked: false,
        visible: true,
        damage: 0.4,
        glass_hit_uv: [0.5; 2],
        loose: false,
        detach_velocity_boost: [0.0; 3],
        fire_intensity: 0.0,
        fire_remaining_seconds: 0.0,
        presentation_override: false,
        detached_entity: None,
    };
    let mut pose = SceneModelPartPose::default();
    pose.pivot = parent.pivot;
    apply_vehicle_parent_articulation(&mut pose, &parent);
    assert!((pose.rotation_degrees[0] + 32.4).abs() < 1.0e-5);
    assert!((pose.translation[1] + 0.0072).abs() < 1.0e-5);
}
#[test]
fn detached_fragment_inherits_velocity_at_its_own_position() {
    assert_eq!(
        detached_fragment_velocity([3.0, 0.0, 1.0], [0.0, 2.0, 0.0], [1.0, 0.0, 0.0]),
        [3.0, 0.0, -1.0]
    );
}
#[test]
fn tyre_damage_has_separate_puncture_rim_and_detachment_thresholds() {
    assert_eq!(tire_condition_from_damage(0.0), TireCondition::Intact);
    assert_eq!(tire_condition_from_damage(0.3), TireCondition::Punctured);
    assert_eq!(tire_condition_from_damage(0.7), TireCondition::Rim);
    assert_eq!(tire_condition_from_damage(1.0), TireCondition::Missing);
    assert!(vehicle_part_detachable(ModelFragmentPartRole::Bonnet));
    assert!(vehicle_part_detachable(ModelFragmentPartRole::Breakable));
    assert!(vehicle_part_detachable(ModelFragmentPartRole::BodyPanel));
    assert!(!vehicle_part_detachable(ModelFragmentPartRole::Light));
}

#[test]
fn door_audio_follows_actual_ratio_crossings() {
    assert_eq!(vehicle_door_audio_transition(0.0, 0.009), None);
    assert_eq!(
        vehicle_door_audio_transition(0.009, 0.011),
        Some(VehicleEventKind::DoorOpened)
    );
    assert_eq!(vehicle_door_audio_transition(0.8, 0.11), None);
    assert_eq!(
        vehicle_door_audio_transition(0.11, 0.09),
        Some(VehicleEventKind::DoorClosed)
    );
}

#[test]
fn door_close_drive_carries_more_latch_momentum() {
    let opening = doors::drive_parameters(ModelFragmentPartRole::Door, false);
    let closing = doors::drive_parameters(ModelFragmentPartRole::Door, true);
    assert!(closing.1 < opening.1, "closing damping must be lower");
    assert!(closing.2 > opening.2, "closing max speed must be higher");
    assert!(
        closing.3 > opening.3,
        "closing motor must allow a stronger latch approach"
    );
}

#[test]
fn reference_panel_break_gates_preserve_damage_tiers() {
    assert!(!loose_panel_damage_gate(1000.0, 20.0));
    assert!(loose_panel_damage_gate(1000.0, 20.1));
    assert!(loose_panel_damage_gate(599.0, 5.1));

    assert!(!break_panel_damage_gate(1000.0, 120.0, false));
    assert!(break_panel_damage_gate(1000.0, 120.1, false));
    assert!(break_panel_damage_gate(699.0, 30.1, false));
    assert!(break_panel_damage_gate(499.0, 10.1, false));
    assert!(break_panel_damage_gate(1000.0, 5.1, true));
}

#[test]
fn rollover_multiplier_reduces_latch_damage_thresholds() {
    assert!((damage_threshold(40.0, 0.0) - 40.0).abs() < 1.0e-6);
    assert!((damage_threshold(40.0, 0.5) - 20.0).abs() < 1.0e-6);
    assert_eq!(damage_threshold(40.0, 1.0), 0.0);
    assert!(damage_probability(0.5, 100.0, 0.0) >= 0.999);
    assert!(damage_probability(0.5, 40.0, 0.8) >= 0.8);
}

#[test]
fn loose_latch_uses_reference_small_open_angles() {
    assert!((loose_latched_ratio(ModelFragmentPartRole::Door) - 0.021).abs() < 1.0e-6);
    assert!((loose_latched_ratio(ModelFragmentPartRole::Bonnet) - 0.042).abs() < 1.0e-6);
}

#[test]
fn source_misc_break_parts_exclude_chassis() {
    assert!(vehicle_part_damage_detachable(
        ModelFragmentPartRole::BodyPanel,
        "bumper_f"
    ));
    assert!(vehicle_part_damage_detachable(
        ModelFragmentPartRole::BodyPanel,
        "wing_lf"
    ));
    assert!(vehicle_part_damage_detachable(
        ModelFragmentPartRole::Breakable,
        "breakable_extra_1"
    ));
    assert!(!vehicle_part_damage_detachable(
        ModelFragmentPartRole::BodyPanel,
        "chassis"
    ));
}
