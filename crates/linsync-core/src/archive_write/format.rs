// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

//! Archive format detection and the format-agnostic [`ArchiveEditor`] trait.
//!
//! The zip implementation lives in [`ZipEditor`]; tar and 7z editors will be
//! added in later tasks. The host code in `archive_write.rs` handles member-path
//! validation, archive fingerprinting, locking, staging lifecycle, atomic
//! publish, and backup handling — everything that is independent of the
//! particular archive format.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{ArchiveEditCaps, ArchiveWriteError, SandboxGrants, io_err, run_helper, unsupported};

/// Built-in archive formats that support member editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    Tar { compression: TarCompression },
    SevenZip,
}

/// Compression variant for tar archives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TarCompression {
    None,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

impl ArchiveFormat {
    /// Detect the archive format from the file path's extension(s).
    pub fn detect(path: &Path) -> Option<ArchiveFormat> {
        let name = path.to_string_lossy().to_lowercase();

        if name.ends_with(".tar") {
            return Some(ArchiveFormat::Tar {
                compression: TarCompression::None,
            });
        }
        if name.ends_with(".tgz") || name.ends_with(".tar.gz") {
            return Some(ArchiveFormat::Tar {
                compression: TarCompression::Gzip,
            });
        }
        if name.ends_with(".tbz2") || name.ends_with(".tar.bz2") {
            return Some(ArchiveFormat::Tar {
                compression: TarCompression::Bzip2,
            });
        }
        if name.ends_with(".txz") || name.ends_with(".tar.xz") {
            return Some(ArchiveFormat::Tar {
                compression: TarCompression::Xz,
            });
        }
        if name.ends_with(".tzst") || name.ends_with(".tar.zst") {
            return Some(ArchiveFormat::Tar {
                compression: TarCompression::Zstd,
            });
        }

        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            match ext.to_lowercase().as_str() {
                "zip" | "jar" | "war" | "apk" | "ipa" => {
                    return Some(ArchiveFormat::Zip);
                }
                "7z" => return Some(ArchiveFormat::SevenZip),
                _ => {}
            }
        }

        None
    }
}

/// Metadata for a located archive member, returned by [`ArchiveEditor::locate_member`].
pub struct LocatedMember {
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub mode: Option<u32>,
    pub is_regular: bool,
    pub member_count: usize,
}

/// Per-format editor: parses metadata, extracts one member, and repacks the
/// archive with that member replaced by a staged file.
pub trait ArchiveEditor: Send + Sync {
    fn format(&self) -> ArchiveFormat;

    /// Parse archive metadata and return the member entry if it exists and is editable.
    fn locate_member(
        &self,
        archive: &Path,
        member: &str,
        caps: &ArchiveEditCaps,
    ) -> Result<LocatedMember, ArchiveWriteError>;

    /// Extract `member` into `extract_root`, returning the staged file path.
    fn extract_member(
        &self,
        archive: &Path,
        member: &str,
        extract_root: &Path,
        grants: &SandboxGrants,
    ) -> Result<PathBuf, ArchiveWriteError>;

    /// Repack the archive, replacing `member` with the staged file at `staged_path`.
    /// The working copy is written to `work_archive_path`.
    fn repack(
        &self,
        archive: &Path,
        member: &str,
        staged_path: &Path,
        work_dir: &Path,
        work_archive_path: &Path,
        grants: &SandboxGrants,
    ) -> Result<(), ArchiveWriteError>;
}

/// Format-specific editor for zip archives.
#[derive(Debug, Clone, Copy)]
pub struct ZipEditor;

/// Format-specific editor for tar archives (plain or compressed).
#[derive(Debug, Clone, Copy)]
pub struct TarEditor {
    compression: TarCompression,
}

impl TarEditor {
    pub fn new(compression: TarCompression) -> Self {
        Self { compression }
    }
}

/// Format-specific editor for 7z archives.
#[derive(Debug, Clone, Copy)]
pub struct SevenZipEditor;

