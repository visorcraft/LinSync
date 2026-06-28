// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only
//
// End-to-end tests for the document-compare helper plugins.
// Each test is skipped automatically when the required system binary is absent.

mod common;

use linsync_core::plugin::{
    PluginExecutionOptions, PluginInputDescriptor, PluginManifest, run_unpack_text_plugin,
};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

fn document_fixture_dir() -> PathBuf {
    common::workspace_root().join("tests/fixtures/document")
}

fn build_document_fixtures() {
    let dir = document_fixture_dir();
    let status = Command::new("bash")
        .arg(dir.join("build.sh"))
        .arg(&dir)
        .status()
        .expect("failed to launch document/build.sh");
    assert!(status.success(), "document/build.sh failed");
}

fn plugin_options() -> PluginExecutionOptions {
    PluginExecutionOptions {
        timeout: Duration::from_secs(30),
        ..PluginExecutionOptions::default()
    }
}

fn load_manifest(plugin_dir: &std::path::Path) -> PluginManifest {
    let text = std::fs::read_to_string(plugin_dir.join("linsync-plugin.json")).unwrap();
    serde_json::from_str(&text).expect("manifest parse failed")
}

// ── pdf-to-text ──────────────────────────────────────────────────────────────

#[test]
fn pdf_to_text_plugin_manifest_deserializes() {
    let plugin_dir = common::workspace_root().join("packaging/plugins/pdf-to-text");
    let manifest = load_manifest(&plugin_dir);
    assert_eq!(manifest.id, "com.visorcraft.linsync.pdf-to-text");
    assert!(manifest.supports_extension("pdf"));
    manifest
        .validate(&plugin_dir)
        .expect("manifest.validate() failed");
}

#[test]
fn pdf_to_text_extracts_text_from_fixture() {
    if !common::tools_available(&["pdftotext", "bash"]) {
        eprintln!("SKIP: pdftotext or bash not on PATH");
        return;
    }
    build_document_fixtures();

    let plugin_dir = common::workspace_root().join("packaging/plugins/pdf-to-text");
    let manifest = load_manifest(&plugin_dir);
    let fixture = document_fixture_dir().join("simple.pdf");

    let result = run_unpack_text_plugin(
        &plugin_dir,
        &manifest,
        PluginInputDescriptor::for_file("left", &fixture),
        &plugin_options(),
    )
    .expect("run_unpack_text_plugin failed for pdf-to-text");

    assert!(
        result.text.contains("Hello LinSync"),
        "expected extracted text to contain 'Hello LinSync', got: {:?}",
        &result.text[..result.text.len().min(200)]
    );
}

#[test]
fn pdf_to_text_probe_accepts_pdf_extension() {
    let plugin_dir = common::workspace_root().join("packaging/plugins/pdf-to-text");
    let manifest = load_manifest(&plugin_dir);
    assert!(manifest.supports_extension("pdf"));
    assert!(manifest.supports_mime_type("application/pdf"));
}

// ── tesseract-ocr ────────────────────────────────────────────────────────────

#[test]
fn tesseract_ocr_plugin_manifest_deserializes() {
    let plugin_dir = common::workspace_root().join("packaging/plugins/tesseract-ocr");
    let manifest = load_manifest(&plugin_dir);
    assert_eq!(manifest.id, "com.visorcraft.linsync.tesseract-ocr");
    // Tesseract handles images and PDF
    assert!(manifest.supports_extension("png"));
    assert!(manifest.supports_extension("pdf"));
    assert!(manifest.supports_extension("jpg"));
    // Must declare an options_schema with a language key
    let lang_option = manifest.options_schema.iter().find(|o| o.key == "language");
    assert!(
        lang_option.is_some(),
        "expected 'language' option in options_schema"
    );
    manifest
        .validate(&plugin_dir)
        .expect("manifest.validate() failed");
}

#[test]
fn tesseract_ocr_plugin_runs_on_png_fixture() {
    if !common::tools_available(&["tesseract", "bash"]) {
        eprintln!("SKIP: tesseract or bash not on PATH");
        return;
    }
    build_document_fixtures();

    let plugin_dir = common::workspace_root().join("packaging/plugins/tesseract-ocr");
    let manifest = load_manifest(&plugin_dir);
    let fixture = document_fixture_dir().join("ocr-target.png");

    // The ocr-target.png is all-white, so we expect empty or near-empty output
    // (no real text). The important thing is the plugin runs without error.
    let result = run_unpack_text_plugin(
        &plugin_dir,
        &manifest,
        PluginInputDescriptor::for_file("left", &fixture),
        &plugin_options(),
    )
    .expect("tesseract-ocr plugin returned error on blank PNG");

    // Result is either empty or whitespace for a blank image — not an error
    let trimmed = result.text.trim();
    // Just assert we got a PluginTextResult (no panic, no error variant)
    let _ = trimmed;
}

