use serde::{Deserialize, Serialize};

pub const ASSET_STREAMING_SCHEMA_V1: &str = "newengine.assets.streaming.v1";
pub const ASSET_STREAMING_SCHEMA_V2: &str = "newengine.assets.streaming.v2";

/// Semantic reason for keeping an asset non-evictable.
///
/// Pins are reference-counted by `(class, owner)` so independent mission,
/// cutscene, interior and script systems cannot accidentally unpin each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssetStreamingPinClassV1 {
    #[default]
    Runtime,
    Mission,
    Cutscene,
    Interior,
    Script,
    Editor,
    Manual,
}

impl AssetStreamingPinClassV1 {
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Mission => "mission",
            Self::Cutscene => "cutscene",
            Self::Interior => "interior",
            Self::Script => "script",
            Self::Editor => "editor",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetStreamingRequestV1 {
    pub logical_path: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub pin: bool,
    #[serde(default)]
    pub pin_class: AssetStreamingPinClassV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetStreamingPinRequestV1 {
    pub logical_path: String,
    pub owner: String,
    #[serde(default)]
    pub class: AssetStreamingPinClassV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetStreamingTouchRequestV1 {
    pub logical_path: String,
    #[serde(default)]
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetStreamingCleanupRequestV1 {
    /// Desired maximum CPU decoded-blob footprint after cleanup. `None` uses the
    /// provider's configured/default budget.
    #[serde(default)]
    pub target_bytes: Option<u64>,
    /// Maximum number of blobs to evict in one call. Zero uses provider default.
    #[serde(default)]
    pub max_evictions: usize,
    /// Manual flush may evict every unpinned blob; required/reference pins still win.
    #[serde(default)]
    pub aggressive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetStreamingCleanupResponseV1 {
    pub ok: bool,
    pub before_bytes: u64,
    pub after_bytes: u64,
    pub freed_bytes: u64,
    pub evicted_assets: usize,
    pub pinned_assets: usize,
    pub target_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetStreamingStatsV1 {
    pub schema: String,
    pub resident_assets: usize,
    pub resident_bytes: u64,
    pub tracked_assets: usize,
    pub pinned_assets: usize,
    pub pin_references: u64,
    pub pending_requests: usize,
    pub cleanup_calls: u64,
    pub total_evictions: u64,
    pub total_freed_bytes: u64,
    pub compaction_calls: u64,
    pub budget_bytes: u64,
}

/// Provider execution state acknowledged back to the engine-owned residency scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssetStreamingLifecycleStateV2 {
    Queued,
    Loading,
    Resident,
    Failed,
    #[default]
    Unloaded,
}

/// Execute one scheduler-selected admission. Demand selection belongs to the engine control plane.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssetStreamingAdmitRequestV2 {
    pub logical_path: String,
    pub priority: i32,
    pub frame: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssetStreamingAdmitResponseV2 {
    pub schema: String,
    pub ok: bool,
    pub logical_path: String,
    pub id_u128: Option<String>,
    pub state: AssetStreamingLifecycleStateV2,
    pub resident_bytes: u64,
    pub pin_references: u64,
    pub error: Option<String>,
}

/// Execute one exact scheduler-selected eviction. Semantic pins remain authoritative.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssetStreamingEvictRequestV2 {
    pub logical_path: String,
    pub frame: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssetStreamingEvictResponseV2 {
    pub schema: String,
    pub ok: bool,
    pub logical_path: String,
    pub evicted: bool,
    pub blocked_by_pin: bool,
    pub freed_bytes: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssetStreamingLifecycleRequestV2 {
    pub logical_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssetStreamingLifecycleRowV2 {
    pub logical_path: String,
    pub id_u128: String,
    pub state: AssetStreamingLifecycleStateV2,
    pub resident_bytes: u64,
    pub pin_references: u64,
    pub priority: i32,
    pub last_touch_epoch: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AssetStreamingLifecycleResponseV2 {
    pub schema: String,
    pub rows: Vec<AssetStreamingLifecycleRowV2>,
}

#[cfg(test)]
mod v2_tests {
    use super::*;

    #[test]
    fn lifecycle_state_wire_is_stable() {
        assert_eq!(
            serde_json::to_string(&AssetStreamingLifecycleStateV2::Resident).unwrap(),
            "\"resident\""
        );
        assert_eq!(
            serde_json::from_str::<AssetStreamingLifecycleStateV2>("\"unloaded\"").unwrap(),
            AssetStreamingLifecycleStateV2::Unloaded
        );
    }

    #[test]
    fn admit_v2_defaults_are_non_demanding() {
        let request = AssetStreamingAdmitRequestV2::default();
        assert!(request.logical_path.is_empty());
        assert_eq!(request.priority, 0);
        assert_eq!(request.frame, 0);
    }
}