/// Parse a tar `-tv` listing line. Returns `(type_char, size, mode)`.
///
/// GNU tar output looks like:
/// `-rw-r--r-- 0/0               6 2000-01-01 00:00 ./alpha.txt`
/// We require `--numeric-owner` so the owner field is not a bare numeric UID
/// that could be mistaken for the size.
fn parse_tar_tv_line(line: &str) -> Option<(char, u64, u32)> {
    let line = line.trim_end();
    if line.len() < 10 {
        return None;
    }
    let type_char = line.chars().next()?;
    let perms = &line[1..10];
    let rest = &line[10..];
    // The first all-digit token after the permissions is the size.
    let size_token = rest
        .split_whitespace()
        .find(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()))?;
    let size: u64 = size_token.parse().ok()?;
    let mode = parse_tar_mode(perms)?;
    Some((type_char, size, mode))
}

/// Convert a tar permission string (e.g. `rw-r--r--`) into a Unix mode.
fn parse_tar_mode(perms: &str) -> Option<u32> {
    if perms.len() != 9 {
        return None;
    }
    let mut mode = 0u32;
    let bytes = perms.as_bytes();
    if bytes[0] == b'r' {
        mode |= 0o400;
    }
    if bytes[1] == b'w' {
        mode |= 0o200;
    }
    match bytes[2] {
        b'x' => mode |= 0o100,
        b's' => mode |= 0o4100,
        b'S' => mode |= 0o4000,
        b'-' => {}
        _ => return None,
    }
    if bytes[3] == b'r' {
        mode |= 0o040;
    }
    if bytes[4] == b'w' {
        mode |= 0o020;
    }
    match bytes[5] {
        b'x' => mode |= 0o010,
        b's' => mode |= 0o2010,
        b'S' => mode |= 0o2000,
        b'-' => {}
        _ => return None,
    }
    if bytes[6] == b'r' {
        mode |= 0o004;
    }
    if bytes[7] == b'w' {
        mode |= 0o002;
    }
    match bytes[8] {
        b'x' => mode |= 0o001,
        b't' => mode |= 0o1001,
        b'T' => mode |= 0o1000,
        b'-' => {}
        _ => return None,
    }
    Some(mode)
}

/// Parse a 7z `-slt` listing block into key/value pairs.
fn parse_sevenzip_block(block: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in block.lines() {
        if let Some((k, v)) = line.split_once(" = ") {
            map.insert(k.trim().to_owned(), v.trim().to_owned());
        }
    }
    map
}

/// Whether the 7z listing block describes a regular file.
fn sevenzip_block_is_regular(block: &std::collections::HashMap<String, String>) -> bool {
    // Directories are reported as `Folder = +`.
    if block.get("Folder").map(|s| s.as_str()) == Some("+") {
        return false;
    }
    if let Some(attrs) = block.get("Attributes") {
        // 7z Attributes look like `A -rw-r--r--` or `A lrwxrwxrwx`. The
        // optional leading Windows attribute letter is followed by a space and
        // then the Unix permission string.
        let perm = attrs.split_whitespace().nth(1).unwrap_or(attrs);
        if !perm.starts_with('-') {
            return false;
        }
    }
    true
}

/// Sanity cap on the zip central directory size the host will parse.
const MAX_CENTRAL_DIRECTORY_SIZE: u64 = 64 * 1024 * 1024;

struct CdEntry {
    name: Vec<u8>,
    utf8_flag: bool,
    compressed_size: u64,
    uncompressed_size: u64,
    /// `Some(mode)` when the entry was made on Unix (external attrs >> 16).
    unix_mode: Option<u32>,
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

impl ArchiveEditor for ZipEditor {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Zip
    }

    fn locate_member(
        &self,
        archive: &Path,
        member: &str,
        caps: &ArchiveEditCaps,
    ) -> Result<LocatedMember, ArchiveWriteError> {
        let mut file = File::open(archive)
            .map_err(|e| io_err(format!("opening '{}' failed", archive.display()), e))?;
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
            return Err(ArchiveWriteError::NonRegularMember {
                member: member.to_owned(),
            });
        }

        let is_regular = if let Some(mode) = entry.unix_mode {
            let fmt = mode & libc::S_IFMT;
            fmt == 0 || fmt == libc::S_IFREG
        } else {
            true
        };
        if !is_regular {
            return Err(ArchiveWriteError::NonRegularMember {
                member: member.to_owned(),
            });
        }

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
        if super::exceeds_ratio(
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

        Ok(LocatedMember {
            uncompressed_size: entry.uncompressed_size,
            compressed_size: entry.compressed_size,
            mode: member_mode,
            is_regular: true,
            member_count,
        })
    }

