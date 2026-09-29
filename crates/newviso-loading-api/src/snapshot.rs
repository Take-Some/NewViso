use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadingSubsystemPhase {
    Waiting,
    Running,
    Ready,
    Degraded,
    Failed,
}

impl Default for LoadingSubsystemPhase {
    #[inline]
    fn default() -> Self {
        Self::Waiting
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadingSubsystemSnapshot {
    pub id: String,
    pub label: String,
    pub phase: LoadingSubsystemPhase,
    pub state_label: String,
    pub detail: String,
    pub progress_01: Option<f32>,
}

impl LoadingSubsystemSnapshot {
    #[inline]
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        phase: LoadingSubsystemPhase,
        state_label: impl Into<String>,
        detail: impl Into<String>,
        progress_01: Option<f32>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            phase,
            state_label: state_label.into(),
            detail: detail.into(),
            progress_01: progress_01.map(|v| v.clamp(0.0, 1.0)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadingScreenSnapshot {
    pub active: bool,
    pub title: String,
    pub status: String,
    pub detail: String,
    pub progress_01: f32,
    pub spinner_phase: u32,
    pub source: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub view_json: String,
    #[serde(default)]
    pub subsystems: Vec<LoadingSubsystemSnapshot>,
}

impl Default for LoadingScreenSnapshot {
    #[inline]
    fn default() -> Self {
        Self {
            active: false,
            title: "NORTH STAR ENGINE // BOOTSTRAP".to_owned(),
            status: "Preparing runtime...".to_owned(),
            detail: "The loading status bridge is waiting for startup telemetry.".to_owned(),
            progress_01: 0.0,
            spinner_phase: 0,
            source: "engine.ui.loading".to_owned(),
            provider: "engine-loading-data".to_owned(),
            view_json: String::new(),
            subsystems: Vec::new(),
        }
    }
}

impl LoadingScreenSnapshot {
    #[inline]
    pub fn inactive() -> Self {
        Self::default()
    }

    #[inline]
    pub fn normalize(mut self) -> Self {
        self.progress_01 = self.progress_01.clamp(0.0, 1.0);
        for subsystem in &mut self.subsystems {
            subsystem.progress_01 = subsystem.progress_01.map(|v| v.clamp(0.0, 1.0));
        }
        self
    }
}
