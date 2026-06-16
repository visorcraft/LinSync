// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

//! Document and OCR compare paths (feature-gated: `document-compare`).
//!
//! All helpers shell out — no Poppler/Tesseract/LibreOffice crate is linked.
//! All helpers run inside the Phase 6 sandbox (see `linsync_sandbox::run_sandboxed`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::plugin::{
    DiscoveredPlugin, PluginExecutionOptions, PluginInputDescriptor, PluginTextOperationOptions,
    discover_plugins, run_unpack_text_plugin_with_options,
};
use crate::text::{TextCompareOptions, compare_text};

/// Which extraction strategy to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentCompareMode {
    /// Extract text via a document-text plugin (pdftotext, libreoffice-extract).
    Text,
    /// Extract text via an OCR engine plugin (tesseract-ocr).
    OcrText,
    /// Render pages to images via pdftoppm, then compare via Phase 7 image compare.
    Rendered,
}

/// Options for a document or OCR compare.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentCompareOptions {
    /// Which extraction strategy to use (default: `Text`).
    pub mode: DocumentCompareMode,
    /// ISO 639-2 language code passed to Tesseract when `mode` is `OcrText` (default: `"eng"`).
    pub ocr_language: String,
    /// When `true`, intermediate rendered PNGs are kept in
    /// `$XDG_CACHE_HOME/linsync/rendered-pages/<session-id>/` until session close.
    /// When `false` (default), they are removed immediately after the image engine finishes.
    pub retain_rendered_pages: bool,
    /// Per-side helper timeout in seconds (default: 30).
    pub timeout_secs: u64,
    /// When set, only pages in this 1-based inclusive range are compared in
    /// `Rendered` mode; pages outside it are skipped and the result reports just
    /// the selected pages (by their 0-based absolute index). `None` (default)
    /// compares every page. The renderer still produces every page; the range
    /// narrows the diff, so the user can target a section of a long document.
    pub page_range: Option<(usize, usize)>,
    /// Override for the plugin temp-dir root (normally `std::env::temp_dir()`).
    /// Used in tests to isolate temp-dir cleanup assertions.
    #[serde(skip, default)]
    pub temp_root: Option<std::path::PathBuf>,
}

impl Default for DocumentCompareOptions {
    fn default() -> Self {
        Self {
            mode: DocumentCompareMode::Text,
            ocr_language: "eng".to_owned(),
            retain_rendered_pages: false,
            timeout_secs: 30,
            page_range: None,
            temp_root: None,
        }
    }
}

/// Per-page outcome of a `Rendered` document compare (feature-independent
/// summary of the underlying image comparison).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RenderedPageSummary {
    /// Zero-based page index.
    pub page: usize,
    /// Whether the two rendered pages are pixel-equal.
    pub equal: bool,
    /// Fraction of differing pixels (0.0–1.0); 1.0 for a one-sided page.
    pub diff_ratio: f64,
    /// Number of differing pixels.
    pub differing_pixels: u64,
    /// True when the page exists on only one side (page-count mismatch).
    pub one_sided: bool,
    /// Path to the rendered left page PNG, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_path: Option<std::path::PathBuf>,
    /// Path to the rendered right page PNG, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_path: Option<std::path::PathBuf>,
}

/// Result produced by a document compare.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentCompareResult {
    /// Displayable name of the helper that extracted the left side (e.g. `"pdftotext"`).
    pub left_extractor: String,
    /// Displayable name of the helper that extracted the right side (e.g. `"pdftotext"`).
    pub right_extractor: String,
    /// Underlying text compare result (only populated when `mode` is `Text` or `OcrText`).
    pub text_result: Option<crate::text::TextCompareResult>,
    /// Per-page image comparison (only populated when `mode` is `Rendered`).
    pub rendered_pages: Vec<RenderedPageSummary>,
    /// Per-word OCR bounding boxes for the left side (grouped per line), when
    /// `mode` is `OcrText` and the OCR engine returned positions. `None`
    /// otherwise. Lets a client overlay per-word diff highlights on the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_word_positions: Option<Vec<Vec<crate::plugin::WordPosition>>>,
    /// Per-word OCR bounding boxes for the right side; see `left_word_positions`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_word_positions: Option<Vec<Vec<crate::plugin::WordPosition>>>,
}

