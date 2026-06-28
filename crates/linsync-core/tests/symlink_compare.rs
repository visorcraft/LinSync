// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// Integration tests for symlink handling in folder compare.
// The fixture tree is generated on demand by tests/fixtures/symlink/build.sh.

mod common;

use linsync_core::{
    FolderCompareOptions, FolderEntryState, FolderEntryType, SymlinkPolicy, compare_folders,
};
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

/// Calls build.sh exactly once per test binary run (thread-safe via OnceLock).
/// Returns the path to the fixture directory on success, or None if a required
/// tool is missing.
fn fixture_dir() -> Option<PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        // Check required tools first.
        for tool in ["bash", "ln"] {
            let missing = Command::new("which")
                .arg(tool)
                .output()
                .map(|o| !o.status.success())
                .unwrap_or(true);
            if missing {
                eprintln!("symlink fixture: required tool '{tool}' not on PATH — tests will skip");
                return None;
            }
        }

        let dir = common::workspace_root().join("tests/fixtures/symlink");
        let status = Command::new("bash")
            .arg(dir.join("build.sh"))
            .arg(&dir)
            .status()
            .expect("failed to spawn bash for symlink build.sh");
        assert!(status.success(), "symlink build.sh failed: {status}");
        Some(dir)
    })
    .clone()
}

// ---------------------------------------------------------------------------
// CompareTarget policy (default)
// ---------------------------------------------------------------------------

/// Default policy (CompareTarget) must not panic or error on dangling symlinks.
/// It reads the raw link target path via readlink — no filesystem follow occurs.
#[test]
fn symlink_compare_default_policy_does_not_panic_on_dangling() {
    let Some(dir) = fixture_dir() else { return };
    let opts = FolderCompareOptions::default();
    let result = compare_folders(&dir.join("left"), &dir.join("right"), &opts);
    assert!(
        result.is_ok(),
        "default (CompareTarget) policy must not error on dangling symlinks; got: {result:?}"
    );
}

/// CompareTarget: symlinks whose targets differ are reported as Different.
/// left/dangling → /nonexistent, right/dangling → /also-nonexistent
#[test]
fn symlink_compare_target_policy_reports_different_for_different_link_targets() {
    let Some(dir) = fixture_dir() else { return };
    let opts = FolderCompareOptions {
        symlink_policy: SymlinkPolicy::CompareTarget,
        ..FolderCompareOptions::default()
    };
    let result = compare_folders(&dir.join("left"), &dir.join("right"), &opts).unwrap();

    let dangling = result
        .entries
        .iter()
        .find(|e| e.name == "dangling")
        .expect("dangling symlink entry must appear in result");

    assert_eq!(
        dangling.state,
        FolderEntryState::Different,
        "dangling symlinks with different targets must be Different"
    );
    assert_eq!(
        dangling.entry_type,
        FolderEntryType::Symlink,
        "entry_type must be Symlink under CompareTarget policy"
    );
}

/// CompareTarget: symlinks with identical targets are Identical.
/// Both left and right have symlink-to-file → target.txt.
#[test]
fn symlink_compare_target_policy_reports_identical_for_matching_targets() {
    let Some(dir) = fixture_dir() else { return };
    let opts = FolderCompareOptions {
        symlink_policy: SymlinkPolicy::CompareTarget,
        ..FolderCompareOptions::default()
    };
    let result = compare_folders(&dir.join("left"), &dir.join("right"), &opts).unwrap();

    let entry = result
        .entries
        .iter()
        .find(|e| e.name == "symlink-to-file")
        .expect("symlink-to-file must appear in result");

    assert_eq!(
        entry.state,
        FolderEntryState::Identical,
        "symlinks with identical target strings must be Identical"
    );
}

/// CompareTarget: symlink-relative has different link paths on each side.
/// left → ../left/target.txt, right → ../right/target.txt
#[test]
fn symlink_compare_target_policy_reports_different_for_different_relative_targets() {
    let Some(dir) = fixture_dir() else { return };
    let opts = FolderCompareOptions {
        symlink_policy: SymlinkPolicy::CompareTarget,
        ..FolderCompareOptions::default()
    };
    let result = compare_folders(&dir.join("left"), &dir.join("right"), &opts).unwrap();

    let entry = result
        .entries
        .iter()
        .find(|e| e.name == "symlink-relative")
        .expect("symlink-relative must appear in result");

    assert_eq!(
        entry.state,
        FolderEntryState::Different,
        "symlinks with different relative target paths must be Different"
    );
}

// ---------------------------------------------------------------------------
// Follow policy
// ---------------------------------------------------------------------------

/// Follow policy: symlink-to-file follows to target.txt whose content is
/// identical on both sides ("hello\n"), so the entry must be Identical.
#[test]
fn symlink_compare_follow_policy_compares_target_content() {
    let Some(dir) = fixture_dir() else { return };
    let opts = FolderCompareOptions {
        symlink_policy: SymlinkPolicy::Follow,
        ..FolderCompareOptions::default()
    };
    let result = compare_folders(&dir.join("left"), &dir.join("right"), &opts).unwrap();

    let entry = result
        .entries
        .iter()
        .find(|e| e.name == "symlink-to-file")
        .expect("symlink-to-file must appear in result");

    assert_eq!(
        entry.state,
        FolderEntryState::Identical,
        "followed symlinks with identical file content must be Identical"
    );
}

/// Follow policy: dangling symlinks cause a stat failure, which the engine
/// records as an Error entry — not a panic, not an Err return from compare_folders.
#[test]
fn symlink_compare_follow_policy_records_error_for_dangling() {
    let Some(dir) = fixture_dir() else { return };
    let opts = FolderCompareOptions {
        symlink_policy: SymlinkPolicy::Follow,
        ..FolderCompareOptions::default()
    };
    // compare_folders itself must succeed (Ok)
    let result = compare_folders(&dir.join("left"), &dir.join("right"), &opts)
        .expect("compare_folders must not return Err for dangling symlinks under Follow policy");

    let dangling = result
        .entries
        .iter()
        .find(|e| e.name == "dangling")
        .expect("dangling entry must appear in result");

    assert_eq!(
        dangling.state,
        FolderEntryState::Error,
        "dangling symlink under Follow policy must produce an Error entry"
    );
    assert!(
        dangling.error.is_some(),
        "Error entry must carry an error message"
    );
}

// ---------------------------------------------------------------------------
// SpecialFile policy
// ---------------------------------------------------------------------------

/// SpecialFile policy treats all symlinks as opaque special files.
/// Both sides have the same symlink names. With no link_target comparison
/// (both sides record None), each entry compares as Identical.
/// Verify no Error entries are produced for symlinks under this policy.
#[test]
fn symlink_compare_special_file_policy_does_not_read_link_targets() {
    let Some(dir) = fixture_dir() else { return };
    let opts = FolderCompareOptions {
        symlink_policy: SymlinkPolicy::SpecialFile,
        ..FolderCompareOptions::default()
    };
    let result = compare_folders(&dir.join("left"), &dir.join("right"), &opts).unwrap();

    let symlink_entries: Vec<_> = result
        .entries
        .iter()
        .filter(|e| e.entry_type == FolderEntryType::Symlink)
        .collect();

    assert!(
        !symlink_entries.is_empty(),
        "SpecialFile policy must still enumerate symlink entries"
    );

    for entry in &symlink_entries {
        assert_ne!(
            entry.state,
            FolderEntryState::Error,
            "SpecialFile policy must not produce Error entries for symlinks (entry: {})",
            entry.name
        );
    }
}
