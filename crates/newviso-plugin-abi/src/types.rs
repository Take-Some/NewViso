#![forbid(unsafe_op_in_unsafe_fn)]

use abi_stable::std_types::{RResult, RString, RVec};

pub type Blob = RVec<u8>;
pub type CapabilityId = RString;
pub type MethodName = RString;

pub type AbiResult<T> = RResult<T, RString>;

#[inline]
pub fn ok<T>(v: T) -> AbiResult<T> {
    RResult::ROk(v)
}

#[inline]
pub fn err<T>(msg: impl Into<RString>) -> AbiResult<T> {
    RResult::RErr(msg.into())
}
