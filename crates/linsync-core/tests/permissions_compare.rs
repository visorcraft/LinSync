// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// Integration tests for the permissions fixture tree.
// The fixture tree is generated on demand by tests/fixtures/permissions/build.sh.
//
// API audit finding
// -----------------
// By default, Unix file-mode bits are metadata only and do not affect folder
// equality. When `FolderCompareOptions::compare_permissions` is enabled, file
// permission differences are part of the comparison result.

mod common;

use linsync_core::{CompareMethod, FolderCompareOptions, FolderEntryState, compare_folders};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{env, fs};

/// Calls build.sh exactly once per test binary run (thread-safe via OnceLock).
/// Returns the path to the fixture directory on success, or None if a required
/// tool is missing.
fn fixture_dir() -> Option<PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        {
            let tool = "bash";
            let missing = Command::new("which")
                .arg(tool)
                .output()
                .map(|o| !o.status.success())
                .unwrap_or(true);
            if missing {
                eprintln!(
                    "permissions fixture: required tool '{tool}' not on PATH — tests will skip"
                );
                return None;
            }
        }

        let dir = common::workspace_root().join("tests/fixtures/permissions");
        let status = Command::new("bash")
            .arg(dir.join("build.sh"))
            .arg(&dir)
            .status()
            .expect("failed to spawn bash for permissions build.sh");
        assert!(status.success(), "permissions build.sh failed: {status}");
        Some(dir)
    })
    .clone()
}

/// Helper: read the Unix permission bits for a path (mode & 0o777).
fn mode(path: &Path) -> io::Result<u32> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o777)
}

/// Unique temporary directory that cleans itself up on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!("linsync-perms-test-{}-{n}", std::process::id()));
        fs::create_dir_all(&path).expect("failed to create test tempdir");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best-effort cleanup; ignore errors (e.g. already removed).
        let _ = fs::remove_dir_all(&self.path);
    }
}

// ---------------------------------------------------------------------------
// Fixture sanity
// ---------------------------------------------------------------------------

/// Verify the build script produces the expected permission bits on each file.
#[test]
fn permissions_fixture_has_expected_modes() {
    let Some(dir) = fixture_dir() else { return };

    for side in ["left", "right"] {
        let base = dir.join(side);
        assert_eq!(
            mode(&base.join("644.txt")).unwrap(),
            0o644,
            "{side}/644.txt"
        );
        assert_eq!(
            mode(&base.join("600.txt")).unwrap(),
            0o600,
            "{side}/600.txt"
        );
        assert_eq!(mode(&base.join("755.sh")).unwrap(), 0o755, "{side}/755.sh");
        // 000.txt is unreadable; we can still stat it for the mode.
        assert_eq!(
            mode(&base.join("000.txt")).unwrap(),
            0o000,
            "{side}/000.txt"
        );
        assert_eq!(
            mode(&base.join("dir-0755")).unwrap(),
            0o755,
            "{side}/dir-0755"
        );
        assert_eq!(
            mode(&base.join("dir-0700")).unwrap(),
            0o700,
            "{side}/dir-0700"
        );
    }
}

// ---------------------------------------------------------------------------
// Permission comparison behaviour
// ---------------------------------------------------------------------------

/// Files with identical content but different modes appear IDENTICAL in a
/// default compare because permission comparison is opt-in.
#[test]
fn permissions_different_modes_same_content_appear_identical_in_default_compare() {
    let tmp = TempDir::new();
    let left = tmp.path.join("left");
    let right = tmp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();

    fs::write(left.join("file.sh"), b"#!/bin/sh\necho hi\n").unwrap();
    fs::write(right.join("file.sh"), b"#!/bin/sh\necho hi\n").unwrap();
    fs::set_permissions(left.join("file.sh"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::set_permissions(right.join("file.sh"), fs::Permissions::from_mode(0o755)).unwrap();

    // Verify the modes actually differ (the test would be meaningless otherwise).
    assert_eq!(mode(&left.join("file.sh")).unwrap(), 0o644);
    assert_eq!(mode(&right.join("file.sh")).unwrap(), 0o755);

    let opts = FolderCompareOptions::default();
    let result = compare_folders(&left, &right, &opts).unwrap();

    let entry = result
        .entries
        .iter()
        .find(|e| e.name == "file.sh")
        .expect("file.sh must appear in result");

    assert_eq!(
        entry.state,
        FolderEntryState::Identical,
        "default compare should ignore permission-only differences"
    );
}

#[test]
fn compare_permissions_marks_mode_only_file_difference() {
    let tmp = TempDir::new();
    let left = tmp.path.join("left");
    let right = tmp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();

    fs::write(left.join("file.sh"), b"#!/bin/sh\necho hi\n").unwrap();
    fs::write(right.join("file.sh"), b"#!/bin/sh\necho hi\n").unwrap();
    fs::set_permissions(left.join("file.sh"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::set_permissions(right.join("file.sh"), fs::Permissions::from_mode(0o755)).unwrap();

    let opts = FolderCompareOptions {
        compare_permissions: true,
        ..FolderCompareOptions::default()
    };
    let result = compare_folders(&left, &right, &opts).unwrap();

    let entry = result
        .entries
        .iter()
        .find(|e| e.name == "file.sh")
        .expect("file.sh must appear in result");

    assert_eq!(entry.state, FolderEntryState::Different);
    assert_eq!(entry.left_permissions, Some(0o644));
    assert_eq!(entry.right_permissions, Some(0o755));
    assert!(!result.is_equal());
}

/// The 000-mode file is unreadable. Under BinaryContents (default), attempting
/// to open it for reading will fail. The engine should record this as an Error
/// entry rather than panicking or returning Err from compare_folders.
#[test]
fn permissions_unreadable_file_produces_error_entry() {
    // Only run when not root (root can always read 000 files).
    if unsafe { libc::getuid() } == 0 {
        eprintln!("skipping: running as root; 0000-mode files are always readable");
        return;
    }

    let Some(dir) = fixture_dir() else { return };
    let opts = FolderCompareOptions {
        compare_method: CompareMethod::BinaryContents,
        ..FolderCompareOptions::default()
    };
    let result = compare_folders(&dir.join("left"), &dir.join("right"), &opts)
        .expect("compare_folders must not return Err for unreadable files");

    let entry = result
        .entries
        .iter()
        .find(|e| e.name == "000.txt")
        .expect("000.txt must appear in result");

    assert_eq!(
        entry.state,
        FolderEntryState::Error,
        "unreadable file (mode 0000) must produce an Error entry; got {:?}",
        entry.state
    );
    assert!(
        entry.error.is_some(),
        "Error entry for 000.txt must carry an error message"
    );
}

/// Directories with different modes (0755 vs 0700) appear IDENTICAL in a
/// folder compare because directory entries are compared only on is_dir status.
#[test]
fn permissions_directory_modes_not_compared() {
    let Some(dir) = fixture_dir() else { return };
    let opts = FolderCompareOptions::default();
    let result = compare_folders(&dir.join("left"), &dir.join("right"), &opts).unwrap();

    // dir-0755 and dir-0700 both exist on both sides with the same name;
    // they should be Identical because directory comparison is existence-only.
    for dir_name in ["dir-0755", "dir-0700"] {
        let entry = result
            .entries
            .iter()
            .find(|e| e.name == dir_name)
            .unwrap_or_else(|| panic!("{dir_name} must appear in result"));

        assert_eq!(
            entry.state,
            FolderEntryState::Identical,
            "current API: directories with different modes must report Identical \
             (no directory-mode comparison implemented); entry: {dir_name}"
        );
    }
}
