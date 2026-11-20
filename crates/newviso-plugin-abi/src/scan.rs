#![forbid(unsafe_op_in_unsafe_fn)]

use abi_stable::std_types::RString;
use abi_stable::StableAbi;

use crate::capability::PluginKind;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableAbi)]
pub enum PluginBootstrapPhase {
    Bootstrap = 1,
    Platform = 2,
    Engine = 3,
}

#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct PluginSignatureV1 {
    pub id: RString,
    pub name: RString,
    pub version: RString,
    pub kind: PluginKind,
    pub bootstrap_phase: PluginBootstrapPhase,
}