    fn extract_member(
        &self,
        archive: &Path,
        member: &str,
        extract_root: &Path,
        grants: &SandboxGrants,
    ) -> Result<PathBuf, ArchiveWriteError> {
        let mut cmd = Command::new("unzip");
        cmd.arg("-q")
            .arg("-o")
            .arg("-d")
            .arg(extract_root)
            .arg(archive)
            .arg(member);
        let (ok, _stdout, stderr) = run_helper(cmd, grants, "unzip")?;
        if !ok {
            return Err(ArchiveWriteError::ExtractFailed {
                detail: format!("unzip failed: {}", stderr.trim()),
            });
        }
        Ok(extract_root.join(member))
    }

    fn repack(
        &self,
        archive: &Path,
        member: &str,
        staged_path: &Path,
        work_dir: &Path,
        work_archive_path: &Path,
        grants: &SandboxGrants,
    ) -> Result<(), ArchiveWriteError> {
        // Start from the original archive and let Info-ZIP replace the one
        // member in the staging-bound working copy. The working copy must be
        // writable even if the original archive is read-only.
        fs::copy(archive, work_archive_path)
            .map_err(|e| io_err("copying original to working copy failed", e))?;
        fs::set_permissions(work_archive_path, fs::Permissions::from_mode(0o644))
            .map_err(|e| io_err("making working copy writable failed", e))?;

        // `zip` expects a relative member path, so run it from the extract root
        // where the staged file lives. `-b` keeps zip's own temp files inside
        // the staging grant.
        let mut extract_root = staged_path.to_path_buf();
        for _ in member.split('/') {
            extract_root = extract_root
                .parent()
                .expect("member path is validated and stays inside extract root")
                .to_path_buf();
        }

        let mut cmd = Command::new("zip");
        cmd.arg("-q")
            .arg("-b")
            .arg(work_dir)
            .arg(work_archive_path)
            .arg(member)
            .current_dir(&extract_root);
        let (ok, _stdout, stderr) = run_helper(cmd, grants, "zip")?;
        if !ok {
            return Err(ArchiveWriteError::RepackFailed {
                detail: format!("zip failed: {}", stderr.trim()),
            });
        }

        // Post-repack assertion (design §5): list the working copy under the
        // same sandbox policy; member count unchanged, target exactly once. `-UU`
        // makes zipinfo print stored name bytes verbatim so the comparison is
        // byte-exact; `-1` prints names only, one per line.
        let mut file = File::open(archive)
            .map_err(|e| io_err(format!("opening '{}' failed", archive.display()), e))?;
        let meta = file
            .metadata()
            .map_err(|e| io_err("stat of archive failed", e))?;
        let expected_count = parse_central_directory(&mut file, meta.len())?.len();

        let mut cmd = Command::new("unzip");
        cmd.arg("-Z").arg("-1").arg("-UU").arg(work_archive_path);
        let (ok, stdout, stderr) = run_helper(cmd, grants, "unzip -Z")?;
        if !ok {
            return Err(ArchiveWriteError::PostRepackAssertion {
                detail: format!("listing the working copy failed: {}", stderr.trim()),
            });
        }
        let names: Vec<&[u8]> = stdout
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .collect();
        super::verify_post_repack_listing(&names, member, expected_count)?;

        Ok(())
    }
}

