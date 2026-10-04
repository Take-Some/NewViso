use std::collections::BTreeSet;

/// Render fragments can contain tens of thousands of duplicate vertices.
/// The provider accepts at most 4096 points per convex hull. For larger
/// fragments, keep support points in 124 directions, including every axis
/// extremum. The visual mesh remains unchanged.
pub(crate) fn fragment_collision_hull(mut points: Vec<[f32; 3]>) -> Vec<[f32; 3]> {
    points.retain(|point| point.iter().all(|v| v.is_finite()));
    points.sort_unstable_by(|a, b| {
        a[0].total_cmp(&b[0])
            .then(a[1].total_cmp(&b[1]))
            .then(a[2].total_cmp(&b[2]))
    });
    points.dedup();
    if points.len() <= 4096 {
        return points;
    }

    let mut support = BTreeSet::new();
    for x in -2..=2 {
        for y in -2..=2 {
            for z in -2..=2 {
                if x == 0 && y == 0 && z == 0 {
                    continue;
                }
                let direction = [x as f32, y as f32, z as f32];
                let dot =
                    |p: &[f32; 3]| p[0] * direction[0] + p[1] * direction[1] + p[2] * direction[2];
                let (index, _) = points
                    .iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| dot(a).total_cmp(&dot(b)))
                    .expect("oversized hull is nonempty");
                support.insert(index);
            }
        }
    }
    support.into_iter().map(|index| points[index]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_fragment_keeps_outer_corners_and_satisfies_provider_limit() {
        let mut points = Vec::new();
        for x in 0..24 {
            for y in 0..24 {
                for z in 0..24 {
                    points.push([x as f32 / 23.0, y as f32 / 23.0, z as f32 / 23.0]);
                }
            }
        }
        points.push([f32::NAN, 0.0, 0.0]);
        let hull = fragment_collision_hull(points);
        assert!((4..=4096).contains(&hull.len()));
        for x in [0.0, 1.0] {
            for y in [0.0, 1.0] {
                for z in [0.0, 1.0] {
                    assert!(hull.contains(&[x, y, z]));
                }
            }
        }
        assert!(hull.iter().flatten().all(|v| v.is_finite()));
    }
}
