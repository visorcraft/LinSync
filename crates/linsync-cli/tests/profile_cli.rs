// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// Integration tests for the `linsync-cli profile` subcommand and the
// `--profile` flag on `compare`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_linsync-cli"))
}

/// Run `linsync-cli` with an isolated XDG_CONFIG_HOME / XDG_DATA_HOME /
/// XDG_CACHE_HOME / XDG_STATE_HOME so the profile store points at a
/// scratch dir and never touches the developer's real profiles.
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
        "linsync-cli-profile-test-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn profile_list_includes_every_builtin() {
    let home = temp_home("list");
    let out = run_isolated(&home, &["profile", "list"]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    for id in [
        "default",
        "strict-bytes",
        "ignore-formatting",
        "code-review",
        "prose-review",
        "folder-sync-preview",
        "webpage-source-safe",
    ] {
        assert!(
            stdout.contains(id),
            "profile list missing built-in '{id}'; got:\n{stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn profile_show_emits_valid_json() {
    let home = temp_home("show");
    let out = run_isolated(&home, &["profile", "show", "code-review"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("show output is JSON");
    assert_eq!(v["id"], serde_json::json!("code-review"));
    assert_eq!(v["builtin"], serde_json::json!(true));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn profile_delete_builtin_refused_with_clear_error() {
    let home = temp_home("delete-builtin");
    let out = run_isolated(&home, &["profile", "delete", "default"]);
    assert!(!out.status.success(), "deleting a built-in must fail");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("built-in") && stderr.contains("cannot be deleted"),
        "expected 'built-in cannot be deleted' message, got: {stderr}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn profile_export_import_round_trip() {
    let home = temp_home("export-import");
    // Export a built-in to a file...
    let export_path = home.join("code-review.json");
    let export_path_str = export_path.to_str().unwrap();
    let out = run_isolated(
        &home,
        &[
            "profile",
            "export",
            "code-review",
            "--output",
            export_path_str,
        ],
    );
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(export_path.exists(), "export should create the file");

    // Rewrite the file with a non-builtin id so import is allowed
    // (importing a profile whose id matches a built-in must be
    // rejected — covered in a separate test).
    let mut profile: serde_json::Value =
        serde_json::from_slice(&fs::read(&export_path).unwrap()).unwrap();
    profile["id"] = serde_json::json!("my-code-review");
    profile["name"] = serde_json::json!("My Code Review");
    profile["builtin"] = serde_json::json!(false);
    fs::write(&export_path, serde_json::to_vec_pretty(&profile).unwrap()).unwrap();

    let out = run_isolated(&home, &["profile", "import", export_path_str]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // List should now include the imported profile.
    let out = run_isolated(&home, &["profile", "list"]);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("my-code-review"),
        "imported profile should appear in list; got:\n{stdout}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn profile_import_refuses_shadowing_builtin_id() {
    let home = temp_home("import-shadow");
    // Build a profile JSON whose id matches a built-in.
    let path = home.join("shadow.json");
    fs::write(
        &path,
        br#"{"schema_version": 1, "id": "default", "name": "Shadow"}"#,
    )
    .unwrap();
    let out = run_isolated(&home, &["profile", "import", path.to_str().unwrap()]);
    assert!(!out.status.success(), "shadowing a built-in must fail");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn profile_flag_seeds_compare_options_and_cli_overrides() {
    // strict-bytes has ignore_case=false; with --ignore-case the CLI
    // flag must override and equate FOO with foo.
    let home = temp_home("flag-override");
    let left = home.join("left.txt");
    let right = home.join("right.txt");
    fs::write(&left, b"FOO\n").unwrap();
    fs::write(&right, b"foo\n").unwrap();

    // Without override: strict-bytes finds a diff.
    let out = run_isolated(
        &home,
        &[
            "compare",
            "--profile",
            "strict-bytes",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "strict-bytes should report a diff for FOO vs foo; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // With CLI override: --ignore-case wins, exit 0.
    let out = run_isolated(
        &home,
        &[
            "compare",
            "--profile",
            "strict-bytes",
            "--ignore-case",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "--ignore-case should override the profile; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Order independence: --ignore-case before --profile.
    let out = run_isolated(
        &home,
        &[
            "compare",
            "--ignore-case",
            "--profile",
            "strict-bytes",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "flag order should not matter; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn profile_validate_reports_invalid_input() {
    let home = temp_home("validate-bad");
    let path = home.join("bad.json");
    fs::write(
        &path,
        br#"{"schema_version": 9999, "id": "ok", "name": "Bad"}"#,
    )
    .unwrap();
    let out = run_isolated(&home, &["profile", "validate", path.to_str().unwrap()]);
    assert!(
        !out.status.success(),
        "future schema_version must fail validate"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn unknown_profile_id_emits_helpful_error() {
    let home = temp_home("unknown");
    let left = home.join("left.txt");
    let right = home.join("right.txt");
    fs::write(&left, b"x\n").unwrap();
    fs::write(&right, b"y\n").unwrap();
    let out = run_isolated(
        &home,
        &[
            "compare",
            "--profile",
            "this-does-not-exist",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("default") && stderr.contains("code-review"),
        "error should list known built-ins; got: {stderr}"
    );
    let _ = fs::remove_dir_all(&home);
}
