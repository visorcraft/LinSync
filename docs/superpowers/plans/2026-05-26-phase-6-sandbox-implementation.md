# Phase 6 — Sandbox Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wrap every plugin-helper process in a Landlock + seccomp-bpf sandbox, with bubblewrap fallback. New `crates/linsync-sandbox` crate isolates the sandbox API. `linsync-core::plugin::run_plugin_helper` calls into the new crate. Existing plugin tests pass.

**Architecture:** Per `docs/sandbox-design.md`. The sandbox setup lives in a dedicated `crates/linsync-sandbox` crate to keep platform C-FFI out of every build of `linsync-core`. `linsync-core` gains an optional `sandbox` Cargo feature (on by default) that adds `linsync-sandbox` as a dependency. Inside `run_plugin_helper_with_temp`, a `SandboxedCommand` builder wraps the existing `std::process::Command` — this avoids the `pre_exec` / bubblewrap conflict (open issue 2 from the design doc). `apply_policy` installs Landlock + seccomp-bpf inside the child via `pre_exec` on the Landlock path; the bubblewrap path replaces the command and all args instead. Both paths converge to a single `SandboxedCommand::spawn()` call in `run_plugin_helper_with_temp`.

**Tech Stack:** Rust 2024 edition, `landlock` 0.4 (Apache-2.0), `seccompiler` 0.4 (Apache-2.0), `bubblewrap` (system binary, runtime dep only), GPL-3.0-only.

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `crates/linsync-sandbox/Cargo.toml` | Create | Crate manifest, `landlock`/`seccompiler`/`libc` deps |
| `crates/linsync-sandbox/src/lib.rs` | Create | Public API: `SandboxPolicy`, `SandboxError`, `SandboxedCommand`, `SandboxStrategy` |
| `crates/linsync-sandbox/src/landlock.rs` | Create | Landlock ABI detection + filesystem rule installation |
| `crates/linsync-sandbox/src/seccomp.rs` | Create | seccomp-bpf filter (network block + hardened syscall set) |
| `crates/linsync-sandbox/src/bubblewrap.rs` | Create | `bwrap` detection + command rewriting |
| `crates/linsync-sandbox/src/policy.rs` | Create | `SandboxPolicy` builder, `policy_for_plugin` constructor |
| `crates/linsync-sandbox/tests/sandbox_integration.rs` | Create | Landlock/seccomp/bwrap integration tests (gated on Linux + availability) |
| `crates/linsync-core/Cargo.toml` | Modify | Add `sandbox` feature flag + `linsync-sandbox` optional dep |
| `crates/linsync-core/src/plugin.rs` | Modify | Replace `Command` + `spawn_plugin_helper` with `SandboxedCommand` |
| `crates/linsync-core/tests/plugin_sandbox_stress.rs` | Create | Stress tests: symlink escape, fork bomb, network block, etc. |
| `Cargo.toml` | Modify | Add `linsync-sandbox` to workspace members; add `landlock`/`seccompiler` workspace deps |
| `deny.toml` | Modify | Verify Apache-2.0 already allowed; add any newly-introduced transitive licenses |
| `packaging/flatpak/com.visorcraft.LinSync.yml` | Modify | Add comment block documenting intra-sandbox Landlock enforcement |
| `docs/plugin-protocol.md` | Modify | Add "Sandboxing" section documenting the `sandbox` manifest block |
| `scripts/gui-smoke.sh` | Modify | Export `LINSYNC_SANDBOX_SKIP=1` before smoke runs |

---

## Task 6.1 — Create `crates/linsync-sandbox` crate skeleton

**Files:**
- Create: `crates/linsync-sandbox/Cargo.toml`
- Create: `crates/linsync-sandbox/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create `crates/linsync-sandbox/Cargo.toml`**

```toml
[package]
name = "linsync-sandbox"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
libc.workspace = true
tracing.workspace = true

[target.'cfg(target_os = "linux")'.dependencies]
landlock = "0.4"
seccompiler = "0.4"

[dev-dependencies]
tempfile = "3"
scopeguard = "1"
```

- [ ] **Step 2: Create `crates/linsync-sandbox/src/lib.rs`**

```rust
//! Plugin-helper sandbox enforcement.
//!
//! Call [`SandboxedCommand::spawn`] in place of [`std::process::Command::spawn`]
//! to apply a [`SandboxPolicy`] before the child executes.
//!
//! On Linux kernels >= 5.13 the primary path uses Landlock + seccomp-bpf.
//! On older kernels it falls back to `bwrap` (bubblewrap).
//! If neither is available the process enters degraded mode and logs a warning.

pub mod policy;

#[cfg(target_os = "linux")]
pub(crate) mod landlock;
#[cfg(target_os = "linux")]
pub(crate) mod seccomp;
#[cfg(target_os = "linux")]
pub(crate) mod bubblewrap;

pub use policy::{SandboxPolicy, PluginSandboxFields, policy_for_plugin};

use std::process::{Command, Child};

/// Error type returned when sandbox setup fails.
#[derive(Debug)]
pub enum SandboxError {
    /// Landlock ABI < 1 on this kernel; bubblewrap fallback should be tried.
    LandlockUnsupported,
    /// Neither Landlock nor bubblewrap is available; caller should degrade.
    NoSandboxAvailable,
    /// Low-level OS error during policy installation.
    Os(std::io::Error),
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LandlockUnsupported => write!(f, "Landlock not supported on this kernel"),
            Self::NoSandboxAvailable => write!(f, "no sandbox available (Landlock < ABI 1 and bwrap not found)"),
            Self::Os(e) => write!(f, "sandbox OS error: {e}"),
        }
    }
}

impl std::error::Error for SandboxError {}

impl From<std::io::Error> for SandboxError {
    fn from(e: std::io::Error) -> Self {
        Self::Os(e)
    }
}

/// Which sandbox backend will be used for a given execution environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxStrategy {
    /// Landlock + seccomp-bpf (kernel >= 5.13, ABI >= 1).
    Landlock,
    /// bubblewrap binary fallback (kernel < 5.13).
    Bubblewrap(std::path::PathBuf),
    /// Neither backend available; execution proceeds unsandboxed with a WARN.
    Degraded,
}

impl SandboxStrategy {
    /// Detect the best available sandbox strategy at runtime.
    pub fn detect() -> Self {
        if std::env::var_os("LINSYNC_SANDBOX_SKIP").is_some() {
            return Self::Degraded;
        }
        #[cfg(target_os = "linux")]
        {
            if crate::landlock::landlock_abi_version() >= 1 {
                return Self::Landlock;
            }
            if let Some(bwrap) = crate::bubblewrap::find_bwrap() {
                return Self::Bubblewrap(bwrap);
            }
        }
        Self::Degraded
    }
}

/// Wraps a [`Command`] with sandbox policy, then spawns it.
///
/// On the `Landlock` path the policy is installed inside the child via
/// `pre_exec`. On the `Bubblewrap` path the command is rewritten to run
/// through `bwrap`. On `Degraded` the command is spawned unchanged after a
/// `tracing::warn!` call.
pub struct SandboxedCommand {
    inner: Command,
    policy: SandboxPolicy,
    strategy: SandboxStrategy,
}

impl SandboxedCommand {
    pub fn new(command: Command, policy: SandboxPolicy) -> Self {
        let strategy = SandboxStrategy::detect();
        Self { inner: command, policy, strategy }
    }

    /// Spawn the child process, applying the sandbox according to the
    /// detected strategy.
    pub fn spawn(self) -> Result<Child, SandboxError> {
        match self.strategy {
            SandboxStrategy::Degraded => {
                tracing::warn!(
                    "sandbox unavailable (LINSYNC_SANDBOX_SKIP or no Landlock/bwrap): \
                     plugin helper running unsandboxed"
                );
                self.inner.spawn().map_err(SandboxError::Os)
            }
            #[cfg(target_os = "linux")]
            SandboxStrategy::Landlock => {
                crate::landlock::spawn_with_landlock(self.inner, self.policy)
            }
            #[cfg(target_os = "linux")]
            SandboxStrategy::Bubblewrap(bwrap) => {
                crate::bubblewrap::spawn_with_bwrap(self.inner, self.policy, &bwrap)
            }
            #[allow(unreachable_patterns)]
            _ => {
                tracing::warn!("sandbox not supported on this platform; running unsandboxed");
                self.inner.spawn().map_err(SandboxError::Os)
            }
        }
    }
}
```

- [ ] **Step 3: Wire into workspace `Cargo.toml`**

Open `/work/repos/visorcraft/linsync/Cargo.toml`. Add `"crates/linsync-sandbox"` to the `members` array and add workspace deps for the new crates:

```toml
[workspace]
members = [
    "apps/linsync-gui",
    "crates/linsync-cli",
    "crates/linsync-core",
    "crates/linsync-sandbox",
]
resolver = "3"

