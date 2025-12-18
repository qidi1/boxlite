//! Initialization stages.
//!
//! Each stage is a function with typed input/output.
//! Stages do ONE thing and have no side effects beyond their output.
//!
//! ## Stage Dependency Graph
//!
//! ```text
//! Filesystem ─────┐
//!                 │
//! Rootfs ─────────┼──→ Config ──→ Spawn ──→ Guest
//!                 │
//! GuestRootfs ────┘
//!
//! Parallel:   [Filesystem, Rootfs, GuestRootfs]
//! Sequential: Config → Spawn → Guest
//! ```

pub mod config;
pub mod filesystem;
pub mod guest;
pub mod guest_rootfs;
pub mod rootfs;
pub mod spawn;
