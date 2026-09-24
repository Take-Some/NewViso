#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct WorldClockPolicyDesc {
    pub fixed_hz: f32,
    pub time_scale: f32,
    pub max_steps_per_frame: u32,
}

impl Default for WorldClockPolicyDesc {
    fn default() -> Self {
        Self {
            fixed_hz: 20.0,
            time_scale: 1.0,
            max_steps_per_frame: 8,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WorldStep {
    pub(crate) world_seconds: f64,
    pub(crate) fixed_tick: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct WorldClockRuntime {
    pub(crate) policy: WorldClockPolicyDesc,
    pub(crate) world_seconds: f64,
    pub(crate) accumulator_seconds: f64,
    pub(crate) fixed_tick: u64,
    pub(crate) last_frame_steps: u32,
}

impl Default for WorldClockRuntime {
    fn default() -> Self {
        Self {
            policy: WorldClockPolicyDesc::default(),
            world_seconds: 0.0,
            accumulator_seconds: 0.0,
            fixed_tick: 0,
            last_frame_steps: 0,
        }
    }
}

impl WorldClockRuntime {
    pub(crate) fn configure(&mut self, policy: WorldClockPolicyDesc) -> Result<(), String> {
        if !policy.fixed_hz.is_finite()
            || !(0.1..=1000.0).contains(&policy.fixed_hz)
            || !policy.time_scale.is_finite()
            || !(0.0..=10_000.0).contains(&policy.time_scale)
            || !(1..=4096).contains(&policy.max_steps_per_frame)
        {
            return Err("invalid generic WorldClockPolicyDesc parameters".to_owned());
        }
        self.policy = policy;
        Ok(())
    }

    pub(crate) fn set_world_seconds(&mut self, world_seconds: f64) -> Result<(), String> {
        if !world_seconds.is_finite() || world_seconds.abs() > 1.0e15 {
            return Err("world clock seconds must be finite and bounded".to_owned());
        }
        self.world_seconds = world_seconds;
        self.accumulator_seconds = 0.0;
        Ok(())
    }

    pub(crate) fn advance_frame(&mut self, frame_dt: f32) -> Vec<WorldStep> {
        self.last_frame_steps = 0;
        if !frame_dt.is_finite() || frame_dt < 0.0 || self.policy.time_scale <= 0.0 {
            return Vec::new();
        }

        let fixed_dt = 1.0 / f64::from(self.policy.fixed_hz);
        self.accumulator_seconds += f64::from(frame_dt) * f64::from(self.policy.time_scale);

        let mut steps = Vec::new();
        // Relative tolerance prevents a rounded remainder from losing a whole tick.
        let step_epsilon = fixed_dt * 1.0e-9;
        while self.accumulator_seconds + step_epsilon >= fixed_dt
            && steps.len() < self.policy.max_steps_per_frame as usize
        {
            self.accumulator_seconds = (self.accumulator_seconds - fixed_dt).max(0.0);
            self.world_seconds += fixed_dt;
            self.fixed_tick = self.fixed_tick.wrapping_add(1);
            steps.push(WorldStep {
                world_seconds: self.world_seconds,
                fixed_tick: self.fixed_tick,
            });
        }
        self.last_frame_steps = steps.len() as u32;
        steps
    }
}
