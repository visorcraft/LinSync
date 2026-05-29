# Plugin Sandbox Foundation Design

> Status: design — implementation pending follow-up plan.

## Goals

Wrap every plugin helper process in a kernel-enforced sandbox before exec so that
a compromised or malicious plugin cannot read arbitrary user files, write outside
its temp directory, or open network connections it did not declare.

## Non-goals

- Windows or macOS sandbox support (LinSync is Linux-only).
- Sandboxing the LinSync host process itself (that is an OS/Flatpak concern).
- Content-inspection of plugin JSON output (the existing size and path checks cover this).
- Per-operation syscall allow-lists beyond the network and filesystem axes.
- Writable archive-member editing safety (Phase 10 owns that design).

## Threat model

**What the sandbox protects against:**

- A plugin reading files outside its declared inputs (e.g., `~/.ssh/id_rsa`).
- A plugin writing to arbitrary paths on the host filesystem.
- A plugin opening outbound network connections when `sandbox.network` is false.
- A plugin fork-bombing or exhausting file descriptors (process rlimits added by
  the sandbox wrapper).
- A plugin following symlinks to escape its allowed path set.

**What the sandbox does not protect against:**

- A plugin exploiting a kernel vulnerability to break out of Landlock/seccomp.
- Side-channel attacks between host and plugin (timing, cache).
- Plugins the user installs from untrusted sources — the sandbox reduces blast
  radius but is not a substitute for provenance verification.
- Exfiltration via allowed paths (a plugin can read what it is given read access to).

## Sandbox stack

### Native Linux

**Primary:** Landlock (kernel 5.13+) + seccomp-bpf via the
[`seccompiler`](https://crates.io/crates/seccompiler) crate (Apache-2.0,
GPL-3.0 compatible).

Landlock restricts filesystem access to the three permitted path sets (plugin
dir, per-invocation temp dir, source file) with `LANDLOCK_ACCESS_FS_READ_FILE |
LANDLOCK_ACCESS_FS_READ_DIR` on the plugin dir and source, plus
`LANDLOCK_ACCESS_FS_MAKE_REG | LANDLOCK_ACCESS_FS_WRITE_FILE` on the temp dir.
seccomp-bpf adds a narrow syscall allow-list that blocks `socket(2)` when
`sandbox.network` is false, and blocks `setuid`, `ptrace`, and raw kernel module
calls unconditionally.

**Fallback for kernels < 5.13:** `bubblewrap` (`bwrap` binary, LGPL-2.0+,
GPL-3.0 compatible). The sandbox wrapper detects Landlock ABI support at runtime
via `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)`. If that
returns `ENOSYS` or the returned ABI is < 1, the wrapper falls back to invoking
`bwrap` with `--ro-bind` for plugin dir + source path, `--bind` for temp dir,
and `--unshare-net` when `sandbox.network` is false. `bwrap` is expected to be
present on any modern desktop Linux; if missing, the host enters degraded mode
(see Failure modes).

Landlock does not restrict network in its filesystem ABI; seccomp-bpf handles
the `socket(2)` block independently of the fallback path, so seccomp is applied
in both cases.

### Flatpak

Inside a Flatpak sandbox the process already runs in a separate mount namespace.
Plugin processes are launched via `flatpak-spawn --host` when the plugin is
user-installed outside the Flatpak (requiring the `--talk-name=org.freedesktop.Flatpak`
finish-arg, added to the manifest). Plugins bundled inside the Flatpak run within
the existing Flatpak sandbox; for those, Landlock + seccomp-bpf is applied
exactly as in the native case — Flatpak does not disable Landlock for guest
processes.

The Flatpak manifest gains no new broad permissions. The plugin temp dir lives
under `$XDG_CACHE_HOME/linsync/plugin-tmp/<pid>/` which is already persisted
via `--persist=.cache/linsync`.

### AppImage

The AppImage runtime does not apply any sandbox of its own. The LinSync binary
inside the AppImage applies Landlock + seccomp-bpf from within itself, identical
to the native case. The `bwrap` fallback is also available because AppImage
distributes a bundled `bwrap` alongside the main binary. No AppImage-specific
changes are required.

### Packaged (deb/rpm/pacman)

Identical to native Linux. `bwrap` is a dependency declared in each packaging
recipe for the fallback case; on systems that ship kernel 5.13+ (Debian trixie,
Ubuntu 22.04 LTS, Fedora 34+, Arch as of 2021) Landlock is the primary path.

## Filesystem policy

Per invocation, the helper process is granted:

| Path | Landlock rights | Notes |
|---|---|---|
| `<plugin_dir>/` | `READ_FILE`, `READ_DIR`, `EXECUTE` | Entry binary + support files |
| `<source_path>` (from request) | `READ_FILE` | The file being unpacked/prediffed |
| `$XDG_CACHE_HOME/linsync/plugin-tmp/<pid>/<invocation-id>/` | `READ_FILE`, `READ_DIR`, `WRITE_FILE`, `MAKE_REG`, `REMOVE_FILE` | Output files, intermediate state |

