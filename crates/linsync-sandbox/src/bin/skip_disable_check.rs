//! Child-process helper for `degraded_tests::skip_env_disables_sandbox`.
//!
//! Runs `cat` on a file with a sandbox policy that allows no paths, while
//! honoring `LINSYNC_SANDBOX_SKIP`. Exits 0 only if the unsandboxed cat
//! succeeds.

fn main() {
    let dir =
        std::env::temp_dir().join(format!("linsync-skip-disable-check-{}", std::process::id()));
    if std::fs::create_dir_all(&dir).is_err() {
        std::process::exit(2);
    }
    let file = dir.join("data.txt");
    if std::fs::write(&file, b"unsandboxed").is_err() {
        let _ = std::fs::remove_dir_all(&dir);
        std::process::exit(2);
    }

    let policy = linsync_sandbox::SandboxPolicy::builder().build();

    let mut cmd = std::process::Command::new("cat");
    cmd.arg(&file).stdout(std::process::Stdio::null());

    let exit_code = match linsync_sandbox::SandboxedCommand::new(cmd, policy).spawn() {
        Ok(mut child) => match child.wait() {
            Ok(status) if status.success() => 0,
            _ => 1,
        },
        Err(_) => 1,
    };

    let _ = std::fs::remove_dir_all(&dir);
    std::process::exit(exit_code);
}
