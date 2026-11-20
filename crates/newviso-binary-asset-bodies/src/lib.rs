#![forbid(unsafe_op_in_unsafe_fn)]
//! Native body formats for current NewViso asset dictionaries.
//!
//! The NEF8 file envelope is owned by the asset provider. Here we implement
//! the YDD/YSNCD uncompressed binary-body contracts, not a second VFS.
pub mod ydd_binary;
pub mod ysncd_binary;
pub mod ysncd_sound_graph;

pub use ydd_binary::{
    decode_ydd_binary_body, decode_ydd_binary_entries, decode_ydd_binary_entry,
    encode_ydd_binary_body, YDD_BINARY_ENCODING, YDD_BINARY_SCHEMA_VERSION,
};
pub use ysncd_binary::{
    decode_ysncd_binary_body, encode_ysncd_binary_body, YsncdAttenuation, YsncdClip, YsncdCue,
    YsncdCueDescriptor, YsncdDictionary, YSNCD_BINARY_MAGIC, YSNCD_BINARY_SCHEMA_VERSION,
};

/// Canonical stable asset-key hash (FNV-1a 64-bit).
pub fn stable_hash_from_text(value: &str) -> u64 {
    value
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
}