impl ArchiveEditor for TarEditor {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Tar {
            compression: self.compression,
        }
    }

    fn locate_member(
        &self,
        archive: &Path,
        member: &str,
        caps: &ArchiveEditCaps,
    ) -> Result<LocatedMember, ArchiveWriteError> {
        let metadata =
            fs::symlink_metadata(archive).map_err(|_| ArchiveWriteError::ArchiveNotFound {
                archive: archive.to_path_buf(),
            })?;
        let archive_size = metadata.len();

        // Resolve the user-facing name to the stored name (e.g. `alpha.txt` →
        // `./alpha.txt`) before asking tar for metadata.
        let stored = resolve_tar_member(archive, member, &grants_read_archive(archive))?;

        // `--numeric-owner` keeps the owner field from looking like a size.
        let mut cmd = Command::new("tar");
        cmd.arg("-tvf")
            .arg(archive)
            .arg("--numeric-owner")
            .arg(&stored);
        let (ok, stdout, stderr) = run_helper(cmd, &grants_read_archive(archive), "tar -tv")?;
        let stdout = String::from_utf8_lossy(&stdout);
        if !ok {
            return Err(ArchiveWriteError::ExtractFailed {
                detail: format!("tar listing failed: {}", stderr.trim()),
            });
        }

        let lines: Vec<&str> = stdout.lines().collect();
        if lines.is_empty() {
            return Err(ArchiveWriteError::MemberNotFound {
                member: member.to_owned(),
            });
        }
        // Multiple matching lines means the member is ambiguous (e.g. both a
        // directory and a file with the same prefix). Treat as non-regular.
        if lines.len() > 1 {
            return Err(ArchiveWriteError::NonRegularMember {
                member: member.to_owned(),
            });
        }
        let (type_char, uncompressed_size, mode) =
            parse_tar_tv_line(lines[0]).ok_or_else(|| ArchiveWriteError::UnsupportedArchive {
                detail: format!("cannot parse tar listing line: {}", lines[0]),
            })?;
        if type_char != '-' {
            return Err(ArchiveWriteError::NonRegularMember {
                member: member.to_owned(),
            });
        }

        // Apply caps to the declared uncompressed size.
        if uncompressed_size > caps.max_member_size {
            return Err(ArchiveWriteError::CapsExceeded {
                member: member.to_owned(),
                detail: format!(
                    "declared size {} exceeds the {} byte cap",
                    uncompressed_size, caps.max_member_size
                ),
            });
        }
        // Tar does not record a per-member compressed size; use the whole
        // archive size as a conservative ratio denominator.
        if super::exceeds_ratio(uncompressed_size, archive_size, caps.max_compression_ratio) {
            return Err(ArchiveWriteError::CapsExceeded {
                member: member.to_owned(),
                detail: format!(
                    "declared size {} exceeds the {}:1 ratio cap against archive size {}",
                    uncompressed_size, caps.max_compression_ratio, archive_size
                ),
            });
        }

        // Count members for the post-repack assertion.
        let member_count = count_tar_members(archive, &grants_read_archive(archive))?;

        Ok(LocatedMember {
            uncompressed_size,
            compressed_size: archive_size,
            mode: Some(mode),
            is_regular: true,
            member_count,
        })
    }

    fn extract_member(
        &self,
        archive: &Path,
        member: &str,
        extract_root: &Path,
        grants: &SandboxGrants,
    ) -> Result<PathBuf, ArchiveWriteError> {
        let stored = resolve_tar_member(archive, member, grants)?;
        let mut cmd = Command::new("tar");
        cmd.arg("-xf")
            .arg(archive)
            .arg("-C")
            .arg(extract_root)
            .arg(&stored);
        let (ok, _stdout, stderr) = run_helper(cmd, grants, "tar -x")?;
        if !ok {
            return Err(ArchiveWriteError::ExtractFailed {
                detail: format!("tar extraction failed: {}", stderr.trim()),
            });
        }
        // `tar` strips a leading `./` on extraction, so the staged file lives
        // at the user-facing path.
        Ok(extract_root.join(member))
    }

    fn repack(
        &self,
        archive: &Path,
        member: &str,
        staged_path: &Path,
        work_dir: &Path,
        work_archive_path: &Path,
        grants: &SandboxGrants,
    ) -> Result<(), ArchiveWriteError> {
        // Whole-archive rewrite: extract everything, overwrite the member, then
        // repack with the matching compression flag.
        let extract_dir = work_dir.join("extract");
        fs::create_dir_all(&extract_dir)
            .map_err(|e| io_err("creating tar work extract dir failed", e))?;

        let mut cmd = Command::new("tar");
        cmd.arg("-xf").arg(archive).arg("-C").arg(&extract_dir);
        let (ok, _stdout, stderr) = run_helper(cmd, grants, "tar work extract")?;
        if !ok {
            return Err(ArchiveWriteError::RepackFailed {
                detail: format!("tar work extraction failed: {}", stderr.trim()),
            });
        }

        let dest = extract_dir.join(member);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| io_err("creating parent dir for staged member failed", e))?;
        }
        fs::copy(staged_path, &dest)
            .map_err(|e| io_err("copying staged file into tar work dir failed", e))?;

        let mut cmd = Command::new("tar");
        match self.compression {
            TarCompression::None => cmd.arg("-cf"),
            TarCompression::Gzip => cmd.arg("-czf"),
            TarCompression::Bzip2 => cmd.arg("-cjf"),
            TarCompression::Xz => cmd.arg("-cJf"),
            TarCompression::Zstd => cmd.arg("--zstd").arg("-cf"),
        };
        cmd.arg(work_archive_path)
            .arg("-C")
            .arg(&extract_dir)
            .arg(".");
        let (ok, _stdout, stderr) = run_helper(cmd, grants, "tar repack")?;
        if !ok {
            return Err(ArchiveWriteError::RepackFailed {
                detail: format!("tar repack failed: {}", stderr.trim()),
            });
        }

        // Post-repack assertion: the target member exists exactly once and the
        // member count is unchanged.
        let expected_count = count_tar_members(archive, grants)?;
        let actual_count = count_tar_members(work_archive_path, grants)?;
        if actual_count != expected_count {
            return Err(ArchiveWriteError::PostRepackAssertion {
                detail: format!(
                    "member count changed: expected {expected_count}, working copy has {actual_count}"
                ),
            });
        }
        // The repack creates entries with a `./` prefix; resolve the member
        // name against the new archive before asserting.
        let stored = resolve_tar_member(work_archive_path, member, grants)?;
        let mut cmd = Command::new("tar");
        cmd.arg("-tf").arg(work_archive_path).arg(&stored);
        let (ok, stdout, stderr) = run_helper(cmd, grants, "tar post-repack list")?;
        if !ok {
            return Err(ArchiveWriteError::PostRepackAssertion {
                detail: format!("post-repack tar listing failed: {}", stderr.trim()),
            });
        }
        let occurrences = stdout
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
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
}

