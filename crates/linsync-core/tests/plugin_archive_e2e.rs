// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// End-to-end integration tests for the bundled ZIP and tar unpack plugins.
// These tests invoke the real shell scripts against real fixture archives.
//
// Requirements: python3, zip, tar on PATH.  Tests are skipped automatically
// when any required tool is absent so CI without those tools does not fail.

mod common;

use linsync_core::plugin::{
    PluginExecutionOptions, PluginManifest, extract_archive_member, run_unpack_folder_plugin,
};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Once;
use std::time::Duration;

static ARCHIVE_BUILD: Once = Once::new();

/// Build fixture archives into `tests/fixtures/archive/`, exactly once per test
/// process. Requires bash, zip, and tar.
///
/// `build.sh` rewrites `sample.zip`/`sample.tar` (and its source tree) in place.
/// Both the zip and tar tests call this, and each then spawns a sandboxed helper
/// that opens the archive under a Landlock path rule. If a second `build.sh`
/// rebuilt the archive while the first test's helper still had it open, that
/// read could fail with EACCES (the pinned inode is unlinked and replaced) —
/// flaky under parallel load. Building once keeps the archives stable and
/// read-only for the lifetime of the process, removing the rewrite-vs-read race.
fn build_fixtures(archive_dir: &PathBuf) {
    ARCHIVE_BUILD.call_once(|| {
        let build_sh = archive_dir.join("build.sh");
        let status = Command::new("bash")
            .arg(&build_sh)
            .arg(archive_dir)
            .status()
            .expect("failed to launch build.sh");
        assert!(status.success(), "build.sh failed with status: {status}");
    });
}

fn archive_dir() -> PathBuf {
    common::workspace_root().join("tests/fixtures/archive")
}

fn plugin_execution_options() -> PluginExecutionOptions {
    PluginExecutionOptions {
        timeout: Duration::from_secs(15),
        ..PluginExecutionOptions::default()
    }
}

fn load_plugin_manifest(plugin_dir: &std::path::Path) -> PluginManifest {
    let manifest_text = std::fs::read_to_string(plugin_dir.join("linsync-plugin.json")).unwrap();
    serde_json::from_str::<PluginManifest>(&manifest_text).expect("manifest deserialization failed")
}

#[test]
fn zip_unpack_plugin_lists_archive_contents() {
    if !common::tools_available(&["python3", "zip", "bash"]) {
        eprintln!("SKIP: python3, zip, or bash not on PATH");
        return;
    }

    let dir = archive_dir();
    build_fixtures(&dir);

    let plugin_dir = common::workspace_root().join("packaging/plugins/zip-unpacker");
    let manifest = load_plugin_manifest(&plugin_dir);

    let source = dir.join("sample.zip");
    let result = run_unpack_folder_plugin(
        &plugin_dir,
        &manifest,
        source.to_str().unwrap(),
        &plugin_execution_options(),
    )
    .expect("run_unpack_folder_plugin failed");

    assert!(result.ok, "expected ok:true, error={:?}", result.error);

    let has_alpha = result
        .tree
        .iter()
        .any(|n| n.path == "alpha.txt" && n.kind == "file");
    let has_beta = result
        .tree
        .iter()
        .any(|n| n.path == "sub/beta.txt" && n.kind == "file");
    let has_gamma = result
        .tree
        .iter()
        .any(|n| n.path == "sub/gamma.txt" && n.kind == "file");
    let has_sub_dir = result
        .tree
        .iter()
        .any(|n| n.path == "sub" && n.kind == "dir");

    assert!(
        has_alpha,
        "expected alpha.txt in tree, got: {:?}",
        result.tree
    );
    assert!(
        has_beta,
        "expected sub/beta.txt in tree, got: {:?}",
        result.tree
    );
    assert!(
        has_gamma,
        "expected sub/gamma.txt in tree, got: {:?}",
        result.tree
    );
    assert!(
        has_sub_dir,
        "expected sub/ dir in tree, got: {:?}",
        result.tree
    );
}

