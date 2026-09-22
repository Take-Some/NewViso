#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimePhase {
    Boot,
    Platform,
    Engine,
    Running,
    Shutdown,
}

#[derive(Debug)]
pub struct EngineState {
    phase: RuntimePhase,
    frame_index: u64,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            phase: RuntimePhase::Boot,
            frame_index: 0,
        }
    }
}

impl EngineState {
    pub fn phase(&self) -> RuntimePhase {
        self.phase
    }

    pub fn set_phase(&mut self, phase: RuntimePhase) {
        self.phase = phase;
    }

    pub fn frame_index(&self) -> u64 {
        self.frame_index
    }

    pub fn advance_frame(&mut self) -> u64 {
        let current = self.frame_index;
        self.frame_index = self.frame_index.wrapping_add(1);
        current
    }
}
