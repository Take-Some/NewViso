use super::*;
impl PhysicsRuntime {
    pub(super) fn activate_scene_hinge(
        &mut self,
        spec: &Spec,
        source: &newviso_scene::ScenePhysicsSource,
        g: &Geometry,
        scene: &mut newviso_scene::Scene3dRuntime,
    ) -> Result<(), String> {
        if self.bodies.contains_key(&spec.entity)
            || self.bodies.contains_key(&spec.anchor_entity)
            || self.streamed_colliders.contains_key(&spec.entity)
            || spec.joint_ids.iter().any(|id| self.joints.contains_key(id))
        {
            return Err("scene hinge activation would replace an existing physics owner".into());
        }
        let h = g.half_extents;
        let inertia = [
            spec.mass * (h[1] * h[1] + h[2] * h[2]) / 3.0,
            spec.mass * (h[0] * h[0] + h[2] * h[2]) / 3.0,
            spec.mass * (h[0] * h[0] + h[1] * h[1]) / 3.0,
        ];
        let hinge = spec.hinge(g.child_pivot, g.span);
        let result = (|| {
            self.upsert_body_from_script(&json!({"entity":spec.entity,"body_kind":"dynamic","position":g.center,"rotation":g.rotation,
                "shape":{"kind":"box","half_extents":h},"friction":spec.friction,"restitution":spec.restitution,
                "linear_damping":spec.linear_damping,"angular_damping":spec.angular_damping,"continuous_collision":true,"participates_in_queries":true,"casts_contacts":true,
                "mass_properties":{"mass":spec.mass,"center_of_mass":[0,0,0],"inertia_diagonal":inertia}}),0)?;
            self.upsert_body_from_script(&json!({"entity":spec.anchor_entity,"body_kind":"static","position":g.hinge_position,"rotation":g.rotation,
                "shape":{"kind":"sphere","radius":h.into_iter().fold(f32::INFINITY,f32::min)},
                "participates_in_queries":false,"casts_contacts":false}),0)?;
            self.upsert_hinge_from_script(
                &serde_json::to_value(hinge).map_err(|e| e.to_string())?,
            )?;
            self.scene_pose_offsets.insert(spec.entity, g.visual_offset);
            self.scene_rotation_offsets.remove(&spec.entity);
            scene.set_physics_ownership(spec.entity, true, false)?;
            Ok::<(), String>(())
        })();
        if result.is_err() {
            self.destroy_body_from_script(&json!({"entity":spec.entity}), 0)?;
            self.destroy_body_from_script(&json!({"entity":spec.anchor_entity}), 0)?;
            scene.restore_physics_source(source)?;
        }
        result
    }
    pub(crate) fn unbind_scene_hinge(
        &mut self,
        id: u64,
        scene: &mut newviso_scene::Scene3dRuntime,
    ) -> Result<(), String> {
        if let Some(b) = self.scene_hinges.entries.remove(&id) {
            self.cleanup_scene_hinge(&b, scene)?;
        }
        Ok(())
    }
    pub(super) fn cleanup_scene_hinge(
        &mut self,
        b: &Binding,
        scene: &mut newviso_scene::Scene3dRuntime,
    ) -> Result<(), String> {
        if b.geometry.is_some() {
            self.destroy_body_from_script(&json!({"entity":b.spec.entity}), 0)?;
            self.destroy_body_from_script(&json!({"entity":b.spec.anchor_entity}), 0)?;
            if let Some(source) = &b.source {
                if scene.physics_source_exists(b.spec.entity) {
                    scene.restore_physics_source(source)?;
                }
            }
        }
        Ok(())
    }
}
