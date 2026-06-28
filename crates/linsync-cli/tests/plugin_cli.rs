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

/// Install a prediffer whose helper strips ASCII digits from each side, used to
/// exercise multi-prediffer *chaining* alongside the lowercaser.
fn install_digit_stripper_prediffer(home: &Path) -> &'static str {
    let plugin_dir = home.join("data/linsync/plugins/strip");
    fs::create_dir_all(&plugin_dir).unwrap();
    let script = "#!/bin/sh\n\
        request=$(cat)\n\
        rid=$(printf '%s' \"$request\" | sed -n 's/.*\"request_id\":\"\\([^\"]*\\)\".*/\\1/p')\n\
        role=$(printf '%s' \"$request\" | sed -n 's/.*\"role\":\"\\([^\"]*\\)\".*/\\1/p')\n\
        path=$(printf '%s' \"$request\" | sed -n 's/.*\"path\":\"\\([^\"]*\\)\".*/\\1/p')\n\
        text=$(tr -d '0-9' < \"$path\")\n\
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
      "id": "test.strip",
      "name": "Digit Stripper Prediffer",
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
    "test.strip"
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
    // The diagnostic surfaces the active sandbox confinement. Run under
    // LINSYNC_SANDBOX_SKIP, so it is unconfined and labelled "degraded".
    assert_eq!(json["sandbox"]["confined"], serde_json::json!(false));
    assert!(
        json["sandbox"]["label"]
            .as_str()
            .unwrap()
            .contains("degraded")
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
        String::from_utf8_lossy(&routed.stderr)
            .contains("applying prediffer chain before diffing: test.lower")
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

#[test]
fn compare_prediffer_chain_applies_all_stages_in_order() {
    let home = temp_home("prediffer-chain");
    let lower = install_lowercasing_prediffer(&home);
    let strip = install_digit_stripper_prediffer(&home);
    let left = home.join("a.txt");
    let right = home.join("b.txt");
    // Differ by both case and digits; only lowercase+strip together make them equal.
    fs::write(&left, "HELLO123").unwrap();
    fs::write(&right, "hello999").unwrap();
    let (l, r) = (left.to_str().unwrap(), right.to_str().unwrap());

    // One prediffer alone is not enough: lowercasing leaves the digits differing.
    let one = run_isolated_unsandboxed(&home, &["compare", "--prediffer", lower, l, r]);
    assert_eq!(
        one.status.code(),
        Some(1),
        "lowercase alone still differs on digits"
    );

    // The full chain (lowercase -> strip-digits) normalizes both sides to "hello".
    let chained = run_isolated_unsandboxed(
        &home,
        &["compare", "--prediffer", lower, "--prediffer", strip, l, r],
    );
    assert_eq!(
        chained.status.code(),
        Some(0),
        "chain should normalize to equal; stderr={}",
        String::from_utf8_lossy(&chained.stderr)
    );
    assert!(
        String::from_utf8_lossy(&chained.stderr).contains(&format!("{lower} -> {strip}")),
        "stderr should report the chain order"
    );

    // A prediffer-routed compare records the sandbox confinement on the result
    // type itself; --save-result must therefore carry a `sandbox` object.
    let result_json = home.join("chain-result.json");
    let saved = run_isolated_unsandboxed(
        &home,
        &[
            "compare",
            "--prediffer",
            lower,
            "--prediffer",
            strip,
            "--save-result",
            result_json.to_str().unwrap(),
            l,
            r,
        ],
    );
    assert_eq!(saved.status.code(), Some(0));
    let parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&result_json).unwrap()).unwrap();
    let sandbox = &parsed["result"]["sandbox"];
    assert!(
        sandbox["label"].is_string(),
        "prediffer-routed result should carry the sandbox confinement: {parsed}"
    );
    assert!(
        sandbox["confined"].is_boolean(),
        "sandbox confinement should report a confined flag: {parsed}"
    );
}

/// Install a folder-virtualizer plugin whose helper emits a one-file virtual
/// tree whose sha256 is the source file's content, so two archives with equal
/// content compare equal and differing content compares different.
fn install_virtualizer_plugin(home: &Path, id: &str, dir: &str, extension: &str) {
    let plugin_dir = home.join(format!("data/linsync/plugins/{dir}"));
    fs::create_dir_all(&plugin_dir).unwrap();
    let script = "#!/bin/sh\n\
        request=$(cat)\n\
        source=$(printf '%s' \"$request\" | sed -n 's/.*\"source\":\"\\([^\"]*\\)\".*/\\1/p')\n\
        content=$(cat \"$source\")\n\
        printf '{\"ok\":true,\"tree\":[{\"path\":\"entry.txt\",\"kind\":\"file\",\"sha256\":\"%s\"}]}\\n' \"$content\"\n";
    let helper = plugin_dir.join("helper.sh");
    fs::write(&helper, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&helper).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&helper, perms).unwrap();
    }
    let manifest = format!(
        r#"{{
      "schema_version": 1,
      "id": "{id}",
      "name": "Virtualizer Fixture",
      "version": "1.0.0",
      "license": "GPL-3.0-only",
      "entry": ["./helper.sh"],
      "classes": ["folder_virtualizer"],
      "mime_types": ["application/octet-stream"],
      "extensions": ["{extension}"],
      "capabilities": [],
      "deterministic": true,
      "sandbox": {{ "network": false, "writes_input": false, "requires_home_access": false }},
      "options_schema": []
    }}"#
    );
    fs::write(plugin_dir.join("linsync-plugin.json"), manifest).unwrap();
}

