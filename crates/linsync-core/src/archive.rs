// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

//! Built-in archive extraction for archive-as-folder comparison.
//!
//! Supports ZIP-family archives (`.zip`, `.jar`, `.war`, `.apk`, `.ipa`), tar
//! archives (`.tar`, `.tgz`/`.tar.gz`, `.tbz2`/`.tar.bz2`, `.txz`/`.tar.xz`,
//! `.tzst`/`.tar.zst`), and 7z archives (`.7z`). Extraction is performed by the
//! host's `unzip`, `tar`, or `7z` binary inside the Phase 6 sandbox
//! (`linsync_sandbox`). The extracted trees are then compared with the standard
//! folder engine.

use std::fs;
use std::path::{Path, PathBuf};

use crate::archive_write::ArchiveFormat;
use crate::folder::{
    FolderCompareError, FolderCompareOptions, FolderCompareResult, compare_folders,
};

/// Failure surface for built-in archive comparison.
#[derive(Debug)]
pub enum ArchiveError {
    /// The archive extension is not supported by the built-in extractor.
    UnsupportedFormat(PathBuf),
    /// The sandboxed helper exited non-zero or could not be spawned.
    ExtractionFailed { command: String, stderr: String },
    /// The extracted trees could not be compared.
    FolderCompare(FolderCompareError),
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat(p) => write!(f, "unsupported archive format: {}", p.display()),
            Self::ExtractionFailed { command, stderr } => {
                let tail = stderr
                    .chars()
                    .rev()
                    .take(400)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>();
                write!(f, "{} failed: {}", command, tail)
            }
            Self::FolderCompare(e) => write!(f, "folder compare failed: {e}"),
        }
    }
}

impl std::error::Error for ArchiveError {}

/// Return `true` if `path` has a built-in archive extension.
pub fn is_builtin_archive_format(path: &Path) -> bool {
    ArchiveFormat::detect(path).is_some()
}

/// Compare two archives by extracting both to caller-supplied directories and
/// running the folder compare engine over the extracted trees.
pub fn compare_builtin_archives_with_dirs<
    P: AsRef<Path>,
    Q: AsRef<Path>,
    LD: AsRef<Path>,
    RD: AsRef<Path>,
>(
    left: P,
    right: Q,
    left_dir: LD,
    right_dir: RD,
    options: &FolderCompareOptions,
) -> Result<FolderCompareResult, ArchiveError> {
    let left = left.as_ref();
    let right = right.as_ref();
    let left_dir = left_dir.as_ref();
    let right_dir = right_dir.as_ref();

    fs::create_dir_all(left_dir).map_err(|e| ArchiveError::ExtractionFailed {
        command: "mkdir".into(),
        stderr: e.to_string(),
    })?;
    fs::create_dir_all(right_dir).map_err(|e| ArchiveError::ExtractionFailed {
        command: "mkdir".into(),
        stderr: e.to_string(),
    })?;

    extract_archive(left, left_dir)?;
    extract_archive(right, right_dir)?;

    compare_folders(left_dir, right_dir, options).map_err(ArchiveError::FolderCompare)
}

/// Compare two archives by extracting both to temporary directories and running
/// the folder compare engine over the extracted trees.
///
/// `TempDir` handles cleanup automatically when the returned result is dropped.
#[cfg(test)]
pub fn compare_builtin_archives<P: AsRef<Path>>(
    left: P,
    right: P,
    options: &FolderCompareOptions,
) -> Result<FolderCompareResult, ArchiveError> {
    let left_dir = tempfile::TempDir::new().map_err(|e| ArchiveError::ExtractionFailed {
        command: "tempdir".into(),
        stderr: e.to_string(),
    })?;
    let right_dir = tempfile::TempDir::new().map_err(|e| ArchiveError::ExtractionFailed {
        command: "tempdir".into(),
        stderr: e.to_string(),
    })?;

    compare_builtin_archives_with_dirs(left, right, left_dir.path(), right_dir.path(), options)
}

fn extract_archive(archive: &Path, dest: &Path) -> Result<(), ArchiveError> {
    let policy = linsync_sandbox::SandboxPolicy::builder()
        .read(archive)
        .write(dest)
        .build();

    if looks_like_zip(archive) {
        let mut cmd = std::process::Command::new("unzip");
        cmd.arg("-q").arg(archive).arg("-d").arg(dest);
        run_sandboxed(cmd, policy, "unzip")?;
    } else if looks_like_tar(archive) {
        let mut cmd = std::process::Command::new("tar");
        cmd.arg("-xf").arg(archive).arg("-C").arg(dest);
        run_sandboxed(cmd, policy, "tar")?;
    } else if looks_like_7z(archive) {
        let mut cmd = std::process::Command::new("7z");
        cmd.arg("x")
            .arg(archive)
            .arg("-y")
            .arg(format!("-o{}", dest.display()));
        run_sandboxed(cmd, policy, "7z")?;
    } else {
        return Err(ArchiveError::UnsupportedFormat(archive.to_path_buf()));
    }
    validate_extracted_paths(dest)?;
    Ok(())
}

