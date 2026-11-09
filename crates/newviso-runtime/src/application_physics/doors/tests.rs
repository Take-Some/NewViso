use super::*;
fn runtime() -> PhysicsRuntime {
    let mut p = PhysicsRuntime::empty_for_tests();
    p.supports_joints = true;
    p.supports_articulation = true;
    p.upsert_body_from_script(
        &json!({"entity":1,"body_kind":"static","position":[0,1.06,-5],
  "shape":{"kind":"sphere","radius":0.03},"casts_contacts":false,"participates_in_queries":false}),
        0,
    )
    .unwrap();
    p.upsert_body_from_script(
        &json!({"entity":2,"body_kind":"dynamic","position":[0.5,1.06,-5],
  "shape":{"kind":"box","half_extents":[0.5,1.05,0.03]},"mass_properties":{"mass":25,
  "center_of_mass":[0,0,0],"inertia_diagonal":[9.2,2.1,11.3]}}),
        0,
    )
    .unwrap();
    p.upsert_hinge_from_script(&json!({"joint_ids":[11,12],"parent":1,"child":2,
  "parent_pivot":[0,0,0],"child_pivot":[-0.5,0,0],"parent_frame_rotation":[0,0,0,1],
  "child_frame_rotation":[0,0,0,1],"span":1.5,"max_force":50000,"collision_family":1,
  "angle_limits":[-1.7,1.7],"stop_stiffness":1600,"stop_damping":12,"stop_max_torque":800}))
        .unwrap();
    for id in [3, 4] {
        p.upsert_body_from_script(
            &json!({"entity":id,"body_kind":"dynamic",
  "position":[if id==3{0.8}else{0.2},0.95,-4.4],"shape":{"kind":"capsule","radius":0.3,"half_height":0.6}}),
            0,
        )
        .unwrap();
    }
    p.configure_door(&json!({"entity":2,"joint_ids":[11,12],"mechanism":{"closer_torque_nm":0}}))
        .unwrap();
    p.update_door_controls(1.0 / 120.0).unwrap();
    p
}
fn actor(id: &str, body: u64) -> DoorActor {
    DoorActor {
        id: id.into(),
        body,
        radius: 0.3,
        height: 1.8,
        access: BTreeSet::new(),
    }
}
#[test]
fn latch_is_native_effort_and_handle_restores_the_original_hinge_envelope() {
    let mut p = runtime();
    let pose = p.bodies[&2].position;
    let hardware = p.doors.entries[&2].retention_joint;
    assert!(p.joints.contains_key(&hardware));
    assert!((p.joints[&hardware].max_force - 500.0).abs() < 0.001);
    assert_eq!(
        p.interact_door(2, actor("player", 3), "open").unwrap(),
        dm::DoorResult::Accepted
    );
    p.update_door_controls(1.0 / 120.0).unwrap();
    assert_eq!(p.joints[&11].angular_limits.unwrap().twist, [-1.7, 1.7]);
    assert!(!p.joints.contains_key(&hardware));
    assert!(p.joints[&11].motor.unwrap().max_torque <= 45.0);
    assert_eq!(p.bodies[&2].position, pose);
    p.remove_door(2);
    assert!(p.joints[&11].motor.is_none());
}
#[test]
fn door_queues_are_fifo_and_removed_actors_cannot_keep_ownership() {
    let mut p = runtime();
    p.plan_door_passage(actor("first", 3), [0.8, 0.05, -4.4], [0.8, 0.05, -6.0])
        .unwrap();
    let second = p
        .plan_door_passage(actor("second", 4), [0.8, 0.05, -4.4], [0.8, 0.05, -6.0])
        .unwrap();
    assert_eq!(second.phase, "queued");
    assert_eq!(p.doors.entries[&2].queue.len(), 2);
    p.release_door_actor("first");
    assert_eq!(p.doors.entries[&2].queue.front().unwrap().actor, "second");
    p.destroy_body_from_script(&json!({"entity":4}), 0).unwrap();
    p.update_door_controls(1.0 / 120.0).unwrap();
    assert!(p.doors.entries[&2].queue.is_empty());
}
#[test]
fn conflicting_mechanisms_are_rejected_and_teardown_restores_joint_authority() {
    let mut p = runtime();
    assert!(p
        .configure_door(&json!({"entity":99,"joint_ids":[11,12],"mechanism":{}}))
        .is_err());
    assert_eq!(p.doors.entries.len(), 1);
    p.remove_door(2);
    assert_eq!(p.joints[&11].angular_limits.unwrap().twist, [-1.7, 1.7]);
    assert!(p.doors.entries.is_empty());
}
#[test]
fn locked_passages_are_blocked_until_the_actual_actor_has_access() {
    let mut p = runtime();
    p.doors
        .entries
        .get_mut(&2)
        .unwrap()
        .mechanism
        .config
        .key_item = Some("key".into());
    p.interact_door(
        2,
        DoorActor {
            access: BTreeSet::from([2]),
            ..actor("owner", 3)
        },
        "lock",
    )
    .unwrap();
    let blocked = p
        .plan_door_passage(actor("visitor", 4), [0.8, 0.05, -4.4], [0.8, 0.05, -6.0])
        .unwrap();
    assert!(blocked.denied);
    let allowed = p
        .plan_door_passage(
            DoorActor {
                access: BTreeSet::from([2]),
                ..actor("owner", 3)
            },
            [0.8, 0.05, -4.4],
            [0.8, 0.05, -6.0],
        )
        .unwrap();
    assert!(!allowed.denied);
    assert!(!p.doors.entries[&2].mechanism.state.locked);
}

