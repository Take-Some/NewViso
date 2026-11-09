use super::*;
use newviso_endorphin::{math, BodyPose, RigDefinition, RigInput, RigPart};

impl EngineApplication {
    pub(crate) fn ped_has_ragdoll(&self, id: &str) -> bool {
        self.ped_physics
            .bodies
            .get(id)
            .is_some_and(|b| b.endorphin.ragdoll.is_some())
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
        if let Some(rig) = &body.endorphin.ragdoll {
            let selected = rig
                .parts
                .iter()
                .min_by(|a, b| {
                    let distance = |e| {
                        self.physics
                            .as_ref()
                            .and_then(|p| p.vehicle_body_state(e))
                            .map_or(f32::MAX, |p| math::length(math::sub(p.position, point)))
                    };
                    distance(a.entity).total_cmp(&distance(b.entity))
                })
                .map_or(body.entity, |p| p.entity);
            if impulse > 0.0 {
                if let Some(physics) = self.physics.as_mut() {
                    physics.apply_impulse_from_script(
                        &json!({"entity":selected,
                        "impulse":push.map(|v|v*impulse),"point":point}),
                        0,
                    )?;
                }
            }
            return Ok(true);
        }
        let definition = self
            .world_actor_presentations
            .get(id)
            .and_then(|p| p.locomotion.as_ref())
            .and_then(|l| l.ragdoll.clone())
            .unwrap_or_else(|| body.endorphin.model.rig.clone());
        if !self
            .physics
            .as_ref()
            .is_some_and(|p| p.supports_joint_constraints())
        {
            return Ok(false);
        }
        let Some(plan) = self.build_ped_ragdoll(id, &definition)? else {
            // Zero-impulse ownership transfers can precede streamed skeleton
            // residency. Resume once it binds, using the then-current momentum.
            // A rejected explicit impulse remains the caller's responsibility.
            if impulse == 0.0 && body.endorphin.bound_skeleton.is_none() {
                self.ped_physics
                    .bodies
                    .get_mut(id)
                    .expect("deferred ped")
                    .endorphin
                    .articulation_pending = true;
            }
            return Ok(false);
        };
        let entity = body.entity;
        let mut control = newviso_endorphin::BodyControlState::default();
        if !self.peds.is_dead(id) {
            if let (Some(physics), Some(anatomy)) =
                (self.physics.as_ref(), body.endorphin.model.anatomy.as_ref())
            {
                let root_height = physics
                    .vehicle_body_state(entity)
                    .map_or(f32::MIN, |p| p.position[1]);
                let support = physics
                    .endorphin_support(
                        &[entity],
                        body.endorphin.model.recovery.minimum_support_cosine,
                        root_height,
                    )
                    .map(|(_, s)| s);
                let joint = |name: &str| {
                    plan.state
                        .palette()
                        .iter()
                        .find(|(bone, _)| bone == name)
                        .map(|(_, m)| math::position(*m))
                };
                if let Some((upper, lower)) =
                    joint(&anatomy.orientation.upper).zip(joint(&anatomy.orientation.lower))
                {
                    let capacity = self.peds.state(id).map_or(0.0, |p| {
                        let stamina = p
                            .melee
                            .as_ref()
                            .zip(p.profile.melee.as_ref())
                            .map_or(1.0, |(s, m)| s.stamina / m.max_stamina);
                        ((p.health / p.profile.max_health) * stamina)
                            .clamp(0.0, 1.0)
                            .sqrt()
                    });
                    control.inherit_support(
                        &plan.state,
                        support,
                        math::sub(upper, lower),
                        capacity,
                        |e| plan.bodies.iter().find(|b| b.entity == e).map(|b| b.pose),
                    );
                }
            }
        }
        let physics = self.physics.as_mut().expect("ragdoll physics");
        let material = &body.endorphin.model.ragdoll;
        physics.release_character_body(entity, material.angular_damping)?;
        for part in &plan.bodies {
            let shape = if let Some(h) = part.box_half_extents {
                json!({"kind":"box","half_extents":h})
            } else if part.half_height > 0.0 {
                json!({"kind":"capsule","radius":part.radius,"half_height":part.half_height})
            } else {
                json!({"kind":"sphere","radius":part.radius})
            };
            let command = json!({"entity":part.entity,"body_kind":"dynamic","shape":shape,
                "position":part.pose.position,"rotation":part.pose.rotation,
                "linear_velocity":part.pose.linear_velocity,"angular_velocity":part.pose.angular_velocity,
                "mass_properties":{"mass":part.mass_kg,"center_of_mass":[0,0,0],"inertia_diagonal":part.inertia_diagonal},
                "density":part.mass_kg/part.volume(),"friction":material.friction,"restitution":material.restitution,
                "linear_damping":material.linear_damping,"angular_damping":material.angular_damping,
                "continuous_collision":true,"participates_in_queries":true,"casts_contacts":true});
            physics.upsert_body_from_script(&command, 0)?;
            physics.set_character_angular_velocity(part.entity, part.pose.angular_velocity);
        }
        for joint in &plan.state.joints {
            physics.upsert_joint(PhysicsJointSnapshot {
                id: joint.id,
                parent: joint.parent,
                child: joint.child,
                parent_anchor: joint.parent_anchor,
                child_anchor: joint.child_anchor,
                parent_frame_rotation: joint.parent_frame_rotation,
                child_frame_rotation: joint.child_frame_rotation,
                max_force: joint.max_force,
                cone_limit: joint.cone_limit,
                angular_stiffness: joint.angular_stiffness,
                angular_damping: joint.angular_damping,
                collision_family: joint.collision_family,
                angular_limits: joint.angular_limits.map(|l| {
                    newviso_physics_client::PhysicsAngularLimits {
                        swing_x: l.swing_x,
                        swing_z: l.swing_z,
                        twist: l.twist,
                        stiffness: l.stiffness,
                        damping: l.damping,
                        max_torque: l.max_torque,
                    }
                }),
                motor: None,
            })?;
        }
        let selected = plan
            .state
            .parts
            .iter()
            .min_by(|a, b| {
                let d = |part: &RigPart| {
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
        // Ownership transfer invalidates pending voluntary actions immediately.
        self.cancel_ped_melee(id);
        self.invalidate_ped_grip_motion(id);
        let body = self.ped_physics.bodies.get_mut(id).expect("ped body");
        body.endorphin.articulation_pending = false;
        body.endorphin.ragdoll = Some(plan.state);
        body.endorphin.control = control;
        body.endorphin.getup_match = Default::default();
        body.endorphin.recovery_support = None;
        if body.endorphin.getup_support.status == "bound" {
            body.endorphin.getup_support.status = "interrupted";
        }
        body.endorphin.control_clock.reset();
        // The root COM changes from capsule to pelvis without moving the skin.
        // A prior root-relative hand sample is not an animation velocity.
        if self.peds.is_dead(id) {
            body.reaction = None;
        }
        body.hit_reaction = None;
        body.recovery_seconds = 0.0;
        body.was_dead = self.peds.is_dead(id);
        self.sync_ped_ragdoll_poses()?;
        host::publish_event_json(
            "ped.ragdoll.started",
            "newviso.peds",
            json!({"actor_id":id,"entity_key":entity.to_string(),
            "hit_entity_key":selected.to_string(),"impulse":impulse}),
        )?;
        Ok(true)
    }

    pub(crate) fn resume_deferred_ped_articulation(&mut self, id: &str) -> Result<(), String> {
        let Some(body) = self.ped_physics.bodies.get(id) else {
            return Ok(());
        };
        if !body.endorphin.articulation_pending
            || body.endorphin.bound_skeleton.is_none()
            || body.endorphin.ragdoll.is_some()
            || body.reset_requested
        {
            return Ok(());
        }
        let Some(point) = self
            .physics
            .as_ref()
            .and_then(|p| p.vehicle_body_state(body.entity))
            .map(|p| p.position)
        else {
            return Ok(());
        };
        self.ped_physics
            .bodies
            .get_mut(id)
            .expect("deferred ped")
            .endorphin
            .articulation_pending = false;
        self.activate_ped_ragdoll(id, [0.0; 3], point, 0.0)?;
        Ok(())
    }

    pub(crate) fn clear_ped_ragdoll(&mut self, id: &str) -> Result<(), String> {
        let Some(body) = self.ped_physics.bodies.get_mut(id) else {
            return Ok(());
        };
        body.endorphin.articulation_pending = false;
        let entity = body.entity;
        if let Some(state) = body.endorphin.ragdoll.take() {
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
            self.invalidate_ped_grip_motion(id);
        }
        Ok(())
    }

    pub(crate) fn sync_ped_ragdoll_poses(&mut self) -> Result<(), String> {
        let Some(physics) = self.physics.as_ref() else {
            return Ok(());
        };
        for body in self.ped_physics.bodies.values_mut() {
            let Some(ragdoll) = &mut body.endorphin.ragdoll else {
                continue;
            };
            if ragdoll.refresh_palette(|entity| {
                physics.vehicle_body_state(entity).map(|p| BodyPose {
                    position: p.position,
                    rotation: p.rotation,
                    linear_velocity: p.linear_velocity,
                    angular_velocity: p.angular_velocity,
                })
            }) {
                self.scene
                    .set_entity_physical_pose(body.entity, ragdoll.palette())?;
            }
        }
        Ok(())
    }

    pub(crate) fn ragdoll_snapshot(&self, body: &PedBody) -> Option<Value> {
        let ragdoll = body.endorphin.ragdoll.as_ref()?;
        let physics = self.physics.as_ref()?;
        let parts:Vec<_>=ragdoll.parts.iter().map(|part| {
            let pose=physics.vehicle_body_state(part.entity);
            json!({"name":part.name,"joint":part.joint,"entity_key":part.entity.to_string(),"mass_kg":part.mass_kg,"capacity":body.endorphin.damage.part_capacity(&part.name),
                "position":pose.map(|p|p.position),"rotation":pose.map(|p|p.rotation),
                "linear_velocity":pose.map(|p|p.linear_velocity),"angular_velocity":pose.map(|p|p.angular_velocity),
                "joint_position":pose.map(|p|math::position(math::product(math::matrix(p.position,p.rotation),part.body_to_joint)))})
        }).collect();
        let joints:Vec<_>=ragdoll.joints.iter().filter_map(|joint| {
            let a=physics.vehicle_body_state(joint.parent)?;let b=physics.vehicle_body_state(joint.child)?;
            let ap=math::add(a.position,math::rotate(a.rotation,joint.parent_anchor));
            let bp=math::add(b.position,math::rotate(b.rotation,joint.child_anchor));
            Some(json!({"id_key":joint.id.to_string(),"parent_key":joint.parent.to_string(),"child_key":joint.child.to_string(),
                "separation":math::length(math::sub(ap,bp)),
                "angular_limits":physics.joint_state(joint.id).and_then(|j|j.angular_limits),
                "initial_rotation":joint.initial_rotation,
                "relative_rotation":math::multiply(math::inverse(math::multiply(a.rotation,joint.parent_frame_rotation)),math::multiply(b.rotation,joint.child_frame_rotation)),
                "motor":physics.joint_state(joint.id).and_then(|j|j.motor),"parent_anchor":ap,"child_anchor":bp}))
        }).collect();
        Some(
            json!({"active":true,"elapsed_seconds":ragdoll.elapsed,"parts":parts,"joints":joints,
            "physical_skeleton":self.scene.entity_has_physical_pose(body.entity),
            "skeleton_integrity":self.scene.entity_physical_skeleton_integrity(body.entity),
            "system":"Endorphin","pose_updates":ragdoll.pose_updates,"pose_skips":ragdoll.pose_skips}),
        )
    }

    fn build_ped_ragdoll(
        &self,
        id: &str,
        definition: &RigDefinition,
    ) -> Result<Option<newviso_endorphin::RigPlan>, String> {
        let body = &self.ped_physics.bodies[id];
        let physics = self.physics.as_ref().expect("ragdoll physics");
        let Some(source) = physics.vehicle_body_state(body.entity) else {
            return Ok(None);
        };
        let mut matrices = BTreeMap::new();
        let mut reference_matrices = BTreeMap::new();
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
                    let Ok(m) = self.scene.entity_joint_world_matrix(body.entity, joint) else {
                        return Ok(None);
                    };
                    let reference = self
                        .scene
                        .entity_joint_bind_world_matrix(body.entity, joint)?;
                    reference_matrices.insert(joint.clone(), reference);
                    matrices.insert(joint.clone(), m);
                }
            }
        }
        let Some(presentation) = self.world_presentation_states.get(id) else {
            return Ok(None);
        };
        let entities: Vec<_> = definition
            .parts
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i == 0 {
                    body.entity
                } else {
                    crate::application_peds::actor_hash(&format!("physics.ragdoll.{id}.{}", p.name))
                        & !(1u64 << 63)
                }
            })
            .collect();
        definition
            .build(
                &body.endorphin.model.ragdoll,
                RigInput {
                    matrices: &matrices,
                    reference_matrices: Some(&reference_matrices),
                    anatomy: body
                        .endorphin
                        .model
                        .anatomy
                        .as_ref()
                        .filter(|a| a.validate(definition).is_ok()),
                    entities: &entities,
                    source: BodyPose {
                        position: source.position,
                        rotation: source.rotation,
                        linear_velocity: source.linear_velocity,
                        angular_velocity: source.angular_velocity,
                    },
                    source_radius: body.radius,
                    source_half_height: body.half_height,
                    source_mass_kg: body.mass_kg,
                    visual_position: presentation.position,
                    visual_heading: presentation.rotation_degrees[1],
                },
            )
            .map(Some)
    }
}
