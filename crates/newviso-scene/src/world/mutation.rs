use super::*;

impl SceneWorld {
    pub(crate) fn add_entity(&mut self, mut entity: SceneEntity) -> Result<(), String> {
        if self.by_id.contains_key(&entity.id) {
            return Err(format!("duplicate scene entity id {}", entity.id.0));
        }
        entity.lifecycle = SceneLifecycle::Added;
        let index = self.entities.len();
        let parent = entity.parent;
        let id = entity.id;
        self.entities.push(entity);
        self.by_id.insert(id, index);

        if let Some(parent_id) = parent {
            let parent_index = *self
                .by_id
                .get(&parent_id)
                .ok_or_else(|| format!("scene parent {} not found for {}", parent_id.0, id.0))?;
            self.entities[parent_index].children.push(id);
        }
        Ok(())
    }
    pub(crate) fn activate_all(&mut self) {
        for entity in &mut self.entities {
            if entity.lifecycle == SceneLifecycle::Added
                || entity.lifecycle == SceneLifecycle::Dormant
            {
                entity.lifecycle = SceneLifecycle::Active;
            }
        }
    }
    pub(crate) fn request_remove(&mut self, id: SceneEntityId) -> Result<(), String> {
        let entity = self
            .entity_mut(id)
            .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
        if entity.lifecycle != SceneLifecycle::Removed {
            entity.lifecycle = SceneLifecycle::PendingRemove;
        }
        Ok(())
    }
    pub(crate) fn flush_removals(&mut self) {
        for entity in &mut self.entities {
            if entity.lifecycle == SceneLifecycle::PendingRemove {
                entity.lifecycle = SceneLifecycle::Removed;
            }
        }
    }
    pub(crate) fn entity(&self, id: SceneEntityId) -> Option<&SceneEntity> {
        self.by_id
            .get(&id)
            .and_then(|index| self.entities.get(*index))
    }
    pub(crate) fn entity_mut(&mut self, id: SceneEntityId) -> Option<&mut SceneEntity> {
        let index = *self.by_id.get(&id)?;
        self.entities.get_mut(index)
    }
    pub(crate) fn set_light(
        &mut self,
        id: SceneEntityId,
        light: Option<LightComponent>,
    ) -> Result<(), String> {
        let entity = self
            .entity_mut(id)
            .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
        entity.light = light.map(LightComponent::validate).transpose()?;
        if entity.light.is_some() {
            entity.kind = SceneEntityKind::Light;
        }
        Ok(())
    }
    pub(crate) fn active_lights(&self) -> Vec<(SceneEntityId, SceneTransform, LightComponent)> {
        let mut lights = self
            .entities
            .iter()
            .filter_map(|entity| {
                if entity.lifecycle == SceneLifecycle::Active
                    && entity.visibility.all_visible()
                    && entity.residency == SceneResidency::Resident
                {
                    entity
                        .light
                        .map(|light| (entity.id, entity.transform, light))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        lights.sort_by_key(|(id, _, _)| id.0);
        lights
    }
    pub(crate) fn entity_id_by_name(&self, name: &str) -> Option<SceneEntityId> {
        self.entities
            .iter()
            .find(|entity| entity.lifecycle != SceneLifecycle::Removed && entity.name == name)
            .map(|entity| entity.id)
    }
    pub(crate) fn update_transform(
        &mut self,
        id: SceneEntityId,
        transform: SceneTransform,
    ) -> Result<(), String> {
        let entity = self
            .entity_mut(id)
            .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
        entity.transform = transform;
        Ok(())
    }
    pub(crate) fn update_spatial(
        &mut self,
        id: SceneEntityId,
        transform: SceneTransform,
        bounds: SceneBounds,
    ) -> Result<(), String> {
        let entity = self
            .entity_mut(id)
            .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
        entity.transform = transform;
        entity.bounds = bounds;
        Ok(())
    }
    pub(crate) fn set_parent(
        &mut self,
        child_id: SceneEntityId,
        parent_id: Option<SceneEntityId>,
    ) -> Result<(), String> {
        if parent_id == Some(child_id) {
            return Err(format!("scene entity {} cannot parent itself", child_id.0));
        }
        if self.entity(child_id).is_none() {
            return Err(format!("scene child entity {} does not exist", child_id.0));
        }
        if let Some(parent_id) = parent_id {
            if self.entity(parent_id).is_none() {
                return Err(format!(
                    "scene parent entity {} does not exist",
                    parent_id.0
                ));
            }
            let mut cursor = Some(parent_id);
            while let Some(id) = cursor {
                if id == child_id {
                    return Err(format!(
                        "scene hierarchy cycle child={} parent={}",
                        child_id.0, parent_id.0
                    ));
                }
                cursor = self.entity(id).and_then(|entity| entity.parent);
            }
        }

        let old_parent = self.entity(child_id).and_then(|entity| entity.parent);
        if let Some(old_parent) = old_parent {
            if let Some(parent) = self.entity_mut(old_parent) {
                parent.children.retain(|id| *id != child_id);
            }
        }

        self.entity_mut(child_id)
            .expect("child existence checked")
            .parent = parent_id;
        if let Some(parent_id) = parent_id {
            let parent = self
                .entity_mut(parent_id)
                .expect("parent existence checked");
            if !parent.children.contains(&child_id) {
                parent.children.push(child_id);
            }
        }
        Ok(())
    }
    pub(crate) fn set_visibility(
        &mut self,
        id: SceneEntityId,
        module: VisibilityModule,
        visible: bool,
    ) -> Result<(), String> {
        let entity = self
            .entity_mut(id)
            .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
        entity.visibility.set(module, visible);
        Ok(())
    }
    pub(crate) fn set_residency(
        &mut self,
        id: SceneEntityId,
        residency: SceneResidency,
    ) -> Result<(), String> {
        let entity = self
            .entity_mut(id)
            .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
        if entity.asset_ref.is_none() && residency != SceneResidency::Resident {
            return Err(format!(
                "scene entity {} has no asset_ref and cannot enter {:?}",
                id.0, residency
            ));
        }
        entity.residency = residency;
        Ok(())
    }
    pub(crate) fn set_focus_camera(&mut self) {
        self.focus.source = SceneFocusSource::Camera;
    }
    pub(crate) fn set_focus_entity(&mut self, id: SceneEntityId) -> Result<(), String> {
        if self.entity(id).is_none() {
            return Err(format!("scene focus entity {} does not exist", id.0));
        }
        self.focus.source = SceneFocusSource::Entity(id);
        Ok(())
    }
    pub(crate) fn set_focus_override(&mut self, position: Vec3, velocity: Vec3) {
        self.focus = SceneFocus {
            position,
            velocity,
            source: SceneFocusSource::Override,
        };
        self.last_focus_position = position;
    }
}
