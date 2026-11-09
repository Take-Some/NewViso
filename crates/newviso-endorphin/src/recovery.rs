use crate::{math, RecoveryModel};
use serde::Serialize;

#[cfg(test)]
mod high_fall_tests;

#[derive(Clone, Copy, Debug)]
pub struct Support {
    pub normal: [f32; 3],
    /// Velocity of the supporting surface at the contact point.
    pub velocity: [f32; 3],
}
#[derive(Clone, Copy, Debug)]
pub struct BodyObservation {
    pub rotation: [f32; 4],
    pub linear_velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub radius: f32,
    pub half_height: f32,
    pub gravity: f32,
    /// Continuous remaining physical capacity, independent of HP thresholds.
    pub capacity: f32,
    /// Exact articulated energy when a full physical skeleton is available.
    pub motion_energy_per_mass: Option<f32>,
    pub support: Option<Support>,
    pub space_clear: bool,
    pub down: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct RecoveryState {
    pub confidence: f32,
    pub stability: f32,
    pub relative_energy_ratio: f32,
    pub support_available: bool,
    pub space_clear: bool,
    pub ready: bool,
    pub falling: bool,
    pub tipped: bool,
    pub hard_landing: bool,
    pub high_fall: bool,
    pub airborne_distance: f32,
    pub peak_descent_speed: f32,
    #[serde(skip)]
    was_airborne: bool,
}
impl RecoveryState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    /// Exact exponential evidence integration: no per-frame decay constants.
    /// No world contacts or clearance is inferred from a timer.
    pub fn observe(&mut self, model: &RecoveryModel, body: BodyObservation, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.ready = false;
        self.hard_landing = false;
        self.high_fall = false;
        self.space_clear = body.space_clear;
        let finite = body
            .rotation
            .iter()
            .chain(&body.linear_velocity)
            .chain(&body.angular_velocity)
            .all(|v| v.is_finite())
            && body.radius.is_finite()
            && body.radius > 0.0
            && body.half_height.is_finite()
            && body.half_height >= 0.0
            && body.gravity.is_finite()
            && body.gravity > 0.0
            && body.capacity.is_finite();
        if !finite {
            self.reset();
            return;
        }
        let height = 2.0 * (body.radius + body.half_height);
        let capacity = body.capacity.clamp(model.capacity_floor, 1.0);
        let support = body.support.filter(|s| {
            s.normal.iter().chain(&s.velocity).all(|v| v.is_finite())
                && math::length(s.normal) > f32::EPSILON
        });
        self.support_available = support
            .is_some_and(|s| s.normal[1] / math::length(s.normal) >= model.minimum_support_cosine);
        let absorbed_height = height * model.landing_absorption_height_fraction * capacity;
        if !self.support_available {
            if !self.was_airborne {
                self.airborne_distance = 0.0;
                self.peak_descent_speed = 0.0;
            }
            self.peak_descent_speed = self.peak_descent_speed.max(-body.linear_velocity[1]);
            self.airborne_distance += (-body.linear_velocity[1]).max(0.0) * dt;
            self.was_airborne = true;
        } else if self.was_airborne {
            self.hard_landing =
                self.peak_descent_speed.powi(2) > 2.0 * body.gravity * absorbed_height;
            self.was_airborne = false;
        }
        self.falling = !self.support_available
            && self.airborne_distance > body.radius * model.airborne_clearance_fraction;
        self.high_fall =
            self.falling && self.peak_descent_speed.powi(2) > 2.0 * body.gravity * absorbed_height;
        let axis = math::rotate(body.rotation, [0.0, 1.0, 0.0]);
        let normal = support.map_or([0.0, 1.0, 0.0], |s| {
            s.normal.map(|v| v / math::length(s.normal))
        });
        let along = math::dot(axis, normal).abs().clamp(0.0, 1.0);
        let projected_com = (body.radius + body.half_height) * (1.0 - along * along).sqrt();
        self.tipped = self.support_available
            && projected_com > body.radius * model.balance_reach * capacity.sqrt();
        let velocity = math::sub(
            body.linear_velocity,
            support.map_or([0.0; 3], |s| s.velocity),
        );
        // Rotational energy expressed as equivalent COM motion. Mass cancels
        // between kinetic energy and gravitational support work.
        let gyration_squared = (height * height + 3.0 * body.radius * body.radius) / 12.0;
        let capsule_energy = 0.5
            * (math::dot(velocity, velocity)
                + gyration_squared * math::dot(body.angular_velocity, body.angular_velocity));
        let kinetic_per_mass = body
            .motion_energy_per_mass
            .filter(|e| e.is_finite() && *e >= 0.0)
            .unwrap_or(capsule_energy);
        let budget = body.gravity * body.radius * model.settling_energy_fraction * capacity;
        self.relative_energy_ratio = kinetic_per_mass / budget;
        let alignment = support.map_or(0.0, |s| {
            ((s.normal[1] / math::length(s.normal) - model.minimum_support_cosine)
                / (1.0 - model.minimum_support_cosine))
                .clamp(0.0, 1.0)
        });
        self.stability = if self.support_available && body.space_clear {
            alignment * (-self.relative_energy_ratio).exp()
        } else {
            0.0
        };
        if !body.down || !self.support_available || !body.space_clear {
            // Losing the support/clearance invalidates accumulated evidence.
            self.confidence = 0.0;
        } else {
            let response_seconds =
                (height / body.gravity).sqrt() * model.response_time_scale / capacity.sqrt();
            self.confidence +=
                (self.stability - self.confidence) * (1.0 - (-dt / response_seconds).exp());
        }
        self.ready = body.down && self.confidence >= model.ready_confidence;
    }
}
