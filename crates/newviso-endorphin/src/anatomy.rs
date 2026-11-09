//! Authored anatomy, local functional damage and physical muscle commands.
use crate::{math, RigDefinition};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnatomyModel {
    pub orientation: BodyOrientation,
    pub balance: crate::BalanceModel,
    pub parts: BTreeMap<String, AnatomicalPart>,
    pub functions: BTreeMap<String, Vec<String>>,
    pub resting_activation: f32,
    pub brace_energy_fraction: f32,
    pub response_time_scale: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyOrientation {
    pub lower: String,
    pub upper: String,
    pub left: String,
    pub right: String,
    #[serde(default)]
    pub forward: Option<[String; 2]>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnatomicalPart {
    pub region: String,
    pub aliases: Vec<String>,
    pub damage_multiplier: f32,
    pub trauma_gain: f32,
    pub limits: Option<AngularLimits>,
    pub muscle: Option<MuscleModel>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AngularLimits {
    /// Radians in a neutral anatomical frame: Y twist, X/Z swing.
    pub swing_x: [f32; 2],
    pub swing_z: [f32; 2],
    pub twist: [f32; 2],
    pub stiffness: f32,
    pub damping: f32,
    pub max_torque: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MuscleModel {
    /// Swing X, twist Y, swing Z in radians, relative to neutral anatomy.
    pub brace_angles: [f32; 3],
    pub stiffness: f32,
    pub damping: f32,
    pub max_torque: f32,
}
impl AnatomyModel {
    pub fn validate(&self, rig: &RigDefinition) -> Result<(), String> {
        let positive = |v: f32| v.is_finite() && v > 0.0;
        let nonnegative = |v: f32| v.is_finite() && v >= 0.0;
        let names: BTreeSet<_> = rig.parts.iter().map(|p| p.name.as_str()).collect();
        let bones: BTreeSet<_> = rig
            .parts
            .iter()
            .flat_map(|p| {
                [
                    Some(p.joint.as_str()),
                    p.end_joint.as_deref(),
                    p.anchor_joint.as_deref(),
                ]
                .into_iter()
                .flatten()
            })
            .collect();
        if self.parts.len() != names.len()
            || self.parts.keys().any(|k| !names.contains(k.as_str()))
            || [
                &self.orientation.lower,
                &self.orientation.upper,
                &self.orientation.left,
                &self.orientation.right,
            ]
            .iter()
            .any(|s| !bones.contains(s.as_str()))
            || self
                .orientation
                .forward
                .as_ref()
                .is_some_and(|names| names.iter().any(|n| !bones.contains(n.as_str())))
            || !self.resting_activation.is_finite()
            || !(0.0..=1.0).contains(&self.resting_activation)
            || !positive(self.brace_energy_fraction)
            || !positive(self.response_time_scale)
        {
            return Err("invalid Endorphin anatomy topology or control parameters".into());
        }
        if self.balance.support_parts.is_empty()
            || self
                .balance
                .support_parts
                .iter()
                .any(|p| !names.contains(p.as_str()))
            || self
                .balance
                .joint_weights
                .iter()
                .any(|(p, w)| !names.contains(p.as_str()) || !w.is_finite())
            || [
                self.balance.gains.height,
                self.balance.gains.plant_position,
                self.balance.gains.posture,
                self.balance.gains.capture,
                self.balance.gains.damping,
                self.balance.gains.gravity_compensation,
            ]
            .iter()
            .any(|v| !nonnegative(*v))
            || !positive(self.balance.footprint_padding)
            || !positive(self.balance.max_lean_angle)
            || self.balance.max_lean_angle > std::f32::consts::FRAC_PI_2
            || !self.balance.recovery_upright_cosine.is_finite()
            || !(0.0..=1.0).contains(&self.balance.recovery_upright_cosine)
        {
            return Err("invalid anatomical balance model".into());
        }
        if let Some(stepping) = &self.balance.stepping {
            stepping.validate(rig)?;
        }
        for (name, part) in &self.parts {
            if part.region.is_empty()
                || part.aliases.iter().any(|s| s.is_empty())
                || !positive(part.damage_multiplier)
                || !nonnegative(part.trauma_gain)
            {
                return Err(format!("invalid anatomical damage model {name}"));
            }
            if let Some(l) = part.limits {
                if [l.swing_x, l.swing_z, l.twist].iter().any(|a| {
                    a.iter()
                        .any(|v| !v.is_finite() || v.abs() > std::f32::consts::PI)
                        || a[0] > 0.0
                        || a[1] < 0.0
                }) || !positive(l.stiffness)
                    || !nonnegative(l.damping)
                    || !positive(l.max_torque)
                {
                    return Err(format!("invalid anatomical angular limits {name}"));
                }
            }
            if let Some(m) = part.muscle {
                if m.brace_angles
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > std::f32::consts::PI)
                    || !nonnegative(m.stiffness)
                    || !nonnegative(m.damping)
                    || !positive(m.max_torque)
                {
                    return Err(format!("invalid anatomical muscle {name}"));
                }
            }
        }
        if self.functions.iter().any(|(name, parts)| {
            name.is_empty() || parts.is_empty() || parts.iter().any(|p| !names.contains(p.as_str()))
        }) {
            return Err("invalid anatomical functional chain".into());
        }
        Ok(())
    }
    pub fn part_for_alias(&self, name: &str) -> Option<&str> {
        if let Some((key, _)) = self.parts.get_key_value(name) {
            return Some(key);
        }
        self.parts
            .iter()
            .find(|(_, p)| p.aliases.iter().any(|a| a == name))
            .map(|(k, _)| k.as_str())
    }
    pub fn damage_multiplier(&self, name: &str) -> Option<f32> {
        self.part_for_alias(name)
            .map(|n| self.parts[n].damage_multiplier)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyDamageState {
    pub trauma: BTreeMap<String, f32>,
}
impl BodyDamageState {
    pub fn valid(&self, model: &AnatomyModel) -> bool {
        self.trauma.iter().all(|(name, value)| {
            model.parts.contains_key(name) && value.is_finite() && *value >= 0.0
        })
    }
    /// Only vitality actually lost after armour enters local tissue response.
    /// Addition in normalized vitality space makes multiple hits order-independent.
    pub fn injure(&mut self, model: &AnatomyModel, part: &str, lost_fraction: f32) -> bool {
        let Some(p) = model.parts.get(part) else {
            return false;
        };
        if !lost_fraction.is_finite() || lost_fraction <= 0.0 {
            return false;
        }
        let trauma = self.trauma.entry(part.into()).or_default();
        *trauma = (*trauma + lost_fraction * p.trauma_gain).min(f32::MAX.sqrt());
        true
    }
    pub fn part_capacity(&self, part: &str) -> f32 {
        (-self.trauma.get(part).copied().unwrap_or(0.0)).exp()
    }
    pub fn function_capacity(&self, model: &AnatomyModel, name: &str) -> f32 {
        model.functions.get(name).map_or(1.0, |parts| {
            parts
                .iter()
                .map(|p| self.part_capacity(p))
                .fold(1.0, f32::min)
        })
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct MuscleCommand {
    pub feed_forward_torque: [f32; 3],
    pub target_rotation: [f32; 4],
    pub stiffness: f32,
    pub damping: f32,
    pub max_torque: f32,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct BodyControlState {
    pub brace: f32,
    pub grounding: f32,
    pub activation: f32,
    pub updates: u64,
    pub balance: crate::BalanceState,
    pub balance_torque: [f32; 3],
    pub joint_torques: Vec<[f32; 3]>,
    pub world_drives: Vec<Option<crate::support::WorldAngularDrive>>,
    pub stance_hips: Vec<usize>,
    pub torso_target: Option<[f32; 4]>,
    pub height_target: Option<f32>,
    pub support_target: Option<[f32; 3]>,
    pub plants: crate::FootPlantState,
    pub stepping: crate::SteppingState,
    pub joint_targets: Vec<Option<[f32; 4]>>,
    pub protection: crate::ProtectionState,
}
impl BodyControlState {
    /// Preserve real support authority across capsule -> articulated ownership.
    /// Native foot contacts still determine balance.supported; this is decaying
    /// motor/plant evidence, not an invented physical contact.
    pub fn inherit_support(
        &mut self,
        rig: &crate::RigState,
        support: Option<crate::Support>,
        body_up: [f32; 3],
        capacity: f32,
        mut pose: impl FnMut(u64) -> Option<crate::BodyPose>,
    ) {
        let Some(support) = support else {
            return;
        };
        let Some(up) = math::unit(body_up).ok() else {
            return;
        };
        let uprightness = up[1].clamp(0.0, 1.0);
        self.torso_target = rig
            .parts
            .first()
            .and_then(|part| pose(part.entity))
            .map(|p| math::multiply(math::align_axis(up, [0.0, 1.0, 0.0]), p.rotation));
        let mut mass = 0.0;
        let mut com_y = 0.0;
        let mut com_xz = [0.0; 2];
        let mut floor_y = 0.0;
        let mut feet = 0.0;
        for part in &rig.parts {
            if let Some(p) = pose(part.entity) {
                mass += part.mass_kg;
                com_y += part.mass_kg * p.position[1];
                com_xz[0] += part.mass_kg * p.position[0];
                com_xz[1] += part.mass_kg * p.position[2];
                if rig.stance_entities.contains(&part.entity) {
                    floor_y += p.position[1] - part.support_clearance.unwrap_or(0.0);
                    feet += 1.0;
                }
            }
        }
        if mass > 0.0 && feet > 0.0 {
            self.height_target = Some(com_y / mass - floor_y / feet);
            self.support_target = Some([com_xz[0] / mass, floor_y / feet, com_xz[1] / mass]);
        }
        self.balance.body_up = up;
        self.balance.uprightness = uprightness;
        self.balance.support_normal = support.normal;
        self.grounding = support.normal[1].clamp(0.0, 1.0);
        self.activation = capacity.clamp(0.0, 1.0) * uprightness.powi(2);
        for (index, part) in rig.parts.iter().enumerate() {
            if rig.stance_entities.contains(&part.entity) {
                if let Some(p) = pose(part.entity) {
                    self.plants.targets.insert(
                        index,
                        crate::FootPlant {
                            position: p.position,
                            confidence: uprightness.powi(2),
                        },
                    );
                }
            }
        }
    }

    pub fn observe(
        &mut self,
        model: &AnatomyModel,
        energy_per_mass: f32,
        gravity: f32,
        height: f32,
        capacity: f32,
        elastic_energy_per_mass: f64,
        ground_support: f32,
        dt: f32,
    ) {
        if !elastic_energy_per_mass.is_finite()
            || elastic_energy_per_mass < 0.0
            || ![
                energy_per_mass,
                gravity,
                height,
                capacity,
                ground_support,
                dt,
            ]
            .iter()
            .all(|v| v.is_finite())
            || gravity <= 0.0
            || height <= 0.0
            || dt <= 0.0
        {
            self.activation = 0.0;
            return;
        }
        let ratio = f64::from(energy_per_mass.max(0.0))
            / ((f64::from(gravity) * f64::from(height) + elastic_energy_per_mass)
                * f64::from(model.brace_energy_fraction));
        let response = (height / gravity).sqrt() * model.response_time_scale;
        self.grounding +=
            (ground_support.clamp(0.0, 1.0) - self.grounding) * (1.0 - (-dt / response).exp());
        let target = (1.0 - (-ratio).exp()) as f32 * (1.0 - self.grounding);
        self.brace += (target - self.brace) * (1.0 - (-dt / response).exp());
        self.activation = capacity.clamp(0.0, 1.0)
            * (model.resting_activation
                + (1.0 - model.resting_activation)
                    * self.brace.max(
                        self.balance.uprightness.powi(2)
                            * if self.balance.supported {
                                1.0
                            } else {
                                self.plants
                                    .targets
                                    .values()
                                    .map(|p| p.confidence)
                                    .fold(0.0, f32::max)
                            },
                    ));
        self.updates += 1;
    }
    pub fn command(
        &self,
        muscle: MuscleModel,
        initial: [f32; 4],
        current: [f32; 4],
        local_capacity: f32,
    ) -> Option<MuscleCommand> {
        let activation = self.activation * local_capacity.clamp(0.0, 1.0);
        if activation <= f32::EPSILON {
            return None;
        }
        let target = math::swing_twist(muscle.brace_angles);
        let support = self.balance.uprightness.powi(2)
            * if self.balance.supported {
                1.0
            } else {
                self.plants
                    .targets
                    .values()
                    .map(|p| p.confidence)
                    .fold(0.0, f32::max)
            };
        let initial = math::slerp(current, initial, support);
        Some(MuscleCommand {
            feed_forward_torque: [0.0; 3],
            target_rotation: math::slerp(initial, target, self.brace),
            stiffness: muscle.stiffness * activation,
            damping: muscle.damping * activation.sqrt(),
            max_torque: muscle.max_torque * activation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn anatomy() -> AnatomyModel {
        let part = |region: &str| AnatomicalPart {
            region: region.into(),
            aliases: vec![],
            damage_multiplier: 1.0,
            trauma_gain: 5.0,
            limits: None,
            muscle: None,
        };
        AnatomyModel {
            orientation: BodyOrientation {
                lower: "a".into(),
                upper: "b".into(),
                left: "c".into(),
                right: "d".into(),
                forward: None,
            },
            parts: BTreeMap::from([
                ("left".into(), part("arm")),
                ("right".into(), part("arm")),
                ("leg".into(), part("leg")),
            ]),
            functions: BTreeMap::from([
                ("left_grip".into(), vec!["left".into()]),
                ("right_grip".into(), vec!["right".into()]),
                ("left_stance".into(), vec!["leg".into()]),
            ]),
            resting_activation: 0.025,
            brace_energy_fraction: 0.25,
            response_time_scale: 0.25,
            balance: crate::BalanceModel {
                stepping: None,
                support_parts: vec!["leg".into()],
                joint_weights: BTreeMap::new(),
                footprint_padding: 0.04,
                max_lean_angle: 0.35,
                recovery_upright_cosine: 0.95,
                gains: Default::default(),
            },
        }
    }
    #[test]
    fn local_damage_is_side_specific_order_independent_and_survives_serialization() {
        let model = anatomy();
        let mut a = BodyDamageState::default();
        let mut b = a.clone();
        assert!(!a.injure(&model, "left", 0.0)); // fully absorbed armour does not injure tissue
        assert!(!a.injure(&model, "left", f32::NAN));
        a.injure(&model, "left", 0.1);
        a.injure(&model, "left", 0.2);
        b.injure(&model, "left", 0.3);
        assert!((a.part_capacity("left") - b.part_capacity("left")).abs() < 1e-6);
        assert!(a.function_capacity(&model, "left_grip") < 0.3);
        assert_eq!(a.function_capacity(&model, "right_grip"), 1.0);
        assert_eq!(a.function_capacity(&model, "left_stance"), 1.0);
        let restored: BodyDamageState =
            serde_json::from_value(serde_json::to_value(&a).unwrap()).unwrap();
        assert!(restored.valid(&model));
        assert_eq!(restored.part_capacity("left"), a.part_capacity("left"));
    }
    #[test]
    fn muscle_response_adapts_to_motion_local_capacity_and_observation_cadence() {
        let model = anatomy();
        let mut a = BodyControlState::default();
        let mut b = a.clone();
        for _ in 0..60 {
            a.observe(&model, 5.0, 9.81, 1.8, 0.8, 0.0, 0.0, 1.0 / 60.0);
        }
        for _ in 0..144 {
            b.observe(&model, 5.0, 9.81, 1.8, 0.8, 0.0, 0.0, 1.0 / 144.0);
        }
        assert!((a.brace - b.brace).abs() < 1e-5);
        let muscle = MuscleModel {
            brace_angles: [1.0, 0.0, 0.0],
            stiffness: 50.0,
            damping: 0.1,
            max_torque: 30.0,
        };
        let full = a
            .command(muscle, [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0], 1.0)
            .unwrap();
        let injured = a
            .command(muscle, [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0], 0.25)
            .unwrap();
        assert!((injured.max_torque / full.max_torque - 0.25).abs() < 1e-6);
        assert!((injured.stiffness / full.stiffness - 0.25).abs() < 1e-6);
        assert!(a
            .command(muscle, [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0], 0.0)
            .is_none());
        a.activation = 0.0;
        assert!(a
            .command(muscle, [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0], 1.0)
            .is_none());
    }
}

#[cfg(test)]
mod pose_memory_tests {
    use super::*;
    #[test]
    fn unsupported_muscles_relax_without_erasing_anatomy_and_stance_keeps_pose_memory() {
        let mut s = BodyControlState::default();
        s.activation = 0.2;
        let muscle = MuscleModel {
            brace_angles: [0.0; 3],
            stiffness: 100.0,
            damping: 0.1,
            max_torque: 20.0,
        };
        let initial = math::swing_twist([1.0, 0.0, 0.0]);
        let current = math::swing_twist([-0.5, 0.0, 0.0]);
        let relaxed = s.command(muscle, initial, current, 1.0).unwrap();
        let same = |a: [f32; 4], b: [f32; 4]| {
            a.iter().zip(b).map(|(a, b)| a * b).sum::<f32>().abs() > 0.99999
        };
        assert!(same(relaxed.target_rotation, current));
        s.balance.uprightness = 1.0;
        s.balance.supported = true;
        let standing = s.command(muscle, initial, current, 1.0).unwrap();
        assert!(same(standing.target_rotation, initial));
        assert_eq!(standing.max_torque, relaxed.max_torque);
    }
}
