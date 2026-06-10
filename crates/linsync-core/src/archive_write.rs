// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

//! Writable archive-member editing: single-member zip repack.
//!
//! Implements the core half of `docs/archive-write-safety-design.md` (v1:
//! built-in zip only — no plugin `repack_member`, which is deferred on sandbox
//! API support). The flow is two-phase:
//!
//! 1. [`extract_member_for_edit`] validates the member path (zip-slip rules,
//!    encoding round-trip, symlink rejection), enforces size/ratio caps from
//!    the zip central directory, extracts the single member via sandboxed
//!    `unzip` into the caller's staging root, and captures a freshness
//!    fingerprint (size + ns-mtime + SHA-256) — all under a shared
//!    `flock(LOCK_SH)` on the archive so the fingerprint provably matches the
//!    extracted bytes.
//! 2. [`commit_member_edit`] performs the atomic publish under a non-blocking
//!    `flock(LOCK_EX)`: re-verifies the fingerprint, copies the original to a
//!    working copy, lets a sandboxed `zip` replace the one member in that
//!    working copy, asserts via a sandboxed listing that the member count is
//!    unchanged and the target appears exactly once, then writes the verified
//!    working copy to a sibling `<archive>.linsync-tmp`, fsyncs, creates an
//!    `<archive>.bak` (hard link, copy fallback), and publishes with a single
//!    atomic `rename(2)`. The original is never opened for writing; a failure
//!    at any pre-rename step leaves it byte-identical.
//!
//! The sandbox policy for the helper processes is built directly with
//! `SandboxPolicy::builder()` (like the built-in `unzip`/`tar` extraction in
//! `linsync-cli archive`): the design's upper bound on write grants is the
//! staging dir plus the `.linsync-tmp` working copy. The implementation is
//! tighter still: Info-ZIP `zip` publishes its result by re-creating the
//! output file (unlink + create), which a Landlock *file*-path grant on
//! `.linsync-tmp` cannot authorize (creation rights live on the parent
//! directory, which must never be granted). So the helper's working copy
//! lives *inside the staging dir* — the helpers get no grant anywhere near
//! the archive's directory at all — and the trusted host alone creates the
//! `.linsync-tmp` sibling from the verified working copy and performs the
//! atomic publish, exactly as design §3/§4 require of the host.

use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Default cap on a member's uncompressed size (design §2: archive-bomb row).
pub const DEFAULT_MAX_MEMBER_SIZE: u64 = 1024 * 1024 * 1024; // 1 GiB
/// Default cap on the compressed:uncompressed expansion ratio.
pub const DEFAULT_MAX_COMPRESSION_RATIO: u64 = 200;

/// Sanity cap on the zip central directory size the host will parse.
const MAX_CENTRAL_DIRECTORY_SIZE: u64 = 64 * 1024 * 1024;
/// Bound on captured helper stderr, for error messages.
const STDERR_CAP: usize = 64 * 1024;

/// Extraction caps enforced on the staging copy (design defaults; both the
/// central-directory declared sizes and the actual bytes written are checked).
#[derive(Debug, Clone, Copy)]
pub struct ArchiveEditCaps {
    /// Maximum uncompressed member size in bytes.
    pub max_member_size: u64,
    /// Maximum uncompressed:compressed ratio.
    pub max_compression_ratio: u64,
}

impl Default for ArchiveEditCaps {
    fn default() -> Self {
        Self {
            max_member_size: DEFAULT_MAX_MEMBER_SIZE,
            max_compression_ratio: DEFAULT_MAX_COMPRESSION_RATIO,
        }
    }
}

/// `uncompressed : compressed` strictly exceeds `max_ratio`, without the
/// truncation of integer division (200.99:1 must exceed a 200:1 cap) and
/// overflow-safe. A declared compressed size of zero with any uncompressed
/// bytes is a lie (stored members have equal sizes) and always exceeds.
fn exceeds_ratio(uncompressed: u64, compressed: u64, max_ratio: u64) -> bool {
    match compressed.checked_mul(max_ratio) {
        Some(limit) => uncompressed > limit,
        // The limit exceeds u64::MAX; nothing can exceed it.
        None => false,
    }
}

/// Typed failure surface for archive-member editing. Variants are distinct so
/// the bridge can map them onto HTTP statuses:
///
/// - 400: [`InvalidMemberName`](Self::InvalidMemberName),
///   [`MemberNameEncoding`](Self::MemberNameEncoding),
///   [`NonRegularMember`](Self::NonRegularMember),
///   [`NonRegularStagedFile`](Self::NonRegularStagedFile),
///   [`CapsExceeded`](Self::CapsExceeded),
///   [`UnsupportedArchive`](Self::UnsupportedArchive)
/// - 404: [`ArchiveNotFound`](Self::ArchiveNotFound),
///   [`MemberNotFound`](Self::MemberNotFound)
/// - 409: [`StaleArchive`](Self::StaleArchive) (token must be invalidated),
///   [`LockContention`](Self::LockContention) (retry later)
/// - 500: [`ExtractFailed`](Self::ExtractFailed),
///   [`RepackFailed`](Self::RepackFailed),
///   [`PostRepackAssertion`](Self::PostRepackAssertion),
///   [`RenameFailed`](Self::RenameFailed), [`Io`](Self::Io)
#[derive(Debug)]
pub enum ArchiveWriteError {
    /// The member path failed zip-slip / argument-injection validation.
    InvalidMemberName {
        member: String,
        reason: &'static str,
    },
    /// The member's stored name does not round-trip UTF-8 (cp437 legacy entry
    /// or missing UTF-8 flag on a non-ASCII name); editing it would risk
    /// Info-ZIP adding a second entry instead of replacing.
    MemberNameEncoding { member: String },
    /// The member is a symlink, directory, or other non-regular entry.
    NonRegularMember { member: String },
    /// The staged file was replaced by a symlink or non-regular file.
    NonRegularStagedFile { staged: PathBuf },
    /// Declared or actual size/ratio exceeds the extraction caps.
    CapsExceeded { member: String, detail: String },
    /// Not a parseable zip archive (or zip64, unsupported in v1).
    UnsupportedArchive { detail: String },
    /// The archive path does not exist or is not a regular file.
    ArchiveNotFound { archive: PathBuf },
    /// The member is not present in the archive.
    MemberNotFound { member: String },
    /// The archive changed since the edit fingerprint was captured.
    StaleArchive { archive: PathBuf },
    /// Another commit holds the exclusive lock on this archive.
    LockContention { archive: PathBuf },
    /// The sandboxed `unzip` extraction failed.
    ExtractFailed { detail: String },
    /// The sandboxed `zip` repack of the working copy failed.
    RepackFailed { detail: String },
    /// The post-repack listing assertion failed (member count changed or the
    /// target member does not appear exactly once); the commit was aborted
    /// and the original left untouched.
    PostRepackAssertion { detail: String },
    /// The final atomic `rename(2)` failed. The original is untouched; the
    /// working copy and backup named here are retained for retry.
    RenameFailed {
        archive: PathBuf,
        tmp: PathBuf,
        bak: PathBuf,
        detail: String,
    },
    /// Portal-granted archive is read-only; the original is untouched. The
    /// user's edited member is retained at `staged`; `backup` (when present)
    /// is the app-private copy of the *original, unedited* archive.
    PortalReadOnly {
        archive: PathBuf,
        backup: Option<PathBuf>,
        staged: PathBuf,
    },
    /// An I/O step failed; the original archive is untouched.
    Io { context: String, source: io::Error },
}

