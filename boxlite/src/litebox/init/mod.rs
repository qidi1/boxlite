//! Box initialization orchestration.
//!
//! ## Architecture
//!
//! Initialization is split into stages executed by `InitPipeline`:
//!
//! ```text
//! Filesystem ─────┐
//!                 │
//! Rootfs ─────────┼──→ Config ──→ Spawn ──→ Guest
//!                 │
//! GuestRootfs ────┘
//!
//! Parallel:   [Filesystem, Rootfs, GuestRootfs]  (tokio::join!)
//! Sequential: Config → Spawn → Guest
//! ```
//!
//! `CleanupGuard` provides RAII cleanup on failure.

mod pipeline;
mod stages;
mod types;

pub(crate) use types::BoxInner;

use crate::BoxID;
use crate::runtime::RuntimeInner;
use crate::runtime::options::BoxOptions;
use boxlite_shared::errors::BoxliteResult;
use pipeline::InitPipeline;
use std::sync::Arc;

/// Builds and initializes box components.
///
/// # Example
///
/// ```ignore
/// let inner = BoxBuilder::new(box_id, runtime, options)
///     .build()
///     .await?;
/// ```
pub(crate) struct BoxBuilder {
    box_id: BoxID,
    runtime: RuntimeInner,
    options: BoxOptions,
}

impl BoxBuilder {
    /// Create a new builder.
    ///
    /// # Arguments
    ///
    /// * `box_id` - Unique identifier for this box
    /// * `runtime` - Runtime providing resources (layout, guest_rootfs, etc.)
    /// * `options` - Box configuration (image, memory, cpus, etc.)
    pub(crate) fn new(box_id: BoxID, runtime: RuntimeInner, options: BoxOptions) -> Self {
        Self {
            box_id,
            runtime,
            options,
        }
    }

    /// Build and initialize the box.
    ///
    /// Executes all initialization stages with automatic cleanup on failure.
    pub(crate) async fn build(self) -> BoxliteResult<BoxInner> {
        // Derive internal values from runtime
        let home_dir = self.runtime.non_sync_state.layout.home_dir().to_path_buf();
        let guest_rootfs_cell = Arc::clone(&self.runtime.non_sync_state.guest_rootfs);

        let pipeline = InitPipeline::new(
            self.box_id,
            home_dir,
            self.options,
            self.runtime,
            guest_rootfs_cell,
        );

        pipeline.run().await
    }
}
