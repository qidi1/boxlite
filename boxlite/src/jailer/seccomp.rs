//! Seccomp BPF filter generator for libkrun VMM process.
//!
//! This module generates a seccomp filter that whitelists syscalls
//! needed by the libkrun VMM process while blocking dangerous ones.
//!
//! The filter is generated as BPF bytecode that can be passed to
//! bubblewrap via `--seccomp <fd>`.
//!
//! ## Syscall Categories
//!
//! **ALLOWED** (needed for VMM operation):
//! - Memory management: mmap, munmap, mprotect, brk, madvise
//! - File I/O: read, write, openat, close, fstat, lseek
//! - KVM: ioctl with KVM_* commands
//! - Events: epoll_*, eventfd2, poll
//! - Networking: socket, connect, sendto, recvfrom (for gvproxy/vsock)
//! - Process: exit, exit_group, futex, clock_gettime
//!
//! **BLOCKED** (dangerous, attack vectors):
//! - mount, umount - filesystem manipulation
//! - ptrace - process debugging/control
//! - execve, execveat - execute new binaries
//! - init_module, finit_module - kernel module loading
//! - reboot - system reboot
//! - setns, unshare - namespace manipulation

#[cfg(target_os = "linux")]
use super::error::IsolationError;
use super::error::JailerError;
use std::collections::HashSet;

// Unused imports on non-Linux (kept for potential future use)
#[allow(unused_imports)]
use std::io::Write;
#[allow(unused_imports)]
use std::os::unix::io::{AsRawFd, RawFd};

/// Syscalls that libkrun VMM process needs to operate.
///
/// This whitelist is based on analysis of libkrun's operation.
/// When in doubt, it's better to allow a syscall than to break functionality.
pub const ALLOWED_SYSCALLS: &[&str] = &[
    // Memory management
    "brk",
    "mmap",
    "munmap",
    "mprotect",
    "madvise",
    "mremap",
    // File operations
    "read",
    "write",
    "pread64",
    "pwrite64",
    "readv",
    "writev",
    "openat",
    "close",
    "fstat",
    "newfstatat",
    "lseek",
    "fcntl",
    "dup",
    "dup2",
    "dup3",
    "pipe2",
    "statx",
    "access",
    "faccessat",
    "faccessat2",
    "readlink",
    "readlinkat",
    "getcwd",
    "getdents64",
    "unlink",
    "unlinkat",
    "mkdir",
    "mkdirat",
    "rmdir",
    "rename",
    "renameat",
    "renameat2",
    "symlink",
    "symlinkat",
    "ftruncate",
    "fallocate",
    "fsync",
    "fdatasync",
    // KVM operations (via ioctl)
    "ioctl",
    // Memory mapping for KVM
    "memfd_create",
    // Events and polling
    "epoll_create1",
    "epoll_ctl",
    "epoll_wait",
    "epoll_pwait",
    "epoll_pwait2",
    "eventfd2",
    "poll",
    "ppoll",
    "select",
    "pselect6",
    // Timers and clocks
    "clock_gettime",
    "clock_getres",
    "clock_nanosleep",
    "nanosleep",
    "gettimeofday",
    "timerfd_create",
    "timerfd_settime",
    "timerfd_gettime",
    // Signals
    "rt_sigaction",
    "rt_sigprocmask",
    "rt_sigreturn",
    "sigaltstack",
    // Threading
    "clone",
    "clone3",
    "futex",
    "set_robust_list",
    "get_robust_list",
    "rseq",
    "set_tid_address",
    "gettid",
    // Process info
    "getpid",
    "getppid",
    "getuid",
    "geteuid",
    "getgid",
    "getegid",
    "getgroups",
    // Process exit
    "exit",
    "exit_group",
    // Resource limits
    "getrlimit",
    "prlimit64",
    // Networking (for gvproxy/vsock)
    "socket",
    "socketpair",
    "connect",
    "accept",
    "accept4",
    "bind",
    "listen",
    "sendto",
    "recvfrom",
    "sendmsg",
    "recvmsg",
    "shutdown",
    "getsockname",
    "getpeername",
    "getsockopt",
    "setsockopt",
    // Misc
    "uname",
    "arch_prctl",
    "prctl",
    "getrandom",
    "sched_yield",
    "sched_getaffinity",
    "sched_setaffinity",
    "setpriority",
    "getpriority",
    // Landlock (security)
    "landlock_create_ruleset",
    "landlock_add_rule",
    "landlock_restrict_self",
];

