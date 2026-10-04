use super::*;

impl LivingWorldRuntime {
    pub fn upsert_process(&mut self, mut desc: WorldProcessDesc) -> Result<(), String> {
        desc.validate()?;
        desc.phase_seconds = desc.phase_seconds.rem_euclid(desc.interval_seconds);

        if let Some(existing) = self.processes.get_mut(&desc.id) {
            let schedule_changed = existing.desc.interval_seconds != desc.interval_seconds
                || existing.desc.phase_seconds != desc.phase_seconds;
            existing.desc = desc;
            if schedule_changed {
                existing.next_due_seconds = first_due_at_or_after(
                    self.clock.world_seconds,
                    existing.desc.interval_seconds,
                    existing.desc.phase_seconds,
                );
            }
        } else {
            let next_due_seconds = first_due_at_or_after(
                self.clock.world_seconds,
                desc.interval_seconds,
                desc.phase_seconds,
            );
            self.processes.insert(
                desc.id.clone(),
                WorldProcessRecord {
                    desc,
                    next_due_seconds,
                },
            );
        }
        Ok(())
    }

    pub fn remove_process(&mut self, id: &str) {
        self.processes.remove(id.trim());
    }

    pub(super) fn tick_processes(&mut self, step: WorldStep) {
        for record in self.processes.values_mut() {
            if !record.desc.enabled || step.world_seconds + f64::EPSILON < record.next_due_seconds {
                continue;
            }
            let due = record.next_due_seconds;
            let elapsed = (step.world_seconds - due).max(0.0);
            let occurrences = 1 + (elapsed / record.desc.interval_seconds).floor() as u64;
            record.next_due_seconds += record.desc.interval_seconds * occurrences as f64;
            self.frame_process_activations.push(WorldProcessActivation {
                id: record.desc.id.clone(),
                priority: record.desc.priority,
                due_world_seconds: due,
                delivered_world_seconds: step.world_seconds,
                occurrences,
                tags: record.desc.tags.clone(),
                payload: record.desc.payload.clone(),
            });
        }
    }
}
