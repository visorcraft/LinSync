//! Landlock filesystem-restriction backend.
//!
//! Called from the child side of fork() via pre_exec. All operations here
//! must be async-signal-safe: no heap allocation, no locking, no stdio.
//! The `landlock` crate uses only raw syscalls and meets this requirement.

use landlock::{
    ABI, Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
    RulesetCreatedAttr,
};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};

use crate::{SandboxError, SandboxPolicy};

/// Return the Landlock ABI version supported by the running kernel, or 0 if
/// the syscall is not available.
pub(crate) fn landlock_abi_version() -> u32 {
    // Probe via the raw syscall: landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)
    let version = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<libc::c_void>(),
            0usize,
            1u32, // LANDLOCK_CREATE_RULESET_VERSION flag
        )
    };
    if version < 0 { 0 } else { version as u32 }
}

/// Map a raw ABI version number to an `ABI` enum value (capped at the highest known).
fn abi_from_version(v: u32) -> ABI {
    match v {
        0 => ABI::Unsupported,
        1 => ABI::V1,
        2 => ABI::V2,
        3 => ABI::V3,
        4 => ABI::V4,
        5 => ABI::V5,
        6 => ABI::V6,
        _ => ABI::V7,
    }
}

/// Spawn `cmd` with Landlock filesystem restrictions applied in the child
/// before exec. seccomp-bpf is also installed in the child.
pub(crate) fn spawn_with_landlock(
    mut cmd: Command,
    policy: SandboxPolicy,
) -> Result<Child, SandboxError> {
    // Sanitize the child environment. Landlock confines the filesystem but
    // does nothing about environment-borne secrets, so — matching the
    // bubblewrap backend's `--clearenv` discipline — drop the host environment
    // and re-inject only an allowlist of runtime essentials plus the caller's
    // explicitly-set variables (e.g. LINSYNC_PLUGIN_TEMP_DIR).
    {
        let explicit: Vec<(std::ffi::OsString, std::ffi::OsString)> = cmd
            .get_envs()
            .filter_map(|(k, v)| v.map(|v| (k.to_os_string(), v.to_os_string())))
            .collect();
        cmd.env_clear();
        for (k, v) in crate::allowlisted_host_env() {
            cmd.env(k, v);
        }
        // Caller's explicit vars win over the host allowlist.
        for (k, v) in explicit {
            cmd.env(k, v);
        }
    }

    // Compile the seccomp filter in the parent (no allocation after fork).
    let seccomp_program = crate::seccomp::build_seccomp_filter(policy.network)
        .map_err(|e| SandboxError::Os(std::io::Error::other(format!("{e:?}"))))?;

    // Clone data needed in the pre_exec closure.
    // Always include essential system read paths so the child can exec binaries
    // and load shared libraries.
    let sys_read_paths: &[&str] = &[
        "/usr",
        "/lib",
        "/lib64",
        "/etc/ld.so.cache",
        "/etc/alternatives",
        "/proc/self",
        "/dev/null",
        "/dev/urandom",
        "/dev/random",
    ];
    let mut read_paths: Vec<PathBuf> = policy.read_paths.clone();
    for p in sys_read_paths {
        let pb = PathBuf::from(p);
        if pb.exists() {
            read_paths.push(pb);
        }
    }
    // Network-enabled plugins additionally need the resolver config + TLS trust
    // store, otherwise getaddrinfo() fails inside the sandbox (EAI_AGAIN).
    if policy.network {
        for p in crate::network_resolution_read_paths() {
            let pb = PathBuf::from(p);
            if pb.exists() {
                read_paths.push(pb);
            }
        }
    }
    let write_paths: Vec<PathBuf> = policy.write_paths.clone();
    let fd_limit = policy.fd_limit;
    let proc_limit = policy.proc_limit;
    let abi_ver = landlock_abi_version();

    // SAFETY: pre_exec runs after fork. We use only async-signal-safe calls:
    // setrlimit(2), raw Landlock syscalls (via the landlock crate), and
    // prctl/seccomp (via seccompiler). No heap allocation or locking.
    unsafe {
        cmd.pre_exec(move || {
            // 1. Resource limits (fork-bomb + fd-leak prevention).
            crate::set_rlimit(libc::RLIMIT_NOFILE as libc::__rlimit_resource_t, fd_limit)?;
            crate::set_rlimit(libc::RLIMIT_NPROC as libc::__rlimit_resource_t, proc_limit)?;

            // 2. Landlock filesystem restrictions.
            apply_landlock_policy(&read_paths, &write_paths, abi_ver)?;

            // 3. seccomp-bpf (network block + privilege-escalation block).
            crate::seccomp::apply_seccomp_filter(&seccomp_program)?;

            Ok(())
        });
    }

    crate::spawn_retrying_etxtbsy(&mut cmd)
}

fn apply_landlock_policy(
    read_paths: &[PathBuf],
    write_paths: &[PathBuf],
    abi_ver: u32,
) -> std::io::Result<()> {
    let abi = abi_from_version(abi_ver);

    // Read rights: read files, list dirs, execute binaries.
    let read_rights = AccessFs::from_read(abi) | AccessFs::Execute;

    // Write rights: all access rights for the given ABI.
    let write_rights = AccessFs::from_all(abi);

    let ruleset_result = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_all(abi))
        .map_err(io_err)?
        .create()
        .map_err(io_err)?;

    let mut ruleset = ruleset_result;

    for path in read_paths {
        if let Ok(fd) = PathFd::new(path) {
            let effective_read = read_rights & AccessFs::from_all(abi);
            let rule =
                PathBeneath::new(fd, effective_read).set_compatibility(CompatLevel::BestEffort);
            ruleset = ruleset.add_rule(rule).map_err(io_err)?;
        }
    }

    for path in write_paths {
        if let Ok(fd) = PathFd::new(path) {
            let effective_write = write_rights & AccessFs::from_all(abi);
            let rule =
                PathBeneath::new(fd, effective_write).set_compatibility(CompatLevel::BestEffort);
            ruleset = ruleset.add_rule(rule).map_err(io_err)?;
        }
    }

    ruleset.restrict_self().map_err(io_err)?;

    Ok(())
}

fn io_err<E: std::fmt::Debug>(e: E) -> std::io::Error {
    std::io::Error::other(format!("{e:?}"))
}
