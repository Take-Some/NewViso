use crate::RenderWorkBudget;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UploadPumpReason {
    Explicit,
    BeginFrame,
    LoadingScreenWarmup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadPumpDesc {
    pub reason: UploadPumpReason,
    pub budget: Option<RenderWorkBudget>,
}

impl UploadPumpDesc {
    #[inline]
    pub const fn explicit() -> Self {
        Self {
            reason: UploadPumpReason::Explicit,
            budget: None,
        }
    }

    #[inline]
    pub const fn loading_screen_warmup() -> Self {
        Self {
            reason: UploadPumpReason::LoadingScreenWarmup,
            budget: None,
        }
    }

    #[inline]
    pub fn with_budget(mut self, budget: RenderWorkBudget) -> Self {
        self.budget = Some(budget);
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UploadPumpReport {
    pub processed_jobs: u32,
    pub processed_bytes: u64,
    pub remaining_jobs: u32,
    pub remaining_bytes: u64,
    pub blocked_by_budget: bool,
    pub failed_jobs: u32,
}
