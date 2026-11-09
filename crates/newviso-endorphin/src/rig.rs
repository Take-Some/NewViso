use crate::{math, RagdollMaterial};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RigPartDefinition {
    pub name: String,
    pub joint: String,
    #[serde(default)]
    pub end_joint: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub anchor_joint: Option<String>,
    pub radius: f32,
    pub mass_weight: f32,
    pub cone_limit_degrees: f32,
    #[serde(default)]
    pub sole: Option<SoleGeometry>,
}

/// Sole dimensions scale with the authored collision radius and neutral bone span.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SoleGeometry {
    pub width_radius_scale: f32,
    pub thickness_radius_scale: f32,
    pub heel_radius_scale: f32,
    pub vertical_offset_radius_scale: f32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RigDefinition {
    pub mass_kg: f32,
    pub parts: Vec<RigPartDefinition>,
}

impl RigDefinition {
    pub fn floor_contacts(&self) -> impl Iterator<Item = (&str, f32)> {
        self.parts.iter().map(|p| (p.joint.as_str(), p.radius))
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.mass_kg.is_finite()
            || !(1.0..=5000.0).contains(&self.mass_kg)
            || !(2..=32).contains(&self.parts.len())
        {
            return Err("invalid ragdoll mass or part count".into());
        }
        let mut names = BTreeSet::new();
        let mut bones = BTreeSet::new();
        for (index, part) in self.parts.iter().enumerate() {
            if part.name.trim().is_empty()
                || part.joint.trim().is_empty()
                || part.name.len() > 128
                || part.joint.len() > 128
                || !part.radius.is_finite()
                || !(0.02..=1.0).contains(&part.radius)
                || !part.mass_weight.is_finite()
                || !(0.01..=1000.0).contains(&part.mass_weight)
                || part.sole.is_some_and(|s| {
                    ![
                        s.width_radius_scale,
                        s.thickness_radius_scale,
                        s.heel_radius_scale,
                        s.vertical_offset_radius_scale,
                    ]
                    .iter()
                    .all(|v| v.is_finite())
                        || s.width_radius_scale <= 0.0
                        || s.thickness_radius_scale <= 0.0
                        || s.heel_radius_scale < 0.0
                        || part.end_joint.is_none()
                })
                || !part.cone_limit_degrees.is_finite()
                || !(0.0..=180.0).contains(&part.cone_limit_degrees)
                || (index == 0 && part.parent.is_some())
                || (index > 0 && !part.parent.as_ref().is_some_and(|p| names.contains(p)))
                || !names.insert(part.name.clone())
                || !bones.insert(part.joint.clone())
                || part
                    .end_joint
                    .as_ref()
                    .is_some_and(|s| s.trim().is_empty() || s == &part.joint)
                || part
                    .anchor_joint
                    .as_ref()
                    .is_some_and(|s| s.trim().is_empty())
            {
                return Err(format!("invalid ragdoll part {}", part.name));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BodyPose {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub linear_velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
}
#[derive(Clone, Debug)]
pub struct RigPart {
    pub support_axis: Option<[f32; 3]>,
    pub support_clearance: Option<f32>,
    pub support_half_extents: Option<[f32; 3]>,
    pub name: String,
    pub joint: String,
    pub entity: u64,
    pub body_to_joint: [f32; 16],
    pub mass_kg: f32,
    pub inertia_diagonal: [f32; 3],
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct JointSpec {
    pub id: u64,
    pub parent: u64,
    pub child: u64,
    pub parent_anchor: [f32; 3],
    pub child_anchor: [f32; 3],
    pub parent_frame_rotation: [f32; 4],
    pub child_frame_rotation: [f32; 4],
    pub max_force: f32,
    pub cone_limit: f32,
    pub angular_stiffness: f32,
    pub angular_damping: f32,
    pub collision_family: u64,
    pub parent_part: usize,
    pub child_part: usize,
    pub initial_rotation: [f32; 4],
    pub angular_limits: Option<crate::AngularLimits>,
    pub muscle: Option<crate::MuscleModel>,
}
#[derive(Clone, Debug)]
pub struct BodySpec {
    pub entity: u64,
    pub pose: BodyPose,
    pub radius: f32,
    pub half_height: f32,
    pub mass_kg: f32,
    pub inertia_diagonal: [f32; 3],
    pub box_half_extents: Option<[f32; 3]>,
}
impl BodySpec {
    pub fn volume(&self) -> f32 {
        if let Some(h) = self.box_half_extents {
            return 8.0 * h[0] * h[1] * h[2];
        }
        std::f32::consts::PI
            * self.radius
            * self.radius
            * (2.0 * self.half_height + 4.0 * self.radius / 3.0)
    }
}
#[derive(Clone, Debug)]
pub struct RigState {
    pub parts: Vec<RigPart>,
    pub joints: Vec<JointSpec>,
    pub elapsed: f32,
    pub visual_offset: [f32; 3],
    pub visual_rotation: [f32; 3],
    pub pose_updates: u64,
    pub pose_skips: u64,
    pub stance_entities: Vec<u64>,
    palette: Vec<(String, [f32; 16])>,
    entities: Vec<u64>,
    previous: Vec<Option<([f32; 3], [f32; 4])>>,
}
impl RigState {
    pub fn advance(&mut self, dt: f32) {
        if dt.is_finite() && dt > 0.0 {
            self.elapsed += dt;
        }
    }
    pub fn contains(&self, entity: u64) -> bool {
        self.parts.iter().any(|p| p.entity == entity)
    }
    pub fn presentation(&self, position: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        (
            math::add(position, self.visual_offset),
            self.visual_rotation,
        )
    }
    /// Reuses joint names and matrix storage. A stationary body produces no
    /// scene palette writes; contacts and native physics continue normally.
    pub fn refresh_palette(&mut self, mut pose: impl FnMut(u64) -> Option<BodyPose>) -> bool {
        let mut changed = false;
        for (index, part) in self.parts.iter().enumerate() {
            let Some(pose) = pose(part.entity) else {
                continue;
            };
            let key = (pose.position, pose.rotation);
            if self.previous[index] == Some(key) {
                continue;
            }
            self.previous[index] = Some(key);
            self.palette[index].1 = math::product(
                math::matrix(pose.position, pose.rotation),
                part.body_to_joint,
            );
            changed = true;
        }
        if changed {
            self.pose_updates += 1;
        } else {
            self.pose_skips += 1;
        }
        changed
    }
    /// Includes each limb's translation and rotation relative to its support.
    pub fn kinetic_energy_per_mass(
        &self,
        surface_velocity: [f32; 3],
        mut pose: impl FnMut(u64) -> Option<BodyPose>,
    ) -> Option<f32> {
        let mut energy = 0.0;
        let mut mass = 0.0;
        for part in &self.parts {
            let pose = pose(part.entity)?;
            let velocity = math::sub(pose.linear_velocity, surface_velocity);
            let angular = math::tensor_vector(
                math::tensor(pose.rotation, part.inertia_diagonal),
                pose.angular_velocity,
            );
            energy += 0.5
                * (part.mass_kg * math::dot(velocity, velocity)
                    + math::dot(pose.angular_velocity, angular));
            mass += part.mass_kg;
        }
        (mass > 0.0 && energy.is_finite()).then_some(energy / mass)
    }
    pub fn inertia_about(
        &self,
        point: [f32; 3],
        mut pose: impl FnMut(u64) -> Option<BodyPose>,
    ) -> Option<[[f32; 3]; 3]> {
        let mut inertia = [[0.0; 3]; 3];
        for part in &self.parts {
            let p = pose(part.entity)?;
            let local = math::tensor(p.rotation, part.inertia_diagonal);
            let r = math::sub(p.position, point);
            let square = math::dot(r, r);
            for i in 0..3 {
                for j in 0..3 {
                    inertia[i][j] += local[i][j]
                        + part.mass_kg * ((if i == j { square } else { 0.0 }) - r[i] * r[j]);
                }
            }
        }
        inertia
            .iter()
            .flatten()
            .all(|v| v.is_finite())
            .then_some(inertia)
    }
    /// Equivalent whole-body spin about a supporting point, including orbital
    /// momentum and support translation. Local pelvis wobble is not body momentum.
    pub fn angular_velocity_about(
        &self,
        point: [f32; 3],
        surface_velocity: [f32; 3],
        inertia: [[f32; 3]; 3],
        mut pose: impl FnMut(u64) -> Option<BodyPose>,
    ) -> Option<[f32; 3]> {
        let mut momentum = [0.0; 3];
        for part in &self.parts {
            let p = pose(part.entity)?;
            momentum = math::add(
                momentum,
                math::add(
                    math::tensor_vector(
                        math::tensor(p.rotation, part.inertia_diagonal),
                        p.angular_velocity,
                    ),
                    math::cross(
                        math::sub(p.position, point),
                        math::sub(p.linear_velocity, surface_velocity).map(|v| v * part.mass_kg),
                    ),
                ),
            );
        }
        math::solve_symmetric(inertia, momentum)
    }
    /// Elastic work available before the authored drives reach their effort limit.
    pub fn elastic_energy_per_mass(&self) -> f64 {
        let mass = self.parts.iter().map(|p| f64::from(p.mass_kg)).sum::<f64>();
        let energy = self
            .joints
            .iter()
            .filter_map(|j| j.muscle)
            .filter(|m| m.stiffness > 0.0)
            .map(|m| f64::from(m.max_torque).powi(2) / (2.0 * f64::from(m.stiffness)))
            .sum::<f64>();
        if mass > 0.0 {
            energy / mass
        } else {
            0.0
        }
    }
    pub fn entities(&self) -> &[u64] {
        &self.entities
    }
    pub fn palette(&self) -> &[(String, [f32; 16])] {
        &self.palette
    }
}
pub struct RigInput<'a> {
    pub matrices: &'a BTreeMap<String, [f32; 16]>,
    pub reference_matrices: Option<&'a BTreeMap<String, [f32; 16]>>,
    pub anatomy: Option<&'a crate::AnatomyModel>,
    pub entities: &'a [u64],
    pub source: BodyPose,
    pub source_radius: f32,
    pub source_half_height: f32,
    pub source_mass_kg: f32,
    pub visual_position: [f32; 3],
    pub visual_heading: f32,
}
pub struct RigPlan {
    pub state: RigState,
    pub bodies: Vec<BodySpec>,
    pub center_of_mass: [f32; 3],
    pub linear_momentum: [f32; 3],
    pub angular_momentum: [f32; 3],
}

mod build;
