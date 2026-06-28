//! Plugin-helper sandbox enforcement.
//!
//! Call [`SandboxedCommand::spawn`] in place of [`std::process::Command::spawn`]
//! to apply a [`SandboxPolicy`] before the child executes.
//!
//! On Linux kernels >= 5.13 the primary path uses Landlock + seccomp-bpf.
//! On older kernels it falls back to `bwrap` (bubblewrap).
//! If neither backend is available the spawn **fails closed**
//! ([`SandboxError::NoSandboxAvailable`]) rather than silently running the
//! helper unconfined. An operator who knowingly accepts the risk on a
//! restricted host can opt back in with `LINSYNC_SANDBOX_ALLOW_UNSANDBOXED=1`;
//! the dedicated test/CI escape hatch `LINSYNC_SANDBOX_SKIP` is unchanged.

pub mod policy;

#[cfg(target_os = "linux")]
pub(crate) mod bubblewrap;
#[cfg(target_os = "linux")]
pub(crate) mod landlock;
#[cfg(target_os = "linux")]
pub(crate) mod seccomp;

/// Set a soft+hard `setrlimit(2)` resource limit. Async-signal-safe — safe to
/// call from `pre_exec`. Shared by the Landlock and bubblewrap backends.
#[cfg(target_os = "linux")]
pub(crate) fn set_rlimit(resource: libc::__rlimit_resource_t, value: u64) -> std::io::Result<()> {
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

pub use policy::{PluginSandboxFields, SandboxPolicy, policy_for_plugin};

use std::process::{Child, Command};

/// Host environment variables that are safe and necessary for a confined
/// plugin helper to run: locating interpreters and external tools via `PATH`,
/// resolving the user profile (`HOME`) for tools like LibreOffice, locale, and
/// the desktop session essentials a viewer/extractor may consult. Every other
/// variable from the host environment is dropped before exec so a confined
/// helper never inherits host-process secrets (API keys, tokens, credentials
/// that may be present in the parent environment).
///
/// Both backends apply this list: Landlock via `Command::env_clear` +
/// re-injection, bubblewrap via `--clearenv` + `--setenv`. The caller's
/// explicitly-set variables (e.g. `LINSYNC_PLUGIN_TEMP_DIR`) are always
/// preserved on top of this list.
#[cfg(target_os = "linux")]
pub(crate) const SANDBOX_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "TERM",
    "TZ",
    "TMPDIR",
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_COLLATE",
    "LC_MONETARY",
    "LC_PAPER",
    "LC_MEASUREMENT",
    "LC_IDENTIFICATION",
    // Desktop session essentials some viewer/extractor helpers consult. None
    // of these are secrets; credential-bearing variables are intentionally
    // excluded by virtue of not being on this list.
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_RUNTIME_DIR",
    "XDG_DATA_DIRS",
    "XDG_CONFIG_DIRS",
    "DBUS_SESSION_BUS_ADDRESS",
];

/// Collect the host values for [`SANDBOX_ENV_ALLOWLIST`] that are present in
/// the current process environment, as `(name, value)` pairs ready to inject
/// into a confined child. Used by both sandbox backends.
///
/// In addition to the static allowlist, every `LINSYNC_*` variable is
/// forwarded: these are LinSync's own configuration toggles (part of the
/// plugin contract, e.g. test/debug switches), never third-party secrets.
#[cfg(target_os = "linux")]
pub(crate) fn allowlisted_host_env() -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    let mut out: Vec<(std::ffi::OsString, std::ffi::OsString)> = SANDBOX_ENV_ALLOWLIST
        .iter()
        .filter_map(|name| std::env::var_os(name).map(|v| (std::ffi::OsString::from(name), v)))
        .collect();
    for (k, v) in std::env::vars_os() {
        if k.as_encoded_bytes().starts_with(b"LINSYNC_") {
            out.push((k, v));
        }
    }
    out
}

