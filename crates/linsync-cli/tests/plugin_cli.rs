// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// Integration tests for the `linsync-cli plugin` subcommand: discovery
// listing, enable/disable persistence, and schema-validated option set/clear.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_linsync-cli"))
}

/// Isolated XDG dirs so discovery + the plugin store point at a scratch dir
/// and never touch the developer's real plugins.
fn run_isolated(home: &Path, args: &[&str]) -> Output {
    Command::new(bin())
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("HOME", home)
        .args(args)
        .output()
        .expect("run linsync-cli")
}

fn temp_home(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "linsync-cli-plugin-test-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Install a fixture plugin (with a bool + enum option schema) into the
/// isolated user plugins dir so `discover_installed_plugins` finds it.
fn install_fixture_plugin(home: &Path) -> &'static str {
    let plugin_dir = home.join("data/linsync/plugins/optplugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(plugin_dir.join("helper.sh"), "#!/bin/sh\n").unwrap();
    let manifest = r#"{
      "schema_version": 1,
      "id": "test.optplugin",
      "name": "Option Fixture",
      "version": "1.0.0",
      "license": "GPL-3.0-only",
      "entry": ["./helper.sh"],
      "classes": ["prediffer"],
      "mime_types": ["text/plain"],
      "extensions": ["txt"],
      "capabilities": [],
      "deterministic": true,
      "sandbox": { "network": false, "writes_input": false, "requires_home_access": false },
      "options_schema": [
        { "key": "strip_comments", "label": "Strip comments", "kind": "bool", "default": false },
        { "key": "language", "label": "Language", "kind": "enum", "choices": ["eng", "fra"], "default": "eng" }
      ]
    }"#;
    fs::write(plugin_dir.join("linsync-plugin.json"), manifest).unwrap();
    "test.optplugin"
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn plugin_list_shows_installed_fixture_with_options() {
    let home = temp_home("list");
    let id = install_fixture_plugin(&home);

    let out = run_isolated(&home, &["plugin", "list"]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = stdout(&out);
    assert!(
        text.contains(id),
        "list should show the fixture id; got:\n{text}"
    );
    assert!(
        text.contains("[options]"),
        "fixture declares options; got:\n{text}"
    );

    let out = run_isolated(&home, &["plugin", "list", "--json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let plugins = json["plugins"].as_array().unwrap();
    let entry = plugins
        .iter()
        .find(|p| p["id"] == id)
        .expect("fixture in JSON");
    assert_eq!(entry["enabled"], serde_json::json!(true), "default enabled");
    assert_eq!(entry["has_options"], serde_json::json!(true));
}

#[test]
fn plugin_enable_disable_persists() {
    let home = temp_home("enable");
    let id = install_fixture_plugin(&home);

    let out = run_isolated(&home, &["plugin", "disable", id]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = run_isolated(&home, &["plugin", "list", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let entry = json["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap()
        .clone();
    assert_eq!(
        entry["enabled"],
        serde_json::json!(false),
        "disable should persist"
    );

    run_isolated(&home, &["plugin", "enable", id]);
    let out = run_isolated(&home, &["plugin", "inspect", id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(json["enabled"], serde_json::json!(true));
}

#[test]
fn plugin_set_option_validates_against_schema() {
    let home = temp_home("setopt");
    let id = install_fixture_plugin(&home);

    // Valid enum value persists.
    let out = run_isolated(&home, &["plugin", "set-option", id, "language", "fra"]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = run_isolated(&home, &["plugin", "inspect", id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(json["values"]["language"], serde_json::json!("fra"));

    // Valid bool (parsed as JSON true, not the string "true").
    let out = run_isolated(
        &home,
        &["plugin", "set-option", id, "strip_comments", "true"],
    );
    assert!(out.status.success());
    let out = run_isolated(&home, &["plugin", "inspect", id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(json["values"]["strip_comments"], serde_json::json!(true));

    // Invalid enum choice is rejected (non-zero exit), nothing persisted over it.
    let out = run_isolated(&home, &["plugin", "set-option", id, "language", "klingon"]);
    assert!(!out.status.success(), "invalid enum should fail");
    assert!(String::from_utf8_lossy(&out.stderr).contains("not one of"));

    // Unknown option key is rejected.
    let out = run_isolated(&home, &["plugin", "set-option", id, "nope", "1"]);
    assert!(!out.status.success(), "unknown option should fail");

    // The earlier valid value survived the rejected writes.
    let out = run_isolated(&home, &["plugin", "inspect", id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(json["values"]["language"], serde_json::json!("fra"));

    // validate reports the persisted options as valid.
    let out = run_isolated(&home, &["plugin", "validate", id]);
    assert!(out.status.success());

    // Clearing removes the key.
    run_isolated(&home, &["plugin", "clear-option", id, "language"]);
    let out = run_isolated(&home, &["plugin", "inspect", id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert!(
        json["values"].get("language").is_none(),
        "cleared key should be gone"
    );
}

#[test]
fn plugin_inspect_unknown_id_errors() {
    let home = temp_home("unknown");
    let out = run_isolated(&home, &["plugin", "inspect", "does.not.exist"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no installed plugin"));
}