impl ArchiveEditor for SevenZipEditor {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::SevenZip
    }

    fn locate_member(
        &self,
        archive: &Path,
        member: &str,
        caps: &ArchiveEditCaps,
    ) -> Result<LocatedMember, ArchiveWriteError> {
        if !sevenzip_available() {
            return Err(ArchiveWriteError::UnsupportedArchive {
                detail: "7z command not found".to_owned(),
            });
        }

        let metadata =
            fs::symlink_metadata(archive).map_err(|_| ArchiveWriteError::ArchiveNotFound {
                archive: archive.to_path_buf(),
            })?;
        let archive_size = metadata.len();

        let mut cmd = Command::new("7z");
        cmd.arg("l").arg("-slt").arg(archive).arg(member);
        let (ok, stdout, stderr) = run_helper(cmd, &grants_read_archive(archive), "7z l")?;
        if !ok {
            if stderr.to_ascii_lowercase().contains("can not open")
                || stderr.to_ascii_lowercase().contains("errors: 1")
            {
                return Err(ArchiveWriteError::ArchiveNotFound {
                    archive: archive.to_path_buf(),
                });
            }
            return Err(ArchiveWriteError::ExtractFailed {
                detail: format!("7z listing failed: {}", stderr.trim()),
            });
        }

        let text = String::from_utf8_lossy(&stdout);
        let block = find_sevenzip_block(&text, member).ok_or_else(|| {
            ArchiveWriteError::MemberNotFound {
                member: member.to_owned(),
            }
        })?;
        let props = parse_sevenzip_block(block);

        if !sevenzip_block_is_regular(&props) {
            return Err(ArchiveWriteError::NonRegularMember {
                member: member.to_owned(),
            });
        }

        let uncompressed_size = props
            .get("Size")
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| unsupported("7z listing missing Size"))?;
        let compressed_size = props
            .get("Packed Size")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(archive_size);
        let mode = props.get("Attributes").and_then(|s| parse_sevenzip_mode(s));

        if uncompressed_size > caps.max_member_size {
            return Err(ArchiveWriteError::CapsExceeded {
                member: member.to_owned(),
                detail: format!(
                    "declared size {} exceeds the {} byte cap",
                    uncompressed_size, caps.max_member_size
                ),
            });
        }
        if super::exceeds_ratio(
            uncompressed_size,
            compressed_size,
            caps.max_compression_ratio,
        ) {
            return Err(ArchiveWriteError::CapsExceeded {
                member: member.to_owned(),
                detail: format!(
                    "declared compression ratio {}:{} exceeds the {}:1 cap",
                    uncompressed_size, compressed_size, caps.max_compression_ratio
                ),
            });
        }

        let member_count = count_sevenzip_members(archive, &grants_read_archive(archive))?;

        Ok(LocatedMember {
            uncompressed_size,
            compressed_size,
            mode,
            is_regular: true,
            member_count,
        })
    }

    fn extract_member(
        &self,
        archive: &Path,
        member: &str,
        extract_root: &Path,
        grants: &SandboxGrants,
    ) -> Result<PathBuf, ArchiveWriteError> {
        let mut cmd = Command::new("7z");
        // 7z expects `-o<outdir>` without a space.
        cmd.arg("x")
            .arg(format!("-o{}", extract_root.display()))
            .arg(archive)
            .arg(member);
        let (ok, _stdout, stderr) = run_helper(cmd, grants, "7z x")?;
        if !ok {
            return Err(ArchiveWriteError::ExtractFailed {
                detail: format!("7z extraction failed: {}", stderr.trim()),
            });
        }
        Ok(extract_root.join(member))
    }

    fn repack(
        &self,
        archive: &Path,
        member: &str,
        staged_path: &Path,
        work_dir: &Path,
        work_archive_path: &Path,
        grants: &SandboxGrants,
    ) -> Result<(), ArchiveWriteError> {
        let extract_dir = work_dir.join("extract");
        fs::create_dir_all(&extract_dir)
            .map_err(|e| io_err("creating 7z work extract dir failed", e))?;

        let mut cmd = Command::new("7z");
        cmd.arg("x")
            .arg(format!("-o{}", extract_dir.display()))
            .arg(archive);
        let (ok, _stdout, stderr) = run_helper(cmd, grants, "7z work extract")?;
        if !ok {
            return Err(ArchiveWriteError::RepackFailed {
                detail: format!("7z work extraction failed: {}", stderr.trim()),
            });
        }

        let dest = extract_dir.join(member);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| io_err("creating parent dir for staged member failed", e))?;
        }
        fs::copy(staged_path, &dest)
            .map_err(|e| io_err("copying staged file into 7z work dir failed", e))?;

        // Build the new archive from the extracted tree, running 7z inside the
        // extract dir so stored paths are relative to the archive root.
        let mut cmd = Command::new("7z");
        cmd.arg("a")
            .arg(work_archive_path)
            .current_dir(&extract_dir);
        let entries: Vec<_> = fs::read_dir(&extract_dir)
            .map_err(|e| io_err("reading 7z work extract dir failed", e))?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        if entries.is_empty() {
            return Err(ArchiveWriteError::RepackFailed {
                detail: "7z work extract dir is empty".to_owned(),
            });
        }
        cmd.args(&entries);
        let (ok, _stdout, stderr) = run_helper(cmd, grants, "7z repack")?;
        if !ok {
            return Err(ArchiveWriteError::RepackFailed {
                detail: format!("7z repack failed: {}", stderr.trim()),
            });
        }

        let expected_count = count_sevenzip_members(archive, grants)?;
        let actual_count = count_sevenzip_members(work_archive_path, grants)?;
        if actual_count != expected_count {
            return Err(ArchiveWriteError::PostRepackAssertion {
                detail: format!(
                    "member count changed: expected {expected_count}, working copy has {actual_count}"
                ),
            });
        }
        let mut cmd = Command::new("7z");
        cmd.arg("l").arg("-slt").arg(work_archive_path).arg(member);
        let (ok, stdout, stderr) = run_helper(cmd, grants, "7z post-repack list")?;
        if !ok {
            return Err(ArchiveWriteError::PostRepackAssertion {
                detail: format!("post-repack 7z listing failed: {}", stderr.trim()),
            });
        }
        let text = String::from_utf8_lossy(&stdout);
        let occurrences = text
            .split("----------")
            .filter(|b| !b.trim().is_empty())
            .filter(|b| {
                let props = parse_sevenzip_block(b.trim());
                props.get("Path").map(|s| s.as_str()) == Some(member)
            })
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
}

