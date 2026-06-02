// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// OCR word-position threading, exercised with a synthetic OCR-engine plugin so
// the test needs no Tesseract install. The fake helper emits canned word
// positions only when the request asks for them (`want_positions`), proving the
// opt-in flag reaches the plugin and the positions thread back into the
// `DocumentCompareResult`.
#![cfg(feature = "document-compare")]

use std::path::Path;

use linsync_core::document::{DocumentCompareMode, DocumentCompareOptions, compare_document_files};

/// Install a fake OCR plugin under the real tesseract-ocr id. When the incoming
/// request contains `"want_positions":true`, it returns text plus a one-line,
/// two-word position array; otherwise it returns text only.
fn install_fake_ocr_plugin(plugins_root: &Path) {
    let dir = plugins_root.join("com.visorcraft.linsync.tesseract-ocr");
    std::fs::create_dir_all(&dir).unwrap();
    let helper = dir.join("ocr.sh");
    // The helper echoes a fixed transcript. The positions block is only emitted
    // when the request opted in, so the test can assert the flag round-trips.
    let script = r#"#!/bin/sh
req=$(cat)
rid=$(printf '%s' "$req" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
role=$(printf '%s' "$req" | sed -n 's/.*"role":"\([^"]*\)".*/\1/p')
positions=""
case "$req" in
  *'"want_positions":true'*)
    positions=',"word_positions":[[{"text":"hello","line":0,"x":10,"y":20,"width":40,"height":15,"confidence":96},{"text":"world","line":0,"x":60,"y":20,"width":45,"height":15,"confidence":91}]]'
    ;;
esac
printf '{"protocol_version":1,"request_id":"%s","status":"ok","outputs":[{"role":"%s","kind":"text","inline_text":"hello world","encoding":"utf-8","line_ending":"lf"%s}],"diagnostics":[]}\n' "$rid" "$role" "$positions"
"#;
    std::fs::write(&helper, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&helper).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&helper, perms).unwrap();
    }
    std::fs::write(
        dir.join("linsync-plugin.json"),
        r#"{
          "schema_version": 1,
          "id": "com.visorcraft.linsync.tesseract-ocr",
          "name": "Fake OCR",
          "version": "1.0.0",
          "license": "GPL-3.0-only",
          "entry": ["./ocr.sh"],
          "classes": ["unpacker", "ocr_engine"],
          "mime_types": ["image/png"],
          "extensions": ["png"],
          "capabilities": ["unpack-text"],
          "deterministic": false,
          "sandbox": { "network": false, "writes_input": false, "requires_home_access": false },
          "options_schema": []
        }"#,
    )
    .unwrap();
}

fn scratch(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("linsync-ocr-pos-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn ocr_mode_threads_word_positions_into_result() {
    let tmp = scratch("ocr");
    let plugins_root = tmp.join("plugins");
    std::fs::create_dir_all(&plugins_root).unwrap();
    install_fake_ocr_plugin(&plugins_root);

    // Two image inputs (content irrelevant — the fake helper emits canned text).
    let left = tmp.join("a.png");
    let right = tmp.join("b.png");
    std::fs::write(&left, b"\x89PNG-left").unwrap();
    std::fs::write(&right, b"\x89PNG-right").unwrap();

    let opts = DocumentCompareOptions {
        mode: DocumentCompareMode::OcrText,
        temp_root: Some(tmp.clone()),
        ..DocumentCompareOptions::default()
    };
    let result = compare_document_files(&left, &right, &plugins_root, &opts).unwrap();

    // Positions threaded through for both sides.
    let left_pos = result
        .left_word_positions
        .as_ref()
        .expect("OCR mode requests and threads left positions");
    assert_eq!(left_pos.len(), 1, "one line of words");
    assert_eq!(left_pos[0].len(), 2, "two words on the line");
    assert_eq!(left_pos[0][0].text, "hello");
    assert_eq!(left_pos[0][0].x, 10);
    assert_eq!(left_pos[0][1].text, "world");
    assert_eq!(left_pos[0][1].confidence, Some(91));
    assert!(
        result.right_word_positions.is_some(),
        "right positions threaded too"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn text_mode_does_not_request_positions() {
    // The same plugin is also a plain text extractor for pngs; in Text mode the
    // engine must NOT receive want_positions, so no positions come back.
    let tmp = scratch("text");
    let plugins_root = tmp.join("plugins");
    std::fs::create_dir_all(&plugins_root).unwrap();
    install_fake_ocr_plugin(&plugins_root);

    let left = tmp.join("a.png");
    let right = tmp.join("b.png");
    std::fs::write(&left, b"x").unwrap();
    std::fs::write(&right, b"y").unwrap();

    // For pngs, Text mode also routes to the tesseract-ocr id (OCR is the only
    // sensible text extraction for images), but without requesting positions.
    let opts = DocumentCompareOptions {
        mode: DocumentCompareMode::Text,
        temp_root: Some(tmp.clone()),
        ..DocumentCompareOptions::default()
    };
    let result = compare_document_files(&left, &right, &plugins_root, &opts).unwrap();
    assert!(
        result.left_word_positions.is_none(),
        "Text mode must not request OCR positions"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