# existing [workspace.package] block is unchanged

[workspace.dependencies]
# existing deps are unchanged; add these three lines:
landlock = "0.4"
linsync-sandbox = { path = "crates/linsync-sandbox" }
seccompiler = "0.4"
```

- [ ] **Step 4: Stub the platform-specific modules so the crate compiles**

The `lib.rs` references `landlock`, `seccomp`, and `bubblewrap` modules under `#[cfg(target_os = "linux")]`. Create empty stubs so `cargo build` passes even before those modules are implemented in later tasks. Create these three files:

`crates/linsync-sandbox/src/landlock.rs` — one-line stub:

```rust
// Implemented in Task 6.3.
pub(crate) fn landlock_abi_version() -> u32 { 0 }
pub(crate) fn spawn_with_landlock(_cmd: std::process::Command, _policy: crate::SandboxPolicy) -> Result<std::process::Child, crate::SandboxError> { Err(crate::SandboxError::LandlockUnsupported) }
```

`crates/linsync-sandbox/src/seccomp.rs` — one-line stub:

```rust
// Implemented in Task 6.4.
pub(crate) fn install_seccomp_filter(_allow_network: bool) -> std::io::Result<()> { Ok(()) }
pub fn build_seccomp_filter(_allow_network: bool) -> Result<seccompiler::BpfProgram, seccompiler::Error> { seccompiler::SeccompFilter::new(Default::default(), seccompiler::SeccompAction::Allow, seccompiler::SeccompAction::Allow, std::env::consts::ARCH.try_into().unwrap())?.try_into() }
pub fn apply_seccomp_filter(_program: &seccompiler::BpfProgram) -> std::io::Result<()> { Ok(()) }
```

`crates/linsync-sandbox/src/bubblewrap.rs` — one-line stub:

```rust
// Implemented in Task 6.5.
pub(crate) fn find_bwrap() -> Option<std::path::PathBuf> { None }
pub(crate) fn spawn_with_bwrap(_cmd: std::process::Command, _policy: crate::SandboxPolicy, _bwrap: &std::path::Path) -> Result<std::process::Child, crate::SandboxError> { Err(crate::SandboxError::NoSandboxAvailable) }
```

`crates/linsync-sandbox/src/policy.rs` — minimal stub (will be replaced in Task 6.2):

```rust
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    pub read_paths: Vec<PathBuf>,
    pub write_paths: Vec<PathBuf>,
    pub network: bool,
    pub fd_limit: u64,
    pub proc_limit: u64,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self { read_paths: vec![], write_paths: vec![], network: false, fd_limit: 256, proc_limit: 16 }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PluginSandboxFields {
    pub network: bool,
}

pub fn policy_for_plugin(_f: &PluginSandboxFields, _plugin_dir: &Path, _source: &Path, _temp: &Path) -> SandboxPolicy {
    SandboxPolicy::default()
}
```

- [ ] **Step 5: Run the build, expect it to compile**

```bash
cargo build -p linsync-sandbox
```

Expected: zero errors.

- [ ] **Step 6: Commit**

```bash
git add crates/linsync-sandbox/ Cargo.toml Cargo.lock
git commit -m "feat(sandbox): add linsync-sandbox crate skeleton"
```

---

## Task 6.2 — `SandboxPolicy` builder + `policy_for_plugin`

**Files:**
- Modify: `crates/linsync-sandbox/src/policy.rs` (replace the Task 6.1 stub)

- [ ] **Step 1: Write the failing tests**

Replace the entire contents of `crates/linsync-sandbox/src/policy.rs` with:

```rust
use std::path::{Path, PathBuf};

/// Per-invocation sandbox policy derived from a plugin manifest + caller paths.
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    /// Paths the helper may read (directories are read recursively).
    pub read_paths: Vec<PathBuf>,
    /// Paths the helper may read and write (its temp dir).
    pub write_paths: Vec<PathBuf>,
    /// If false (default), `socket(AF_INET/AF_INET6/AF_NETLINK, ...)` is blocked
    /// via seccomp-bpf; AF_UNIX is always allowed.
    pub network: bool,
    /// Maximum number of open file descriptors.  Default 256.
    pub fd_limit: u64,
    /// Maximum number of child processes.  Default 16.
    pub proc_limit: u64,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            read_paths: Vec::new(),
            write_paths: Vec::new(),
            network: false,
            fd_limit: 256,
            proc_limit: 16,
        }
    }
}

impl SandboxPolicy {
    pub fn builder() -> SandboxPolicyBuilder {
        SandboxPolicyBuilder::default()
    }
}

/// Fluent builder for [`SandboxPolicy`].
#[derive(Debug, Default)]
pub struct SandboxPolicyBuilder {
    read_paths: Vec<PathBuf>,
    write_paths: Vec<PathBuf>,
    network: bool,
    fd_limit: Option<u64>,
    proc_limit: Option<u64>,
}

impl SandboxPolicyBuilder {
    pub fn read(mut self, path: impl AsRef<Path>) -> Self {
        self.read_paths.push(path.as_ref().to_path_buf());
        self
    }

    pub fn write(mut self, path: impl AsRef<Path>) -> Self {
        self.write_paths.push(path.as_ref().to_path_buf());
        self
    }

    pub fn network(mut self, allow: bool) -> Self {
        self.network = allow;
        self
    }

    pub fn fd_limit(mut self, n: u64) -> Self {
        self.fd_limit = Some(n);
        self
    }

    pub fn proc_limit(mut self, n: u64) -> Self {
        self.proc_limit = Some(n);
        self
    }

    pub fn build(self) -> SandboxPolicy {
        SandboxPolicy {
            read_paths: self.read_paths,
            write_paths: self.write_paths,
            network: self.network,
            fd_limit: self.fd_limit.unwrap_or(256),
            proc_limit: self.proc_limit.unwrap_or(16),
        }
    }
}

/// Mirror of `linsync_core::plugin::PluginSandbox` fields needed by the sandbox
/// crate.  A plain struct avoids a circular dependency on linsync-core.
#[derive(Debug, Clone, Default)]
pub struct PluginSandboxFields {
    pub network: bool,
}

/// Construct a [`SandboxPolicy`] from the fields that `run_plugin_helper_with_temp`
/// has at the point of spawn.
///
/// - `plugin_dir` is added to `read_paths` (plugin binary + support files).
/// - `source_path` is added to `read_paths` (the file being processed).
/// - `temp_dir` is added to `write_paths` (plugin output, intermediate state).
/// - `sandbox.network` controls the seccomp socket block.
pub fn policy_for_plugin(
    plugin_sandbox: &PluginSandboxFields,
    plugin_dir: &Path,
    source_path: &Path,
    temp_dir: &Path,
) -> SandboxPolicy {
    SandboxPolicy::builder()
        .read(plugin_dir)
        .read(source_path)
        .write(temp_dir)
        .network(plugin_sandbox.network)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn builder_defaults_deny_network() {
        let policy = SandboxPolicy::builder().build();
        assert!(!policy.network, "network must be denied by default");
    }

    #[test]
    fn builder_accumulates_paths() {
        let policy = SandboxPolicy::builder()
            .read("/tmp/plugin")
            .read("/tmp/source.zip")
            .write("/tmp/work")
            .build();
        assert_eq!(policy.read_paths.len(), 2);
        assert_eq!(policy.write_paths.len(), 1);
    }

    #[test]
    fn builder_sets_network() {
        let policy = SandboxPolicy::builder().network(true).build();
        assert!(policy.network);
    }

    #[test]
    fn builder_sets_limits() {
        let policy = SandboxPolicy::builder().fd_limit(64).proc_limit(4).build();
        assert_eq!(policy.fd_limit, 64);
        assert_eq!(policy.proc_limit, 4);
    }

    #[test]
    fn policy_for_plugin_maps_paths_correctly() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("plugin");
        let source = tmp.path().join("source.zip");
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(&source, b"data").unwrap();

        let fields = PluginSandboxFields { network: false };
        let policy = policy_for_plugin(&fields, &plugin_dir, &source, &work);

        assert!(policy.read_paths.contains(&plugin_dir));
        assert!(policy.read_paths.contains(&source));
        assert!(policy.write_paths.contains(&work));
        assert!(!policy.network);
    }

    #[test]
    fn policy_for_plugin_allows_network_when_declared() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("p");
        let source = tmp.path().join("s");
        let work = tmp.path().join("w");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(&source, b"x").unwrap();

        let fields = PluginSandboxFields { network: true };
        let policy = policy_for_plugin(&fields, &plugin_dir, &source, &work);
        assert!(policy.network);
    }
}
```