/// Read-only host paths a network-enabled plugin needs for hostname resolution
/// (glibc resolver + NSS, including systemd-resolved) and TLS certificate
/// verification.
///
/// These are added to the sandbox read set **only** when the plugin's policy
/// grants network access; non-network plugins never gain visibility into them.
/// Without these, a network plugin under Landlock/bwrap cannot read
/// `/etc/resolv.conf`/`nsswitch.conf`, so `getaddrinfo()` fails with
/// `EAI_AGAIN` ("Temporary failure in name resolution") even though sockets are
/// permitted. The caller filters out paths that do not exist on the host.
#[cfg(target_os = "linux")]
pub(crate) fn network_resolution_read_paths() -> &'static [&'static str] {
    &[
        // Name resolution: glibc resolver + NSS configuration.
        "/etc/resolv.conf",
        "/etc/nsswitch.conf",
        "/etc/hosts",
        "/etc/host.conf",
        "/etc/gai.conf",
        "/etc/services",
        "/etc/protocols",
        // systemd-resolved: /etc/resolv.conf is typically a symlink into here,
        // and nss-resolve reaches the resolver via the socket in this dir.
        "/run/systemd/resolve",
        // NSS backends (e.g. nss-resolve) that talk over the system D-Bus.
        "/run/dbus/system_bus_socket",
        // TLS trust store for HTTPS certificate verification.
        "/etc/ssl",
        "/etc/ca-certificates",
        "/etc/pki",
    ]
}

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
            Self::NoSandboxAvailable => write!(
                f,
                "no sandbox available (Landlock < ABI 1 and bwrap not found)"
            ),
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

#[cfg(all(test, target_os = "linux"))]
mod env_tests {
    use super::SandboxStrategy;

    #[test]
    fn strategy_confinement_and_labels() {
        assert!(SandboxStrategy::Landlock.is_confined());
        assert_eq!(SandboxStrategy::Landlock.describe(), "landlock+seccomp");
        let bwrap = SandboxStrategy::Bubblewrap(std::path::PathBuf::from("/usr/bin/bwrap"));
        assert!(bwrap.is_confined());
        assert_eq!(bwrap.describe(), "bubblewrap");
        // Degraded never reports confinement; the exact label depends on env.
        assert!(!SandboxStrategy::Degraded.is_confined());
    }

    #[test]
    fn allowlist_includes_runtime_essentials_and_excludes_secrets() {
        assert!(super::SANDBOX_ENV_ALLOWLIST.contains(&"PATH"));
        assert!(super::SANDBOX_ENV_ALLOWLIST.contains(&"HOME"));
        for bad in [
            "AWS_SECRET_ACCESS_KEY",
            "GITHUB_TOKEN",
            "SSH_AUTH_SOCK",
            "OPENAI_API_KEY",
            "AWS_SESSION_TOKEN",
        ] {
            assert!(
                !super::SANDBOX_ENV_ALLOWLIST.contains(&bad),
                "{bad} must never be on the sandbox env allowlist"
            );
        }
    }

    #[test]
    fn allowlisted_host_env_drops_secrets_but_forwards_linsync_vars() {
        // SAFETY: unique var names; env mutation in tests is racy, so run with
        // `--test-threads=1` (the sandbox crate's other env tests do the same).
        let secret = "DEFINITELY_NOT_ALLOWLISTED_SECRET_XYZ";
        let lin = "LINSYNC_ENV_TEST_XYZ";
        unsafe {
            std::env::set_var(secret, "leak");
            std::env::set_var(lin, "ok");
        }
        let names: Vec<String> = super::allowlisted_host_env()
            .iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        unsafe {
            std::env::remove_var(secret);
            std::env::remove_var(lin);
        }
        assert!(
            !names.iter().any(|n| n == secret),
            "a non-allowlisted host variable must not be forwarded to the sandbox"
        );
        assert!(
            names.iter().any(|n| n == lin),
            "LINSYNC_* variables must be forwarded (plugin contract)"
        );
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

    /// Whether helpers actually run confined under this strategy. `Degraded`
    /// means no confinement is applied (helpers either run unsandboxed via an
    /// explicit opt-out, or are refused).
    pub fn is_confined(&self) -> bool {
        matches!(self, Self::Landlock | Self::Bubblewrap(_))
    }

    /// A stable, human-facing label for diagnostics. For `Degraded` it
    /// distinguishes the reason (explicit skip, accepted-risk opt-out, or no
    /// backend available — in which case helpers are refused).
    pub fn describe(&self) -> &'static str {
        match self {
            Self::Landlock => "landlock+seccomp",
            Self::Bubblewrap(_) => "bubblewrap",
            Self::Degraded => {
                if std::env::var_os("LINSYNC_SANDBOX_SKIP").is_some() {
                    "degraded (LINSYNC_SANDBOX_SKIP set: unsandboxed)"
                } else if std::env::var_os("LINSYNC_SANDBOX_ALLOW_UNSANDBOXED").is_some() {
                    "degraded (LINSYNC_SANDBOX_ALLOW_UNSANDBOXED set: unsandboxed)"
                } else {
                    "degraded (no backend available: helpers refused)"
                }
            }
        }
    }
}

