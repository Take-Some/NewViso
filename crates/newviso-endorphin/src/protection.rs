//! Predictive, provider-independent protective reaching. Outputs are intent;
//! animated IK or bounded physical muscles decide the achieved motion.
use crate::math;
use serde::Serialize;
mod model;
pub use model::*;
mod rig;
#[derive(Clone, Debug)]
pub struct ProtectiveArmObservation {
    pub shoulder: [f32; 3],
    pub elbow: [f32; 3],
    pub wrist: [f32; 3],
    pub palm_offset_local: [f32; 3],
    pub capacity: f32,
}
impl ProtectiveArmObservation {
    pub fn reach(&self) -> f32 {
        math::length(math::sub(self.elbow, self.shoulder))
            + math::length(math::sub(self.wrist, self.elbow))
    }
}
#[derive(Clone, Debug)]
pub struct ProtectionObservation {
    pub head: [f32; 3],
    pub up: [f32; 3],
    pub forward: [f32; 3],
    pub gravity: f32,
    pub arms: Vec<ProtectiveArmObservation>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ImpactThreat {
    pub entity: Option<u64>,
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub surface_velocity: [f32; 3],
    pub closing_speed: f32,
    pub time_to_impact: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct ProtectiveArmGoal {
    pub arm: usize,
    pub wrist: [f32; 3],
    pub palm: [f32; 3],
    pub pole: [f32; 3],
    pub hand_rotation: [f32; 4],
    pub palm_normal: [f32; 3],
    pub weight: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct ProtectionFrame {
    pub up: [f32; 3],
    pub forward: [f32; 3],
    pub shoulders: Vec<[f32; 3]>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct ProtectionState {
    pub phase: String,
    pub activation: f32,
    pub updates: u64,
    /// The observation frame which produced these world-space goals.
    pub frame: Option<ProtectionFrame>,
    pub threat: Option<ImpactThreat>,
    pub arms: Vec<ProtectiveArmGoal>,
}
pub(super) fn palm_rotation(normal: [f32; 3], up: [f32; 3]) -> Option<[f32; 4]> {
    let z = math::unit(normal).ok()?;
    let y = math::unit(math::sub(up, z.map(|v| v * math::dot(z, up)))).ok()?;
    let x = math::cross(y, z);
    Some(math::rotation([
        x[0], x[1], x[2], 0.0, y[0], y[1], y[2], 0.0, z[0], z[1], z[2], 0.0, 0.0, 0.0, 0.0, 1.0,
    ]))
}
/// Humanoid profiles bind their anatomical right arm before their left arm.
/// Up cross right points forwards for the runtime's right-handed frame.
pub fn protection_forward(
    up: [f32; 3],
    right_shoulder: [f32; 3],
    left_shoulder: [f32; 3],
) -> Option<[f32; 3]> {
    math::unit(math::cross(up, math::sub(right_shoulder, left_shoulder))).ok()
}
impl ImpactThreat {
    pub fn valid(&self) -> bool {
        self.point
            .iter()
            .chain(&self.normal)
            .chain(&self.surface_velocity)
            .all(|v| v.is_finite())
            && (math::length(self.normal) - 1.0).abs() < 0.001
            && self.closing_speed.is_finite()
            && self.closing_speed > 0.0
            && self.time_to_impact.is_finite()
            && self.time_to_impact >= 0.0
    }
}
impl ProtectionState {
    pub fn active(&self) -> bool {
        self.arms.iter().any(|a| a.weight > f32::EPSILON)
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn observe(
        &mut self,
        profile: &ProtectionProfile,
        body: &ProtectionObservation,
        incoming: Option<ImpactThreat>,
        dt: f32,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        if body.arms.len() != profile.arms.len()
            || !body.gravity.is_finite()
            || body.gravity < 0.0
            || !body
                .head
                .iter()
                .chain(&body.up)
                .chain(&body.forward)
                .all(|v| v.is_finite())
            || body.arms.iter().any(|a| {
                !a.shoulder
                    .iter()
                    .chain(&a.elbow)
                    .chain(&a.wrist)
                    .chain(&a.palm_offset_local)
                    .all(|v| v.is_finite())
                    || !a.capacity.is_finite()
                    || a.reach() <= f32::EPSILON
            })
        {
            self.reset();
            return;
        }
        let Some((up, forward)) = math::unit(body.up).ok().zip(math::unit(body.forward).ok())
        else {
            self.reset();
            return;
        };
        // Keep the small shoulder buffer resident across observations.
        let frame = self.frame.get_or_insert_with(|| ProtectionFrame {
            up,
            forward,
            shoulders: Vec::with_capacity(body.arms.len()),
        });
        frame.up = up;
        frame.forward = forward;
        frame.shoulders.clear();
        frame.shoulders.extend(body.arms.iter().map(|a| a.shoulder));
        let reach = body.arms.iter().map(|a| a.reach()).sum::<f32>() / body.arms.len() as f32;
        let incoming = incoming.filter(ImpactThreat::valid);
        let speed = incoming.as_ref().map_or(0.0, |t| t.closing_speed);
        let horizon = profile.tuning.horizon(reach, body.gravity, speed);
        let target = incoming.as_ref().map_or(0.0, |t| {
            (-t.time_to_impact / horizon.max(f32::EPSILON)).exp() * t.closing_speed
                / (t.closing_speed
                    + (body.gravity * reach).sqrt() * profile.tuning.impact_speed_fraction)
        });
        let time = horizon / profile.tuning.horizon_scale
            * if incoming.is_some() {
                profile.tuning.raise_time_scale
            } else {
                profile.tuning.release_time_scale
            };
        let change = (target - self.activation)
            .clamp(-dt / time.max(f32::EPSILON), dt / time.max(f32::EPSILON));
        self.activation = (self.activation + change).clamp(0.0, 1.0);
        self.updates += 1;
        if let Some(threat) = incoming {
            self.threat = Some(threat);
            self.phase = "protect".into();
        } else {
            // Follow the last observed surface while protective effort fades.
            // Fresh observations already contain the current contact point.
            if let Some(threat) = self.threat.as_mut() {
                threat.point = math::add(threat.point, threat.surface_velocity.map(|v| v * dt));
                threat.time_to_impact = (threat.time_to_impact - dt).max(0.0);
            }
            self.phase = "release".into();
        }
        self.arms.clear();
        if self.activation <= f32::EPSILON {
            self.activation = 0.0;
            self.threat = None;
            self.phase = "idle".into();
            return;
        }
        let Some(threat) = self.threat.as_ref() else {
            return;
        };
        let normal = threat.normal;
        let center = body
            .arms
            .iter()
            .fold([0.0; 3], |sum, a| math::add(sum, a.shoulder))
            .map(|v| v / body.arms.len() as f32);
        for (i, (arm, binding)) in body.arms.iter().zip(&profile.arms).enumerate() {
            let length = arm.reach();
            let clearance = length * profile.tuning.hand_radius_fraction;
            let Some(local) = palm_rotation(binding.palm_normal_local, binding.palm_up_local)
            else {
                continue;
            };
            let side =
                math::unit(math::sub(arm.shoulder, center)).unwrap_or(math::cross(up, forward));
            let projection = math::dot(math::sub(arm.shoulder, threat.point), normal);
            let mut palm = math::add(
                math::sub(arm.shoulder, normal.map(|v| v * projection)),
                normal.map(|v| v * clearance),
            );
            // A rear impact protects the head without chasing a wall through the back.
            let guarding_head = math::dot(math::sub(palm, arm.shoulder), forward) < 0.0;
            if guarding_head {
                palm = math::add(
                    body.head,
                    math::add(
                        math::sub(arm.shoulder, center),
                        forward.map(|v| v * clearance),
                    ),
                );
            }
            // Head guards face outwards beside the temples, avoiding a rear-facing
            // palm target which would twist a front-bound wrist through the torso.
            let palm_normal = if guarding_head {
                side
            } else {
                normal.map(|v| -v)
            };
            let hand_up = if math::length(math::cross(up, palm_normal)) > f32::EPSILON {
                up
            } else {
                forward
            };
            let Some(world) = palm_rotation(palm_normal, hand_up) else {
                continue;
            };
            let rotation = math::multiply(world, math::inverse(local));
            let offset = math::rotate(rotation, arm.palm_offset_local);
            let mut delta = math::sub(math::sub(palm, offset), arm.shoulder);
            let front = math::dot(delta, forward);
            delta = math::add(
                delta,
                forward.map(|v| {
                    v * (length * profile.tuning.forward_clearance_fraction - front).max(0.0)
                }),
            );
            let distance = math::length(delta);
            let limit = length * profile.tuning.reach_fraction;
            let wrist = math::add(
                arm.shoulder,
                delta.map(|v| v * (limit / distance.max(limit))),
            );
            palm = math::add(wrist, offset);
            let pole = math::add(
                arm.shoulder,
                math::sub(
                    side.map(|v| v * length * profile.tuning.elbow_outward_fraction),
                    up.map(|v| v * length * profile.tuning.elbow_drop_fraction),
                ),
            );
            self.arms.push(ProtectiveArmGoal {
                arm: i,
                wrist,
                palm,
                pole,
                hand_rotation: rotation,
                palm_normal,
                weight: self.activation * arm.capacity.clamp(0.0, 1.0).sqrt(),
            });
        }
    }
}
#[cfg(test)]
mod tests;