impl std::fmt::Display for ArchiveWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMemberName { member, reason } => {
                write!(f, "invalid member path '{member}': {reason}")
            }
            Self::MemberNameEncoding { member } => write!(
                f,
                "member name encoding not supported for editing: '{member}'"
            ),
            Self::NonRegularMember { member } => write!(
                f,
                "member '{member}' is not a regular file (symlink/directory/special)"
            ),
            Self::NonRegularStagedFile { staged } => write!(
                f,
                "staged file '{}' is not a regular file",
                staged.display()
            ),
            Self::CapsExceeded { member, detail } => {
                write!(f, "member '{member}' exceeds extraction caps: {detail}")
            }
            Self::UnsupportedArchive { detail } => write!(f, "unsupported archive: {detail}"),
            Self::ArchiveNotFound { archive } => {
                write!(f, "archive '{}' not found", archive.display())
            }
            Self::MemberNotFound { member } => {
                write!(f, "member '{member}' not found in archive")
            }
            Self::StaleArchive { archive } => write!(
                f,
                "archive '{}' changed since the edit was started; original untouched, re-extract to edit",
                archive.display()
            ),
            Self::LockContention { archive } => write!(
                f,
                "archive '{}' is locked by another commit; retry later",
                archive.display()
            ),
            Self::ExtractFailed { detail } => write!(f, "member extraction failed: {detail}"),
            Self::RepackFailed { detail } => {
                write!(f, "repack failed; original archive untouched: {detail}")
            }
            Self::PostRepackAssertion { detail } => write!(
                f,
                "post-repack verification failed; original archive untouched: {detail}"
            ),
            Self::RenameFailed {
                archive,
                tmp,
                bak,
                detail,
            } => write!(
                f,
                "atomic publish of '{}' failed ({detail}); original untouched, working copy retained at '{}' and backup at '{}'",
                archive.display(),
                tmp.display(),
                bak.display()
            ),
            Self::PortalReadOnly {
                archive,
                backup,
                staged,
            } => {
                write!(
                    f,
                    "portal-granted archive '{}' is read-only; original untouched, edited member retained at '{}'",
                    archive.display(),
                    staged.display()
                )?;
                if let Some(backup) = backup {
                    write!(
                        f,
                        " (backup of the original archive at '{}')",
                        backup.display()
                    )?;
                }
                Ok(())
            }
            Self::Io { context, source } => write!(f, "{context}: {source}"),
        }
    }
}

impl std::error::Error for ArchiveWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn io_err(context: impl Into<String>, source: io::Error) -> ArchiveWriteError {
    ArchiveWriteError::Io {
        context: context.into(),
        source,
    }
}

fn is_portal_path(path: &Path) -> bool {
    use std::path::Component;
    let mut comps = path.components();
    matches!(comps.next(), Some(Component::RootDir))
        && matches!(comps.next(), Some(Component::Normal(os)) if os == "run")
        && matches!(comps.next(), Some(Component::Normal(os)) if os == "user")
        && comps.next().is_some() // <uid>
        && matches!(comps.next(), Some(Component::Normal(os)) if os == "doc")
}

/// Freshness fingerprint of the whole archive (design §2: TOCTOU row).
#[derive(Debug, Clone, PartialEq, Eq)]
struct ArchiveFingerprint {
    size: u64,
    mtime_ns: i128,
    sha256: [u8; 32],
}

/// Server-side state for one outstanding member edit. Produced by
/// [`extract_member_for_edit`]; consumed by [`commit_member_edit`]. The bridge
/// holds this behind its opaque token — clients never supply paths to commit.
#[derive(Debug, Clone)]
pub struct MemberEditContext {
    archive: PathBuf,
    member: String,
    staging_root: PathBuf,
    extract_root: PathBuf,
    work_dir: PathBuf,
    staged_path: PathBuf,
    fingerprint: ArchiveFingerprint,
    /// Original member Unix mode (zip external attributes), restored onto the
    /// staged file before repack so the mode round-trips.
    member_mode: Option<u32>,
    /// Central-directory entry count at edit time, for the post-repack
    /// member-count assertion.
    member_count: usize,
    /// Whether the commit will use the atomic rename path (`true`) or degrade
    /// to a non-atomic O_TRUNC write for portal-granted archives (`false`).
    atomic: bool,
    /// App-private backup path for portal-granted archives; retained across
    /// commit so the original can be recovered.
    portal_backup: Option<PathBuf>,
}

impl MemberEditContext {
    /// Canonical path of the archive being edited.
    pub fn archive(&self) -> &Path {
        &self.archive
    }

    /// Member path within the archive, `/`-separated.
    pub fn member(&self) -> &str {
        &self.member
    }

    /// Root of the staging area created for this edit.
    pub fn staging_root(&self) -> &Path {
        &self.staging_root
    }

    /// The staged copy of the member, the file the user edits.
    pub fn staged_path(&self) -> &Path {
        &self.staged_path
    }

    /// Whether the commit uses the atomic rename path.
    pub fn atomic(&self) -> bool {
        self.atomic
    }

    /// The app-private backup path for portal-granted archives.
    pub fn portal_backup(&self) -> Option<&Path> {
        self.portal_backup.as_deref()
    }
}

/// Options for [`commit_member_edit`].
#[derive(Debug, Clone, Default)]
pub struct CommitOptions {
    /// Retain the `<archive>.bak` backup on success (the `keepArchiveBackup`
    /// setting; default `false` deletes it after a confirmed publish).
    pub keep_backup: bool,
}

/// Successful commit report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    /// The retained backup path when `keep_backup` was set.
    pub bak_path: Option<PathBuf>,
    /// Set when the publish succeeded but deleting the `.bak` failed; the
    /// stale backup path is reported for diagnostics (design §3 step 7).
    pub bak_cleanup_warning: Option<String>,
}

// ── Member path validation ──────────────────────────────────────────────────

