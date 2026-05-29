// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

use linsync_core::plugin::{
    CURRENT_PLUGIN_SCHEMA_VERSION, PluginClass, PluginExecutionOptions, PluginManifest,
    PluginSandbox, run_unpack_folder_plugin,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_root(tag: &str) -> std::path::PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("linsync-unpack-folder-{tag}-{pid}-{id}"));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn build_test_manifest_for_executable(script: &Path) -> PluginManifest {
    let entry_name = script
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap()
        .to_owned();
    PluginManifest {
        schema_version: CURRENT_PLUGIN_SCHEMA_VERSION,
        id: "test.unpack-folder".to_owned(),
        name: "Test Unpack Folder".to_owned(),
        version: "0.1.0".to_owned(),
        license: "GPL-3.0-only".to_owned(),
        entry: vec![entry_name],
        classes: vec![PluginClass::FolderVirtualizer],
        mime_types: vec!["application/zip".to_owned()],
        extensions: vec!["zip".to_owned()],
        capabilities: vec![],
        deterministic: true,
        sandbox: PluginSandbox::default(),
        streaming: false,
        options_schema: vec![],
    }
}

fn write_executable(dir: &Path, name: &str, script: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, script).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

#[test]
fn unpack_folder_plugin_returns_virtual_tree() {
    let dir = temp_root("ok");
    let script = write_executable(
        &dir,
        "p.sh",
        "#!/usr/bin/env bash\n\
         read REQ\n\
         echo '{\"ok\":true,\"tree\":[{\"path\":\"a/b.txt\",\"kind\":\"file\",\"sha256\":\"deadbeef\",\"size\":4},{\"path\":\"a\",\"kind\":\"dir\"}]}'\n",
    );

    let manifest = build_test_manifest_for_executable(&script);
    let result = run_unpack_folder_plugin(
        &dir,
        &manifest,
        "/tmp/whatever.zip",
        &PluginExecutionOptions {
            timeout: Duration::from_secs(5),
            ..PluginExecutionOptions::default()
        },
    )
    .unwrap();

    assert!(result.ok);
    assert_eq!(result.tree.len(), 2);
    assert!(
        result
            .tree
            .iter()
            .any(|n| n.path == "a/b.txt" && n.kind == "file")
    );
    assert!(result.tree.iter().any(|n| n.path == "a" && n.kind == "dir"));
}

#[test]
fn unpack_folder_plugin_propagates_error_field() {
    let dir = temp_root("err");
    let script = write_executable(
        &dir,
        "p.sh",
        "#!/usr/bin/env bash\n\
         read REQ\n\
         echo '{\"ok\":false,\"error\":\"unsupported format\"}'\n",
    );

    let manifest = build_test_manifest_for_executable(&script);
    let result = run_unpack_folder_plugin(
        &dir,
        &manifest,
        "/tmp/whatever.rar",
        &PluginExecutionOptions {
            timeout: Duration::from_secs(5),
            ..PluginExecutionOptions::default()
        },
    )
    .unwrap();

    assert!(!result.ok);
    assert_eq!(result.error.as_deref(), Some("unsupported format"));
    assert!(result.tree.is_empty());
}

#[test]
fn unpack_folder_plugin_empty_tree_on_ok() {
    let dir = temp_root("empty");
    let script = write_executable(
        &dir,
        "p.sh",
        "#!/usr/bin/env bash\n\
         read REQ\n\
         echo '{\"ok\":true}'\n",
    );

    let manifest = build_test_manifest_for_executable(&script);
    let result = run_unpack_folder_plugin(
        &dir,
        &manifest,
        "/tmp/empty.zip",
        &PluginExecutionOptions {
            timeout: Duration::from_secs(5),
            ..PluginExecutionOptions::default()
        },
    )
    .unwrap();

    assert!(result.ok);
    assert!(result.tree.is_empty());
}
