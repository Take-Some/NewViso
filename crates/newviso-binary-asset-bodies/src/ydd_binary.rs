#![forbid(unsafe_op_in_unsafe_fn)]

//! Strict binary runtime contract for NEF8 `.ydd` drawable bodies.
//!
//! JSON geometry is intentionally rejected. Legacy assets must be migrated by
//! the offline packer before they enter a runtime asset catalog.

mod decode;
mod encode;
mod types;

pub use decode::{decode_ydd_binary_body, decode_ydd_binary_entries, decode_ydd_binary_entry};
pub use encode::encode_ydd_binary_body;
pub use types::{
    YddBinaryDocument, YddBinaryEntry, YddBinaryMesh, YddBinarySkinVertex, YddBinaryVertex,
    YDD_BINARY_CONTRACT_SPEC, YDD_BINARY_ENCODING, YDD_BINARY_ENCODING_V2, YDD_BINARY_ENCODING_V3,
    YDD_BINARY_ENCODING_V4, YDD_BINARY_SCHEMA_VERSION, YDD_BINARY_SCHEMA_VERSION_V2,
    YDD_BINARY_SCHEMA_VERSION_V3, YDD_BINARY_SCHEMA_VERSION_V4,
};

#[cfg(test)]
mod tests;
