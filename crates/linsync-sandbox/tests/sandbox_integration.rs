//! Integration tests for the sandbox backends.
//!
//! Gated on Linux + Landlock ABI >= 1 being available + LINSYNC_SANDBOX_SKIP not set.
//! The standard CI test job and `just ci` / `just test` set LINSYNC_SANDBOX_SKIP=1
//! for reliability (Landlock probe can report ABI>=1 but fs enforcement may be a
//! no-op in containers/CI/dev shells). Real enforcement is exercised by running
//! the integration test directly without the var on a kernel+env where it works:
//!   env -u LINSYNC_SANDBOX_SKIP cargo test -p linsync-sandbox --test sandbox_integration
//!
//! (There is no active separate sandbox-stress job in ci.yml at present.)

#[cfg(target_os = "linux")]
mod landlock_tests {
    use linsync_sandbox::{SandboxPolicy, SandboxStrategy, SandboxedCommand};
    use std::process::Command;
    use tempfile::TempDir;

    fn skip_if_unavailable() -> bool {
        if std::env::var_os("LINSYNC_SANDBOX_SKIP").is_some() {
            eprintln!("SKIP: LINSYNC_SANDBOX_SKIP is set");
            return true;
        }
        if !matches!(SandboxStrategy::detect(), SandboxStrategy::Landlock) {
            eprintln!("SKIP: Landlock not available on this kernel");
            return true;
        }
        false
    }

    #[test]
    fn cat_reads_allowed_file() {
        if skip_if_unavailable() {
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
        if skip_if_unavailable() {
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
    use linsync_sandbox::{SandboxPolicy, SandboxStrategy, SandboxedCommand};
    use std::process::{Command, Stdio};
    use tempfile::TempDir;

    fn skip_if_unavailable() -> bool {
        if std::env::var_os("LINSYNC_SANDBOX_SKIP").is_some() {
            eprintln!("SKIP: LINSYNC_SANDBOX_SKIP is set");
            return true;
        }
        matches!(
            SandboxStrategy::detect(),
            linsync_sandbox::SandboxStrategy::Degraded
        ) && {
            eprintln!("SKIP: no sandbox backend available");
            true
        }
    }

    /// Sandboxed process cannot open an AF_INET socket when network=false.
    /// python3 attempts the socket; seccomp returns EACCES; script exits 42.
    #[test]
    fn network_socket_denied_by_default() {
        if skip_if_unavailable() {
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
        if skip_if_unavailable() {
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
    use linsync_sandbox::{SandboxPolicy, SandboxedCommand};
    use std::process::{Command, Stdio};
    use tempfile::TempDir;

    /// With LINSYNC_SANDBOX_SKIP=1 the command runs unsandboxed and can
    /// access paths that would normally be blocked.
    #[test]
    fn skip_env_disables_sandbox() {
        // SAFETY: set_var is unsafe in multi-threaded tests. This test is
        // designed to run in isolation or with --test-threads=1.
        // Restore (not blindly remove) the prior value: when the whole suite
        // runs with LINSYNC_SANDBOX_SKIP=1 (the documented CI mode), unsetting
        // it here would race sibling tests into a real-sandbox path.
        let prev = std::env::var_os("LINSYNC_SANDBOX_SKIP");
        unsafe { std::env::set_var("LINSYNC_SANDBOX_SKIP", "1") };
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("data.txt");
        std::fs::write(&file, b"unsandboxed").unwrap();

        let policy = SandboxPolicy::builder().build(); // no allowed paths

        let mut cmd = Command::new("cat");
        cmd.arg(&file).stdout(Stdio::null());

        let mut child = SandboxedCommand::new(cmd, policy).spawn().unwrap();
        let status = child.wait().unwrap();
        unsafe {
            match prev {
                Some(value) => std::env::set_var("LINSYNC_SANDBOX_SKIP", value),
                None => std::env::remove_var("LINSYNC_SANDBOX_SKIP"),
            }
        }
        assert!(status.success(), "unsandboxed cat should succeed");
    }
}
