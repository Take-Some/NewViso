use crate::{PipelineDesc, PipelineId, ShaderStage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShaderCachePolicy {
    PreferCache,
    ForceRebuild,
    ReadOnlyCache,
}

impl Default for ShaderCachePolicy {
    #[inline]
    fn default() -> Self {
        Self::PreferCache
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaderBakeRequest {
    pub logical_path: String,
    pub stage: ShaderStage,
    pub entry: String,
    pub source: String,
    #[serde(default)]
    pub cache_policy: ShaderCachePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaderBakeReport {
    pub logical_path: String,
    pub cache_hit: bool,
    pub words: u32,
    pub elapsed_ms: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShaderRuntimeCacheStats {
    pub memory_hits: u64,
    pub disk_hits: u64,
    pub misses: u64,
    pub writes: u64,
    pub failures: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineWarmupDesc {
    pub pipelines: Vec<PipelineDesc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PipelineWarmupReport {
    pub requested: u32,
    pub created: Vec<PipelineId>,
    pub failed: u32,
    pub elapsed_ms: f32,
}
