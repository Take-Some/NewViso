//! Provider-neutral door mechanisms and passage geometry; metres, seconds and Nm.
mod config;
mod impact;
mod mechanism;
mod passage;
pub use config::*;
pub use impact::*;
pub use mechanism::*;
pub use passage::*;
#[cfg(test)]
mod tests;
