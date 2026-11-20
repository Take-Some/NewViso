#![forbid(unsafe_op_in_unsafe_fn)]

use abi_stable::sabi_trait;
use abi_stable::std_types::{RBox, RResult, RString, RVec};
use abi_stable::StableAbi;

use crate::capability::PluginDescriptor;
use crate::config::{ConfigApplyResultV1, ConfigBlobV1, ConfigDiagV1, ConfigPatchV1};
use crate::host::HostApiV1;

#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct PluginInfo {
    pub id: RString,
    pub name: RString,
    pub version: RString,
}

/// Canonical plugin module contract.
///
/// This is the former V3 contract renamed to the unversioned public ABI. Versioning
/// belongs in `PluginDescriptor`, `ConfigBlobV1` format metadata and root module
/// package metadata, not in parallel trait names or create callbacks.
#[sabi_trait]
pub trait PluginModule: Send + Sync {
    /// Plugin descriptor: id/kind/capabilities/contracts.
    fn descriptor(&self) -> PluginDescriptor;

    /// Returns plugin default configuration blob.
    fn config_defaults(&self) -> RResult<ConfigBlobV1, RString>;

    /// Applies patches, migrations and validation; returns effective config and diagnostics.
    fn config_apply_patches(
        &self,
        base: &ConfigBlobV1,
        patches: RVec<ConfigPatchV1>,
    ) -> RResult<ConfigApplyResultV1, RString>;

    /// Whether live config updates are supported after init.
    fn config_supports_live_update(&self) -> bool;

    /// Applies effective config live after init.
    /// Should return diagnostics; use Error level if restart is required.
    fn config_update_live(
        &mut self,
        effective: &ConfigBlobV1,
    ) -> RResult<RVec<ConfigDiagV1>, RString>;

    /// Initializes plugin with effective config.
    fn init(&mut self, host: HostApiV1, effective: ConfigBlobV1) -> RResult<(), RString>;

    fn start(&mut self) -> RResult<(), RString>;
    fn fixed_update(&mut self, dt: f32) -> RResult<(), RString>;
    fn update(&mut self, dt: f32) -> RResult<(), RString>;
    fn render(&mut self, dt: f32) -> RResult<(), RString>;
    fn shutdown(&mut self);
}

pub type PluginModuleDyn<'a> = PluginModule_TO<'a, RBox<()>>;
