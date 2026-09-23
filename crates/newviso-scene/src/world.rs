use crate::math::Vec3;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SceneEntityId(pub(crate) u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneEntityKind {
    Camera,
    StaticMesh,
    DynamicMesh,
    Light,
    Trigger,
    Portal,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneMobility {
    Static,
    Dynamic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LightType {
    Directional,
    Point,
    Spot,
    Area,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LightComponent {
    pub(crate) light_type: LightType,
    pub(crate) color: [f32; 3],
    pub(crate) intensity: f32,
    pub(crate) range: f32,
    pub(crate) cone_inner_degrees: f32,
    pub(crate) cone_outer_degrees: f32,
    pub(crate) casts_shadows: bool,
    pub(crate) shadow_bias: f32,
    pub(crate) shadow_normal_bias: f32,
    pub(crate) shadow_resolution: u32,
    pub(crate) shadow_distance: f32,
}

impl LightComponent {
    pub(crate) fn validate(self) -> Result<Self, String> {
        if self
            .color
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
            || !self.intensity.is_finite()
            || self.intensity < 0.0
            || !self.range.is_finite()
            || self.range < 0.0
            || !self.cone_inner_degrees.is_finite()
            || !self.cone_outer_degrees.is_finite()
            || self.cone_inner_degrees < 0.0
            || self.cone_outer_degrees < self.cone_inner_degrees
            || self.cone_outer_degrees > 179.0
            || !self.shadow_bias.is_finite()
            || self.shadow_bias < 0.0
            || !self.shadow_normal_bias.is_finite()
            || self.shadow_normal_bias < 0.0
            || self.shadow_resolution == 0
            || !self.shadow_distance.is_finite()
            || self.shadow_distance <= 0.0
        {
            return Err("invalid generic LightComponent parameters".to_owned());
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneLifecycle {
    Constructed,
    Added,
    Active,
    Dormant,
    PendingRemove,
    Removed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneResidency {
    Unloaded,
    Requested,
    Resident,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum VisibilityModule {
    Debug = 0,
    Camera = 1,
    Script = 2,
    Gameplay = 3,
    Frontend = 4,
    Vfx = 5,
    World = 6,
    Player = 7,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VisibilityMask(u32);

impl Default for VisibilityMask {
    fn default() -> Self {
        Self(u32::MAX)
    }
}

impl VisibilityMask {
    pub(crate) fn set(&mut self, module: VisibilityModule, visible: bool) {
        let bit = 1_u32 << module as u8;
        if visible {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }

    #[cfg(test)]
    pub(crate) fn visible_for(self, module: VisibilityModule) -> bool {
        self.0 & (1_u32 << module as u8) != 0
    }

    pub(crate) fn all_visible(self) -> bool {
        self.0 == u32::MAX
    }

    pub(crate) fn raw(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneBounds {
    pub(crate) min: Vec3,
    pub(crate) max: Vec3,
}

impl SceneBounds {
    pub(crate) fn from_center_half_extent(center: Vec3, half: Vec3) -> Self {
        Self {
            min: Vec3::new(center.x - half.x, center.y - half.y, center.z - half.z),
            max: Vec3::new(center.x + half.x, center.y + half.y, center.z + half.z),
        }
    }

    pub(crate) fn center(self) -> Vec3 {
        Vec3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }

    pub(crate) fn radius(self) -> f32 {
        self.max.sub(self.center()).length()
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneLodPolicy {
    pub(crate) visible_distance: f32,
    pub(crate) stream_distance: f32,
    pub(crate) fade_range: f32,
}

impl Default for SceneLodPolicy {
    fn default() -> Self {
        Self {
            visible_distance: f32::INFINITY,
            stream_distance: f32::INFINITY,
            fade_range: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneTransform {
    pub(crate) position: Vec3,
    pub(crate) rotation_degrees: Vec3,
    pub(crate) scale: Vec3,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneEntity {
    pub(crate) id: SceneEntityId,
    pub(crate) name: String,
    pub(crate) kind: SceneEntityKind,
    pub(crate) mobility: SceneMobility,
    pub(crate) lifecycle: SceneLifecycle,
    pub(crate) transform: SceneTransform,
    pub(crate) light: Option<LightComponent>,
    pub(crate) bounds: SceneBounds,
    pub(crate) parent: Option<SceneEntityId>,
    pub(crate) children: Vec<SceneEntityId>,
    pub(crate) visibility: VisibilityMask,
    pub(crate) lod: SceneLodPolicy,
    pub(crate) solid: bool,
    pub(crate) asset_ref: Option<String>,
    pub(crate) render_slot: Option<usize>,
    pub(crate) residency: SceneResidency,
    pub(crate) priority_score: f32,
    pub(crate) lod_alpha: f32,
    pub(crate) last_visible_frame: Option<u64>,
}

impl SceneEntity {
    pub(crate) fn is_renderable(&self) -> bool {
        self.render_slot.is_some()
            && self.lifecycle == SceneLifecycle::Active
            && self.residency == SceneResidency::Resident
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SceneFocusSource {
    Camera,
    Entity(SceneEntityId),
    Override,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneFocus {
    pub(crate) position: Vec3,
    pub(crate) velocity: Vec3,
    pub(crate) source: SceneFocusSource,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SceneView {
    pub(crate) position: Vec3,
    pub(crate) forward: Vec3,
    pub(crate) up: Vec3,
    pub(crate) near: f32,
    pub(crate) far: f32,
    pub(crate) fov_y_radians: f32,
    pub(crate) aspect: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SceneFramePlan {
    pub(crate) frame: u64,
    pub(crate) visible_render_slots: Vec<usize>,
    pub(crate) visible_entities: Vec<SceneEntityId>,
    pub(crate) requested_entities: Vec<SceneEntityId>,
    pub(crate) streaming_entities: Vec<SceneEntityId>,
    pub(crate) visible_count: usize,
    pub(crate) culled_count: usize,
    pub(crate) resident_count: usize,
    pub(crate) dynamic_count: usize,
}

#[derive(Debug)]
pub(crate) struct SceneWorld {
    entities: Vec<SceneEntity>,
    by_id: HashMap<SceneEntityId, usize>,
    frame: u64,
    focus: SceneFocus,
    last_focus_position: Vec3,
    last_dt: f32,
}

impl SceneWorld {
    pub(crate) fn new(initial_focus: Vec3) -> Self {
        Self {
            entities: Vec::new(),
            by_id: HashMap::new(),
            frame: 0,
            focus: SceneFocus {
                position: initial_focus,
                velocity: Vec3::ZERO,
                source: SceneFocusSource::Camera,
            },
            last_focus_position: initial_focus,
            last_dt: 0.0,
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

    pub(crate) fn pre_update(&mut self, camera_position: Vec3, dt: f32) {
        self.frame = self.frame.wrapping_add(1);
        let dt = if dt.is_finite() && dt > 0.0 {
            dt.min(0.25)
        } else {
            0.0
        };

        let resolved_focus = match self.focus.source {
            SceneFocusSource::Camera => camera_position,
            SceneFocusSource::Entity(id) => self
                .entity(id)
                .map(|entity| entity.transform.position)
                .unwrap_or(camera_position),
            SceneFocusSource::Override => self.focus.position,
        };

        if self.focus.source != SceneFocusSource::Override {
            self.focus.velocity = if dt > 0.0 {
                let delta = resolved_focus.sub(self.last_focus_position);
                Vec3::new(delta.x / dt, delta.y / dt, delta.z / dt)
            } else {
                Vec3::ZERO
            };
        }
        self.focus.position = resolved_focus;
        self.last_focus_position = resolved_focus;
        self.last_dt = dt;

        for entity in &mut self.entities {
            if entity.lifecycle == SceneLifecycle::Added {
                entity.lifecycle = SceneLifecycle::Active;
            }
            entity.priority_score = 0.0;
        }
    }

    pub(crate) fn update(&mut self) {
        for entity in &mut self.entities {
            if entity.lifecycle == SceneLifecycle::Removed {
                continue;
            }
            if entity.mobility == SceneMobility::Dynamic
                && entity.lifecycle == SceneLifecycle::Dormant
            {
                entity.lifecycle = SceneLifecycle::Active;
            }
        }
        self.flush_removals();
    }

    pub(crate) fn scan_visibility(&mut self, view: SceneView) -> SceneFramePlan {
        let forward = view.forward.normalized();
        let right = forward.cross(view.up).normalized();
        let up = right.cross(forward).normalized();
        // Visibility must be conservative: render-space projection and coarse
        // scene bounds are not exact inverses of one another. A small angular
        // guard band prevents entities from popping at the edge of the screen.
        const CULL_GUARD_DEGREES: f32 = 8.0;
        let cull_half_fov =
            (view.fov_y_radians * 0.5 + CULL_GUARD_DEGREES.to_radians()).min(89.0_f32.to_radians());
        let tan_y = cull_half_fov.tan().max(0.0001);
        let tan_x = (tan_y * view.aspect.max(0.0001)).max(0.0001);

        let mut plan = SceneFramePlan {
            frame: self.frame,
            ..Default::default()
        };
        let mut requested_by_priority = Vec::<(SceneEntityId, f32)>::new();
        let mut streaming_by_priority = Vec::<(SceneEntityId, f32)>::new();
        let hierarchy_visible = self
            .entities
            .iter()
            .map(|entity| self.hierarchy_allows(entity.id))
            .collect::<Vec<_>>();

        for (entity_index, entity) in self.entities.iter_mut().enumerate() {
            if entity.lifecycle != SceneLifecycle::Active
                || !hierarchy_visible
                    .get(entity_index)
                    .copied()
                    .unwrap_or(false)
            {
                plan.culled_count += 1;
                continue;
            }
            if entity.residency == SceneResidency::Resident {
                plan.resident_count += 1;
            }
            if entity.mobility == SceneMobility::Dynamic {
                plan.dynamic_count += 1;
            }

            let center = entity.bounds.center();
            let radius = entity.bounds.radius();
            let to_center = center.sub(view.position);
            let distance = to_center.length();
            let focus_distance = center.sub(self.focus.position).length();

            entity.priority_score = stream_priority(radius, focus_distance, entity.mobility);
            let wants_streaming =
                entity.asset_ref.is_some() && focus_distance <= entity.lod.stream_distance;
            if wants_streaming {
                streaming_by_priority.push((entity.id, entity.priority_score));
            }
            if entity.residency == SceneResidency::Unloaded && wants_streaming {
                entity.residency = SceneResidency::Requested;
                requested_by_priority.push((entity.id, entity.priority_score));
            }

            if !entity.visibility.all_visible() || !entity.is_renderable() {
                plan.culled_count += 1;
                continue;
            }

            let max_visible = entity.lod.visible_distance.min(view.far);
            if distance - radius > max_visible {
                entity.lod_alpha = 0.0;
                plan.culled_count += 1;
                continue;
            }
            entity.lod_alpha = if entity.lod.fade_range > 0.0 && max_visible.is_finite() {
                let fade_start = (max_visible - entity.lod.fade_range).max(0.0);
                if distance <= fade_start {
                    1.0
                } else {
                    ((max_visible - distance) / entity.lod.fade_range).clamp(0.0, 1.0)
                }
            } else {
                1.0
            };

            let depth = to_center.dot(forward);
            // Add a depth guard band for the same reason as the angular guard:
            // coarse bounds should fail open rather than visibly pop.
            let depth_guard = (radius * 0.25).max(0.25);
            if depth + radius + depth_guard < view.near || depth - radius - depth_guard > view.far {
                plan.culled_count += 1;
                continue;
            }

            // Objects surrounding the camera must not be rejected merely because
            // their centre is behind the eye plane.
            if depth > 0.0 {
                let horizontal = to_center.dot(right).abs();
                let vertical = to_center.dot(up).abs();
                if horizontal > depth * tan_x + radius || vertical > depth * tan_y + radius {
                    plan.culled_count += 1;
                    continue;
                }
            }

            entity.last_visible_frame = Some(self.frame);
            plan.visible_count += 1;
            plan.visible_entities.push(entity.id);
            if let Some(slot) = entity.render_slot {
                plan.visible_render_slots.push(slot);
            }
        }

        plan.visible_render_slots.sort_unstable();
        plan.visible_entities.sort_by_key(|entity_id| entity_id.0);
        requested_by_priority.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0 .0.cmp(&b.0 .0))
        });
        plan.requested_entities = requested_by_priority
            .into_iter()
            .map(|(entity_id, _)| entity_id)
            .collect();
        streaming_by_priority.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0 .0.cmp(&b.0 .0))
        });
        plan.streaming_entities = streaming_by_priority
            .into_iter()
            .map(|(entity_id, _)| entity_id)
            .collect();

        plan
    }

    fn hierarchy_allows(&self, id: SceneEntityId) -> bool {
        let mut cursor = self.entity(id).and_then(|entity| entity.parent);
        let mut depth = 0usize;
        while let Some(parent_id) = cursor {
            let Some(parent) = self.entity(parent_id) else {
                return false;
            };
            if parent.lifecycle != SceneLifecycle::Active || !parent.visibility.all_visible() {
                return false;
            }
            cursor = parent.parent;
            depth += 1;
            if depth > self.entities.len() {
                return false;
            }
        }
        true
    }

    pub(crate) fn active_render_slots(&self) -> Vec<usize> {
        self.entities
            .iter()
            .filter_map(|entity| {
                (entity.is_renderable()
                    && entity.visibility.all_visible()
                    && self.hierarchy_allows(entity.id))
                .then_some(entity.render_slot)
                .flatten()
            })
            .collect()
    }

    pub(crate) fn solid_bounds(&self) -> impl Iterator<Item = SceneBounds> + '_ {
        self.entities.iter().filter_map(|entity| {
            (entity.solid
                && entity.lifecycle == SceneLifecycle::Active
                && entity.residency == SceneResidency::Resident)
                .then_some(entity.bounds)
        })
    }

    pub(crate) fn entity_count(&self) -> usize {
        self.entities
            .iter()
            .filter(|entity| entity.lifecycle != SceneLifecycle::Removed)
            .count()
    }

    pub(crate) fn static_count(&self) -> usize {
        self.entities
            .iter()
            .filter(|entity| {
                entity.lifecycle != SceneLifecycle::Removed
                    && entity.mobility == SceneMobility::Static
            })
            .count()
    }

    pub(crate) fn dynamic_count(&self) -> usize {
        self.entities
            .iter()
            .filter(|entity| {
                entity.lifecycle != SceneLifecycle::Removed
                    && entity.mobility == SceneMobility::Dynamic
            })
            .count()
    }

    pub(crate) fn focus(&self) -> SceneFocus {
        self.focus
    }
}

fn stream_priority(radius: f32, distance: f32, mobility: SceneMobility) -> f32 {
    let size_score = radius.max(0.05) / distance.max(0.5);
    let mobility_bias = match mobility {
        SceneMobility::Static => 1.0,
        SceneMobility::Dynamic => 1.35,
    };
    size_score * mobility_bias
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(id: u64, z: f32, render_slot: usize) -> SceneEntity {
        SceneEntity {
            id: SceneEntityId(id),
            name: format!("entity-{id}"),
            kind: SceneEntityKind::StaticMesh,
            mobility: SceneMobility::Static,
            lifecycle: SceneLifecycle::Constructed,
            transform: SceneTransform {
                position: Vec3::new(0.0, 0.0, z),
                rotation_degrees: Vec3::ZERO,
                scale: Vec3::ONE,
            },
            light: None,
            bounds: SceneBounds::from_center_half_extent(
                Vec3::new(0.0, 0.0, z),
                Vec3::new(1.0, 1.0, 1.0),
            ),
            parent: None,
            children: Vec::new(),
            visibility: VisibilityMask::default(),
            lod: SceneLodPolicy::default(),
            solid: false,
            asset_ref: None,
            render_slot: Some(render_slot),
            residency: SceneResidency::Resident,
            priority_score: 0.0,
            lod_alpha: 1.0,
            last_visible_frame: None,
        }
    }

    fn view() -> SceneView {
        SceneView {
            position: Vec3::ZERO,
            forward: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::Y,
            near: 0.1,
            far: 100.0,
            fov_y_radians: 70.0_f32.to_radians(),
            aspect: 16.0 / 9.0,
        }
    }

    #[test]
    fn lifecycle_and_visibility_scan_match_scene_phases() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(1, -8.0, 0)).unwrap();
        world.add_entity(entity(2, 8.0, 1)).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        world.update();

        let plan = world.scan_visibility(view());
        assert_eq!(plan.visible_render_slots, vec![0]);
        assert_eq!(plan.visible_count, 1);
        assert_eq!(plan.culled_count, 1);
    }

    #[test]
    fn visibility_modules_can_hide_entity_without_removing_it() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(1, -8.0, 0)).unwrap();
        world.activate_all();
        world
            .set_visibility(SceneEntityId(1), VisibilityModule::Script, false)
            .unwrap();
        assert!(!world
            .entity(SceneEntityId(1))
            .unwrap()
            .visibility
            .visible_for(VisibilityModule::Script));
        world
            .set_visibility(SceneEntityId(1), VisibilityModule::Camera, false)
            .unwrap();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        let plan = world.scan_visibility(view());
        assert!(plan.visible_render_slots.is_empty());
    }

    #[test]
    fn generic_light_component_is_stored_without_game_semantics() {
        let mut e = entity(9, -4.0, 0);
        e.kind = SceneEntityKind::Light;
        e.render_slot = None;
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(e).unwrap();
        world.activate_all();

        world
            .set_light(
                SceneEntityId(9),
                Some(LightComponent {
                    light_type: LightType::Directional,
                    color: [1.0, 0.9, 0.8],
                    intensity: 1.5,
                    range: 0.0,
                    cone_inner_degrees: 0.0,
                    cone_outer_degrees: 0.0,
                    casts_shadows: true,
                    shadow_bias: 0.0015,
                    shadow_normal_bias: 0.02,
                    shadow_resolution: 2048,
                    shadow_distance: 96.0,
                }),
            )
            .unwrap();

        let lights = world.active_lights();
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].2.light_type, LightType::Directional);
        assert!(lights[0].2.casts_shadows);
    }

    #[test]
    fn dynamic_entities_receive_streaming_priority_bias() {
        let static_score = stream_priority(1.0, 10.0, SceneMobility::Static);
        let dynamic_score = stream_priority(1.0, 10.0, SceneMobility::Dynamic);
        assert!(dynamic_score > static_score);
    }

    #[test]
    fn conservative_guard_band_keeps_edge_visible_entity() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut e = entity(1, -10.0, 0);
        e.transform.position = Vec3::new(14.5, 0.0, -10.0);
        e.bounds =
            SceneBounds::from_center_half_extent(e.transform.position, Vec3::new(1.0, 1.0, 1.0));
        world.add_entity(e).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert_eq!(plan.visible_render_slots, vec![0]);
    }

    #[test]
    fn hierarchy_propagates_visibility_and_rejects_cycles() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(1, -8.0, 0)).unwrap();
        world.add_entity(entity(2, -8.0, 1)).unwrap();
        world.activate_all();
        world
            .set_parent(SceneEntityId(2), Some(SceneEntityId(1)))
            .unwrap();
        assert!(world
            .set_parent(SceneEntityId(1), Some(SceneEntityId(2)))
            .is_err());

        world
            .set_visibility(SceneEntityId(1), VisibilityModule::World, false)
            .unwrap();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        let plan = world.scan_visibility(view());
        assert!(!plan.visible_entities.contains(&SceneEntityId(2)));
    }

    #[test]
    fn focus_tracks_entity_and_supports_explicit_override() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        world.add_entity(entity(7, -12.0, 0)).unwrap();
        world.activate_all();

        world.set_focus_entity(SceneEntityId(7)).unwrap();
        world.pre_update(Vec3::new(99.0, 0.0, 0.0), 1.0 / 60.0);
        assert!((world.focus().position.z + 12.0).abs() < 0.001);

        world.set_focus_override(Vec3::new(3.0, 4.0, 5.0), Vec3::new(1.0, 0.0, 0.0));
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);
        assert_eq!(world.focus().source, SceneFocusSource::Override);
        assert!((world.focus().position.x - 3.0).abs() < 0.001);

        world.set_focus_camera();
        world.pre_update(Vec3::new(2.0, 0.0, 0.0), 1.0 / 60.0);
        assert_eq!(world.focus().source, SceneFocusSource::Camera);
        assert!((world.focus().position.x - 2.0).abs() < 0.001);
    }

    #[test]
    fn unloaded_assets_are_requested_in_priority_order() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut far = entity(1, -30.0, 0);
        far.residency = SceneResidency::Unloaded;
        far.asset_ref = Some("assets/far_model@main".to_owned());
        let mut near = entity(2, -5.0, 1);
        near.residency = SceneResidency::Unloaded;
        near.asset_ref = Some("assets/near_model@main".to_owned());
        world.add_entity(far).unwrap();
        world.add_entity(near).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert_eq!(
            plan.requested_entities,
            vec![SceneEntityId(2), SceneEntityId(1)]
        );
        assert_eq!(
            plan.streaming_entities,
            vec![SceneEntityId(2), SceneEntityId(1)]
        );

        world
            .set_residency(SceneEntityId(2), SceneResidency::Resident)
            .unwrap();
        let next = world.scan_visibility(view());
        assert_eq!(
            next.streaming_entities,
            vec![SceneEntityId(2), SceneEntityId(1)]
        );
        assert!(!next.requested_entities.contains(&SceneEntityId(2)));
    }

    #[test]
    fn lod_fade_is_computed_before_visibility_gather() {
        let mut world = SceneWorld::new(Vec3::ZERO);
        let mut e = entity(1, -8.0, 0);
        e.lod = SceneLodPolicy {
            visible_distance: 10.0,
            stream_distance: 12.0,
            fade_range: 4.0,
        };
        world.add_entity(e).unwrap();
        world.activate_all();
        world.pre_update(Vec3::ZERO, 1.0 / 60.0);

        let plan = world.scan_visibility(view());
        assert_eq!(plan.visible_count, 1);
        let alpha = world.entity(SceneEntityId(1)).unwrap().lod_alpha;
        assert!(alpha > 0.0 && alpha < 1.0);
    }
}
