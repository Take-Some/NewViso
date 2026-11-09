//! Contact-aware terrain correction. Authored gait supplies cadence and swing motion.
use crate::Vec3;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FootMotionProfile {
    pub contact_height_ratio: f32,
    pub lift_off_speed_ratio: f32,
    pub stance_speed_ratio: f32,
    pub transition_seconds: f32,
    pub correction_seconds: f32,
    pub anticipation_seconds: f32,
    pub pelvis_drop_ratio: f32,
    pub speed_ratio: f32,
    pub acceleration_ratio: f32,
    pub plant_tolerance_ratio: f32,
    pub anchor_engage_weight: f32,
    pub anchor_release_weight: f32,
    pub anchor_reach_ratio: f32,
    pub anchor_turn_degrees: f32,
    pub roll_compliance_degrees: f32,
}
impl FootMotionProfile {
    pub fn validate(self) -> Result<(), String> {
        if [
            self.contact_height_ratio,
            self.lift_off_speed_ratio,
            self.stance_speed_ratio,
            self.transition_seconds,
            self.correction_seconds,
            self.anticipation_seconds,
            self.pelvis_drop_ratio,
            self.speed_ratio,
            self.acceleration_ratio,
            self.plant_tolerance_ratio,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.0)
            || !(0.0..1.0).contains(&self.pelvis_drop_ratio)
            || !(0.001..=1.0).contains(&self.transition_seconds)
            || !(0.001..=1.0).contains(&self.correction_seconds)
            || !(0.0..1.0).contains(&self.anchor_release_weight)
            || !(self.anchor_release_weight..=1.0).contains(&self.anchor_engage_weight)
            || self.anchor_engage_weight == self.anchor_release_weight
            || !(0.0..1.0).contains(&self.anchor_reach_ratio)
            || !(1.0..=179.0).contains(&self.anchor_turn_degrees)
            || !(0.1..=45.0).contains(&self.roll_compliance_degrees)
        {
            return Err("invalid foot motion profile".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub struct FootMotionInput {
    pub authored: Vec3,
    /// Lowest authored sole point relative to the model origin, excluding capsule motion.
    pub relative_height: f32,
    pub root: Vec3,
    pub root_velocity: Vec3,
    pub sole_length: f32,
    pub leg_length: f32,
    pub skin: f32,
    pub gravity: f32,
    pub dt: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct FootMotionOutput {
    pub target: Vec3,
    pub correction_velocity: Vec3,
    pub contact: f32,
    pub landing_weight: f32,
    pub demand: f32,
}
#[derive(Clone, Debug, Default)]
pub struct FootMotionState {
    previous: Option<(Vec3, f32, Vec3)>,
    offset: Vec3,
    velocity: Vec3,
    collision_offset: Vec3,
    contact: f32,
    contact_velocity: f32,
    landing: f32,
    landing_velocity: f32,
    reference_height: Option<f32>,
}
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
fn length(v: Vec3) -> f32 {
    v.iter().map(|v| v * v).sum::<f32>().sqrt()
}
fn limited(v: Vec3, max: f32) -> Vec3 {
    let n = length(v);
    if n > max && n > f32::EPSILON {
        v.map(|x| x * max / n)
    } else {
        v
    }
}
impl FootMotionState {
    pub fn estimate(&self, authored: Vec3) -> Vec3 {
        std::array::from_fn(|k| {
            authored[k]
                + self.offset[k] * if k == 1 { 1.0 } else { self.landing }
                + self.collision_offset[k]
        })
    }
    pub fn project_horizontal(&mut self, delta: Vec3) {
        for k in [0, 2] {
            self.collision_offset[k] += delta[k];
        }
    }
    pub fn project_clearance(&mut self, delta: f32) {
        self.offset[1] += delta;
        if (delta > 0.0 && self.velocity[1] < 0.0) || (delta < 0.0 && self.velocity[1] > 0.0) {
            self.velocity[1] = 0.0;
        }
    }
    pub fn constraint_response(&self) -> Vec3 {
        self.collision_offset
    }
    pub fn contact(&self) -> f32 {
        self.contact
    }
    pub fn authored_velocity(&self, i: FootMotionInput) -> Vec3 {
        self.previous
            .filter(|_| i.dt > 0.0)
            .map_or(i.root_velocity, |(p, _, root)| {
                std::array::from_fn(|k| {
                    ((i.authored[k] - i.root[k]) - (p[k] - root[k])) / i.dt + i.root_velocity[k]
                })
            })
    }
    pub fn lift(&self, i: FootMotionInput) -> f32 {
        (i.relative_height - self.reference_height.unwrap_or(i.relative_height)).max(0.0)
    }
    pub fn prediction_seconds(&self, i: FootMotionInput, p: FootMotionProfile) -> f32 {
        let height =
            (i.relative_height - self.reference_height.unwrap_or(i.relative_height)).max(0.0);
        let vertical = self
            .previous
            .filter(|_| i.dt > 0.0)
            .map_or(0.0, |(_, h, _)| (i.relative_height - h) / i.dt);
        if vertical < 0.0 {
            (height / (-vertical)).min(p.anticipation_seconds)
        } else {
            p.anticipation_seconds
        }
    }
    pub fn landing_demand(&self, i: FootMotionInput, p: FootMotionProfile) -> f32 {
        let band = (p.contact_height_ratio * i.sole_length).max(f32::EPSILON);
        let rising = self
            .previous
            .filter(|_| i.dt > 0.0)
            .map_or(0.0, |(_, h, _)| ((i.relative_height - h) / i.dt).max(0.0));
        let speed = (i.gravity.abs() * i.leg_length).sqrt() * p.lift_off_speed_ratio;
        (1.0 - smooth((self.lift(i) - i.skin) / band))
            * (1.0 - smooth(rising / speed.max(f32::EPSILON)))
    }
    pub fn demand(
        &self,
        i: FootMotionInput,
        p: FootMotionProfile,
        has_surface: bool,
        held: bool,
    ) -> f32 {
        if !has_surface {
            return 0.0;
        }
        let velocity = self.authored_velocity(i);
        let horizontal = (velocity[0] * velocity[0] + velocity[2] * velocity[2]).sqrt();
        let speed = (i.gravity.abs() * i.leg_length).sqrt() * p.stance_speed_ratio;
        self.landing_demand(i, p)
            * if held {
                1.0
            } else {
                1.0 - smooth(horizontal / speed.max(f32::EPSILON))
            }
    }
    /// Filter correction, not the animation itself. A held contact cancels only
    /// its weighted authored displacement; swing retains the source trajectory.
    pub fn update(
        &mut self,
        i: FootMotionInput,
        p: FootMotionProfile,
        goal: Option<Vec3>,
        held: bool,
        support_delta: Vec3,
    ) -> FootMotionOutput {
        let demand = self.demand(i, p, goal.is_some(), held);
        let landing = if goal.is_some() {
            self.landing_demand(i, p)
        } else {
            0.0
        };
        let dt = i.dt.max(0.0);
        let decay = (-dt / p.correction_seconds).exp();
        for k in [0, 2] {
            self.collision_offset[k] *= decay;
        }
        if self.previous.is_none() {
            self.contact = demand;
            self.landing = landing;
            self.offset = goal.map_or([0.0; 3], |g| {
                std::array::from_fn(|k| (g[k] - i.authored[k]) * if k == 1 { 1.0 } else { landing })
            });
        } else if dt > 0.0 {
            spring_unit(
                &mut self.contact,
                &mut self.contact_velocity,
                demand,
                p.transition_seconds,
                dt,
            );
            spring_unit(
                &mut self.landing,
                &mut self.landing_velocity,
                landing,
                p.transition_seconds,
                dt,
            );
        }
        if let Some((previous, _, _)) = self.previous.filter(|_| held) {
            for k in 0..3 {
                let delta = (support_delta[k] - (i.authored[k] - previous[k])) * self.contact;
                self.offset[k] += delta;
            }
        }
        if !held {
            if let Some((_, _, root)) = self.previous {
                let delta = (i.root[1] - root[1]) * self.contact;
                self.offset[1] -= delta;
            }
        }
        // Acquire a landing correction during swing; contact masks its visible influence.
        let wanted = goal.map_or([0.0; 3], |g| std::array::from_fn(|k| g[k] - i.authored[k]));
        let omega = 4.0 / p.correction_seconds;
        let max_speed = p.speed_ratio * (i.gravity.abs() * i.leg_length).sqrt();
        let max_acceleration = p.acceleration_ratio * i.gravity.abs();
        let mut remaining = dt;
        while remaining > 1e-7 {
            let h = remaining.min(p.correction_seconds * 0.125);
            let a = limited(
                std::array::from_fn(|k| {
                    omega * omega * (wanted[k] - self.offset[k]) - 2.0 * omega * self.velocity[k]
                }),
                max_acceleration,
            );
            self.velocity = limited(
                std::array::from_fn(|k| self.velocity[k] + a[k] * h),
                max_speed,
            );
            for k in 0..3 {
                self.offset[k] += self.velocity[k] * h;
            }
            remaining -= h;
        }
        let velocity = self.authored_velocity(i);
        let natural_speed = (i.gravity.abs() * i.leg_length).sqrt();
        let reference = self.reference_height.get_or_insert(i.relative_height);
        if i.relative_height < *reference {
            *reference = i.relative_height;
        } else if held
            && i.relative_height - *reference <= i.skin
            && (velocity[0] * velocity[0] + velocity[2] * velocity[2]).sqrt()
                < p.stance_speed_ratio * natural_speed
            && velocity[1].abs() < p.lift_off_speed_ratio * natural_speed
        {
            *reference +=
                (i.relative_height - *reference) * (1.0 - (-dt / p.transition_seconds).exp());
        }
        self.previous = Some((i.authored, i.relative_height, i.root));
        FootMotionOutput {
            target: self.estimate(i.authored),
            correction_velocity: self.velocity,
            contact: self.contact,
            landing_weight: self.landing,
            demand,
        }
    }
}
#[cfg(test)]
mod tests;

fn spring_unit(value: &mut f32, velocity: &mut f32, target: f32, seconds: f32, dt: f32) {
    let omega = 4.0 / seconds;
    let error = *value - target;
    let j = *velocity + omega * error;
    let decay = (-omega * dt).exp();
    *value = (target + (error + j * dt) * decay).clamp(0.0, 1.0);
    *velocity = (*velocity - omega * j * dt) * decay;
}