/// Validate a member path against the zip-slip and argument-injection rules
/// (design §2 row 1): rejects empty paths, NUL/control bytes, absolute paths,
/// drive prefixes, backslashes, `.`/`..` components, empty components
/// (including trailing `/` directory names), leading `-` (option injection)
/// and `@` (Info-ZIP filelist specifier), and Info-ZIP glob metacharacters
/// (`*?[]` — `unzip`/`zip` treat member arguments as match patterns).
pub fn validate_member_path(member: &str) -> Result<(), ArchiveWriteError> {
    let reject = |reason: &'static str| {
        Err(ArchiveWriteError::InvalidMemberName {
            member: member.to_owned(),
            reason,
        })
    };
    if member.is_empty() {
        return reject("empty member path");
    }
    if member.bytes().any(|b| b == 0) {
        return reject("contains NUL byte");
    }
    if member.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return reject("contains control characters");
    }
    if member.starts_with('/') {
        return reject("absolute path");
    }
    if member.contains('\\') {
        return reject("contains backslash");
    }
    let bytes = member.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return reject("drive prefix");
    }
    if member.starts_with('-') {
        return reject("leading '-' could be parsed as an option");
    }
    if member.starts_with('@') {
        return reject("leading '@' could be parsed as an Info-ZIP filelist");
    }
    if member.contains(['*', '?', '[', ']']) {
        return reject("contains Info-ZIP wildcard characters");
    }
    for component in member.split('/') {
        match component {
            "" => return reject("empty path component (or trailing '/')"),
            "." | ".." => return reject("relative '.'/'..' path component"),
            _ => {}
        }
    }
    Ok(())
}

// ── Post-repack listing assertion ───────────────────────────────────────────

/// Assert that a post-repack member listing of the working copy has the same
/// entry count as the original and contains the target member exactly once
/// (design §5: the guard against Info-ZIP adding instead of replacing).
/// Names are compared as raw bytes (the listing is produced with
/// `zipinfo -1 -UU`, which prints stored name bytes verbatim). Exposed so the
/// guard can be tested directly on crafted listings.
pub fn verify_post_repack_listing(
    names: &[impl AsRef<[u8]>],
    member: &str,
    expected_count: usize,
) -> Result<(), ArchiveWriteError> {
    if names.len() != expected_count {
        return Err(ArchiveWriteError::PostRepackAssertion {
            detail: format!(
                "member count changed: expected {expected_count}, working copy has {}",
                names.len()
            ),
        });
    }
    let occurrences = names
        .iter()
        .filter(|name| name.as_ref() == member.as_bytes())
        .count();
    if occurrences != 1 {
        return Err(ArchiveWriteError::PostRepackAssertion {
            detail: format!(
                "member '{member}' appears {occurrences} times in the working copy, expected exactly once"
            ),
        });
    }
    Ok(())
}

// ── flock helpers ───────────────────────────────────────────────────────────

/// Take a non-blocking `flock` on the archive, absorbing transient
/// contention with a short bounded retry.
///
/// The retry exists because `fork(2)` duplicates the whole fd table: any
/// *other* helper this process spawns while an archive lock is held briefly
/// inherits the locked fd between fork and exec (the fd is CLOEXEC, but the
/// sandbox `pre_exec` setup widens that window). Such phantom holders clear
/// in milliseconds; a real concurrent commit holds its lock far longer and
/// still surfaces as [`ArchiveWriteError::LockContention`].
fn flock_nonblocking(
    file: &File,
    exclusive: bool,
    archive: &Path,
) -> Result<(), ArchiveWriteError> {
    let op = if exclusive {
        libc::LOCK_EX | libc::LOCK_NB
    } else {
        libc::LOCK_SH | libc::LOCK_NB
    };
    for attempt in 0..50u32 {
        // SAFETY: flock on a valid open fd.
        let rc = unsafe { libc::flock(file.as_raw_fd(), op) };
        if rc == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::WouldBlock {
            return Err(io_err(
                format!("flock on '{}' failed", archive.display()),
                err,
            ));
        }
        if attempt < 49 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    Err(ArchiveWriteError::LockContention {
        archive: archive.to_path_buf(),
    })
}

// ── Fingerprinting ──────────────────────────────────────────────────────────

fn mtime_ns(metadata: &fs::Metadata) -> i128 {
    i128::from(metadata.mtime()) * 1_000_000_000 + i128::from(metadata.mtime_nsec())
}

fn sha256_of(file: &mut File) -> Result<[u8; 32], ArchiveWriteError> {
    file.seek(SeekFrom::Start(0))
        .map_err(|e| io_err("seek for hashing failed", e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| io_err("read for hashing failed", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().into())
}

// ── Zip central directory parsing ───────────────────────────────────────────

struct CdEntry {
    name: Vec<u8>,
    utf8_flag: bool,
    compressed_size: u64,
    uncompressed_size: u64,
    /// `Some(mode)` when the entry was made on Unix (external attrs >> 16).
    unix_mode: Option<u32>,
}

fn unsupported(detail: impl Into<String>) -> ArchiveWriteError {
    ArchiveWriteError::UnsupportedArchive {
        detail: detail.into(),
    }
}

fn read_u16(buf: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([buf[at], buf[at + 1]])
}

fn read_u32(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]])
}

