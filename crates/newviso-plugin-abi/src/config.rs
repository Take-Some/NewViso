#![forbid(unsafe_op_in_unsafe_fn)]

use abi_stable::std_types::{ROption, RResult, RString, RVec};
use abi_stable::StableAbi;

#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct ConfigBlobV1 {
    pub content_type: RString,
    pub bytes: RVec<u8>,
    pub format_version: u32,
}

#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub enum ConfigPatchSourceV1 {
    File,
    Env,
    HostRule,
    Remote,
    Other,
}

#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct ConfigPatchV1 {
    pub source: ConfigPatchSourceV1,
    pub content_type: RString,
    pub bytes: RVec<u8>,
    pub priority: i32,
    pub name: RString,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, StableAbi)]
pub enum ConfigDiagLevelV1 {
    Info,
    Warn,
    Error,
}

#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct ConfigDiagV1 {
    pub level: ConfigDiagLevelV1,
    pub code: RString,
    pub message: RString,
    pub path: RString,
    pub patch_name: ROption<RString>,
}

#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct ConfigApplyResultV1 {
    pub effective: ConfigBlobV1,
    pub diags: RVec<ConfigDiagV1>,
    pub changed: bool,
}

/// Optional: keep as a separate trait if you want, but do NOT use it as a supertrait of #[sabi_trait] traits.
#[abi_stable::sabi_trait]
pub trait PluginConfigV1: Send + Sync {
    fn defaults(&self) -> RResult<ConfigBlobV1, RString>;

    fn apply_patches(
        &self,
        base: &ConfigBlobV1,
        patches: RVec<ConfigPatchV1>,
    ) -> RResult<ConfigApplyResultV1, RString>;

    fn supports_live_update(&self) -> bool;

    fn update_live(&self, effective: &ConfigBlobV1) -> RResult<RVec<ConfigDiagV1>, RString>;
}