impl DocumentCompareResult {
    /// Whether the documents compared equal (text result, or all rendered pages).
    pub fn is_equal(&self) -> bool {
        if let Some(text) = &self.text_result {
            return text.is_equal();
        }
        if !self.rendered_pages.is_empty() {
            return self.rendered_pages.iter().all(|p| p.equal);
        }
        true
    }

    /// Render a standalone HTML report of this document comparison so a saved
    /// result can be re-rendered by `report --from-json`.
    ///
    /// For `Text`/`OcrText` mode the meaningful artifact is the line-level text
    /// diff, so this reuses the (already fixture-tested) text-engine report. For
    /// `Rendered` mode it emits a per-page summary table with the extractor
    /// attribution (the page rasters themselves are not part of the result).
    pub fn to_html_report(&self) -> String {
        if let Some(text) = &self.text_result {
            return text.to_html_report();
        }

        let mut html = String::new();
        html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">\n");
        html.push_str("<title>LinSync document report</title>\n");
        html.push_str(
            "<style>\n\
             body{font-family:system-ui,sans-serif;margin:1.5rem;}\n\
             table{border-collapse:collapse;margin-top:0.5rem;}\n\
             td,th{border:1px solid #ccc;padding:2px 6px;font-family:monospace;text-align:left;}\n\
             .equal{color:#1a7f37;}\n\
             .diff{color:#b00020;}\n\
             </style>\n</head><body>\n",
        );
        html.push_str("<h1>LinSync document report</h1>\n");
        html.push_str(&format!(
            "<p>Rendered via {} (left) / {} (right).</p>\n",
            escape_document_html(&self.left_extractor),
            escape_document_html(&self.right_extractor)
        ));
        let differing = self.rendered_pages.iter().filter(|p| !p.equal).count();
        let status = if self.is_equal() {
            "<span class=\"equal\">equal</span>"
        } else {
            "<span class=\"diff\">different</span>"
        };
        html.push_str(&format!(
            "<p>Result: {status} — {} of {} page(s) differ.</p>\n",
            differing,
            self.rendered_pages.len()
        ));
        html.push_str("<table>\n<thead><tr><th>Page</th><th>Status</th><th>Diff ratio</th><th>Differing pixels</th></tr></thead>\n<tbody>\n");
        for page in &self.rendered_pages {
            let (class, label) = if page.one_sided {
                ("diff", "one-sided".to_owned())
            } else if page.equal {
                ("equal", "equal".to_owned())
            } else {
                ("diff", "different".to_owned())
            };
            html.push_str(&format!(
                "<tr><td>{}</td><td class=\"{class}\">{label}</td><td>{:.4}%</td><td>{}</td></tr>\n",
                page.page + 1,
                page.diff_ratio * 100.0,
                page.differing_pixels
            ));
        }
        html.push_str("</tbody></table>\n</body></html>\n");
        html
    }
}

/// Minimal HTML escaper for the few attribution strings rendered above.
fn escape_document_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Error returned when a document compare fails.
#[derive(Debug)]
pub enum DocumentCompareError {
    /// No plugin capable of handling the given MIME type / extension is installed.
    NoSuitablePlugin { path: String, mime_hint: String },
    /// The helper plugin returned an error.
    Plugin(crate::plugin::PluginError),
    /// The fallback text compare failed.
    Io(std::io::Error),
}

impl std::fmt::Display for DocumentCompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSuitablePlugin { path, mime_hint } => {
                write!(
                    f,
                    "no document-compare plugin for '{path}' (MIME hint: {mime_hint})"
                )
            }
            Self::Plugin(err) => write!(f, "plugin error: {err}"),
            Self::Io(err) => write!(f, "IO error: {err}"),
        }
    }
}

