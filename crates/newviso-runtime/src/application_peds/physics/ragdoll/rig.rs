use super::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct RagdollPartDefinition {
    name: String,
    joint: String,
    #[serde(default)]
    end_joint: Option<String>,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    anchor_joint: Option<String>,
    radius: f32,
    mass_weight: f32,
    cone_limit_degrees: f32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct RagdollDefinition {
    mass_kg: f32,
    parts: Vec<RagdollPartDefinition>,
}

impl RagdollDefinition {
    pub(crate) fn floor_contacts(&self) -> impl Iterator<Item = (&str, f32)> {
        self.parts.iter().map(|p| (p.joint.as_str(), p.radius))
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self.mass_kg.is_finite()
            || !(1.0..=5000.0).contains(&self.mass_kg)
            || !(2..=32).contains(&self.parts.len())
        {
            return Err("invalid ragdoll mass or part count".into());
        }
        let mut names = BTreeSet::new();
        for (index, part) in self.parts.iter().enumerate() {
            if part.name.trim().is_empty()
                || part.joint.trim().is_empty()
                || part.name.len() > 128
                || part.joint.len() > 128
                || !part.radius.is_finite()
                || !(0.02..=1.0).contains(&part.radius)
                || !part.mass_weight.is_finite()
                || !(0.01..=1000.0).contains(&part.mass_weight)
                || !part.cone_limit_degrees.is_finite()
                || !(0.0..=180.0).contains(&part.cone_limit_degrees)
                || (index == 0 && part.parent.is_some())
                || (index > 0 && !part.parent.as_ref().is_some_and(|p| names.contains(p)))
                || !names.insert(part.name.clone())
            {
                return Err(format!("invalid ragdoll part {}", part.name));
            }
        }
        Ok(())
    }
}

pub(super) struct RagdollPlan {
    pub state: RagdollState,
    pub commands: Vec<Value>,
}

impl EngineApplication {
    pub(super) fn build_ped_ragdoll(
        &self,
        id: &str,
        definition: &RagdollDefinition,
        push: [f32; 3],
    ) -> Result<Option<RagdollPlan>, String> {
        definition.validate()?;
        let body = &self.ped_physics.bodies[id];
        let physics = self.physics.as_ref().expect("ragdoll physics");
        let Some(native) = physics.vehicle_body_state(body.entity) else {
            return Ok(None);
        };
        // A streamed skeleton must be installed before ownership can transfer.
        // Keep the existing animated fallback until then; retry on later frames.
        let mut matrices = BTreeMap::new();
        for part in &definition.parts {
            for joint in [
                Some(&part.joint),
                part.end_joint.as_ref(),
                part.anchor_joint.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                if !matrices.contains_key(joint) {
                    let Ok(matrix) = self.scene.entity_joint_world_matrix(body.entity, joint)
                    else {
                        return Ok(None);
                    };
                    matrices.insert(joint.clone(), matrix);
                }
            }
        }
        let visual_position = self
            .world_presentation_states
            .get(id)
            .ok_or_else(|| "missing ragdoll presentation".to_owned())?
            .position;
        let weight: f32 = definition.parts.iter().map(|p| p.mass_weight).sum();
        let mut parts = Vec::<RagdollPart>::new();
        let mut joints = Vec::new();
        let mut commands = Vec::new();
        let mut initial = BTreeMap::<String, ([f32; 3], [f32; 4], u64)>::new();
        let n = math::length([push[0], 0.0, push[2]]);
        let seed = if n > 0.01 {
            [push[2] / n * 0.7, 0.0, -push[0] / n * 0.7]
        } else {
            [0.7, 0.0, 0.35]
        };
        let angular = math::add(native.angular_velocity.map(|v| v.clamp(-6.0, 6.0)), seed);
        for (index, part) in definition.parts.iter().enumerate() {
            let start = matrices[&part.joint];
            let a = math::position(start);
            let end = part.end_joint.as_ref().map(|j| math::position(matrices[j]));
            let center = end.map_or(a, |b| math::add(a, b).map(|v| v * 0.5));
            let rotation = end.map_or_else(
                || math::rotation(start),
                |b| math::from_axis(math::sub(b, a)),
            );
            let half_height = end.map_or(0.0, |b| {
                (math::length(math::sub(b, a)) * 0.5 - part.radius * 0.6).max(0.01)
            });
            let entity = if index == 0 {
                body.entity
            } else {
                actor_hash(&format!("physics.ragdoll.{id}.{}", part.name)) & !(1u64 << 63)
            };
            let mass = definition.mass_kg * part.mass_weight / weight;
            let volume = std::f32::consts::PI
                * part.radius
                * part.radius
                * (2.0 * half_height + 4.0 * part.radius / 3.0);
            let velocity = math::add(
                native.linear_velocity,
                math::cross(angular, math::sub(center, native.position)),
            );
            let shape = if half_height > 0.0 {
                json!({"kind":"capsule","radius":part.radius,"half_height":half_height})
            } else {
                json!({"kind":"sphere","radius":part.radius})
            };
            commands.push(json!({"entity":entity,"body_kind":"dynamic","shape":shape,"position":center,"rotation":rotation,
                "linear_velocity":velocity,"angular_velocity":angular,"density":mass/volume,"friction":0.75,"restitution":0.0,
                "linear_damping":0.08,"angular_damping":0.45,"continuous_collision":true,
                "participates_in_queries":true,"casts_contacts":true}));
            let body_to_joint = math::product(math::inverse_pose(center, rotation), start);
            parts.push(RagdollPart {
                name: part.name.clone(),
                joint: part.joint.clone(),
                entity,
                body_to_joint,
                mass_kg: mass,
            });
            if let Some(parent) = part.parent.as_ref() {
                let (parent_center, parent_rotation, parent_entity) = initial[parent];
                let anchor_joint = part.anchor_joint.as_ref().unwrap_or(&part.joint);
                let anchor = math::position(matrices[anchor_joint]);
                let frame = math::rotation(matrices[anchor_joint]);
                joints.push(PhysicsJointSnapshot {
                    id: entity,
                    parent: parent_entity,
                    child: entity,
                    parent_anchor: math::rotate(
                        math::inverse(parent_rotation),
                        math::sub(anchor, parent_center),
                    ),
                    child_anchor: math::rotate(math::inverse(rotation), math::sub(anchor, center)),
                    parent_frame_rotation: math::multiply(math::inverse(parent_rotation), frame),
                    child_frame_rotation: math::multiply(math::inverse(rotation), frame),
                    // This rig has no joint breaking: retain articulation under wheel/chassis loads.
                    max_force: 1_000_000.0,
                    cone_limit: part.cone_limit_degrees.to_radians(),
                    angular_stiffness: 12.0,
                    angular_damping: 0.04,
                    collision_family: body.entity,
                });
            }
            initial.insert(part.name.clone(), (center, rotation, entity));
        }
        let root_center = initial[&definition.parts[0].name].0;
        Ok(Some(RagdollPlan {
            state: RagdollState {
                parts,
                joints,
                elapsed: 0.0,
                visual_offset: math::sub(visual_position, root_center),
                visual_rotation: [
                    0.0,
                    self.world_presentation_states[id].rotation_degrees[1],
                    0.0,
                ],
            },
            commands,
        }))
    }
}
