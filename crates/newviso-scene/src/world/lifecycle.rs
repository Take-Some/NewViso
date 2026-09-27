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
        self.process_elapsed_seconds += f64::from(dt);

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
        //
        // Work is both cadence-gated and time-sliced. A rotating cursor guarantees
        // that overload delays work instead of starving high stable IDs forever.
        let frame = self.frame;
        let now = self.process_elapsed_seconds;
        self.process_tickets.clear();
        self.process_scanned_count = 0;

        let active_ids = self.process_active_ids();
        if active_ids.is_empty() {
            self.process_scan_cursor = 0;
            self.process_effective_budget = PROCESS_BASE_UPDATES_PER_FRAME;
            self.flush_removals();
            return;
        }

        let frame_scale = if self.last_dt > 0.0 {
            (PROCESS_TARGET_FRAME_SECONDS / self.last_dt).clamp(0.125, 2.0)
        } else {
            1.0
        };
        self.process_effective_budget = ((PROCESS_BASE_UPDATES_PER_FRAME as f32 * frame_scale)
            .round() as usize)
            .clamp(PROCESS_MIN_UPDATES_PER_FRAME, PROCESS_MAX_UPDATES_PER_FRAME)
            .min(active_ids.len());
        let scan_budget = self
            .process_effective_budget
            .saturating_mul(4)
            .min(active_ids.len());
        let start = self.process_scan_cursor % active_ids.len();

        for offset in 0..scan_budget {
            if self.process_tickets.len() >= self.process_effective_budget {
                break;
            }
            let index = (start + offset) % active_ids.len();
            let id = active_ids[index];
            self.process_scanned_count += 1;

            let Some(entity) = self.entity(id) else {
                self.process_active.remove(&id);
                self.reset_process_schedule(id);
                continue;
            };
            if matches!(
                entity.lifecycle,
                SceneLifecycle::PendingRemove | SceneLifecycle::Removed
            ) {
                self.process_active.remove(&id);
                self.reset_process_schedule(id);
                continue;
            }

            let claimed_reasons = entity.process_claims.mask();
            if claimed_reasons.is_empty() {
                self.process_active.remove(&id);
                self.reset_process_schedule(id);
                continue;
            }

            let mut due_reasons = SceneProcessReasons::EMPTY;
            for reason in SceneProcessReasons::ALL {
                if !claimed_reasons.contains(reason) {
                    continue;
                }

                let hz = self.process_rate_hz(id, reason);
                let interval_seconds = if hz > 0.0 { 1.0 / f64::from(hz) } else { 0.0 };
                let key = (id, reason.bits());
                let due = self
                    .process_last_due_seconds
                    .get(&key)
                    .map(|last| {
                        interval_seconds == 0.0 || now - *last + f64::EPSILON >= interval_seconds
                    })
                    .unwrap_or(true);

                if due {
                    self.process_last_due_seconds.insert(key, now);
                    due_reasons = due_reasons.union(reason);
                }
            }

            if due_reasons.is_empty() {
                continue;
            }
            if let Some(entity) = self.entity_mut(id) {
                entity.last_process_frame = Some(frame);
            }
            self.process_tickets.push(SceneProcessTicket {
                entity: id,
                frame,
                reasons: due_reasons,
                elapsed_seconds: now,
            });
        }

        self.process_scan_cursor = (start + self.process_scanned_count.max(1)) % active_ids.len();
        self.flush_removals();
    }

    pub(crate) fn process_tickets(&self) -> &[SceneProcessTicket] {
        &self.process_tickets
    }

    pub(crate) fn process_due_count(&self) -> usize {
        self.process_tickets.len()
    }

    pub(crate) fn process_effective_budget(&self) -> usize {
        self.process_effective_budget
    }

    pub(crate) fn process_scanned_count(&self) -> usize {
        self.process_scanned_count
    }
}
