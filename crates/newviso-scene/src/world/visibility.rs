use super::*;
use crate::math::transform_point;

impl SceneWorld {
    pub(crate) fn remove_spatial_entity(&mut self, id: SceneEntityId) {
        if let Some(cells) = self.spatial_entity_cells.remove(&id) {
            for cell in cells {
                let remove_cell = if let Some(entities) = self.spatial_cells.get_mut(&cell) {
                    entities.remove(&id);
                    entities.is_empty()
                } else {
                    false
                };
                if remove_cell {
                    self.spatial_cells.remove(&cell);
                }
            }
        }
        self.spatial_oversized.remove(&id);
        self.spatial_always_stream.remove(&id);
    }

    pub(crate) fn index_spatial_entity(&mut self, id: SceneEntityId) {
        let Some(entity) = self.entity(id) else {
            self.remove_spatial_entity(id);
            return;
        };
        let lifecycle = entity.lifecycle;
        let bounds = entity.bounds;
        let has_asset = entity.asset_ref.is_some();
        let stream_distance = entity.lod.stream_distance;

        self.remove_spatial_entity(id);
        if matches!(
            lifecycle,
            SceneLifecycle::PendingRemove | SceneLifecycle::Removed
        ) {
            return;
        }

        match spatial_cells_for_bounds(bounds) {
            Some(cells) => {
                for cell in &cells {
                    self.spatial_cells.entry(*cell).or_default().insert(id);
                }
                self.spatial_entity_cells.insert(id, cells);
            }
            None => {
                self.spatial_oversized.insert(id);
            }
        }

        if has_asset {
            if stream_distance.is_finite() {
                self.spatial_max_stream_distance = self
                    .spatial_max_stream_distance
                    .max(stream_distance.max(0.0));
            } else {
                self.spatial_always_stream.insert(id);
            }
        }
    }

    fn append_spatial_range(
        &self,
        center: Vec3,
        radius: f32,
        candidates: &mut BTreeSet<SceneEntityId>,
    ) {
        if !radius.is_finite() {
            candidates.extend(
                self.entities
                    .iter()
                    .filter(|entity| entity.lifecycle != SceneLifecycle::Removed)
                    .map(|entity| entity.id),
            );
            return;
        }
        if radius <= 0.0 {
            if let Some(entities) = self.spatial_cells.get(&spatial_cell_for_point(center)) {
                candidates.extend(entities.iter().copied());
            }
            return;
        }

        let min = spatial_cell_for_point(Vec3::new(center.x - radius, center.y, center.z - radius));
        let max = spatial_cell_for_point(Vec3::new(center.x + radius, center.y, center.z + radius));
        for x in min.0..=max.0 {
            for z in min.1..=max.1 {
                if let Some(entities) = self.spatial_cells.get(&(x, z)) {
                    candidates.extend(entities.iter().copied());
                }
            }
        }
    }

    pub(crate) fn visibility_candidates(&self, view: SceneView) -> BTreeSet<SceneEntityId> {
        let mut candidates = self.spatial_oversized.clone();
        candidates.extend(self.spatial_always_stream.iter().copied());
        self.append_spatial_range(view.position, view.far.max(0.0), &mut candidates);
        self.append_spatial_range(
            self.focus.position,
            self.spatial_max_stream_distance,
            &mut candidates,
        );
        candidates
    }

    pub(crate) fn spatial_cell_count(&self) -> usize {
        self.spatial_cells.len()
    }

    pub(crate) fn spatial_oversized_count(&self) -> usize {
        self.spatial_oversized.len()
    }

    #[cfg(test)]
    pub(crate) fn scan_visibility(&mut self, view: SceneView) -> SceneFramePlan {
        let candidates = self.visibility_candidates(view);
        self.scan_visibility_candidates(view, candidates)
    }

