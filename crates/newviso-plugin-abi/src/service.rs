#![forbid(unsafe_op_in_unsafe_fn)]

use abi_stable::sabi_trait;
use abi_stable::std_types::{RResult, RString};
use abi_stable::StableAbi;

use crate::types::{Blob, CapabilityId, MethodName};

#[sabi_trait]
pub trait ServiceV1: Send + Sync {
    fn id(&self) -> CapabilityId;
    fn describe(&self) -> RString;
    fn call(&self, method: MethodName, payload: Blob) -> RResult<Blob, RString>;
}

pub type ServiceV1Dyn<'a> = ServiceV1_TO<'a, abi_stable::std_types::RBox<()>>;

#[sabi_trait]
pub trait EventSinkV1: Send + Sync {
    fn on_event(&mut self, topic: RString, payload: Blob);
}

pub type EventSinkV1Dyn<'a> = EventSinkV1_TO<'a, abi_stable::std_types::RBox<()>>;

#[repr(C)]
#[derive(Clone, StableAbi)]
pub struct ServiceCall {
    pub service_id: CapabilityId,
    pub method: MethodName,
    pub payload: Blob,
}
