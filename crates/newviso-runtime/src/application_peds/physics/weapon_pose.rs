//! Shared NPC weapon geometry. Authored bindings supply names; installed bones supply dimensions.
use super::*;
use newviso_endorphin::math;

impl EngineApplication {
    pub(crate) fn refresh_ped_skeleton_binding(&mut self, id: &str) {
        let Some(body) = self.ped_physics.bodies.get(id) else {
            return;
        };
        let Some(skeleton) = self.scene.entity_skeleton(body.entity).cloned() else {
            return;
        };
        if body
            .endorphin
            .bound_skeleton
            .as_ref()
            .is_some_and(|old| std::sync::Arc::ptr_eq(old, &skeleton))
        {
            return;
        }
        let error = (|| -> Result<(), String> {
            let rig = body
                .endorphin
                .model
                .weapon_rig
                .as_ref()
                .ok_or("weapon rig is not authored")?;
            let index = |name: &str| {
                skeleton
                    .joints
                    .iter()
                    .position(|j| j.name.eq_ignore_ascii_case(name))
                    .ok_or_else(|| {
                        format!(
                            "skeleton '{}' has no weapon joint '{}'",
                            skeleton.name, name
                        )
                    })
            };
            let descendant = |mut child: usize, parent: usize| {
                for _ in 0..skeleton.joints.len() {
                    let Some(p) = skeleton.joints[child].parent else {
                        return false;
                    };
                    if p as usize == parent {
                        return true;
                    }
                    child = p as usize;
                }
                false
            };
            index(&rig.eye_joint)?;
            for a in [&rig.right, &rig.left] {
                let u = index(&a.upper_joint)?;
                let e = index(&a.root_joint)?;
                let h = index(&a.tip_joint)?;
                let s = index(&a.weapon_joint)?;
                if !descendant(e, u) || !descendant(h, e) || (s != h && !descendant(s, h)) {
                    return Err(
                        "weapon binding requires ordered shoulder/elbow/wrist/socket descendants"
                            .into(),
                    );
                }
                for g in a.grips.values() {
                    for f in &g.joint_rotations {
                        if !descendant(index(&f.joint)?, h) {
                            return Err(format!("grip joint '{}' is outside its hand", f.joint));
                        }
                    }
                }
            }
            Ok(())
        })()
        .err();
        let state = &mut self
            .ped_physics
            .bodies
            .get_mut(id)
            .expect("resident skeleton")
            .endorphin;
        state.bound_skeleton = Some(skeleton);
        state.weapon_binding_error = error;
    }
    pub(crate) fn ped_joint_positions(&self, body: &PedBody) -> Value {
        let names: std::collections::BTreeSet<_> = body
            .endorphin
            .model
            .rig
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
        let matrices = self
            .scene
            .entity_joint_world_matrices(body.entity, &names.into_iter().collect::<Vec<_>>());
        json!(matrices
            .unwrap_or_default()
            .into_iter()
            .map(|(name, m)| (name, math::position(m)))
            .collect::<BTreeMap<_, _>>())
    }
    pub(crate) fn ped_weapon_forward(&self, id: &str) -> Option<[f32; 3]> {
        let binding = self.world_actor_presentations.get(id)?;
        let state = self.world_presentation_states.get(id)?;
        let yaw = (state.rotation_degrees[1] - binding.rotation_degrees[1]).to_radians();
        Some([yaw.sin(), 0.0, yaw.cos()])
    }
    pub(crate) fn ped_weapon_pose(&self, id: &str, body: &PedBody) -> Value {
        if let Some(error) = &body.endorphin.weapon_binding_error {
            return json!({"ready":false,"error":error,"rig_asset":body.endorphin.model_asset});
        }
        let Some(rig) = body.endorphin.model.weapon_rig.as_ref() else {
            return json!({"ready":false,"error":"weapon rig is not authored"});
        };
        let forward = self.ped_weapon_forward(id);
        let matrices = match self
            .scene
            .entity_joint_world_matrices(body.entity, &rig.joints())
        {
            Ok(v) => v,
            Err(error) => {
                return json!({"ready":false,"error":error,"rig_asset":body.endorphin.model_asset,"forward":forward})
            }
        };
        let point = |name: &str| math::position(matrices[name]);
        let arm = |a: &newviso_endorphin::ArmBinding| {
            let shoulder = point(&a.upper_joint);
            let elbow = point(&a.root_joint);
            let wrist = point(&a.tip_joint);
            json!({"hand":a.tip_joint,"tip":a.weapon_joint,"shoulder":shoulder,"elbow":elbow,
                "wrist":wrist,"position":point(&a.weapon_joint),
                "reach":math::length(math::sub(elbow,shoulder))+math::length(math::sub(wrist,elbow))})
        };
        json!({"ready":forward.is_some(),"rig_asset":body.endorphin.model_asset,"forward":forward,
            "eye":point(&rig.eye_joint),"arms":[arm(&rig.right),arm(&rig.left)],
            "muzzle":self.ped_weapon_ray(id).map(|(origin,direction)|json!({"origin":origin,"direction":direction}))})
    }
    /// Read the equipped prop's real sight ray, after skeleton and attachment solving.
    pub(crate) fn ped_weapon_ray(&self, id: &str) -> Option<([f32; 3], [f32; 3])> {
        let body = self.ped_physics.bodies.get(id)?;
        let item = self.items.equipped_item(id, "hands")?;
        let metadata = &self.items.definition(item)?.metadata;
        let presentation = metadata.get("presentation")?;
        let expected = presentation.get("model")?.as_str()?;
        let muzzle = presentation
            .get("muzzle_joint")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty());
        let point: Option<[f32; 3]> = presentation
            .get("muzzle_point")
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        let sights = presentation.get("aim")?.get("sights")?;
        let rear: [f32; 3] = serde_json::from_value(sights.get("rear")?.clone()).ok()?;
        let front: [f32; 3] = serde_json::from_value(sights.get("front")?.clone()).ok()?;
        let offset: [f32; 3] = presentation
            .get("muzzle_joint_offset")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or([0.0; 3]);
        let transform = |m: [f32; 16], p: [f32; 3]| {
            std::array::from_fn(|i| m[12 + i] + m[i] * p[0] + m[4 + i] * p[1] + m[8 + i] * p[2])
        };
        self.scene
            .joint_attachment_children(body.entity)
            .into_iter()
            .find_map(|weapon| {
                if !self.scene.entity_has_resident_asset(weapon, expected) {
                    return None;
                }
                let world = self.scene.entity_transform_matrix(weapon)?;
                let origin = if let Some(joint) = muzzle {
                    transform(
                        self.scene.entity_joint_world_matrix(weapon, joint).ok()?,
                        offset,
                    )
                } else {
                    transform(world, point?)
                };
                let direction = math::sub(transform(world, front), transform(world, rear));
                let n = math::length(direction);
                (n > f32::EPSILON).then(|| (origin, direction.map(|v| v / n)))
            })
    }
    pub(crate) fn ped_can_aim_weapon(&self, id: &str, direction: [f32; 3]) -> bool {
        let Some(body) = self.ped_physics.bodies.get(id) else {
            return true;
        };
        body.endorphin.weapon_binding_error.is_none()
            && body
                .endorphin
                .model
                .weapon_rig
                .as_ref()
                .zip(self.ped_weapon_forward(id))
                .is_some_and(|(rig, forward)| rig.can_aim(forward, direction))
    }
}