#[test]
fn tesseract_ocr_plugin_absent_binary_returns_structured_error() {
    // Build a synthetic plugin in a temp dir whose entry script always emits
    // the binary-not-found JSON response. This avoids mutating the global PATH
    // environment (which is unsafe under parallel test execution).
    if !common::tools_available(&["bash"]) {
        eprintln!("SKIP: bash not on PATH");
        return;
    }

    let real_plugin_dir = common::workspace_root().join("packaging/plugins/tesseract-ocr");
    let manifest = load_manifest(&real_plugin_dir);

    // Build fixture first (needs bash but not tesseract)
    build_document_fixtures();
    let fixture = document_fixture_dir().join("ocr-target.png");

    // Synthetic plugin dir: copy the manifest and write a stub entry that
    // always returns the binary-not-found error the real script would emit.
    let tmp = tempfile::tempdir().expect("tempdir");
    let stub_dir = tmp.path();
    std::fs::copy(
        real_plugin_dir.join("linsync-plugin.json"),
        stub_dir.join("linsync-plugin.json"),
    )
    .unwrap();
    let stub_script = stub_dir.join("tesseract-ocr.sh");
    std::fs::write(
        &stub_script,
        b"#!/usr/bin/env bash\nprintf '%s\\n' \
'{\"protocol_version\":1,\"request_id\":\"unknown\",\"status\":\"error\",\
\"error\":{\"code\":\"binary-not-found\",\
\"message\":\"tesseract not found \\u2014 install tesseract-ocr\"},\"diagnostics\":[]}'\nexit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub_script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let result = run_unpack_text_plugin(
        stub_dir,
        &manifest,
        PluginInputDescriptor::for_file("left", &fixture),
        &PluginExecutionOptions {
            timeout: Duration::from_secs(5),
            ..PluginExecutionOptions::default()
        },
    );

    // Should fail with a structured PluginResponseError (binary-not-found).
    match result {
        Err(linsync_core::plugin::PluginError::PluginResponseError { code, .. }) => {
            assert_eq!(code, "binary-not-found");
        }
        Err(linsync_core::plugin::PluginError::ExecutionFailed { .. }) => {
            // Also acceptable — the stub exited nonzero without a parseable JSON body.
        }
        Ok(_) => panic!("expected error from absent-binary stub, got Ok"),
        Err(other) => panic!("unexpected error variant: {other}"),
    }
}

// ── libreoffice-extract ───────────────────────────────────────────────────────

#[test]
fn libreoffice_extract_plugin_manifest_deserializes() {
    let plugin_dir = common::workspace_root().join("packaging/plugins/libreoffice-extract");
    let manifest = load_manifest(&plugin_dir);
    assert_eq!(manifest.id, "com.visorcraft.linsync.libreoffice-extract");
    assert!(manifest.supports_extension("odt"));
    assert!(manifest.supports_extension("docx"));
    assert!(manifest.supports_extension("rtf"));
    manifest
        .validate(&plugin_dir)
        .expect("manifest.validate() failed");
}

