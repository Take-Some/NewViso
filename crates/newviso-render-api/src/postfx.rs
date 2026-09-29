#[inline]
pub(super) fn default_true() -> bool {
    true
}

#[path = "postfx/display.rs"]
mod display;
#[path = "postfx/frame.rs"]
mod frame;
#[path = "postfx/lighting.rs"]
mod lighting;
#[path = "postfx/pipeline.rs"]
mod pipeline;

pub use display::*;
pub use frame::*;
pub use lighting::*;
pub use pipeline::*;

#[cfg(test)]
#[path = "postfx/tests.rs"]
mod tests;
