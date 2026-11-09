//! Capture-point feedback over the convex support area of resident foot contacts.
use crate::math;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BalanceModel {
    #[serde(default)]
    pub stepping: Option<crate::SteppingModel>,
    pub support_parts: Vec<String>,
    pub joint_weights: BTreeMap<String, f32>,
    pub footprint_padding: f32,
    pub max_lean_angle: f32,
    pub recovery_upright_cosine: f32,
    #[serde(default)]
    pub gains: BalanceGains,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BalanceGains {
    #[serde(default = "unit_gain")]
    pub height: f32,
    #[serde(default = "unit_gain")]
    pub plant_position: f32,
    pub posture: f32,
    pub capture: f32,
    pub damping: f32,
    pub gravity_compensation: f32,
}
fn unit_gain() -> f32 {
    1.0
}
impl Default for BalanceGains {
    fn default() -> Self {
        Self {
            height: 1.0,
            plant_position: 1.0,
            posture: 1.0,
            capture: 1.0,
            damping: 1.0,
            gravity_compensation: 1.0,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct BalanceState {
    pub supported: bool,
    pub support_normal: [f32; 3],
    pub body_up: [f32; 3],
    pub uprightness: f32,
    pub support_margin: f32,
    pub capture_point: [f32; 3],
    pub support_center: [f32; 3],
    pub correction_axis: [f32; 3],
}
fn cross(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
#[derive(Clone, Debug, Default)]
pub(crate) struct BalanceScratch {
    points: Vec<[f32; 2]>,
    hull: Vec<[f32; 2]>,
}
pub fn measure_balance(
    contacts: &[[f32; 3]],
    com: [f32; 3],
    velocity: [f32; 3],
    surface_velocity: [f32; 3],
    up: [f32; 3],
    gravity: f32,
    padding: f32,
    max_lean: f32,
) -> BalanceState {
    measure_balance_reusing(
        contacts,
        com,
        velocity,
        surface_velocity,
        up,
        gravity,
        padding,
        max_lean,
        &mut BalanceScratch::default(),
    )
}

pub(crate) fn measure_balance_reusing(
    contacts: &[[f32; 3]],
    com: [f32; 3],
    velocity: [f32; 3],
    surface_velocity: [f32; 3],
    up: [f32; 3],
    gravity: f32,
    padding: f32,
    max_lean: f32,
    scratch: &mut BalanceScratch,
) -> BalanceState {
    let mut result = BalanceState::default();
    if up.iter().all(|v| v.is_finite()) {
        result.body_up = up.map(|v| v / math::length(up).max(f32::EPSILON));
        result.uprightness = result.body_up[1].clamp(0.0, 1.0);
    }
    if contacts.is_empty()
        || ![com, velocity, surface_velocity, up]
            .iter()
            .flatten()
            .all(|v| v.is_finite())
        || !contacts.iter().flatten().all(|v| v.is_finite())
        || !gravity.is_finite()
        || gravity <= 0.0
    {
        return result;
    }
    let count = contacts.len() as f32;
    let mut center = std::array::from_fn(|i| contacts.iter().map(|p| p[i]).sum::<f32>() / count);
    let height = com[1] - center[1];
    if height <= f32::EPSILON {
        return result;
    }
    let time = (height / gravity).sqrt();
    let capture = math::add(com, math::sub(velocity, surface_velocity).map(|v| v * time));
    let BalanceScratch { points, hull } = scratch;
    points.clear();
    points.extend(contacts.iter().map(|p| [p[0], p[2]]));
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    hull.clear();
    for &p in points.iter() {
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(p);
    }
    let lower = hull.len();
    for &p in points.iter().rev().skip(1) {
        while hull.len() > lower && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(p);
    }
    if hull.len() > 1 {
        hull.pop();
    }
    let p = [capture[0], capture[2]];
    let margin = if hull.len() >= 3 {
        (0..hull.len())
            .map(|i| {
                let a = hull[i];
                let b = hull[(i + 1) % hull.len()];
                cross(a, b, p) / ((b[0] - a[0]).hypot(b[1] - a[1])).max(f32::EPSILON)
            })
            .fold(f32::INFINITY, f32::min)
            + padding
    } else {
        let a = points[0];
        let b = *points.last().unwrap();
        let dx = b[0] - a[0];
        let dz = b[1] - a[1];
        let square = dx * dx + dz * dz;
        let t = if square > f32::EPSILON {
            (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / square).clamp(0.0, 1.0)
        } else {
            0.0
        };
        padding - (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dz)
    };
    center[0] = hull.iter().map(|p| p[0]).sum::<f32>() / hull.len() as f32;
    center[2] = hull.iter().map(|p| p[1]).sum::<f32>() / hull.len() as f32;
    let mut nearest = center;
    let mut nearest_distance = f32::INFINITY;
    for i in 0..hull.len() {
        let a = hull[i];
        let b = hull[(i + 1) % hull.len()];
        let dx = b[0] - a[0];
        let dz = b[1] - a[1];
        let square = dx * dx + dz * dz;
        let t = if square > f32::EPSILON {
            (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / square).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let x = a[0] + t * dx;
        let z = a[1] + t * dz;
        let d = (p[0] - x).hypot(p[1] - z);
        if d < nearest_distance {
            nearest_distance = d;
            nearest = [x, center[1], z];
        }
    }
    let offset = if margin >= 0.0 {
        [0.0; 3]
    } else {
        let delta = [capture[0] - nearest[0], 0.0, capture[2] - nearest[2]];
        let length = math::length(delta);
        let factor = (length - padding).max(0.0) / length.max(f32::EPSILON);
        delta.map(|v| v * factor)
    };
    let distance = math::length(offset);
    let angle = (distance / height).atan().min(max_lean);
    let correction = if distance > f32::EPSILON {
        math::cross([0.0, 1.0, 0.0], offset).map(|v| v * angle / distance)
    } else {
        [0.0; 3]
    };
    result.body_up = up.map(|v| v / math::length(up).max(f32::EPSILON));
    result.supported = true;
    result.uprightness = (up[1] / math::length(up).max(f32::EPSILON)).clamp(0.0, 1.0);
    result.support_margin = margin;
    result.capture_point = capture;
    result.support_center = center;
    result.correction_axis = correction;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_point_detects_velocity_outside_support_and_tracks_moving_floor() {
        let contacts = [
            [-0.15, 0.0, -0.15],
            [0.15, 0.0, -0.15],
            [0.15, 0.0, 0.15],
            [-0.15, 0.0, 0.15],
        ];
        let measure = |v, s| {
            measure_balance(
                &contacts,
                [0.0, 1.0, 0.0],
                v,
                s,
                [0.0, 1.0, 0.0],
                9.81,
                0.04,
                0.35,
            )
        };
        let still = measure([0.0; 3], [0.0; 3]);
        assert!(
            still.supported
                && still.support_margin > 0.15
                && math::length(still.correction_axis) < 1e-6
        );
        let moving = measure([2.0, 0.0, 0.0], [0.0; 3]);
        assert!(moving.support_margin < 0.0 && moving.correction_axis[2] < 0.0);
        assert!((math::length(moving.correction_axis) - 0.35).abs() < 1e-5);
        let platform = measure([2.0, 0.0, 0.0], [2.0, 0.0, 0.0]);
        assert!((platform.support_margin - still.support_margin).abs() < 1e-5);
    }
    #[test]
    fn contact_order_duplicates_and_collinear_feet_do_not_break_support() {
        let a = [[-0.1, 0.0, 0.0], [0.1, 0.0, 0.0], [0.1, 0.0, 0.0]];
        let b = [[0.1, 0.0, 0.0], [-0.1, 0.0, 0.0]];
        for c in [&a[..], &b[..]] {
            let s = measure_balance(
                c,
                [0.0, 1.0, 0.0],
                [0.0; 3],
                [0.0; 3],
                [0.0, 1.0, 0.0],
                9.81,
                0.04,
                0.35,
            );
            assert!(s.supported && (s.support_margin - 0.04).abs() < 1e-6);
        }
        assert!(
            !measure_balance(
                &[],
                [0.0; 3],
                [0.0; 3],
                [0.0; 3],
                [0.0; 3],
                9.81,
                0.04,
                0.35
            )
            .supported
        );
    }
}
