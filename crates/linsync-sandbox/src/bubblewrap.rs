//! bubblewrap (bwrap) fallback sandbox for kernels < 5.13.
//!
//! Rebuilds the Command to run through bwrap with policy-derived bind mounts
//! and --unshare-net when network=false. seccomp-bpf is applied via
//! --seccomp <fd> so the privilege-escalation block still fires.

use seccompiler::BpfProgram;
use std::io::Write;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use crate::{SandboxError, SandboxPolicy};

/// Search $PATH for the `bwrap` binary.
pub(crate) fn find_bwrap() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join("bwrap");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Rewrite `cmd` to execute through `bwrap` with policy-derived bind mounts.
pub(crate) fn spawn_with_bwrap(
    cmd: Command,
    policy: SandboxPolicy,
    bwrap_path: &Path,
) -> Result<Child, SandboxError> {
    let orig_program = cmd.get_program().to_os_string();
    let orig_args: Vec<_> = cmd.get_args().map(|a| a.to_os_string()).collect();
    let orig_envs: Vec<_> = cmd
        .get_envs()
        .filter_map(|(k, v)| v.map(|v| (k.to_os_string(), v.to_os_string())))
        .collect();

    let mut bwrap_cmd = Command::new(bwrap_path);

    // Minimal system paths needed by most executables.
    let sys_ro: &[(&str, &str)] = &[
        ("/usr", "/usr"),
        ("/lib", "/lib"),
        ("/lib64", "/lib64"),
        ("/etc/ld.so.cache", "/etc/ld.so.cache"),
        ("/proc/self", "/proc/self"),
        ("/dev/null", "/dev/null"),
    ];
    for (src, dest) in sys_ro {
        if Path::new(src).exists() {
            bwrap_cmd.args(["--ro-bind", src, dest]);
        }
    }

    // Read-only plugin + source paths.
    for rp in &policy.read_paths {
        bwrap_cmd.arg("--ro-bind").arg(rp).arg(rp);
    }

    // Network-enabled plugins additionally need the resolver config + TLS trust
    // store bound in, otherwise getaddrinfo() fails inside the sandbox.
    if policy.network {
        for p in crate::network_resolution_read_paths() {
            if Path::new(p).exists() {
                bwrap_cmd.arg("--ro-bind").arg(p).arg(p);
            }
        }
    }

    // Read-write temp dir.
    for wp in &policy.write_paths {
        std::fs::create_dir_all(wp).ok();
        bwrap_cmd.arg("--bind").arg(wp).arg(wp);
    }

    if !policy.network {
        bwrap_cmd.arg("--unshare-net");
    }

    bwrap_cmd.arg("--unshare-pid");
    bwrap_cmd.arg("--die-with-parent");

    // Start from an empty environment and re-inject only an allowlist of
    // runtime essentials (PATH/HOME/locale/desktop-session) plus the caller's
    // explicitly-set variables. Host secrets present in the parent environment
    // are never propagated into the confined child.
    bwrap_cmd.arg("--clearenv");
    for (k, v) in crate::allowlisted_host_env() {
        bwrap_cmd.arg("--setenv").arg(k).arg(v);
    }
    // Caller's explicit vars (e.g. LINSYNC_PLUGIN_TEMP_DIR) win.
    for (k, v) in orig_envs {
        bwrap_cmd.arg("--setenv").arg(k).arg(v);
    }

    // Apply the same resource limits the Landlock path sets in its pre_exec.
    // Set on the bwrap process itself; setrlimit values are preserved across
    // bwrap's execve and inherited by the confined target it spawns, so the
    // fd/proc caps still bind the helper. Without this the bwrap fallback would
    // silently drop policy.fd_limit / policy.proc_limit.
    let fd_limit = policy.fd_limit;
    let proc_limit = policy.proc_limit;
    // SAFETY: pre_exec runs in the forked child before exec. set_rlimit calls
    // only setrlimit(2), which is async-signal-safe; no heap allocation or
    // locking — the same constraints the Landlock path's pre_exec observes.
    unsafe {
        bwrap_cmd.pre_exec(move || {
            set_rlimit(libc::RLIMIT_NOFILE as libc::__rlimit_resource_t, fd_limit)?;
            set_rlimit(libc::RLIMIT_NPROC as libc::__rlimit_resource_t, proc_limit)?;
            Ok(())
        });
    }

    // Compile seccomp filter and pass via a pipe fd to bwrap --seccomp.
    let seccomp_prog = crate::seccomp::build_seccomp_filter(policy.network)
        .map_err(|e| SandboxError::Os(std::io::Error::other(format!("{e:?}"))))?;

    let (read_fd, write_guard) = pipe_seccomp_filter(&seccomp_prog)?;
    bwrap_cmd
        .arg("--seccomp")
        .arg(read_fd.as_raw_fd().to_string());

    // Original command after separator.
    bwrap_cmd.arg("--").arg(orig_program).args(orig_args);

    // Keep the read fd open into bwrap (it is inherited by default; bwrap
    // reads and closes it before exec-ing the target).
    let child = bwrap_cmd.spawn().map_err(SandboxError::Os)?;
    // write_guard (the write end) is dropped here, closing it.
    drop(write_guard);
    Ok(child)
}

/// Set a soft+hard `setrlimit(2)` resource limit. Async-signal-safe — safe to
/// call from `pre_exec`. Mirrors the helper on the Landlock path (kept local
/// because that one is module-private to `landlock.rs`).
fn set_rlimit(resource: libc::__rlimit_resource_t, value: u64) -> std::io::Result<()> {
    let lim = libc::rlimit {
        rlim_cur: value,
        rlim_max: value,
    };
    let rc = unsafe { libc::setrlimit(resource, &lim) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Write the BPF program bytes to a pipe; return the (read_fd, write_guard).
/// The write guard must be kept alive until after bwrap forks, then dropped.
///
/// Each `sock_filter` instruction is 8 bytes: code(2LE) + jt(1) + jf(1) + k(4LE).
fn pipe_seccomp_filter(prog: &BpfProgram) -> Result<(OwnedFd, std::fs::File), SandboxError> {
    let mut fds = [0i32; 2];
    // Create both ends close-on-exec. The write end must NOT leak into the
    // bwrap child: if bwrap inherited a copy of the write end it would never
    // observe EOF on the --seccomp fd and would block forever. We then clear
    // close-on-exec on the read end alone, since bwrap must inherit it to read
    // the filter.
    let rc = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    if rc != 0 {
        return Err(SandboxError::Os(std::io::Error::last_os_error()));
    }
    if unsafe { libc::fcntl(fds[0], libc::F_SETFD, 0) } != 0 {
        let err = std::io::Error::last_os_error();
        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
        return Err(SandboxError::Os(err));
    }
    let read_fd = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let mut write_fd = unsafe { std::fs::File::from_raw_fd(fds[1]) };

    // Encode each BPF instruction as 8 bytes: code(2LE) + jt(1) + jf(1) + k(4LE).
    for insn in prog.iter() {
        write_fd
            .write_all(&insn.code.to_le_bytes())
            .map_err(SandboxError::Os)?;
        write_fd.write_all(&[insn.jt]).map_err(SandboxError::Os)?;
        write_fd.write_all(&[insn.jf]).map_err(SandboxError::Os)?;
        write_fd
            .write_all(&insn.k.to_le_bytes())
            .map_err(SandboxError::Os)?;
    }

    Ok((read_fd, write_fd))
}