impl std::error::Error for DocumentCompareError {}

impl From<crate::plugin::PluginError> for DocumentCompareError {
    fn from(err: crate::plugin::PluginError) -> Self {
        Self::Plugin(err)
    }
}

impl From<std::io::Error> for DocumentCompareError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Detect a best-effort MIME type hint from the file extension.
///
/// Returns a bare string like `"application/pdf"` or `"unknown"`.
/// This is not a full MIME database lookup — it covers the document formats
/// this phase supports without pulling in the `mime_guess` crate.
pub fn mime_hint_from_path(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => "application/pdf".to_owned(),
        "docx" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_owned()
        }
        "odt" => "application/vnd.oasis.opendocument.text".to_owned(),
        "rtf" => "application/rtf".to_owned(),
        "png" => "image/png".to_owned(),
        "jpg" | "jpeg" => "image/jpeg".to_owned(),
        "tiff" | "tif" => "image/tiff".to_owned(),
        "webp" => "image/webp".to_owned(),
        _ => "unknown".to_owned(),
    }
}

/// Return the plugin ID best suited to extract text from a file at `path`,
/// given the requested `mode`.
///
/// Returns `None` when no plugin matches.
pub fn select_plugin_id(path: &Path, mode: DocumentCompareMode) -> Option<&'static str> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match mode {
        DocumentCompareMode::OcrText => Some("com.visorcraft.linsync.tesseract-ocr"),
        // Rendered mode discovers a renderer by the `pdf_renderer` class rather
        // than a fixed id (see `find_renderer_plugin`).
        DocumentCompareMode::Rendered => None,
        DocumentCompareMode::Text => match ext.as_str() {
            "pdf" => Some("com.visorcraft.linsync.pdf-to-text"),
            "docx" | "odt" | "rtf" => Some("com.visorcraft.linsync.libreoffice-extract"),
            "png" | "jpg" | "jpeg" | "tiff" | "tif" | "webp" => {
                // OCR is the only sensible text extraction for images
                Some("com.visorcraft.linsync.tesseract-ocr")
            }
            _ => None,
        },
    }
}

/// Discover the plugin that handles `path` for the given `mode` by searching
/// the plugin directory at `plugins_root`.
fn find_plugin_for(
    path: &Path,
    mode: DocumentCompareMode,
    plugins_root: &Path,
) -> Option<DiscoveredPlugin> {
    let target_id = select_plugin_id(path, mode)?;
    let discovery = discover_plugins(&[plugins_root.to_path_buf()]);
    discovery
        .plugins
        .into_iter()
        .find(|p| p.manifest.id == target_id)
}

/// Display name extracted from a plugin for the "extracted via" attribution header.
fn extractor_name(plugin: &DiscoveredPlugin) -> String {
    // Use the first entry word (the script name minus path and extension)
    plugin
        .manifest
        .entry
        .first()
        .and_then(|e| std::path::Path::new(e).file_stem())
        .and_then(|s| s.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| plugin.manifest.id.clone())
}

/// Extract text from one side using the discovered plugin. When `want_positions`
/// is set (OCR mode), also returns the per-word bounding boxes the engine
/// reported (`None` when it returned none).
type ExtractedText = (String, Option<Vec<Vec<crate::plugin::WordPosition>>>);

fn extract_text_with_plugin(
    plugin: &DiscoveredPlugin,
    path: &Path,
    role: &str,
    timeout_secs: u64,
    language: &str,
    want_positions: bool,
    temp_root: Option<&Path>,
) -> Result<ExtractedText, DocumentCompareError> {
    let opts = PluginExecutionOptions {
        timeout: Duration::from_secs(timeout_secs),
        temp_root: temp_root.map(Path::to_path_buf),
        ..PluginExecutionOptions::default()
    };
    let operation_options = PluginTextOperationOptions {
        language: Some(language.to_owned()),
        want_positions,
        ..PluginTextOperationOptions::default()
    };
    let input = PluginInputDescriptor::for_file(role, path);
    let text_result = run_unpack_text_plugin_with_options(
        &plugin.root,
        &plugin.manifest,
        input,
        &operation_options,
        &opts,
    )?;
    Ok((text_result.text, text_result.word_positions))
}

