//! Guest rootfs types and metadata.

use std::path::{Path, PathBuf};

use boxlite_shared::errors::{BoxliteError, BoxliteResult};

/// A fully resolved and ready-to-use guest rootfs.
///
/// This struct represents the box's guest rootfs that runs boxlite-guest:
/// - Image pulled (if needed)
/// - Layers extracted/overlayed
/// - Guest binary injected and validated
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GuestRootfs {
    /// Path to the merged/final rootfs directory
    pub path: PathBuf,

    /// How this rootfs was prepared
    pub strategy: Strategy,

    /// Kernel images path (for Firecracker/microVM)
    pub kernel: Option<PathBuf>,

    /// Initrd path (optional)
    pub initrd: Option<PathBuf>,

    /// Environment variables from the init image config (e.g., PATH)
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

/// Strategy used to prepare the rootfs.
///
/// This tracks how the rootfs was assembled, which is important for:
/// - Cleanup logic (overlayfs mounts need unmounting)
/// - Debugging (understand which strategy was used)
/// - Performance metrics (compare overlay vs extraction)
#[derive(Clone, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum Strategy {
    /// Direct path provided by user (no processing needed)
    #[default]
    Direct,

    /// Layers extracted into a single directory
    ///
    /// Used on macOS (no overlayfs) and as fallback on Linux
    Extracted {
        /// Number of layers extracted
        layers: usize,
    },

    /// Linux overlayfs mount (requires cleanup on drop)
    ///
    /// This is the preferred strategy on Linux when CAP_SYS_ADMIN is available
    OverlayMount {
        /// Lower directories (read-only layers)
        lower: Vec<PathBuf>,
        /// Upper directory (writable layer)
        upper: PathBuf,
        /// Work directory (required by overlayfs)
        work: PathBuf,
    },

    /// Disk-based rootfs (ext4 disk image)
    ///
    /// The guest rootfs is stored in an ext4 disk image that the box boots from.
    /// This provides better performance than virtiofs for the guest rootfs.
    Disk {
        /// Path to the ext4 disk image
        disk_path: PathBuf,
        /// Device path in guest (e.g., "/dev/vdc").
        /// Set by build_disk_attachments when disks are configured.
        device_path: Option<String>,
    },
}

impl GuestRootfs {
    /// Create a new GuestRootfs, injecting the guest binary if needed.
    pub fn new(
        path: PathBuf,
        strategy: Strategy,
        kernel: Option<PathBuf>,
        initrd: Option<PathBuf>,
        env: Vec<(String, String)>,
    ) -> BoxliteResult<Self> {
        // Inject guest binary for directory-based strategies only.
        // For disk-based strategies, the guest binary was already included
        // during disk creation from the merged layers.
        match &strategy {
            Strategy::Disk { disk_path, .. } => {
                tracing::debug!(
                    "Skipping guest binary injection for disk-based rootfs: {}",
                    disk_path.display()
                );
            }
            _ => {
                crate::util::inject_guest_binary(&path)?;
            }
        }

        Ok(Self {
            path,
            strategy,
            kernel,
            initrd,
            env,
        })
    }

    /// Clean up this rootfs.
    ///
    /// Behavior depends on strategy:
    /// - `Direct`: No-op (user-provided path, don't delete)
    /// - `Extracted`: Remove the directory
    /// - `OverlayMount`: Unmount, then remove the directory
    ///
    /// Returns Ok(()) if cleanup succeeded or wasn't needed.
    pub fn cleanup(&self) -> BoxliteResult<()> {
        match &self.strategy {
            Strategy::Direct => {
                // User-provided path - don't clean up
                tracing::debug!(
                    "Skipping cleanup for direct rootfs: {}",
                    self.path.display()
                );
                Ok(())
            }
            Strategy::Extracted { layers } => {
                tracing::info!(
                    "Cleaning up extracted rootfs ({} layers): {}",
                    layers,
                    self.path.display()
                );
                // Remove parent directory (contains merged/)
                if let Some(parent) = self.path.parent() {
                    Self::remove_directory(parent)
                } else {
                    Self::remove_directory(&self.path)
                }
            }
            Strategy::OverlayMount { .. } => {
                tracing::info!("Cleaning up overlay mount: {}", self.path.display());

                #[cfg(target_os = "linux")]
                {
                    // Unmount overlay first
                    Self::unmount_overlay(&self.path)?;
                }

                // Remove parent directory (contains merged/, upper/, work/, patch/)
                if let Some(parent) = self.path.parent() {
                    Self::remove_directory(parent)
                } else {
                    Ok(())
                }
            }
            Strategy::Disk { disk_path, .. } => {
                // Disk-based rootfs: disk is managed by the cache, don't clean up
                tracing::debug!(
                    "Skipping cleanup for disk-based rootfs: {} (managed by cache)",
                    disk_path.display()
                );
                Ok(())
            }
        }
    }

    /// Unmount overlayfs (Linux only)
    #[cfg(target_os = "linux")]
    fn unmount_overlay(merged_dir: &Path) -> BoxliteResult<()> {
        if !merged_dir.exists() {
            return Ok(());
        }

        match std::process::Command::new("umount")
            .arg(merged_dir)
            .status()
        {
            Ok(status) if status.success() => {
                tracing::debug!("Unmounted overlay: {}", merged_dir.display());
                Ok(())
            }
            Ok(status) => {
                tracing::warn!(
                    "Failed to unmount overlay {}: exit status {}",
                    merged_dir.display(),
                    status
                );
                Err(BoxliteError::Storage(format!(
                    "umount failed with status {}",
                    status
                )))
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to execute umount for {}: {}",
                    merged_dir.display(),
                    e
                );
                Err(BoxliteError::Storage(format!(
                    "umount execution failed: {}",
                    e
                )))
            }
        }
    }

    /// Remove directory recursively
    fn remove_directory(path: &Path) -> BoxliteResult<()> {
        if let Err(e) = std::fs::remove_dir_all(path) {
            tracing::warn!(
                "Failed to cleanup rootfs directory {}: {}",
                path.display(),
                e
            );
            Err(BoxliteError::Storage(format!("cleanup failed: {}", e)))
        } else {
            tracing::info!("Cleaned up rootfs directory: {}", path.display());
            Ok(())
        }
    }
}
