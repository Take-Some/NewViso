//! Existing scene meshes become physical hinged objects without replacement visuals.
use super::*;
mod lifecycle;
mod model;
#[cfg(test)]
mod tests;
pub(crate) use model::SceneHingeBindings;
use model::*;
impl PhysicsRuntime {
    pub(crate) fn bind_scene_hinge(&mut self, value: &Value) -> Result<(), String> {
        if !self.supports_articulated_drives() {
            return Err("scene hinges require native articulation capabilities".into());
        }
        let spec: Spec = serde_json::from_value(value.clone())
            .map_err(|e| format!("invalid scene hinge: {e}"))?;
        spec.validate()?;
        if let Some(old) = self.scene_hinges.entries.get(&spec.entity) {
            if old.spec == spec {
                return Ok(());
            }
            return Err("unbind the existing scene hinge before changing its definition".into());
        }
        if self.bodies.contains_key(&spec.entity)
            || self.bodies.contains_key(&spec.anchor_entity)
            || self.joints.keys().any(|id| spec.joint_ids.contains(id))
            || self.scene_hinges.entries.values().any(|old| {
                old.spec.anchor_entity == spec.anchor_entity
                    || old.spec.anchor_entity == spec.entity
                    || old.spec.entity == spec.anchor_entity
                    || old
                        .spec
                        .joint_ids
                        .iter()
                        .any(|id| spec.joint_ids.contains(id))
            })
        {
            return Err("scene hinge body or joint IDs already belong to another owner".into());
        }
        self.scene_hinges.entries.insert(
            spec.entity,
            Binding {
                spec,
                source: None,
                geometry: None,
                error: None,
            },
        );
        Ok(())
    }
    pub(crate) fn update_scene_hinges(
        &mut self,
        scene: &mut newviso_scene::Scene3dRuntime,
    ) -> Result<(), String> {
        let ids = self
            .scene_hinges
            .entries
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for id in ids {
            let alive = scene.physics_source_exists(id);
            if !alive {
                self.unbind_scene_hinge(id, scene)?;
                continue;
            }
            let mut b = self.scene_hinges.entries.remove(&id).unwrap();
            if b.geometry.is_some() {
                if !self.bodies.contains_key(&id)
                    || !self.bodies.contains_key(&b.spec.anchor_entity)
                {
                    self.cleanup_scene_hinge(&b, scene)?;
                    continue;
                }
            } else if b.error.is_none() {
                match scene.physics_source(id)? {
                    Some(source) => {
                        b.source = Some(source.clone());
                        if source.local_bounds.is_some() {
                            match Geometry::from_source(&source, &b.spec).and_then(|g| {
                                self.activate_scene_hinge(&b.spec, &source, &g, scene)
                                    .map(|_| g)
                            }) {
                                Ok(g) => b.geometry = Some(g),
                                Err(e) => b.error = Some(e),
                            }
                        }
                    }
                    None => {
                        self.cleanup_scene_hinge(&b, scene)?;
                        continue;
                    }
                }
            }
            self.scene_hinges.entries.insert(id, b);
        }
        Ok(())
    }
    pub(crate) fn scene_hinges_snapshot(&self) -> Value {
        json!({"bindings":self.scene_hinges.entries.iter().map(|(id,b)|{
            let g=&b.geometry;
            let separation=g.as_ref().and_then(|g|self.bodies.get(id).map(|body|{
                let p=rotate_camera_vector(g.child_pivot,body.rotation);
                (0..3).map(|i|(body.position[i]+p[i]-g.hinge_position[i]).powi(2)).sum::<f32>().sqrt()
            }));
            json!({"entity":id,"entity_key":id.to_string(),"name":b.source.as_ref().map(|s|&s.name),
                "asset":b.source.as_ref().and_then(|s|s.asset_ref.as_deref()),"status":if b.error.is_some(){"error"}else if g.is_some(){"active"}else{"pending_geometry"},
                "error":b.error,"geometry":g,"source_position":b.source.as_ref().map(|s|s.position),
                "source_rotation_degrees":b.source.as_ref().map(|s|s.rotation_degrees),
                "source_scale":b.source.as_ref().map(|s|s.scale),"model_bounds":b.source.as_ref().and_then(|s|s.local_bounds),
                "anchor_entity":b.spec.anchor_entity,"joint_ids":b.spec.joint_ids,"anchor_separation":separation})
        }).collect::<Vec<_>>()})
    }
}
