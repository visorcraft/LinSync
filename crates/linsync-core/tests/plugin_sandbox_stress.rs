//! Sandbox stress tests for plugin-helper execution.
//!
//! These tests stress the four canonical escape paths a malicious or
//! buggy plugin might try:
//!
//! 1. **Symlink escape** — read a file outside the policy via a symlink
//!    that resolves out-of-tree.
//! 2. **Fork bomb** — spawn many children to exhaust process budget.
//! 3. **Oversize stdout** — emit more bytes than the limit allows.
//! 4. **Timeout escape** — sleep past the helper timeout.
//!
//! When the sandbox is in degraded mode (LINSYNC_SANDBOX_SKIP=1 or
//! kernel without Landlock and without bubblewrap), tests that depend
//! on enforcement are skipped rather than failed — the host has done
//! everything it can; the kernel/runtime can't enforce the policy.

use linsync_core::plugin::{
    PluginClass, PluginExecutionOptions, PluginManifest, PluginSandbox, run_plugin_helper,
};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

fn write_script(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

fn manifest_for(entry: &str) -> PluginManifest {
    PluginManifest {
        schema_version: 1,
        id: "test.sandbox-stress".into(),
        name: "Sandbox stress".into(),
        version: "1.0".into(),
        license: "MIT".into(),
        entry: vec![entry.into()],
        classes: vec![PluginClass::Unpacker],
        mime_types: vec![],
        extensions: vec![],
        capabilities: vec![],
        deterministic: true,
        sandbox: PluginSandbox {
            network: false,
            writes_input: false,
            requires_home_access: false,
        },
        streaming: false,
        options_schema: vec![],
        normalization_categories: vec![],
    }
}

fn sandbox_enforces() -> bool {
    use linsync_sandbox::SandboxStrategy;
    !matches!(SandboxStrategy::detect(), SandboxStrategy::Degraded)
}

#[test]
fn oversize_stdout_is_capped() {
    let tmp = TempDir::new().unwrap();
    let helper = tmp.path().join("helper.sh");
    // Emit 2 MiB to stdout (small enough that the OS process quota isn't
    // a factor under parallel test load) — well past the 256 KiB cap.
    write_script(&helper, "#!/bin/sh\nread REQ\nyes A | head -c 2097152\n");

    let manifest = manifest_for("./helper.sh");
    let opts = PluginExecutionOptions {
        timeout: Duration::from_secs(5),
        max_total_bytes: 256 * 1024,
        ..Default::default()
    };

    let req = serde_json::json!({"op":"probe","source":tmp.path().to_str().unwrap()});
    let result = run_plugin_helper(tmp.path(), &manifest, &req.to_string(), &opts);
    // The cap MUST be enforced — either via truncation or an error. The
    // child must never deliver the full 2 MiB to the host result.
    match result {
        Ok(r) => assert!(
            r.stdout.len() as u64 <= 512 * 1024,
            "stdout {} bytes exceeded 512 KiB safety bound (cap was 256 KiB)",
            r.stdout.len()
        ),
        Err(_) => { /* Any error path counts as enforcement. */ }
    }
}

#[test]
fn timeout_escape_is_killed() {
    let tmp = TempDir::new().unwrap();
    let helper = tmp.path().join("helper.sh");
    // Sleep far longer than the timeout; the host must kill the child.
    write_script(
        &helper,
        "#!/bin/sh\nread REQ\nsleep 30\necho '{\"ok\":true}'\n",
    );

    let manifest = manifest_for("./helper.sh");
    let opts = PluginExecutionOptions {
        timeout: Duration::from_millis(500),
        ..Default::default()
    };

    let start = std::time::Instant::now();
    let req = serde_json::json!({"op":"probe","source":tmp.path().to_str().unwrap()});
    let result = run_plugin_helper(tmp.path(), &manifest, &req.to_string(), &opts);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "helper ran {elapsed:?} despite 500ms timeout"
    );
    assert!(result.is_err(), "expected timeout error, got {result:?}");
}

#[test]
fn symlink_escape_does_not_read_target_when_sandboxed() {
    if !sandbox_enforces() {
        eprintln!("SKIP: sandbox in degraded mode — kernel cannot enforce filesystem policy");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let helper = tmp.path().join("helper.sh");
    let secret = tmp.path().join("..").join("linsync-stress-secret.txt");
    let secret_canon = secret.canonicalize().unwrap_or_else(|_| {
        // Create the secret if it doesn't already exist for this test run.
        std::fs::write(&secret, b"top-secret").unwrap();
        secret.canonicalize().unwrap()
    });
    if !secret_canon.exists() {
        std::fs::write(&secret_canon, b"top-secret").unwrap();
    }

    // Helper tries to symlink to the secret then read through it.
    // If the sandbox confines the helper to tmp_path / source / temp_dir,
    // the read should fail or the symlink itself shouldn't resolve.
    write_script(
        &helper,
        &format!(
            "#!/bin/sh\nread REQ\nln -sf {secret} ./link 2>/dev/null\nif cat ./link 2>/dev/null | grep -q top-secret; then\n  echo '{{\"ok\":false,\"error\":\"symlink escape succeeded\"}}'\nelse\n  echo '{{\"ok\":true}}'\nfi\n",
            secret = secret_canon.display()
        ),
    );

    let manifest = manifest_for("./helper.sh");
    let opts = PluginExecutionOptions {
        timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let req = serde_json::json!({"op":"probe","source":tmp.path().to_str().unwrap()});
    let result = run_plugin_helper(tmp.path(), &manifest, &req.to_string(), &opts);
    // Cleanup the secret regardless of result.
    let _ = std::fs::remove_file(&secret_canon);

    match result {
        Ok(r) => assert!(
            !r.stdout.contains("symlink escape succeeded"),
            "sandbox failed to block symlink escape: {}",
            r.stdout
        ),
        Err(_) => { /* sandbox refused the read entirely; that's also pass */ }
    }
}

#[test]
fn fork_bomb_does_not_hang_the_test_runner() {
    let tmp = TempDir::new().unwrap();
    let helper = tmp.path().join("helper.sh");
    // A tame fork bomb: 50 background sleeps, then exit. If the helper
    // process tree isn't killed cleanly, the test runner can hang.
    write_script(
        &helper,
        "#!/bin/sh\nread REQ\ni=0\nwhile [ $i -lt 50 ]; do sleep 5 & i=$((i+1)); done\necho '{\"ok\":true}'\n",
    );

    let manifest = manifest_for("./helper.sh");
    let opts = PluginExecutionOptions {
        timeout: Duration::from_secs(2),
        ..Default::default()
    };

    let start = std::time::Instant::now();
    let req = serde_json::json!({"op":"probe","source":tmp.path().to_str().unwrap()});
    let _ = run_plugin_helper(tmp.path(), &manifest, &req.to_string(), &opts);
    let elapsed = start.elapsed();

    // The shell exits quickly after spawning the background sleeps; the
    // helper invocation should return within a few seconds regardless of
    // whether the backgrounded sleeps are still alive.
    assert!(
        elapsed < Duration::from_secs(15),
        "fork-bomb helper hung the runner for {elapsed:?}"
    );
}
