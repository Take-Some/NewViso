use super::*;

#[test]
fn authored_weights_can_change_the_best_pose() {
    let points = clip();
    let mut one = points.clone();
    let mut two = points.clone();
    one[4][1] += 0.3;
    two[5][1] += 0.3;
    let choose = |left: f32, right: f32| {
        let mut observed: Vec<_> = points
            .iter()
            .map(|&position| WeightedRecoveryMarker {
                position,
                weight: 1.0,
            })
            .collect();
        observed[4].weight = left;
        observed[5].weight = right;
        match_weighted_recovery_samples(&observed, [one.as_slice(), two.as_slice()])
            .unwrap()
            .0
    };
    assert_eq!(choose(10.0, 0.1), 1);
    assert_eq!(choose(0.1, 10.0), 0);
}

#[test]
fn recovery_search_selects_the_later_matching_pose_and_breaks_ties_by_time() {
    let matched = clip();
    let mut first = matched.clone();
    first[4][1] += 0.4;
    let observed = observe(&matched, 0.4, [2.0, 0.3, -3.0]);
    let (index, result) = match_recovery_samples(
        &observed,
        [first.as_slice(), &matched[..2], &matched, &matched],
    )
    .unwrap();
    assert_eq!(index, 2);
    assert!(result.normalized_error < 1e-6);
    assert!((result.heading_radians - 0.4).abs() < 1e-6);
    assert!(match_recovery_samples(&observed, [&matched[..2]]).is_none());
}

#[test]
fn recovery_sampling_covers_short_windows_and_bounds_long_clips() {
    for duration in [0.05, 0.4, 1.0, 2.0, 60.0] {
        let times: Vec<_> = recovery_sample_times(duration).collect();
        assert_eq!(times[0], 0.0);
        assert_eq!(*times.last().unwrap(), (duration * 0.25).min(0.5));
        assert!((2..=16).contains(&times.len()));
        assert!(times.windows(2).all(|pair| pair[0] < pair[1]));
    }
    assert_eq!(recovery_sample_times(f32::NAN).collect::<Vec<_>>(), [0.0]);
}

fn clip() -> Vec<[f32; 3]> {
    vec![
        [-0.2, 0.0, 0.0],
        [0.2, 0.0, 0.0],
        [0.0, 0.35, 0.8],
        [0.0, -0.2, -0.8],
        [-0.6, 0.25, 0.2],
        [0.6, 0.25, 0.2],
    ]
}
fn observe(points: &[[f32; 3]], heading: f32, offset: [f32; 3]) -> Vec<RecoveryMarker> {
    let (s, c) = heading.sin_cos();
    points
        .iter()
        .enumerate()
        .map(|(i, p)| RecoveryMarker {
            position: [
                c * p[0] + s * p[2] + offset[0],
                p[1] + offset[1],
                -s * p[0] + c * p[2] + offset[2],
            ],
            mass_kg: [14.0, 14.0, 18.0, 7.0, 2.5, 2.5][i],
        })
        .collect()
}
#[test]
fn recovery_matching_follows_the_settled_pose_at_all_headings_and_scales() {
    for scale in [0.1, 1.0, 3.0] {
        let candidate: Vec<_> = clip().into_iter().map(|p| p.map(|v| v * scale)).collect();
        for degrees in [-179.0_f32, -90.0, 0.0, 45.0, 179.0] {
            let heading = degrees.to_radians();
            for offset in [[2.0, 0.2, -5.0], [20000.0, 20.0, -16000.0]] {
                let result =
                    match_recovery_pose(&observe(&candidate, heading, offset), &candidate).unwrap();
                assert!((result.heading_radians - heading).cos() > 0.999);
                assert!(result.normalized_error < 0.03);
                for axis in 0..3 {
                    assert!((result.translation[axis] - offset[axis]).abs() < 0.01);
                }
            }
        }
    }
}
#[test]
fn front_back_and_side_pose_differences_are_not_hidden_by_heading_alignment() {
    let candidate = clip();
    let observed = observe(&candidate, 0.7, [3.0, 0.0, -5.0]);
    let exact = match_recovery_pose(&observed, &candidate).unwrap();
    for wrong in [
        candidate
            .iter()
            .map(|p| [p[0], -p[1], p[2]])
            .collect::<Vec<_>>(),
        candidate
            .iter()
            .map(|p| [p[1], -p[0], p[2]])
            .collect::<Vec<_>>(),
    ] {
        let mismatch = match_recovery_pose(&observed, &wrong).unwrap();
        assert!(exact.normalized_error < 1e-5);
        assert!(mismatch.normalized_error > 0.1);
    }
}
#[test]
fn marker_order_and_total_mass_do_not_change_a_recovery_choice() {
    let mut candidate = clip();
    let mut observed = observe(&candidate, 1.4, [0.0, 1.0, 0.0]);
    let first = match_recovery_pose(&observed, &candidate).unwrap();
    candidate.reverse();
    observed.reverse();
    for marker in &mut observed {
        marker.mass_kg *= 10.0;
    }
    let second = match_recovery_pose(&observed, &candidate).unwrap();
    assert!((first.heading_radians - second.heading_radians).abs() < 1e-6);
    assert!((first.normalized_error - second.normalized_error).abs() < 1e-6);
}
#[test]
fn invalid_or_underdetermined_pose_data_cannot_choose_a_clip() {
    let candidate = clip();
    let mut observed = observe(&candidate, 0.0, [0.0; 3]);
    assert!(match_recovery_pose(&observed, &candidate[..3]).is_none());
    observed[0].mass_kg = 0.0;
    assert!(match_recovery_pose(&observed, &candidate).is_none());
    observed[0].mass_kg = f32::NAN;
    assert!(match_recovery_pose(&observed, &candidate).is_none());
    observed = observe(&candidate, 0.0, [0.0; 3]);
    observed[1].position[0] = f32::INFINITY;
    assert!(match_recovery_pose(&observed, &candidate).is_none());
    let vertical = vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0]];
    let markers: Vec<_> = vertical
        .iter()
        .map(|p| RecoveryMarker {
            position: *p,
            mass_kg: 1.0,
        })
        .collect();
    assert!(match_recovery_pose(&markers, &vertical).is_none());
}
