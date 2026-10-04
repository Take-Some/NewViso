use super::*;

impl LivingWorldRuntime {
    pub fn tick_frame(&mut self, dt: f32, transient_observers: &[[f32; 3]]) {
        self.frame_actor_updates.clear();
        self.frame_process_activations.clear();
        self.frame_events.clear();
        self.frame_reality_events.clear();
        self.frame_travel_completions.clear();
        self.frame_representation_changes.clear();

        for step in self.clock.advance_frame(dt) {
            self.tick_step(step, transient_observers);
        }

        self.frame_reality_events = std::mem::take(&mut self.pending_reality_events);

        self.frame_process_activations.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| {
                    a.due_world_seconds
                        .partial_cmp(&b.due_world_seconds)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| a.id.cmp(&b.id))
        });
        self.frame_events.sort_by(|a, b| {
            b.desc
                .priority
                .cmp(&a.desc.priority)
                .then_with(|| {
                    a.due_world_seconds
                        .partial_cmp(&b.due_world_seconds)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| a.desc.id.cmp(&b.desc.id))
        });
    }

    pub(super) fn tick_step(&mut self, step: WorldStep, transient_observers: &[[f32; 3]]) {
        self.stimuli
            .retain(|_, active| active.expires_world_seconds > step.world_seconds);
        self.scenario_reservations
            .retain(|_, reservation| reservation.ends_world_seconds > step.world_seconds);
        self.refresh_actor_tiers(step, transient_observers);
        self.tick_processes(step);
        self.tick_scheduled_events(step);
        self.tick_actors(step);
        self.refresh_actor_tiers(step, transient_observers);
    }
}