#[test]
fn archive_unpacker_compares_virtual_trees() {
    let home = temp_home("archive-virt");
    install_virtualizer_plugin(&home, "test.virt", "virt", "zip");
    let id = "test.virt";
    let a = home.join("a.zip");
    let b = home.join("b.zip");
    let c = home.join("c.zip");
    fs::write(&a, "AAA").unwrap();
    fs::write(&b, "AAA").unwrap(); // same content as a
    fs::write(&c, "BBB").unwrap(); // different
    let (a, b, c) = (
        a.to_str().unwrap(),
        b.to_str().unwrap(),
        c.to_str().unwrap(),
    );

    // Equal virtual trees → exit 0.
    let equal = run_isolated_unsandboxed(&home, &["archive", "--unpacker", id, a, b]);
    assert_eq!(
        equal.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&equal.stderr)
    );

    // Differing trees → exit 1, JSON reports the difference.
    let diff = run_isolated_unsandboxed(&home, &["archive", "--unpacker", id, a, c, "--json"]);
    assert_eq!(diff.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_str(&stdout(&diff)).unwrap();
    assert_eq!(json["equal"], serde_json::json!(false));
    assert_eq!(json["summary"]["different"], serde_json::json!(1));
    // The unpacker ran under the sandbox; its confinement is surfaced (here
    // unconfined, since the test degrades the sandbox).
    assert_eq!(json["sandbox"]["confined"], serde_json::json!(false));

    // Unknown plugin id → error exit 2.
    let unknown = run_isolated_unsandboxed(&home, &["archive", "--unpacker", "nope", a, b]);
    assert_eq!(unknown.status.code(), Some(2));
}

#[test]
fn archive_auto_routes_unsupported_extension_to_virtualizer() {
    let home = temp_home("archive-auto");
    // The built-in extractor has no idea about ".rar"; a virtualizer declares it.
    install_virtualizer_plugin(&home, "test.rar", "rar", "rar");
    let a = home.join("a.rar");
    let b = home.join("b.rar");
    fs::write(&a, "SAME").unwrap();
    fs::write(&b, "SAME").unwrap();
    let (a, b) = (a.to_str().unwrap(), b.to_str().unwrap());

    // No --unpacker: the unsupported extension auto-routes to the virtualizer,
    // and equal content compares equal (exit 0).
    let out = run_isolated_unsandboxed(&home, &["archive", a, b]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // With the plugin disabled, there is no fallback: the built-in extractor
    // rejects the unsupported extension (exit 2).
    run_isolated(&home, &["plugin", "disable", "test.rar"]);
    let out = run_isolated_unsandboxed(&home, &["archive", a, b]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unsupported archive extension"));
}

/// Stage a minimal valid plugin directory *outside* the user plugins dir so it
/// can be installed via `plugin install PATH`.
fn stage_source_plugin(home: &Path) -> PathBuf {
    let src = home.join("staged-plugin");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("helper.sh"), "#!/bin/sh\n").unwrap();
    let manifest = r#"{
      "schema_version": 1,
      "id": "test.stageable",
      "name": "Stageable Fixture",
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
    fs::write(src.join("linsync-plugin.json"), manifest).unwrap();
    src
}

#[test]
fn plugin_install_and_remove_round_trip() {
    let home = temp_home("install");
    let src = stage_source_plugin(&home);
    let src_str = src.to_str().unwrap();

    // Not yet visible to discovery.
    let out = run_isolated(&home, &["plugin", "list", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert!(
        !json["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == "test.stageable"),
        "plugin should not exist before install"
    );

    // Install copies it into the user plugins dir and reports the id.
    let out = run_isolated(&home, &["plugin", "install", src_str]);
    assert!(
        out.status.success(),
        "install failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout(&out).contains("test.stageable"));
    assert!(
        home.join("data/linsync/plugins/test.stageable/linsync-plugin.json")
            .exists(),
        "manifest should be copied into the user plugins dir"
    );

    // Now discoverable.
    let out = run_isolated(&home, &["plugin", "list", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert!(
        json["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == "test.stageable"),
        "plugin should be discovered after install"
    );

    // Re-installing the same id is rejected (exit 2), without clobbering.
    let out = run_isolated(&home, &["plugin", "install", src_str]);
    assert_eq!(out.status.code(), Some(2), "duplicate install should fail");
    assert!(String::from_utf8_lossy(&out.stderr).contains("already installed"));

    // Remove deletes it from the user dir.
    let out = run_isolated(&home, &["plugin", "remove", "test.stageable"]);
    assert!(
        out.status.success(),
        "remove failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !home.join("data/linsync/plugins/test.stageable").exists(),
        "plugin dir should be gone after remove"
    );

    // Removing again reports the plugin is gone (exit 2).
    let out = run_isolated(&home, &["plugin", "remove", "test.stageable"]);
    assert_eq!(out.status.code(), Some(2), "removing absent plugin fails");

    // Installing a path with no manifest is rejected.
    let empty = home.join("empty-dir");
    fs::create_dir_all(&empty).unwrap();
    let out = run_isolated(&home, &["plugin", "install", empty.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "install without manifest fails");
}

#[test]
fn plugin_trust_untrust_persists_and_shows_in_list() {
    let home = temp_home("trust");
    let id = install_fixture_plugin(&home);

    // A freshly discovered plugin is untrusted by default.
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
        entry["trusted"],
        serde_json::json!(false),
        "discovered plugins start untrusted"
    );

    // Trust it.
    let out = run_isolated(&home, &["plugin", "trust", id]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = run_isolated(&home, &["plugin", "inspect", id, "--json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(
        json["trusted"],
        serde_json::json!(true),
        "trust should persist"
    );

    // Untrust it again.
    let out = run_isolated(&home, &["plugin", "untrust", id]);
    assert!(out.status.success());
    let out = run_isolated(&home, &["plugin", "list", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let entry = json["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap()
        .clone();
    assert_eq!(entry["trusted"], serde_json::json!(false));
}
