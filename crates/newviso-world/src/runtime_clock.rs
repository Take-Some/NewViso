use super::*;

impl LivingWorldRuntime {
    pub fn world_seconds(&self) -> f64 {
        self.clock.world_seconds
    }

    pub fn configure_clock(&mut self, policy: WorldClockPolicyDesc) -> Result<(), String> {
        self.clock.configure(policy)
    }

    pub fn set_world_seconds(&mut self, world_seconds: f64) -> Result<(), String> {
        self.clock.set_world_seconds(world_seconds)?;
        for process in self.processes.values_mut() {
            process.next_due_seconds = first_due_at_or_after(
                self.clock.world_seconds,
                process.desc.interval_seconds,
                process.desc.phase_seconds,
            );
        }
        Ok(())
    }

    pub fn configure_simulation(
        &mut self,
        policy: WorldSimulationPolicyDesc,
    ) -> Result<(), String> {
        policy.validate()?;
        self.simulation_policy = policy;
        Ok(())
    }

    pub fn upsert_observer(&mut self, desc: WorldObserverDesc) -> Result<(), String> {
        desc.validate()?;
        self.observers.insert(desc.id.clone(), desc);
        Ok(())
    }

    pub fn remove_observer(&mut self, id: &str) {
        self.observers.remove(id.trim());
    }
}
