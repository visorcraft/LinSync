// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// CLI smoke tests for `linsync-cli compare --type document`.
// Tests skip automatically when pdftotext or bash are absent.

use std::path::{Path, PathBuf};
use std::process::Command;

fn cli_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_linsync-cli"))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn document_fixture_dir() -> PathBuf {
    workspace_root().join("tests/fixtures/document")
}

fn tools_available(tools: &[&str]) -> bool {
    tools.iter().all(|tool| {
        Command::new("which")
            .arg(tool)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

fn build_fixtures() {
    let dir = document_fixture_dir();
    let status = Command::new("bash")
        .arg(dir.join("build.sh"))
        .arg(&dir)
        .status()
        .expect("failed to run document/build.sh");
    assert!(status.success(), "document/build.sh failed");
}

#[test]
fn document_compare_unknown_type_exits_nonzero() {
    // Verify --type flag validation works (no fixture needed).
    let out = Command::new(cli_bin())
        .args(["compare", "--type", "unknown_xyz", "/tmp/a", "/tmp/b"])
        .output()
        .expect("run linsync-cli");
    assert_ne!(
        out.status.code(),
        Some(0),
        "unknown compare type must exit nonzero"
    );
}

#[test]
fn document_compare_identical_pdfs_exit_0() {
    if !tools_available(&["pdftotext", "bash"]) {
        eprintln!("SKIP: pdftotext or bash not on PATH");
        return;
    }
    build_fixtures();

    let pdf = document_fixture_dir().join("simple.pdf");
    let out = Command::new(cli_bin())
        .args([
            "compare",
            "--type",
            "document",
            "--json",
            pdf.to_str().unwrap(),
            pdf.to_str().unwrap(),
        ])
        .output()
        .expect("run linsync-cli");

    assert_eq!(
        out.status.code(),
        Some(0),
        "identical PDFs must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON output");
    assert_eq!(json["equal"], serde_json::json!(true));
    assert_eq!(json["differing_lines"], serde_json::json!(0));
}

#[test]
fn document_compare_different_pdfs_exit_1() {
    if !tools_available(&["pdftotext", "bash"]) {
        eprintln!("SKIP: pdftotext or bash not on PATH");
        return;
    }
    build_fixtures();

    let left = document_fixture_dir().join("simple.pdf");
    let right = document_fixture_dir().join("simple-changed.pdf");

    let out = Command::new(cli_bin())
        .args([
            "compare",
            "--type",
            "document",
            "--json",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
        ])
        .output()
        .expect("run linsync-cli");

    assert_eq!(
        out.status.code(),
        Some(1),
        "different PDFs must exit 1; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON output");
    assert_eq!(json["equal"], serde_json::json!(false));
    assert!(
        json["differing_lines"].as_u64().unwrap_or(0) > 0,
        "expected differing_lines > 0"
    );
}

#[test]
fn document_compare_invalid_page_range_rejected() {
    // Pure arg validation — needs no external tools or fixtures.
    let out = Command::new(cli_bin())
        .args([
            "compare",
            "--type",
            "document",
            "--document-mode",
            "rendered",
            "--document-pages",
            "5-2",
            "/tmp/a.pdf",
            "/tmp/b.pdf",
        ])
        .output()
        .expect("run linsync-cli");
    assert_eq!(
        out.status.code(),
        Some(2),
        "an empty/backwards page range is a usage error (exit 2)"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("page"),
        "error should explain the bad --document-pages value"
    );
}

#[test]
fn document_compare_rendered_mode_with_page_range() {
    // Regression: `--document-mode rendered` must be accepted (it was rejected
    // by an outdated flag validator), and `--document-pages` must select pages.
    if !tools_available(&["pdftoppm", "bash"]) {
        eprintln!("SKIP: pdftoppm or bash not on PATH");
        return;
    }
    build_fixtures();
    let pdf = document_fixture_dir().join("simple.pdf");
    if !pdf.exists() {
        eprintln!("SKIP: simple.pdf fixture missing");
        return;
    }

    // Rendered compare of a document against itself: every page equal → exit 0,
    // and the JSON reports rendered mode. Restrict to page 1 to exercise the
    // page-range path on a possibly-single-page fixture.
    let out = Command::new(cli_bin())
        .args([
            "compare",
            "--type",
            "document",
            "--document-mode",
            "rendered",
            "--document-pages",
            "1",
            "--json",
            pdf.to_str().unwrap(),
            pdf.to_str().unwrap(),
        ])
        .output()
        .expect("run linsync-cli");
    assert_eq!(
        out.status.code(),
        Some(0),
        "rendered compare of identical pages must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON output");
    assert_eq!(json["mode"], serde_json::json!("rendered"));
    assert_eq!(json["equal"], serde_json::json!(true));
    assert_eq!(
        json["pages"].as_array().map(|a| a.len()),
        Some(1),
        "only the single selected page is reported"
    );
}