    pub(crate) fn scan_visibility_candidates(
        &mut self,
        view: SceneView,
        candidate_ids: BTreeSet<SceneEntityId>,
    ) -> SceneFramePlan {
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

        let hierarchy_visible = candidate_ids
            .iter()
            .map(|id| (*id, self.hierarchy_allows(*id)))
            .collect::<BTreeMap<_, _>>();
        let mut plan = SceneFramePlan {
            frame: self.frame,
            spatial_candidate_count: candidate_ids.len(),
            culled_count: self.entities.len().saturating_sub(candidate_ids.len()),
            ..Default::default()
        };
        let mut requested_by_priority = Vec::<(SceneEntityId, f32)>::new();
        let mut streaming_by_priority = Vec::<(SceneEntityId, f32)>::new();

        for id in candidate_ids {
            let Some(index) = self.by_id.get(&id).copied() else {
                continue;
            };
            let Some(entity) = self.entities.get_mut(index) else {
                continue;
            };
            if entity.lifecycle != SceneLifecycle::Active
                || !hierarchy_visible.get(&id).copied().unwrap_or(false)
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

            // Stream against distance to the entity's authored bounds surface,
            // not merely its transform/origin. Large collision sectors can extend
            // hundreds of metres away from their origin; using center distance can
            // leave a wall/floor intersecting the player completely unloaded.
            let surface_distance = (focus_distance - radius).max(0.0);
            entity.priority_score = stream_priority(radius, surface_distance, entity.mobility);
            let wants_streaming =
                entity.asset_ref.is_some() && surface_distance <= entity.lod.stream_distance;
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
            let depth_guard = (radius * 0.25).max(0.25);
            if depth + radius + depth_guard < view.near || depth - radius - depth_guard > view.far {
                plan.culled_count += 1;
                continue;
            }

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
    pub(super) fn hierarchy_allows(&self, id: SceneEntityId) -> bool {
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
    pub(crate) fn collision_bounds(&self, id: SceneEntityId) -> Option<SceneBounds> {
        self.entity(id).map(entity_collision_bounds)
    }

    pub(crate) fn solid_bounds(&self) -> impl Iterator<Item = SceneBounds> + '_ {
        self.entities.iter().filter_map(|entity| {
            (entity.solid
                && entity.lifecycle == SceneLifecycle::Active
                && entity.residency == SceneResidency::Resident)
                .then(|| entity_collision_bounds(entity))
        })
    }
    pub(crate) fn static_solid_colliders_near(
        &self,
        interests: &[SceneBounds],
    ) -> Vec<(SceneEntityId, SceneBounds)> {
        if interests.is_empty() {
            return Vec::new();
        }

        let mut candidates = self.spatial_oversized.clone();
        for interest in interests {
            let min = spatial_cell_for_point(interest.min);
            let max = spatial_cell_for_point(interest.max);
            for x in min.0..=max.0 {
                for z in min.1..=max.1 {
                    if let Some(entities) = self.spatial_cells.get(&(x, z)) {
                        candidates.extend(entities.iter().copied());
                    }
                }
            }
        }

        candidates
            .into_iter()
            .filter_map(|id| {
                let entity = self.entity(id)?;
                let collision_bounds = entity_collision_bounds(entity);
                if !entity.solid
                    || entity.mobility != SceneMobility::Static
                    || entity.lifecycle != SceneLifecycle::Active
                    || entity.residency != SceneResidency::Resident
                    || !interests
                        .iter()
                        .any(|interest| bounds_intersect(collision_bounds, *interest))
                {
                    return None;
                }
                Some((id, collision_bounds))
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn static_solid_bounds_near(&self, interests: &[SceneBounds]) -> Vec<SceneBounds> {
        self.static_solid_colliders_near(interests)
            .into_iter()
            .map(|(_, bounds)| bounds)
            .collect()
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

fn entity_collision_bounds(entity: &SceneEntity) -> SceneBounds {
    let Some(local) = entity.collision_local_bounds else {
        return entity.bounds;
    };

    let mut world_min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut world_max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for x in [local.min.x, local.max.x] {
        for y in [local.min.y, local.max.y] {
            for z in [local.min.z, local.max.z] {
                let point = transform_point(
                    Vec3::new(x, y, z),
                    entity.transform.scale,
                    entity.transform.rotation_degrees,
                    entity.transform.position,
                );
                world_min.x = world_min.x.min(point.x);
                world_min.y = world_min.y.min(point.y);
                world_min.z = world_min.z.min(point.z);
                world_max.x = world_max.x.max(point.x);
                world_max.y = world_max.y.max(point.y);
                world_max.z = world_max.z.max(point.z);
            }
        }
    }
    SceneBounds {
        min: world_min,
        max: world_max,
    }
}

fn bounds_intersect(a: SceneBounds, b: SceneBounds) -> bool {
    a.min.x <= b.max.x
        && a.max.x >= b.min.x
        && a.min.y <= b.max.y
        && a.max.y >= b.min.y
        && a.min.z <= b.max.z
        && a.max.z >= b.min.z
}