- [ ] **Step 2: Run tests, expect FAIL**

```bash
cargo test -p linsync-sandbox -- policy::tests
```

Expected: FAIL — `tempfile` not yet a dev-dep (if you skipped adding it in Task 6.1 Cargo.toml, add it now).

- [ ] **Step 3: Run tests after ensuring dev-deps are present, expect PASS**

```bash
cargo test -p linsync-sandbox -- policy::tests
```

Expected: all 5 tests PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/linsync-sandbox/src/policy.rs crates/linsync-sandbox/Cargo.toml
git commit -m "feat(sandbox): SandboxPolicy builder + policy_for_plugin constructor"
```

---

## Task 6.3 — Landlock backend

**Files:**
- Modify: `crates/linsync-sandbox/src/landlock.rs` (replace the Task 6.1 stub)
- Create: `crates/linsync-sandbox/tests/sandbox_integration.rs`

- [ ] **Step 1: Write the failing integration tests**

Create `crates/linsync-sandbox/tests/sandbox_integration.rs`:

```rust
//! Integration tests for the sandbox backends.
//!
//! Gated on Linux + Landlock ABI >= 1 being available + LINSYNC_SANDBOX_SKIP not set.
//! CI runs these in a separate `sandbox-stress` job on Ubuntu 24.04 (kernel 6.8).
//! The standard CI job sets LINSYNC_SANDBOX_SKIP=1.

#[cfg(target_os = "linux")]
mod landlock_tests {
    use linsync_sandbox::{SandboxPolicy, SandboxedCommand, SandboxStrategy};
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
        if skip_if_unavailable() { return; }

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
        if skip_if_unavailable() { return; }

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
        assert!(!status.success(), "cat of forbidden file should fail (EACCES via Landlock)");
    }
}
```

- [ ] **Step 2: Run test, expect FAIL**

```bash
cargo test -p linsync-sandbox --test sandbox_integration -- landlock_tests
```

Expected: FAIL — `landlock_abi_version` in the stub always returns 0, so `SandboxStrategy::detect()` returns `Degraded` instead of `Landlock`, and `spawn` does not apply Landlock restrictions. The `cat_denied_outside_allowed_path` test will pass (cat succeeds because no sandbox is applied) but the logic is wrong — the test will actually pass for the wrong reason, so confirm the stub returns Degraded and the test skips.

- [ ] **Step 3: Implement `crates/linsync-sandbox/src/landlock.rs`**

Replace the entire stub file with:

```rust
//! Landlock filesystem-restriction backend.
//!
//! Called from the child side of fork() via pre_exec. All operations here
//! must be async-signal-safe: no heap allocation, no locking, no stdio.
//! The `landlock` crate uses only raw syscalls and meets this requirement.

use landlock::{
    ABI, Access, AccessFs, Compatible, PathBeneath, PathFd,
    Ruleset, RulesetAttr, RulesetCreatedAttr,
};
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

/// Spawn `cmd` with Landlock filesystem restrictions applied in the child
/// before exec. seccomp-bpf is also installed in the child.
pub(crate) fn spawn_with_landlock(
    mut cmd: Command,
    policy: SandboxPolicy,
) -> Result<Child, SandboxError> {
    // Compile the seccomp filter in the parent (no allocation after fork).
    let seccomp_program = crate::seccomp::build_seccomp_filter(policy.network)
        .map_err(|e| SandboxError::Os(std::io::Error::new(std::io::ErrorKind::Other, format!("{e:?}"))))?;

    // Clone data needed in the pre_exec closure.
    let read_paths: Vec<PathBuf> = policy.read_paths.clone();
    let write_paths: Vec<PathBuf> = policy.write_paths.clone();
    let fd_limit = policy.fd_limit;
    let proc_limit = policy.proc_limit;

    // SAFETY: pre_exec runs after fork. We use only async-signal-safe calls:
    // setrlimit(2), raw Landlock syscalls (via the landlock crate), and
    // prctl/seccomp (via seccompiler). No heap allocation or locking.
    unsafe {
        cmd.pre_exec(move || {
            // 1. Resource limits (fork-bomb + fd-leak prevention).
            set_rlimit(libc::RLIMIT_NOFILE, fd_limit)?;
            set_rlimit(libc::RLIMIT_NPROC, proc_limit)?;

            // 2. Landlock filesystem restrictions.
            apply_landlock_policy(&read_paths, &write_paths)?;

            // 3. seccomp-bpf (network block + privilege-escalation block).
            crate::seccomp::apply_seccomp_filter(&seccomp_program)?;

            Ok(())
        });
    }

    cmd.spawn().map_err(SandboxError::Os)
}

fn set_rlimit(resource: libc::c_int, value: u64) -> std::io::Result<()> {
    let lim = libc::rlimit { rlim_cur: value, rlim_max: value };
    let rc = unsafe { libc::setrlimit(resource as _, &lim) };
    if rc == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

fn apply_landlock_policy(
    read_paths: &[PathBuf],
    write_paths: &[PathBuf],
) -> std::io::Result<()> {
    let abi = ABI::new_current();

    // Base read rights (ABI v1, kernel 5.13+).
    let read_rights =
        AccessFs::ReadFile | AccessFs::ReadDir | AccessFs::Execute;

    // Write rights include read rights plus mutation rights.
    let mut write_rights = read_rights
        | AccessFs::WriteFile
        | AccessFs::MakeReg
        | AccessFs::RemoveFile;

    // ABI v3 (kernel 6.2) adds Truncate. Activate when available so plugins
    // that write temp files via truncate(2) work correctly.
    if abi as u32 >= 3 {
        write_rights = write_rights | AccessFs::Truncate;
    }

    let ruleset_result = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .map_err(io_err)?
        .create()
        .map_err(io_err)?;

    let mut ruleset = ruleset_result;

    for path in read_paths {
        let fd = PathFd::new(path).map_err(io_err)?;
        let effective_read = AccessFs::from_read(abi) | AccessFs::Execute;
        let rule = PathBeneath::new(fd, effective_read);
        ruleset = ruleset.add_rule(rule).map_err(io_err)?;
    }

    for path in write_paths {
        let fd = PathFd::new(path).map_err(io_err)?;
        let effective_write = write_rights & AccessFs::from_all(abi);
        let rule = PathBeneath::new(fd, effective_write);
        ruleset = ruleset.add_rule(rule).map_err(io_err)?;
    }

    ruleset.restrict_self().map_err(io_err)?;

    Ok(())
}

fn io_err<E: std::fmt::Debug>(e: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, format!("{e:?}"))
}
```

Note: verify the exact method names against `landlock` 0.4 docs with `cargo doc -p linsync-sandbox --open`. If `AccessFs::from_read` or `from_all` differ in 0.4, use the documented equivalents. The `ABI::new_current()` function is stable across recent versions.

- [ ] **Step 4: Run tests, expect PASS**

```bash
cargo test -p linsync-sandbox --test sandbox_integration -- landlock_tests
```

Expected: both tests PASS on a kernel with Landlock ABI >= 1. On a kernel without Landlock both self-skip.

- [ ] **Step 5: Commit**

```bash
git add crates/linsync-sandbox/src/landlock.rs crates/linsync-sandbox/tests/sandbox_integration.rs
git commit -m "feat(sandbox): Landlock filesystem restriction backend"
```

---

## Task 6.4 — seccomp-bpf filter

**Files:**
- Modify: `crates/linsync-sandbox/src/seccomp.rs` (replace Task 6.1 stub)
- Modify: `crates/linsync-sandbox/tests/sandbox_integration.rs` (add seccomp_tests module)

- [ ] **Step 1: Add network-blocking tests to `sandbox_integration.rs`**

Append this module to the end of `crates/linsync-sandbox/tests/sandbox_integration.rs`:

```rust
#[cfg(target_os = "linux")]
mod seccomp_tests {
    use linsync_sandbox::{SandboxPolicy, SandboxedCommand, SandboxStrategy};
    use std::process::{Command, Stdio};
    use tempfile::TempDir;