fn validate_extracted_paths(dest: &Path) -> Result<(), ArchiveError> {
    let canonical_dest = fs::canonicalize(dest).map_err(|e| ArchiveError::ExtractionFailed {
        command: "canonicalize".into(),
        stderr: e.to_string(),
    })?;
    fn walk(dir: &Path, canonical_dest: &Path) -> Result<(), ArchiveError> {
        for entry in fs::read_dir(dir).map_err(|e| ArchiveError::ExtractionFailed {
            command: "read_dir".into(),
            stderr: e.to_string(),
        })? {
            let entry = entry.map_err(|e| ArchiveError::ExtractionFailed {
                command: "read_dir".into(),
                stderr: e.to_string(),
            })?;
            let path = entry.path();
            let canonical =
                fs::canonicalize(&path).map_err(|e| ArchiveError::ExtractionFailed {
                    command: "canonicalize".into(),
                    stderr: e.to_string(),
                })?;
            if !canonical.starts_with(canonical_dest) {
                return Err(ArchiveError::ExtractionFailed {
                    command: "extract".into(),
                    stderr: format!("path traversal detected: {}", path.display()),
                });
            }
            if path.is_dir() {
                walk(&path, canonical_dest)?;
            }
        }
        Ok(())
    }
    walk(dest, &canonical_dest)
}

fn run_sandboxed(
    cmd: std::process::Command,
    policy: linsync_sandbox::SandboxPolicy,
    name: &str,
) -> Result<(), ArchiveError> {
    let child = linsync_sandbox::SandboxedCommand::new(cmd, policy)
        .spawn()
        .map_err(|e| ArchiveError::ExtractionFailed {
            command: name.into(),
            stderr: e.to_string(),
        })?;
    let output = child
        .wait_with_output()
        .map_err(|e| ArchiveError::ExtractionFailed {
            command: name.into(),
            stderr: e.to_string(),
        })?;
    if !output.status.success() {
        return Err(ArchiveError::ExtractionFailed {
            command: name.into(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }
    Ok(())
}

fn looks_like_zip(path: &Path) -> bool {
    matches!(ArchiveFormat::detect(path), Some(ArchiveFormat::Zip))
}

fn looks_like_tar(path: &Path) -> bool {
    matches!(ArchiveFormat::detect(path), Some(ArchiveFormat::Tar { .. }))
}

fn looks_like_7z(path: &Path) -> bool {
    matches!(ArchiveFormat::detect(path), Some(ArchiveFormat::SevenZip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::folder::FolderEntryState;
    use std::fs;
    use std::process::Command;

    fn make_zip(dir: &Path, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let archive = dir.join(name);
        for (name, contents) in files {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, contents.as_bytes()).unwrap();
        }
        let mut args: Vec<String> = vec!["-q".into(), archive.to_string_lossy().into_owned()];
        for (name, _) in files {
            args.push("-j".into());
            args.push(dir.join(name).to_string_lossy().into_owned());
        }
        let status = Command::new("zip")
            .args(&args)
            .status()
            .expect("zip binary required for tests");
        assert!(status.success());
        archive
    }

    fn make_tar(dir: &Path, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let archive = dir.join(name);
        for (name, contents) in files {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, contents.as_bytes()).unwrap();
        }
        let mut args: Vec<String> = vec!["-cf".into(), archive.to_string_lossy().into_owned()];
        for (name, _) in files {
            args.push("-C".into());
            args.push(dir.to_string_lossy().into_owned());
            args.push((*name).into());
        }
        let status = Command::new("tar")
            .args(&args)
            .status()
            .expect("tar binary required for tests");
        assert!(status.success());
        archive
    }

    #[test]
    fn compare_two_zip_archives() {
        let tmp = tempfile::TempDir::new().unwrap();
        let left = make_zip(
            tmp.path(),
            "left.zip",
            &[("a.txt", "hello"), ("b.txt", "world")],
        );
        let right = make_zip(
            tmp.path(),
            "right.zip",
            &[("a.txt", "hello"), ("b.txt", "rust")],
        );

        let result =
            compare_builtin_archives(&left, &right, &FolderCompareOptions::default()).unwrap();
        assert!(!result.is_equal());
        let changed = result
            .entries
            .iter()
            .filter(|e| e.state != FolderEntryState::Identical)
            .count();
        assert_eq!(changed, 1);
    }

    #[test]
    fn compare_two_tar_archives() {
        let tmp = tempfile::TempDir::new().unwrap();
        let left = make_tar(
            tmp.path(),
            "left.tar",
            &[("a.txt", "hello"), ("b.txt", "world")],
        );
        let right = make_tar(
            tmp.path(),
            "right.tar",
            &[("a.txt", "hello"), ("b.txt", "rust")],
        );

        let result =
            compare_builtin_archives(&left, &right, &FolderCompareOptions::default()).unwrap();
        assert!(!result.is_equal());
        let changed = result
            .entries
            .iter()
            .filter(|e| e.state != FolderEntryState::Identical)
            .count();
        assert_eq!(changed, 1);
    }

    #[test]
    fn is_archive_format_detection() {
        assert!(is_builtin_archive_format(Path::new("archive.zip")));
        assert!(is_builtin_archive_format(Path::new("archive.jar")));
        assert!(is_builtin_archive_format(Path::new("/tmp/x.tar.gz")));
        assert!(is_builtin_archive_format(Path::new("x.tar.bz2")));
        assert!(is_builtin_archive_format(Path::new("x.txz")));
        assert!(is_builtin_archive_format(Path::new("x.7z")));
        assert!(!is_builtin_archive_format(Path::new("x.txt")));
        assert!(!is_builtin_archive_format(Path::new("x.rar")));
    }

    #[test]
    fn unsupported_format_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let left = tmp.path().join("left.rar");
        let right = tmp.path().join("right.rar");
        fs::write(&left, b"x").unwrap();
        fs::write(&right, b"y").unwrap();
        let err =
            compare_builtin_archives(&left, &right, &FolderCompareOptions::default()).unwrap_err();
        assert!(matches!(err, ArchiveError::UnsupportedFormat(_)));
    }
}