#[test]
fn repeated_configuration_is_idempotent_and_save_identity_cannot_alias() {
    let mut p = runtime();
    p.configure_door(&json!({"entity":2,"joint_ids":[11,12],"mechanism":{"closer_torque_nm":0}}))
        .unwrap();
    assert!(p
        .configure_door(&json!({"entity":8,"joint_ids":[21,22],"persistent_id":"2","mechanism":{}}))
        .is_err());
    assert_eq!(p.doors.entries.len(), 1);
}
#[test]
fn checkpoint_restores_hardware_and_native_pose_but_not_actor_leases() {
    let mut p = runtime();
    p.doors.entries.get_mut(&2).unwrap().mechanism.state.locked = true;
    p.plan_door_passage(actor("temporary", 3), [0.8, 0.05, -4.4], [0.8, 0.05, -6.0]);
    let checkpoint = p.door_checkpoint();
    let mut resumed = runtime();
    resumed.remove_door(2);
    resumed.load_door_checkpoint(&checkpoint).unwrap();
    resumed
        .configure_door(&json!({"entity":2,"joint_ids":[11,12],"mechanism":{"closer_torque_nm":0}}))
        .unwrap();
    resumed.update_door_controls(1.0 / 120.0).unwrap();
    assert!(resumed.doors.entries[&2].mechanism.state.locked);
    assert!(resumed.doors.entries[&2].queue.is_empty());
    assert!(resumed.doors.entries[&2].mechanism.state.goal.is_none());
    assert_eq!(resumed.bodies[&2].position, p.bodies[&2].position);
    assert!(resumed.doors.events.iter().any(|e| e["kind"] == "restored"));
    // Scene changes invalidate old hardware/pose instead of moving the new door.
    let mut moved = runtime();
    moved.remove_door(2);
    for id in [1, 2] {
        moved.bodies.get_mut(&id).unwrap().position[0] += 5.0;
    }
    moved.load_door_checkpoint(&checkpoint).unwrap();
    moved
        .configure_door(&json!({"entity":2,"joint_ids":[11,12],"mechanism":{"closer_torque_nm":0}}))
        .unwrap();
    moved.update_door_controls(1.0 / 120.0).unwrap();
    assert!(!moved.doors.entries[&2].mechanism.state.locked);
    assert_eq!(moved.bodies[&2].position[0], 5.5);
    assert!(moved
        .doors
        .events
        .iter()
        .any(|e| e["kind"] == "restore_rejected"));
}
#[test]
fn action_acknowledgement_does_not_claim_the_physical_door_has_opened() {
    let mut p = runtime();
    p.interact_door(2, actor("player", 3), "open").unwrap();
    p.update_door_controls(1.0 / 120.0).unwrap();
    assert!(p
        .doors
        .events
        .iter()
        .any(|e| e["kind"] == "action_result" && e["action"] == "open"));
    assert!(!p.doors.events.iter().any(|e| e["kind"] == "opened"));
}
#[test]
fn stall_expiry_uses_active_ownership_and_does_not_starve_waiters() {
    let mut p = runtime();
    p.plan_door_passage(actor("first", 3), [0.8, 0.05, -4.4], [0.8, 0.05, -6.0]);
    p.plan_door_passage(actor("second", 4), [0.2, 0.05, -4.4], [0.2, 0.05, -6.0]);
    p.update_door_controls(1.0 / 120.0).unwrap();
    p.fixed_seconds = 9.0;
    for r in p.doors.entries.get_mut(&2).unwrap().queue.iter_mut() {
        r.seen = 9.0;
    }
    p.update_door_controls(1.0 / 120.0).unwrap();
    assert_eq!(p.doors.entries[&2].queue.front().unwrap().actor, "second");
    p.update_door_controls(1.0 / 120.0).unwrap();
    assert_eq!(
        p.doors.entries[&2].queue.front().unwrap().active_since,
        Some(9.0)
    );
}

