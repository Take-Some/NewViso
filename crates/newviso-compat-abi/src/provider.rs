#![allow(non_local_definitions)]

use abi_stable::{
    sabi_trait,
    std_types::{RBox, ROption, RResult, RString, RVec},
    StableAbi,
};

pub type Blob = RVec<u8>;
pub type CapabilityId = RString;
pub type MethodName = RString;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum PluginKind {
    Runtime = 1,
    Importer = 2,
    Editor = 3,
    Tool = 4,
    Other = 255,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum CapabilityRole {
    Provides = 1,
    Requires = 2,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum CapabilityKind {
    ServiceV1 = 1,
    EventsV1 = 2,
    AssetImporterV1 = 3,
    SceneContributionV1 = 4,
    Other = 255,
}

#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct CapabilityDesc {
    pub id: CapabilityId,
    pub role: CapabilityRole,
    pub kind: CapabilityKind,
    pub version: u32,
    pub describe_json: RString,
}

#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct PluginDescriptor {
    pub id: RString,
    pub name: RString,
    pub version: RString,
    pub kind: PluginKind,
    pub capabilities: RVec<CapabilityDesc>,
}

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

#[sabi_trait]
pub trait ServiceV1: Send + Sync {
    fn id(&self) -> CapabilityId;
    fn describe(&self) -> RString;
    fn call(&self, method: MethodName, payload: Blob) -> RResult<Blob, RString>;
}

pub type ServiceV1Dyn<'a> = ServiceV1_TO<'a, RBox<()>>;

#[sabi_trait]
pub trait EventSinkV1: Send + Sync {
    fn on_event(&mut self, topic: RString, payload: Blob);
}

pub type EventSinkV1Dyn<'a> = EventSinkV1_TO<'a, RBox<()>>;

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct HostApiV1 {
    pub log_info: extern "C" fn(RString),
    pub log_warn: extern "C" fn(RString),
    pub log_error: extern "C" fn(RString),
    pub register_service_v1: extern "C" fn(ServiceV1Dyn<'static>) -> RResult<(), RString>,
    pub call_service_v1: extern "C" fn(CapabilityId, MethodName, Blob) -> RResult<Blob, RString>,
    pub emit_event_v1: extern "C" fn(RString, Blob) -> RResult<(), RString>,
    pub subscribe_events_v1: extern "C" fn(EventSinkV1Dyn<'static>) -> RResult<(), RString>,
}

#[sabi_trait]
pub trait PluginModule: Send + Sync {
    fn descriptor(&self) -> PluginDescriptor;
    fn config_defaults(&self) -> RResult<ConfigBlobV1, RString>;
    fn config_apply_patches(
        &self,
        base: &ConfigBlobV1,
        patches: RVec<ConfigPatchV1>,
    ) -> RResult<ConfigApplyResultV1, RString>;
    fn config_supports_live_update(&self) -> bool;
    fn config_update_live(
        &mut self,
        effective: &ConfigBlobV1,
    ) -> RResult<RVec<ConfigDiagV1>, RString>;
    fn init(&mut self, host: HostApiV1, effective: ConfigBlobV1) -> RResult<(), RString>;
    fn start(&mut self) -> RResult<(), RString>;
    fn fixed_update(&mut self, dt: f32) -> RResult<(), RString>;
    fn update(&mut self, dt: f32) -> RResult<(), RString>;
    fn render(&mut self, dt: f32) -> RResult<(), RString>;
    fn shutdown(&mut self);
}

pub type PluginModuleDyn<'a> = PluginModule_TO<'a, RBox<()>>;

#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_ref = PluginRootV1Ref)))]
pub struct PluginRootV1 {
    #[sabi(last_prefix_field)]
    pub create: extern "C" fn() -> PluginModuleDyn<'static>,
}
