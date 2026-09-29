use crate::TextureId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuResourceResidencyState {
    Missing,
    Queued,
    Uploading,
    Ready,
    Failed,
}

impl Default for GpuResourceResidencyState {
    #[inline]
    fn default() -> Self {
        Self::Missing
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureResidencySnapshot {
    pub id: TextureId,
    pub state: GpuResourceResidencyState,
    pub queued_bytes: u64,
    pub uploaded_bytes: u64,
    pub label: Option<String>,
    pub message: Option<String>,
}

impl TextureResidencySnapshot {
    #[inline]
    pub fn missing(id: TextureId) -> Self {
        Self {
            id,
            state: GpuResourceResidencyState::Missing,
            queued_bytes: 0,
            uploaded_bytes: 0,
            label: None,
            message: None,
        }
    }

    #[inline]
    pub fn ready(id: TextureId, label: Option<String>) -> Self {
        Self {
            id,
            state: GpuResourceResidencyState::Ready,
            queued_bytes: 0,
            uploaded_bytes: 0,
            label,
            message: None,
        }
    }

    #[inline]
    pub fn queued(id: TextureId, bytes: u64, label: Option<String>) -> Self {
        Self {
            id,
            state: GpuResourceResidencyState::Queued,
            queued_bytes: bytes,
            uploaded_bytes: 0,
            label,
            message: None,
        }
    }

    #[inline]
    pub fn failed(id: TextureId, label: Option<String>, message: impl Into<String>) -> Self {
        Self {
            id,
            state: GpuResourceResidencyState::Failed,
            queued_bytes: 0,
            uploaded_bytes: 0,
            label,
            message: Some(message.into()),
        }
    }
}