/// Syscalls that are explicitly blocked (dangerous).
pub const BLOCKED_SYSCALLS: &[&str] = &[
    // Filesystem manipulation
    "mount",
    "umount",
    "umount2",
    "pivot_root",
    "chroot",
    // Process control
    "ptrace",
    "process_vm_readv",
    "process_vm_writev",
    // Execute new binaries (escape vector)
    "execve",
    "execveat",
    // Kernel module loading
    "init_module",
    "finit_module",
    "delete_module",
    // System control
    "reboot",
    "kexec_load",
    "kexec_file_load",
    // Namespace manipulation (already in namespace)
    "setns",
    "unshare",
    // Capability manipulation
    "capset",
    // Keyring (potential info leak)
    "keyctl",
    "add_key",
    "request_key",
    // BPF (kernel code execution)
    "bpf",
    // Userfaultfd (exploit helper)
    "userfaultfd",
    // Performance (info leak)
    "perf_event_open",
    // Process accounting
    "acct",
    // Swap
    "swapon",
    "swapoff",
    // Quotas
    "quotactl",
    "quotactl_fd",
];

/// Generate a seccomp filter description for logging/debugging.
pub fn describe_filter() -> String {
    let allowed: HashSet<&str> = ALLOWED_SYSCALLS.iter().copied().collect();
    let blocked: HashSet<&str> = BLOCKED_SYSCALLS.iter().copied().collect();

    format!(
        "Seccomp filter:\n  Allowed: {} syscalls\n  Blocked: {} syscalls\n  Default: TRAP (block with SIGSYS)",
        allowed.len(),
        blocked.len()
    )
}

/// Write a simple seccomp filter configuration for documentation.
///
/// Note: Actual BPF generation requires the `seccompiler` crate.
/// This function generates a JSON representation that can be used
/// with seccompiler or for documentation purposes.
pub fn generate_filter_json() -> String {
    let mut json = String::from(
        "{\n  \"main\": {\n    \"default_action\": \"trap\",\n    \"filter_action\": \"allow\",\n    \"filter\": [\n",
    );

    for (i, syscall) in ALLOWED_SYSCALLS.iter().enumerate() {
        if i > 0 {
            json.push_str(",\n");
        }
        json.push_str(&format!("      {{ \"syscall\": \"{}\" }}", syscall));
    }

    json.push_str("\n    ]\n  }\n}");
    json
}

/// Generate a seccomp BPF filter program.
///
/// Creates a filter that:
/// - **Allows** all syscalls in `ALLOWED_SYSCALLS`
/// - **Traps** (sends SIGSYS) for all other syscalls
///
/// The filter uses seccompiler to generate BPF bytecode that can be
/// applied to the current process.
///
/// # Errors
///
/// Returns an error if filter creation or BPF compilation fails.
#[cfg(target_os = "linux")]
pub fn generate_bpf_filter() -> Result<seccompiler::BpfProgram, JailerError> {
    use seccompiler::{SeccompAction, SeccompFilter, SeccompRule};
    use std::collections::BTreeMap;

    // Build rules map: syscall_number -> Vec<SeccompRule>
    // Empty rules vector = unconditional allow for that syscall
    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();

    let mut mapped_count = 0;
    let mut unmapped = Vec::new();

    for syscall_name in ALLOWED_SYSCALLS {
        if let Some(nr) = syscall_name_to_nr(syscall_name) {
            rules.insert(nr, vec![]); // Empty rules = allow unconditionally
            mapped_count += 1;
        } else {
            unmapped.push(*syscall_name);
        }
    }

    if !unmapped.is_empty() {
        tracing::warn!(
            unmapped_syscalls = ?unmapped,
            "Some syscalls could not be mapped to numbers (may not exist on this architecture)"
        );
    }

    tracing::debug!(
        total_syscalls = ALLOWED_SYSCALLS.len(),
        mapped = mapped_count,
        unmapped = unmapped.len(),
        "Building seccomp filter"
    );

    // Create filter with:
    // - Default action: Trap (SIGSYS for unknown syscalls)
    // - Filter action: Allow (for matched syscalls)
    let filter = SeccompFilter::new(
        rules,
        SeccompAction::Trap,  // Default: send SIGSYS for unlisted syscalls
        SeccompAction::Allow, // Match: allow the syscall
        target_arch(),
    )
    .map_err(|e| {
        JailerError::Isolation(IsolationError::Seccomp(format!(
            "Failed to create seccomp filter: {}",
            e
        )))
    })?;

    // Convert to BPF bytecode
    filter.try_into().map_err(|e: seccompiler::Error| {
        JailerError::Isolation(IsolationError::Seccomp(format!(
            "Failed to compile seccomp filter to BPF: {}",
            e
        )))
    })
}

