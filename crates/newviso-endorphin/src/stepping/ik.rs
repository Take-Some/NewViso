//! Length-preserving leg geometry, independent of terrain and control state.
use crate::math;
/// Analytic triangle solution; reach is clamped without changing segment lengths.
pub fn solve_two_link(
    hip: [f32; 3],
    target: [f32; 3],
    bend: [f32; 3],
    a: f32,
    b: f32,
    reach: f32,
) -> Option<([f32; 3], [f32; 3])> {
    if a <= 0.0 || b <= 0.0 || !reach.is_finite() || reach <= 0.0 || reach > 1.0 {
        return None;
    }
    let delta = math::sub(target, hip);
    let axis = math::unit(delta).ok()?;
    let distance = math::length(delta).clamp((a - b).abs() + f32::EPSILON, (a + b) * reach);
    let along = (a * a - b * b + distance * distance) / (2.0 * distance);
    let radial = (a * a - along * along).max(0.0).sqrt();
    let perpendicular =
        math::unit(math::sub(bend, axis.map(|v| v * math::dot(bend, axis)))).ok()?;
    let knee = math::add(
        hip,
        math::add(axis.map(|v| v * along), perpendicular.map(|v| v * radial)),
    );
    Some((knee, math::add(hip, axis.map(|v| v * distance))))
}
/// Match a segment and its hinge plane together, without introducing axial twist.
pub(super) fn segment_frame(
    from: [f32; 3],
    from_hinge: [f32; 3],
    to: [f32; 3],
    to_hinge: [f32; 3],
) -> [f32; 4] {
    let swing = math::align_axis(from, to);
    let Some(axis) = math::unit(to).ok() else {
        return swing;
    };
    let old = math::rotate(swing, from_hinge);
    let project = |p: [f32; 3]| math::unit(math::sub(p, axis.map(|v| v * math::dot(p, axis)))).ok();
    let Some((old, new)) = project(old).zip(project(to_hinge)) else {
        return swing;
    };
    let angle = math::dot(axis, math::cross(old, new)).atan2(math::dot(old, new));
    let twist = [
        axis[0] * (angle * 0.5).sin(),
        axis[1] * (angle * 0.5).sin(),
        axis[2] * (angle * 0.5).sin(),
        (angle * 0.5).cos(),
    ];
    math::multiply(twist, swing)
}
