//! Match a recovery clip to the measured body, allowing only heading and translation.
//! Pitch, roll and limb differences remain in the score; an impact direction is
//! not a description of the pose in which an articulated body finally settles.
#[derive(Clone, Copy, Debug)]
pub struct RecoveryMarker {
    pub position: [f32; 3],
    pub mass_kg: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct WeightedRecoveryMarker {
    pub position: [f32; 3],
    pub weight: f32,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct RecoveryPoseMatch {
    pub heading_radians: f32,
    pub translation: [f32; 3],
    /// Weighted RMS residual divided by the observed body's RMS span.
    pub normalized_error: f32,
}

/// Marker order must identify the same installed joints in both arrays.
/// Uses f64 accumulation so ordinary far-world positions do not dominate a
/// small character's pose. No pose or velocity is written by this calculation.
pub fn match_recovery_pose(
    observed: &[RecoveryMarker],
    candidate: &[[f32; 3]],
) -> Option<RecoveryPoseMatch> {
    match_weighted_pose(observed.iter().map(|m| (m.position, m.mass_kg)), candidate)
}

fn match_weighted_pose(
    observed: impl ExactSizeIterator<Item = ([f32; 3], f32)> + Clone,
    candidate: &[[f32; 3]],
) -> Option<RecoveryPoseMatch> {
    if observed.len() < 3 || observed.len() != candidate.len() {
        return None;
    }
    let mut mass = 0.0_f64;
    let mut actual_center = [0.0_f64; 3];
    let mut clip_center = [0.0_f64; 3];
    for ((actual, weight), clip) in observed.clone().zip(candidate) {
        if !weight.is_finite() || weight <= 0.0 || !actual.iter().chain(clip).all(|v| v.is_finite())
        {
            return None;
        }
        let weight = f64::from(weight);
        mass += weight;
        for axis in 0..3 {
            actual_center[axis] += f64::from(actual[axis]) * weight;
            clip_center[axis] += f64::from(clip[axis]) * weight;
        }
    }
    for axis in 0..3 {
        actual_center[axis] /= mass;
        clip_center[axis] /= mass;
    }
    let mut dot = 0.0;
    let mut cross = 0.0;
    let mut span = 0.0;
    for ((actual, weight), clip) in observed.clone().zip(candidate) {
        let a: [f64; 3] = std::array::from_fn(|i| f64::from(actual[i]) - actual_center[i]);
        let c: [f64; 3] = std::array::from_fn(|i| f64::from(clip[i]) - clip_center[i]);
        let weight = f64::from(weight);
        dot += weight * (c[0] * a[0] + c[2] * a[2]);
        cross += weight * (c[2] * a[0] - c[0] * a[2]);
        span += weight * a.iter().map(|v| v * v).sum::<f64>();
    }
    if span <= f64::MIN_POSITIVE || dot.hypot(cross) <= f64::MIN_POSITIVE {
        return None;
    }
    let heading = cross.atan2(dot);
    let (sin, cos) = heading.sin_cos();
    let rotate = |v: [f64; 3]| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];
    let center = rotate(clip_center);
    let translation: [f64; 3] = std::array::from_fn(|i| actual_center[i] - center[i]);
    let mut residual = 0.0;
    for ((actual, weight), clip) in observed.zip(candidate) {
        let transformed = rotate(clip.map(f64::from));
        residual += f64::from(weight)
            * (0..3)
                .map(|i| (f64::from(actual[i]) - transformed[i] - translation[i]).powi(2))
                .sum::<f64>();
    }
    let result = RecoveryPoseMatch {
        heading_radians: heading as f32,
        translation: translation.map(|v| v as f32),
        normalized_error: (residual / span).sqrt() as f32,
    };
    let valid = result.translation.iter().all(|v| v.is_finite())
        && result.heading_radians.is_finite()
        && result.normalized_error.is_finite();
    valid.then_some(result)
}

/// Use the reference matcher's 30 Hz default cadence. Until authored match
/// windows are available, constrain admission to the beginning of the clip
/// so a settled body cannot skip most of the get-up.
pub fn recovery_sample_times(duration_seconds: f32) -> impl Iterator<Item = f32> {
    let end = if duration_seconds.is_finite() && duration_seconds > 0.0 {
        (duration_seconds * 0.25).min(0.5)
    } else {
        0.0
    };
    let steps = (end * 30.0).ceil() as u32;
    (0..=steps).map(move |step| (step as f32 / 30.0).min(end))
}

/// Search chronological samples, retaining the earliest sample for equal scores.
pub fn match_recovery_samples<'a>(
    observed: &[RecoveryMarker],
    candidates: impl IntoIterator<Item = &'a [[f32; 3]]>,
) -> Option<(usize, RecoveryPoseMatch)> {
    best_sample(candidates, |points| match_recovery_pose(observed, points))
}

pub fn match_weighted_recovery_samples<'a>(
    observed: &[WeightedRecoveryMarker],
    candidates: impl IntoIterator<Item = &'a [[f32; 3]]>,
) -> Option<(usize, RecoveryPoseMatch)> {
    best_sample(candidates, |points| {
        match_weighted_pose(observed.iter().map(|m| (m.position, m.weight)), points)
    })
}

fn best_sample<'a>(
    candidates: impl IntoIterator<Item = &'a [[f32; 3]]>,
    fit_pose: impl Fn(&[[f32; 3]]) -> Option<RecoveryPoseMatch>,
) -> Option<(usize, RecoveryPoseMatch)> {
    let mut best: Option<(usize, RecoveryPoseMatch)> = None;
    for (index, points) in candidates.into_iter().enumerate() {
        if let Some(fit) = fit_pose(points) {
            if best.is_none_or(|(_, old)| fit.normalized_error < old.normalized_error) {
                best = Some((index, fit));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests;