/// Placeholder for non-Linux platforms.
///
/// Seccomp is Linux-specific, so this returns an empty filter on other platforms.
#[cfg(not(target_os = "linux"))]
pub fn generate_bpf_filter() -> Result<Vec<u8>, JailerError> {
    tracing::warn!("Seccomp is only available on Linux");
    Ok(Vec::new())
}

/// Apply a seccomp BPF filter to the current process.
///
/// Once applied, the filter cannot be removed. The process will be
/// restricted to the syscalls allowed by the filter.
///
/// # Safety
///
/// This permanently restricts the process. Ensure all required syscalls
/// are in the allowlist before calling.
#[cfg(target_os = "linux")]
pub fn apply_filter(filter: &seccompiler::BpfProgram) -> Result<(), JailerError> {
    seccompiler::apply_filter(filter).map_err(|e| {
        JailerError::Isolation(IsolationError::Seccomp(format!(
            "Failed to apply seccomp filter: {}",
            e
        )))
    })
}

/// Placeholder for non-Linux platforms.
#[cfg(not(target_os = "linux"))]
pub fn apply_filter(_filter: &[u8]) -> Result<(), JailerError> {
    tracing::warn!("Seccomp is only available on Linux, filter not applied");
    Ok(())
}

/// Get the target architecture for seccomp filter compilation.
#[cfg(target_os = "linux")]
fn target_arch() -> seccompiler::TargetArch {
    #[cfg(target_arch = "x86_64")]
    {
        seccompiler::TargetArch::x86_64
    }
    #[cfg(target_arch = "aarch64")]
    {
        seccompiler::TargetArch::aarch64
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        compile_error!("Unsupported architecture for seccomp")
    }
}