#[test]
fn hardware_targets_can_be_authored_in_leaf_space_without_model_name_rules() {
    let mut p = runtime();
    p.remove_door(2);
    let spec = json!({"entity":2,"joint_ids":[11,12],"handle_local":[0.3,0.1,0.08],
 "latch_local":[0.48,0.1,0.0],"mechanism":{"closer_torque_nm":0}});
    p.configure_door(&spec).unwrap();
    p.update_door_controls(1.0 / 120.0).unwrap();
    let b = &p.doors.entries[&2];
    let g = b.geometry.as_ref().unwrap();
    assert_eq!(g.handle_point, [0.3, 0.1, 0.08]);
    assert_eq!(p.joints[&b.retention_joint].child_anchor, [0.48, 0.1, 0.0]);
    assert!((p.joints[&b.retention_joint].max_force - 500.0 / 0.98).abs() < 0.001);
}

#[test]
fn native_ccd_contact_breaks_bolt_even_with_zero_reported_impulse() {
    let mut p = runtime();
    p.doors.entries.get_mut(&2).unwrap().mechanism.state.locked = true;
    p.upsert_body_from_script(&json!({"entity":9,"body_kind":"dynamic","position":[0.95,1.15,-3.5],
 "shape":{"kind":"sphere","radius":0.22},"mass_properties":{"mass":80,"center_of_mass":[0,0,0],"inertia_diagonal":[1.5488,1.5488,1.5488]}}),0).unwrap();
    p.last_output.events.push(
        serde_json::from_value(json!({"ContactBegin":{"a":9,"b":2,
 "point":[0.95,0.988,-4.974],"normal":[0,0,1],"impulse":0,"relative_velocity":[0,1.716,7]}}))
        .unwrap(),
    );
    p.observe_door_contacts(1.0 / 120.0);
    let mut vehicles = VehicleRuntime::default();
    p.step(0.001, &[], &mut vehicles).unwrap();
    assert!(
        p.last_output.events.is_empty(),
        "render-only frame clears script contact edges"
    );
    p.update_door_controls(1.0 / 120.0).unwrap();
    assert!(
        p.doors.entries[&2].mechanism.state.bolt_broken,
        "{:?}",
        p.doors.entries[&2].mechanism.state
    );
}

#[test]
fn roller_releases_from_measured_contact_when_last_substep_impulse_is_zero() {
    let mut p = runtime();
    p.doors.entries.get_mut(&2).unwrap().mechanism.config.latch = dm::LatchKind::Roller;
    p.last_output.events.push(
        serde_json::from_value(json!({"ContactBegin":{"a":3,"b":2,
 "point":[0.8,0.95,-5],"normal":[0,0,1],"impulse":0,"relative_velocity":[0,0,0.5]}}))
        .unwrap(),
    );
    p.observe_door_contacts(1.0 / 120.0);
    p.update_door_controls(1.0 / 120.0).unwrap();
    let b = &p.doors.entries[&2];
    assert!(!b.mechanism.state.latch_engaged && !b.mechanism.state.latch_broken);
    assert!(!p.joints.contains_key(&b.retention_joint));
}
