//! Scene-owned character capability and lifecycle, shared by authored/runtime entities.
use super::*;
pub use newviso_scene_document::SceneCharacterBinding;
impl Scene3dRuntime {
    pub fn character_bindings(&self) -> &BTreeMap<u64, std::sync::Arc<SceneCharacterBinding>> {
        &self.character_bindings
    }
    pub fn character_entity_is_active(&self, id: u64) -> bool {
        self.world.entity(SceneEntityId(id)).is_some_and(|e| {
            e.lifecycle == SceneLifecycle::Active && e.residency == SceneResidency::Resident
        })
    }
    pub fn character_bindings_snapshot(&self) -> serde_json::Value {
        serde_json::json!(self.character_bindings.iter().map(|(id,b)|{
   let body=b.body_entity.or_else(||b.body_id.as_deref().and_then(|name|self.runtime_entity_stable_id(name))).or_else(||b.body_id.is_none().then_some(*id));
   serde_json::json!({"entity_key":id.to_string(),"id":self.world.entity(SceneEntityId(*id)).map(|e|e.name.as_str()),
    "body_key":body.map(|v|v.to_string()),"binding":b.as_ref(),
    "active":self.character_entity_is_active(*id),"physical":self.entity_has_physical_pose(*id),
    "position":self.entity_transform_values(*id).map(|v|v.0)})
  }).collect::<Vec<_>>())
    }
    pub fn set_entity_character_binding(
        &mut self,
        id: u64,
        binding: Option<SceneCharacterBinding>,
    ) -> Result<(), String> {
        if !self.physics_source_exists(id) {
            return Err("character entity does not exist".into());
        }
        if let Some(value) = &binding {
            value.validate()?;
        }
        self.leg_ik_constraints.remove(&id);
        self.set_entity_ground_pose_offset(id, [0.0; 3])?;
        if let Some(value) = binding {
            self.character_bindings
                .insert(id, std::sync::Arc::new(value));
        } else {
            self.character_bindings.remove(&id);
        }
        Ok(())
    }
}
