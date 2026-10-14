use super::*;
use crate::application_peds::actor_hash;
mod math;
mod rig;
pub(crate) use rig::RagdollDefinition;

#[derive(Clone, Debug)]
pub(super) struct RagdollPart {
    name: String,
    joint: String,
    entity: u64,
    body_to_joint: [f32; 16],
    mass_kg: f32,
}

#[derive(Clone, Debug)]
pub(super) struct RagdollState {
    parts: Vec<RagdollPart>,
    joints: Vec<PhysicsJointSnapshot>,
    elapsed: f32,
    visual_offset: [f32; 3],
    visual_rotation: [f32; 3],
}

impl RagdollState {
    pub(super) fn advance(&mut self, dt: f32) {
        self.elapsed += dt;
    }
    pub(super) fn contains(&self, entity: u64) -> bool {
        self.parts.iter().any(|p| p.entity == entity)
    }
    pub(super) fn presentation(&self, position: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        (
            math::add(position, self.visual_offset),
            self.visual_rotation,
        )
    }
}

impl EngineApplication {
    pub(crate) fn ped_has_ragdoll(&self, id: &str) -> bool {
        self.ped_physics
            .bodies
            .get(id)
            .is_some_and(|b| b.ragdoll.is_some())
    }
    pub(crate) fn activate_ped_ragdoll(
        &mut self,
        id: &str,
        push: [f32; 3],
        point: [f32; 3],
        impulse: f32,
    ) -> Result<bool, String> {
        let Some(body) = self.ped_physics.bodies.get(id) else {
            return Ok(false);
        };
        if body.ragdoll.is_some() {
            return Ok(true);
        }
        let Some(definition) = self
            .world_actor_presentations
            .get(id)
            .and_then(|p| p.locomotion.as_ref())
            .and_then(|l| l.ragdoll.clone())
        else {
            return Ok(false);
        };
        if !self
            .physics
            .as_ref()
            .is_some_and(|p| p.supports_joint_constraints())
        {
            return Ok(false);
        }
        let Some(plan) = self.build_ped_ragdoll(id, &definition, push)? else {
            return Ok(false);
        };
        let entity = body.entity;
        let physics = self.physics.as_mut().expect("ragdoll physics");
        for command in &plan.commands {
            physics.upsert_body_from_script(command, 0)?;
            // Density-only replacements also need their inherited angular motion.
            physics.set_character_angular_velocity(
                command["entity"].as_u64().expect("ragdoll entity"),
                serde_json::from_value(command["angular_velocity"].clone())
                    .map_err(|e| e.to_string())?,
            );
        }
        for joint in &plan.state.joints {
            physics.upsert_joint(*joint)?;
        }
        let selected = plan
            .state
            .parts
            .iter()
            .min_by(|a, b| {
                let d = |part: &RagdollPart| {
                    physics
                        .vehicle_body_state(part.entity)
                        .map_or(f32::MAX, |p| math::length(math::sub(p.position, point)))
                };
                d(a).total_cmp(&d(b))
            })
            .map(|p| p.entity)
            .unwrap_or(entity);
        if impulse > 0.0 {
            physics.apply_impulse_from_script(
                &json!({"entity":selected,"impulse":push.map(|v|v*impulse),"point":point}),
                0,
            )?;
        }
        let body = self.ped_physics.bodies.get_mut(id).expect("ped body");
        body.ragdoll = Some(plan.state);
        body.reaction = None;
        body.hit_reaction = None;
        body.recovery_seconds = 0.0;
        body.was_dead = true;
        self.sync_ped_ragdoll_poses()?;
        host::publish_event_json(
            "ped.ragdoll.started",
            "newviso.peds",
            json!({"actor_id":id,"entity_key":entity.to_string(),
            "hit_entity_key":selected.to_string(),"impulse":impulse}),
        )?;
        Ok(true)
    }

    pub(crate) fn clear_ped_ragdoll(&mut self, id: &str) -> Result<(), String> {
        let Some(body) = self.ped_physics.bodies.get_mut(id) else {
            return Ok(());
        };
        let entity = body.entity;
        if let Some(state) = body.ragdoll.take() {
            if let Some(physics) = self.physics.as_mut() {
                for joint in state.joints {
                    physics.remove_joint(joint.id);
                }
                for part in state.parts {
                    if part.entity != entity {
                        physics.destroy_body_from_script(&json!({"entity":part.entity}), 0)?;
                    }
                }
            }
            self.scene.clear_entity_physical_pose(entity);
        }
        Ok(())
    }

    pub(crate) fn sync_ped_ragdoll_poses(&mut self) -> Result<(), String> {
        let Some(physics) = self.physics.as_ref() else {
            return Ok(());
        };
        for body in self.ped_physics.bodies.values() {
            let Some(ragdoll) = &body.ragdoll else {
                continue;
            };
            let joints: Vec<_> = ragdoll
                .parts
                .iter()
                .filter_map(|part| {
                    physics.vehicle_body_state(part.entity).map(|p| {
                        (
                            part.joint.clone(),
                            math::product(math::matrix(p.position, p.rotation), part.body_to_joint),
                        )
                    })
                })
                .collect();
            self.scene.set_entity_physical_pose(body.entity, &joints)?;
        }
        Ok(())
    }

    pub(super) fn ragdoll_snapshot(&self, body: &PedBody) -> Option<Value> {
        let ragdoll = body.ragdoll.as_ref()?;
        let physics = self.physics.as_ref()?;
        let parts:Vec<_>=ragdoll.parts.iter().map(|part| {
            let pose=physics.vehicle_body_state(part.entity);
            json!({"name":part.name,"joint":part.joint,"entity_key":part.entity.to_string(),"mass_kg":part.mass_kg,
                "position":pose.map(|p|p.position),"rotation":pose.map(|p|p.rotation),
                "linear_velocity":pose.map(|p|p.linear_velocity),"angular_velocity":pose.map(|p|p.angular_velocity),
                "joint_position":pose.map(|p|math::position(math::product(math::matrix(p.position,p.rotation),part.body_to_joint)))})
        }).collect();
        let joints:Vec<_>=ragdoll.joints.iter().filter_map(|joint| {
            let a=physics.vehicle_body_state(joint.parent)?;let b=physics.vehicle_body_state(joint.child)?;
            let ap=math::add(a.position,math::rotate(a.rotation,joint.parent_anchor));
            let bp=math::add(b.position,math::rotate(b.rotation,joint.child_anchor));
            Some(json!({"id_key":joint.id.to_string(),"parent_key":joint.parent.to_string(),"child_key":joint.child.to_string(),
                "separation":math::length(math::sub(ap,bp)),"parent_anchor":ap,"child_anchor":bp}))
        }).collect();
        Some(
            json!({"active":true,"elapsed_seconds":ragdoll.elapsed,"parts":parts,"joints":joints,
            "physical_skeleton":self.scene.entity_has_physical_pose(body.entity)}),
        )
    }
}
