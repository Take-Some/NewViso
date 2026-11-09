//! Renderer/provider-free scene authoring contract shared by runtime and tools.
mod character;
mod persistence;
pub use character::{CharacterUpdatePhase, SceneCharacterBinding};
mod registry;
pub use persistence::write_authoring_file;
mod document;
pub use document::*;
pub use registry::*;

#[cfg(test)]
mod tests;