/// Parse the zip central directory. Read-only, host-side: the design checks
/// caps "against the zip central-directory sizes before extraction", and the
/// same parse supplies the symlink/encoding/entry-count facts.
fn parse_central_directory(
    file: &mut File,
    file_size: u64,
) -> Result<Vec<CdEntry>, ArchiveWriteError> {
    const EOCD_SIG: u32 = 0x0605_4b50;
    const EOCD_LEN: u64 = 22;
    const CD_SIG: u32 = 0x0201_4b50;
    const CD_FIXED_LEN: usize = 46;

    if file_size < EOCD_LEN {
        return Err(unsupported("file too small to be a zip archive"));
    }
    // The EOCD record is in the last 22 + 65535 bytes (max comment length).
    let tail_len = file_size.min(EOCD_LEN + 65_535);
    let tail_start = file_size - tail_len;
    file.seek(SeekFrom::Start(tail_start))
        .map_err(|e| io_err("seek to zip tail failed", e))?;
    let mut tail = vec![0u8; tail_len as usize];
    file.read_exact(&mut tail)
        .map_err(|e| io_err("read of zip tail failed", e))?;

    let mut eocd_at = None;
    for at in (0..=(tail.len() - EOCD_LEN as usize)).rev() {
        if read_u32(&tail, at) == EOCD_SIG {
            let comment_len = read_u16(&tail, at + 20) as usize;
            if at + EOCD_LEN as usize + comment_len == tail.len() {
                eocd_at = Some(at);
                break;
            }
        }
    }
    let Some(eocd_at) = eocd_at else {
        return Err(unsupported(
            "no end-of-central-directory record (not a zip archive?)",
        ));
    };
    let total_entries = read_u16(&tail, eocd_at + 10) as u64;
    let cd_size = u64::from(read_u32(&tail, eocd_at + 12));
    let cd_offset = u64::from(read_u32(&tail, eocd_at + 16));
    if total_entries == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_offset == 0xFFFF_FFFF {
        return Err(unsupported("zip64 archives are not supported for editing"));
    }
    if cd_size > MAX_CENTRAL_DIRECTORY_SIZE {
        return Err(unsupported("central directory exceeds the parse cap"));
    }
    if cd_offset
        .checked_add(cd_size)
        .is_none_or(|end| end > file_size)
    {
        return Err(unsupported("central directory extends past end of file"));
    }

    file.seek(SeekFrom::Start(cd_offset))
        .map_err(|e| io_err("seek to central directory failed", e))?;
    let mut cd = vec![0u8; cd_size as usize];
    file.read_exact(&mut cd)
        .map_err(|e| io_err("read of central directory failed", e))?;

    let mut entries = Vec::with_capacity(total_entries as usize);
    let mut at = 0usize;
    for _ in 0..total_entries {
        if at + CD_FIXED_LEN > cd.len() || read_u32(&cd, at) != CD_SIG {
            return Err(unsupported("malformed central directory entry"));
        }
        let made_by_os = cd[at + 5]; // high byte of "version made by"
        let flags = read_u16(&cd, at + 8);
        let compressed_size = u64::from(read_u32(&cd, at + 20));
        let uncompressed_size = u64::from(read_u32(&cd, at + 24));
        let name_len = read_u16(&cd, at + 28) as usize;
        let extra_len = read_u16(&cd, at + 30) as usize;
        let comment_len = read_u16(&cd, at + 32) as usize;
        let external_attrs = read_u32(&cd, at + 38);
        let name_start = at + CD_FIXED_LEN;
        let entry_end = name_start + name_len + extra_len + comment_len;
        if entry_end > cd.len() {
            return Err(unsupported("central directory entry overruns directory"));
        }
        entries.push(CdEntry {
            name: cd[name_start..name_start + name_len].to_vec(),
            utf8_flag: flags & 0x0800 != 0,
            compressed_size,
            uncompressed_size,
            unix_mode: (made_by_os == 3).then_some(external_attrs >> 16),
        });
        at = entry_end;
    }
    Ok(entries)
}

// ── Sandboxed helper execution ──────────────────────────────────────────────

/// Read/write grants for one sandboxed helper invocation. Translated into a
/// `SandboxPolicy` built directly with `SandboxPolicy::builder()`, mirroring
/// the built-in `unzip`/`tar` extraction in `linsync-cli archive` (and the
/// same `LINSYNC_SANDBOX_SKIP` escape used by `just test`).
struct SandboxGrants {
    read: Vec<PathBuf>,
    write: Vec<PathBuf>,
}

#[cfg(feature = "sandbox")]
fn spawn_confined(cmd: Command, grants: &SandboxGrants) -> io::Result<std::process::Child> {
    use linsync_sandbox::{SandboxPolicy, SandboxedCommand};
    let mut builder = SandboxPolicy::builder();
    for path in &grants.read {
        builder = builder.read(path);
    }
    for path in &grants.write {
        builder = builder.read(path).write(path);
    }
    SandboxedCommand::new(cmd, builder.build())
        .spawn()
        .map_err(io::Error::other)
}

#[cfg(not(feature = "sandbox"))]
fn spawn_confined(_cmd: Command, _grants: &SandboxGrants) -> io::Result<std::process::Child> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "archive write requires the 'sandbox' feature to confine helper processes",
    ))
}

/// Default wall-clock timeout for sandboxed archive helpers.
const HELPER_TIMEOUT_MS: u64 = 60_000;

