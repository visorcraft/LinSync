// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

#[cfg(feature = "document-compare")]
#[test]
fn document_compare_types_are_accessible() {
    use linsync_core::{DocumentCompareMode, DocumentCompareOptions};

    let opts = DocumentCompareOptions {
        mode: DocumentCompareMode::Text,
        ocr_language: "eng".to_owned(),
        retain_rendered_pages: false,
        timeout_secs: 30,
        temp_root: None,
    };
    assert_eq!(opts.ocr_language, "eng");
    assert!(!opts.retain_rendered_pages);
    assert_eq!(opts.timeout_secs, 30);

    // DocumentCompareMode round-trips as expected
    let mode = DocumentCompareMode::OcrText;
    assert!(matches!(mode, DocumentCompareMode::OcrText));
    let mode = DocumentCompareMode::Rendered;
    assert!(matches!(mode, DocumentCompareMode::Rendered));
}
