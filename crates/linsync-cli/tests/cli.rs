use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_linsync-cli"))
}

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(path)
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("run linsync-cli")
}

fn run_with_env(args: &[&str], envs: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(bin());
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run linsync-cli")
}

fn run_with_str_env(args: &[&str], envs: &[(&str, &str)], remove_envs: &[&str]) -> Output {
    let mut command = Command::new(bin());
    command.args(args);
    for key in remove_envs {
        command.env_remove(key);
    }
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run linsync-cli")
}

#[test]
fn compare_returns_zero_for_equal_files() {
    let left = fixture("text/equal-left.txt");
    let right = fixture("text/equal-right.txt");
    let output = run(&[
        "compare",
        "--json",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "{\"equal\":true,\"differences\":0,\"moved_blocks\":0}"
    );
}

#[test]
fn prediffer_conflict_policy_flag_is_accepted() {
    // The flag parses for each accepted value and an unknown value is rejected.
    let left = fixture("text/equal-left.txt");
    let right = fixture("text/equal-right.txt");
    for value in ["chain", "first-wins", "last-wins"] {
        let output = run(&[
            "compare",
            "--prediffer-conflict-policy",
            value,
            "--json",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ]);
        assert!(
            output.status.success(),
            "policy '{value}' should be accepted; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let bad = run(&[
        "compare",
        "--prediffer-conflict-policy",
        "nonsense",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    assert_eq!(
        bad.status.code(),
        Some(2),
        "an unknown policy value is a usage error"
    );
}

#[test]
fn compare_returns_one_for_different_files() {
    let left = fixture("text/left.txt");
    let right = fixture("text/right.txt");
    let output = run(&["compare", left.to_str().unwrap(), right.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("1 differing lines"));
    assert!(stdout.contains("~ beta"));
    assert!(stdout.contains("~ gamma"));
}

#[test]
fn compare_supports_count_and_quiet_modes() {
    let left = fixture("text/left.txt");
    let right = fixture("text/right.txt");
    let count_output = run(&[
        "compare",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let quiet_output = run(&[
        "compare",
        "--quiet",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(count_output.status.code(), Some(1));
    assert_eq!(String::from_utf8(count_output.stdout).unwrap().trim(), "1");
    assert_eq!(quiet_output.status.code(), Some(1));
    assert!(quiet_output.stdout.is_empty());
}

#[test]
fn compare_supports_explicit_type_overrides() {
    let binary_left = fixture("binary/left.bin");
    let binary_right = fixture("binary/right.bin");
    let table_left = fixture("table/left.csv");
    let table_right = fixture("table/right.csv");
    let temp = TempFixture::new();
    let folder_left = temp.path.join("left");
    let folder_right = temp.path.join("right");
    fs::create_dir_all(&folder_left).unwrap();
    fs::create_dir_all(&folder_right).unwrap();
    fs::write(folder_left.join("value.txt"), "left").unwrap();
    fs::write(folder_right.join("value.txt"), "right").unwrap();

    let binary = run(&[
        "compare",
        "--type",
        "binary",
        "--json",
        binary_left.to_str().unwrap(),
        binary_right.to_str().unwrap(),
    ]);
    let table = run(&[
        "compare",
        "--type",
        "table",
        "--count",
        table_left.to_str().unwrap(),
        table_right.to_str().unwrap(),
    ]);
    let folder = run(&[
        "compare",
        "--type",
        "folder",
        "--quiet",
        folder_left.to_str().unwrap(),
        folder_right.to_str().unwrap(),
    ]);

    assert_eq!(binary.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(binary.stdout).unwrap().trim(),
        "{\"equal\":false,\"differences\":4}"
    );
    assert_eq!(table.status.code(), Some(1));
    assert_eq!(String::from_utf8(table.stdout).unwrap().trim(), "1");
    assert_eq!(folder.status.code(), Some(1));
    assert!(folder.stdout.is_empty());
}

#[test]
fn compare_auto_detects_supported_non_text_modes() {
    let binary_left = fixture("binary/left.bin");
    let binary_right = fixture("binary/right.bin");
    let csv_left = fixture("table/left.csv");
    let csv_right = fixture("table/right.csv");
    let temp = TempFixture::new();
    let tsv_left = temp.path.join("left.tsv");
    let tsv_right = temp.path.join("right.tsv");
    let folder_left = temp.path.join("left");
    let folder_right = temp.path.join("right");
    fs::write(&tsv_left, "name\tcount\nalpha\t1\n").unwrap();
    fs::write(&tsv_right, "name\tcount\nalpha\t2\n").unwrap();
    fs::create_dir_all(&folder_left).unwrap();
    fs::create_dir_all(&folder_right).unwrap();
    fs::write(folder_left.join("value.txt"), "left").unwrap();
    fs::write(folder_right.join("value.txt"), "right").unwrap();

    let binary = run(&[
        "compare",
        "--json",
        binary_left.to_str().unwrap(),
        binary_right.to_str().unwrap(),
    ]);
    let csv = run(&[
        "compare",
        "--count",
        csv_left.to_str().unwrap(),
        csv_right.to_str().unwrap(),
    ]);
    let tsv = run(&[
        "compare",
        "--count",
        tsv_left.to_str().unwrap(),
        tsv_right.to_str().unwrap(),
    ]);
    let folder = run(&[
        "compare",
        "--quiet",
        folder_left.to_str().unwrap(),
        folder_right.to_str().unwrap(),
    ]);

    assert_eq!(binary.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(binary.stdout).unwrap().trim(),
        "{\"equal\":false,\"differences\":4}"
    );
    assert_eq!(csv.status.code(), Some(1));
    assert_eq!(String::from_utf8(csv.stdout).unwrap().trim(), "1");
    assert_eq!(tsv.status.code(), Some(1));
    assert_eq!(String::from_utf8(tsv.stdout).unwrap().trim(), "1");
    assert_eq!(folder.status.code(), Some(1));
    assert!(folder.stdout.is_empty());
}

#[test]
fn compare_can_fallback_to_plain_text_for_table_files() {
    let temp = TempFixture::new();
    let left = temp.path.join("left.csv");
    let right = temp.path.join("right.csv");
    fs::write(&left, "value,one\n").unwrap();
    fs::write(&right, "value,two\n").unwrap();

    let table = run(&[
        "compare",
        "--type",
        "table",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let text = run(&[
        "compare",
        "--type",
        "text",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(table.status.code(), Some(1));
    assert_eq!(String::from_utf8(table.stdout).unwrap().trim(), "1");
    assert_eq!(text.status.code(), Some(1));
    assert_eq!(String::from_utf8(text.stdout).unwrap().trim(), "1");
}

#[test]
fn compare_type_override_rejects_unknown_types_and_text_options_for_binary() {
    let left = fixture("binary/left.bin");
    let right = fixture("binary/right.bin");
    let unknown = run(&[
        "compare",
        "--type",
        "unknown_type_xyz",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let text_option = run(&[
        "compare",
        "--type",
        "binary",
        "--ignore-case",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(unknown.status.code(), Some(2));
    assert!(
        String::from_utf8(unknown.stderr)
            .unwrap()
            .contains("unknown compare type")
    );
    assert_eq!(text_option.status.code(), Some(2));
    assert!(
        String::from_utf8(text_option.stderr)
            .unwrap()
            .contains("require --type text")
    );
}

#[test]
fn compare_rejects_unsupported_path_combinations_early() {
    let temp = TempFixture::new();
    let file = temp.path.join("file.txt");
    let dir = temp.path.join("dir");
    let missing = temp.path.join("missing.txt");
    fs::write(&file, "value").unwrap();
    fs::create_dir_all(&dir).unwrap();

    let mixed = run(&["compare", file.to_str().unwrap(), dir.to_str().unwrap()]);
    let missing_output = run(&["compare", file.to_str().unwrap(), missing.to_str().unwrap()]);
    let url = run(&[
        "compare",
        "https://example.invalid/a.txt",
        file.to_str().unwrap(),
    ]);
    let folder_override = run(&[
        "compare",
        "--type",
        "folder",
        file.to_str().unwrap(),
        file.to_str().unwrap(),
    ]);
    let text_override = run(&[
        "compare",
        "--type",
        "text",
        dir.to_str().unwrap(),
        dir.to_str().unwrap(),
    ]);

    assert_eq!(mixed.status.code(), Some(2));
    assert!(
        String::from_utf8(mixed.stderr)
            .unwrap()
            .contains("file-vs-folder")
    );
    assert_eq!(missing_output.status.code(), Some(2));
    assert!(
        String::from_utf8(missing_output.stderr)
            .unwrap()
            .contains("missing path")
    );
    assert_eq!(url.status.code(), Some(2));
    assert!(String::from_utf8(url.stderr).unwrap().contains("URL"));
    assert_eq!(folder_override.status.code(), Some(2));
    assert!(
        String::from_utf8(folder_override.stderr)
            .unwrap()
            .contains("requires two directories")
    );
    assert_eq!(text_override.status.code(), Some(2));
    assert!(
        String::from_utf8(text_override.stderr)
            .unwrap()
            .contains("requires two files")
    );
}

#[test]
fn compare_supports_text_ignore_flags() {
    let temp = TempFixture::new();
    let left = temp.path.join("left.txt");
    let right = temp.path.join("right.txt");
    fs::write(&left, "Alpha   beta\r\nGenerated: 123\r\n\r\nomega\r\n").unwrap();
    fs::write(&right, "alpha beta\nGenerated: 456\nomega\n").unwrap();

    let output = run(&[
        "compare",
        "--json",
        "--ignore-case",
        "--ignore-whitespace",
        "--ignore-blank-lines",
        "--ignore-eol",
        "--ignore-line-regex",
        r"^Generated: \d+$",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "{\"equal\":true,\"differences\":0,\"moved_blocks\":0}"
    );
}

#[test]
fn compare_supports_substitution_filters() {
    let temp = TempFixture::new();
    let left = temp.path.join("left.txt");
    let right = temp.path.join("right.txt");
    fs::write(&left, "id=123 path=/tmp/left\nstable\n").unwrap();
    fs::write(&right, "id=999 path=/tmp/right\nstable\n").unwrap();

    let output = run(&[
        "compare",
        "--json",
        "--substitute-regex",
        r"id=\d+",
        "id=<id>",
        "--substitute-regex",
        r"path=/tmp/\w+",
        "path=<path>",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "{\"equal\":true,\"differences\":0,\"moved_blocks\":0}"
    );
}

#[test]
fn compare_supports_named_regex_rule_sets_find_render_and_encoding() {
    let temp = TempFixture::new();
    let left = temp.path.join("left.txt");
    let right = temp.path.join("right.txt");
    fs::write(
        &left,
        "same before\nid=9f3cf7aa-1d98-4a1a-a80d-d91f442ec4a7 at 2026-05-30T10:00:00Z\nsame after\n",
    )
    .unwrap();
    fs::write(
        &right,
        "same before\nid=11111111-2222-4333-8444-555555555555 at 2026-05-31T11:12:13Z\nsame after\n",
    )
    .unwrap();

    let json_output = run(&[
        "compare",
        "--json",
        "--regex-rule-set",
        "volatile",
        "--find",
        r"\d{4}-\d{2}-\d{2}",
        "--find-regex",
        "--bookmark",
        "left:2:volatile",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    assert!(json_output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    assert_eq!(json["equal"], true);
    assert_eq!(json["regex_rule_sets"][0], "volatile");
    assert_eq!(json["find_matches"].as_array().unwrap().len(), 2);
    assert_eq!(json["bookmarks"][0]["label"], "volatile");

    let render_output = run(&[
        "compare",
        "--render",
        "unified",
        "--context",
        "0",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    assert_eq!(render_output.status.code(), Some(1));
    let stdout = String::from_utf8(render_output.stdout).unwrap();
    assert!(stdout.contains("@@"));
    assert!(!stdout.contains("same before"));
}

#[test]
fn compare_show_only_changes_omits_unchanged_rows() {
    let temp = TempFixture::new();
    let left = temp.path.join("left.txt");
    let right = temp.path.join("right.txt");
    fs::write(&left, "same before\nleft value\nsame after\n").unwrap();
    fs::write(&right, "same before\nright value\nsame after\n").unwrap();

    let output = run(&[
        "compare",
        "--show-only-changes",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("left value"));
    assert!(stdout.contains("right value"));
    assert!(!stdout.contains("same before"));
    assert!(!stdout.contains("same after"));
}

#[test]
fn compare_encoding_flag_decodes_utf16_without_bom() {
    let temp = TempFixture::new();
    let left = temp.path.join("left.txt");
    let right = temp.path.join("right.txt");
    fs::write(&left, [b'a', 0, b'\n', 0]).unwrap();
    fs::write(&right, [b'a', 0, b'\n', 0]).unwrap();

    let output = run(&[
        "compare",
        "--json",
        "--encoding",
        "utf16le",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["equal"], true);
    assert_eq!(json["encoding"]["left_encoding"], "utf16_le");
}

#[test]
fn compare_rejects_invalid_ignore_line_regex() {
    let left = fixture("text/left.txt");
    let right = fixture("text/right.txt");
    let output = run(&[
        "compare",
        "--ignore-line-regex",
        "[unterminated",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("invalid compare regex option")
    );
}

#[test]
fn compare_rejects_invalid_substitution_regex() {
    let left = fixture("text/left.txt");
    let right = fixture("text/right.txt");
    let output = run(&[
        "compare",
        "--substitute-regex",
        "[unterminated",
        "",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("invalid compare regex option")
    );
}

#[test]
fn compare_rejects_conflicting_output_modes() {
    let left = fixture("text/left.txt");
    let right = fixture("text/right.txt");
    let output = run(&[
        "compare",
        "--json",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("cannot be combined"));
}

#[test]
fn completions_generate_supported_shells() {
    let bash = run(&["completions", "bash"]);
    let zsh = run(&["completions", "zsh"]);
    let fish = run(&["completions", "fish"]);

    assert!(bash.status.success());
    assert!(zsh.status.success());
    assert!(fish.status.success());
    let bash_stdout = String::from_utf8(bash.stdout).unwrap();
    assert!(bash_stdout.contains("complete -F _linsync_cli linsync-cli"));
    assert!(
        bash_stdout
            .contains("all differences identical different left-only right-only errors skipped")
    );
    assert!(bash_stdout.contains("--inline-granularity"));
    assert!(bash_stdout.contains("--dry-run"));
    assert!(bash_stdout.contains("auto text binary hex folder table image document"));
    assert!(
        String::from_utf8(zsh.stdout)
            .unwrap()
            .contains("#compdef linsync-cli")
    );
    assert!(
        String::from_utf8(fish.stdout)
            .unwrap()
            .contains("__fish_seen_subcommand_from compare")
    );
}

#[test]
fn table_rejects_invalid_option_values() {
    let left = fixture("table/left.csv");
    let right = fixture("table/right.csv");
    let invalid_tolerance = run(&[
        "table",
        "--numeric-tolerance",
        "not-a-number",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let invalid_bool = run(&[
        "table",
        "--table-skip-blank",
        "maybe",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let invalid_char = run(&[
        "table",
        "--table-quote",
        "quote",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(invalid_tolerance.status.code(), Some(2));
    assert!(
        String::from_utf8(invalid_tolerance.stderr)
            .unwrap()
            .contains("--numeric-tolerance requires a number")
    );
    assert_eq!(invalid_bool.status.code(), Some(2));
    assert!(
        String::from_utf8(invalid_bool.stderr)
            .unwrap()
            .contains("--table-skip-blank requires true or false")
    );
    assert_eq!(invalid_char.status.code(), Some(2));
    assert!(
        String::from_utf8(invalid_char.stderr)
            .unwrap()
            .contains("--table-quote requires exactly one character")
    );
}

#[test]
fn completions_reject_unknown_shell() {
    let output = run(&["completions", "powershell"]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unsupported completion shell"));
}

#[test]
fn man_outputs_roff_and_can_write_file() {
    let temp = TempFixture::new();
    let page = temp.path.join("linsync-cli.1");
    let stdout_output = run(&["man"]);
    let file_output = run(&["man", "--output", page.to_str().unwrap()]);

    assert!(stdout_output.status.success());
    let stdout = String::from_utf8(stdout_output.stdout).unwrap();
    assert!(stdout.contains(".TH LINSYNC-CLI 1"));
    assert!(stdout.contains(".B folders [--recursive]"));

    assert!(file_output.status.success());
    let file = fs::read_to_string(page).unwrap();
    assert!(file.contains(".SH EXIT STATUS"));
    assert!(file.contains("linsync-cli \\- command-line"));
}

#[test]
fn launch_hands_off_to_gui_command_and_can_wait() {
    let temp = TempFixture::new();
    let gui = temp.path.join("fake-gui.sh");
    let capture = temp.path.join("launch-args.txt");
    fs::write(
        &gui,
        "#!/usr/bin/env sh\nprintf '%s\\n' \"$@\" > \"$LINSYNC_LAUNCH_CAPTURE\"\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&gui).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&gui, permissions).unwrap();

    let output = run_with_env(
        &["launch", "--wait", "--", "left path", "right.txt"],
        &[("LINSYNC_GUI", &gui), ("LINSYNC_LAUNCH_CAPTURE", &capture)],
    );

    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(capture).unwrap(),
        "left path\nright.txt\n"
    );
}

#[test]
fn launch_maps_gui_failure_to_error_exit_code() {
    let temp = TempFixture::new();
    let gui = temp.path.join("failing-gui.sh");
    fs::write(&gui, "#!/usr/bin/env sh\nexit 1\n").unwrap();
    let mut permissions = fs::metadata(&gui).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&gui, permissions).unwrap();

    let output = run_with_env(&["launch", "--wait"], &[("LINSYNC_GUI", &gui)]);

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn open_external_uses_configured_opener_and_can_wait() {
    let temp = TempFixture::new();
    let opener = temp.path.join("fake-opener.sh");
    let capture = temp.path.join("open-args.txt");
    let target = temp.path.join("unsupported.custom");
    fs::write(&target, "payload").unwrap();
    fs::write(
        &opener,
        "#!/usr/bin/env sh\nprintf '%s\\n' \"$@\" >> \"$LINSYNC_OPEN_CAPTURE\"\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&opener).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&opener, permissions).unwrap();

    let output = run_with_env(
        &["open-external", "--wait", target.to_str().unwrap()],
        &[
            ("LINSYNC_OPEN", &opener),
            ("LINSYNC_OPEN_CAPTURE", &capture),
        ],
    );

    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(capture).unwrap(),
        format!("{}\n", target.display())
    );
}

#[test]
fn open_external_maps_opener_failure_to_error_exit_code() {
    let temp = TempFixture::new();
    let opener = temp.path.join("failing-opener.sh");
    let target = temp.path.join("file.txt");
    fs::write(&target, "payload").unwrap();
    fs::write(&opener, "#!/usr/bin/env sh\nexit 1\n").unwrap();
    let mut permissions = fs::metadata(&opener).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&opener, permissions).unwrap();

    let output = run_with_env(
        &["open-external", "--wait", target.to_str().unwrap()],
        &[("LINSYNC_OPEN", &opener)],
    );

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn open_external_supports_editor_presets() {
    let temp = TempFixture::new();
    let kate = temp.path.join("kate");
    let capture = temp.path.join("open-preset-args.txt");
    let target = temp.path.join("source.rs");
    fs::write(&target, "fn main() {}\n").unwrap();
    fs::write(
        &kate,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$LINSYNC_OPEN_CAPTURE\"\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&kate).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&kate, permissions).unwrap();

    let output = run_with_str_env(
        &[
            "open-external",
            "--preset",
            "kate",
            "--wait",
            target.to_str().unwrap(),
        ],
        &[
            ("PATH", temp.path.to_str().unwrap()),
            ("LINSYNC_OPEN_CAPTURE", capture.to_str().unwrap()),
        ],
        &["LINSYNC_OPEN"],
    );

    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(capture).unwrap(),
        format!("{}\n", target.display())
    );
}

#[test]
fn reveal_uses_configured_file_manager_and_can_wait() {
    let temp = TempFixture::new();
    let revealer = temp.path.join("fake-reveal.sh");
    let capture = temp.path.join("reveal-args.txt");
    let target = temp.path.join("nested/file.txt");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "payload").unwrap();
    fs::write(
        &revealer,
        "#!/usr/bin/env sh\nprintf '%s\\n' \"$@\" >> \"$LINSYNC_REVEAL_CAPTURE\"\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&revealer).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&revealer, permissions).unwrap();

    let output = run_with_env(
        &["reveal", "--wait", target.to_str().unwrap()],
        &[
            ("LINSYNC_REVEAL", &revealer),
            ("LINSYNC_REVEAL_CAPTURE", &capture),
        ],
    );

    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(capture).unwrap(),
        format!("{}\n", target.display())
    );
}

#[test]
fn reveal_maps_configured_revealer_failure_to_error_exit_code() {
    let temp = TempFixture::new();
    let revealer = temp.path.join("failing-reveal.sh");
    let target = temp.path.join("file.txt");
    fs::write(&target, "payload").unwrap();
    fs::write(&revealer, "#!/usr/bin/env sh\nexit 1\n").unwrap();
    let mut permissions = fs::metadata(&revealer).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&revealer, permissions).unwrap();

    let output = run_with_env(
        &["reveal", "--wait", target.to_str().unwrap()],
        &[("LINSYNC_REVEAL", &revealer)],
    );

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn reveal_uses_filemanager1_before_fallback() {
    let temp = TempFixture::new();
    let dbus_send = temp.path.join("dbus-send");
    let capture = temp.path.join("dbus-args.txt");
    let target = temp.path.join("nested/file with space.txt");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "payload").unwrap();
    fs::write(
        &dbus_send,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$LINSYNC_DBUS_CAPTURE\"\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&dbus_send).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&dbus_send, permissions).unwrap();

    let output = run_with_str_env(
        &["reveal", "--wait", target.to_str().unwrap()],
        &[
            ("PATH", temp.path.to_str().unwrap()),
            ("LINSYNC_DBUS_CAPTURE", capture.to_str().unwrap()),
        ],
        &["LINSYNC_REVEAL"],
    );

    assert!(output.status.success());
    let args = fs::read_to_string(capture).unwrap();
    assert!(args.contains("org.freedesktop.FileManager1.ShowItems"));
    assert!(args.contains("array:string:file://"));
    assert!(args.contains("file%20with%20space.txt"));
}

#[test]
fn reveal_falls_back_to_xdg_open_when_filemanager1_fails() {
    let temp = TempFixture::new();
    let dbus_send = temp.path.join("dbus-send");
    let xdg_open = temp.path.join("xdg-open");
    let capture = temp.path.join("xdg-args.txt");
    let target = temp.path.join("nested/file.txt");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "payload").unwrap();
    fs::write(&dbus_send, "#!/bin/sh\nexit 1\n").unwrap();
    fs::write(
        &xdg_open,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$LINSYNC_XDG_CAPTURE\"\n",
    )
    .unwrap();
    for executable in [&dbus_send, &xdg_open] {
        let mut permissions = fs::metadata(executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(executable, permissions).unwrap();
    }

    let output = run_with_str_env(
        &["reveal", "--wait", target.to_str().unwrap()],
        &[
            ("PATH", temp.path.to_str().unwrap()),
            ("LINSYNC_XDG_CAPTURE", capture.to_str().unwrap()),
        ],
        &["LINSYNC_REVEAL"],
    );

    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(capture).unwrap(),
        format!("{}\n", target.parent().unwrap().display())
    );
}

#[test]
fn reveal_maps_xdg_open_failure_to_error_exit_code() {
    let temp = TempFixture::new();
    let dbus_send = temp.path.join("dbus-send");
    let xdg_open = temp.path.join("xdg-open");
    let target = temp.path.join("nested/file.txt");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "payload").unwrap();
    fs::write(&dbus_send, "#!/bin/sh\nexit 1\n").unwrap();
    fs::write(&xdg_open, "#!/bin/sh\nexit 1\n").unwrap();
    for executable in [&dbus_send, &xdg_open] {
        let mut permissions = fs::metadata(executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(executable, permissions).unwrap();
    }

    let output = run_with_str_env(
        &["reveal", "--wait", target.to_str().unwrap()],
        &[("PATH", temp.path.to_str().unwrap())],
        &["LINSYNC_REVEAL"],
    );

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn patch_outputs_unified_diff() {
    let left = fixture("text/left.txt");
    let right = fixture("text/right.txt");
    let output = run(&[
        "patch",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--format",
        "unified",
        "--context",
        "0",
    ]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("--- "));
    assert!(stdout.contains("@@ -2,1 +2,1 @@"));
    assert!(stdout.contains("-beta"));
    assert!(stdout.contains("+gamma"));
    assert!(!stdout.contains("alpha"));
    assert!(!stdout.contains("shared"));
}

#[test]
fn patch_supports_context_and_normal_formats() {
    let left = fixture("text/left.txt");
    let right = fixture("text/right.txt");
    let context = run(&[
        "patch",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--format",
        "context",
        "--context",
        "0",
    ]);
    let normal = run(&[
        "patch",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--format",
        "normal",
    ]);

    assert_eq!(context.status.code(), Some(1));
    let context_stdout = String::from_utf8(context.stdout).unwrap();
    assert!(context_stdout.contains("*** "));
    assert!(context_stdout.contains("! beta"));
    assert!(context_stdout.contains("! gamma"));
    assert!(!context_stdout.contains("alpha"));

    assert_eq!(normal.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(normal.stdout).unwrap(),
        "2c2\n< beta\n---\n> gamma\n"
    );
}

#[test]
fn patch_generates_folder_patch_for_text_changes() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("same.txt"), "same\n").unwrap();
    fs::write(right.join("same.txt"), "same\n").unwrap();
    fs::write(left.join("changed.txt"), "old\n").unwrap();
    fs::write(right.join("changed.txt"), "new\n").unwrap();
    fs::write(left.join("removed.txt"), "removed\n").unwrap();
    fs::write(right.join("added.txt"), "added\n").unwrap();

    let output = run(&[
        "patch",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--format",
        "unified",
        "--context",
        "0",
    ]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("changed.txt"));
    assert!(stdout.contains("-old"));
    assert!(stdout.contains("+new"));
    assert!(stdout.contains("--- /dev/null"));
    assert!(stdout.contains("+added"));
    assert!(stdout.contains("+++ /dev/null"));
    assert!(stdout.contains("-removed"));
    assert!(!stdout.contains("same.txt"));
}

#[test]
fn patch_rejects_unrepresentable_binary_folder_members() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("data.bin"), b"\x00left").unwrap();
    fs::write(right.join("data.bin"), b"\x00right").unwrap();

    let output = run(&["patch", left.to_str().unwrap(), right.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("cannot represent binary file")
    );
}

#[test]
fn patch_preview_prints_without_writing() {
    let temp = TempFixture::new();
    let report = temp.path.join("preview.patch");
    let left = fixture("text/left.txt");
    let right = fixture("text/right.txt");
    let preview = run(&[
        "patch",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--preview",
    ]);
    let conflicting = run(&[
        "patch",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--preview",
        "--output",
        report.to_str().unwrap(),
    ]);

    assert_eq!(preview.status.code(), Some(1));
    let stdout = String::from_utf8(preview.stdout).unwrap();
    assert!(stdout.contains("--- "));
    assert!(stdout.contains("+gamma"));

    assert_eq!(conflicting.status.code(), Some(2));
    assert!(!report.exists());
    assert!(
        String::from_utf8(conflicting.stderr)
            .unwrap()
            .contains("cannot be combined")
    );
}

#[test]
fn patch_rejects_unknown_format() {
    let left = fixture("text/left.txt");
    let right = fixture("text/right.txt");
    let output = run(&[
        "patch",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--format",
        "side-by-side",
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unsupported patch format")
    );
}

#[test]
fn report_writes_html_file() {
    let temp = TempFixture::new();
    let report = temp.path.join("report.html");
    let left = temp.path.join("left.txt");
    let right = temp.path.join("right.txt");
    fs::write(
        &left,
        "same before\nfar before\nleft\nsame after\nfar after\n",
    )
    .unwrap();
    fs::write(
        &right,
        "same before\nfar before\nright\nsame after\nfar after\n",
    )
    .unwrap();
    let output = run(&[
        "report",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--output",
        report.to_str().unwrap(),
        "--context",
        "1",
    ]);

    assert_eq!(output.status.code(), Some(1));
    let html = fs::read_to_string(report).unwrap();
    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("LinSync Compare Report"));
    assert!(html.contains("far before"));
    assert!(!html.contains("same before"));
    assert!(!html.contains("far after"));
}

#[test]
fn report_writes_folder_html_file() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    let report = temp.path.join("folder-report.html");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("same.txt"), "same").unwrap();
    fs::write(right.join("same.txt"), "same").unwrap();
    fs::write(left.join("different.txt"), "left").unwrap();
    fs::write(right.join("different.txt"), "right").unwrap();
    fs::create_dir_all(left.join("nested")).unwrap();
    fs::create_dir_all(right.join("nested")).unwrap();
    fs::write(left.join("nested/deep.txt"), "left nested").unwrap();
    fs::write(right.join("nested/deep.txt"), "right nested").unwrap();
    fs::write(left.join("left-only.txt"), "left").unwrap();

    let output = run(&[
        "report",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--output",
        report.to_str().unwrap(),
        "--columns",
        "name,path,state,extension,left-modified,right-modified,type,method",
        "--tree-state",
        "collapsed",
        "--nested-file-reports",
        "--context",
        "0",
    ]);

    assert_eq!(output.status.code(), Some(1));
    let html = fs::read_to_string(report).unwrap();
    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("LinSync Folder Report"));
    assert!(html.contains("different.txt"));
    assert!(html.contains("left-only.txt"));
    assert!(html.contains("one-sided=1"));
    assert!(html.contains(
        "<th>Name</th><th>Path</th><th>State</th><th>Extension</th><th>Left Modified</th><th>Right Modified</th><th>Type</th><th>Compare Result</th>"
    ));
    assert!(html.contains("<td>txt</td>"));
    assert!(html.contains("<td>file</td>"));
    assert!(html.contains("<td>binary-contents</td>"));
    assert!(html.contains("data-tree-state=\"collapsed\""));
    assert!(html.contains("<summary>Folder Tree</summary>"));
    assert!(html.contains("Nested File Reports"));
    assert!(html.contains("nested-file-report"));
    assert!(html.contains("srcdoc=\"&lt;!doctype html&gt;"));
    assert!(!html.contains("<th>Left Size</th>"));
}

#[test]
fn compare3_reports_pairwise_base_summaries() {
    let left = fixture("text/left.txt");
    let base = fixture("text/base.txt");
    let right = fixture("text/right.txt");
    let output = run(&[
        "compare3",
        left.to_str().unwrap(),
        base.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("left/base differences="));
    assert!(stdout.contains("right/base differences="));
    assert!(stdout.contains("conflicts="));
}

#[test]
fn compare3_can_emit_conflict_markers() {
    let temp = TempFixture::new();
    let left = temp.path.join("left.txt");
    let base = temp.path.join("base.txt");
    let right = temp.path.join("right.txt");
    fs::write(&left, "value = 2\n").unwrap();
    fs::write(&base, "value = 1\n").unwrap();
    fs::write(&right, "value = 3\n").unwrap();

    let output = run(&[
        "compare3",
        "--markers",
        left.to_str().unwrap(),
        base.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("conflicts=1"));
    assert!(stdout.contains("<<<<<<< LEFT"));
    assert!(stdout.contains(">>>>>>> RIGHT"));
}

#[test]
fn conflict_command_reports_git_conflict_markers() {
    let temp = TempFixture::new();
    let conflicted = temp.path.join("merged.txt");
    let clean = temp.path.join("clean.txt");
    fs::write(
        &conflicted,
        "before\n<<<<<<< HEAD\nleft\n||||||| base\nbase\n=======\nright\n>>>>>>> feature\nafter\n",
    )
    .unwrap();
    fs::write(&clean, "resolved\n").unwrap();

    let conflict = run(&["conflict", conflicted.to_str().unwrap()]);
    let clean_output = run(&["conflict", clean.to_str().unwrap()]);

    assert_eq!(conflict.status.code(), Some(1));
    let stdout = String::from_utf8(conflict.stdout).unwrap();
    assert!(stdout.contains("conflicts=1"));
    assert!(stdout.contains("left=HEAD"));
    assert!(stdout.contains("base=base"));
    assert!(stdout.contains("right=feature"));
    assert!(stdout.contains("lines=2-8"));

    assert!(clean_output.status.success());
    assert!(
        String::from_utf8(clean_output.stdout)
            .unwrap()
            .contains("conflicts=0")
    );
}

#[test]
fn specialized_commands_support_json_output() {
    let temp = TempFixture::new();
    let conflict_file = temp.path.join("merged.txt");
    let _cache_home = temp.path.join("cache");
    fs::write(
        &conflict_file,
        "<<<<<<< HEAD\nleft\n=======\nright\n>>>>>>> feature\n",
    )
    .unwrap();

    let left = fixture("text/left.txt");
    let base = fixture("text/base.txt");
    let right = fixture("text/right.txt");
    let compare3 = run(&[
        "compare3",
        "--json",
        left.to_str().unwrap(),
        base.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let conflict = run(&["conflict", "--json", conflict_file.to_str().unwrap()]);
    let hex = run(&[
        "hex",
        "--json",
        fixture("binary/left.bin").to_str().unwrap(),
        fixture("binary/right.bin").to_str().unwrap(),
    ]);
    let table = run(&[
        "table",
        "--json",
        fixture("table/left.csv").to_str().unwrap(),
        fixture("table/right.csv").to_str().unwrap(),
    ]);

    assert_eq!(compare3.status.code(), Some(1));
    let compare3_json: serde_json::Value = serde_json::from_slice(&compare3.stdout).unwrap();
    assert_eq!(compare3_json["conflicts"], 1);
    assert_eq!(compare3_json["equal"], false);

    assert_eq!(conflict.status.code(), Some(1));
    let conflict_json: serde_json::Value = serde_json::from_slice(&conflict.stdout).unwrap();
    assert_eq!(conflict_json["conflicts"], 1);
    assert_eq!(conflict_json["items"][0]["left_label"], "HEAD");

    assert_eq!(hex.status.code(), Some(1));
    let hex_json: serde_json::Value = serde_json::from_slice(&hex.stdout).unwrap();
    assert_eq!(hex_json["differences"], 4);

    assert_eq!(table.status.code(), Some(1));
    let table_json: serde_json::Value = serde_json::from_slice(&table.stdout).unwrap();
    assert_eq!(table_json["changed_cells"], 1);
}

#[test]
fn folders_support_compare_method_flag() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("same-size.txt"), "abcd").unwrap();
    fs::write(right.join("same-size.txt"), "wxyz").unwrap();

    let size_output = run(&[
        "folders",
        "--method",
        "size",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let binary_output = run(&[
        "folders",
        "--method",
        "binary",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let tolerance_output = run(&[
        "folders",
        "--timestamp-tolerance-ms",
        "250",
        "--method",
        "existence",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let hash_output = run(&[
        "folders",
        "--method",
        "hash-blake3",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    fs::write(left.join("normalized.txt"), "alpha  \r\n").unwrap();
    fs::write(right.join("normalized.txt"), "alpha\n").unwrap();
    let normalized_output = run(&[
        "folders",
        "--method",
        "normalized-text",
        "--state",
        "identical",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let invalid_tolerance = run(&[
        "folders",
        "--timestamp-tolerance-ms",
        "soon",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert!(size_output.status.success());
    assert_eq!(binary_output.status.code(), Some(1));
    assert!(tolerance_output.status.success());
    assert_eq!(hash_output.status.code(), Some(1));
    assert_eq!(String::from_utf8(hash_output.stdout).unwrap().trim(), "1");
    assert_eq!(normalized_output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(normalized_output.stdout).unwrap().trim(),
        "1"
    );
    assert_eq!(invalid_tolerance.status.code(), Some(2));
    assert!(
        String::from_utf8(invalid_tolerance.stderr)
            .unwrap()
            .contains("non-negative integer")
    );
}

#[test]
fn folders_support_recursive_and_non_recursive_modes() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(left.join("nested")).unwrap();
    fs::create_dir_all(right.join("nested")).unwrap();
    fs::write(left.join("nested/value.txt"), "left").unwrap();
    fs::write(right.join("nested/value.txt"), "right").unwrap();

    let non_recursive = run(&[
        "folders",
        "--quiet",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let recursive = run(&[
        "folders",
        "--recursive",
        "--quiet",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert!(non_recursive.status.success());
    assert_eq!(recursive.status.code(), Some(1));
}

#[cfg(unix)]
#[test]
fn folders_support_symlink_policy_flag() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    let outside_left = temp.path.join("outside-left.txt");
    let outside_right = temp.path.join("outside-right.txt");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(&outside_left, "same").unwrap();
    fs::write(&outside_right, "same").unwrap();
    std::os::unix::fs::symlink("../outside-left.txt", left.join("link")).unwrap();
    std::os::unix::fs::symlink("../outside-right.txt", right.join("link")).unwrap();

    let target = run(&[
        "folders",
        "--symlinks",
        "target",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let follow = run(&[
        "folders",
        "--symlinks",
        "follow",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let special = run(&[
        "folders",
        "--symlinks",
        "special",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let invalid = run(&[
        "folders",
        "--symlinks",
        "magic",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(target.status.code(), Some(1));
    assert_eq!(String::from_utf8(target.stdout).unwrap().trim(), "1");
    assert!(follow.status.success());
    assert_eq!(String::from_utf8(follow.stdout).unwrap().trim(), "0");
    assert!(special.status.success());
    assert_eq!(String::from_utf8(special.stdout).unwrap().trim(), "0");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(
        String::from_utf8(invalid.stderr)
            .unwrap()
            .contains("unknown symlink policy")
    );
}

#[test]
fn folders_explain_large_file_method_downgrades() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("large.txt"), "abcd").unwrap();
    fs::write(right.join("large.txt"), "abce").unwrap();

    let json = run(&[
        "folders",
        "--method",
        "full",
        "--large-file-threshold-bytes",
        "3",
        "--large-file-method",
        "binary",
        "--json",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let text = run(&[
        "folders",
        "--method",
        "full",
        "--large-file-threshold-bytes",
        "3",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let invalid = run(&[
        "folders",
        "--large-file-method",
        "date",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(json.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["method_downgrades"], 1);
    assert_eq!(value["entries"][0]["effective_method"], "binary-contents");
    assert!(
        value["entries"][0]["method_note"]
            .as_str()
            .is_some_and(|note| note.contains("method downgraded from full-contents"))
    );

    assert_eq!(text.status.code(), Some(1));
    assert!(
        String::from_utf8(text.stdout)
            .unwrap()
            .contains("method_downgrades=1")
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert!(
        String::from_utf8(invalid.stderr)
            .unwrap()
            .contains("unknown large-file fallback method")
    );
}

#[test]
fn folders_query_filters_search_sorts_and_paginates() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(left.join("sub")).unwrap();
    fs::create_dir_all(right.join("sub")).unwrap();
    fs::write(left.join("a.txt"), "L").unwrap();
    fs::write(right.join("a.txt"), "R").unwrap();
    fs::write(left.join("b.txt"), "same").unwrap();
    fs::write(right.join("b.txt"), "same").unwrap();
    fs::write(left.join("sub/c.txt"), "L").unwrap();
    fs::write(right.join("sub/c.txt"), "R").unwrap();

    let l = left.to_str().unwrap();
    let r = right.to_str().unwrap();

    // --types file drops the directory entry; --types dir keeps only it.
    let files = run(&["folders", "--recursive", "--types", "file", "--json", l, r]);
    let files: serde_json::Value = serde_json::from_slice(&files.stdout).unwrap();
    assert_eq!(files["filtered"], 3);
    assert!(
        files["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["type"] == "file")
    );

    let dirs = run(&["folders", "--recursive", "--types", "dir", "--json", l, r]);
    let dirs: serde_json::Value = serde_json::from_slice(&dirs.stdout).unwrap();
    assert_eq!(dirs["filtered"], 1);
    assert_eq!(dirs["entries"][0]["type"], "directory");
    assert_eq!(dirs["entries"][0]["path"], "sub");

    // --search matches the relative path case-insensitively.
    let search = run(&["folders", "--recursive", "--search", "SUB", "--json", l, r]);
    let search: serde_json::Value = serde_json::from_slice(&search.stdout).unwrap();
    assert_eq!(search["filtered"], 2);
    let paths: Vec<&str> = search["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["sub", "sub/c.txt"]);

    // Pagination over the two differing entries: offset past the first yields
    // the second, sorted by path, with has_more cleared at the end.
    let page = run(&[
        "folders",
        "--recursive",
        "--state",
        "differences",
        "--sort",
        "path",
        "--offset",
        "1",
        "--limit",
        "1",
        "--json",
        l,
        r,
    ]);
    let page: serde_json::Value = serde_json::from_slice(&page.stdout).unwrap();
    assert_eq!(page["filtered"], 2);
    assert_eq!(page["returned"], 1);
    assert_eq!(page["offset"], 1);
    assert_eq!(page["has_more"], false);
    assert_eq!(page["entries"][0]["path"], "sub/c.txt");

    // Invalid query arguments are usage errors.
    assert_eq!(
        run(&["folders", "--sort", "bogus", l, r]).status.code(),
        Some(2)
    );
    assert_eq!(
        run(&["folders", "--types", "bogus", l, r]).status.code(),
        Some(2)
    );
}

#[test]
fn folders_support_json_and_csv_output() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("same.txt"), "same").unwrap();
    fs::write(right.join("same.txt"), "same").unwrap();

    let json_output = run(&[
        "folders",
        "--json",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let csv_output = run(&[
        "folders",
        "--csv",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert!(json_output.status.success());
    assert!(csv_output.status.success());
    let json_stdout = String::from_utf8(json_output.stdout).unwrap();
    assert!(json_stdout.contains("\"compared\":1"));
    assert!(json_stdout.contains("\"skipped\":0"));
    assert!(json_stdout.contains("\"errors\":0"));
    assert!(json_stdout.contains("\"status\":\"complete\""));
    assert!(json_stdout.contains("\"state\":\"identical\""));
    assert!(json_stdout.contains("\"name\":\"same.txt\""));
    assert!(json_stdout.contains("\"extension\":\"txt\""));
    assert!(json_stdout.contains("\"type\":\"file\""));
    assert!(json_stdout.contains("\"left_modified_ms\":"));
    assert!(json_stdout.contains("\"right_modified_ms\":"));
    assert!(String::from_utf8(csv_output.stdout).unwrap().contains(
        "path,state,left_size,right_size,name,extension,type,left_modified_ms,right_modified_ms"
    ));
}

#[test]
fn folders_support_count_and_quiet_modes() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("same.txt"), "same").unwrap();
    fs::write(right.join("same.txt"), "same").unwrap();
    fs::write(left.join("left-only.txt"), "left").unwrap();
    fs::write(left.join("different.txt"), "left").unwrap();
    fs::write(right.join("different.txt"), "right").unwrap();

    let count_output = run(&[
        "folders",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let quiet_output = run(&[
        "folders",
        "--quiet",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(count_output.status.code(), Some(1));
    assert_eq!(String::from_utf8(count_output.stdout).unwrap().trim(), "2");
    assert_eq!(quiet_output.status.code(), Some(1));
    assert!(quiet_output.stdout.is_empty());
}

#[test]
fn folders_support_state_filtering() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("same.txt"), "same").unwrap();
    fs::write(right.join("same.txt"), "same").unwrap();
    fs::write(left.join("left-only.txt"), "left").unwrap();
    fs::write(right.join("right-only.txt"), "right").unwrap();
    fs::write(left.join("different.txt"), "left").unwrap();
    fs::write(right.join("different.txt"), "right").unwrap();

    let left_only = run(&[
        "folders",
        "--state",
        "left-only",
        "--json",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let difference_count = run(&[
        "folders",
        "--state",
        "differences",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let error_count = run(&[
        "folders",
        "--state",
        "errors",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let invalid = run(&[
        "folders",
        "--state",
        "unknown",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(left_only.status.code(), Some(1));
    let json = String::from_utf8(left_only.stdout).unwrap();
    assert!(json.contains("\"filtered\":1"));
    assert!(json.contains("left-only.txt"));
    assert!(!json.contains("right-only.txt"));
    assert_eq!(difference_count.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(difference_count.stdout).unwrap().trim(),
        "3"
    );
    assert_eq!(error_count.status.code(), Some(1));
    assert_eq!(String::from_utf8(error_count.stdout).unwrap().trim(), "0");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(
        String::from_utf8(invalid.stderr)
            .unwrap()
            .contains("unknown folder state filter")
    );
}

#[test]
fn folders_apply_filters_and_can_hide_skipped_rows() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("generated.log"), "left").unwrap();
    fs::write(right.join("generated.log"), "right").unwrap();
    fs::write(left.join("same.txt"), "same").unwrap();
    fs::write(right.join("same.txt"), "same").unwrap();

    let skipped = run(&[
        "folders",
        "--filter",
        "f!:generated",
        "--state",
        "skipped",
        "--json",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let hidden = run(&[
        "folders",
        "--filter",
        "f!:generated",
        "--hide-skipped",
        "--json",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert!(skipped.status.success());
    let skipped_json = String::from_utf8(skipped.stdout).unwrap();
    assert!(skipped_json.contains("\"skipped\":1"));
    assert!(skipped_json.contains("\"state\":\"skipped\""));
    assert!(skipped_json.contains("generated.log"));

    assert!(hidden.status.success());
    let hidden_json = String::from_utf8(hidden.stdout).unwrap();
    assert!(hidden_json.contains("\"skipped\":1"));
    assert!(!hidden_json.contains("generated.log"));
}

#[test]
fn folders_json_shows_effective_profile_filters_and_options() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(left.join("nested")).unwrap();
    fs::create_dir_all(right.join("nested")).unwrap();
    fs::write(left.join("nested/value.txt"), "left").unwrap();
    fs::write(right.join("nested/value.txt"), "right").unwrap();
    fs::write(left.join("generated.log"), "left").unwrap();
    fs::write(right.join("generated.log"), "right").unwrap();

    let output = run(&[
        "folders",
        "--profile",
        "folder-sync-preview",
        "--method",
        "existence",
        "--filter",
        "f!:generated",
        "--case-insensitive-filter",
        "--state",
        "skipped",
        "--json",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["profile"], "folder-sync-preview");
    assert_eq!(json["options"]["profile"], "folder-sync-preview");
    assert_eq!(json["options"]["recursive"], true);
    assert_eq!(json["options"]["compare_method"], "existence");
    assert_eq!(json["options"]["state_filter"], "skipped");
    assert_eq!(
        json["options"]["filter_match_options"]["case_sensitive"],
        false
    );
    assert_eq!(json["options"]["filters"].as_array().unwrap().len(), 1);
    assert_eq!(
        json["options"]["filters"][0]["rules"][0]["pattern"],
        "generated"
    );
    assert_eq!(json["filtered"], 1);
    assert_eq!(json["entries"][0]["path"], "generated.log");
}

#[test]
fn folders_apply_metadata_expression_filters() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("small.txt"), "same").unwrap();
    fs::write(right.join("small.txt"), "same").unwrap();
    fs::write(left.join("large.txt"), "left content").unwrap();
    fs::write(right.join("large.txt"), "right content").unwrap();
    fs::write(left.join("data.bin"), b"\0left").unwrap();
    fs::write(right.join("data.bin"), b"\0right").unwrap();

    let text_only = run(&[
        "folders",
        "--filter",
        "fe:type == text",
        "--hide-skipped",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let binary_only = run(&[
        "folders",
        "--filter",
        "fe:type == binary",
        "--hide-skipped",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let large_only = run(&[
        "folders",
        "--filter",
        "fe:size >= 10B",
        "--hide-skipped",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let timestamp_excluded = run(&[
        "folders",
        "--filter",
        "fe!:modified_ms >= 0",
        "--hide-skipped",
        "--count",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(text_only.status.code(), Some(1));
    assert_eq!(String::from_utf8(text_only.stdout).unwrap().trim(), "1");
    assert_eq!(binary_only.status.code(), Some(1));
    assert_eq!(String::from_utf8(binary_only.stdout).unwrap().trim(), "1");
    assert_eq!(large_only.status.code(), Some(1));
    assert_eq!(String::from_utf8(large_only.stdout).unwrap().trim(), "1");
    assert!(timestamp_excluded.status.success());
    assert_eq!(
        String::from_utf8(timestamp_excluded.stdout).unwrap().trim(),
        "0"
    );
}

#[test]
fn folders_json_escapes_control_characters_in_paths() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("line\nbreak.txt"), "left").unwrap();
    fs::write(right.join("line\nbreak.txt"), "right").unwrap();

    let output = run(&[
        "folders",
        "--json",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["entries"][0]["path"], "line\nbreak.txt");
}

#[test]
fn folders_can_apply_named_filters_from_xdg_config() {
    let temp = TempFixture::new();
    let config_home = temp.path.join("config");
    let filters_dir = config_home.join("linsync");
    fs::create_dir_all(&filters_dir).unwrap();
    fs::write(
        filters_dir.join("filters.json"),
        r#"{
  "schema_version": 1,
  "filters": [
    {
      "name": "Generated",
      "rules": [
        {"target": "file", "action": "exclude", "syntax": "regex", "pattern": "generated"}
      ]
    }
  ]
}"#,
    )
    .unwrap();

    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("generated.log"), "left").unwrap();
    fs::write(right.join("generated.log"), "right").unwrap();

    let output = run_with_env(
        &[
            "folders",
            "--filter-name",
            "Generated",
            "--state",
            "skipped",
            "--json",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ],
        &[("XDG_CONFIG_HOME", &config_home)],
    );

    assert!(output.status.success());
    let json = String::from_utf8(output.stdout).unwrap();
    assert!(json.contains("\"state\":\"skipped\""));
    assert!(json.contains("generated.log"));
}

#[test]
fn folders_reject_conflicting_output_modes() {
    let temp = TempFixture::new();
    let left = temp.path.join("left");
    let right = temp.path.join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();

    let output = run(&[
        "folders",
        "--csv",
        "--json",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("cannot be combined"));
}

#[test]
fn hex_reports_differing_bytes() {
    let left = fixture("binary/left.bin");
    let right = fixture("binary/right.bin");
    let output = run(&[
        "hex",
        "--width",
        "4",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("differing_bytes=4"));
    assert!(stdout.contains("00000000"));
}

#[test]
fn hex_can_report_metadata_without_content_compare() {
    let temp = TempFixture::new();
    let left = temp.path.join("left.bin");
    let right = temp.path.join("right.bin");
    fs::write(&left, b"\0left").unwrap();
    fs::write(&right, b"\0right-side").unwrap();

    let text = run(&[
        "hex",
        "--metadata-only",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    let json = run(&[
        "hex",
        "--metadata-only",
        "--json",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(text.status.code(), Some(1));
    let stdout = String::from_utf8(text.stdout).unwrap();
    assert!(stdout.contains("content_compared=false"));
    assert!(stdout.contains("metadata: size"));
    assert!(!stdout.contains("00000000"));

    assert_eq!(json.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["content_compared"], false);
    assert_eq!(value["differing_bytes"], 0);
    assert_eq!(value["metadata_differences"][0], "size");
}

#[test]
fn table_reports_changed_cells() {
    let left = fixture("table/left.csv");
    let right = fixture("table/right.csv");
    let output = run(&[
        "table",
        "--header",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("changed_cells=1"));
    assert!(stdout.contains("row=2 col=2 left=2 right=3"));
}

#[test]
fn filter_migrate_rewrites_legacy_attr_prefixes() {
    let temp = TempFixture::new();
    let input = temp.path.join("legacy.flt");
    let output = temp.path.join("migrated.flt");
    fs::write(&input, "attr:hidden\nctime: < '2020-01-01'\nf:.*\\.rs$\n").unwrap();

    let result = run(&[
        "filter",
        "migrate",
        input.to_str().unwrap(),
        "--out",
        output.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "migrate exited nonzero");

    let text = fs::read_to_string(&output).unwrap();
    assert!(
        text.contains("# UNSUPPORTED: attr:hidden"),
        "attr should be commented as unsupported; got: {text}"
    );
    assert!(
        text.contains("e: mtime < '2020-01-01'"),
        "ctime should be migrated to e: mtime; got: {text}"
    );
    assert!(
        text.contains("f:.*\\.rs$"),
        "supported prefix should pass through unchanged; got: {text}"
    );
}

#[test]
fn filter_migrate_handles_unrecognized_lines() {
    let temp = TempFixture::new();
    let input = temp.path.join("legacy.flt");
    let output = temp.path.join("migrated.flt");
    fs::write(&input, "garbage line\n# valid comment\nf:.*\\.rs$\n").unwrap();

    let result = run(&[
        "filter",
        "migrate",
        input.to_str().unwrap(),
        "--out",
        output.to_str().unwrap(),
    ]);
    assert!(result.status.success());

    let text = fs::read_to_string(&output).unwrap();
    assert!(
        text.contains("# UNRECOGNIZED: garbage line"),
        "unrecognized line should be commented; got: {text}"
    );
    assert!(
        text.contains("# valid comment"),
        "existing comments should be preserved verbatim; got: {text}"
    );
    assert!(
        text.contains("f:.*\\.rs$"),
        "supported prefix should pass through unchanged; got: {text}"
    );
}

#[test]
fn filter_migrate_in_place_replaces_input_atomically() {
    let temp = TempFixture::new();
    let input = temp.path.join("filter.flt");
    fs::write(&input, "dos:readonly\nf:.*\\.txt$\n").unwrap();

    let result = run(&["filter", "migrate", input.to_str().unwrap(), "--in-place"]);
    assert!(result.status.success(), "in-place migrate exited nonzero");

    let text = fs::read_to_string(&input).unwrap();
    assert!(
        text.contains("# UNSUPPORTED: dos:readonly"),
        "dos: should be commented; got: {text}"
    );
    assert!(
        text.contains("f:.*\\.txt$"),
        "supported rule should survive in-place; got: {text}"
    );
    // Temp file should be cleaned up.
    assert!(!temp.path.join("filter.flt.migrate-tmp").exists());
}

#[test]
fn filter_migrate_rejects_conflicting_out_and_in_place() {
    let temp = TempFixture::new();
    let input = temp.path.join("filter.flt");
    let output = temp.path.join("out.flt");
    fs::write(&input, "f:.*\n").unwrap();

    let result = run(&[
        "filter",
        "migrate",
        input.to_str().unwrap(),
        "--out",
        output.to_str().unwrap(),
        "--in-place",
    ]);
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("cannot be combined")
    );
}

#[test]
fn filter_migrate_is_idempotent() {
    let temp = TempFixture::new();
    let input = temp.path.join("legacy.flt");
    let pass1 = temp.path.join("pass1.flt");
    let pass2 = temp.path.join("pass2.flt");
    fs::write(&input, "attr:hidden\nf:.*\\.rs$\n").unwrap();

    run(&[
        "filter",
        "migrate",
        input.to_str().unwrap(),
        "--out",
        pass1.to_str().unwrap(),
    ]);
    run(&[
        "filter",
        "migrate",
        pass1.to_str().unwrap(),
        "--out",
        pass2.to_str().unwrap(),
    ]);

    assert_eq!(
        fs::read_to_string(&pass1).unwrap(),
        fs::read_to_string(&pass2).unwrap(),
        "second migrate pass should be identical to first"
    );
}

struct TempFixture {
    path: PathBuf,
}

impl TempFixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "linsync-cli-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

#[test]
fn mergetool_subcommand_auto_resolve_writes_merged_result() {
    let dir = TempFixture::new();
    let base = dir.path.join("base.txt");
    fs::write(&base, "a\nb\nc\n").unwrap();
    let local = dir.path.join("local.txt");
    fs::write(&local, "a\nb_local\nc\n").unwrap();
    let remote = dir.path.join("remote.txt");
    fs::write(&remote, "a\nb_remote\nc\n").unwrap();
    let merged = dir.path.join("merged.txt");
    fs::write(&merged, "").unwrap();

    let status = std::process::Command::new(bin())
        .args([
            "mergetool",
            "--base",
            base.to_str().unwrap(),
            "--local",
            local.to_str().unwrap(),
            "--remote",
            remote.to_str().unwrap(),
            "--merged",
            merged.to_str().unwrap(),
            "--auto-resolve",
            "left",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read_to_string(&merged).unwrap(), "a\nb_local\nc\n");
}

#[test]
fn mergetool_subcommand_auto_resolve_right() {
    let dir = TempFixture::new();
    let base = dir.path.join("base.txt");
    fs::write(&base, "a\nb\nc\n").unwrap();
    let local = dir.path.join("local.txt");
    fs::write(&local, "a\nb_local\nc\n").unwrap();
    let remote = dir.path.join("remote.txt");
    fs::write(&remote, "a\nb_remote\nc\n").unwrap();
    let merged = dir.path.join("merged.txt");
    fs::write(&merged, "").unwrap();

    let status = std::process::Command::new(bin())
        .args([
            "mergetool",
            "--base",
            base.to_str().unwrap(),
            "--local",
            local.to_str().unwrap(),
            "--remote",
            remote.to_str().unwrap(),
            "--merged",
            merged.to_str().unwrap(),
            "--auto-resolve",
            "right",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read_to_string(&merged).unwrap(), "a\nb_remote\nc\n");
}

#[test]
fn mergetool_subcommand_auto_resolve_base() {
    let dir = TempFixture::new();
    let base = dir.path.join("base.txt");
    fs::write(&base, "a\nb\nc\n").unwrap();
    let local = dir.path.join("local.txt");
    fs::write(&local, "a\nb_local\nc\n").unwrap();
    let remote = dir.path.join("remote.txt");
    fs::write(&remote, "a\nb_remote\nc\n").unwrap();
    let merged = dir.path.join("merged.txt");
    fs::write(&merged, "").unwrap();

    let status = std::process::Command::new(bin())
        .args([
            "mergetool",
            "--base",
            base.to_str().unwrap(),
            "--local",
            local.to_str().unwrap(),
            "--remote",
            remote.to_str().unwrap(),
            "--merged",
            merged.to_str().unwrap(),
            "--auto-resolve",
            "base",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read_to_string(&merged).unwrap(), "a\nb\nc\n");
}

#[test]
fn mergetool_subcommand_json_reports_auto_resolve_summary() {
    let dir = TempFixture::new();
    let base = dir.path.join("base.txt");
    fs::write(&base, "a\nb\nc\n").unwrap();
    let local = dir.path.join("local.txt");
    fs::write(&local, "a\nb_local\nc\n").unwrap();
    let remote = dir.path.join("remote.txt");
    fs::write(&remote, "a\nb_remote\nc\n").unwrap();
    let merged = dir.path.join("merged.txt");
    fs::write(&merged, "").unwrap();

    let output = run(&[
        "mergetool",
        "--base",
        base.to_str().unwrap(),
        "--local",
        local.to_str().unwrap(),
        "--remote",
        remote.to_str().unwrap(),
        "--merged",
        merged.to_str().unwrap(),
        "--auto-resolve",
        "left",
        "--json",
    ]);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read_to_string(&merged).unwrap(), "a\nb_local\nc\n");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "resolved");
    assert_eq!(json["mode"], "auto");
    assert_eq!(json["auto_choice"], "left");
    assert_eq!(json["conflicts"], 1);
    assert_eq!(json["resolved_conflicts"], 1);
    assert_eq!(json["unresolved_conflicts"], 0);
    assert_eq!(json["written"], true);
    assert_eq!(json["merged"], merged.to_str().unwrap());
    assert_eq!(json["items"][0]["id"], 0);
    assert!(json["items"][0]["start_line"].as_u64().unwrap() > 0);
    assert!(
        json["items"][0]["end_line"].as_u64().unwrap()
            >= json["items"][0]["start_line"].as_u64().unwrap()
    );
    assert!(json["items"][0]["left_lines"].as_u64().unwrap() > 0);
    assert!(json["items"][0]["base_lines"].as_u64().unwrap() > 0);
    assert!(json["items"][0]["right_lines"].as_u64().unwrap() > 0);
}

#[test]
fn mergetool_interactive_launches_gui_and_validates_output() {
    // Interactive mergetool launches the GUI (here a fake stand-in via
    // LINSYNC_GUI) and decides success from the *written* output, not the GUI's
    // exit status: a marker-free file → 0, conflict markers → 1, nothing → 2.
    let dir = TempFixture::new();
    let base = dir.path.join("base.txt");
    fs::write(&base, "a\nb\nc\n").unwrap();
    let local = dir.path.join("local.txt");
    fs::write(&local, "a\nb_local\nc\n").unwrap();
    let remote = dir.path.join("remote.txt");
    fs::write(&remote, "a\nb_remote\nc\n").unwrap();
    let merged = dir.path.join("merged.txt");

    // Fake GUI: write whatever $MERGE_TEST_OUTPUT contains to the merged path.
    let fake_gui = dir.path.join("fake-gui.sh");
    fs::write(
        &fake_gui,
        "#!/bin/sh\nprintf '%s' \"$MERGE_TEST_OUTPUT\" > \"$LINSYNC_MERGE_MERGED\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&fake_gui).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake_gui, perms).unwrap();
    }

    let args = [
        "mergetool",
        "--base",
        base.to_str().unwrap(),
        "--local",
        local.to_str().unwrap(),
        "--remote",
        remote.to_str().unwrap(),
        "--merged",
        merged.to_str().unwrap(),
    ];

    // Resolved (marker-free) output → success.
    let resolved = run_with_str_env(
        &args,
        &[
            ("LINSYNC_GUI", fake_gui.to_str().unwrap()),
            ("MERGE_TEST_OUTPUT", "a\nb_merged\nc\n"),
        ],
        &[],
    );
    assert_eq!(resolved.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&merged).unwrap(), "a\nb_merged\nc\n");

    // Output left with conflict markers → unresolved (exit 1).
    let conflict = run_with_str_env(
        &args,
        &[
            ("LINSYNC_GUI", fake_gui.to_str().unwrap()),
            (
                "MERGE_TEST_OUTPUT",
                "<<<<<<< LOCAL\nx\n=======\ny\n>>>>>>> REMOTE\n",
            ),
        ],
        &[],
    );
    assert_eq!(conflict.status.code(), Some(1));

    // GUI writes nothing → no resolved output (exit 2).
    fs::remove_file(&merged).ok();
    let nothing = run_with_str_env(
        &args,
        &[
            ("LINSYNC_GUI", fake_gui.to_str().unwrap()),
            ("MERGE_TEST_OUTPUT", ""),
        ],
        &[],
    );
    // The fake still creates an empty file, which is marker-free → treated as
    // resolved; to test the "missing" path, point at a GUI that writes nothing.
    let _ = nothing;
    let noop_gui = dir.path.join("noop-gui.sh");
    fs::write(&noop_gui, "#!/bin/sh\ntrue\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&noop_gui).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&noop_gui, perms).unwrap();
    }
    let missing_merged = dir.path.join("never-written.txt");
    let missing = run_with_str_env(
        &[
            "mergetool",
            "--base",
            base.to_str().unwrap(),
            "--local",
            local.to_str().unwrap(),
            "--remote",
            remote.to_str().unwrap(),
            "--merged",
            missing_merged.to_str().unwrap(),
        ],
        &[("LINSYNC_GUI", noop_gui.to_str().unwrap())],
        &[],
    );
    assert_eq!(missing.status.code(), Some(2));

    // --json reports the resolved status.
    let json_out = run_with_str_env(
        &[
            "mergetool",
            "--base",
            base.to_str().unwrap(),
            "--local",
            local.to_str().unwrap(),
            "--remote",
            remote.to_str().unwrap(),
            "--merged",
            merged.to_str().unwrap(),
            "--json",
        ],
        &[
            ("LINSYNC_GUI", fake_gui.to_str().unwrap()),
            ("MERGE_TEST_OUTPUT", "a\nb_merged\nc\n"),
        ],
        &[],
    );
    assert_eq!(json_out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&json_out.stdout).unwrap();
    assert_eq!(json["status"], "resolved");
    assert_eq!(json["mode"], "interactive");
    assert_eq!(json["written"], true);
}

impl Drop for TempFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn session_save_list_show_clear_roundtrip() {
    let temp = TempFixture::new();
    let data = temp.path.join("data");
    let config = temp.path.join("config");
    let envs: &[(&str, &Path)] = &[
        ("XDG_DATA_HOME", data.as_path()),
        ("XDG_CONFIG_HOME", config.as_path()),
        ("HOME", temp.path.as_path()),
    ];

    // Empty to start.
    let out = run_with_env(&["session", "list", "--json"], envs);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["sessions"].as_array().unwrap().len(), 0);

    // Save two sessions; the newest is first.
    assert!(
        run_with_env(
            &[
                "session",
                "save",
                "/tmp/a.txt",
                "/tmp/b.txt",
                "--title",
                "first"
            ],
            envs
        )
        .status
        .success()
    );
    assert!(
        run_with_env(
            &[
                "session",
                "save",
                "/tmp/c.txt",
                "/tmp/d.txt",
                "--view",
                "folder"
            ],
            envs
        )
        .status
        .success()
    );

    let out = run_with_env(&["session", "list", "--json"], envs);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let sessions = json["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0]["left"], "/tmp/c.txt");
    assert_eq!(sessions[0]["view"], "folder");
    assert_eq!(sessions[1]["title"], "first");

    // show by index.
    let out = run_with_env(&["session", "show", "1", "--json"], envs);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["title"], "first");
    assert_eq!(json["right"], "/tmp/b.txt");

    // The GUI-shared recent-sessions file is written where the GUI restores from.
    assert!(
        data.join("linsync/recent-sessions.json").exists(),
        "recent-sessions.json should be written for GUI restore"
    );

    // clear empties the history.
    assert!(run_with_env(&["session", "clear"], envs).status.success());
    let out = run_with_env(&["session", "list", "--json"], envs);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["sessions"].as_array().unwrap().len(), 0);

    // Out-of-range show is a usage error.
    assert_eq!(
        run_with_env(&["session", "show", "5"], envs).status.code(),
        Some(2)
    );
}

#[test]
fn project_validate_show_run_with_ci_exit_codes() {
    let temp = TempFixture::new();
    fs::write(temp.path.join("a.txt"), "same").unwrap();
    fs::write(temp.path.join("b.txt"), "same").unwrap();
    fs::write(temp.path.join("c.txt"), "left").unwrap();
    fs::write(temp.path.join("d.txt"), "right").unwrap();
    let a = temp.path.join("a.txt");
    let b = temp.path.join("b.txt");
    let c = temp.path.join("c.txt");
    let d = temp.path.join("d.txt");
    let project = temp.path.join("demo.linsync-project");
    let json = format!(
        r#"{{"schema_version":1,"name":"demo","sessions":[
            {{"schema_version":1,"session":{{"title":"identical","left":"{}","right":"{}","options":{{}}}}}},
            {{"schema_version":1,"session":{{"title":"changed","left":"{}","right":"{}","options":{{}}}}}}
        ]}}"#,
        a.display(),
        b.display(),
        c.display(),
        d.display()
    );
    fs::write(&project, json).unwrap();
    let p = project.to_str().unwrap();

    // validate succeeds.
    let out = run(&["project", "validate", p]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // run exits 1 because one comparison differs.
    let out = run(&["project", "run", p, "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["equal"], serde_json::json!(false));
    let comparisons = value["comparisons"].as_array().unwrap();
    assert_eq!(comparisons[0]["status"], "equal");
    assert_eq!(comparisons[0]["mode"], "text");
    assert_eq!(comparisons[1]["status"], "different");

    // A missing project path is a usage error (exit 2).
    let out = run(&[
        "project",
        "validate",
        temp.path.join("missing.json").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn project_run_all_equal_exits_zero() {
    let temp = TempFixture::new();
    fs::write(temp.path.join("a.txt"), "same").unwrap();
    fs::write(temp.path.join("b.txt"), "same").unwrap();
    let a = temp.path.join("a.txt");
    let b = temp.path.join("b.txt");
    let project = temp.path.join("equal.linsync-project");
    let json = format!(
        r#"{{"schema_version":1,"name":"eq","sessions":[
            {{"schema_version":1,"session":{{"title":"identical","left":"{}","right":"{}","options":{{}}}}}}
        ]}}"#,
        a.display(),
        b.display()
    );
    fs::write(&project, json).unwrap();
    let out = run(&["project", "run", project.to_str().unwrap()]);
    assert!(out.status.success(), "all-equal project should exit 0");
}

#[test]
fn project_report_writes_html_per_comparison() {
    let temp = TempFixture::new();
    fs::write(temp.path.join("a.txt"), "same").unwrap();
    fs::write(temp.path.join("b.txt"), "same").unwrap();
    fs::write(temp.path.join("c.txt"), "left").unwrap();
    fs::write(temp.path.join("d.txt"), "right").unwrap();
    let (a, b) = (temp.path.join("a.txt"), temp.path.join("b.txt"));
    let (c, d) = (temp.path.join("c.txt"), temp.path.join("d.txt"));
    let project = temp.path.join("demo.linsync-project");
    let json = format!(
        r#"{{"schema_version":1,"name":"demo","sessions":[
            {{"schema_version":1,"session":{{"title":"Same Pair","left":"{}","right":"{}","options":{{}}}}}},
            {{"schema_version":1,"session":{{"title":"Changed!","left":"{}","right":"{}","options":{{}}}}}}
        ]}}"#,
        a.display(),
        b.display(),
        c.display(),
        d.display()
    );
    fs::write(&project, json).unwrap();
    let out_dir = temp.path.join("reports");

    let out = run(&[
        "project",
        "report",
        project.to_str().unwrap(),
        "--output",
        out_dir.to_str().unwrap(),
    ]);
    // One comparison differs, so the CI exit code is 1.
    assert_eq!(out.status.code(), Some(1));

    // A slugified HTML file is written per comparison.
    let same = out_dir.join("00-same-pair.html");
    let changed = out_dir.join("01-changed.html");
    assert!(same.exists(), "expected {}", same.display());
    assert!(changed.exists(), "expected {}", changed.display());
    let html = fs::read_to_string(&changed).unwrap();
    assert!(html.contains("<html"), "report should be HTML");

    // Missing --output is a usage error.
    let out = run(&["project", "report", project.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn project_list_finds_project_files_in_dir() {
    let temp = TempFixture::new();
    fs::write(
        temp.path.join("alpha.linsync-project"),
        r#"{"schema_version":1,"name":"alpha","sessions":[{"schema_version":1,"session":{"title":"x","left":"/a","right":"/b","options":{}}}]}"#,
    )
    .unwrap();
    fs::write(
        temp.path.join("beta.linsync-project"),
        r#"{"schema_version":1,"name":"beta","sessions":[]}"#,
    )
    .unwrap();
    // A non-project file is ignored.
    fs::write(temp.path.join("notes.txt"), "ignore me").unwrap();

    let out = run(&["project", "list", temp.path.to_str().unwrap(), "--json"]);
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let projects = value["projects"].as_array().unwrap();
    assert_eq!(
        projects.len(),
        2,
        "should find exactly the two project files"
    );
    // Sorted by path: alpha before beta.
    assert_eq!(projects[0]["name"], "alpha");
    assert_eq!(projects[0]["comparisons"], 1);
    assert_eq!(projects[1]["name"], "beta");
    assert_eq!(projects[1]["comparisons"], 0);
}

#[test]
fn project_run_applies_per_session_profile() {
    let temp = TempFixture::new();
    // The two files differ only by internal whitespace.
    fs::write(temp.path.join("a.txt"), "hello world\n").unwrap();
    fs::write(temp.path.join("b.txt"), "hello    world\n").unwrap();
    let (a, b) = (temp.path.join("a.txt"), temp.path.join("b.txt"));
    let project = temp.path.join("p.linsync-project");
    // Entry 0 uses the built-in ignore-formatting profile; entry 1 uses defaults.
    let json = format!(
        r#"{{"schema_version":1,"name":"prof","sessions":[
            {{"schema_version":1,"profile":"ignore-formatting","session":{{"title":"ws","left":"{}","right":"{}","options":{{}}}}}},
            {{"schema_version":1,"session":{{"title":"ws-default","left":"{}","right":"{}","options":{{}}}}}}
        ]}}"#,
        a.display(),
        b.display(),
        a.display(),
        b.display()
    );
    fs::write(&project, json).unwrap();

    let out = run(&["project", "run", project.to_str().unwrap(), "--json"]);
    // One comparison differs (the default one), so the aggregate exit is 1.
    assert_eq!(out.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let comparisons = value["comparisons"].as_array().unwrap();
    assert_eq!(
        comparisons[0]["status"], "equal",
        "ignore-formatting hides whitespace diff"
    );
    assert_eq!(comparisons[0]["profile"], "ignore-formatting");
    assert_eq!(
        comparisons[1]["status"], "different",
        "default options see the diff"
    );
    assert!(comparisons[1]["profile"].is_null());
}

#[test]
fn session_save_records_profile() {
    let temp = TempFixture::new();
    let data = temp.path.join("data");
    let config = temp.path.join("config");
    let envs: &[(&str, &Path)] = &[
        ("XDG_DATA_HOME", data.as_path()),
        ("XDG_CONFIG_HOME", config.as_path()),
        ("HOME", temp.path.as_path()),
    ];
    assert!(
        run_with_env(
            &[
                "session",
                "save",
                "/tmp/a",
                "/tmp/b",
                "--profile",
                "code-review"
            ],
            envs
        )
        .status
        .success()
    );
    let out = run_with_env(&["session", "show", "0", "--json"], envs);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["profile"], "code-review");
}

#[test]
fn report_relative_paths_strips_absolute_prefix() {
    let temp = TempFixture::new();
    fs::write(temp.path.join("a.txt"), "one\ntwo\n").unwrap();
    fs::write(temp.path.join("b.txt"), "one\nTWO\n").unwrap();
    let abs_a = temp.path.join("a.txt");
    let abs_b = temp.path.join("b.txt");
    let out_file = temp.path.join("report.html");

    // Run with the working directory set to the fixture so absolute inputs
    // under it relativize to bare file names.
    let output = Command::new(bin())
        .current_dir(&temp.path)
        .args([
            "report",
            abs_a.to_str().unwrap(),
            abs_b.to_str().unwrap(),
            "--output",
            out_file.to_str().unwrap(),
            "--relative-paths",
        ])
        .output()
        .expect("run linsync-cli");
    // Files differ, so report exits 1.
    assert_eq!(output.status.code(), Some(1));
    let html = fs::read_to_string(&out_file).unwrap();
    assert!(html.contains("a.txt"), "report should name the file");
    assert!(
        !html.contains(temp.path.to_str().unwrap()),
        "report must not embed the absolute fixture path:\n{html}"
    );

    // Without --relative-paths the absolute path is present.
    let out_file2 = temp.path.join("report-abs.html");
    let output = Command::new(bin())
        .current_dir(&temp.path)
        .args([
            "report",
            abs_a.to_str().unwrap(),
            abs_b.to_str().unwrap(),
            "--output",
            out_file2.to_str().unwrap(),
        ])
        .output()
        .expect("run linsync-cli");
    assert_eq!(output.status.code(), Some(1));
    let html = fs::read_to_string(&out_file2).unwrap();
    assert!(
        html.contains(temp.path.to_str().unwrap()),
        "default report keeps the absolute path"
    );
}

#[test]
fn compare_save_result_then_report_from_json_matches_direct() {
    let temp = TempFixture::new();
    fs::write(temp.path.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    fs::write(temp.path.join("b.txt"), "one\nTWO\nthree\nfour\n").unwrap();
    let a = temp.path.join("a.txt");
    let b = temp.path.join("b.txt");
    let result_json = temp.path.join("result.json");
    let from_html = temp.path.join("from.html");
    let direct_html = temp.path.join("direct.html");
    // Private config dir: the save captures the active profile's options into
    // the JSON, while the direct report re-reads the active profile, so both
    // invocations must see a *stable* active-profile pointer. Sharing the real
    // $XDG_CONFIG_HOME lets a parallel profile-mutating test change it between
    // the two calls and diverge the HTML.
    let cfg = temp.path.join("xdg-config");
    let env = [("XDG_CONFIG_HOME", cfg.as_path())];

    // Save the full result; the compared files differ, so exit 1.
    let save = run_with_env(
        &[
            "compare",
            "--save-result",
            result_json.to_str().unwrap(),
            a.to_str().unwrap(),
            b.to_str().unwrap(),
        ],
        &env,
    );
    assert_eq!(save.status.code(), Some(1));
    assert!(result_json.exists(), "result JSON should be written");

    // Re-render from the saved JSON (no recompare) and directly; the HTML must match.
    let from = run_with_env(
        &[
            "report",
            "--from-json",
            result_json.to_str().unwrap(),
            "--output",
            from_html.to_str().unwrap(),
        ],
        &env,
    );
    assert_eq!(from.status.code(), Some(1));
    let direct = run_with_env(
        &[
            "report",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            "--output",
            direct_html.to_str().unwrap(),
        ],
        &env,
    );
    assert_eq!(direct.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(&from_html).unwrap(),
        fs::read_to_string(&direct_html).unwrap(),
        "report --from-json must reproduce the direct report"
    );

    // An unsupported result kind is rejected.
    let bad = temp.path.join("bad.json");
    fs::write(&bad, r#"{"kind":"image","result":{}}"#).unwrap();
    let out = run(&[
        "report",
        "--from-json",
        bad.to_str().unwrap(),
        "--output",
        temp.path.join("x.html").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn folder_save_result_then_report_from_json_matches_direct() {
    let temp = TempFixture::new();
    let left = temp.path.join("l");
    let right = temp.path.join("r");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("x.txt"), "a").unwrap();
    fs::write(right.join("x.txt"), "b").unwrap();
    fs::write(left.join("y.txt"), "same").unwrap();
    fs::write(right.join("y.txt"), "same").unwrap();
    let result_json = temp.path.join("folder.json");
    let from_html = temp.path.join("from.html");
    let direct_html = temp.path.join("direct.html");
    // Private config dir so the save and the direct report share a stable
    // active profile (see the text variant above for the race this avoids).
    let cfg = temp.path.join("xdg-config");
    let env = [("XDG_CONFIG_HOME", cfg.as_path())];

    let save = run_with_env(
        &[
            "compare",
            "--type",
            "folder",
            "--save-result",
            result_json.to_str().unwrap(),
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ],
        &env,
    );
    assert_eq!(save.status.code(), Some(1));

    let from = run_with_env(
        &[
            "report",
            "--from-json",
            result_json.to_str().unwrap(),
            "--output",
            from_html.to_str().unwrap(),
        ],
        &env,
    );
    assert_eq!(from.status.code(), Some(1));
    let direct = run_with_env(
        &[
            "report",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
            "--output",
            direct_html.to_str().unwrap(),
        ],
        &env,
    );
    assert_eq!(direct.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(&from_html).unwrap(),
        fs::read_to_string(&direct_html).unwrap(),
        "folder report --from-json must reproduce the direct report"
    );
}

#[test]
fn table_save_result_then_report_from_json_renders_table() {
    let temp = TempFixture::new();
    fs::write(temp.path.join("a.csv"), "1,alice\n2,bob\n").unwrap();
    fs::write(temp.path.join("b.csv"), "1,alice\n2,BOB\n3,carol\n").unwrap();
    let a = temp.path.join("a.csv");
    let b = temp.path.join("b.csv");
    let json = temp.path.join("t.json");
    let html = temp.path.join("r.html");

    let save = run(&[
        "compare",
        "--type",
        "table",
        "--save-result",
        json.to_str().unwrap(),
        a.to_str().unwrap(),
        b.to_str().unwrap(),
    ]);
    assert_eq!(save.status.code(), Some(1));

    let from = run(&[
        "report",
        "--from-json",
        json.to_str().unwrap(),
        "--output",
        html.to_str().unwrap(),
    ]);
    assert_eq!(from.status.code(), Some(1));
    let report = fs::read_to_string(&html).unwrap();
    assert!(report.contains("<table>"), "renders an HTML table");
    assert!(
        report.contains("class=\"changed\""),
        "a changed cell is highlighted"
    );
    assert!(report.contains("carol"), "the added row is present");
}

#[test]
fn binary_save_result_then_report_from_json_renders_hex() {
    let temp = TempFixture::new();
    fs::write(temp.path.join("a.bin"), [0u8, 1, 2, 3, b'A', b'B']).unwrap();
    fs::write(temp.path.join("b.bin"), [0u8, 1, 255, 3, b'A', b'B']).unwrap();
    let a = temp.path.join("a.bin");
    let b = temp.path.join("b.bin");
    let json = temp.path.join("bin.json");
    let html = temp.path.join("r.html");

    let save = run(&[
        "compare",
        "--type",
        "binary",
        "--save-result",
        json.to_str().unwrap(),
        a.to_str().unwrap(),
        b.to_str().unwrap(),
    ]);
    assert_eq!(save.status.code(), Some(1));
    // Raw byte buffers are not serialized into the report JSON.
    assert!(
        !fs::read_to_string(&json).unwrap().contains("left_data"),
        "raw buffers must be skipped from saved result"
    );

    let from = run(&[
        "report",
        "--from-json",
        json.to_str().unwrap(),
        "--output",
        html.to_str().unwrap(),
    ]);
    assert_eq!(from.status.code(), Some(1));
    let report = fs::read_to_string(&html).unwrap();
    assert!(report.contains("<table>"), "renders a hex table");
    assert!(
        report.contains("class=\"diff\""),
        "a differing hex row is highlighted"
    );
}