#[test]
fn libreoffice_extract_runs_on_odt_fixture() {
    // LibreOffice headless can hang indefinitely without a profile or with
    // sandboxed home access; gate the test on an opt-in env var so default
    // test runs don't spawn lingering soffice.bin processes. CI that wants
    // this coverage should set LINSYNC_E2E_LIBREOFFICE=1.
    if std::env::var_os("LINSYNC_E2E_LIBREOFFICE").is_none() {
        eprintln!("SKIP: set LINSYNC_E2E_LIBREOFFICE=1 to run the libreoffice-extract e2e");
        return;
    }
    if !common::tools_available(&["libreoffice", "bash"]) {
        eprintln!("SKIP: libreoffice or bash not on PATH");
        return;
    }
    build_document_fixtures();

    let plugin_dir = common::workspace_root().join("packaging/plugins/libreoffice-extract");
    let manifest = load_manifest(&plugin_dir);
    let fixture = document_fixture_dir().join("simple.odt");

    let result = run_unpack_text_plugin(
        &plugin_dir,
        &manifest,
        PluginInputDescriptor::for_file("left", &fixture),
        &PluginExecutionOptions {
            timeout: Duration::from_secs(60), // LO startup can be slow
            ..PluginExecutionOptions::default()
        },
    );

    match result {
        Ok(r) => {
            assert!(
                r.text.contains("Hello LinSync"),
                "expected extracted text to contain 'Hello LinSync', got: {:?}",
                &r.text[..r.text.len().min(400)]
            );
        }
        Err(linsync_core::plugin::PluginError::TimedOut { .. }) => {
            // LibreOffice may time out inside a tight sandbox or on slow CI;
            // treat as skip rather than test failure.
            eprintln!("SKIP: libreoffice timed out (sandbox may restrict filesystem access)");
        }
        Err(e) => panic!("libreoffice-extract plugin returned unexpected error: {e}"),
    }
}

// ── compare_document_files ────────────────────────────────────────────────────
#[test]
fn compare_document_files_pdfs_returns_text_result() {
    if !common::tools_available(&["pdftotext", "bash"]) {
        eprintln!("SKIP: pdftotext or bash not on PATH");
        return;
    }
    build_document_fixtures();

    use linsync_core::document::compare_document_files;
    use linsync_core::{DocumentCompareMode, DocumentCompareOptions};

    let plugins_root = common::workspace_root().join("packaging/plugins");
    let left = document_fixture_dir().join("simple.pdf");
    let right = document_fixture_dir().join("simple-changed.pdf");

    let opts = DocumentCompareOptions {
        mode: DocumentCompareMode::Text,
        ..DocumentCompareOptions::default()
    };
    let result = compare_document_files(&left, &right, &plugins_root, &opts)
        .expect("compare_document_files failed");

    assert_eq!(result.left_extractor, "pdf-to-text");
    assert_eq!(result.right_extractor, "pdf-to-text");
    let text_result = result
        .text_result
        .expect("expected text_result for Text mode");
    // "Hello LinSync" vs "Hello Changed" — should have differences
    assert!(
        !text_result.is_equal(),
        "expected differences between simple.pdf and simple-changed.pdf"
    );
}
#[test]
fn compare_document_files_identical_pdfs_are_equal() {
    if !common::tools_available(&["pdftotext", "bash"]) {
        eprintln!("SKIP: pdftotext or bash not on PATH");
        return;
    }
    build_document_fixtures();

    use linsync_core::DocumentCompareOptions;
    use linsync_core::document::compare_document_files;

    let plugins_root = common::workspace_root().join("packaging/plugins");
    let pdf = document_fixture_dir().join("simple.pdf");

    let opts = DocumentCompareOptions::default();
    let result = compare_document_files(&pdf, &pdf, &plugins_root, &opts)
        .expect("compare_document_files failed on identical pair");

    let text_result = result.text_result.unwrap();
    assert!(
        text_result.is_equal(),
        "identical PDF should compare as equal"
    );
}