/// Run a helper under the sandbox grants, capturing bounded stdout/stderr.
/// Kills the child if it does not finish within [`HELPER_TIMEOUT_MS`].
fn run_helper(
    mut cmd: Command,
    grants: &SandboxGrants,
    what: &'static str,
) -> Result<(bool, Vec<u8>, String), ArchiveWriteError> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child =
        spawn_confined(cmd, grants).map_err(|e| io_err(format!("spawning {what} failed"), e))?;

    // Drain both pipes on background threads while polling for exit. A child
    // whose output exceeds the kernel pipe buffer (e.g. `unzip -Z` listing a
    // many-member archive) blocks on write(2) until someone reads; draining
    // only after exit would deadlock against that write until the timeout.
    let drain = |pipe: Option<Box<dyn io::Read + Send>>| {
        pipe.map(|mut r| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = io::copy(&mut r, &mut buf);
                buf
            })
        })
    };
    let stdout_reader = drain(
        child
            .stdout
            .take()
            .map(|r| Box::new(r) as Box<dyn io::Read + Send>),
    );
    let stderr_reader = drain(
        child
            .stderr
            .take()
            .map(|r| Box::new(r) as Box<dyn io::Read + Send>),
    );

    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(HELPER_TIMEOUT_MS);
    let status = loop {
        match child
            .try_wait()
            .map_err(|e| io_err(format!("waiting for {what} failed"), e))?
        {
            Some(status) => break status,
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(io_err(
                        format!("{what} exceeded {HELPER_TIMEOUT_MS}s timeout"),
                        io::Error::new(io::ErrorKind::TimedOut, "helper timeout"),
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    };
    let collect = |handle: Option<std::thread::JoinHandle<Vec<u8>>>| {
        handle.and_then(|h| h.join().ok()).unwrap_or_default()
    };
    let stdout = collect(stdout_reader);
    let mut stderr = collect(stderr_reader);
    stderr.truncate(STDERR_CAP);
    Ok((
        status.success(),
        stdout,
        String::from_utf8_lossy(&stderr).into_owned(),
    ))
}

// ── Edit-time extraction ────────────────────────────────────────────────────

/// Extract `member` from the zip `archive` into `staging_root` for editing,
/// with the default size/ratio caps. See [`extract_member_for_edit_with_caps`].
pub fn extract_member_for_edit(
    archive: &Path,
    member: &str,
    staging_root: &Path,
    portal_backup: Option<&Path>,
) -> Result<MemberEditContext, ArchiveWriteError> {
    extract_member_for_edit_with_caps(
        archive,
        member,
        staging_root,
        &ArchiveEditCaps::default(),
        portal_backup,
    )
}

/// Extract `member` from the zip `archive` into `staging_root` for editing.
///
/// Validates the member path ([`validate_member_path`]), parses the central
/// directory to reject symlink/non-regular members, names that do not
/// round-trip UTF-8, and members exceeding `caps`; extracts the single member
/// via sandboxed `unzip`; and captures the freshness fingerprint — all under
/// a shared `flock(LOCK_SH)` on the archive. Nothing is staged on rejection.
pub fn extract_member_for_edit_with_caps(
    archive: &Path,
    member: &str,
    staging_root: &Path,
    caps: &ArchiveEditCaps,
    portal_backup: Option<&Path>,
) -> Result<MemberEditContext, ArchiveWriteError> {
    validate_member_path(member)?;

    let archive = archive
        .canonicalize()
        .map_err(|_| ArchiveWriteError::ArchiveNotFound {
            archive: archive.to_path_buf(),
        })?;
    let atomic = !is_portal_path(&archive);
    let portal_backup = if atomic {
        None
    } else {
        // Portal commits degrade to a non-atomic O_TRUNC write; the backup is
        // the only recovery path from a partial write (design §7), so refuse
        // to start an edit without one rather than discover it at commit.
        if portal_backup.is_none() {
            return Err(io_err(
                format!(
                    "portal-granted archive '{}' requires a portal backup path",
                    archive.display()
                ),
                io::Error::new(io::ErrorKind::InvalidInput, "missing portal_backup"),
            ));
        }
        portal_backup.map(|p| p.to_path_buf())
    };
    let metadata =
        fs::symlink_metadata(&archive).map_err(|_| ArchiveWriteError::ArchiveNotFound {
            archive: archive.clone(),
        })?;
    if !metadata.is_file() {
        return Err(ArchiveWriteError::ArchiveNotFound { archive });
    }

    // Shared lock for the whole extract+fingerprint window (design §2 TOCTOU
    // row): held for as long as `file` is open.
    let mut file = File::open(&archive)
        .map_err(|e| io_err(format!("opening '{}' failed", archive.display()), e))?;
    flock_nonblocking(&file, false, &archive)?;
    let metadata = file
        .metadata()
        .map_err(|e| io_err("stat of archive failed", e))?;

    let entries = parse_central_directory(&mut file, metadata.len())?;
    let member_count = entries.len();

    // Locate the member by exact stored-name bytes; diagnose encoding
    // problems (design §5 edit-time guard) before reporting "not found".
    let exact: Vec<&CdEntry> = entries
        .iter()
        .filter(|e| e.name.as_slice() == member.as_bytes())
        .collect();
    let entry = match exact.as_slice() {
        [entry] => *entry,
        [] => {
            let lossy_match = entries
                .iter()
                .any(|e| String::from_utf8_lossy(&e.name) == member);
            if lossy_match {
                // The stored bytes are not valid UTF-8; the caller is holding
                // a lossily decoded name `zip` could never address.
                return Err(ArchiveWriteError::MemberNameEncoding {
                    member: member.to_owned(),
                });
            }
            return Err(ArchiveWriteError::MemberNotFound {
                member: member.to_owned(),
            });
        }
        _ => {
            return Err(unsupported(format!(
                "member '{member}' appears more than once in the archive"
            )));
        }
    };
    // Non-ASCII name without the UTF-8 flag: Info-ZIP re-encodes such names
    // (cp437 interpretation) and would *add* a second entry on repack.
    if !entry.utf8_flag && !member.is_ascii() {
        return Err(ArchiveWriteError::MemberNameEncoding {
            member: member.to_owned(),
        });
    }
    if member.ends_with('/') {
        // Unreachable after validate_member_path, but keep the invariant local.
        return Err(ArchiveWriteError::NonRegularMember {
            member: member.to_owned(),
        });
    }
    if let Some(mode) = entry.unix_mode {
        let fmt = mode & libc::S_IFMT;
        if fmt != 0 && fmt != libc::S_IFREG {
            return Err(ArchiveWriteError::NonRegularMember {
                member: member.to_owned(),
            });
        }
    }

    // Caps against the declared central-directory sizes (design §2 bomb row).
    if entry.compressed_size == 0xFFFF_FFFF || entry.uncompressed_size == 0xFFFF_FFFF {
        return Err(unsupported(
            "zip64 member sizes are not supported for editing",
        ));
    }
    // A declared compressed size larger than the archive itself is physically
    // impossible — an inflated value would defeat the ratio check below by
    // shrinking the apparent expansion, so reject it outright.
    if entry.compressed_size > metadata.len() {
        return Err(unsupported(
            "declared compressed size exceeds the archive size",
        ));
    }
    if entry.uncompressed_size > caps.max_member_size {
        return Err(ArchiveWriteError::CapsExceeded {
            member: member.to_owned(),
            detail: format!(
                "declared size {} exceeds the {} byte cap",
                entry.uncompressed_size, caps.max_member_size
            ),
        });
    }
    if exceeds_ratio(
        entry.uncompressed_size,
        entry.compressed_size,
        caps.max_compression_ratio,
    ) {
        return Err(ArchiveWriteError::CapsExceeded {
            member: member.to_owned(),
            detail: format!(
                "declared compression ratio {}:{} exceeds the {}:1 cap",
                entry.uncompressed_size, entry.compressed_size, caps.max_compression_ratio
            ),
        });
    }
    let member_mode = entry.unix_mode.map(|m| m & 0o7777).filter(|m| *m != 0);
    let compressed_size = entry.compressed_size;

    // Fingerprint the exact bytes the extraction below will read.
    let fingerprint = ArchiveFingerprint {
        size: metadata.len(),
        mtime_ns: mtime_ns(&metadata),
        sha256: sha256_of(&mut file)?,
    };

    // Stage: only now do we create anything on disk.
    let extract_root = staging_root.join("extract");
    let work_dir = staging_root.join("work");
    let staged_path = extract_root.join(member);
    // Design §2: prefix-check the staged path against the staging root.
    // `validate_member_path` already makes escape impossible (no `..`, no
    // absolute components); this asserts the invariant explicitly.
    if !staged_path.starts_with(&extract_root) {
        return Err(ArchiveWriteError::InvalidMemberName {
            member: member.to_owned(),
            reason: "staged path escapes the staging root",
        });
    }
    let cleanup_staging = || {
        let _ = fs::remove_dir_all(staging_root);
    };
    fs::create_dir_all(&extract_root)
        .and_then(|()| fs::create_dir_all(&work_dir))
        .map_err(|e| io_err("creating staging dirs failed", e))?;

    let mut cmd = Command::new("unzip");
    cmd.arg("-q")
        .arg("-o")
        .arg("-d")
        .arg(&extract_root)
        .arg(&archive)
        .arg(member);
    let grants = SandboxGrants {
        read: vec![archive.clone()],
        write: vec![staging_root.to_path_buf()],
    };
    let (ok, _stdout, stderr) = run_helper(cmd, &grants, "unzip").inspect_err(|_| {
        cleanup_staging();
    })?;
    if !ok {
        cleanup_staging();
        return Err(ArchiveWriteError::ExtractFailed {
            detail: format!("unzip failed: {}", stderr.trim()),
        });
    }

    // Re-check the actual bytes written against the caps and require a
    // regular file at exactly the expected path.
    let staged_meta = match fs::symlink_metadata(&staged_path) {
        Ok(meta) => meta,
        Err(e) => {
            cleanup_staging();
            return Err(io_err("stat of staged member failed", e));
        }
    };
    if !staged_meta.is_file() {
        cleanup_staging();
        return Err(ArchiveWriteError::NonRegularMember {
            member: member.to_owned(),
        });
    }
    if staged_meta.len() > caps.max_member_size
        || exceeds_ratio(
            staged_meta.len(),
            compressed_size,
            caps.max_compression_ratio,
        )
    {
        cleanup_staging();
        return Err(ArchiveWriteError::CapsExceeded {
            member: member.to_owned(),
            detail: format!(
                "actual extracted size {} violates the caps (declared sizes lied)",
                staged_meta.len()
            ),
        });
    }

    if !atomic && let Some(ref bak) = portal_backup {
        if let Some(parent) = bak.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| io_err("creating portal backup parent dir failed", e))?;
        }
        let mut src = File::open(&archive)
            .map_err(|e| io_err("opening archive for portal backup copy failed", e))?;
        let mut dst = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(bak)
            .map_err(|e| io_err("creating portal backup file failed", e))?;
        io::copy(&mut src, &mut dst)
            .map_err(|e| io_err("copying archive to portal backup failed", e))?;
    }

    Ok(MemberEditContext {
        archive,
        member: member.to_owned(),
        staging_root: staging_root.to_path_buf(),
        extract_root,
        work_dir,
        staged_path,
        fingerprint,
        member_mode,
        member_count,
        atomic,
        portal_backup,
    })
    // `file` drops here, releasing LOCK_SH.
}

