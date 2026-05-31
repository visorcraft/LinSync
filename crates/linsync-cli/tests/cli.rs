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

fn assert_comparison_cache_was_cleaned(cache_home: &Path) {
    let comparisons = cache_home.join("linsync/comparisons");
    assert!(comparisons.exists());
    assert_eq!(fs::read_dir(comparisons).unwrap().count(), 0);
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
    let cache_home = temp.path.join("cache");
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
    let self_compare = run_with_env(
        &["self-compare", "--json", left.to_str().unwrap()],
        &[("XDG_CACHE_HOME", &cache_home)],
    );

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

    assert!(self_compare.status.success());
    let self_json: serde_json::Value = serde_json::from_slice(&self_compare.stdout).unwrap();
    assert_eq!(self_json["equal"], true);
    assert_eq!(self_json["type"], "text");
    assert_comparison_cache_was_cleaned(&cache_home);
}

#[test]
fn self_compare_reports_temporary_copy_without_differences() {
    let temp = TempFixture::new();
    let cache_home = temp.path.join("cache");
    let source = fixture("text/left.txt");
    let output = run_with_env(
        &["self-compare", source.to_str().unwrap()],
        &[("XDG_CACHE_HOME", &cache_home)],
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("vs temporary copy"));
    assert!(stdout.contains("0 differing lines"));
    assert_comparison_cache_was_cleaned(&cache_home);
}

#[test]
fn self_compare_accepts_binary_files() {
    let temp = TempFixture::new();
    let cache_home = temp.path.join("cache");
    let source = temp.path.join("binary.bin");
    fs::write(&source, b"\x00ab\xff").unwrap();
    let output = run_with_env(
        &["self-compare", source.to_str().unwrap()],
        &[("XDG_CACHE_HOME", &cache_home)],
    );

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("vs temporary copy"));
    assert!(stdout.contains("0 differing bytes"));
    assert_comparison_cache_was_cleaned(&cache_home);
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

impl Drop for TempFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