    fn skip_if_unavailable() -> bool {
        if std::env::var_os("LINSYNC_SANDBOX_SKIP").is_some() {
            eprintln!("SKIP: LINSYNC_SANDBOX_SKIP is set");
            return true;
        }
        matches!(SandboxStrategy::detect(), linsync_sandbox::SandboxStrategy::Degraded) && {
            eprintln!("SKIP: no sandbox backend available");
            true
        }
    }

    /// Sandboxed process cannot open an AF_INET socket when network=false.
    /// python3 attempts the socket; seccomp returns EACCES; script exits 42.
    #[test]
    fn network_socket_denied_by_default() {
        if skip_if_unavailable() { return; }

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
            status.code(), Some(42),
            "expected exit 42 (EACCES from seccomp); got {:?}", status.code()
        );
    }

    /// When network=true the same socket call succeeds.
    #[test]
    fn network_socket_allowed_when_declared() {
        if skip_if_unavailable() { return; }

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
            status.code(), Some(0),
            "socket should succeed when network=true; got {:?}", status.code()
        );
    }
}
```

- [ ] **Step 2: Run tests, expect FAIL**

```bash
cargo test -p linsync-sandbox --test sandbox_integration -- seccomp_tests
```

Expected: FAIL — seccomp stub does not actually install any filter, so the socket call succeeds and exits 0 instead of 42.

- [ ] **Step 3: Implement `crates/linsync-sandbox/src/seccomp.rs`**

Replace the entire stub file:

```rust
//! seccomp-bpf filter installation.
//!
//! Applied in the child after fork() via pre_exec. Uses only async-signal-safe
//! operations: prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, ...) via seccompiler.
//!
//! The filter is compiled in the parent (build_seccomp_filter) and applied in
//! the child (apply_seccomp_filter) to avoid any allocation after fork.

use seccompiler::{
    BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp,
    SeccompCondition, SeccompFilter, SeccompRule,
};
use std::collections::BTreeMap;

/// Compile the seccomp BPF program. Call this in the parent before fork.
///
/// When `allow_network` is false, socket(AF_INET/AF_INET6/AF_NETLINK, ...) returns
/// EACCES. AF_UNIX is always permitted. setuid, setgid, ptrace, and kernel module
/// syscalls are blocked unconditionally.
pub fn build_seccomp_filter(allow_network: bool) -> Result<BpfProgram, seccompiler::Error> {
    // Rules map: syscall_nr -> list of conditions that MATCH the block action.
    // A rule matches if all its conditions match (AND within a rule).
    // Multiple rules for the same syscall are OR'd together.
    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();

    // Unconditionally blocked syscalls (empty condition list = always match).
    let always_block: &[i64] = &[
        libc::SYS_setuid,
        libc::SYS_setgid,
        libc::SYS_ptrace,
        libc::SYS_init_module,
        libc::SYS_finit_module,
        libc::SYS_delete_module,
    ];
    for &nr in always_block {
        rules.insert(nr, vec![SeccompRule::new(vec![]).unwrap()]);
    }

    // Block network socket families when allow_network is false.
    if !allow_network {
        let families: &[u32] = &[
            libc::AF_INET as u32,
            libc::AF_INET6 as u32,
            libc::AF_NETLINK as u32,
        ];
        let socket_rules: Vec<SeccompRule> = families
            .iter()
            .map(|&family| {
                SeccompRule::new(vec![
                    SeccompCondition::new(
                        0,                        // arg index: domain
                        SeccompCmpArgLen::Dword,
                        SeccompCmpOp::Eq,
                        family as u64,
                    )
                    .unwrap(),
                ])
                .unwrap()
            })
            .collect();
        rules.insert(libc::SYS_socket, socket_rules);
    }

    let arch = std::env::consts::ARCH.try_into().map_err(|_| {
        // Return a dummy error; seccompiler::Error is non-exhaustive so we
        // create one via the filter construction path with empty rules.
        seccompiler::SeccompFilter::new(
            BTreeMap::new(),
            SeccompAction::Allow,
            SeccompAction::Allow,
            std::env::consts::ARCH.try_into().unwrap_or(seccompiler::TargetArch::x86_64),
        )
        .unwrap_err()
    })?;

    SeccompFilter::new(
        rules,
        // Action for rules that MATCH (i.e., the blocked syscalls):
        SeccompAction::Errno(libc::EACCES as u32),
        // Default action for everything else (allow):
        SeccompAction::Allow,
        arch,
    )?
    .try_into()
}

/// Apply a pre-compiled BPF program via prctl(PR_SET_SECCOMP, ...).
/// Async-signal-safe — safe to call from pre_exec.
pub fn apply_seccomp_filter(program: &BpfProgram) -> std::io::Result<()> {
    seccompiler::apply_filter(program)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{e:?}")))
}

/// Convenience wrapper: compile and immediately apply. Use only from pre_exec
/// when you cannot pre-compile (e.g. in the bubblewrap path where we pass the
/// BPF bytes via a pipe to bwrap --seccomp).
pub(crate) fn install_seccomp_filter(allow_network: bool) -> std::io::Result<()> {
    let prog = build_seccomp_filter(allow_network)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{e:?}")))?;
    apply_seccomp_filter(&prog)
}
```

Note: `SeccompFilter::new` argument order and `SeccompAction` semantics vary slightly across seccompiler versions. In seccompiler 0.4, `new(rules, mismatch_action, match_action, arch)` — the *second* argument is what happens when NO rule matches (the default), and the *third* is what happens when a rule matches. Double-check against `cargo doc -p linsync-sandbox` and swap arguments if needed. The intent: blocked syscalls return EACCES, everything else is allowed.

- [ ] **Step 4: Run tests, expect PASS**

```bash
cargo test -p linsync-sandbox --test sandbox_integration -- seccomp_tests
cargo test -p linsync-sandbox --test sandbox_integration -- landlock_tests
```

Expected: all PASS (or self-skip on incapable kernels).

- [ ] **Step 5: Commit**

```bash
git add crates/linsync-sandbox/src/seccomp.rs crates/linsync-sandbox/src/landlock.rs crates/linsync-sandbox/tests/sandbox_integration.rs
git commit -m "feat(sandbox): seccomp-bpf network and privilege-escalation filter"
```

---

## Task 6.5 — bubblewrap fallback + `SandboxStrategy::detect`

**Files:**
- Modify: `crates/linsync-sandbox/src/bubblewrap.rs` (replace Task 6.1 stub)
- Modify: `crates/linsync-sandbox/tests/sandbox_integration.rs` (add strategy_tests module)

- [ ] **Step 1: Add strategy detection tests**

Append to `crates/linsync-sandbox/tests/sandbox_integration.rs`:

```rust
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
    /// We test this by spawning a child process that sets the env var and
    /// runs a small Rust binary that calls detect() and exits 0 if Degraded.
    #[test]
    fn skip_env_forces_degraded() {
        // Build the checker binary once (it's in the same crate).
        // We rely on the test binary itself to double as the checker.
        // Pass a sentinel arg that we intercept in main (see below).
        let exe = std::env::current_exe().unwrap();
        let status = std::process::Command::new(&exe)
            .arg("--linsync-sandbox-test-degraded")
            .env("LINSYNC_SANDBOX_SKIP", "1")
            .status()
            .unwrap();
        assert!(status.success(), "expected Degraded when LINSYNC_SANDBOX_SKIP=1; got {:?}", status.code());
    }
}

// Subprocess entrypoint: if the sentinel arg is present, check detection and exit.
// This runs before the test harness because it exits immediately.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
fn maybe_check_degraded() {
    if std::env::args().any(|a| a == "--linsync-sandbox-test-degraded") {
        let ok = matches!(linsync_sandbox::SandboxStrategy::detect(), linsync_sandbox::SandboxStrategy::Degraded);
        std::process::exit(if ok { 0 } else { 1 });
    }
}

