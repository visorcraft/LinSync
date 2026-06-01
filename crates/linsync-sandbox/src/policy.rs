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
    /// Maximum number of open file descriptors. Default 256.
    pub fd_limit: u64,
    /// Maximum number of processes under RLIMIT_NPROC.
    ///
    /// `RLIMIT_NPROC` is **user-wide**, not per-process tree: it caps the
    /// total number of processes of the calling UID. Setting it too low
    /// (e.g. 16) immediately fails `fork()` because the user already has
    /// many processes running. Default 8192 — high enough that normal
    /// helper shell scripts work, low enough that egregious fork bombs
    /// still hit the cap.
    pub proc_limit: u64,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            read_paths: Vec::new(),
            write_paths: Vec::new(),
            network: false,
            fd_limit: 256,
            proc_limit: 8192,
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
            proc_limit: self.proc_limit.unwrap_or(8192),
        }
    }
}

/// Mirror of `linsync_core::plugin::PluginSandbox` fields needed by the sandbox
/// crate. A plain struct avoids a circular dependency on linsync-core.
#[derive(Debug, Clone, Default)]
pub struct PluginSandboxFields {
    pub network: bool,
    /// When `true`, the user home directory is added to the sandbox read paths.
    /// Required by tools like LibreOffice that store runtime profiles in `$HOME`.
    pub requires_home_access: bool,
}

/// Construct a [`SandboxPolicy`] from the fields that `run_plugin_helper_with_temp`
/// has at the point of spawn.
///
/// - `plugin_dir` is added to `read_paths` (plugin binary + support files).
/// - `source_path` is added to `read_paths` (the file being processed).
/// - `temp_dir` is added to `write_paths` (plugin output, intermediate state).
/// - `sandbox.network` controls the seccomp socket block.
/// - `sandbox.requires_home_access` adds `$HOME` to read paths only (plugins
///   that need to write keep their writable `temp_dir`; granting write over the
///   whole home would let a confined helper tamper with the user's files).
pub fn policy_for_plugin(
    plugin_sandbox: &PluginSandboxFields,
    plugin_dir: &Path,
    source_path: &Path,
    temp_dir: &Path,
) -> SandboxPolicy {
    let mut builder = SandboxPolicy::builder()
        .read(plugin_dir)
        .read(source_path)
        .write(temp_dir)
        .network(plugin_sandbox.network);

    if plugin_sandbox.requires_home_access
        && let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from)
    {
        // Read-only: helpers read runtime profiles from `$HOME`, but a confined
        // process must never be able to write across the user's whole home.
        builder = builder.read(&home);
    }

    builder.build()
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

        let fields = PluginSandboxFields {
            network: false,
            requires_home_access: false,
        };
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

        let fields = PluginSandboxFields {
            network: true,
            requires_home_access: false,
        };
        let policy = policy_for_plugin(&fields, &plugin_dir, &source, &work);
        assert!(policy.network);
    }
}
