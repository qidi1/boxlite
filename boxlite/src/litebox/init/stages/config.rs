//! Stage 4: Configuration construction.
//!
//! Builds InstanceSpec from prepared components.
//! Includes disk creation (minimal I/O).

use crate::litebox::init::types::{
    ConfigInput, ConfigOutput, ResolvedVolume, RootfsPrepResult, resolve_user_volumes,
};
use crate::net::{NetworkBackendConfig, NetworkBackendFactory};
use crate::rootfs::operations::fix_rootfs_permissions;
use crate::runtime::constants::{guest_paths, mount_tags};
use crate::vmm::{Entrypoint, InstanceSpec, Mounts};
use crate::volumes::{BackingFormat, BlockDeviceManager, DiskFormat, Qcow2Helper};
use boxlite_shared::Transport;
use boxlite_shared::errors::BoxliteResult;
use std::collections::{HashMap, HashSet};

/// Build box configuration.
///
/// **Single Responsibility**: Assemble all config objects.
pub async fn run(input: ConfigInput<'_>) -> BoxliteResult<ConfigOutput> {
    // Transport setup
    let transport = Transport::unix(input.layout.socket_path());
    let ready_transport = Transport::unix(input.layout.ready_socket_path());

    let user_volumes = resolve_user_volumes(&input.options.volumes)?;

    let volumes = build_volume_config(input.layout, &input.rootfs.rootfs_result, &user_volumes)?;

    // Guest entrypoint
    let guest_entrypoint = build_guest_entrypoint(
        &transport,
        &ready_transport,
        input.init_rootfs,
        input.options,
    )?;

    // Network backend
    let network_backend = setup_networking(&input.rootfs.container_config, input.options)?;

    // Create disks based on rootfs strategy
    let (disk, is_cow_child, rootfs_disk) = create_disks(
        input.layout,
        &input.rootfs.image,
        &input.rootfs.rootfs_result,
    )
    .await?;

    // Register block devices
    let mut block_manager = BlockDeviceManager::new();
    block_manager.add_disk(disk.path(), DiskFormat::Qcow2);
    if let Some(ref rootfs) = rootfs_disk {
        block_manager.add_disk(rootfs.path(), DiskFormat::Qcow2);
    }

    // Create COW child disk for init rootfs (protects shared base from writes)
    let (init_rootfs, init_disk) =
        create_init_disk(input.layout, input.init_rootfs, &mut block_manager)?;

    let disks = block_manager.build();

    // Assemble config
    let box_config = InstanceSpec {
        cpus: input.options.cpus,
        memory_mib: input.options.memory_mib,
        volumes,
        disks,
        guest_entrypoint,
        transport: transport.clone(),
        ready_transport: ready_transport.clone(),
        init_rootfs,
        network_backend_endpoint: network_backend.as_ref().map(|b| b.endpoint()).transpose()?,
        home_dir: input.home_dir.clone(),
        console_output: None,
    };

    Ok(ConfigOutput {
        box_config,
        network_backend,
        disk,
        is_cow_child,
        user_volumes,
        rootfs_disk,
        init_disk,
    })
}

fn build_volume_config(
    layout: &crate::runtime::layout::BoxFilesystemLayout,
    rootfs_result: &RootfsPrepResult,
    user_volumes: &[ResolvedVolume],
) -> BoxliteResult<Mounts> {
    let rw_dir = layout.rw_dir();
    fix_rootfs_permissions(&rw_dir)?;

    let mut mounts = Mounts::new();

    mounts.add(mount_tags::RW, rw_dir, false);

    match rootfs_result {
        RootfsPrepResult::Merged(path) => {
            mounts.add(mount_tags::ROOTFS, path.clone(), false);
        }
        RootfsPrepResult::Layers { layers_dir, .. } => {
            mounts.add(mount_tags::LAYERS, layers_dir.clone(), true);
        }
        RootfsPrepResult::DiskImage { .. } => {
            // No virtiofs mounts needed for disk-based rootfs
            // The rootfs is on a block device
            tracing::debug!("Using disk-based rootfs, no virtiofs layers mount needed");
        }
    }

    for vol in user_volumes {
        mounts.add(&vol.tag, vol.host_path.clone(), vol.read_only);
    }

    Ok(mounts)
}

fn build_guest_entrypoint(
    transport: &Transport,
    ready_transport: &Transport,
    init_rootfs: &crate::runtime::initrf::InitRootfs,
    options: &crate::runtime::options::BoxOptions,
) -> BoxliteResult<Entrypoint> {
    let listen_uri = transport.to_uri();
    let ready_notify_uri = ready_transport.to_uri();

    // Start with init image's env
    let mut env: Vec<(String, String)> = init_rootfs.env.clone();

    // Override with user env vars
    for (key, value) in &options.env {
        env.retain(|(k, _)| k != key);
        env.push((key.clone(), value.clone()));
    }

    // Inject RUST_LOG from host
    if !env.iter().any(|(k, _)| k == "RUST_LOG")
        && let Ok(rust_log) = std::env::var("RUST_LOG")
        && !rust_log.is_empty()
    {
        env.push(("RUST_LOG".to_string(), rust_log));
    }

    Ok(Entrypoint {
        executable: format!("{}/boxlite-guest", guest_paths::BIN_DIR),
        args: vec![
            "--listen".to_string(),
            listen_uri,
            "--notify".to_string(),
            ready_notify_uri,
        ],
        env,
    })
}

