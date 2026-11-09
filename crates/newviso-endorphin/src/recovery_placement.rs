//! Provider-independent admission of a matched recovery pose.
//! Queries stay in the host; no ownership or body transform is changed here.
use crate::RecoveryPoseMatch;

#[derive(Clone, Copy, Debug)]
pub struct RecoveryPlacementInput {
    pub pose_match: RecoveryPoseMatch,
    /// Installed animation-root position in scaled model coordinates.
    pub root_local: [f32; 3],
    pub model_offset: [f32; 3],
    pub floor_offset: f32,
    pub capsule_radius: f32,
    pub capsule_center_offset: f32,
    pub clearance: f32,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct RecoveryPlacement {
    pub model_origin: [f32; 3],
    pub capsule_center: [f32; 3],
    pub root_world: [f32; 3],
    pub ground: f32,
    pub heading_radians: f32,
    /// Zero is the matched position; 1..=8 are the nearby radial positions.
    pub radial_sample: usize,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct RecoveryPlacementReport {
    pub invalid_input: bool,
    pub tested: usize,
    pub missing_ground: usize,
    pub obstructed: usize,
    pub occluded: usize,
    pub selected: Option<RecoveryPlacement>,
}

/// Try the matched position, then a capsule-radius circle in 45-degree steps.
/// Ground, volume and root visibility must all accept before a result is returned.
pub fn find_recovery_placement(
    input: RecoveryPlacementInput,
    mut ground_at: impl FnMut([f32; 3]) -> Option<f32>,
    mut volume_clear: impl FnMut(&RecoveryPlacement) -> bool,
    mut root_path_clear: impl FnMut([f32; 3]) -> bool,
) -> RecoveryPlacementReport {
    let mut report = RecoveryPlacementReport::default();
    if !input
        .pose_match
        .translation
        .iter()
        .chain(&input.root_local)
        .chain(&input.model_offset)
        .all(|v| v.is_finite())
        || ![
            input.pose_match.heading_radians,
            input.pose_match.normalized_error,
            input.floor_offset,
            input.capsule_radius,
            input.capsule_center_offset,
            input.clearance,
        ]
        .iter()
        .all(|v| v.is_finite())
        || input.pose_match.normalized_error < 0.0
        || input.capsule_radius <= 0.0
        || input.capsule_center_offset < input.capsule_radius
        || input.clearance < 0.0
    {
        report.invalid_input = true;
        return report;
    }
    let (sin, cos) = input.pose_match.heading_radians.sin_cos();
    for sample in 0..=8 {
        let mut origin = input.pose_match.translation;
        if sample != 0 {
            let angle = (sample - 1) as f32 * std::f32::consts::FRAC_PI_4;
            origin[0] -= input.capsule_radius * angle.sin();
            origin[2] += input.capsule_radius * angle.cos();
        }
        report.tested += 1;
        let probe = [
            origin[0] - input.model_offset[0],
            origin[1],
            origin[2] - input.model_offset[2],
        ];
        let Some(ground) = ground_at(probe).filter(|v| v.is_finite()) else {
            report.missing_ground += 1;
            continue;
        };
        // Preserve authored presentation lift and keep the sampled contact
        // volumes above the measured floor. Model origin is not a body COM.
        origin[1] =
            (ground + input.model_offset[1]).max(ground - input.floor_offset + input.clearance);
        let root = input.root_local;
        let placement = RecoveryPlacement {
            model_origin: origin,
            capsule_center: [
                probe[0],
                ground + input.capsule_center_offset + input.clearance,
                probe[2],
            ],
            root_world: [
                origin[0] + cos * root[0] + sin * root[2],
                origin[1] + root[1],
                origin[2] - sin * root[0] + cos * root[2],
            ],
            ground,
            heading_radians: input.pose_match.heading_radians,
            radial_sample: sample,
        };
        if !placement
            .model_origin
            .iter()
            .chain(&placement.capsule_center)
            .chain(&placement.root_world)
            .all(|v| v.is_finite())
        {
            report.missing_ground += 1;
            continue;
        }
        if !volume_clear(&placement) {
            report.obstructed += 1;
            continue;
        }
        if !root_path_clear(placement.root_world) {
            report.occluded += 1;
            continue;
        }
        report.selected = Some(placement);
        break;
    }
    report
}

#[cfg(test)]
mod tests;