/// Compare two document files using the appropriate helper plugin.
///
/// `plugins_root` is the directory containing installed plugin sub-directories
/// (e.g. `<workspace>/packaging/plugins` in development, or
/// `/usr/share/linsync/plugins` in a system install).
///
/// When `mode` is `Rendered`, pages are rasterized via a `pdf_renderer`
/// plugin and diffed through the image engine (requires the `image-compare`
/// feature). Without that feature, returns `DocumentCompareError::NoSuitablePlugin`.
pub fn compare_document_files(
    left: &Path,
    right: &Path,
    plugins_root: &Path,
    options: &DocumentCompareOptions,
) -> Result<DocumentCompareResult, DocumentCompareError> {
    if matches!(options.mode, DocumentCompareMode::Rendered) {
        return compare_document_rendered(left, right, plugins_root, options);
    }

    let left_plugin = find_plugin_for(left, options.mode, plugins_root).ok_or_else(|| {
        DocumentCompareError::NoSuitablePlugin {
            path: left.display().to_string(),
            mime_hint: mime_hint_from_path(left),
        }
    })?;

    let right_plugin = find_plugin_for(right, options.mode, plugins_root).ok_or_else(|| {
        DocumentCompareError::NoSuitablePlugin {
            path: right.display().to_string(),
            mime_hint: mime_hint_from_path(right),
        }
    })?;

    let left_name = extractor_name(&left_plugin);
    let right_name = extractor_name(&right_plugin);

    let temp_root = options.temp_root.as_deref();
    // Request per-word positions only for OCR mode (the only path that can
    // produce them); text extractors ignore the flag.
    let want_positions = matches!(options.mode, DocumentCompareMode::OcrText);
    let (left_text, left_word_positions) = extract_text_with_plugin(
        &left_plugin,
        left,
        "left",
        options.timeout_secs,
        &options.ocr_language,
        want_positions,
        temp_root,
    )?;
    let (right_text, right_word_positions) = extract_text_with_plugin(
        &right_plugin,
        right,
        "right",
        options.timeout_secs,
        &options.ocr_language,
        want_positions,
        temp_root,
    )?;

    let left_display = left.file_name().and_then(|n| n.to_str()).unwrap_or("left");
    let right_display = right
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("right");

    let text_result = compare_text(
        left_display,
        &left_text,
        right_display,
        &right_text,
        &TextCompareOptions::default(),
    );

    Ok(DocumentCompareResult {
        left_extractor: left_name,
        right_extractor: right_name,
        text_result: Some(text_result),
        rendered_pages: Vec::new(),
        left_word_positions,
        right_word_positions,
    })
}

/// Find an installed `pdf_renderer` plugin under `plugins_root`.
fn find_renderer_plugin(plugins_root: &Path) -> Option<DiscoveredPlugin> {
    let discovery = discover_plugins(&[plugins_root.to_path_buf()]);
    discovery.plugins.into_iter().find(|p| {
        p.manifest
            .classes
            .contains(&crate::plugin::PluginClass::PdfRenderer)
    })
}

/// Rendered-mode document compare: rasterize both documents to page images via
/// a `pdf_renderer` plugin, then diff corresponding pages through the image
/// engine. Requires the `image-compare` feature.
#[cfg(feature = "image-compare")]
/// RAII guard for the temporary directories created by the rendered document
/// compare. On drop it removes both sides unless explicitly released, so error
/// paths cannot leak the rendered-page PNG caches.
struct RenderedPageDirs {
    left: Option<PathBuf>,
    right: Option<PathBuf>,
}