#[test]
fn zip_extract_member_returns_file_content() {
    if !common::tools_available(&["python3", "zip", "bash"]) {
        eprintln!("SKIP: python3, zip, or bash not on PATH");
        return;
    }
    let dir = archive_dir();
    build_fixtures(&dir);

    let plugin_dir = common::workspace_root().join("packaging/plugins/zip-unpacker");
    let manifest = load_plugin_manifest(&plugin_dir);
    let source = dir.join("sample.zip");
    let out_dir =
        std::env::temp_dir().join(format!("linsync-extract-member-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);

    // Extract a nested member and confirm its content.
    let extracted = extract_archive_member(
        &plugin_dir,
        &manifest,
        source.to_str().unwrap(),
        "sub/beta.txt",
        &out_dir,
        &plugin_execution_options(),
    )
    .expect("extract_archive_member failed");
    assert_eq!(std::fs::read_to_string(&extracted).unwrap(), "beta\n");

    // A missing member surfaces an error rather than a bogus file.
    let missing = extract_archive_member(
        &plugin_dir,
        &manifest,
        source.to_str().unwrap(),
        "does/not/exist.txt",
        &out_dir,
        &plugin_execution_options(),
    );
    assert!(missing.is_err(), "extracting a missing member should error");

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn tar_extract_member_returns_file_content() {
    if !common::tools_available(&["python3", "tar", "bash"]) {
        eprintln!("SKIP: python3, tar, or bash not on PATH");
        return;
    }
    let dir = archive_dir();
    build_fixtures(&dir);

    let plugin_dir = common::workspace_root().join("packaging/plugins/tar-unpacker");
    let manifest = load_plugin_manifest(&plugin_dir);
    let source = dir.join("sample.tar");
    let out_dir = std::env::temp_dir().join(format!("linsync-tar-extract-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);

    let extracted = extract_archive_member(
        &plugin_dir,
        &manifest,
        source.to_str().unwrap(),
        "sub/gamma.txt",
        &out_dir,
        &plugin_execution_options(),
    )
    .expect("tar extract_archive_member failed");
    assert_eq!(std::fs::read_to_string(&extracted).unwrap(), "gamma\n");
    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn tar_unpack_plugin_lists_archive_contents() {
    if !common::tools_available(&["python3", "tar", "bash"]) {
        eprintln!("SKIP: python3, tar, or bash not on PATH");
        return;
    }

    let dir = archive_dir();
    build_fixtures(&dir);

    let plugin_dir = common::workspace_root().join("packaging/plugins/tar-unpacker");
    let manifest = load_plugin_manifest(&plugin_dir);

    let source = dir.join("sample.tar");
    let result = run_unpack_folder_plugin(
        &plugin_dir,
        &manifest,
        source.to_str().unwrap(),
        &plugin_execution_options(),
    )
    .expect("run_unpack_folder_plugin failed");

    assert!(result.ok, "expected ok:true, error={:?}", result.error);

    let has_alpha = result
        .tree
        .iter()
        .any(|n| n.path == "alpha.txt" && n.kind == "file");
    let has_beta = result
        .tree
        .iter()
        .any(|n| n.path == "sub/beta.txt" && n.kind == "file");
    let has_gamma = result
        .tree
        .iter()
        .any(|n| n.path == "sub/gamma.txt" && n.kind == "file");
    let has_sub_dir = result
        .tree
        .iter()
        .any(|n| n.path == "sub" && n.kind == "dir");

    assert!(
        has_alpha,
        "expected alpha.txt in tree, got: {:?}",
        result.tree
    );
    assert!(
        has_beta,
        "expected sub/beta.txt in tree, got: {:?}",
        result.tree
    );
    assert!(
        has_gamma,
        "expected sub/gamma.txt in tree, got: {:?}",
        result.tree
    );
    assert!(
        has_sub_dir,
        "expected sub/ dir in tree, got: {:?}",
        result.tree
    );
}

#[test]
fn zip_plugin_manifest_deserializes() {
    let plugin_dir = common::workspace_root().join("packaging/plugins/zip-unpacker");
    let manifest = load_plugin_manifest(&plugin_dir);
    assert_eq!(manifest.id, "com.visorcraft.zip-unpacker");
    assert!(!manifest.capabilities.is_empty());
    manifest
        .validate(&plugin_dir)
        .expect("manifest.validate() failed");
}

#[test]
fn tar_plugin_manifest_deserializes() {
    let plugin_dir = common::workspace_root().join("packaging/plugins/tar-unpacker");
    let manifest = load_plugin_manifest(&plugin_dir);
    assert_eq!(manifest.id, "com.visorcraft.tar-unpacker");
    assert!(!manifest.capabilities.is_empty());
    manifest
        .validate(&plugin_dir)
        .expect("manifest.validate() failed");
}
