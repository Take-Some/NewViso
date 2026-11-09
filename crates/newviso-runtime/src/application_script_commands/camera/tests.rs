use super::*;

#[test]
fn boom_collision_preserves_direction_even_when_shorter_than_the_old_target_distance() {
    let origin = [0.0, 1.7, 0.0];
    let requested = [0.48, 2.02, 3.25];
    for direction in [[0.0, 0.0, -1.0], [0.6, 0.0, -0.8], [0.0, 0.6, -0.8]] {
        let target = std::array::from_fn(|i| requested[i] + direction[i]);
        for fraction in [0.0, 0.1, 0.2, 0.5, 1.0] {
            let resolved =
                std::array::from_fn(|i| origin[i] + (requested[i] - origin[i]) * fraction);
            let target = camera_collision_target(requested, resolved, target, true);
            for i in 0..3 {
                assert!((target[i] - resolved[i] - direction[i]).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn authored_world_look_at_target_is_unchanged_without_the_directional_contract() {
    let target = [4.0, 2.0, -10.0];
    assert_eq!(
        camera_collision_target([0.0, 2.0, 4.0], [0.0, 2.0, 0.5], target, false),
        target
    );
}
