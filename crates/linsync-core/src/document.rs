// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

//! Document and OCR compare paths (feature-gated: `document-compare`).
//!
//! All helpers shell out — no Poppler/Tesseract/LibreOffice crate is linked.
//! All helpers run inside the Phase 6 sandbox (see `linsync_sandbox::run_sandboxed`).

use std::path::Path;
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
            temp_root: None,
        }
    }
}

/// Result produced by a document compare.
#[derive(Debug, Clone)]
pub struct DocumentCompareResult {
    /// Displayable name of the helper that extracted the left side (e.g. `"pdftotext"`).
    pub left_extractor: String,
    /// Displayable name of the helper that extracted the right side (e.g. `"pdftotext"`).
    pub right_extractor: String,
    /// Underlying text compare result (only populated when `mode` is `Text` or `OcrText`).
    pub text_result: Option<crate::text::TextCompareResult>,
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
        DocumentCompareMode::Rendered => None, // Phase 7 integration; not implemented in v1
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

/// Extract text from one side using the discovered plugin.
fn extract_text_with_plugin(
    plugin: &DiscoveredPlugin,
    path: &Path,
    role: &str,
    timeout_secs: u64,
    language: &str,
    temp_root: Option<&Path>,
) -> Result<String, DocumentCompareError> {
    let opts = PluginExecutionOptions {
        timeout: Duration::from_secs(timeout_secs),
        temp_root: temp_root.map(Path::to_path_buf),
        ..PluginExecutionOptions::default()
    };
    let operation_options = PluginTextOperationOptions {
        language: Some(language.to_owned()),
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
    Ok(text_result.text)
}

/// Compare two document files using the appropriate helper plugin.
///
/// `plugins_root` is the directory containing installed plugin sub-directories
/// (e.g. `<workspace>/packaging/plugins` in development, or
/// `/usr/share/linsync/plugins` in a system install).
///
/// When `mode` is `Rendered`, this function currently returns
/// `DocumentCompareError::NoSuitablePlugin` — Phase 7 integration is not
/// implemented in v1.
pub fn compare_document_files(
    left: &Path,
    right: &Path,
    plugins_root: &Path,
    options: &DocumentCompareOptions,
) -> Result<DocumentCompareResult, DocumentCompareError> {
    if matches!(options.mode, DocumentCompareMode::Rendered) {
        return Err(DocumentCompareError::NoSuitablePlugin {
            path: left.display().to_string(),
            mime_hint: "rendered-mode not implemented in v1".to_owned(),
        });
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
    let left_text = extract_text_with_plugin(
        &left_plugin,
        left,
        "left",
        options.timeout_secs,
        &options.ocr_language,
        temp_root,
    )?;
    let right_text = extract_text_with_plugin(
        &right_plugin,
        right,
        "right",
        options.timeout_secs,
        &options.ocr_language,
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
    })
}