// Register the check as a ctor so it fires before test_main.
// Alternatively, if the `ctor` crate is not a dev-dep, use a #[test]-level
// subprocess approach without ctor: spawn `cargo run --bin check_detection`.
// The inline approach below uses the test binary's argv directly.
```

Note: the `maybe_check_degraded` function needs to be called before tests run. The cleanest approach without adding the `ctor` crate is to create a separate binary:

Create `crates/linsync-sandbox/src/bin/check_detection.rs`:

```rust
fn main() {
    let ok = matches!(linsync_sandbox::SandboxStrategy::detect(), linsync_sandbox::SandboxStrategy::Degraded);
    std::process::exit(if ok { 0 } else { 1 });
}
```

Then update the test to call this binary:

```rust
#[test]
fn skip_env_forces_degraded() {
    // Build the binary first.
    std::process::Command::new("cargo")
        .args(["build", "-p", "linsync-sandbox", "--bin", "check_detection", "--quiet"])
        .status()
        .unwrap();

    let exe = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("check_detection");

    let status = std::process::Command::new(&exe)
        .env("LINSYNC_SANDBOX_SKIP", "1")
        .status()
        .unwrap();

    assert!(status.success(), "expected Degraded when LINSYNC_SANDBOX_SKIP=1");
}
```

- [ ] **Step 2: Run tests, expect FAIL**

```bash
cargo test -p linsync-sandbox --test sandbox_integration -- strategy_tests
```

Expected: `detect_is_deterministic` PASS; `skip_env_forces_degraded` FAIL or PASS depending on whether the stub bubblewrap.rs is returning `None` (it is, so detect() returns Degraded on both calls — which may make the test pass for the wrong reason). Focus on getting the bubblewrap implementation correct.

- [ ] **Step 3: Implement `crates/linsync-sandbox/src/bubblewrap.rs`**

Replace the entire stub:

```rust
//! bubblewrap (bwrap) fallback sandbox for kernels < 5.13.
//!
//! Rebuilds the Command to run through bwrap with policy-derived bind mounts
//! and --unshare-net when network=false.  seccomp-bpf is applied via
//! --seccomp <fd> so the privilege-escalation block still fires.

use std::io::Write;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd};
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

    // Propagate the original environment.
    bwrap_cmd.arg("--clearenv");
    for (k, v) in orig_envs {
        bwrap_cmd.arg("--setenv").arg(k).arg(v);
    }

    // Compile seccomp filter and pass via a pipe fd to bwrap --seccomp.
    let seccomp_prog = crate::seccomp::build_seccomp_filter(policy.network)
        .map_err(|e| SandboxError::Os(std::io::Error::new(std::io::ErrorKind::Other, format!("{e:?}"))))?;

    let (read_fd, _write_guard) = pipe_seccomp_filter(&seccomp_prog)?;
    bwrap_cmd.arg("--seccomp").arg(read_fd.as_raw_fd().to_string());

    // Original command after separator.
    bwrap_cmd.arg("--").arg(orig_program).args(orig_args);

    // Keep the read fd open into bwrap (it is inherited by default; bwrap
    // reads and closes it before exec-ing the target).
    let child = bwrap_cmd.spawn().map_err(SandboxError::Os)?;
    // _write_guard (the write end) is dropped here, closing it.
    drop(_write_guard);
    Ok(child)
}

/// Write the BPF program bytes to a pipe; return the (read_fd, write_guard).
/// The write guard must be kept alive until after bwrap forks, then dropped.
fn pipe_seccomp_filter(
    prog: &seccompiler::BpfProgram,
) -> Result<(OwnedFd, std::fs::File), SandboxError> {
    let mut fds = [0i32; 2];
    let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if rc != 0 {
        return Err(SandboxError::Os(std::io::Error::last_os_error()));
    }
    let read_fd = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let mut write_fd = unsafe { std::fs::File::from_raw_fd(fds[1]) };

    // Encode each BPF instruction as 8 bytes: code(2) + jt(1) + jf(1) + k(4) LE.
    for insn in prog.iter() {
        write_fd.write_all(&insn.code.to_le_bytes()).map_err(SandboxError::Os)?;
        write_fd.write_all(&[insn.jt]).map_err(SandboxError::Os)?;
        write_fd.write_all(&[insn.jf]).map_err(SandboxError::Os)?;
        write_fd.write_all(&insn.k.to_le_bytes()).map_err(SandboxError::Os)?;
    }

    Ok((read_fd, write_fd))
}
```

Note: `seccompiler::BpfInstruction` must expose `code`, `jt`, `jf`, `k` fields. In seccompiler 0.4 the `BpfProgram` is `Vec<sock_filter>` from libc, which has those exact fields. Verify with `cargo doc`.

- [ ] **Step 4: Run all sandbox tests**

```bash
cargo test -p linsync-sandbox
```

Expected: all PASS (landlock + seccomp + strategy + policy unit tests).

- [ ] **Step 5: Commit**

```bash
git add crates/linsync-sandbox/src/bubblewrap.rs crates/linsync-sandbox/src/bin/check_detection.rs crates/linsync-sandbox/tests/sandbox_integration.rs
git commit -m "feat(sandbox): bubblewrap fallback backend + SandboxStrategy::detect"
```

---

## Task 6.6 — Degraded mode + `LINSYNC_SANDBOX_SKIP=1`

**Files:**
- Verify: `crates/linsync-sandbox/src/lib.rs` (degraded path already wired in Task 6.1)
- Modify: `crates/linsync-sandbox/tests/sandbox_integration.rs` (add degraded_tests module)

- [ ] **Step 1: Add the degraded-mode test**

Append to `crates/linsync-sandbox/tests/sandbox_integration.rs`:

```rust
#[cfg(target_os = "linux")]
mod degraded_tests {
    use linsync_sandbox::{SandboxPolicy, SandboxedCommand};
    use std::process::{Command, Stdio};
    use tempfile::TempDir;

    /// With LINSYNC_SANDBOX_SKIP=1 the command runs unsandboxed and can
    /// access paths that would normally be blocked.
    #[test]
    fn skip_env_disables_sandbox() {
        std::env::set_var("LINSYNC_SANDBOX_SKIP", "1");
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("data.txt");
        std::fs::write(&file, b"unsandboxed").unwrap();

        let policy = SandboxPolicy::builder().build(); // no allowed paths

        let mut cmd = Command::new("cat");
        cmd.arg(&file).stdout(Stdio::null());

        let mut child = SandboxedCommand::new(cmd, policy).spawn().unwrap();
        let status = child.wait().unwrap();
        std::env::remove_var("LINSYNC_SANDBOX_SKIP");
        assert!(status.success(), "unsandboxed cat should succeed");
    }
}
```

Warning: `set_var` / `remove_var` are unsafe in multi-threaded tests (they affect the process environment for all threads). Run this test in isolation or use `--test-threads=1` when running the full integration test binary:

```bash
cargo test -p linsync-sandbox --test sandbox_integration -- degraded_tests -- --test-threads=1
```

- [ ] **Step 2: Run test, expect PASS**

```bash
RUST_LOG=linsync_sandbox=warn cargo test -p linsync-sandbox --test sandbox_integration -- degraded_tests -- --test-threads=1 --nocapture 2>&1 | grep -E "SKIP|warn|unsandboxed|ok"
```

Expected: test passes; `tracing::warn!` output contains "running unsandboxed".

- [ ] **Step 3: Commit**

```bash
git add crates/linsync-sandbox/tests/sandbox_integration.rs
git commit -m "test(sandbox): pin degraded-mode LINSYNC_SANDBOX_SKIP=1 behaviour"
```

---

## Task 6.7 — Wire `run_plugin_helper_with_temp` to use `SandboxedCommand`

**Files:**
- Modify: `crates/linsync-core/Cargo.toml`
- Modify: `crates/linsync-core/src/plugin.rs`
- Create: `crates/linsync-core/tests/plugin_sandbox_basic.rs`

The integration point is `run_plugin_helper_with_temp` (lines 628-723 in `plugin.rs`). The existing code calls `spawn_plugin_helper(&mut command)` at line 651. We replace that call with `SandboxedCommand::new(command, policy).spawn()`.

- [ ] **Step 1: Write the pre-change baseline test**

Create `crates/linsync-core/tests/plugin_sandbox_basic.rs`:

```rust
//! Basic end-to-end test: run_plugin_helper works with the sandbox enabled.

use linsync_core::plugin::{
    PluginClass, PluginExecutionOptions, PluginManifest, PluginSandbox, run_plugin_helper,
};
use tempfile::TempDir;

fn make_manifest(dir: &std::path::Path) -> PluginManifest {
    let path = dir.join("helper.sh");
    std::fs::write(&path, b"#!/bin/sh\nread REQ\necho '{\"ok\":true}'").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    PluginManifest {
        schema_version: 1,
        id: "test.basic".into(),
        name: "Test".into(),
        version: "1.0".into(),
        license: "MIT".into(),
        entry: vec!["./helper.sh".into()],
        classes: vec![PluginClass::Unpacker],
        mime_types: vec![],
        extensions: vec![],
        capabilities: vec![],
        deterministic: true,
        sandbox: PluginSandbox { network: false, writes_input: false, requires_home_access: false },
        streaming: false,
        options_schema: vec![],
    }
}

