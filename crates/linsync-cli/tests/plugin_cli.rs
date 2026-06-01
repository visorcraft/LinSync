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

/// Like [`run_isolated`] but also degrades the sandbox so a helper actually
/// executes deterministically regardless of the host's Landlock/seccomp
/// support (the same contract CI's test job sets globally).
fn run_isolated_unsandboxed(home: &Path, args: &[&str]) -> Output {
    Command::new(bin())
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("HOME", home)
        .env("LINSYNC_SANDBOX_SKIP", "1")
        .args(args)
        .output()
        .expect("run linsync-cli")
}

/// Install a plugin whose helper echoes `response` (a probe reply) and exits
/// with `exit_code`, so `plugin run-diagnostic` can be exercised end to end.
fn install_probe_plugin(home: &Path, response: &str, exit_code: u8) -> &'static str {
    let plugin_dir = home.join("data/linsync/plugins/probe");
    fs::create_dir_all(&plugin_dir).unwrap();
    let script =
        format!("#!/bin/sh\nread request\nprintf '%s\\n' '{response}'\nexit {exit_code}\n");
    let helper = plugin_dir.join("helper.sh");
    fs::write(&helper, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&helper).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&helper, perms).unwrap();
    }
    let manifest = r#"{
      "schema_version": 1,
      "id": "test.probe",
      "name": "Probe Fixture",
      "version": "1.0.0",
      "license": "GPL-3.0-only",
      "entry": ["./helper.sh"],
      "classes": ["prediffer"],
      "mime_types": ["text/plain"],
      "extensions": ["txt"],
      "capabilities": [],
      "deterministic": true,
      "sandbox": { "network": false, "writes_input": false, "requires_home_access": false },
      "options_schema": []
    }"#;
    fs::write(plugin_dir.join("linsync-plugin.json"), manifest).unwrap();
    "test.probe"
}

/// Install a prediffer plugin whose helper lowercases each side's content, so a
/// case-only difference disappears once it runs. Exercises the profile/CLI
/// prediffer routing (`resolve_enabled_prediffer` + the prediffer compare path).
fn install_lowercasing_prediffer(home: &Path) -> &'static str {
    let plugin_dir = home.join("data/linsync/plugins/lower");
    fs::create_dir_all(&plugin_dir).unwrap();
    // Echo back the request_id (the host validates it), the input role, and the
    // file's content lowercased, as the prediffer's normalized output.
    let script = "#!/bin/sh\n\
        request=$(cat)\n\
        rid=$(printf '%s' \"$request\" | sed -n 's/.*\"request_id\":\"\\([^\"]*\\)\".*/\\1/p')\n\
        role=$(printf '%s' \"$request\" | sed -n 's/.*\"role\":\"\\([^\"]*\\)\".*/\\1/p')\n\
        path=$(printf '%s' \"$request\" | sed -n 's/.*\"path\":\"\\([^\"]*\\)\".*/\\1/p')\n\
        text=$(tr 'A-Z' 'a-z' < \"$path\")\n\
        printf '{\"protocol_version\":1,\"request_id\":\"%s\",\"status\":\"ok\",\"outputs\":[{\"role\":\"%s\",\"kind\":\"text\",\"inline_text\":\"%s\",\"encoding\":\"utf-8\",\"line_ending\":\"lf\"}],\"diagnostics\":[]}\\n' \"$rid\" \"$role\" \"$text\"\n";
    let helper = plugin_dir.join("helper.sh");
    fs::write(&helper, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&helper).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&helper, perms).unwrap();
    }
    let manifest = r#"{
      "schema_version": 1,
      "id": "test.lower",
      "name": "Lowercase Prediffer",
      "version": "1.0.0",
      "license": "GPL-3.0-only",
      "entry": ["./helper.sh"],
      "classes": ["prediffer"],
      "mime_types": ["text/plain"],
      "extensions": ["txt"],
      "capabilities": [],
      "deterministic": true,
      "sandbox": { "network": false, "writes_input": false, "requires_home_access": false },
      "options_schema": []
    }"#;
    fs::write(plugin_dir.join("linsync-plugin.json"), manifest).unwrap();
    "test.lower"
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

#[test]
fn plugin_run_diagnostic_reports_healthy_probe() {
    let home = temp_home("diag-ok");
    let response = r#"{"protocol_version":1,"request_id":"p","status":"ok","outputs":[],"diagnostics":[{"severity":"info","message":"alive"}]}"#;
    let id = install_probe_plugin(&home, response, 0);

    let out = run_isolated_unsandboxed(&home, &["plugin", "run-diagnostic", id, "--json"]);
    assert!(
        out.status.success(),
        "expected exit 0, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(json["healthy"], serde_json::json!(true));
    assert_eq!(json["exit_code"], serde_json::json!(0));
    assert_eq!(json["timed_out"], serde_json::json!(false));
    assert_eq!(json["response"]["status"], serde_json::json!("ok"));
    assert_eq!(
        json["response"]["diagnostics"][0]["message"],
        serde_json::json!("alive")
    );
}

#[test]
fn plugin_run_diagnostic_reports_failing_helper() {
    let home = temp_home("diag-fail");
    // Helper writes no valid response and exits non-zero; the diagnostic must
    // surface that as unhealthy with exit code 1 (problem, not transport error).
    let id = install_probe_plugin(&home, "not-json", 5);

    let out = run_isolated_unsandboxed(&home, &["plugin", "run-diagnostic", id, "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(json["healthy"], serde_json::json!(false));
    assert_eq!(json["exit_code"], serde_json::json!(5));
    assert!(json["response"].is_null());
}

#[test]
fn plugin_run_diagnostic_unknown_id_errors() {
    let home = temp_home("diag-unknown");
    let out = run_isolated(&home, &["plugin", "run-diagnostic", "does.not.exist"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no installed plugin"));
}

#[test]
fn compare_prediffer_normalizes_then_compares_equal() {
    let home = temp_home("prediffer");
    let id = install_lowercasing_prediffer(&home);
    let left = home.join("a.txt");
    let right = home.join("b.txt");
    fs::write(&left, "HELLO WORLD").unwrap();
    fs::write(&right, "hello world").unwrap();
    let (l, r) = (left.to_str().unwrap(), right.to_str().unwrap());

    // Without a prediffer the case difference makes the files differ (exit 1).
    let plain = run_isolated_unsandboxed(&home, &["compare", l, r]);
    assert_eq!(plain.status.code(), Some(1), "plain compare should differ");

    // Routing the enabled lowercasing prediffer normalizes both sides, so they
    // compare equal (exit 0). This is the Phase 6 prediffer-routing acceptance.
    let routed = run_isolated_unsandboxed(&home, &["compare", "--prediffer", id, l, r]);
    assert_eq!(
        routed.status.code(),
        Some(0),
        "prediffer should normalize to equal; stdout={} stderr={}",
        stdout(&routed),
        String::from_utf8_lossy(&routed.stderr)
    );
    assert!(
        String::from_utf8_lossy(&routed.stderr).contains("applying prediffer plugin 'test.lower'")
    );

    // A disabled prediffer is skipped: the comparison falls back to plain and
    // the files differ again (exit 1), with a visible note.
    run_isolated(&home, &["plugin", "disable", id]);
    let disabled = run_isolated_unsandboxed(&home, &["compare", "--prediffer", id, l, r]);
    assert_eq!(
        disabled.status.code(),
        Some(1),
        "disabled prediffer must not apply"
    );
    assert!(String::from_utf8_lossy(&disabled.stderr).contains("none are installed + enabled"));
}
