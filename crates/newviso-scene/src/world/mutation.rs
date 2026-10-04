use super::*;

fn normalize_process_label(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() || value.len() > 96 {
        return Err(format!("scene process {field} must be 1..=96 characters"));
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
    }) {
        return Err(format!(
            "scene process {field} contains unsupported characters: '{value}'"
        ));
    }
    Ok(value)
}

impl SceneWorld {
    fn refresh_process_membership(&mut self, id: SceneEntityId) {
        let active = self.entity(id).is_some_and(|entity| {
            entity.lifecycle == SceneLifecycle::Active
                && entity.residency == SceneResidency::Resident
                && entity.process_claims.active()
        });
        if active {
            self.process_active.insert(id);
        } else {
            self.process_active.remove(&id);
        }
    }

    pub(crate) fn add_entity(&mut self, mut entity: SceneEntity) -> Result<(), String> {
        if self.by_id.contains_key(&entity.id) {
            return Err(format!("duplicate scene entity id {}", entity.id.0));
        }
        entity.lifecycle = SceneLifecycle::Added;
        let index = self.entities.len();
        let parent = entity.parent;
        let id = entity.id;
        let static_renderable = entity.mobility == SceneMobility::Static;
        self.entities.push(entity);
        self.by_id.insert(id, index);
        if static_renderable {
            self.static_render_epoch = self.static_render_epoch.wrapping_add(1);
        }
        self.refresh_process_membership(id);
        self.index_spatial_entity(id);

        if let Some(parent_id) = parent {
            let parent_index = *self
                .by_id
                .get(&parent_id)
                .ok_or_else(|| format!("scene parent {} not found for {}", parent_id.0, id.0))?;
            self.entities[parent_index].children.push(id);
        }
        Ok(())
    }

    fn record_mutation(
        &mut self,
        id: SceneEntityId,
        source: SceneMutationSource,
        dirty: SceneDirtyFlags,
    ) -> Result<(), String> {
        if dirty == SceneDirtyFlags::EMPTY {
            return Ok(());
        }
        let frame = self.frame;
        let process_active = self.process_active.contains(&id);
        let process_reason_mask = self
            .entity(id)
            .map(|entity| entity.process_claims.mask().bits())
            .unwrap_or(0);
        let mutation = {
            let entity = self
                .entity_mut(id)
                .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
            entity.revision = entity.revision.wrapping_add(1).max(1);
            entity.last_mutation_frame = frame;
            SceneMutation {
                entity: id,
                revision: entity.revision,
                frame,
                source,
                dirty,
                transform: entity.transform,
                bounds: entity.bounds,
                process_active,
                process_reason_mask,
                process_claims: entity.process_claims.snapshot(),
            }
        };
        let static_renderable = self
            .entity(id)
            .is_some_and(|entity| entity.mobility == SceneMobility::Static);
        if dirty.contains(SceneDirtyFlags::MOBILITY)
            || (static_renderable
                && (dirty.contains(SceneDirtyFlags::TRANSFORM)
                    || dirty.contains(SceneDirtyFlags::BOUNDS)
                    || dirty.contains(SceneDirtyFlags::VISIBILITY)
                    || dirty.contains(SceneDirtyFlags::HIERARCHY)
                    || dirty.contains(SceneDirtyFlags::LIFECYCLE)
                    || dirty.contains(SceneDirtyFlags::RESIDENCY)))
        {
            self.static_render_epoch = self.static_render_epoch.wrapping_add(1);
        }
        self.mutations.push(mutation);
        Ok(())
    }

    pub(crate) fn static_render_epoch(&self) -> u64 {
        self.static_render_epoch
    }

    pub(crate) fn drain_mutations(&mut self) -> Vec<SceneMutation> {
        std::mem::take(&mut self.mutations)
    }

    pub(crate) fn activate_all(&mut self) {
        for entity in &mut self.entities {
            if entity.lifecycle == SceneLifecycle::Added
                || entity.lifecycle == SceneLifecycle::Dormant
            {
                entity.lifecycle = SceneLifecycle::Active;
            }
        }
        self.process_active = self
            .entities
            .iter()
            .filter(|entity| {
                entity.lifecycle == SceneLifecycle::Active
                    && entity.residency == SceneResidency::Resident
                    && entity.process_claims.active()
            })
            .map(|entity| entity.id)
            .collect();
    }

