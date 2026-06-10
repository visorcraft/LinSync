// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// Integration tests for the writable archive-member editing path
// (`linsync_core::archive_write`): single-member zip repack per
// docs/archive-write-safety-design.md.
//
// Requirements: zip, unzip on PATH. Tests skip automatically when either
// tool is absent so CI without those tools does not fail.

mod common;

use linsync_core::archive_write::{
    ArchiveEditCaps, ArchiveWriteError, CommitOptions, commit_member_edit, extract_member_for_edit,
    extract_member_for_edit_with_caps, validate_member_path, verify_post_repack_listing,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const TOOLS: &[&str] = &["zip", "unzip"];

/// Build a zip archive at `dir/test.zip` from `(member, content, mode)` triples.
/// Members may contain `/` separators; parent dirs are created and zipped
/// recursively so directory entries are present (like real-world archives).
fn make_zip(dir: &Path, entries: &[(&str, &[u8], u32)]) -> PathBuf {
    let src = dir.join("zip-src");
    fs::create_dir_all(&src).unwrap();
    for (member, content, mode) in entries {
        let path = src.join(member);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(*mode)).unwrap();
    }
    let archive = dir.join("test.zip");
    let status = Command::new("zip")
        .arg("-q")
        .arg("-r")
        .arg(&archive)
        .arg(".")
        .current_dir(&src)
        .status()
        .expect("failed to launch zip");
    assert!(status.success(), "zip fixture build failed: {status}");
    archive
}

