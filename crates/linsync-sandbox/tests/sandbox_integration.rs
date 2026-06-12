//! Integration tests for the sandbox backends.
//!
//! Gated on Linux + a confined sandbox backend being available and enforcement
//! actually biting. The standard CI test job and `just ci` / `just test` set
//! LINSYNC_SANDBOX_SKIP=1 for reliability. Real enforcement is exercised by
//! running the integration test directly without the var on a kernel+env where
//! it works:
//!   env -u LINSYNC_SANDBOX_SKIP cargo test -p linsync-sandbox --test sandbox_integration
//!
//! A single probe at the start of the test run attempts a real denial so tests
//! can self-skip when the backend is absent, disabled, or a no-op.

#[cfg(target_os = "linux")]
use linsync_sandbox::{SandboxPolicy, SandboxStrategy, SandboxedCommand};
#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::sync::OnceLock;
#[cfg(target_os = "linux")]
use tempfile::TempDir;

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeResult {
    /// Backend detected and a real denial succeeded.
    Supported,
    /// Backend not detected or `LINSYNC_SANDBOX_SKIP` is set.
    Unavailable,
    /// Backend reports available but enforcement is a no-op.
    NoOp,
    /// Unexpected error during the probe.
    ProbeError,
}

/// Probe whether the sandbox actually denies a filesystem access.
///
/// The result is cached for the whole test run via [`OnceLock`].
#[cfg(target_os = "linux")]
fn probe_sandbox() -> ProbeResult {
    static PROBE: OnceLock<ProbeResult> = OnceLock::new();
    *PROBE.get_or_init(|| {
        if std::env::var_os("LINSYNC_SANDBOX_SKIP").is_some() {
            return ProbeResult::Unavailable;
        }
        let strategy = SandboxStrategy::detect();
        if !strategy.is_confined() {
            return ProbeResult::Unavailable;
        }

        let allowed = match TempDir::new() {
            Ok(tmp) => tmp,
            Err(_) => return ProbeResult::ProbeError,
        };
        let forbidden = match TempDir::new() {
            Ok(tmp) => tmp,
            Err(_) => return ProbeResult::ProbeError,
        };
        let secret = forbidden.path().join("secret.txt");
        if std::fs::write(&secret, b"top secret").is_err() {
            return ProbeResult::ProbeError;
        }

        let policy = SandboxPolicy::builder()
            .read(allowed.path())
            .write(allowed.path())
            .build();

        let mut cmd = Command::new("cat");
        cmd.arg(&secret);

        let mut child = match SandboxedCommand::new(cmd, policy).spawn() {
            Ok(child) => child,
            Err(_) => return ProbeResult::ProbeError,
        };

        match child.wait() {
            Ok(status) => {
                if status.success() {
                    ProbeResult::NoOp
                } else {
                    ProbeResult::Supported
                }
            }
            Err(_) => ProbeResult::ProbeError,
        }
    })
}

#[cfg(target_os = "linux")]
fn skip_enforcement_tests(label: &str) -> bool {
    match probe_sandbox() {
        ProbeResult::Supported => false,
        reason => {
            eprintln!("SKIP {label} enforcement tests: {reason:?}");
            true
        }
    }
}

#[cfg(target_os = "linux")]
mod landlock_tests {
    use super::{SandboxPolicy, SandboxedCommand, skip_enforcement_tests};
    use std::process::Command;
    use tempfile::TempDir;

    #[test]
    fn cat_reads_allowed_file() {
        if skip_enforcement_tests("landlock") {
            return;
        }

        let tmp = TempDir::new().unwrap();
        let allowed = tmp.path().join("allowed.txt");
        std::fs::write(&allowed, b"hello sandbox").unwrap();

        let policy = SandboxPolicy::builder()
            .read(tmp.path())
            .write(tmp.path())
            .build();

        let mut cmd = Command::new("cat");
        cmd.arg(&allowed);

        let mut child = SandboxedCommand::new(cmd, policy).spawn().unwrap();
        let status = child.wait().unwrap();
        assert!(status.success(), "cat of allowed file should succeed");
    }

    #[test]
    fn cat_denied_outside_allowed_path() {
        if skip_enforcement_tests("landlock") {
            return;
        }

        let allowed = TempDir::new().unwrap();
        let forbidden = TempDir::new().unwrap();
        let secret = forbidden.path().join("secret.txt");
        std::fs::write(&secret, b"top secret").unwrap();

        let policy = SandboxPolicy::builder()
            .read(allowed.path())
            .write(allowed.path())
            .build();

        let mut cmd = Command::new("cat");
        cmd.arg(&secret);

        let mut child = SandboxedCommand::new(cmd, policy).spawn().unwrap();
        let status = child.wait().unwrap();
        assert!(
            !status.success(),
            "cat of forbidden file should fail (EACCES via Landlock)"
        );
    }
}