/// Grants that only read the original archive.
fn grants_read_archive(archive: &Path) -> SandboxGrants {
    SandboxGrants {
        read: vec![archive.to_path_buf()],
        write: Vec::new(),
    }
}

/// Possible stored names for a tar member. GNU tar commonly stores paths with
/// a leading `./` when the archive is created from `.`, but archives created
/// from explicit file lists omit the prefix.
fn tar_member_variants(member: &str) -> Vec<String> {
    let mut variants = vec![member.to_owned()];
    if !member.starts_with("./") {
        variants.push(format!("./{member}"));
    }
    variants
}

/// Resolve a user-facing member name to the name actually stored in the tar
/// archive.
fn resolve_tar_member(
    archive: &Path,
    member: &str,
    grants: &SandboxGrants,
) -> Result<String, ArchiveWriteError> {
    for variant in tar_member_variants(member) {
        let mut cmd = Command::new("tar");
        cmd.arg("-tf").arg(archive).arg(&variant);
        let (ok, stdout, stderr) = run_helper(cmd, grants, "tar resolve")?;
        if ok && !stdout.is_empty() {
            return Ok(variant);
        }
        if !ok && !stderr.contains("Not found in archive") {
            return Err(ArchiveWriteError::ExtractFailed {
                detail: format!("tar resolve failed: {}", stderr.trim()),
            });
        }
    }
    Err(ArchiveWriteError::MemberNotFound {
        member: member.to_owned(),
    })
}