/// Extract one member's bytes via `unzip -p`.
fn member_bytes(archive: &Path, member: &str) -> Vec<u8> {
    let output = Command::new("unzip")
        .arg("-p")
        .arg(archive)
        .arg(member)
        .output()
        .expect("failed to launch unzip");
    assert!(
        output.status.success(),
        "unzip -p {} {member} failed: {}",
        archive.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// Build a minimal single-entry zip (stored, empty content) whose member name
/// is an arbitrary byte string with the UTF-8 general-purpose flag controlled
/// by `utf8_flag`. Empty content means CRC-32 = 0 and sizes = 0, so no
/// compression library is needed.
fn make_raw_name_zip(path: &Path, name_bytes: &[u8], utf8_flag: bool) {
    let flags: u16 = if utf8_flag { 0x0800 } else { 0 };
    let name_len = name_bytes.len() as u16;
    let mut buf: Vec<u8> = Vec::new();
    // Local file header.
    buf.extend_from_slice(&0x04034b50u32.to_le_bytes());
    buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
    buf.extend_from_slice(&flags.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // method: stored
    buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
    buf.extend_from_slice(&0x21u16.to_le_bytes()); // mod date
    buf.extend_from_slice(&0u32.to_le_bytes()); // crc-32 (empty)
    buf.extend_from_slice(&0u32.to_le_bytes()); // compressed size
    buf.extend_from_slice(&0u32.to_le_bytes()); // uncompressed size
    buf.extend_from_slice(&name_len.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // extra len
    buf.extend_from_slice(name_bytes);
    let cd_offset = buf.len() as u32;
    // Central directory entry. "Version made by" is Unix (3 << 8) so Info-ZIP
    // honors the UTF-8 flag when set, with regular-file external attributes.
    buf.extend_from_slice(&0x02014b50u32.to_le_bytes());
    buf.extend_from_slice(&0x031eu16.to_le_bytes()); // version made by (Unix)
    buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
    buf.extend_from_slice(&flags.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // method
    buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
    buf.extend_from_slice(&0x21u16.to_le_bytes()); // mod date
    buf.extend_from_slice(&0u32.to_le_bytes()); // crc-32
    buf.extend_from_slice(&0u32.to_le_bytes()); // compressed size
    buf.extend_from_slice(&0u32.to_le_bytes()); // uncompressed size
    buf.extend_from_slice(&name_len.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // extra len
    buf.extend_from_slice(&0u16.to_le_bytes()); // comment len
    buf.extend_from_slice(&0u16.to_le_bytes()); // disk number
    buf.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
    buf.extend_from_slice(&(0o100644u32 << 16).to_le_bytes()); // external attrs: regular file
    buf.extend_from_slice(&0u32.to_le_bytes()); // local header offset
    buf.extend_from_slice(name_bytes);
    let cd_size = buf.len() as u32 - cd_offset;
    // End of central directory.
    buf.extend_from_slice(&0x06054b50u32.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // disk
    buf.extend_from_slice(&0u16.to_le_bytes()); // cd disk
    buf.extend_from_slice(&1u16.to_le_bytes()); // entries on disk
    buf.extend_from_slice(&1u16.to_le_bytes()); // total entries
    buf.extend_from_slice(&cd_size.to_le_bytes());
    buf.extend_from_slice(&cd_offset.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // comment len
    fs::write(path, buf).unwrap();
}

fn tmp_sibling(archive: &Path) -> PathBuf {
    let mut name = archive.file_name().unwrap().to_os_string();
    name.push(".linsync-tmp");
    archive.with_file_name(name)
}

fn bak_sibling(archive: &Path) -> PathBuf {
    let mut name = archive.file_name().unwrap().to_os_string();
    name.push(".bak");
    archive.with_file_name(name)
}

#[test]
fn archive_edit_commit_replaces_member_atomically() {
    if !common::tools_available(TOOLS) {
        eprintln!("SKIP: zip or unzip not on PATH");
        return;
    }
    let dir = TempDir::new().unwrap();
    let archive = make_zip(
        dir.path(),
        &[
            ("a.txt", b"alpha\n", 0o644),
            ("dir/b.txt", b"bravo\n", 0o755),
            ("c.bin", b"\x00\x01\x02binary", 0o600),
        ],
    );
    let original_bytes = fs::read(&archive).unwrap();

    // First commit: default options (backup deleted on success).
    let staging1 = dir.path().join("staging-1");
    let ctx = extract_member_for_edit(&archive, "dir/b.txt", &staging1, None)
        .expect("extract_member_for_edit failed");
    assert_eq!(
        fs::read(ctx.staged_path()).unwrap(),
        b"bravo\n",
        "staged copy must hold the member's original bytes"
    );
    fs::write(ctx.staged_path(), b"BRAVO EDITED\n").unwrap();
    let outcome =
        commit_member_edit(&ctx, &CommitOptions::default()).expect("commit_member_edit failed");
    assert_eq!(
        outcome.bak_path, None,
        "default commit must not retain .bak"
    );
    assert!(
        !bak_sibling(&archive).exists(),
        ".bak must be deleted on success by default"
    );
    assert!(
        !tmp_sibling(&archive).exists(),
        ".linsync-tmp must not survive a successful commit"
    );
    assert_eq!(member_bytes(&archive, "dir/b.txt"), b"BRAVO EDITED\n");
    // Untouched members are byte-identical to the original archive's.
    assert_eq!(member_bytes(&archive, "a.txt"), b"alpha\n");
    assert_eq!(member_bytes(&archive, "c.bin"), b"\x00\x01\x02binary");

    // Mode preserved: the edited member keeps its original 0755.
    let modecheck = dir.path().join("modecheck");
    fs::create_dir_all(&modecheck).unwrap();
    let status = Command::new("unzip")
        .arg("-q")
        .arg(&archive)
        .arg("dir/b.txt")
        .arg("-d")
        .arg(&modecheck)
        .status()
        .unwrap();
    assert!(status.success());
    let mode = fs::metadata(modecheck.join("dir/b.txt"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o777,
        0o755,
        "member mode must round-trip the repack"
    );

    // Second commit: keep_backup retains a .bak equal to the pre-commit bytes.
    let before_second = fs::read(&archive).unwrap();
    assert_ne!(before_second, original_bytes);
    let staging2 = dir.path().join("staging-2");
    let ctx2 = extract_member_for_edit(&archive, "a.txt", &staging2, None).expect("second extract");
    fs::write(ctx2.staged_path(), b"ALPHA v2\n").unwrap();
    let outcome2 = commit_member_edit(&ctx2, &CommitOptions { keep_backup: true })
        .expect("second commit failed");
    let bak = outcome2.bak_path.expect("keep_backup must report bak path");
    assert_eq!(bak, bak_sibling(&archive));
    assert_eq!(
        fs::read(&bak).unwrap(),
        before_second,
        ".bak must be byte-identical to the archive as it was before the commit"
    );
    assert_eq!(member_bytes(&archive, "a.txt"), b"ALPHA v2\n");
}

#[test]
fn archive_edit_rejects_zip_slip_member_paths() {
    if !common::tools_available(TOOLS) {
        eprintln!("SKIP: zip or unzip not on PATH");
        return;
    }
    let dir = TempDir::new().unwrap();
    let archive = make_zip(dir.path(), &[("a.txt", b"alpha\n", 0o644)]);

    let bad = [
        "../evil",
        "a/../../evil",
        "/abs",
        "C:/windows",
        "c:\\windows",
        "@list",
        "-flag",
        "nul\0byte",
        "back\\slash",
        "trailing/",
        "",
    ];
    for member in bad {
        let staging = dir.path().join("staging");
        let err = extract_member_for_edit(&archive, member, &staging, None)
            .expect_err(&format!("member {member:?} must be rejected"));
        assert!(
            matches!(err, ArchiveWriteError::InvalidMemberName { .. }),
            "member {member:?}: expected InvalidMemberName, got {err:?}"
        );
        assert!(
            !staging.exists(),
            "member {member:?}: nothing may be staged on rejection"
        );
        // The pure validator agrees (this is the function the bridge maps to 400).
        assert!(validate_member_path(member).is_err());
    }
}

#[test]
fn archive_edit_commit_rejects_stale_fingerprint() {
    if !common::tools_available(TOOLS) {
        eprintln!("SKIP: zip or unzip not on PATH");
        return;
    }
    let dir = TempDir::new().unwrap();
    let archive = make_zip(
        dir.path(),
        &[("a.txt", b"alpha\n", 0o644), ("b.txt", b"bravo\n", 0o644)],
    );
    let ctx = extract_member_for_edit(&archive, "a.txt", &dir.path().join("staging"), None)
        .expect("extract failed");
    fs::write(ctx.staged_path(), b"edited\n").unwrap();

    // Another program rewrites the archive between extract and commit.
    let src = dir.path().join("zip-src");
    fs::write(src.join("a.txt"), b"externally changed\n").unwrap();
    let status = Command::new("zip")
        .arg("-q")
        .arg("-r")
        .arg(&archive)
        .arg(".")
        .current_dir(&src)
        .status()
        .unwrap();
    assert!(status.success());
    let modified_bytes = fs::read(&archive).unwrap();

    let err = commit_member_edit(&ctx, &CommitOptions::default())
        .expect_err("commit against a rewritten archive must fail");
    assert!(
        matches!(err, ArchiveWriteError::StaleArchive { .. }),
        "expected StaleArchive, got {err:?}"
    );
    assert_eq!(
        fs::read(&archive).unwrap(),
        modified_bytes,
        "the (externally modified) original must be untouched"
    );
    assert!(
        !tmp_sibling(&archive).exists(),
        "no .linsync-tmp may be left behind on a stale-archive failure"
    );
    assert!(!bak_sibling(&archive).exists());
}

#[test]
fn archive_edit_staging_enforces_size_and_ratio_caps() {
    if !common::tools_available(TOOLS) {
        eprintln!("SKIP: zip or unzip not on PATH");
        return;
    }
    let dir = TempDir::new().unwrap();
    let big = vec![b'x'; 64 * 1024];
    // Incompressible 64 KiB (xorshift PRNG) for the default-caps control:
    // repeated bytes would trip the default 200:1 ratio cap by design.
    let mut noise = Vec::with_capacity(64 * 1024);
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    while noise.len() < 64 * 1024 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        noise.extend_from_slice(&state.to_le_bytes());
    }
    let archive = make_zip(
        dir.path(),
        &[
            ("big.bin", big.as_slice(), 0o644),
            ("noise.bin", noise.as_slice(), 0o644),
        ],
    );

    // Size cap: member larger than the cap is rejected before staging.
    let caps = ArchiveEditCaps {
        max_member_size: 1024,
        ..ArchiveEditCaps::default()
    };
    let staging = dir.path().join("staging-size");
    let err = extract_member_for_edit_with_caps(&archive, "big.bin", &staging, &caps, None)
        .expect_err("oversized member must be rejected");
    assert!(
        matches!(err, ArchiveWriteError::CapsExceeded { .. }),
        "expected CapsExceeded, got {err:?}"
    );
    assert!(!staging.exists(), "nothing may be staged when caps reject");

    // Ratio cap: 64 KiB of identical bytes deflates far beyond 2:1.
    let caps = ArchiveEditCaps {
        max_compression_ratio: 2,
        ..ArchiveEditCaps::default()
    };
    let staging = dir.path().join("staging-ratio");
    let err = extract_member_for_edit_with_caps(&archive, "big.bin", &staging, &caps, None)
        .expect_err("over-ratio member must be rejected");
    assert!(
        matches!(err, ArchiveWriteError::CapsExceeded { .. }),
        "expected CapsExceeded, got {err:?}"
    );
    assert!(!staging.exists());

    // Default caps accept an ordinary (incompressible) 64 KiB member.
    let staging = dir.path().join("staging-ok");
    extract_member_for_edit(&archive, "noise.bin", &staging, None)
        .expect("default caps must accept a 64 KiB member");
}

#[test]
fn post_repack_listing_guard_rejects_duplicates_and_count_changes() {
    // The duplicate-entry guard is hard to trigger through a real Info-ZIP
    // `zip` run (the edit-time encoding gate exists precisely to prevent it),
    // so the assertion helper is exercised directly on crafted listings, as
    // the design's testing strategy allows.
    let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    // Healthy listing passes.
    verify_post_repack_listing(&names(&["a.txt", "dir/", "dir/b.txt"]), "dir/b.txt", 3)
        .expect("healthy listing must pass");

    // Target member duplicated (count unchanged in aggregate is still caught
    // by the exactly-once rule).
    let err =
        verify_post_repack_listing(&names(&["a.txt", "dir/b.txt", "dir/b.txt"]), "dir/b.txt", 3)
            .expect_err("duplicate target member must fail the guard");
    assert!(matches!(err, ArchiveWriteError::PostRepackAssertion { .. }));

    // Member count grew (zip added instead of replaced).
    let err = verify_post_repack_listing(
        &names(&["a.txt", "dir/", "dir/b.txt", "extra"]),
        "dir/b.txt",
        3,
    )
    .expect_err("changed member count must fail the guard");
    assert!(matches!(err, ArchiveWriteError::PostRepackAssertion { .. }));

    // Target member vanished.
    let err = verify_post_repack_listing(&names(&["a.txt", "dir/", "other"]), "dir/b.txt", 3)
        .expect_err("missing target member must fail the guard");
    assert!(matches!(err, ArchiveWriteError::PostRepackAssertion { .. }));
}

#[test]
fn archive_edit_failure_leaves_original_untouched() {
    if !common::tools_available(TOOLS) {
        eprintln!("SKIP: zip or unzip not on PATH");
        return;
    }
    let dir = TempDir::new().unwrap();
    let archive_dir = dir.path().join("archives");
    fs::create_dir_all(&archive_dir).unwrap();
    let archive = make_zip(&archive_dir, &[("a.txt", b"alpha\n", 0o644)]);
    // make_zip puts its source tree next to the archive; relocate the archive
    // into a directory we can lock down without breaking TempDir cleanup.
    let locked_dir = dir.path().join("locked");
    fs::create_dir_all(&locked_dir).unwrap();
    let locked_archive = locked_dir.join("test.zip");
    fs::rename(&archive, &locked_archive).unwrap();
    let original_bytes = fs::read(&locked_archive).unwrap();

    let ctx = extract_member_for_edit(&locked_archive, "a.txt", &dir.path().join("staging"), None)
        .expect("extract failed");
    fs::write(ctx.staged_path(), b"edited\n").unwrap();

    // Read-only parent: creating `<archive>.linsync-tmp` must fail mid-commit.
    fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o555)).unwrap();
    let result = commit_member_edit(&ctx, &CommitOptions::default());
    fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o755)).unwrap();

    let err = result.expect_err("commit with unwritable parent dir must fail");
    assert!(
        matches!(err, ArchiveWriteError::Io { .. }),
        "expected a typed Io error, got {err:?}"
    );
    assert_eq!(
        fs::read(&locked_archive).unwrap(),
        original_bytes,
        "original must be byte-identical after a mid-commit failure"
    );
    assert!(!tmp_sibling(&locked_archive).exists());
    assert!(!bak_sibling(&locked_archive).exists());
}

#[test]
fn archive_edit_rejects_non_utf8_member_name() {
    if !common::tools_available(TOOLS) {
        eprintln!("SKIP: zip or unzip not on PATH");
        return;
    }
    let dir = TempDir::new().unwrap();

    // Raw cp437-style name bytes that are not valid UTF-8. The GUI would have
    // shown the lossily decoded name; asking to edit it must fail with the
    // dedicated encoding error, not MemberNotFound.
    let raw = dir.path().join("raw-name.zip");
    make_raw_name_zip(&raw, b"caf\xe9.txt", false);
    let staging = dir.path().join("staging-raw");
    let err = extract_member_for_edit(&raw, "caf\u{FFFD}.txt", &staging, None)
        .expect_err("non-UTF-8 member name must be rejected at edit time");
    assert!(
        matches!(err, ArchiveWriteError::MemberNameEncoding { .. }),
        "expected MemberNameEncoding, got {err:?}"
    );
    assert!(!staging.exists());

    // Valid UTF-8 bytes but the UTF-8 flag is unset and the name is
    // non-ASCII: Info-ZIP would re-encode it (cp437 interpretation) and add a
    // second entry instead of replacing — reject at edit time.
    let unflagged = dir.path().join("unflagged-name.zip");
    make_raw_name_zip(&unflagged, "café.txt".as_bytes(), false);
    let staging = dir.path().join("staging-unflagged");
    let err = extract_member_for_edit(&unflagged, "café.txt", &staging, None)
        .expect_err("non-ASCII name without the UTF-8 flag must be rejected");
    assert!(
        matches!(err, ArchiveWriteError::MemberNameEncoding { .. }),
        "expected MemberNameEncoding, got {err:?}"
    );
    assert!(!staging.exists());

    // Control: the same name with the UTF-8 flag set is editable (extraction
    // proceeds past the encoding gate; the entry is a regular empty file).
    let flagged = dir.path().join("flagged-name.zip");
    make_raw_name_zip(&flagged, "café.txt".as_bytes(), true);
    let staging = dir.path().join("staging-flagged");
    let ctx = extract_member_for_edit(&flagged, "café.txt", &staging, None)
        .expect("UTF-8-flagged non-ASCII name must be accepted");
    assert_eq!(fs::read(ctx.staged_path()).unwrap(), b"");
}

#[test]
fn archive_edit_rejects_symlink_member() {
    if !common::tools_available(TOOLS) {
        eprintln!("SKIP: zip or unzip not on PATH");
        return;
    }
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("zip-src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("target.txt"), b"real\n").unwrap();
    std::os::unix::fs::symlink("target.txt", src.join("link.txt")).unwrap();
    let archive = dir.path().join("symlinks.zip");
    let status = Command::new("zip")
        .arg("-q")
        .arg("-y") // store symlinks as symlinks
        .arg("-r")
        .arg(&archive)
        .arg(".")
        .current_dir(&src)
        .status()
        .unwrap();
    assert!(status.success());

    let staging = dir.path().join("staging");
    let err = extract_member_for_edit(&archive, "link.txt", &staging, None)
        .expect_err("symlink member must be rejected");
    assert!(
        matches!(err, ArchiveWriteError::NonRegularMember { .. }),
        "expected NonRegularMember, got {err:?}"
    );
    assert!(!staging.exists());
}

#[test]
fn archive_edit_rejects_symlinked_replacement_at_commit() {
    if !common::tools_available(TOOLS) {
        eprintln!("SKIP: zip or unzip not on PATH");
        return;
    }
    let dir = TempDir::new().unwrap();
    let archive = make_zip(dir.path(), &[("a.txt", b"alpha\n", 0o644)]);
    let original_bytes = fs::read(&archive).unwrap();
    let ctx = extract_member_for_edit(&archive, "a.txt", &dir.path().join("staging"), None)
        .expect("extract failed");

    // Swap the staged file for a symlink before commit (design §2: symlink as
    // repack content).
    let outside = dir.path().join("outside.txt");
    fs::write(&outside, b"exfiltrated\n").unwrap();
    fs::remove_file(ctx.staged_path()).unwrap();
    std::os::unix::fs::symlink(&outside, ctx.staged_path()).unwrap();

    let err = commit_member_edit(&ctx, &CommitOptions::default())
        .expect_err("symlinked replacement must be refused");
    assert!(
        matches!(err, ArchiveWriteError::NonRegularStagedFile { .. }),
        "expected NonRegularStagedFile, got {err:?}"
    );
    assert_eq!(
        fs::read(&archive).unwrap(),
        original_bytes,
        "original must be untouched"
    );
    assert!(!tmp_sibling(&archive).exists());
    assert!(!bak_sibling(&archive).exists());
}

#[test]
fn archive_edit_member_not_found() {
    if !common::tools_available(TOOLS) {
        eprintln!("SKIP: zip or unzip not on PATH");
        return;
    }
    let dir = TempDir::new().unwrap();
    let archive = make_zip(dir.path(), &[("a.txt", b"alpha\n", 0o644)]);
    let err = extract_member_for_edit(&archive, "missing.txt", &dir.path().join("staging"), None)
        .expect_err("unknown member must be rejected");
    assert!(
        matches!(err, ArchiveWriteError::MemberNotFound { .. }),
        "expected MemberNotFound, got {err:?}"
    );
}