impl RenderedPageDirs {
    fn release(&mut self) {
        self.left.take();
        self.right.take();
    }
}

impl Drop for RenderedPageDirs {
    fn drop(&mut self) {
        if let Some(dir) = self.left.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
        if let Some(dir) = self.right.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

fn compare_document_rendered(
    left: &Path,
    right: &Path,
    plugins_root: &Path,
    options: &DocumentCompareOptions,
) -> Result<DocumentCompareResult, DocumentCompareError> {
    let renderer = find_renderer_plugin(plugins_root).ok_or_else(|| {
        DocumentCompareError::NoSuitablePlugin {
            path: left.display().to_string(),
            mime_hint: "no pdf_renderer plugin installed".to_owned(),
        }
    })?;
    let name = extractor_name(&renderer);

    // Persistent output dirs for the rendered PNGs (the helper's own temp dir is
    // ephemeral). Unique per side and process so concurrent compares don't clash.
    let base = options
        .temp_root
        .clone()
        .unwrap_or_else(std::env::temp_dir)
        .join("linsync-rendered-pages");
    let path_tag = |p: &Path| -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        p.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    };
    let pid = std::process::id();
    let left_dir = base.join(format!("{pid}-{}-left", path_tag(left)));
    let right_dir = base.join(format!("{pid}-{}-right", path_tag(right)));
    std::fs::create_dir_all(&left_dir)?;
    std::fs::create_dir_all(&right_dir)?;
    let mut dir_guard = RenderedPageDirs {
        left: Some(left_dir.clone()),
        right: Some(right_dir.clone()),
    };

    let exec = PluginExecutionOptions {
        timeout: Duration::from_secs(options.timeout_secs),
        temp_root: options.temp_root.clone(),
        ..PluginExecutionOptions::default()
    };
    let render = |doc: &Path, out: &Path| -> Result<Vec<String>, DocumentCompareError> {
        let response = crate::plugin::run_render_pages_plugin(
            &renderer.root,
            &renderer.manifest,
            &doc.to_string_lossy(),
            out,
            &exec,
        )?;
        if response.ok {
            Ok(response.pages)
        } else {
            Err(DocumentCompareError::Plugin(
                crate::plugin::PluginError::PluginResponseError {
                    code: "render_failed".to_owned(),
                    message: response
                        .error
                        .unwrap_or_else(|| format!("renderer failed for '{}'", doc.display())),
                    diagnostics: Vec::new(),
                },
            ))
        }
    };
    let left_pages = render(left, &left_dir)?;
    let right_pages = render(right, &right_dir)?;

    let img_opts = crate::image::ImageCompareOptions::default();
    let page_count = left_pages.len().max(right_pages.len());
    // Restrict the compared pages to the requested 1-based inclusive range
    // (clamped to the rendered page count); `None` compares every page.
    let (start, end) = match options.page_range {
        Some((first, last)) => {
            let start = first.saturating_sub(1).min(page_count);
            let end = last.min(page_count).max(start);
            (start, end)
        }
        None => (0, page_count),
    };
    let mut rendered_pages = Vec::with_capacity(end.saturating_sub(start));
    for index in start..end {
        match (left_pages.get(index), right_pages.get(index)) {
            (Some(lp), Some(rp)) => {
                let cmp = crate::image::compare_images(Path::new(lp), Path::new(rp), &img_opts)
                    .map_err(|e| DocumentCompareError::Io(std::io::Error::other(e.to_string())))?;
                rendered_pages.push(RenderedPageSummary {
                    page: index,
                    equal: cmp.equal,
                    diff_ratio: cmp.diff_ratio,
                    differing_pixels: cmp.differing_pixels,
                    one_sided: false,
                    left_path: Some(PathBuf::from(lp)),
                    right_path: Some(PathBuf::from(rp)),
                });
            }
            (Some(lp), None) => rendered_pages.push(RenderedPageSummary {
                page: index,
                equal: false,
                diff_ratio: 1.0,
                differing_pixels: 0,
                one_sided: true,
                left_path: Some(PathBuf::from(lp)),
                right_path: None,
            }),
            (None, Some(rp)) => rendered_pages.push(RenderedPageSummary {
                page: index,
                equal: false,
                diff_ratio: 1.0,
                differing_pixels: 0,
                one_sided: true,
                left_path: None,
                right_path: Some(PathBuf::from(rp)),
            }),
            (None, None) => rendered_pages.push(RenderedPageSummary {
                page: index,
                equal: false,
                diff_ratio: 1.0,
                differing_pixels: 0,
                one_sided: true,
                left_path: None,
                right_path: None,
            }),
        }
    }

    if options.retain_rendered_pages {
        // Ownership transfers to the caller (via the page paths in the result).
        dir_guard.release();
    }

    Ok(DocumentCompareResult {
        left_extractor: name.clone(),
        right_extractor: name,
        text_result: None,
        rendered_pages,
        left_word_positions: None,
        right_word_positions: None,
    })
}

/// Without the image engine, rendered mode cannot diff the pages.
#[cfg(not(feature = "image-compare"))]
fn compare_document_rendered(
    left: &Path,
    _right: &Path,
    _plugins_root: &Path,
    _options: &DocumentCompareOptions,
) -> Result<DocumentCompareResult, DocumentCompareError> {
    Err(DocumentCompareError::NoSuitablePlugin {
        path: left.display().to_string(),
        mime_hint: "rendered mode requires the image-compare feature".to_owned(),
    })
}

#[cfg(test)]
mod roundtrip_tests {
    use super::*;
    use crate::text::{TextCompareOptions, compare_text};

