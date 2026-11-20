#![forbid(unsafe_op_in_unsafe_fn)]
//! Provider-neutral NewViso asset contracts. No runtime/host or SDK dependency.
mod raw_range;
pub use raw_range::*;
mod asset_error;
pub use asset_error::*;
mod asset_lifecycle;
pub use asset_lifecycle::*;
mod asset_ref;
pub use asset_ref::*;
mod asset_streaming;
pub use asset_streaming::*;
mod file_types;
pub use file_types::*;
mod texture_assets;
pub use texture_assets::*;
mod map_assets;
pub use map_assets::*;
mod pipeline;
pub use pipeline::*;
mod source_dictionary;
pub use source_dictionary::*;
pub mod list_file;
pub use list_file::*;
include!("lib_contracts/gateway_contracts.rs");
include!("lib_contracts/backend_filetypes.rs");
include!("lib_contracts/methods.rs");
include!("lib_contracts/runtime_contracts.rs");
#[cfg(test)]
mod file_type_tests;