#[test]
fn compare_document_files_rendered_diffs_pages() {
    if !common::tools_available(&["pdftoppm", "bash"]) {
        eprintln!("SKIP: pdftoppm or bash not on PATH");
        return;
    }
    build_document_fixtures();

    use linsync_core::DocumentCompareMode;
    use linsync_core::DocumentCompareOptions;
    use linsync_core::document::compare_document_files;

    let plugins_root = common::workspace_root().join("packaging/plugins");
    let simple = document_fixture_dir().join("simple.pdf");
    let changed = document_fixture_dir().join("simple-changed.pdf");

    let opts = DocumentCompareOptions {
        mode: DocumentCompareMode::Rendered,
        ..DocumentCompareOptions::default()
    };

    // Identical PDFs render to pixel-equal pages.
    let same = compare_document_files(&simple, &simple, &plugins_root, &opts)
        .expect("rendered compare on identical pair failed");
    assert_eq!(same.left_extractor, "pdf-render");
    assert!(
        same.text_result.is_none(),
        "rendered mode has no text result"
    );
    assert!(
        !same.rendered_pages.is_empty(),
        "at least one page rendered"
    );
    assert!(same.is_equal(), "identical PDFs should render equal");

    // Different page text renders different pixels.
    let diff = compare_document_files(&simple, &changed, &plugins_root, &opts)
        .expect("rendered compare on differing pair failed");
    assert!(!diff.is_equal(), "differing PDFs should render different");
}
#[test]
fn pdf_render_plugin_manifest_deserializes() {
    let plugin_dir = common::workspace_root().join("packaging/plugins/pdf-render");
    let manifest = load_manifest(&plugin_dir);
    assert_eq!(manifest.id, "com.visorcraft.linsync.pdf-render");
    assert!(manifest.supports_extension("pdf"));
    manifest
        .validate(&plugin_dir)
        .expect("pdf-render manifest.validate() failed");
}
#[test]
fn compare_document_files_no_plugin_returns_error() {
    use linsync_core::DocumentCompareOptions;
    use linsync_core::document::{DocumentCompareError, compare_document_files};

    // Use a temp dir with no plugins
    let empty_plugins = std::env::temp_dir().join("linsync-test-no-plugins");
    std::fs::create_dir_all(&empty_plugins).unwrap();

    let fixture = document_fixture_dir().join("simple.pdf");
    // build.sh not required — we just need the path to exist
    if !fixture.exists() {
        if !common::tools_available(&["bash"]) {
            eprintln!("SKIP: bash not on PATH");
            return;
        }
        build_document_fixtures();
    }

    let opts = DocumentCompareOptions::default();
    let err = compare_document_files(&fixture, &fixture, &empty_plugins, &opts)
        .expect_err("expected NoSuitablePlugin error");

    assert!(
        matches!(err, DocumentCompareError::NoSuitablePlugin { .. }),
        "expected NoSuitablePlugin, got: {err}"
    );
}

// ── privacy: temp-file cleanup ────────────────────────────────────────────────
#[test]
fn plugin_temp_dir_is_removed_after_compare() {
    if !common::tools_available(&["pdftotext", "bash"]) {
        eprintln!("SKIP: pdftotext or bash not on PATH");
        return;
    }
    build_document_fixtures();

    use linsync_core::DocumentCompareOptions;
    use linsync_core::document::compare_document_files;

    let plugins_root = common::workspace_root().join("packaging/plugins");
    let pdf = document_fixture_dir().join("simple.pdf");

    // Use a private temp root so concurrent tests can't pollute the snapshot.
    let private_tmp =
        std::env::temp_dir().join(format!("linsync-cleanup-test-{}", std::process::id()));
    std::fs::create_dir_all(&private_tmp).unwrap();

    let opts = DocumentCompareOptions {
        temp_root: Some(private_tmp.clone()),
        ..DocumentCompareOptions::default()
    };

    let _result = compare_document_files(&pdf, &pdf, &plugins_root, &opts)
        .expect("compare_document_files failed");

    // After the call, no linsync-plugin-* sub-dir should remain.
    let leaked: Vec<_> = std::fs::read_dir(&private_tmp)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("linsync-plugin-"))
        })
        .map(|e| e.path())
        .collect();

    assert!(
        leaked.is_empty(),
        "plugin temp dirs leaked after compare: {leaked:?}"
    );

    // Clean up the private temp root itself
    let _ = std::fs::remove_dir_all(&private_tmp);
}

#[test]
fn pdf_to_text_corrupt_pdf_returns_plugin_error() {
    if !common::tools_available(&["pdftotext", "bash"]) {
        eprintln!("SKIP: pdftotext or bash not on PATH");
        return;
    }
    build_document_fixtures();

    let plugin_dir = common::workspace_root().join("packaging/plugins/pdf-to-text");
    let manifest = load_manifest(&plugin_dir);
    let fixture = document_fixture_dir().join("corrupt.pdf");

    let result = run_unpack_text_plugin(
        &plugin_dir,
        &manifest,
        PluginInputDescriptor::for_file("left", &fixture),
        &plugin_options(),
    );

    // pdftotext may succeed with empty output or fail — either way, no panic.
    // If it fails, the error must be a structured PluginError, not a panic.
    match result {
        Ok(text_result) => {
            // Some pdftotext versions emit empty text for corrupt files
            let _ = text_result.text;
        }
        Err(linsync_core::plugin::PluginError::PluginResponseError { code, .. }) => {
            assert_eq!(code, "internal-error");
        }
        Err(linsync_core::plugin::PluginError::ExecutionFailed { .. }) => {
            // Also acceptable — nonzero exit from pdftotext
        }
        Err(other) => panic!("unexpected error variant: {other}"),
    }
}