/// Wraps a [`Command`] with sandbox policy, then spawns it.
///
/// On the `Landlock` path the policy is installed inside the child via
/// `pre_exec`. On the `Bubblewrap` path the command is rewritten to run
/// through `bwrap`. On `Degraded` the spawn fails closed with
/// [`SandboxError::NoSandboxAvailable`] unless `LINSYNC_SANDBOX_SKIP` or
/// `LINSYNC_SANDBOX_ALLOW_UNSANDBOXED` is set, in which case it spawns
/// unchanged after a `tracing::warn!`.
pub struct SandboxedCommand {
    inner: Command,
    policy: SandboxPolicy,
    strategy: SandboxStrategy,
}

impl SandboxedCommand {
    pub fn new(command: Command, policy: SandboxPolicy) -> Self {
        let strategy = SandboxStrategy::detect();
        Self {
            inner: command,
            policy,
            strategy,
        }
    }

    /// Spawn the child process, applying the sandbox according to the
    /// detected strategy.
    pub fn spawn(mut self) -> Result<Child, SandboxError> {
        match self.strategy {
            SandboxStrategy::Degraded => {
                if std::env::var_os("LINSYNC_SANDBOX_SKIP").is_some() {
                    // Explicit, trusted opt-out (CI / headless test rigs).
                    tracing::warn!(
                        "LINSYNC_SANDBOX_SKIP set: plugin helper running UNSANDBOXED by request"
                    );
                    spawn_retrying_etxtbsy(&mut self.inner)
                } else if std::env::var_os("LINSYNC_SANDBOX_ALLOW_UNSANDBOXED").is_some() {
                    // No backend available, but the operator accepted the risk.
                    tracing::warn!(
                        "no sandbox backend available (Landlock < ABI 1 and bwrap not found); \
                         LINSYNC_SANDBOX_ALLOW_UNSANDBOXED set: running helper UNSANDBOXED"
                    );
                    spawn_retrying_etxtbsy(&mut self.inner)
                } else {
                    // Fail closed: never run an untrusted helper with no confinement.
                    tracing::error!(
                        "no sandbox backend available (Landlock < ABI 1 and bwrap not found); \
                         refusing to run plugin helper unsandboxed. Install bubblewrap or use a \
                         kernel with Landlock (>= 5.13). Set LINSYNC_SANDBOX_ALLOW_UNSANDBOXED=1 \
                         to override at your own risk."
                    );
                    Err(SandboxError::NoSandboxAvailable)
                }
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
                spawn_retrying_etxtbsy(&mut self.inner)
            }
        }
    }
}

/// Spawn `command`, retrying briefly on `ETXTBSY` (errno 26).
///
/// A helper that was just written — a freshly installed/updated plugin, or a
/// test fixture — can be reported "text file busy" by the kernel when a
/// concurrent `fork` in another thread momentarily inherited the executable's
/// still-open write descriptor (the descriptor is closed in the child only at
/// `exec`). A short bounded retry rides out that window instead of failing the
/// spawn outright. Non-`ETXTBSY` errors return immediately.
pub(crate) fn spawn_retrying_etxtbsy(command: &mut Command) -> Result<Child, SandboxError> {
    let mut last_err = None;
    for attempt in 0..10u32 {
        match command.spawn() {
            Ok(child) => return Ok(child),
            Err(err) if err.raw_os_error() == Some(libc::ETXTBSY) => {
                last_err = Some(err);
                std::thread::sleep(std::time::Duration::from_millis(
                    10 * u64::from(attempt + 1),
                ));
            }
            Err(err) => return Err(SandboxError::Os(err)),
        }
    }
    Err(SandboxError::Os(last_err.unwrap_or_else(|| {
        std::io::Error::other("failed to spawn sandboxed command after ETXTBSY retries")
    })))
}