/// Cached answer to whether the `7z` binary is available.
fn sevenzip_available() -> bool {
    use std::sync::OnceLock;
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("7z")
            .arg("--help")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// Count regular/non-directory entries in a tar archive (used for the
/// post-repack assertion).
fn count_tar_members(archive: &Path, grants: &SandboxGrants) -> Result<usize, ArchiveWriteError> {
    let mut cmd = Command::new("tar");
    cmd.arg("-tf").arg(archive);
    let (ok, stdout, stderr) = run_helper(cmd, grants, "tar -tf")?;
    if !ok {
        return Err(ArchiveWriteError::ExtractFailed {
            detail: format!("tar member count failed: {}", stderr.trim()),
        });
    }
    Ok(stdout
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .count())
}

/// Count entries in a 7z archive (used for the post-repack assertion).
fn count_sevenzip_members(
    archive: &Path,
    grants: &SandboxGrants,
) -> Result<usize, ArchiveWriteError> {
    let mut cmd = Command::new("7z");
    cmd.arg("l").arg("-slt").arg(archive);
    let (ok, stdout, stderr) = run_helper(cmd, grants, "7z l count")?;
    if !ok {
        return Err(ArchiveWriteError::ExtractFailed {
            detail: format!("7z member count failed: {}", stderr.trim()),
        });
    }
    let text = String::from_utf8_lossy(&stdout);
    Ok(text
        .split("----------")
        .filter(|b| !b.trim().is_empty())
        .filter(|b| {
            let props = parse_sevenzip_block(b.trim());
            props.contains_key("Path") && sevenzip_block_is_regular(&props)
        })
        .count())
}

/// Find the `-slt` block whose `Path` property equals `member`.
fn find_sevenzip_block<'a>(text: &'a str, member: &str) -> Option<&'a str> {
    text.split("----------").map(|b| b.trim()).find(|b| {
        let props = parse_sevenzip_block(b);
        props.get("Path").map(|s| s.as_str()) == Some(member)
    })
}

/// Parse a 7z `Attributes` string (e.g. `-rw-r--r--`) into a Unix mode.
fn parse_sevenzip_mode(attrs: &str) -> Option<u32> {
    // 7z Attributes are the Windows/Unix attribute string. When created on
    // Unix they look like a standard permission string. Ignore a leading
    // directory/symlink marker and parse the rest as a tar mode.
    let rest = attrs
        .strip_prefix('-')
        .or_else(|| attrs.strip_prefix('D'))
        .or_else(|| attrs.strip_prefix('L'))
        .unwrap_or(attrs);
    parse_tar_mode(rest)
}