/// Map syscall name to syscall number.
///
/// Returns `None` if the syscall doesn't exist on the current architecture.
/// This is expected for some syscalls (e.g., `epoll_pwait2` on older kernels).
#[cfg(target_os = "linux")]
fn syscall_name_to_nr(name: &str) -> Option<i64> {
    // Map syscall names to libc::SYS_* constants
    // Note: Some syscalls may not exist on all architectures
    Some(match name {
        // Memory management
        "brk" => libc::SYS_brk,
        "mmap" => libc::SYS_mmap,
        "munmap" => libc::SYS_munmap,
        "mprotect" => libc::SYS_mprotect,
        "madvise" => libc::SYS_madvise,
        "mremap" => libc::SYS_mremap,

        // File operations
        "read" => libc::SYS_read,
        "write" => libc::SYS_write,
        "pread64" => libc::SYS_pread64,
        "pwrite64" => libc::SYS_pwrite64,
        "readv" => libc::SYS_readv,
        "writev" => libc::SYS_writev,
        "openat" => libc::SYS_openat,
        "close" => libc::SYS_close,
        "fstat" => libc::SYS_fstat,
        "newfstatat" => libc::SYS_newfstatat,
        "lseek" => libc::SYS_lseek,
        "fcntl" => libc::SYS_fcntl,
        "dup" => libc::SYS_dup,
        "dup2" => libc::SYS_dup2,
        "dup3" => libc::SYS_dup3,
        "pipe2" => libc::SYS_pipe2,
        "statx" => libc::SYS_statx,
        "access" => libc::SYS_access,
        "faccessat" => libc::SYS_faccessat,
        "faccessat2" => libc::SYS_faccessat2,
        "readlink" => libc::SYS_readlink,
        "readlinkat" => libc::SYS_readlinkat,
        "getcwd" => libc::SYS_getcwd,
        "getdents64" => libc::SYS_getdents64,
        "unlink" => libc::SYS_unlink,
        "unlinkat" => libc::SYS_unlinkat,
        "mkdir" => libc::SYS_mkdir,
        "mkdirat" => libc::SYS_mkdirat,
        "rmdir" => libc::SYS_rmdir,
        "rename" => libc::SYS_rename,
        "renameat" => libc::SYS_renameat,
        "renameat2" => libc::SYS_renameat2,
        "symlink" => libc::SYS_symlink,
        "symlinkat" => libc::SYS_symlinkat,
        "ftruncate" => libc::SYS_ftruncate,
        "fallocate" => libc::SYS_fallocate,
        "fsync" => libc::SYS_fsync,
        "fdatasync" => libc::SYS_fdatasync,

        // KVM operations
        "ioctl" => libc::SYS_ioctl,

        // Memory mapping for KVM
        "memfd_create" => libc::SYS_memfd_create,

        // Events and polling
        "epoll_create1" => libc::SYS_epoll_create1,
        "epoll_ctl" => libc::SYS_epoll_ctl,
        "epoll_wait" => libc::SYS_epoll_wait,
        "epoll_pwait" => libc::SYS_epoll_pwait,
        "epoll_pwait2" => libc::SYS_epoll_pwait2,
        "eventfd2" => libc::SYS_eventfd2,
        "poll" => libc::SYS_poll,
        "ppoll" => libc::SYS_ppoll,
        "select" => libc::SYS_select,
        "pselect6" => libc::SYS_pselect6,

        // Timers and clocks
        "clock_gettime" => libc::SYS_clock_gettime,
        "clock_getres" => libc::SYS_clock_getres,
        "clock_nanosleep" => libc::SYS_clock_nanosleep,
        "nanosleep" => libc::SYS_nanosleep,
        "gettimeofday" => libc::SYS_gettimeofday,
        "timerfd_create" => libc::SYS_timerfd_create,
        "timerfd_settime" => libc::SYS_timerfd_settime,
        "timerfd_gettime" => libc::SYS_timerfd_gettime,

        // Signals
        "rt_sigaction" => libc::SYS_rt_sigaction,
        "rt_sigprocmask" => libc::SYS_rt_sigprocmask,
        "rt_sigreturn" => libc::SYS_rt_sigreturn,
        "sigaltstack" => libc::SYS_sigaltstack,

        // Threading
        "clone" => libc::SYS_clone,
        "clone3" => libc::SYS_clone3,
        "futex" => libc::SYS_futex,
        "set_robust_list" => libc::SYS_set_robust_list,
        "get_robust_list" => libc::SYS_get_robust_list,
        "rseq" => libc::SYS_rseq,
        "set_tid_address" => libc::SYS_set_tid_address,
        "gettid" => libc::SYS_gettid,

        // Process info
        "getpid" => libc::SYS_getpid,
        "getppid" => libc::SYS_getppid,
        "getuid" => libc::SYS_getuid,
        "geteuid" => libc::SYS_geteuid,
        "getgid" => libc::SYS_getgid,
        "getegid" => libc::SYS_getegid,
        "getgroups" => libc::SYS_getgroups,

        // Process exit
        "exit" => libc::SYS_exit,
        "exit_group" => libc::SYS_exit_group,

        // Resource limits
        "getrlimit" => libc::SYS_getrlimit,
        "prlimit64" => libc::SYS_prlimit64,

        // Networking
        "socket" => libc::SYS_socket,
        "socketpair" => libc::SYS_socketpair,
        "connect" => libc::SYS_connect,
        "accept" => libc::SYS_accept,
        "accept4" => libc::SYS_accept4,
        "bind" => libc::SYS_bind,
        "listen" => libc::SYS_listen,
        "sendto" => libc::SYS_sendto,
        "recvfrom" => libc::SYS_recvfrom,
        "sendmsg" => libc::SYS_sendmsg,
        "recvmsg" => libc::SYS_recvmsg,
        "shutdown" => libc::SYS_shutdown,
        "getsockname" => libc::SYS_getsockname,
        "getpeername" => libc::SYS_getpeername,
        "getsockopt" => libc::SYS_getsockopt,
        "setsockopt" => libc::SYS_setsockopt,

        // Misc
        "uname" => libc::SYS_uname,
        "arch_prctl" => libc::SYS_arch_prctl,
        "prctl" => libc::SYS_prctl,
        "getrandom" => libc::SYS_getrandom,
        "sched_yield" => libc::SYS_sched_yield,
        "sched_getaffinity" => libc::SYS_sched_getaffinity,
        "sched_setaffinity" => libc::SYS_sched_setaffinity,
        "setpriority" => libc::SYS_setpriority,
        "getpriority" => libc::SYS_getpriority,

        // Landlock (security)
        "landlock_create_ruleset" => libc::SYS_landlock_create_ruleset,
        "landlock_add_rule" => libc::SYS_landlock_add_rule,
        "landlock_restrict_self" => libc::SYS_landlock_restrict_self,

        // Unknown syscall
        _ => return None,
    })
}