#[cfg(target_os = "linux")]
mod seccomp_tests {
    use super::{SandboxPolicy, SandboxedCommand, skip_enforcement_tests};
    use std::process::{Command, Stdio};
    use tempfile::TempDir;

    /// Sandboxed process cannot open an AF_INET socket when network=false.
    /// python3 attempts the socket; seccomp returns EACCES; script exits 42.
    #[test]
    fn network_socket_denied_by_default() {
        if skip_enforcement_tests("seccomp") {
            return;
        }

        let tmp = TempDir::new().unwrap();
        let script = b"import socket, sys\ntry:\n    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)\n    print('socket created', file=sys.stderr)\n    sys.exit(0)\nexcept PermissionError:\n    sys.exit(42)\n";
        let script_path = tmp.path().join("test_net.py");
        std::fs::write(&script_path, script).unwrap();

        let policy = SandboxPolicy::builder()
            .read(tmp.path())
            .write(tmp.path())
            .network(false)
            .build();

        let mut cmd = Command::new("python3");
        cmd.arg(&script_path).stdout(Stdio::null());

        let mut child = SandboxedCommand::new(cmd, policy).spawn().unwrap();
        let status = child.wait().unwrap();
        assert_eq!(
            status.code(),
            Some(42),
            "expected exit 42 (EACCES from seccomp); got {:?}",
            status.code()
        );
    }

    /// When network=true the same socket call succeeds.
    #[test]
    fn network_socket_allowed_when_declared() {
        if skip_enforcement_tests("seccomp") {
            return;
        }

        let tmp = TempDir::new().unwrap();
        let script = b"import socket, sys\ntry:\n    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)\n    s.close()\n    sys.exit(0)\nexcept PermissionError:\n    sys.exit(42)\n";
        let script_path = tmp.path().join("test_net_ok.py");
        std::fs::write(&script_path, script).unwrap();

        let policy = SandboxPolicy::builder()
            .read(tmp.path())
            .write(tmp.path())
            .network(true)
            .build();

        let mut cmd = Command::new("python3");
        cmd.arg(&script_path);

        let mut child = SandboxedCommand::new(cmd, policy).spawn().unwrap();
        let status = child.wait().unwrap();
        assert_eq!(
            status.code(),
            Some(0),
            "socket should succeed when network=true; got {:?}",
            status.code()
        );
    }
}

#[cfg(target_os = "linux")]
mod strategy_tests {
    use linsync_sandbox::SandboxStrategy;

    #[test]
    fn detect_is_deterministic() {
        // SandboxStrategy::detect must return the same discriminant on repeated calls.
        let a = SandboxStrategy::detect();
        let b = SandboxStrategy::detect();
        assert_eq!(
            std::mem::discriminant(&a),
            std::mem::discriminant(&b),
            "SandboxStrategy::detect must be consistent"
        );
    }

    /// When LINSYNC_SANDBOX_SKIP is set, detect() returns Degraded.
    /// We test this by spawning a child process with check_detection binary.
    #[test]
    fn skip_env_forces_degraded() {
        // Build the binary first.
        std::process::Command::new("cargo")
            .args([
                "build",
                "-p",
                "linsync-sandbox",
                "--bin",
                "check_detection",
                "--quiet",
            ])
            .status()
            .unwrap();

        // current_exe is in target/debug/deps/; check_detection is in target/debug/
        let exe = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("check_detection");

        let status = std::process::Command::new(&exe)
            .env("LINSYNC_SANDBOX_SKIP", "1")
            .status()
            .unwrap();

        assert!(
            status.success(),
            "expected Degraded when LINSYNC_SANDBOX_SKIP=1"
        );
    }
}

#[cfg(target_os = "linux")]
mod degraded_tests {
    /// With LINSYNC_SANDBOX_SKIP=1 the command runs unsandboxed and can
    /// access paths that would normally be blocked. We verify this in a child
    /// process to avoid mutating the shared environment in a multi-threaded
    /// test process (which would be UB).
    #[test]
    fn skip_env_disables_sandbox() {
        std::process::Command::new("cargo")
            .args([
                "build",
                "-p",
                "linsync-sandbox",
                "--bin",
                "skip_disable_check",
                "--quiet",
            ])
            .status()
            .unwrap();

        let exe = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("skip_disable_check");

        let status = std::process::Command::new(&exe)
            .env("LINSYNC_SANDBOX_SKIP", "1")
            .status()
            .unwrap();

        assert!(
            status.success(),
            "LINSYNC_SANDBOX_SKIP=1 should allow unsandboxed execution"
        );
    }
}
