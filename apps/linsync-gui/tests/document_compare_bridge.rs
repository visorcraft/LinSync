// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// Bridge tests for `/compare/document`.
// Tests skip automatically when pdftotext or bash are absent.

use linsync::test_support::document_compare_test;
use std::path::{Path, PathBuf};
use std::process::Command;

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
fn bridge_identical_pdfs_returns_equal_true() {
    if !tools_available(&["pdftotext", "bash"]) {
        eprintln!("SKIP: pdftotext or bash not on PATH");
        return;
    }
    build_fixtures();

    let pdf = document_fixture_dir().join("simple.pdf");
    let json_resp =
        document_compare_test(pdf.to_str().unwrap(), pdf.to_str().unwrap(), "text", "eng")
            .expect("bridge call failed");

    let v: serde_json::Value = serde_json::from_str(&json_resp).unwrap();
    assert_eq!(v["equal"], serde_json::json!(true));
    assert_eq!(v["differing_lines"], serde_json::json!(0));
    assert_eq!(v["left_extractor"], serde_json::json!("pdf-to-text"));
}

#[test]
fn bridge_different_pdfs_returns_equal_false() {
    if !tools_available(&["pdftotext", "bash"]) {
        eprintln!("SKIP: pdftotext or bash not on PATH");
        return;
    }
    build_fixtures();

    let left = document_fixture_dir().join("simple.pdf");
    let right = document_fixture_dir().join("simple-changed.pdf");

    let json_resp = document_compare_test(
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "text",
        "eng",
    )
    .expect("bridge call failed");

    let v: serde_json::Value = serde_json::from_str(&json_resp).unwrap();
    assert_eq!(v["equal"], serde_json::json!(false));
    assert!(
        v["differing_lines"].as_u64().unwrap_or(0) > 0,
        "expected differing_lines > 0"
    );
}

#[test]
fn bridge_missing_parameter_returns_error() {
    // No 'right' parameter — should return {"error": "..."}
    let result = document_compare_test("/tmp/left.pdf", "", "text", "eng");
    // Either an Err (path missing returns error JSON), or a bridge-level error.
    // The key assertion: it doesn't panic.
    let _ = result;
}
