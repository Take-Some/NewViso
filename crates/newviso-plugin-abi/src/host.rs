#![forbid(unsafe_op_in_unsafe_fn)]

use abi_stable::std_types::{RResult, RString};
use abi_stable::StableAbi;

use crate::service::{EventSinkV1Dyn, ServiceV1Dyn};
use crate::types::{Blob, CapabilityId, MethodName};

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