fn setup_networking(
    container_config: &crate::images::ContainerConfig,
    options: &crate::runtime::options::BoxOptions,
) -> BoxliteResult<Option<Box<dyn crate::net::NetworkBackend>>> {
    let mut port_map: HashMap<u16, u16> = HashMap::new();

    // Step 1: Collect guest ports that user wants to customize
    // User-provided mappings should override image defaults for the same guest port
    let user_guest_ports: HashSet<u16> = options.ports.iter().map(|p| p.guest_port).collect();

    // Step 2: Image exposed ports (only add default 1:1 mapping if user didn't override)
    for port in container_config.tcp_ports() {
        if !user_guest_ports.contains(&port) {
            port_map.insert(port, port);
        }
    }

    // Step 3: User-provided mappings (always applied)
    for port in &options.ports {
        let host_port = port.host_port.unwrap_or(port.guest_port);
        port_map.insert(host_port, port.guest_port);
    }

    let final_mappings: Vec<(u16, u16)> = port_map.into_iter().collect();

    if !final_mappings.is_empty() {
        tracing::info!(
            "Port mappings: {} (image: {}, user: {}, overridden: {})",
            final_mappings.len(),
            container_config.exposed_ports.len(),
            options.ports.len(),
            user_guest_ports
                .intersection(&container_config.tcp_ports().into_iter().collect())
                .count()
        );
    }

    let config = NetworkBackendConfig::new(final_mappings);
    NetworkBackendFactory::create(config)
}

/// Create disks based on rootfs strategy.
///
/// Returns (data_disk, is_cow_child, rootfs_disk).
/// - data_disk: Always created (for writable data in overlayfs mode, or just data in disk mode)
/// - rootfs_disk: Only created when using disk-based rootfs
async fn create_disks(
    layout: &crate::runtime::layout::BoxFilesystemLayout,
    image: &crate::images::ImageObject,
    rootfs_result: &RootfsPrepResult,
) -> BoxliteResult<(crate::volumes::Disk, bool, Option<crate::volumes::Disk>)> {
    let qcow2_helper = Qcow2Helper::new();
    let disk_path = layout.disk_path();

    // Check if using disk-based rootfs
    if let RootfsPrepResult::DiskImage {
        base_disk_path,
        disk_size,
    } = rootfs_result
    {
        // Disk-based rootfs: create qcow2 COW overlay pointing to base ext4
        let rootfs_disk_path = layout.root().join("rootfs.qcow2");

        // Create qcow2 COW overlay for rootfs
        let rootfs_disk = qcow2_helper.create_cow_child_disk(
            base_disk_path,
            BackingFormat::Raw,
            &rootfs_disk_path,
            *disk_size,
        )?;
        tracing::info!(
            rootfs_disk = %rootfs_disk.path().display(),
            base_disk = %base_disk_path.display(),
            "Created rootfs COW overlay"
        );

        // Create a minimal data disk for any additional writable data
        // NOTE: The data disk is a fresh qcow2 without a filesystem, so is_cow_child=false
        // to ensure the guest formats it with ext4. The rootfs disk (vdb) is already
        // formatted as COW overlay of the base ext4.
        let disk = qcow2_helper.create_disk(&disk_path, false)?;
        tracing::info!(
            disk_path = %disk.path().display(),
            "Created data disk"
        );

        // is_cow_child=false: data disk needs formatting, rootfs disk is already formatted
        return Ok((disk, false, Some(rootfs_disk)));
    }

    // Overlayfs mode: check if we have a cached disk image for layers
    if let Some(disk_image) = image.disk_image().await {
        // COW child from existing qcow2 disk image
        let virtual_size = Qcow2Helper::qcow2_virtual_size(disk_image.path())?;
        let disk = qcow2_helper.create_cow_child_disk(
            disk_image.path(),
            BackingFormat::Qcow2,
            &disk_path,
            virtual_size,
        )?;
        tracing::info!(
            disk_path = %disk.path().display(),
            "Created COW child disk"
        );
        Ok((disk, true, None))
    } else {
        // New empty disk
        let disk = qcow2_helper.create_disk(&disk_path, false)?;
        tracing::info!(
            disk_path = %disk.path().display(),
            "Created empty disk for population"
        );
        Ok((disk, false, None))
    }
}

/// Create COW child disk for init rootfs.
///
/// Protects the shared base init rootfs from writes by creating a per-box
/// qcow2 overlay. Returns the updated InitRootfs with device path and the
/// COW disk (to prevent cleanup on drop).
fn create_init_disk(
    layout: &crate::runtime::layout::BoxFilesystemLayout,
    init_rootfs: &crate::runtime::initrf::InitRootfs,
    block_manager: &mut BlockDeviceManager,
) -> BoxliteResult<(
    crate::runtime::initrf::InitRootfs,
    Option<crate::volumes::Disk>,
)> {
    let mut init_rootfs = init_rootfs.clone();

    let init_disk = if let crate::runtime::initrf::Strategy::Disk { ref disk_path, .. } =
        init_rootfs.strategy
    {
        let base_disk_path = disk_path;

        // Get base disk size
        let base_size = std::fs::metadata(base_disk_path)
            .map(|m| m.len())
            .unwrap_or(512 * 1024 * 1024);

        // Create COW child disk
        let init_disk_path = layout.root().join("init.qcow2");
        let qcow2_helper = Qcow2Helper::new();
        let init_disk = qcow2_helper.create_cow_child_disk(
            base_disk_path,
            BackingFormat::Raw,
            &init_disk_path,
            base_size,
        )?;

        // Register COW child (not the base)
        let device_path = block_manager.add_disk(init_disk.path(), DiskFormat::Qcow2);

        // Update strategy with COW child disk path and device
        init_rootfs.strategy = crate::runtime::initrf::Strategy::Disk {
            disk_path: init_disk_path,
            device_path: Some(device_path),
        };

        Some(init_disk)
    } else {
        None
    };

    Ok((init_rootfs, init_disk))
}
