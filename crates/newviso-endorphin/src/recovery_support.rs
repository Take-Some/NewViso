//! Recovery roots bound to observed rigid support frames, never velocity integration.
use crate::{math, BodyPose, RecoveryPlacement};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct RecoverySupport {
    pub entity: u64,
    pub local_ground: [f32; 3],
    pub local_normal: [f32; 3],
    local_forward: [f32; 3],
    capsule_height: f32,
    model_offset: [f32; 3],
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RecoverySupportSample {
    pub ground_point: [f32; 3],
    pub normal: [f32; 3],
    pub capsule_center: [f32; 3],
    pub model_origin: [f32; 3],
    pub heading_radians: f32,
    pub velocity: [f32; 3],
}
fn valid(frame: BodyPose) -> bool {
    frame
        .position
        .iter()
        .chain(&frame.rotation)
        .chain(&frame.linear_velocity)
        .chain(&frame.angular_velocity)
        .all(|v| v.is_finite())
        && frame.rotation.iter().map(|v| v * v).sum::<f32>() > f32::EPSILON
}
impl RecoverySupport {
    pub fn bind(
        entity: u64,
        frame: BodyPose,
        placement: RecoveryPlacement,
        normal: [f32; 3],
    ) -> Option<Self> {
        if !valid(frame)
            || !placement
                .capsule_center
                .iter()
                .chain(&placement.model_origin)
                .chain(&[placement.ground, placement.heading_radians])
                .all(|v| v.is_finite())
        {
            return None;
        }
        let normal = math::unit(normal).ok()?;
        let ground = [
            placement.capsule_center[0],
            placement.ground,
            placement.capsule_center[2],
        ];
        let inverse = math::inverse(math::normalize(frame.rotation));
        let heading = placement.heading_radians;
        let result = Self {
            entity,
            local_ground: math::rotate(inverse, math::sub(ground, frame.position)),
            local_normal: math::rotate(inverse, normal),
            local_forward: math::rotate(inverse, [heading.sin(), 0.0, heading.cos()]),
            capsule_height: placement.capsule_center[1] - placement.ground,
            model_offset: math::sub(placement.model_origin, ground),
        };
        if result.capsule_height <= 0.0 {
            return None;
        }
        result.sample(Some(frame), 0.0)?;
        Some(result)
    }
    /// The contact follows all three rotation axes; the actor stays gravity-upright.
    /// Presentation offsets retain their existing world-offset convention.
    pub fn sample(
        &self,
        frame: Option<BodyPose>,
        minimum_cosine: f32,
    ) -> Option<RecoverySupportSample> {
        let frame = frame.filter(|f| valid(*f))?;
        if !minimum_cosine.is_finite() || !(0.0..=1.0).contains(&minimum_cosine) {
            return None;
        }
        let q = math::normalize(frame.rotation);
        let normal = math::unit(math::rotate(q, self.local_normal)).ok()?;
        if normal[1] < minimum_cosine {
            return None;
        }
        let ground = math::add(frame.position, math::rotate(q, self.local_ground));
        let forward = math::rotate(q, self.local_forward);
        if forward[0] * forward[0] + forward[2] * forward[2] <= f32::EPSILON {
            return None;
        }
        let sample = RecoverySupportSample {
            ground_point: ground,
            normal,
            capsule_center: [ground[0], ground[1] + self.capsule_height, ground[2]],
            model_origin: math::add(ground, self.model_offset),
            heading_radians: forward[0].atan2(forward[2]),
            velocity: math::add(
                frame.linear_velocity,
                math::cross(frame.angular_velocity, math::sub(ground, frame.position)),
            ),
        };
        sample
            .ground_point
            .iter()
            .chain(&sample.capsule_center)
            .chain(&sample.model_origin)
            .chain(&sample.velocity)
            .chain(&sample.normal)
            .chain(&[sample.heading_radians])
            .all(|v| v.is_finite())
            .then_some(sample)
    }
}
#[cfg(test)]
mod tests;
