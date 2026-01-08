//! Security configuration for the jailer.
//!
//! This module re-exports security types from `crate::runtime::options`.
//! The actual definitions are in `runtime/options.rs` to keep related
//! configuration types together and avoid circular dependencies.

// Re-export security types from runtime::options
pub use crate::runtime::options::{ResourceLimits, SecurityOptions};
