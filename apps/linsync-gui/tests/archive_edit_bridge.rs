// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

use linsync::test_support::{commit_archive_member_edit_test, extract_archive_member_for_test};
use sha2::Digest;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn make_zip(dir: &TempDir, entries: &[(String, String)]) -> PathBuf {
    let zip_path = dir.path().join("test.zip");
    for (name, content) in entries {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
    }
    let mut cmd = Command::new("zip");
    cmd.arg("-q").arg(&zip_path);
    for (name, _) in entries {
        cmd.arg(name);
    }
    cmd.current_dir(dir.path());
    let status = cmd.status().expect("zip command should be available");
    assert!(status.success(), "zip command failed");
    zip_path
}

#[test]
fn archive_member_edit_extracts_and_commit_repacks() {
    let dir = TempDir::new().unwrap();
    let zip = make_zip(
        &dir,
        &[
            ("readme.txt".to_owned(), "hello world".to_owned()),
            ("data/info.json".to_owned(), r#"{"key":"value"}"#.to_owned()),
        ],
    );

    let staging = dir.path().join("staging");
    let (ctx, staged) = extract_archive_member_for_test(&zip, "readme.txt", &staging)
        .expect("extract should succeed");
    assert!(staged.exists(), "staged file should exist");
    assert_eq!(fs::read_to_string(&staged).unwrap(), "hello world");

    // Edit the staged file
    fs::write(&staged, "hello modified world").unwrap();

    let outcome = commit_archive_member_edit_test(
        &ctx, false, // don't keep backup
    )
    .expect("commit should succeed");
    assert!(outcome.bak_path.is_none(), "backup should not be kept");

    // Verify the zip was updated
    let mut cmd = Command::new("unzip");
    cmd.arg("-p").arg(&zip).arg("readme.txt");
    let output = cmd.output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "hello modified world",
        "zip member should contain the edited content"
    );

    // Verify the other member is untouched
    let mut cmd = Command::new("unzip");
    cmd.arg("-p").arg(&zip).arg("data/info.json");
    let output = cmd.output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        r#"{"key":"value"}"#,
        "other member should be untouched"
    );
}

#[test]
fn archive_member_commit_on_unchanged_file_preserves_byte_identity() {
    let dir = TempDir::new().unwrap();
    let zip = make_zip(
        &dir,
        &[("file.txt".to_owned(), "original content".to_owned())],
    );
    let original_hash = {
        let mut hasher = sha2::Sha256::new();
        hasher.update(fs::read(&zip).unwrap());
        hasher.finalize()
    };

    let staging = dir.path().join("staging");
    let (ctx, staged) = extract_archive_member_for_test(&zip, "file.txt", &staging)
        .expect("extract should succeed");

    // Do NOT modify the staged file

    let outcome = commit_archive_member_edit_test(
        &ctx, false, // don't keep backup
    )
    .expect("commit should succeed");

    // The archive should be byte-identical (no .bak kept)
    let new_hash = {
        let mut hasher = sha2::Sha256::new();
        hasher.update(fs::read(&zip).unwrap());
        hasher.finalize()
    };
    assert_eq!(original_hash.as_slice(), new_hash.as_slice());
}

#[test]
fn archive_member_edit_rejects_invalid_member_names() {
    let dir = TempDir::new().unwrap();
    let zip = make_zip(&dir, &[("file.txt".to_owned(), "content".to_owned())]);
    let staging = dir.path().join("staging");

    // Path traversal attempt
    let err = extract_archive_member_for_test(&zip, "../etc/passwd", &staging)
        .expect_err("should reject path traversal");
    assert!(
        err.contains("invalid member path") || err.contains("not found"),
        "error should mention invalid path: {err}"
    );
}

#[test]
fn archive_member_edit_rejects_nonexistent_member() {
    let dir = TempDir::new().unwrap();
    let zip = make_zip(&dir, &[("file.txt".to_owned(), "content".to_owned())]);
    let staging = dir.path().join("staging");

    let err = extract_archive_member_for_test(&zip, "missing.txt", &staging)
        .expect_err("should reject missing member");
    assert!(
        err.contains("not found") || err.contains("MemberNotFound"),
        "error should mention not found: {err}"
    );
}
