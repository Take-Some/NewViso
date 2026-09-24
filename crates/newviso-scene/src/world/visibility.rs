use super::*;

impl SceneWorld {
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