#[test]
fn sandboxed_helper_runs_successfully() {
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.txt");
    std::fs::write(&source, b"content").unwrap();

    let req = serde_json::json!({
        "op": "probe",
        "source": source.to_str().unwrap(),
    });

    let manifest = make_manifest(tmp.path());
    let opts = PluginExecutionOptions {
        timeout: std::time::Duration::from_secs(5),
        ..Default::default()
    };

    let result = run_plugin_helper(tmp.path(), &manifest, &req.to_string(), &opts);
    assert!(result.is_ok(), "sandboxed helper should succeed; err={:?}", result.err());
    assert!(result.unwrap().stdout.contains("\"ok\":true"));
}
```

- [ ] **Step 2: Run with skip, expect PASS (pre-change baseline)**

```bash
LINSYNC_SANDBOX_SKIP=1 cargo test -p linsync-core --test plugin_sandbox_basic -- --nocapture
```

Expected: PASS (no sandbox, straightforward spawn).

- [ ] **Step 3: Add `sandbox` feature to `crates/linsync-core/Cargo.toml`**

Open `crates/linsync-core/Cargo.toml`. The current `[dependencies]` block has no `linsync-sandbox`. Add:

```toml
[features]
default = ["sandbox"]
sandbox = ["dep:linsync-sandbox"]

[dependencies]
# existing deps unchanged
blake3.workspace = true
libc.workspace = true
regex.workspace = true
serde.workspace = true
serde_json.workspace = true
serde_repr.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
# new optional dep:
linsync-sandbox = { workspace = true, optional = true }
```

- [ ] **Step 4: Wire `SandboxedCommand` into `run_plugin_helper_with_temp` in `plugin.rs`**

In `crates/linsync-core/src/plugin.rs`, locate line 651:

```rust
let mut child = spawn_plugin_helper(&mut command)?;
```

Replace it with:

```rust
#[cfg(feature = "sandbox")]
let mut child = {
    use linsync_sandbox::{SandboxedCommand, policy_for_plugin, PluginSandboxFields};

    // Extract the source path from the request JSON when present.
    // Falls back to plugin_dir so the helper can at least read its own binary.
    let source_path: std::path::PathBuf =
        serde_json::from_str::<serde_json::Value>(request_json)
            .ok()
            .and_then(|v| {
                v.get("source")
                    .and_then(|s| s.as_str())
                    .map(std::path::PathBuf::from)
            })
            .unwrap_or_else(|| plugin_dir.to_path_buf());

    let sandbox_fields = PluginSandboxFields { network: manifest.sandbox.network };
    let policy = policy_for_plugin(
        &sandbox_fields,
        plugin_dir,
        &source_path,
        temp_dir.path(),
    );

    SandboxedCommand::new(command, policy)
        .spawn()
        .map_err(|e| PluginError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        )))?
};

#[cfg(not(feature = "sandbox"))]
let mut child = spawn_plugin_helper(&mut command)?;
```

Note: `command` is moved into `SandboxedCommand::new`, so it must not be used as `&mut command` anywhere after this point. The existing code already passes ownership implicitly (the `command` variable is not used after `spawn_plugin_helper`), so no other changes are required.

- [ ] **Step 5: Run the baseline test without skip**

```bash
cargo test -p linsync-core --test plugin_sandbox_basic -- --nocapture
```

Expected: PASS. The sandbox is active; the helper runs from `tmp.path()` (which is in `read_paths`) and writes output to the `temp_dir` (which is in `write_paths`). The source path extracted from the JSON is `tmp.path().join("source.txt")` which is inside `tmp.path()` (already in read_paths via the plugin_dir fallback if not extracted explicitly — since we added it explicitly, it's definitely covered).

- [ ] **Step 6: Run the existing plugin test suite**

```bash
LINSYNC_SANDBOX_SKIP=1 cargo test -p linsync-core -- --nocapture 2>&1 | tail -5
```

Expected: all PASS. Tests that use synthetic plugin scripts already operate within their TempDir, so the sandbox does not break them. Setting `LINSYNC_SANDBOX_SKIP=1` ensures the standard test suite is unaffected.

```bash
cargo test -p linsync-core -- --nocapture 2>&1 | tail -5
```

Expected: all PASS with sandbox enabled (Landlock or bubblewrap). If any test fails with sandbox enabled but passes with `LINSYNC_SANDBOX_SKIP=1`, investigate which path is being blocked (use `RUST_LOG=linsync_sandbox=debug`).

- [ ] **Step 7: Commit**

```bash
git add crates/linsync-core/Cargo.toml crates/linsync-core/src/plugin.rs crates/linsync-core/tests/plugin_sandbox_basic.rs
git commit -m "feat(core): wire run_plugin_helper_with_temp through SandboxedCommand"
```

---

## Task 6.8 — Update `deny.toml` and verify `cargo deny check`

**Files:**
- Modify: `deny.toml` (if any new license appears)

`landlock` 0.4 and `seccompiler` 0.4 are both Apache-2.0. Apache-2.0 is already in the `allow` list.

- [ ] **Step 1: Run `cargo deny check licenses`**

```bash
cargo deny check licenses
```

Expected: zero errors. Common newly-introduced transitive deps and their licenses (all already in the allow list):
- `linux-raw-sys`: MIT
- `rustix`: Apache-2.0 OR MIT
- `zerocopy`: Apache-2.0
- `zerocopy-derive`: Apache-2.0

If output contains `error[license-not-allowed]`, add the flagged license to `deny.toml`'s allow list.

- [ ] **Step 2: Run full `cargo deny check`**

```bash
cargo deny check
```

Expected: zero errors across licenses, bans, and advisories.

- [ ] **Step 3: Commit if `deny.toml` changed**

```bash
git diff deny.toml
# If non-empty:
git add deny.toml
git commit -m "chore: allow licenses introduced by landlock + seccompiler transitive deps"
# If empty: no commit needed for this task.
```

---

## Task 6.9 — Stress tests

**Files:**
- Create: `crates/linsync-core/tests/plugin_sandbox_stress.rs`

- [ ] **Step 1: Create the stress test file**

Create `crates/linsync-core/tests/plugin_sandbox_stress.rs`:

```rust
//! Sandbox stress tests.
//!
//! Run in the `sandbox-stress` CI job (Ubuntu 24.04, kernel 6.8, Landlock ABI 5).
//! Standard CI sets LINSYNC_SANDBOX_SKIP=1 to skip these.

use linsync_core::plugin::{
    PluginClass, PluginExecutionOptions, PluginManifest, PluginSandbox, run_plugin_helper,
};
use std::time::Duration;
use tempfile::TempDir;
use std::os::unix::fs::PermissionsExt;

fn sandbox_active() -> bool {
    std::env::var_os("LINSYNC_SANDBOX_SKIP").is_none()
}

fn script_manifest(dir: &std::path::Path, id: &str, script: &[u8]) -> PluginManifest {
    let name = format!("{id}.sh");
    let path = dir.join(&name);
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    PluginManifest {
        schema_version: 1,
        id: id.into(),
        name: id.into(),
        version: "1.0".into(),
        license: "MIT".into(),
        entry: vec![format!("./{name}")],
        classes: vec![PluginClass::Unpacker],
        mime_types: vec![],
        extensions: vec![],
        capabilities: vec![],
        deterministic: true,
        sandbox: PluginSandbox { network: false, writes_input: false, requires_home_access: false },
        streaming: false,
        options_schema: vec![],
    }
}

fn network_manifest(dir: &std::path::Path, id: &str, script: &[u8]) -> PluginManifest {
    let mut m = script_manifest(dir, id, script);
    m.sandbox.network = true;
    m
}

fn source_req(source: &std::path::Path) -> String {
    serde_json::json!({"op":"probe","source": source.to_str().unwrap()}).to_string()
}