    fn text_mode_result() -> DocumentCompareResult {
        let text = compare_text(
            "left.pdf",
            "alpha\nbeta\n",
            "right.pdf",
            "alpha\nBETA\n",
            &TextCompareOptions::default(),
        );
        DocumentCompareResult {
            left_extractor: "pdftotext".to_owned(),
            right_extractor: "pdftotext".to_owned(),
            text_result: Some(text),
            rendered_pages: Vec::new(),
            left_word_positions: None,
            right_word_positions: None,
        }
    }

    fn rendered_mode_result() -> DocumentCompareResult {
        DocumentCompareResult {
            left_extractor: "pdftoppm".to_owned(),
            right_extractor: "pdftoppm".to_owned(),
            text_result: None,
            left_word_positions: None,
            right_word_positions: None,
            rendered_pages: vec![
                RenderedPageSummary {
                    page: 0,
                    equal: true,
                    diff_ratio: 0.0,
                    differing_pixels: 0,
                    one_sided: false,
                    ..Default::default()
                },
                RenderedPageSummary {
                    page: 1,
                    equal: false,
                    diff_ratio: 0.25,
                    differing_pixels: 42,
                    one_sided: false,
                    ..Default::default()
                },
            ],
        }
    }

    #[test]
    fn document_result_json_round_trips_text_mode() {
        let result = text_mode_result();
        let json = serde_json::to_string(&result).unwrap();
        let back: DocumentCompareResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.left_extractor, "pdftotext");
        assert!(back.text_result.is_some());
        assert_eq!(back.is_equal(), result.is_equal());
        assert!(!back.is_equal(), "the BETA line differs");
    }

    #[test]
    fn document_result_json_round_trips_rendered_mode() {
        let result = rendered_mode_result();
        let json = serde_json::to_string(&result).unwrap();
        let back: DocumentCompareResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.rendered_pages.len(), 2);
        assert!(back.text_result.is_none());
        assert!(!back.is_equal());
        assert_eq!(back.rendered_pages[1].differing_pixels, 42);
    }

    #[test]
    fn text_mode_report_reuses_text_engine_report() {
        let html = text_mode_result().to_html_report();
        assert!(html.contains("<html"));
        // The text-engine report carries the changed content.
        assert!(html.contains("BETA") || html.contains("beta"));
    }

    #[test]
    fn rendered_mode_report_lists_pages_and_extractor() {
        let html = rendered_mode_result().to_html_report();
        assert!(html.contains("LinSync document report"));
        assert!(html.contains("pdftoppm"), "extractor attribution rendered");
        assert!(html.contains("different"), "the differing page is flagged");
        assert!(html.contains("Page"), "per-page table header");
    }
}