No other path is readable or writable. `/proc/self` and `/dev/null` are
read-only bound in the bwrap fallback; Landlock does not restrict these by
default in ABI v1.

Symlink traversal within Landlock respects the target path: a symlink pointing
outside the allowed set is denied at the kernel level without special handling
in the Rust wrapper.

## Network policy

`sandbox.network: false` (the default) causes seccomp-bpf to return `EACCES`
for `socket(AF_INET, ...)`, `socket(AF_INET6, ...)`, and `socket(AF_NETLINK, ...)`
calls. `AF_UNIX` (for D-Bus, Wayland, etc.) is allowed because some helper
runtimes need it. The bwrap fallback adds `--unshare-net` which provides the
same guarantee at the namespace level.

`sandbox.network: true` lifts the socket block. Only the future `web-fetch`
plugin class is expected to declare this.

## Crate organization

The sandbox setup lives in a new `crates/linsync-sandbox` crate. Reasons:

- `linsync-core` is a library imported by CLI, GUI, and tests. Pulling kernel
  Landlock/seccomp setup into it drags platform-specific C FFI into every build.
- The GUI (`apps/linsync-gui`) must not own sandbox logic because the CLI also
  spawns plugins directly.
- A dedicated crate can have its own `#[cfg(target_os = "linux")]` guard, its
  own `deny.toml` license check for `seccompiler` + `landlock`, and can be
  replaced or mocked cleanly in tests.

`linsync-core` gains a thin feature flag `sandbox` (default on) that, when
enabled, adds `linsync-sandbox` as a dependency. `run_plugin_helper_with_temp`
calls `linsync_sandbox::apply_policy(&policy)` in the child process after
`Command::pre_exec`.

## API surface

```rust
// crates/linsync-sandbox/src/lib.rs

/// Apply sandbox restrictions inside the child process before exec.
/// Called from `pre_exec` -- must only use async-signal-safe operations.
pub fn apply_policy(policy: &SandboxPolicy) -> Result<(), SandboxError>;

/// Build a `SandboxPolicy` from a plugin manifest + per-invocation paths.
pub fn policy_for_plugin(
    manifest: &PluginManifest,
    plugin_dir: &Path,
    source_path: &Path,
    temp_dir: &Path,
) -> SandboxPolicy;

pub struct SandboxPolicy {
    /// Paths the helper may read (plugin dir, source file).
    pub read_paths: Vec<PathBuf>,
    /// Paths the helper may read and write (temp dir).
    pub write_paths: Vec<PathBuf>,
    /// If false, block outbound network sockets via seccomp.
    pub network: bool,
    /// Maximum number of open file descriptors.
    pub fd_limit: u64,
    /// Maximum number of child processes (fork bomb prevention).
    pub proc_limit: u64,
}

#[derive(Debug)]
pub enum SandboxError {
    /// Landlock not supported; caller should try bubblewrap fallback.
    LandlockUnsupported,
    /// bubblewrap binary not found and Landlock unavailable.
    NoSandboxAvailable,
    /// Low-level OS error during policy application.
    Os(std::io::Error),
}
```

`run_plugin_helper` in `crates/linsync-core/src/plugin.rs` becomes:

```rust
// Existing Command construction is unchanged up to spawn.
// Before spawning, wrap via linsync_sandbox:
#[cfg(feature = "sandbox")]
{
    use linsync_sandbox::{policy_for_plugin, SandboxStrategy};
    let policy = policy_for_plugin(&manifest, plugin_dir, &source_path, temp_dir.path());
    match SandboxStrategy::detect() {
        SandboxStrategy::Landlock => { command.pre_exec(move || {
            linsync_sandbox::apply_policy(&policy).map_err(Into::into)
        }); }
        SandboxStrategy::Bubblewrap(bwrap) => {
            // Rewrap command through bwrap binary with policy-derived flags.
            command = linsync_sandbox::wrap_with_bubblewrap(command, &policy, &bwrap)?;
        }
        SandboxStrategy::Degraded => { /* log warning, continue unsandboxed */ }
    }
}
```

## Failure modes

| Condition | Behavior |
|---|---|
| Landlock ABI < 1, `bwrap` found | Use bubblewrap fallback -- transparent to caller |
| Landlock ABI < 1, `bwrap` not found | `SandboxError::NoSandboxAvailable` -- caller logs a one-time `WARN` and proceeds unsandboxed (degraded mode) |
| `seccompiler` filter install fails | Hard error -- `PluginError::SandboxSetupFailed`; plugin is refused |
| seccomp setup fails in a CI container with `SECCOMP_RET_ERRNO` restricted | Same as above; test suite must opt out via `LINSYNC_SANDBOX_SKIP=1` |
| Landlock path registration fails for a missing path | Hard error -- `PluginError::SandboxSetupFailed`; source path is validated before policy construction |

Degraded mode is explicitly a compromise for headless/restricted environments
(CI containers, old kernels on embedded systems). It must be logged at `WARN`
level and surfaced as a non-fatal notification in the GUI plugins panel.