    pub(crate) fn activate_entity(&mut self, id: SceneEntityId) -> Result<(), String> {
        let changed = {
            let entity = self
                .entity_mut(id)
                .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
            match entity.lifecycle {
                SceneLifecycle::Added | SceneLifecycle::Dormant | SceneLifecycle::Constructed => {
                    entity.lifecycle = SceneLifecycle::Active;
                    true
                }
                SceneLifecycle::Active => false,
                SceneLifecycle::PendingRemove | SceneLifecycle::Removed => {
                    return Err(format!(
                        "scene entity {} cannot be reactivated after removal",
                        id.0
                    ))
                }
            }
        };
        if changed {
            self.refresh_process_membership(id);
            self.reset_process_schedule(id);
            self.record_mutation(id, SceneMutationSource::Engine, SceneDirtyFlags::LIFECYCLE)?;
        }
        Ok(())
    }

    pub(crate) fn set_dormant(&mut self, id: SceneEntityId, dormant: bool) -> Result<(), String> {
        let changed = {
            let entity = self
                .entity_mut(id)
                .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
            if matches!(
                entity.lifecycle,
                SceneLifecycle::PendingRemove | SceneLifecycle::Removed
            ) {
                return Err(format!(
                    "scene entity {} cannot change dormancy while being removed",
                    id.0
                ));
            }
            let target = if dormant {
                SceneLifecycle::Dormant
            } else {
                SceneLifecycle::Active
            };
            if entity.lifecycle == target {
                false
            } else {
                entity.lifecycle = target;
                true
            }
        };
        if changed {
            self.refresh_process_membership(id);
            self.reset_process_schedule(id);
            self.record_mutation(id, SceneMutationSource::Engine, SceneDirtyFlags::LIFECYCLE)?;
        }
        Ok(())
    }

    pub(crate) fn seal_initial_state(&mut self) {
        self.mutations.clear();
        for entity in &mut self.entities {
            entity.revision = 0;
            entity.last_mutation_frame = self.frame;
        }
        self.process_active = self
            .entities
            .iter()
            .filter(|entity| {
                entity.lifecycle == SceneLifecycle::Active
                    && entity.residency == SceneResidency::Resident
                    && entity.process_claims.active()
            })
            .map(|entity| entity.id)
            .collect();
        self.process_last_due_seconds.clear();
        self.process_tickets.clear();
        self.static_render_epoch = 0;
    }