/// Check if a syscall is in the allowed list.
pub fn is_allowed(syscall: &str) -> bool {
    ALLOWED_SYSCALLS.contains(&syscall)
}

/// Check if a syscall is explicitly blocked.
pub fn is_blocked(syscall: &str) -> bool {
    BLOCKED_SYSCALLS.contains(&syscall)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowed_syscalls() {
        assert!(is_allowed("read"));
        assert!(is_allowed("write"));
        assert!(is_allowed("mmap"));
        assert!(is_allowed("ioctl")); // KVM
        assert!(is_allowed("socket")); // gvproxy
    }

    #[test]
    fn test_blocked_syscalls() {
        assert!(is_blocked("mount"));
        assert!(is_blocked("ptrace"));
        assert!(is_blocked("execve"));
        assert!(is_blocked("reboot"));
        assert!(is_blocked("bpf"));
    }

    #[test]
    fn test_no_overlap() {
        // Ensure no syscall is both allowed and blocked
        let allowed: HashSet<&str> = ALLOWED_SYSCALLS.iter().copied().collect();
        let blocked: HashSet<&str> = BLOCKED_SYSCALLS.iter().copied().collect();

        let overlap: Vec<_> = allowed.intersection(&blocked).collect();
        assert!(
            overlap.is_empty(),
            "Syscalls should not be both allowed and blocked: {:?}",
            overlap
        );
    }

    #[test]
    fn test_filter_description() {
        let desc = describe_filter();
        assert!(desc.contains("Allowed:"));
        assert!(desc.contains("Blocked:"));
    }

    #[test]
    fn test_generate_json() {
        let json = generate_filter_json();
        assert!(json.contains("\"default_action\": \"trap\""));
        assert!(json.contains("\"filter_action\": \"allow\""));
        assert!(json.contains("\"syscall\": \"read\""));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_generate_bpf_filter() {
        // Test that BPF filter generation succeeds
        let result = generate_bpf_filter();
        assert!(result.is_ok(), "BPF filter generation should succeed");

        let bpf = result.unwrap();
        // BPF program should not be empty
        assert!(!bpf.is_empty(), "BPF program should not be empty");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_syscall_name_to_nr() {
        // Test common syscalls map correctly
        assert!(syscall_name_to_nr("read").is_some());
        assert!(syscall_name_to_nr("write").is_some());
        assert!(syscall_name_to_nr("mmap").is_some());
        assert!(syscall_name_to_nr("ioctl").is_some());

        // Test unknown syscall returns None
        assert!(syscall_name_to_nr("nonexistent_syscall").is_none());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_all_allowed_syscalls_mapped() {
        // Verify most syscalls can be mapped (some may not exist on all architectures)
        let mut unmapped = Vec::new();
        let mut mapped = 0;

        for syscall in ALLOWED_SYSCALLS {
            if syscall_name_to_nr(syscall).is_some() {
                mapped += 1;
            } else {
                unmapped.push(*syscall);
            }
        }

        // At least 90% of syscalls should be mapped
        let min_mapped = (ALLOWED_SYSCALLS.len() * 90) / 100;
        assert!(
            mapped >= min_mapped,
            "Expected at least {} mapped syscalls, got {}. Unmapped: {:?}",
            min_mapped,
            mapped,
            unmapped
        );
    }
}
