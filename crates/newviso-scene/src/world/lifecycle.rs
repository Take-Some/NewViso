use super::*;

impl SceneWorld {
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
        // Process-control is deliberately separate from render lifecycle. An entity
        // may remain Active/visible/resident while being absent from this set.
        let frame = self.frame;
        let active_ids = self.process_active_ids();
        for id in active_ids {
            let Some(entity) = self.entity_mut(id) else {
                self.process_active.remove(&id);
                continue;
            };
            if matches!(
                entity.lifecycle,
                SceneLifecycle::PendingRemove | SceneLifecycle::Removed
            ) {
                self.process_active.remove(&id);
                continue;
            }
            entity.last_process_frame = Some(frame);
        }

        self.flush_removals();
    }
}
