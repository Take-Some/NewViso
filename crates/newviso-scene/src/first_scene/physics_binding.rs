//! Bind an existing scene object to physics without replacing its model or identity.
use super::*;
#[derive(Clone, Debug)]
pub struct ScenePhysicsSource {
    pub entity: u64,
    pub name: String,
    pub asset_ref: Option<String>,
    pub position: [f32; 3],
    pub rotation_degrees: [f32; 3],
    pub scale: [f32; 3],
    pub local_bounds: Option<([f32; 3], [f32; 3])>,
    pub dynamic: bool,
    pub solid: bool,
    pub physics_claimed: bool,
}
impl Scene3dRuntime {
    pub fn physics_source_exists(&self, id: u64) -> bool {
        self.world.entity(SceneEntityId(id)).is_some_and(|e| {
            !matches!(
                e.lifecycle,
                SceneLifecycle::Removed | SceneLifecycle::PendingRemove
            )
        })
    }
    pub fn physics_source(&self, id: u64) -> Result<Option<ScenePhysicsSource>, String> {
        let Some(e) = self.world.entity(SceneEntityId(id)) else {
            return Ok(None);
        };
        if matches!(
            e.lifecycle,
            SceneLifecycle::Removed | SceneLifecycle::PendingRemove
        ) {
            return Ok(None);
        }
        if !matches!(
            e.kind,
            SceneEntityKind::StaticMesh | SceneEntityKind::DynamicMesh
        ) {
            return Err(format!("scene entity {id} is not a mesh"));
        }
        let bounds = self
            .asset_meshes
            .get(&id)
            .map(|m| m.local_bounds)
            .or_else(|| {
                e.render_slot.map(|_| {
                    SceneBounds::from_center_half_extent(Vec3::ZERO, Vec3::new(0.5, 0.5, 0.5))
                })
            });
        Ok(Some(ScenePhysicsSource {
            entity: id,
            name: e.name.clone(),
            asset_ref: e.asset_ref.clone(),
            position: [
                e.transform.position.x,
                e.transform.position.y,
                e.transform.position.z,
            ],
            rotation_degrees: [
                e.transform.rotation_degrees.x,
                e.transform.rotation_degrees.y,
                e.transform.rotation_degrees.z,
            ],
            scale: [
                e.transform.scale.x,
                e.transform.scale.y,
                e.transform.scale.z,
            ],
            local_bounds: bounds
                .map(|b| ([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z])),
            dynamic: e.mobility == SceneMobility::Dynamic,
            solid: e.solid,
            physics_claimed: e
                .process_claims
                .snapshot()
                .get("physics")
                .is_some_and(|owners| owners.iter().any(|o| o == "engine.physics")),
        }))
    }
    pub fn set_physics_ownership(
        &mut self,
        id: u64,
        dynamic: bool,
        solid: bool,
    ) -> Result<(), String> {
        self.world
            .set_solid(SceneEntityId(id), solid, SceneMutationSource::Physics)?;
        self.world.set_mobility(
            SceneEntityId(id),
            if dynamic {
                SceneMobility::Dynamic
            } else {
                SceneMobility::Static
            },
            SceneMutationSource::Physics,
        )?;
        self.set_physics_process_active(id, dynamic)?;
        self.mark_entity_model_transform_dirty(id);
        Ok(())
    }
    pub fn restore_physics_source(&mut self, source: &ScenePhysicsSource) -> Result<(), String> {
        if self.world.entity(SceneEntityId(source.entity)).is_none() {
            return Ok(());
        }
        self.set_runtime_entity_transform(
            &source.name,
            Some(source.position),
            Some(source.rotation_degrees),
            Some(source.scale),
        )?;
        self.set_physics_ownership(source.entity, source.dynamic, source.solid)?;
        self.set_physics_process_active(source.entity, source.physics_claimed)?;
        Ok(())
    }
}