/// Plugin tries to read a secret file via a symlink placed inside its allowed dir.
/// Landlock resolves the symlink target and denies access.
#[test]
fn symlink_escape_denied() {
    if !sandbox_active() { eprintln!("SKIP: sandbox not active"); return; }

    let tmp = TempDir::new().unwrap();
    let secret_dir = TempDir::new().unwrap();
    let secret = secret_dir.path().join("secret.txt");
    std::fs::write(&secret, b"top secret").unwrap();

    // Place a symlink inside the plugin dir pointing outside it.
    let symlink = tmp.path().join("evil_link");
    std::os::unix::fs::symlink(&secret, &symlink).unwrap();

    let script = format!(
        "#!/bin/sh\nread REQ\ncat '{}' && echo '{{\"ok\":true}}' || echo '{{\"ok\":false}}'",
        symlink.display()
    );
    let manifest = script_manifest(tmp.path(), "symlink-escape", script.as_bytes());

    let source = tmp.path().join("s.txt");
    std::fs::write(&source, b"x").unwrap();
    let opts = PluginExecutionOptions { timeout: Duration::from_secs(5), ..Default::default() };

    let result = run_plugin_helper(tmp.path(), &manifest, &source_req(&source), &opts);
    match result {
        Err(_) => { /* Landlock denied; helper exited non-zero */ }
        Ok(r) => {
            assert!(
                r.stdout.contains("\"ok\":false") || !r.stdout.contains("top secret"),
                "symlink escape should have been denied; stdout={}", r.stdout
            );
        }
    }
}

/// Plugin tries to fork 20 times. Default proc_limit (16) causes some forks to fail.
#[test]
fn fork_bomb_limited() {
    if !sandbox_active() { eprintln!("SKIP: sandbox not active"); return; }

    let tmp = TempDir::new().unwrap();
    let script = b"#!/bin/bash\nread REQ\ncount=0\nfor i in $(seq 1 20); do\n  bash -c 'sleep 0' & pid=$!\n  if [ $? -eq 0 ]; then count=$((count+1)); fi\ndone\nwait\necho \"{\\\"ok\\\":true,\\\"forks\\\":${count}}\"\n";
    let manifest = script_manifest(tmp.path(), "fork-bomb", script);

    let source = tmp.path().join("s.txt");
    std::fs::write(&source, b"x").unwrap();
    let opts = PluginExecutionOptions { timeout: Duration::from_secs(10), ..Default::default() };

    let result = run_plugin_helper(tmp.path(), &manifest, &source_req(&source), &opts);
    match result {
        Err(_) => { /* proc limit killed the helper */ }
        Ok(r) => {
            let v: serde_json::Value = serde_json::from_str(&r.stdout).unwrap_or_default();
            let forks = v["forks"].as_u64().unwrap_or(20);
            assert!(forks < 20, "expected fewer than 20 successful forks under proc_limit; got {forks}");
        }
    }
}

/// Plugin emits more bytes than stdout_limit. read_limited_text returns OutputTooLarge.
#[test]
fn oversize_stdout_rejected() {
    if !sandbox_active() { eprintln!("SKIP: sandbox not active"); return; }

    let tmp = TempDir::new().unwrap();
    let script = b"#!/bin/sh\nread REQ\npython3 -c \"print('x' * 2097152)\"";
    let manifest = script_manifest(tmp.path(), "oversize-stdout", script);

    let source = tmp.path().join("s.txt");
    std::fs::write(&source, b"x").unwrap();
    let opts = PluginExecutionOptions {
        stdout_limit: 1024 * 1024,
        timeout: Duration::from_secs(10),
        ..Default::default()
    };

    let result = run_plugin_helper(tmp.path(), &manifest, &source_req(&source), &opts);
    assert!(
        matches!(result, Err(linsync_core::plugin::PluginError::OutputTooLarge { .. })),
        "expected OutputTooLarge; got {:?}", result
    );
}

/// Plugin traps SIGTERM and sleeps forever. Timeout fires; helper is killed.
#[test]
fn timeout_escape_killed() {
    if !sandbox_active() { eprintln!("SKIP: sandbox not active"); return; }

    let tmp = TempDir::new().unwrap();
    let script = b"#!/bin/sh\ntrap '' TERM\nread REQ\nsleep 3600\necho '{\"ok\":true}'";
    let manifest = script_manifest(tmp.path(), "timeout-escape", script);

    let source = tmp.path().join("s.txt");
    std::fs::write(&source, b"x").unwrap();
    let opts = PluginExecutionOptions {
        timeout: Duration::from_millis(500),
        ..Default::default()
    };

    let start = std::time::Instant::now();
    let result = run_plugin_helper(tmp.path(), &manifest, &source_req(&source), &opts);
    let elapsed = start.elapsed();

    assert!(
        matches!(result, Err(linsync_core::plugin::PluginError::TimedOut { .. })),
        "expected TimedOut; got {:?}", result
    );
    assert!(elapsed < Duration::from_secs(5), "SIGKILL should fire before 5s; elapsed={elapsed:?}");
}

/// AF_INET socket blocked when sandbox.network=false (default).
#[test]
fn network_denied_by_default() {
    if !sandbox_active() { eprintln!("SKIP: sandbox not active"); return; }

    let tmp = TempDir::new().unwrap();
    let script = b"#!/bin/sh\nread REQ\npython3 -c \"\nimport socket, sys\ntry:\n    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)\n    print('{\\\"ok\\\":true,\\\"got_socket\\\":true}')\nexcept PermissionError:\n    print('{\\\"ok\\\":true,\\\"got_socket\\\":false}')\n\"\n";
    let manifest = script_manifest(tmp.path(), "net-denied", script);

    let source = tmp.path().join("s.txt");
    std::fs::write(&source, b"x").unwrap();
    let opts = PluginExecutionOptions { timeout: Duration::from_secs(5), ..Default::default() };

    let result = run_plugin_helper(tmp.path(), &manifest, &source_req(&source), &opts).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(v["got_socket"].as_bool(), Some(false),
        "AF_INET socket should be denied; stdout={}", result.stdout);
}

/// AF_INET socket succeeds when sandbox.network=true.
#[test]
fn network_allowed_when_declared() {
    if !sandbox_active() { eprintln!("SKIP: sandbox not active"); return; }

    let tmp = TempDir::new().unwrap();
    let script = b"#!/bin/sh\nread REQ\npython3 -c \"\nimport socket, sys\ntry:\n    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)\n    s.close()\n    print('{\\\"ok\\\":true,\\\"got_socket\\\":true}')\nexcept PermissionError:\n    print('{\\\"ok\\\":true,\\\"got_socket\\\":false}')\n\"\n";
    let manifest = network_manifest(tmp.path(), "net-allowed", script);

    let source = tmp.path().join("s.txt");
    std::fs::write(&source, b"x").unwrap();
    let opts = PluginExecutionOptions { timeout: Duration::from_secs(5), ..Default::default() };

    let result = run_plugin_helper(tmp.path(), &manifest, &source_req(&source), &opts).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(v["got_socket"].as_bool(), Some(true),
        "socket should succeed when network=true; stdout={}", result.stdout);
}
```

Note: verify `PluginError::OutputTooLarge` is the correct variant name by checking `crates/linsync-core/src/plugin.rs`. Search for `OutputTooLarge` or `TooLarge` and adjust the `matches!` pattern in `oversize_stdout_rejected` accordingly.

- [ ] **Step 2: Run with skip, expect all to self-skip (PASS)**

```bash
LINSYNC_SANDBOX_SKIP=1 cargo test -p linsync-core --test plugin_sandbox_stress -- --nocapture
```

Expected: all 6 emit "SKIP:" and pass.

- [ ] **Step 3: Run without skip on a capable kernel**

```bash
cargo test -p linsync-core --test plugin_sandbox_stress -- --nocapture
```

Expected: all 6 PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/linsync-core/tests/plugin_sandbox_stress.rs
git commit -m "test(sandbox): stress tests — symlink escape, fork bomb, oversize stdout, timeout, network"
```

---

## Task 6.10 — Update Flatpak manifest

**Files:**
- Modify: `packaging/flatpak/com.visorcraft.LinSync.yml`

- [ ] **Step 1: Add comment block**

In `packaging/flatpak/com.visorcraft.LinSync.yml`, between the `finish-args` block (ending at line 62 with `--persist=.cache/linsync`) and the `build-options:` line, insert the following block (preserving 0-indent YAML):

```yaml

# ---------------------------------------------------------------------------
# Plugin sandbox (Landlock + seccomp-bpf)
# ---------------------------------------------------------------------------
# Plugin helper processes are wrapped by linsync-sandbox::SandboxedCommand
# before exec. Inside the Flatpak sandbox the process already runs in a
# separate mount namespace provided by the Flatpak runtime. Landlock adds
# intra-sandbox filesystem isolation on top:
#
#   Plugin directory       read + execute
#   Source file (request)  read only
#   $XDG_CACHE_HOME/linsync/plugin-tmp/<pid>/<invocation>/  read + write
#
# seccomp-bpf blocks AF_INET/AF_INET6/AF_NETLINK when sandbox.network=false
# (the default), and unconditionally blocks setuid, ptrace, kernel module
# loading.
#
# The --persist=.cache/linsync finish-arg below is what gives plugin temp
# dirs a Flatpak-visible path under $XDG_CACHE_HOME/linsync/plugin-tmp/.
#
# NOTE: --talk-name=org.freedesktop.Flatpak is intentionally absent.
# User-installed plugins outside the Flatpak are not yet a supported use
# case. When that changes, the finish-arg should be added as a user-level
# Flatpak override rather than unconditionally in this manifest.
#
# See docs/sandbox-design.md for the full threat model and architecture.
# ---------------------------------------------------------------------------

```

