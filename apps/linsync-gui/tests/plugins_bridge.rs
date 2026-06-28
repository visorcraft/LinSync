// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

use linsync::test_support::{
    load_plugin_enabled_map, load_plugin_options, save_plugin_enabled, save_plugin_option,
    temp_app_paths, write_fixture_plugin,
};

// ─── Plugin enable-map persistence ───────────────────────────────────────────

#[test]
fn plugin_enabled_defaults_to_absent_map() {
    let paths = temp_app_paths("plugin-enabled-empty");
    let map = load_plugin_enabled_map(&paths);
    assert!(map.is_empty(), "no plugins.json should yield empty map");
}

#[test]
fn plugin_enabled_round_trip() {
    let paths = temp_app_paths("plugin-enabled-rt");
    save_plugin_enabled(&paths, "example.zip-unpack", false).expect("save should succeed");
    let map = load_plugin_enabled_map(&paths);
    assert_eq!(
        map.get("example.zip-unpack"),
        Some(&false),
        "persisted disabled state should reload as false"
    );
}

#[test]
fn plugin_enabled_re_enable_round_trip() {
    let paths = temp_app_paths("plugin-enabled-re");
    save_plugin_enabled(&paths, "example.normalizer", false).expect("disable should succeed");
    save_plugin_enabled(&paths, "example.normalizer", true).expect("re-enable should succeed");
    let map = load_plugin_enabled_map(&paths);
    assert_eq!(
        map.get("example.normalizer"),
        Some(&true),
        "re-enabled plugin should read back as true"
    );
}

#[test]
fn plugin_enabled_multiple_plugins_persist_independently() {
    let paths = temp_app_paths("plugin-enabled-multi");
    save_plugin_enabled(&paths, "plugin.a", true).expect("save a should succeed");
    save_plugin_enabled(&paths, "plugin.b", false).expect("save b should succeed");
    let map = load_plugin_enabled_map(&paths);
    assert_eq!(map.get("plugin.a"), Some(&true));
    assert_eq!(map.get("plugin.b"), Some(&false));
}

// ─── Fixture plugin write helper ──────────────────────────────────────────────

#[test]
fn write_fixture_plugin_creates_discoverable_manifest() {
    let paths = temp_app_paths("plugin-fixture");
    let plugins_dir = paths.user_plugins_dir();
    let plugin_dir = write_fixture_plugin(&plugins_dir, "test.normalizer", "Test Normalizer");
    assert!(
        plugin_dir
            .join(linsync_core::plugin::PLUGIN_MANIFEST_FILE)
            .is_file(),
        "manifest file should exist at {}",
        plugin_dir.display()
    );
    let discovery = linsync_core::discover_installed_plugins(&paths);
    assert!(
        discovery
            .plugins
            .iter()
            .any(|p| p.manifest.id == "test.normalizer"),
        "installed discovery should find test.normalizer; got: {:?}",
        discovery
            .plugins
            .iter()
            .map(|p| &p.manifest.id)
            .collect::<Vec<_>>()
    );
}

// ─── Plugin options persistence ───────────────────────────────────────────────

#[test]
fn plugin_options_defaults_to_empty_map() {
    let paths = temp_app_paths("plugin-opts-empty");
    let values = load_plugin_options(&paths, "any.plugin");
    assert!(values.is_empty(), "missing file should yield empty map");
}

#[test]
fn plugin_options_round_trip() {
    let paths = temp_app_paths("plugin-opts-rt");
    save_plugin_option(&paths, "example.opts", "level", serde_json::json!(7))
        .expect("save should succeed");
    let values = load_plugin_options(&paths, "example.opts");
    assert_eq!(
        values.get("level").and_then(|v| v.as_i64()),
        Some(7),
        "persisted level should reload as 7"
    );
}

#[test]
fn plugin_options_multiple_keys_persist_independently() {
    let paths = temp_app_paths("plugin-opts-multi");
    save_plugin_option(&paths, "plugin.multi", "alpha", serde_json::json!("fast"))
        .expect("save alpha should succeed");
    save_plugin_option(&paths, "plugin.multi", "beta", serde_json::json!(true))
        .expect("save beta should succeed");
    let values = load_plugin_options(&paths, "plugin.multi");
    assert_eq!(values.get("alpha").and_then(|v| v.as_str()), Some("fast"));
    assert_eq!(values.get("beta").and_then(|v| v.as_bool()), Some(true));
}

#[test]
fn plugin_options_isolated_per_plugin_id() {
    let paths = temp_app_paths("plugin-opts-isolated");
    save_plugin_option(&paths, "plugin.a", "key", serde_json::json!(1))
        .expect("save plugin.a should succeed");
    save_plugin_option(&paths, "plugin.b", "key", serde_json::json!(2))
        .expect("save plugin.b should succeed");
    let a = load_plugin_options(&paths, "plugin.a");
    let b = load_plugin_options(&paths, "plugin.b");
    assert_eq!(a.get("key").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(b.get("key").and_then(|v| v.as_i64()), Some(2));
}