// ── Commit ──────────────────────────────────────────────────────────────────

fn tmp_path_for(archive: &Path) -> PathBuf {
    let mut name = archive.file_name().unwrap_or_default().to_os_string();
    name.push(".linsync-tmp");
    archive.with_file_name(name)
}

fn bak_path_for(archive: &Path) -> PathBuf {
    let mut name = archive.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    archive.with_file_name(name)
}

/// Resolve a `link(2)` attempt on the `.bak` backup: filesystems without hard
/// links (FAT32/exFAT, many FUSE mounts) fail with `EXDEV`/`EOPNOTSUPP` (and
/// kin); the backup then degrades to a byte copy, never a silent skip
/// (design §3 step 5). Any other error propagates. Split out so the
/// degradation logic is directly testable without a hard-link-less mount.
fn create_backup_from_link_result(
    link_result: io::Result<()>,
    archive: &Path,
    bak: &Path,
) -> io::Result<()> {
    match link_result {
        Ok(()) => Ok(()),
        Err(err)
            if matches!(
                err.raw_os_error(),
                Some(libc::EXDEV) | Some(libc::EOPNOTSUPP) | Some(libc::EPERM) | Some(libc::EMLINK)
            ) =>
        {
            fs::copy(archive, bak).map(|_| ())
        }
        Err(err) => Err(err),
    }
}

