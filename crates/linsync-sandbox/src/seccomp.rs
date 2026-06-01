//! seccomp-bpf filter installation.
//!
//! Applied in the child after fork() via pre_exec. Uses only async-signal-safe
//! operations: prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, ...) via seccompiler.
//!
//! The filter is compiled in the parent (build_seccomp_filter) and applied in
//! the child (apply_seccomp_filter) to avoid any allocation after fork.

use seccompiler::{
    BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
    SeccompRule,
};
use std::collections::BTreeMap;

/// Compile the seccomp BPF program. Call this in the parent before fork.
///
/// When `allow_network` is false, socket(AF_INET/AF_INET6/AF_NETLINK, ...) returns
/// EACCES. AF_UNIX is always permitted. The credential-changing syscalls
/// (setuid, setgid, setreuid, setresuid, setfsuid, setregid, setresgid,
/// setfsgid, setgroups), ptrace, and kernel module syscalls are blocked
/// unconditionally.
pub fn build_seccomp_filter(allow_network: bool) -> Result<BpfProgram, seccompiler::Error> {
    // Rules map: syscall_nr -> list of conditions that MATCH the block action.
    // A rule matches if all its conditions match (AND within a rule).
    // Multiple rules for the same syscall are OR'd together.
    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();

    // Unconditionally blocked syscalls: an empty Vec<SeccompRule> means the
    // syscall always matches (regardless of arguments), so the match_action fires.
    for &nr in always_blocked_syscalls() {
        rules.insert(nr, vec![]);
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
                        0, // arg index: domain
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

    let arch = std::env::consts::ARCH
        .try_into()
        .unwrap_or(seccompiler::TargetArch::x86_64);

    // SeccompFilter::new(rules, mismatch_action, match_action, arch)
    // mismatch_action: what to do when no rule matches — Allow everything else.
    // match_action:    what to do when a rule matches — return EACCES.
    let filter = SeccompFilter::new(
        rules,
        SeccompAction::Allow,
        SeccompAction::Errno(libc::EACCES as u32),
        arch,
    )?;

    // TryFrom<SeccompFilter> for BpfProgram returns a BackendError, which we
    // wrap into seccompiler::Error via the From impl.
    use std::convert::TryInto;
    let prog: BpfProgram = filter.try_into().map_err(seccompiler::Error::Backend)?;
    Ok(prog)
}

/// Syscalls blocked regardless of arguments or network policy: every common
/// credential-changing variant (so a confined helper cannot alter its UID/GID
/// set — privilege escalation), ptrace, and kernel module (un)loading.
fn always_blocked_syscalls() -> &'static [i64] {
    &[
        libc::SYS_setuid,
        libc::SYS_setgid,
        libc::SYS_setreuid,
        libc::SYS_setresuid,
        libc::SYS_setfsuid,
        libc::SYS_setregid,
        libc::SYS_setresgid,
        libc::SYS_setfsgid,
        libc::SYS_setgroups,
        libc::SYS_ptrace,
        libc::SYS_init_module,
        libc::SYS_finit_module,
        libc::SYS_delete_module,
    ]
}

/// Apply a pre-compiled BPF program via prctl(PR_SET_SECCOMP, ...).
/// Async-signal-safe — safe to call from pre_exec.
pub fn apply_seccomp_filter(program: &BpfProgram) -> std::io::Result<()> {
    seccompiler::apply_filter(program).map_err(|e| std::io::Error::other(format!("{e:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every credential-changing variant must be on the unconditional block
    /// list — not just setuid/setgid (defense-in-depth against priv-esc).
    #[test]
    fn credential_syscalls_are_all_blocked() {
        let blocked = always_blocked_syscalls();
        for nr in [
            libc::SYS_setuid,
            libc::SYS_setgid,
            libc::SYS_setreuid,
            libc::SYS_setresuid,
            libc::SYS_setfsuid,
            libc::SYS_setregid,
            libc::SYS_setresgid,
            libc::SYS_setfsgid,
            libc::SYS_setgroups,
        ] {
            assert!(
                blocked.contains(&nr),
                "credential syscall {nr} must be unconditionally blocked"
            );
        }
    }

    /// The filter compiles for both network policies, proving every blocked
    /// syscall constant resolves on the target arch.
    #[test]
    fn filter_compiles_for_both_network_policies() {
        assert!(build_seccomp_filter(false).is_ok());
        assert!(build_seccomp_filter(true).is_ok());
    }
}
