use super::*;

fn input() -> RecoveryPlacementInput {
    RecoveryPlacementInput {
        pose_match: RecoveryPoseMatch {
            heading_radians: 0.0,
            translation: [0.0, 1.0, 0.0],
            normalized_error: 0.1,
        },
        root_local: [0.0; 3],
        model_offset: [0.0; 3],
        floor_offset: -1.0,
        capsule_radius: 0.3,
        capsule_center_offset: 0.9,
        clearance: 0.002,
    }
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.00001, "{a} != {b}");
}

#[test]
fn model_origin_capsule_center_and_rotated_root_remain_distinct() {
    let mut i = input();
    i.pose_match.heading_radians = std::f32::consts::FRAC_PI_2;
    i.pose_match.translation = [20.0, 10.0, -5.0];
    i.model_offset = [0.4, 0.2, -0.6];
    i.floor_offset = -1.1;
    i.root_local = [1.0, -0.3, 0.5];
    let r = find_recovery_placement(
        i,
        |p| {
            close(p[0], 19.6);
            close(p[2], -4.4);
            Some(2.0)
        },
        |_| true,
        |_| true,
    );
    let p = r.selected.unwrap();
    assert_eq!(r.tested, 1);
    assert_eq!(p.radial_sample, 0);
    close(p.model_origin[1], 3.102);
    close(p.capsule_center[1], 2.902);
    close(p.root_world[0], 20.5);
    close(p.root_world[1], 2.802);
    close(p.root_world[2], -6.0);
}

#[test]
fn a_blocked_match_searches_a_bounded_circle_without_crossing_a_wall() {
    let i = input();
    let r = find_recovery_placement(
        i,
        |_| Some(0.0),
        |p| p.capsule_center[0] > 0.29,
        |p| p[0] > 0.29,
    );
    let p = r.selected.unwrap();
    assert_eq!(p.radial_sample, 7);
    assert_eq!(r.tested, 8);
    close(p.model_origin[0], 0.3);
    close(p.model_origin[2], 0.0);
    assert!(r.obstructed > 0);
}

#[test]
fn missing_floor_does_not_commit_or_query_a_volume() {
    let r = find_recovery_placement(
        input(),
        |_| None,
        |_| panic!("no volume without ground"),
        |_| panic!("no path without ground"),
    );
    assert!(r.selected.is_none());
    assert_eq!(r.tested, 9);
    assert_eq!(r.missing_ground, 9);
}

#[test]
fn occupied_volume_never_releases_ownership_or_asks_for_root_visibility() {
    let r = find_recovery_placement(
        input(),
        |_| Some(0.0),
        |_| false,
        |_| panic!("an obstructed placement is not admissible"),
    );
    assert!(r.selected.is_none());
    assert_eq!(r.obstructed, 9);
}

#[test]
fn root_occlusion_rejects_an_otherwise_empty_capsule() {
    let r = find_recovery_placement(input(), |_| Some(0.0), |_| true, |_| false);
    assert!(r.selected.is_none());
    assert_eq!(r.occluded, 9);
}

#[test]
fn invalid_geometry_and_nonfinite_queries_do_not_admit_a_pose() {
    for field in 0..5 {
        let mut i = input();
        match field {
            0 => i.pose_match.translation[0] = f32::NAN,
            1 => i.root_local[2] = f32::INFINITY,
            2 => i.capsule_radius = -0.3,
            3 => i.capsule_center_offset = 0.1,
            _ => i.clearance = -0.1,
        }
        let r = find_recovery_placement(i, |_| panic!("invalid input"), |_| true, |_| true);
        assert!(r.invalid_input);
        assert_eq!(r.tested, 0);
        assert!(r.selected.is_none());
    }
    let r = find_recovery_placement(input(), |_| Some(f32::NAN), |_| true, |_| true);
    assert!(r.selected.is_none());
    assert_eq!(r.missing_ground, 9);
}

#[test]
fn placement_scales_with_the_installed_body_at_large_world_coordinates() {
    for scale in [0.1, 1.0, 3.0] {
        let mut i = input();
        i.capsule_radius *= scale;
        i.capsule_center_offset *= scale;
        i.floor_offset *= scale;
        i.clearance *= scale;
        i.model_offset = [0.1 * scale, 0.0, -0.2 * scale];
        i.pose_match.translation = [20000.0, scale, -16000.0];
        let p = find_recovery_placement(i, |_| Some(5.0), |_| true, |_| true)
            .selected
            .unwrap();
        assert!((p.capsule_center[0] - (20000.0 - 0.1 * scale)).abs() < 0.003);
        assert!((p.capsule_center[2] - (-16000.0 + 0.2 * scale)).abs() < 0.003);
        close(p.model_origin[1], 5.0 + 1.002 * scale);
        close(p.capsule_center[1], 5.0 + 0.902 * scale);
    }
}

#[test]
fn first_supported_radial_position_is_chosen_in_a_repeatable_order() {
    let r = find_recovery_placement(
        input(),
        |p| if p[2] > 0.29 { Some(0.0) } else { None },
        |_| true,
        |_| true,
    );
    let p = r.selected.unwrap();
    assert_eq!(p.radial_sample, 1);
    assert_eq!(r.tested, 2);
    close(p.model_origin[2], 0.3);
}
