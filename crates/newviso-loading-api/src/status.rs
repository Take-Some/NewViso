use serde::{Deserialize, Serialize};

use crate::{LoadingSubsystemPhase, LoadingSubsystemSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadingStatusPhase {
    Waiting,
    Running,
    Ready,
    Degraded,
    Failed,
}

impl LoadingStatusPhase {
    #[inline]
    pub const fn as_subsystem_phase(self) -> LoadingSubsystemPhase {
        match self {
            Self::Waiting => LoadingSubsystemPhase::Waiting,
            Self::Running => LoadingSubsystemPhase::Running,
            Self::Ready => LoadingSubsystemPhase::Ready,
            Self::Degraded => LoadingSubsystemPhase::Degraded,
            Self::Failed => LoadingSubsystemPhase::Failed,
        }
    }

    #[inline]
    pub const fn state_label(self) -> &'static str {
        match self {
            Self::Waiting => "WAIT",
            Self::Running => "RUNNING",
            Self::Ready => "READY",
            Self::Degraded => "DEGRADED",
            Self::Failed => "ERR",
        }
    }
}

impl Default for LoadingStatusPhase {
    #[inline]
    fn default() -> Self {
        Self::Running
    }
}

/// Generic loading/status event published through the engine bus.
///
/// Producers describe what they are doing; renderers, asset providers and
/// runtime gates do not paint UI directly. `engine.ui.loading` projects this DTO
/// into the current loading surface snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadingStatusEvent {
    pub source: String,
    pub domain: String,
    pub subsystem_id: String,
    pub subsystem_label: String,
    pub phase: LoadingStatusPhase,
    pub state_label: String,
    pub title: String,
    pub status: String,
    pub detail: String,
    pub progress_01: f32,
    #[serde(default)]
    pub terminal: bool,
}

impl LoadingStatusEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: impl Into<String>,
        domain: impl Into<String>,
        subsystem_id: impl Into<String>,
        subsystem_label: impl Into<String>,
        phase: LoadingStatusPhase,
        title: impl Into<String>,
        status: impl Into<String>,
        detail: impl Into<String>,
        progress_01: f32,
    ) -> Self {
        let phase_label = phase.state_label();
        Self {
            source: source.into(),
            domain: domain.into(),
            subsystem_id: subsystem_id.into(),
            subsystem_label: subsystem_label.into(),
            phase,
            state_label: phase_label.to_owned(),
            title: title.into(),
            status: status.into(),
            detail: detail.into(),
            progress_01: progress_01.clamp(0.0, 1.0),
            terminal: matches!(
                phase,
                LoadingStatusPhase::Ready | LoadingStatusPhase::Failed
            ),
        }
    }

    #[inline]
    pub fn with_state_label(mut self, state_label: impl Into<String>) -> Self {
        self.state_label = state_label.into();
        self
    }

    #[inline]
    pub fn with_terminal(mut self, terminal: bool) -> Self {
        self.terminal = terminal;
        self
    }

    pub fn to_subsystem_snapshot(&self) -> LoadingSubsystemSnapshot {
        LoadingSubsystemSnapshot::new(
            self.subsystem_id.as_str(),
            self.subsystem_label.as_str(),
            self.phase.as_subsystem_phase(),
            self.state_label.as_str(),
            self.detail.as_str(),
            Some(self.progress_01),
        )
    }
}
