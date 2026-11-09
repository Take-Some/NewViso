use super::*;
use newviso_endorphin::math;
impl EngineApplication {
    pub(super) fn melee_scene_entity(&self, id: &str) -> Option<u64> {
        self.world_actor_presentations
            .get(id)
            .and_then(|p| self.scene.runtime_entity_stable_id(&p.scene_key))
            .or_else(|| self.scene.runtime_entity_stable_id(id))
            .or_else(|| self.ped_physics.bodies.get(id).map(|b| b.entity))
    }
    pub(super) fn melee_capsules(&self, strike: &ActiveMeleeStrike) -> Option<Vec<StrikeCapsule>> {
        let parent = self.melee_scene_entity(&strike.actor_id)?;
        let mut result = Vec::with_capacity(strike.action.strikes.len());
        for binding in &strike.action.strikes {
            let matrix = if let Some(joint) = &binding.joint {
                let joint = self
                    .peds
                    .state(&strike.actor_id)
                    .and_then(|p| p.profile.melee.as_ref())
                    .and_then(|p| p.joint_map.get(joint))
                    .unwrap_or(joint);
                self.scene.entity_joint_world_matrix(parent, joint).ok()?
            } else {
                let equipped = strike.equipment.as_ref()?;
                let definition = self.items.definition(equipped.item_id.as_ref()?)?;
                let expected = definition
                    .metadata
                    .pointer("/presentation/model")?
                    .as_str()?;
                let child = self
                    .scene
                    .joint_attachment_children(parent)
                    .into_iter()
                    .find(|e| {
                        self.scene.entity_state(*e).is_some_and(|s| {
                            s["residency"] == "resident" && s["asset_ref"] == expected
                        })
                    })?;
                let state = self.scene.entity_state(child)?;
                let transform = state.get("transform")?;
                let position: [f32; 3] =
                    serde_json::from_value(transform.get("position")?.clone()).ok()?;
                let degrees: [f32; 3] =
                    serde_json::from_value(transform.get("rotation_degrees")?.clone()).ok()?;
                math::matrix(
                    position,
                    crate::application_physics::math::euler_degrees_to_quaternion(degrees),
                )
            };
            let point = |v: [f32; 3]| {
                std::array::from_fn(|i| {
                    matrix[12 + i] + matrix[i] * v[0] + matrix[4 + i] * v[1] + matrix[8 + i] * v[2]
                })
            };
            let end = if let Some(joint) = &binding.end_joint {
                let joint = self
                    .peds
                    .state(&strike.actor_id)
                    .and_then(|p| p.profile.melee.as_ref())
                    .and_then(|p| p.joint_map.get(joint))
                    .unwrap_or(joint);
                let end = self.scene.entity_joint_world_matrix(parent, joint).ok()?;
                std::array::from_fn(|i| {
                    end[12 + i]
                        + end[i] * binding.end[0]
                        + end[4 + i] * binding.end[1]
                        + end[8 + i] * binding.end[2]
                })
            } else {
                point(binding.end)
            };
            result.push(StrikeCapsule {
                start: point(binding.start),
                end,
                radius: binding.radius,
            });
        }
        Some(result)
    }
    pub(super) fn melee_receivers(&self) -> Vec<MeleeReceiver> {
        let mut receivers = Vec::new();
        for view in self
            .living_world
            .actor_runtime_views()
            .into_iter()
            .filter(|a| a.enabled && a.simulation_tier == "full")
        {
            let Some(ped) = self.peds.state(&view.id) else {
                continue;
            };
            let mut installed = false;
            if let Some(body) = self.ped_physics.bodies.get(&view.id) {
                for part in &body.endorphin.model.rig.parts {
                    let Some(parent) = self.melee_scene_entity(&view.id) else {
                        continue;
                    };
                    let Ok(a) = self.scene.entity_joint_world_position(parent, &part.joint) else {
                        continue;
                    };
                    let b = part
                        .end_joint
                        .as_ref()
                        .and_then(|j| self.scene.entity_joint_world_position(parent, j).ok())
                        .unwrap_or(a);
                    let entity = body
                        .endorphin
                        .ragdoll
                        .as_ref()
                        .and_then(|r| r.parts.iter().find(|p| p.name == part.name))
                        .map_or(body.entity, |p| p.entity);
                    let region = body
                        .endorphin
                        .model
                        .anatomy
                        .as_ref()
                        .and_then(|a| a.parts.get(&part.name))
                        .map(|p| p.region.clone());
                    receivers.push(MeleeReceiver {
                        actor: view.id.clone(),
                        part: Some(part.name.clone()),
                        region,
                        entity,
                        capsule: StrikeCapsule {
                            start: a,
                            end: b,
                            radius: part.radius,
                        },
                    });
                    installed = true;
                }
            }
            if !installed {
                // External controllers expose their real receiving capsule; no invented skeleton.
                if let Some(entity) = ped.profile.damage_entity {
                    if let Some(pose) = self
                        .physics
                        .as_ref()
                        .and_then(|p| p.vehicle_body_state(entity))
                    {
                        let half = (ped.profile.capsule_height * 0.5 - ped.profile.capsule_radius)
                            .max(0.0);
                        let axis = math::rotate(pose.rotation, [0.0, half, 0.0]);
                        receivers.push(MeleeReceiver {
                            actor: view.id,
                            part: None,
                            region: None,
                            entity,
                            capsule: StrikeCapsule {
                                start: math::sub(pose.position, axis),
                                end: math::add(pose.position, axis),
                                radius: ped.profile.capsule_radius,
                            },
                        });
                    }
                }
            }
        }
        receivers
    }
}
