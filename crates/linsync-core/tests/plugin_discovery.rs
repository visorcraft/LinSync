// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

use linsync_core::plugin::{
    CURRENT_PLUGIN_SCHEMA_VERSION, PLUGIN_MANIFEST_FILE, PluginClass, PluginManifest,
    PluginSandbox, discover_plugins,
};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_root(name: &str) -> std::path::PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("linsync-plugin-discovery-{name}-{pid}-{id}"));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_manifest(plugin_dir: &std::path::Path, manifest: &PluginManifest) {
    fs::create_dir_all(plugin_dir).unwrap();
    let text = serde_json::to_string_pretty(manifest).unwrap();
    fs::write(plugin_dir.join(PLUGIN_MANIFEST_FILE), text).unwrap();
}

fn make_manifest(id: &str, name: &str, version: &str) -> PluginManifest {
    PluginManifest {
        schema_version: CURRENT_PLUGIN_SCHEMA_VERSION,
        id: id.to_owned(),
        name: name.to_owned(),
        version: version.to_owned(),
        license: "MIT".to_owned(),
        entry: vec!["run.sh".to_owned()],
        classes: vec![PluginClass::Unpacker],
        mime_types: vec![],
        extensions: vec![],
        capabilities: vec![],
        deterministic: false,
        sandbox: PluginSandbox::default(),
        streaming: false,
        options_schema: vec![],
        normalization_categories: vec![],
    }
}

#[test]
fn discovers_plugins_from_multiple_roots() {
    let root = temp_root("discover");
    let user_dir = root.join("user-plugins");
    let system_dir = root.join("system-plugins");

    write_manifest(
        &user_dir.join("zip-unpack"),
        &make_manifest("zip-unpack", "Zip Unpacker", "1.0.0"),
    );
    write_manifest(
        &system_dir.join("tar-unpack"),
        &make_manifest("tar-unpack", "Tar Unpacker", "1.0.0"),
    );

    let discovery = discover_plugins(&[user_dir.clone(), system_dir.clone()]);
    let ids: Vec<_> = discovery
        .plugins
        .iter()
        .map(|p| p.manifest.id.clone())
        .collect();
    assert!(
        ids.contains(&"zip-unpack".to_string()),
        "expected zip-unpack in {ids:?}"
    );
    assert!(
        ids.contains(&"tar-unpack".to_string()),
        "expected tar-unpack in {ids:?}"
    );
    assert!(
        discovery.errors.is_empty(),
        "unexpected errors: {:?}",
        discovery.errors
    );
}

#[test]
fn first_root_wins_when_same_id_in_multiple_roots() {
    let root = temp_root("shadow");
    let user_dir = root.join("user");
    let system_dir = root.join("system");

    write_manifest(
        &user_dir.join("p"),
        &make_manifest("p", "Plugin P", "2.0.0"),
    );
    write_manifest(
        &system_dir.join("p"),
        &make_manifest("p", "Plugin P", "1.0.0"),
    );

    let discovery = discover_plugins(&[user_dir.clone(), system_dir.clone()]);
    let p = discovery
        .plugins
        .iter()
        .find(|m| m.manifest.id == "p")
        .expect("plugin 'p' should be discovered");
    assert_eq!(
        p.manifest.version, "2.0.0",
        "first root (user_dir, version 2.0.0) should win"
    );
    // The duplicate from the system dir is reported as a discovery error.
    assert!(
        discovery
            .errors
            .iter()
            .any(|e| e.message.contains("duplicate plugin id")),
        "expected a duplicate-id error, got: {:?}",
        discovery.errors
    );
}

#[test]
fn skips_directories_without_manifest() {
    let root = temp_root("no-manifest");
    let plugins_dir = root.join("plugins");
    // A sub-dir that has no manifest.json / linsync-plugin.json.
    fs::create_dir_all(plugins_dir.join("empty-plugin")).unwrap();

    let discovery = discover_plugins(std::slice::from_ref(&plugins_dir));
    assert!(
        discovery.plugins.is_empty(),
        "should not discover any plugin without a manifest"
    );
    assert!(
        discovery.errors.is_empty(),
        "missing manifest is silently skipped, not an error"
    );
}

#[test]
fn missing_root_is_silently_skipped() {
    let nonexistent = std::env::temp_dir().join("linsync-plugin-nonexistent-root-XXXXXX");
    let discovery = discover_plugins(&[nonexistent]);
    assert!(discovery.plugins.is_empty());
    assert!(discovery.errors.is_empty());
}