## Test plan

**Existing tests:** all current plugin tests in `crates/linsync-core/tests/` pass
unchanged because `apply_policy` is a no-op in test builds that set
`LINSYNC_SANDBOX_SKIP=1`. The CI matrix sets this env var for the standard test
job; a separate `sandbox-stress` job enables it.

**New stress tests** in `crates/linsync-core/tests/plugin_sandbox_stress.rs`:

| Test | What it checks |
|---|---|
| `symlink_escape_denied` | Plugin tries to open a symlink pointing outside its allowed paths; expects `EACCES` |
| `fork_bomb_limited` | Plugin forks repeatedly; `proc_limit = 4` causes the 5th fork to fail with `EAGAIN` |
| `oversize_stdout_killed` | Plugin writes > `stdout_limit` bytes; existing limit check still fires |
| `timeout_escape_attempt` | Plugin ignores `SIGTERM`, stays alive past timeout; sandbox enforces with `SIGKILL` |
| `network_denied_by_default` | Plugin calls `socket(AF_INET, SOCK_STREAM, 0)`; expects `EACCES` |
| `network_allowed_when_declared` | Same socket call with `sandbox.network: true`; expects success |

CI strategy: the `sandbox-stress` job runs in a privileged container on an
Ubuntu 24.04 runner which ships kernel 6.8 (Landlock ABI 5). The standard test
job continues with `LINSYNC_SANDBOX_SKIP=1` and no extra capabilities.

```yaml
# Addition to .github/workflows/ci.yml
sandbox-stress:
  runs-on: ubuntu-24.04
  steps:
    - uses: actions/checkout@v4
    - run: cargo test -p linsync-core --test plugin_sandbox_stress
      env:
        LINSYNC_SANDBOX_SKIP: ""   # empty = sandbox enabled
```

## Migration plan

**Existing plugin code:** `run_plugin_helper` and `run_streaming_plugin` in
`crates/linsync-core/src/plugin.rs` each gain a single sandbox setup block
controlled by `#[cfg(feature = "sandbox")]`. The call site is unchanged for all
callers; sandbox setup is transparent.

**Existing manifests:** `PluginSandbox` already has `network: bool` (defaults
false). No manifest changes required for existing plugins. Future plugins that
need network add `"sandbox": { "network": true }` -- the field is already in the
schema.

**Backward compatibility:** disabling the `sandbox` feature (e.g. for
cross-compilation to exotic targets or for the test-support build) restores
pre-Phase-6 behavior exactly. No public API signatures change.

## Dependencies and blocking

**New Cargo dependencies:**

| Crate | License | Notes |
|---|---|---|
| `seccompiler` 0.4 | Apache-2.0 | GPL-3.0 compatible |
| `landlock` 0.4 | Apache-2.0 | GPL-3.0 compatible |

`bubblewrap` is a runtime binary dependency, not a Cargo crate. It must be
added to packaging recipes as a recommended system dependency and documented in
`deny.toml`'s skip list for binary-only deps.

**What this design unblocks:**

- Phase 7 (image compare): image decode helpers run inside sandbox.
- Phase 8 (OCR): Tesseract/poppler helpers sandboxed; the OCR design doc
  prerequisite "security review for untrusted document parsing" is satisfied.
- Phase 9 (webpage): `web-fetch` plugin uses `sandbox.network: true`; the
  sandbox provides the network-gating control that the webpage design requires.
- Phase 10 (archive write): sandbox scopes write access to temp dir + target
  archive path only, satisfying the Flatpak-portal safety precondition.

## Open issues to resolve before implementation

1. **Landlock ABI version floor:** ABI v1 covers file reads/writes but not
   `truncate(2)` (added in ABI v3, kernel 6.2). Some plugins that produce temp
   files via `truncate` may need ABI v3. Decide whether to require ABI v3 as the
   Landlock primary (raising the no-fallback threshold to kernel 6.2) or allow
   `truncate` unconditionally at the seccomp layer.

2. **`pre_exec` + bubblewrap conflict:** `Command::pre_exec` and the bubblewrap
   path are mutually exclusive -- bwrap re-execs the command. A dedicated
   `SandboxedCommand` builder type may be cleaner than mutating
   `std::process::Command` in place; resolve before writing the implementation plan.

3. **Flatpak `--talk-name=org.freedesktop.Flatpak` privilege:** adding this
   finish-arg is a meaningful escalation for the Flatpak build. Gate it on
   user-installed plugins actually being present; consider making it a Flatpak
   override rather than unconditional in `packaging/flatpak/com.visorcraft.LinSync.yml`.

4. **`deny.toml` update:** `seccompiler` and `landlock` crates must be added to
   the allow-list before the sandbox crate can build with `just deny`.

5. **CI container capability requirements:** the `sandbox-stress` job requires
   `LANDLOCK_CREATE_RULESET` and `PR_SET_SECCOMP` to succeed inside GitHub
   Actions runners. Ubuntu 24.04 runners permit both by default -- confirm before
   writing the follow-up implementation plan.