    pub(crate) fn request_remove(&mut self, id: SceneEntityId) -> Result<(), String> {
        let (changed, cleared_process) = {
            let entity = self
                .entity_mut(id)
                .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
            if entity.lifecycle == SceneLifecycle::Removed
                || entity.lifecycle == SceneLifecycle::PendingRemove
            {
                (false, false)
            } else {
                entity.lifecycle = SceneLifecycle::PendingRemove;
                let cleared_process = entity.process_claims.clear();
                (true, cleared_process)
            }
        };
        if changed {
            self.refresh_process_membership(id);
            self.reset_process_schedule(id);
            self.process_rates_hz.retain(|(entity, _), _| *entity != id);
            self.remove_spatial_entity(id);
            let dirty = if cleared_process {
                SceneDirtyFlags::LIFECYCLE.union(SceneDirtyFlags::PROCESS_CONTROL)
            } else {
                SceneDirtyFlags::LIFECYCLE
            };
            self.record_mutation(id, SceneMutationSource::Engine, dirty)?;
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

    pub(crate) fn set_mobility(
        &mut self,
        id: SceneEntityId,
        mobility: SceneMobility,
        source: SceneMutationSource,
    ) -> Result<bool, String> {
        let changed = {
            let entity = self
                .entity_mut(id)
                .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
            if entity.mobility == mobility {
                false
            } else {
                entity.mobility = mobility;
                if matches!(
                    entity.kind,
                    SceneEntityKind::StaticMesh | SceneEntityKind::DynamicMesh
                ) {
                    entity.kind = match mobility {
                        SceneMobility::Static => SceneEntityKind::StaticMesh,
                        SceneMobility::Dynamic => SceneEntityKind::DynamicMesh,
                    };
                }
                true
            }
        };
        if changed {
            self.record_mutation(id, source, SceneDirtyFlags::MOBILITY)?;
        }
        Ok(changed)
    }

    pub(crate) fn apply_damage(
        &mut self,
        id: SceneEntityId,
        direct_damage: f32,
        contact_impulse: f32,
    ) -> Result<Option<SceneDamageOutcome>, String> {
        if !direct_damage.is_finite() || direct_damage < 0.0 {
            return Err("scene direct damage must be finite and non-negative".to_owned());
        }
        if !contact_impulse.is_finite() || contact_impulse < 0.0 {
            return Err("scene contact impulse must be finite and non-negative".to_owned());
        }

        let outcome = {
            let Some(entity) = self.entity_mut(id) else {
                return Ok(None);
            };
            let Some(destructible) = entity.destructible.as_mut() else {
                return Ok(None);
            };
            if destructible.broken {
                return Ok(Some(SceneDamageOutcome {
                    applied_damage: 0.0,
                    remaining_health: destructible.health,
                    broke_now: false,
                }));
            }

            let impact_damage = (contact_impulse - destructible.impact_damage_threshold).max(0.0)
                * destructible.impact_damage_scale;
            let applied_damage = direct_damage + impact_damage;
            if applied_damage > 0.0 {
                destructible.health = (destructible.health - applied_damage).max(0.0);
            }
            let broke_now =
                destructible.health <= 0.0 || contact_impulse >= destructible.break_impulse;
            if broke_now {
                destructible.health = 0.0;
                destructible.broken = true;
            }
            SceneDamageOutcome {
                applied_damage,
                remaining_health: destructible.health,
                broke_now,
            }
        };

        if outcome.applied_damage > 0.0 || outcome.broke_now {
            self.record_mutation(id, SceneMutationSource::Physics, SceneDirtyFlags::DAMAGE)?;
        }
        if outcome.broke_now {
            self.set_mobility(id, SceneMobility::Dynamic, SceneMutationSource::Physics)?;
        }
        Ok(Some(outcome))
    }

    pub(crate) fn set_light(
        &mut self,
        id: SceneEntityId,
        light: Option<LightComponent>,
    ) -> Result<(), String> {
        let light = light.map(LightComponent::validate).transpose()?;
        let changed = {
            let entity = self
                .entity_mut(id)
                .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
            if entity.light == light {
                false
            } else {
                entity.light = light;
                if entity.light.is_some() {
                    entity.kind = SceneEntityKind::Light;
                }
                true
            }
        };
        if changed {
            self.record_mutation(id, SceneMutationSource::Engine, SceneDirtyFlags::LIGHT)?;
        }
        Ok(())
    }

    pub(crate) fn set_process_claim(
        &mut self,
        id: SceneEntityId,
        owner: &str,
        reason: &str,
        active: bool,
        source: SceneMutationSource,
    ) -> Result<bool, String> {
        let owner = normalize_process_label(owner, "owner")?;
        let reason = normalize_process_label(reason, "reason")?;
        let changed = {
            let entity = self
                .entity_mut(id)
                .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
            if matches!(
                entity.lifecycle,
                SceneLifecycle::PendingRemove | SceneLifecycle::Removed
            ) {
                return Err(format!(
                    "scene entity {} cannot change process claims while being removed",
                    id.0
                ));
            }
            entity.process_claims.set(owner, reason, active)
        };
        if !changed {
            return Ok(false);
        }

        self.refresh_process_membership(id);
        self.reset_process_schedule(id);
        self.record_mutation(id, source, SceneDirtyFlags::PROCESS_CONTROL)?;
        Ok(true)
    }

    pub(crate) fn set_process_rate_hz(
        &mut self,
        id: SceneEntityId,
        reason: &str,
        hz: f32,
    ) -> Result<(), String> {
        if self.entity(id).is_none() {
            return Err(format!("scene entity {} does not exist", id.0));
        }
        if !hz.is_finite() || !(0.0..=1_000.0).contains(&hz) {
            return Err("scene process rate must be finite and within 0..=1000 Hz".to_owned());
        }
        let reason = normalize_process_label(reason, "reason")?;
        let mask = SceneProcessReasons::from_reason_label(&reason);
        if mask == SceneProcessReasons::CUSTOM {
            return Err(format!(
                "scene process rate requires a typed reason, got '{reason}'"
            ));
        }
        if mask == SceneProcessReasons::PHYSICS && hz != 0.0 {
            return Err(
                "physics process cadence is wake/sleep driven; use 0 Hz to clear overrides"
                    .to_owned(),
            );
        }

        let key = (id, mask.bits());
        if hz == 0.0 {
            self.process_rates_hz.remove(&key);
        } else {
            self.process_rates_hz.insert(key, hz);
        }
        self.reset_process_schedule(id);
        Ok(())
    }

    pub(crate) fn process_rate_hz(&self, id: SceneEntityId, reason: SceneProcessReasons) -> f32 {
        self.process_rates_hz
            .get(&(id, reason.bits()))
            .copied()
            .unwrap_or(0.0)
    }

    pub(crate) fn process_rate_snapshot(&self, id: SceneEntityId) -> BTreeMap<&'static str, f32> {
        let mut rates = BTreeMap::new();
        for reason in SceneProcessReasons::ALL {
            let hz = self.process_rate_hz(id, reason);
            if hz > 0.0 {
                let label = reason.labels().into_iter().next().unwrap_or("custom");
                rates.insert(label, hz);
            }
        }
        rates
    }

    pub(crate) fn reset_process_schedule(&mut self, id: SceneEntityId) {
        self.process_last_due_seconds
            .retain(|(entity, _), _| *entity != id);
    }

    pub(crate) fn process_is_active(&self, id: SceneEntityId) -> bool {
        self.process_active.contains(&id)
    }

    pub(crate) fn process_active_count(&self) -> usize {
        self.process_active.len()
    }

    pub(crate) fn process_active_ids(&self) -> Vec<SceneEntityId> {
        self.process_active.iter().copied().collect()
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
        self.update_transform_from(id, transform, SceneMutationSource::Engine)
    }

    pub(crate) fn update_transform_from(
        &mut self,
        id: SceneEntityId,
        transform: SceneTransform,
        source: SceneMutationSource,
    ) -> Result<(), String> {
        let changed = {
            let entity = self
                .entity_mut(id)
                .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
            if entity.transform == transform {
                false
            } else {
                entity.transform = transform;
                true
            }
        };
        if changed {
            self.record_mutation(id, source, SceneDirtyFlags::TRANSFORM)?;
        }
        Ok(())
    }

    pub(crate) fn update_spatial_from(
        &mut self,
        id: SceneEntityId,
        transform: SceneTransform,
        bounds: SceneBounds,
        source: SceneMutationSource,
    ) -> Result<(), String> {
        let mut dirty = SceneDirtyFlags::EMPTY;
        {
            let entity = self
                .entity_mut(id)
                .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
            if entity.transform != transform {
                entity.transform = transform;
                dirty = dirty.union(SceneDirtyFlags::TRANSFORM);
            }
            if entity.bounds != bounds {
                entity.bounds = bounds;
                dirty = dirty.union(SceneDirtyFlags::BOUNDS);
            }
        }
        if dirty.contains(SceneDirtyFlags::BOUNDS) || dirty.contains(SceneDirtyFlags::TRANSFORM) {
            self.index_spatial_entity(id);
        }
        self.record_mutation(id, source, dirty)
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
        if old_parent == parent_id {
            return Ok(());
        }
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
        self.record_mutation(
            child_id,
            SceneMutationSource::Parent,
            SceneDirtyFlags::HIERARCHY,
        )
    }

    pub(crate) fn set_visibility(
        &mut self,
        id: SceneEntityId,
        channel: &str,
        visible: bool,
    ) -> Result<(), String> {
        let changed = {
            let entity = self
                .entity_mut(id)
                .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
            let before = entity.visibility.raw().clone();
            entity.visibility.set(channel, visible);
            before != *entity.visibility.raw()
        };
        if changed {
            self.record_mutation(id, SceneMutationSource::Engine, SceneDirtyFlags::VISIBILITY)?;
        }
        Ok(())
    }

    pub(crate) fn set_residency(
        &mut self,
        id: SceneEntityId,
        residency: SceneResidency,
    ) -> Result<(), String> {
        let changed = {
            let entity = self
                .entity_mut(id)
                .ok_or_else(|| format!("scene entity {} does not exist", id.0))?;
            if entity.asset_ref.is_none() && residency != SceneResidency::Resident {
                return Err(format!(
                    "scene entity {} has no asset_ref and cannot enter {:?}",
                    id.0, residency
                ));
            }
            if entity.residency == residency {
                false
            } else {
                entity.residency = residency;
                true
            }
        };
        if changed {
            if residency == SceneResidency::Resident {
                self.refresh_process_membership(id);
            } else {
                self.process_active.remove(&id);
            }
            self.reset_process_schedule(id);
            self.record_mutation(
                id,
                SceneMutationSource::Streaming,
                SceneDirtyFlags::RESIDENCY,
            )?;
        }
        Ok(())
    }

    pub(crate) fn refresh_focus_for_streaming(&mut self, camera_position: Vec3) {
        let resolved = match self.focus.source {
            SceneFocusSource::Camera => camera_position,
            SceneFocusSource::Entity(id) => self
                .entity(id)
                .map(|entity| entity.transform.position)
                .unwrap_or(camera_position),
            SceneFocusSource::Override => self.focus.position,
        };
        self.focus.position = resolved;
        self.focus.velocity = Vec3::ZERO;
        self.last_focus_position = resolved;
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