- [ ] **Step 2: Verify YAML parses**

```bash
python3 -c "import yaml; yaml.safe_load(open('packaging/flatpak/com.visorcraft.LinSync.yml'))" && echo "YAML OK"
```

Expected: `YAML OK`.

- [ ] **Step 3: Commit**

```bash
git add packaging/flatpak/com.visorcraft.LinSync.yml
git commit -m "docs(flatpak): document intra-sandbox Landlock enforcement for plugin helpers"
```

---

## Task 6.11 — Update `docs/plugin-protocol.md`

**Files:**
- Modify: `docs/plugin-protocol.md`

- [ ] **Step 1: Locate the insertion point**

Open `docs/plugin-protocol.md`. Find the `## Manifest` section. The new "Sandboxing" section goes after the existing "## Operations" section (or as the last top-level section if Operations is the last one). Search for `## Operations` to find it; if the heading does not exist, append at end of file.

- [ ] **Step 2: Append the Sandboxing section**

Add the following at the end of the file (or after the Operations section):

```markdown
## Sandboxing

Every helper process launched by `run_plugin_helper` is wrapped in a
kernel-enforced sandbox before exec.  The sandbox policy is derived from
the manifest's `sandbox` block:

```json
"sandbox": {
  "network": false,
  "writes_input": false,
  "requires_home_access": false
}
```

All fields default to `false`.

### `sandbox.network`

When `false` (the default), outbound network sockets (`AF_INET`, `AF_INET6`,
`AF_NETLINK`) are blocked via seccomp-bpf.  `AF_UNIX` is always permitted
(required for D-Bus and Wayland IPC used by some runtimes).

Set `"network": true` only for plugins that need to fetch remote resources
(e.g. the future `web-fetch` plugin class).

### `sandbox.writes_input` and `sandbox.requires_home_access`

Both fields are reserved for future use and currently ignored.  The sandbox
always makes the source path read-only and never grants access to `$HOME`
outside the declared path set.

### Filesystem policy per invocation

| Path | Access |
|---|---|
| Plugin directory | read + execute |
| Source file (from request `"source"` field) | read only |
| `$XDG_CACHE_HOME/linsync/plugin-tmp/<pid>/<id>/` | read + write |

No other path is accessible.  A symlink pointing outside the allowed set is
denied at the kernel level — Landlock resolves symlink targets before
checking the ruleset.

### Sandbox backends

| Condition | Backend used |
|---|---|
| Kernel >= 5.13, Landlock ABI >= 1 | Landlock + seccomp-bpf |
| Kernel < 5.13, `bwrap` binary found | bubblewrap + seccomp-bpf |
| Neither available | Degraded (unsandboxed) — `WARN` logged |

Set the environment variable `LINSYNC_SANDBOX_SKIP=1` to force degraded mode.
This is used in CI jobs that run the standard test suite inside restricted
containers.  The `sandbox-stress` CI job leaves the variable unset and runs
on Ubuntu 24.04 (kernel 6.8, Landlock ABI 5).

See `docs/sandbox-design.md` for the full threat model and design rationale.
```

- [ ] **Step 3: Verify the file is valid Markdown**

```bash
python3 -c "
text = open('docs/plugin-protocol.md').read()
assert '## Sandboxing' in text
assert 'sandbox.network' in text
assert 'Landlock' in text
print('plugin-protocol.md OK')
"
```

Expected: `plugin-protocol.md OK`.

- [ ] **Step 4: Commit**

```bash
git add docs/plugin-protocol.md
git commit -m "docs(plugin): add Sandboxing section documenting sandbox manifest block + policy"
```

---

## Task 6.12 — Add `LINSYNC_SANDBOX_SKIP=1` to `scripts/gui-smoke.sh`

**Files:**
- Modify: `scripts/gui-smoke.sh`

- [ ] **Step 1: Locate the existing env-var exports**

Open `scripts/gui-smoke.sh`. Find the block that exports `QT_QPA_PLATFORM`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_STATE_HOME` (around line 49 based on the file header read earlier). It looks like:

```bash
export QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}"
export XDG_CONFIG_HOME="${tmpdata}/config"
export XDG_DATA_HOME="${tmpdata}/data"
export XDG_CACHE_HOME="${tmpdata}/cache"
export XDG_STATE_HOME="${tmpdata}/state"
```

- [ ] **Step 2: Add LINSYNC_SANDBOX_SKIP immediately after those lines**

The `:-1` default lets a caller override with `LINSYNC_SANDBOX_SKIP=""` to enable the sandbox during local smoke testing:

```bash
export LINSYNC_SANDBOX_SKIP="${LINSYNC_SANDBOX_SKIP:-1}"
```

- [ ] **Step 3: Run the smoke to confirm it still passes**

```bash
bash scripts/gui-smoke.sh
```

Expected: PASS. The env var is now set to 1 by default, disabling sandbox setup in the smoke environment.

- [ ] **Step 4: Confirm the override works**

```bash
LINSYNC_SANDBOX_SKIP="" bash scripts/gui-smoke.sh 2>&1 | head -10
```

Expected: smoke starts and runs without errors (sandbox enabled but toy helper scripts stay within their temp dirs).

- [ ] **Step 5: Commit**

```bash
git add scripts/gui-smoke.sh
git commit -m "ci(smoke): export LINSYNC_SANDBOX_SKIP=1 so smoke works in restricted containers"
```

---

## Completion Checklist

Run all of these after completing Task 6.12. Every command must exit 0.

- [ ] `cargo build --workspace` — zero errors
- [ ] `cargo clippy --workspace -- -D warnings` — zero warnings
- [ ] `cargo fmt --all -- --check` — zero formatting diffs
- [ ] `cargo deny check` — zero license errors
- [ ] `cargo test -p linsync-sandbox` — all PASS
- [ ] `LINSYNC_SANDBOX_SKIP=1 cargo test --workspace` — all PASS (existing tests unaffected)
- [ ] `cargo test -p linsync-core --test plugin_sandbox_stress` — all PASS on capable kernel (Landlock ABI >= 1)
- [ ] `bash scripts/gui-smoke.sh` — PASS

---

## Open Questions Noted During Planning

1. **`PluginError::OutputTooLarge` variant name:** The stress test uses `PluginError::OutputTooLarge { .. }`. Confirm the actual variant name in `crates/linsync-core/src/plugin.rs` before Task 6.9. The format! output lines in the `Display` impl (around line 512-518) show variants for stream/output size errors — find the one that fires when `read_limited_text` detects an oversize file.

2. **`SeccompFilter::new` argument order:** seccompiler 0.4's `new(rules, mismatch_action, match_action, arch)` may have different semantics than described. The intent is: matched (blocked) syscalls return `EACCES`; everything else is allowed. Run `cargo doc -p linsync-sandbox` and verify the argument order before implementing Task 6.4.

3. **Landlock `AccessFs::from_read` and `from_all` in 0.4:** These convenience constructors may be named differently in landlock 0.4. If `from_read` does not exist, use an explicit bitmask: `AccessFs::ReadFile | AccessFs::ReadDir`. Run `cargo doc -p linsync-sandbox` after adding the dep.

4. **BPF byte encoding for bwrap `--seccomp`:** The `pipe_seccomp_filter` function in Task 6.5 encodes each `sock_filter` instruction as 8 bytes (code:2 + jt:1 + jf:1 + k:4, all LE). This must match the kernel's `sock_filter` ABI exactly. Verify that `seccompiler::BpfProgram` is `Vec<libc::sock_filter>` in 0.4 (it is in 0.3) and that the field layout matches.

5. **Landlock ABI version floor for Truncate:** Task 6.3 activates `AccessFs::Truncate` only on ABI v3+ (kernel 6.2). If bundled plugin scripts use `truncate(2)` on kernels between 5.13 and 6.2 they will receive `EPERM`. No bundled plugins currently use `truncate` — confirm this remains true before Task 6.7 merge.