#[cfg(all(test, feature = "image-compare"))]
mod rendered_tests {
    use super::*;
    use std::io::Write;

    /// Install a `pdf_renderer` fixture plugin. Its helper "renders" one copy of
    /// a bundled 1×1 PNG (kept inside the plugin dir, which the sandbox lets the
    /// helper read via its working directory) per line in the source document,
    /// writing the pages into `$LINSYNC_PLUGIN_TEMP_DIR`. This keeps the fixture
    /// sandbox-independent — it never reads a file outside the plugin dir.
    fn install_renderer_plugin(plugins_root: &Path) {
        let dir = plugins_root.join("test.renderer");
        std::fs::create_dir_all(&dir).unwrap();
        // Bundle the page image inside the plugin dir.
        let img: image::RgbaImage =
            image::ImageBuffer::from_pixel(1, 1, image::Rgba([10, 160, 10, 255]));
        img.save(dir.join("page-fixture.png"))
            .expect("write png fixture");
        let helper = dir.join("render.sh");
        // The helper runs with its working directory set to the plugin dir, so
        // it copies the bundled PNG via the relative "./page-fixture.png".
        let script = "#!/bin/sh\n\
             req=$(cat)\n\
             source=$(printf '%s' \"$req\" | sed -n 's/.*\"source\":\"\\([^\"]*\\)\".*/\\1/p')\n\
             outdir=\"$LINSYNC_PLUGIN_TEMP_DIR\"\n\
             i=0; pages=\"\"\n\
             while IFS= read -r _line; do\n\
               cp './page-fixture.png' \"$outdir/page-$i.png\"\n\
               [ -n \"$pages\" ] && pages=\"$pages,\"\n\
               pages=\"$pages\\\"$outdir/page-$i.png\\\"\"\n\
               i=$((i+1))\n\
             done < \"$source\"\n\
             printf '{\"ok\":true,\"pages\":[%s]}\\n' \"$pages\"\n"
            .to_string();
        let mut f = std::fs::File::create(&helper).unwrap();
        f.write_all(script.as_bytes()).unwrap();
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
              "id": "test.renderer",
              "name": "Fixture Renderer",
              "version": "1.0.0",
              "license": "GPL-3.0-only",
              "entry": ["./render.sh"],
              "classes": ["pdf_renderer"],
              "mime_types": ["application/pdf"],
              "extensions": ["pdf"],
              "capabilities": [],
              "deterministic": true,
              "sandbox": { "network": false, "writes_input": false, "requires_home_access": false },
              "options_schema": []
            }"#,
        )
        .unwrap();
    }

    fn rendered_options(temp_root: &Path) -> DocumentCompareOptions {
        DocumentCompareOptions {
            mode: DocumentCompareMode::Rendered,
            temp_root: Some(temp_root.to_path_buf()),
            ..DocumentCompareOptions::default()
        }
    }

    #[test]
    fn rendered_compare_equal_pages_and_page_count_mismatch() {
        let tmp = std::env::temp_dir().join(format!("linsync-rendered-doc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let plugins_root = tmp.join("plugins");
        std::fs::create_dir_all(&plugins_root).unwrap();
        install_renderer_plugin(&plugins_root);

        // Two 2-page documents (2 lines each) → 2 identical rendered pages.
        let a = tmp.join("a.pdf");
        let b = tmp.join("b.pdf");
        std::fs::write(&a, "p1\np2\n").unwrap();
        std::fs::write(&b, "p1\np2\n").unwrap();
        let result =
            compare_document_files(&a, &b, &plugins_root, &rendered_options(&tmp)).unwrap();
        assert!(
            result.text_result.is_none(),
            "rendered mode has no text result"
        );
        assert_eq!(result.rendered_pages.len(), 2, "two pages compared");
        assert!(
            result
                .rendered_pages
                .iter()
                .all(|p| p.equal && !p.one_sided)
        );
        assert!(result.is_equal());

        // A 3-page right document → third page is one-sided, overall different.
        let c = tmp.join("c.pdf");
        std::fs::write(&c, "p1\np2\np3\n").unwrap();
        let mismatch =
            compare_document_files(&a, &c, &plugins_root, &rendered_options(&tmp)).unwrap();
        assert_eq!(mismatch.rendered_pages.len(), 3);
        assert!(mismatch.rendered_pages[2].one_sided);
        assert!(!mismatch.is_equal());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rendered_page_range_compares_only_selected_pages() {
        let tmp =
            std::env::temp_dir().join(format!("linsync-rendered-range-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let plugins_root = tmp.join("plugins");
        std::fs::create_dir_all(&plugins_root).unwrap();
        install_renderer_plugin(&plugins_root);

        // Two 5-page documents (5 lines each → 5 rendered pages per side).
        let a = tmp.join("a.pdf");
        let b = tmp.join("b.pdf");
        std::fs::write(&a, "1\n2\n3\n4\n5\n").unwrap();
        std::fs::write(&b, "1\n2\n3\n4\n5\n").unwrap();

        // Compare only pages 2–4 (1-based inclusive).
        let mut opts = rendered_options(&tmp);
        opts.page_range = Some((2, 4));
        let result = compare_document_files(&a, &b, &plugins_root, &opts).unwrap();
        assert_eq!(
            result.rendered_pages.len(),
            3,
            "only pages 2,3,4 are compared"
        );
        let pages: Vec<usize> = result.rendered_pages.iter().map(|p| p.page).collect();
        assert_eq!(
            pages,
            vec![1, 2, 3],
            "the result reports the selected pages by 0-based absolute index"
        );

        // A range past the end is clamped to the rendered page count.
        opts.page_range = Some((4, 99));
        let clamped = compare_document_files(&a, &b, &plugins_root, &opts).unwrap();
        let clamped_pages: Vec<usize> = clamped.rendered_pages.iter().map(|p| p.page).collect();
        assert_eq!(
            clamped_pages,
            vec![3, 4],
            "clamped to pages 4,5 (indices 3,4)"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rendered_mode_without_renderer_plugin_errors() {
        let tmp =
            std::env::temp_dir().join(format!("linsync-rendered-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let plugins_root = tmp.join("plugins");
        std::fs::create_dir_all(&plugins_root).unwrap();
        let a = tmp.join("a.pdf");
        std::fs::write(&a, "p1\n").unwrap();
        let err =
            compare_document_files(&a, &a, &plugins_root, &rendered_options(&tmp)).unwrap_err();
        assert!(matches!(err, DocumentCompareError::NoSuitablePlugin { .. }));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rendered_page_dirs_are_removed_on_drop() {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        let right = tmp.path().join("right");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        {
            let _guard = RenderedPageDirs {
                left: Some(left.clone()),
                right: Some(right.clone()),
            };
            assert!(left.exists() && right.exists());
        }
        assert!(
            !left.exists(),
            "left rendered-page temp dir should be removed on drop"
        );
        assert!(
            !right.exists(),
            "right rendered-page temp dir should be removed on drop"
        );
    }

    #[test]
    fn rendered_page_dirs_release_prevents_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        let right = tmp.path().join("right");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        {
            let mut guard = RenderedPageDirs {
                left: Some(left.clone()),
                right: Some(right.clone()),
            };
            guard.release();
        }
        assert!(
            left.exists() && right.exists(),
            "released dirs must survive drop"
        );
    }
}