/// Atomically publish an edited member back into its archive (design §3).
///
/// Under a non-blocking `flock(LOCK_EX)` on the original: re-verifies the
/// freshness fingerprint (cheap size+mtime first, full SHA-256 after the
/// repack), requires the staged file to still be a regular file, restores the
/// member's original mode onto it, copies the original to a working copy in
/// the staging dir, runs sandboxed `zip` (staging-only write grants) to
/// replace the single member in that working copy, asserts the post-repack
/// listing (member count unchanged, target exactly once), then host-writes
/// the verified working copy to `<archive>.linsync-tmp` with the original's
/// mode/ownership, fsyncs, hard-links the original to `<archive>.bak` (copy
/// fallback on filesystems without hard links), and renames the tmp over the
/// original. The original is never opened for writing; every failure before
/// the rename leaves it byte-identical.
pub fn commit_member_edit(
    ctx: &MemberEditContext,
    options: &CommitOptions,
) -> Result<CommitOutcome, ArchiveWriteError> {
    let archive = &ctx.archive;
    let mut original = File::open(archive).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            ArchiveWriteError::StaleArchive {
                archive: archive.clone(),
            }
        } else {
            io_err(format!("opening '{}' failed", archive.display()), e)
        }
    })?;
    // Exclusive, non-blocking: contention with another LinSync commit → 409.
    flock_nonblocking(&original, true, archive)?;
    let orig_meta = original
        .metadata()
        .map_err(|e| io_err("stat of archive failed", e))?;

    // Cheap freshness fields first (design §2 TOCTOU row).
    if orig_meta.len() != ctx.fingerprint.size || mtime_ns(&orig_meta) != ctx.fingerprint.mtime_ns {
        return Err(ArchiveWriteError::StaleArchive {
            archive: archive.clone(),
        });
    }

    // The staged file must still be a regular file (design §2 symlink-as-
    // repack-content row): lstat, never stat.
    let staged_meta = fs::symlink_metadata(&ctx.staged_path)
        .map_err(|e| io_err("stat of staged file failed", e))?;
    if !staged_meta.is_file() {
        return Err(ArchiveWriteError::NonRegularStagedFile {
            staged: ctx.staged_path.clone(),
        });
    }
    // Restore the member's original mode so it round-trips the repack
    // (design §2 mode-loss row).
    if let Some(mode) = ctx.member_mode {
        fs::set_permissions(&ctx.staged_path, fs::Permissions::from_mode(mode))
            .map_err(|e| io_err("restoring member mode on staged file failed", e))?;
    }

    // §3 step 1 (helper side): working copy in the staging dir. The helper
    // never receives a grant near the archive's directory; the host alone
    // writes there (see module docs on why `zip` cannot update a
    // Landlock-file-granted `.linsync-tmp` in place).
    let work_copy = ctx.work_dir.join("repack.zip");
    {
        original
            .seek(SeekFrom::Start(0))
            .map_err(|e| io_err("seek of original failed", e))?;
        let mut work_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&work_copy)
            .map_err(|e| {
                io_err(
                    format!("creating working copy '{}' failed", work_copy.display()),
                    e,
                )
            })?;
        io::copy(&mut original, &mut work_file)
            .map_err(|e| io_err("copying original to working copy failed", e))?;
    }

    // §3 step 3: sandboxed `zip` replaces the one member in the working copy.
    // cwd is the staging extract root so the member's relative path matches
    // its archive path; `-b` keeps zip's own temp files inside the staging
    // grant. The original archive is not in the policy at all.
    let mut cmd = Command::new("zip");
    cmd.arg("-q")
        .arg("-b")
        .arg(&ctx.work_dir)
        .arg(&work_copy)
        .arg(&ctx.member)
        .current_dir(&ctx.extract_root);
    let grants = SandboxGrants {
        read: Vec::new(),
        write: vec![ctx.staging_root.clone()],
    };
    let (ok, _stdout, stderr) = run_helper(cmd, &grants, "zip")?;
    if !ok {
        return Err(ArchiveWriteError::RepackFailed {
            detail: format!("zip failed: {}", stderr.trim()),
        });
    }

    // Post-repack assertion (design §5): list the working copy under the same
    // sandbox policy; member count unchanged, target exactly once. `-UU`
    // makes zipinfo print stored name bytes verbatim so the comparison is
    // byte-exact; `-1` prints names only, one per line.
    let mut cmd = Command::new("unzip");
    cmd.arg("-Z").arg("-1").arg("-UU").arg(&work_copy);
    let grants = SandboxGrants {
        read: vec![ctx.staging_root.clone()],
        write: Vec::new(),
    };
    let (ok, stdout, stderr) = run_helper(cmd, &grants, "unzip -Z")?;
    if !ok {
        return Err(ArchiveWriteError::PostRepackAssertion {
            detail: format!("listing the working copy failed: {}", stderr.trim()),
        });
    }
    // Compare raw name bytes (`-UU` prints stored bytes verbatim): a lossy
    // decode could let a non-UTF-8 sibling entry collide with a member name
    // containing U+FFFD.
    let names: Vec<&[u8]> = stdout
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    verify_post_repack_listing(&names, &ctx.member, ctx.member_count)?;

    // §3 step 4: full fingerprint re-verification under the commit lock.
    let sha = sha256_of(&mut original)?;
    if sha != ctx.fingerprint.sha256 {
        return Err(ArchiveWriteError::StaleArchive {
            archive: archive.clone(),
        });
    }

    if ctx.atomic {
        // §3 steps 1–2 + 5 (host side): publish the verified working copy as
        // `<archive>.linsync-tmp` in the same directory (same filesystem ⇒ the
        // final rename(2) is atomic), with the original's mode and best-effort
        // ownership, then fsync it.
        let tmp = tmp_path_for(archive);
        let remove_tmp = || {
            let _ = fs::remove_file(&tmp);
        };
        {
            let mut work_file = File::open(&work_copy)
                .map_err(|e| io_err("reopening working copy for publish failed", e))?;
            let mut tmp_opts = OpenOptions::new();
            tmp_opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                // O_NOFOLLOW: if an attacker planted a symlink at the predictable
                // tmp path, refuse to follow it and truncate the target.
                tmp_opts.custom_flags(libc::O_NOFOLLOW);
            }
            let mut tmp_file = tmp_opts.open(&tmp).map_err(|e| {
                io_err(
                    format!("creating working copy '{}' failed", tmp.display()),
                    e,
                )
            })?;
            let copy_steps = io::copy(&mut work_file, &mut tmp_file).and_then(|_| {
                tmp_file.set_permissions(fs::Permissions::from_mode(orig_meta.mode() & 0o7777))?;
                // Best-effort fchown: EPERM for foreign-owned files is logged,
                // not fatal (design §3 step 2).
                // SAFETY: fchown on a valid open fd.
                let rc =
                    unsafe { libc::fchown(tmp_file.as_raw_fd(), orig_meta.uid(), orig_meta.gid()) };
                if rc != 0 {
                    tracing::warn!(
                        "fchown of '{}' failed ({}); committing user becomes the owner",
                        tmp.display(),
                        io::Error::last_os_error()
                    );
                }
                tmp_file.sync_all()
            });
            if let Err(e) = copy_steps {
                remove_tmp();
                return Err(io_err("writing the publish working copy failed", e));
            }
        }
        let bak = bak_path_for(archive);
        if bak.exists() {
            // A stale backup from a previous keep_backup commit; replace it.
            let _ = fs::remove_file(&bak);
        }
        if let Err(e) = create_backup_from_link_result(fs::hard_link(archive, &bak), archive, &bak)
        {
            let _ = fs::remove_file(&bak);
            remove_tmp();
            return Err(io_err("creating .bak backup failed", e));
        }

        // §3 step 6: atomic publish. On failure the original is untouched and
        // both the working copy and the backup are retained for retry.
        if let Err(e) = fs::rename(&tmp, archive) {
            return Err(ArchiveWriteError::RenameFailed {
                archive: archive.clone(),
                tmp,
                bak,
                detail: e.to_string(),
            });
        }
        if let Some(parent) = archive.parent()
            && let Err(e) = File::open(parent).and_then(|d| d.sync_all())
        {
            tracing::warn!("fsync of '{}' failed after publish: {e}", parent.display());
        }

        // §3 step 7: cleanup-only. A `.bak` deletion failure does not fail the
        // commit; the stale path is reported in diagnostics.
        let mut outcome = CommitOutcome {
            bak_path: None,
            bak_cleanup_warning: None,
        };
        if options.keep_backup {
            outcome.bak_path = Some(bak);
        } else if let Err(e) = fs::remove_file(&bak) {
            outcome.bak_cleanup_warning = Some(format!(
                "commit succeeded but deleting backup '{}' failed: {e}",
                bak.display()
            ));
        }
        Ok(outcome)
    } else {
        // Portal path: non-atomic O_TRUNC write over the original portal FD.
        // The file is truncated on open; if the copy or fsync fails partway,
        // the archive may be left corrupted. This is an inherent limitation of
        // portal-granted files (the parent directory is a FUSE mount, so
        // atomic rename is impossible). The app-private backup is the recovery
        // path (design §7).
        let mut work_file = File::open(&work_copy)
            .map_err(|e| io_err("reopening working copy for portal publish failed", e))?;
        let mut portal_file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(archive)
            .map_err(|e| {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    ArchiveWriteError::PortalReadOnly {
                        archive: archive.clone(),
                        backup: ctx.portal_backup.clone(),
                        staged: ctx.staged_path.clone(),
                    }
                } else {
                    io_err(
                        format!(
                            "opening portal path '{}' for write failed",
                            archive.display()
                        ),
                        e,
                    )
                }
            })?;
        io::copy(&mut work_file, &mut portal_file)
            .map_err(|e| io_err("copying working copy to portal path failed", e))?;
        portal_file
            .sync_all()
            .map_err(|e| io_err("fsync of portal path failed", e))?;

        let outcome = CommitOutcome {
            bak_path: ctx.portal_backup.clone(),
            bak_cleanup_warning: None,
        };
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_check_has_no_truncation_and_no_overflow() {
        // 200.99:1 must exceed a 200:1 cap (integer division would pass it).
        assert!(exceeds_ratio(20_099, 100, 200));
        assert!(!exceeds_ratio(20_000, 100, 200));
        assert!(exceeds_ratio(20_001, 100, 200));
        // A zero compressed size with real bytes is a lying central directory.
        assert!(exceeds_ratio(1, 0, 200));
        assert!(!exceeds_ratio(0, 0, 200));
        // Overflowing limit means the cap cannot be exceeded.
        assert!(!exceeds_ratio(u64::MAX, u64::MAX, 200));
    }

    #[test]
    fn bak_falls_back_to_copy_on_exdev_and_eopnotsupp() {
        let dir = tempfile::TempDir::new().unwrap();
        let archive = dir.path().join("a.zip");
        std::fs::write(&archive, b"archive bytes").unwrap();
        for errno in [libc::EXDEV, libc::EOPNOTSUPP, libc::EPERM, libc::EMLINK] {
            let bak = dir.path().join(format!("a.zip.bak-{errno}"));
            create_backup_from_link_result(
                Err(io::Error::from_raw_os_error(errno)),
                &archive,
                &bak,
            )
            .expect("degradable link failure must fall back to a copy");
            assert_eq!(
                std::fs::read(&bak).unwrap(),
                b"archive bytes",
                ".bak copy must be byte-identical to the original"
            );
        }
    }

    #[test]
    fn bak_does_not_degrade_on_other_link_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let archive = dir.path().join("a.zip");
        std::fs::write(&archive, b"archive bytes").unwrap();
        let bak = dir.path().join("a.zip.bak");
        let err = create_backup_from_link_result(
            Err(io::Error::from_raw_os_error(libc::EACCES)),
            &archive,
            &bak,
        )
        .expect_err("non-degradable link failure must propagate");
        assert_eq!(err.raw_os_error(), Some(libc::EACCES));
        assert!(!bak.exists(), "no .bak may be created on a hard failure");
    }

    #[test]
    fn portal_path_detection() {
        assert!(is_portal_path(Path::new("/run/user/1000/doc/abc/file.zip")));
        assert!(!is_portal_path(Path::new("/home/user/file.zip")));
        assert!(!is_portal_path(Path::new("/run/user/1000/file.zip")));
        // Must not match if "doc" is not the 5th component.
        assert!(!is_portal_path(Path::new("/run/user/1000/other/file.zip")));
        // A path ending exactly at /doc IS a portal directory (the FUSE mount
        // root itself), so it matches.
        assert!(is_portal_path(Path::new("/run/user/1000/doc")));
        // Non-UTF-8 paths fall through to false (Path::components handles OS
        // strings without requiring valid UTF-8).
        assert!(!is_portal_path(Path::new("/home/user/file.zip")));
    }

    #[test]
    fn portal_commit_path_uses_o_trunc_and_retains_backup() {
        if !std::process::Command::new("zip")
            .arg("-v")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
            || !std::process::Command::new("unzip")
                .arg("-v")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        {
            eprintln!("SKIP: zip or unzip not on PATH");
            return;
        }
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("zip-src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), b"alpha\n").unwrap();
        let archive = dir.path().join("test.zip");
        let status = std::process::Command::new("zip")
            .arg("-q")
            .arg("-r")
            .arg(&archive)
            .arg(".")
            .current_dir(&src)
            .status()
            .unwrap();
        assert!(status.success());

        let staging = dir.path().join("staging");
        let mut ctx =
            extract_member_for_edit(&archive, "a.txt", &staging, None).expect("extract failed");

        // Force portal path by mutating the context directly.
        let portal_bak = dir.path().join("portal.bak");
        std::fs::copy(&archive, &portal_bak).unwrap();
        ctx.atomic = false;
        ctx.portal_backup = Some(portal_bak.clone());

        std::fs::write(ctx.staged_path(), b"edited\n").unwrap();

        let outcome =
            commit_member_edit(&ctx, &CommitOptions::default()).expect("portal commit failed");

        // O_TRUNC write should have updated the archive.
        let output = std::process::Command::new("unzip")
            .arg("-p")
            .arg(&archive)
            .arg("a.txt")
            .output()
            .unwrap();
        assert_eq!(output.stdout, b"edited\n");

        // Backup is always retained for portal commits.
        assert_eq!(outcome.bak_path, Some(portal_bak.clone()));
        assert!(portal_bak.exists());
    }

    #[test]
    fn portal_read_only_returns_error_with_backup_path() {
        if !std::process::Command::new("zip")
            .arg("-v")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
            || !std::process::Command::new("unzip")
                .arg("-v")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        {
            eprintln!("SKIP: zip or unzip not on PATH");
            return;
        }
        let dir = tempfile::TempDir::new().unwrap();
        let src = dir.path().join("zip-src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), b"alpha\n").unwrap();
        let archive = dir.path().join("ro.zip");
        let status = std::process::Command::new("zip")
            .arg("-q")
            .arg("-r")
            .arg(&archive)
            .arg(".")
            .current_dir(&src)
            .status()
            .unwrap();
        assert!(status.success());
        let meta = std::fs::metadata(&archive).unwrap();
        // Make the file read-only.
        let mut perms = meta.permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&archive, perms).unwrap();

        let staging = dir.path().join("staging");
        let mut ctx =
            extract_member_for_edit(&archive, "a.txt", &staging, None).expect("extract failed");

        let portal_bak = dir.path().join("portal.bak");
        std::fs::copy(&archive, &portal_bak).unwrap();
        ctx.atomic = false;
        ctx.portal_backup = Some(portal_bak.clone());

        let err = commit_member_edit(&ctx, &CommitOptions::default())
            .expect_err("commit on read-only portal path must fail");
        match err {
            ArchiveWriteError::PortalReadOnly {
                archive: a,
                backup,
                staged,
            } => {
                assert_eq!(a, archive);
                assert_eq!(backup.as_deref(), Some(portal_bak.as_path()));
                assert_eq!(staged, ctx.staged_path);
                // The message must direct the user at the edited member, not
                // present the pristine original backup as their edit.
                let msg = ArchiveWriteError::PortalReadOnly {
                    archive: a,
                    backup,
                    staged,
                }
                .to_string();
                assert!(msg.contains("edited member retained"), "got: {msg}");
                assert!(msg.contains("backup of the original archive"), "got: {msg}");
            }
            other => panic!("expected PortalReadOnly, got {other:?}"),
        }
    }
}
