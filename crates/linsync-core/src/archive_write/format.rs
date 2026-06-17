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
