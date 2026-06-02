// SPDX-FileCopyrightText: 2026 VisorCraft LLC
// SPDX-License-Identifier: GPL-3.0-only

use linsync_core::{Settings as CoreSettings, ThemePreference};

const RESPONSE_SCHEMA_VERSION: u32 = 1;

// ── Image compare bridge ──────────────────────────────────────────────────────

/// Percent-decode a URL-encoded query string value.
fn percent_decode_value(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = hex_nibble(bytes[index + 1]);
                let low = hex_nibble(bytes[index + 2]);
                if let (Some(h), Some(l)) = (high, low) {
                    decoded.push((h << 4) | l);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn json_with_schema(mut value: serde_json::Value) -> String {
    if let Some(object) = value.as_object_mut() {
        object
            .entry("schema_version".to_owned())
            .or_insert_with(|| serde_json::json!(RESPONSE_SCHEMA_VERSION));
    }
    serde_json::to_string(&value).unwrap_or_else(|_| {
        format!(r#"{{"schema_version":{RESPONSE_SCHEMA_VERSION},"error":"serialization error"}}"#)
    })
}

fn error_json(message: impl Into<String>) -> String {
    json_with_schema(serde_json::json!({ "error": message.into() }))
}

fn image_mode_label(mode: &linsync_core::ImageCompareMode) -> &'static str {
    match mode {
        linsync_core::ImageCompareMode::Exact => "exact",
        linsync_core::ImageCompareMode::Tolerance(_) => "tolerance",
        linsync_core::ImageCompareMode::Perceptual => "perceptual",
    }
}

fn document_mode_label(mode: linsync_core::DocumentCompareMode) -> &'static str {
    match mode {
        linsync_core::DocumentCompareMode::Text => "text",
        linsync_core::DocumentCompareMode::OcrText => "ocr_text",
        linsync_core::DocumentCompareMode::Rendered => "rendered",
    }
}

fn image_query_param(query: &str, key: &str) -> Option<String> {
    for part in query.split('&') {
        if let Some((_k, v)) = part.split_once('=').filter(|(k, _)| *k == key) {
            return Some(percent_decode_value(v));
        }
    }
    None
}

/// Handle `/compare/image?left=…&right=…&mode=…&tolerance=…&delta_e=…&overlay=…`
/// Returns a JSON string and the optional compare result. Uses default options
/// when no profile is in scope — see [`image_compare_bridge_response_with_profile`]
/// for the profile-aware variant.
pub fn image_compare_bridge_response(
    query: &str,
) -> (String, Option<linsync_core::ImageCompareResult>) {
    image_compare_bridge_response_with_profile(query, &linsync_core::ImageCompareOptions::default())
}

/// Profile-aware variant of [`image_compare_bridge_response`].
///
/// Query parameters override the profile's options field-by-field:
/// `mode` / `tolerance` / `delta_e` / `overlay`. Fields not present in the
/// query inherit from `profile_options`.
///
/// Returns `(json_string, Some(result))` on success or `(error_json, None)` on failure.
pub fn image_compare_bridge_response_with_profile(
    query: &str,
    profile_options: &linsync_core::ImageCompareOptions,
) -> (String, Option<linsync_core::ImageCompareResult>) {
    use linsync_core::{ImageCompareMode, ImageCompareOptions, compare_images};

    let left = match image_query_param(query, "left") {
        Some(v) => std::path::PathBuf::from(v),
        None => return (error_json("missing 'left' parameter"), None),
    };
    let right = match image_query_param(query, "right") {
        Some(v) => std::path::PathBuf::from(v),
        None => return (error_json("missing 'right' parameter"), None),
    };
    let mode_str = image_query_param(query, "mode");
    let want_overlay = image_query_param(query, "overlay")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let mode = match mode_str.as_deref() {
        Some("tolerance") => ImageCompareMode::Tolerance(
            image_query_param(query, "tolerance")
                .and_then(|v| v.parse().ok())
                .unwrap_or(profile_options.tolerance),
        ),
        Some("perceptual") => ImageCompareMode::Perceptual,
        Some("exact") => ImageCompareMode::Exact,
        Some(_) | None => profile_options.mode.clone(),
    };
    let tolerance = image_query_param(query, "tolerance")
        .and_then(|v| v.parse().ok())
        .unwrap_or(profile_options.tolerance);
    let delta_e = image_query_param(query, "delta_e")
        .and_then(|v| v.parse().ok())
        .unwrap_or(profile_options.delta_e_threshold);

    let opts = ImageCompareOptions {
        mode,
        tolerance,
        delta_e_threshold: delta_e,
        ..profile_options.clone()
    };

    let result = match compare_images(&left, &right, &opts) {
        Ok(r) => r,
        Err(e) => {
            return (error_json(e.to_string()), None);
        }
    };

    let overlay_path_uri = if want_overlay {
        build_overlay_png(&result, &left, &right, &opts)
    } else {
        None
    };

    let mut json = serde_json::json!({
        "equal": result.equal,
        "left_dims": result.left_dims,
        "right_dims": result.right_dims,
        "total_pixels": result.total_pixels,
        "differing_pixels": result.differing_pixels,
        "diff_ratio": result.diff_ratio,
        "mode": image_mode_label(&opts.mode),
        "diff_bbox": result.diff_bbox,
        "padded": result.padded,
        "diff_regions": result.diff_regions,
    });

    if let Some(uri) = overlay_path_uri {
        json["overlay_path"] = serde_json::Value::String(uri);
    }

    (json_with_schema(json), Some(result))
}

/// Return the image decoder formats compiled into the current build.
pub fn image_formats_bridge_response() -> String {
    let formats = linsync_core::supported_image_formats();
    let extension_globs: Vec<String> = formats
        .iter()
        .flat_map(|format| {
            format
                .extensions
                .iter()
                .map(|extension| format!("*.{extension}"))
        })
        .collect();

    json_with_schema(serde_json::json!({
        "formats": formats,
        "extension_globs": extension_globs,
    }))
}

/// Generate an RGBA8 overlay PNG highlighting differing pixels and return a `file://` URI.
fn build_overlay_png(
    result: &linsync_core::ImageCompareResult,
    left: &std::path::Path,
    right: &std::path::Path,
    options: &linsync_core::ImageCompareOptions,
) -> Option<String> {
    use ::image::ImageBuffer;
    let (width, height) = (
        result.left_dims.0.max(result.right_dims.0),
        result.left_dims.1.max(result.right_dims.1),
    );
    let img: image::RgbaImage = if result.overlay.is_empty() {
        let overlay_result = linsync_core::generate_overlay(left, right, options).ok()?;
        ImageBuffer::from_raw(width, height, overlay_result.overlay)?
    } else {
        ImageBuffer::from_raw(width, height, result.overlay.clone())?
    };

    let tmp_path = overlay_output_path()?;
    img.save(&tmp_path).ok()?;
    // The PNG can contain rendered file contents, so deny group/other access.
    restrict_overlay_file(&tmp_path);
    Some(format!("file://{}", tmp_path.display()))
}

/// Build an unpredictable, per-process overlay output path under a private,
/// owner-only directory in the temp dir.
///
/// The previous scheme used only `SystemTime` subsec-nanos as the filename
/// token directly in the shared temp dir, which another local user could
/// predict (and pre-create or read). The token now combines full epoch nanos,
/// the process id, and a process-wide monotonic counter, and the file lives in
/// a `0700` per-process subdirectory so the bytes are not world-readable.
fn overlay_output_path() -> Option<std::path::PathBuf> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let dir = overlay_dir()?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    Some(dir.join(format!(
        "linsync-overlay-{}-{nanos}-{seq}.png",
        std::process::id()
    )))
}

/// Resolve (creating if needed) a per-process, owner-only directory for overlay
/// PNGs. Returns `None` if it cannot be confirmed as a directory owned by us.
fn overlay_dir() -> Option<std::path::PathBuf> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let dir = std::env::temp_dir().join(format!("linsync-overlays-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    // Confirm a real directory owned by us before trusting it.
    let meta = std::fs::symlink_metadata(&dir).ok()?;
    let euid = unsafe { libc::geteuid() };
    if !meta.is_dir() || meta.uid() != euid {
        return None;
    }
    // Lock to owner-only and reject any residual group/other access.
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    if std::fs::symlink_metadata(&dir).ok()?.mode() & 0o077 != 0 {
        return None;
    }
    Some(dir)
}

/// Tighten an overlay PNG to owner-only (`0600`) after `image` has written it.
fn restrict_overlay_file(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

// ── Document compare bridge ───────────────────────────────────────────────────

/// Handle `/compare/document?left=…&right=…&mode=…&ocr_language=…`
/// Returns a JSON string. Uses default options when no profile is in scope —
/// see [`document_compare_bridge_response_with_profile`] for the
/// profile-aware variant.
pub fn document_compare_bridge_response(query: &str) -> String {
    document_compare_bridge_response_with_profile(
        query,
        &linsync_core::DocumentCompareOptions::default(),
    )
}

/// Resolve the effective [`DocumentCompareOptions`](linsync_core::DocumentCompareOptions)
/// for a single `/compare/document` request.
///
/// The resolved profile supplies the defaults; `?mode` (`text` / `ocr_text`)
/// and `?ocr_language` override `mode` / `ocr_language` field-by-field. Any
/// unrecognised `?mode` value (e.g. `rendered`) and all other fields inherit
/// from `profile_options`.
pub fn resolve_document_options(
    query: &str,
    profile_options: &linsync_core::DocumentCompareOptions,
) -> linsync_core::DocumentCompareOptions {
    use linsync_core::DocumentCompareMode;
    let mode = match image_query_param(query, "mode").as_deref() {
        Some("ocr_text") => DocumentCompareMode::OcrText,
        Some("text") => DocumentCompareMode::Text,
        Some(_) | None => profile_options.mode,
    };
    let ocr_language = image_query_param(query, "ocr_language")
        .unwrap_or_else(|| profile_options.ocr_language.clone());
    linsync_core::DocumentCompareOptions {
        mode,
        ocr_language,
        ..profile_options.clone()
    }
}

/// Profile-aware variant of [`document_compare_bridge_response`].
///
/// Query parameters override the profile's options field-by-field via
/// [`resolve_document_options`]: `mode` and `ocr_language`. Fields not present
/// in the query inherit from `profile_options`.
pub fn document_compare_bridge_response_with_profile(
    query: &str,
    profile_options: &linsync_core::DocumentCompareOptions,
) -> String {
    use linsync_core::document::{DocumentCompareError, compare_document_files};

    let left = match image_query_param(query, "left") {
        Some(v) => std::path::PathBuf::from(v),
        None => return error_json("missing 'left' parameter"),
    };
    let right = match image_query_param(query, "right") {
        Some(v) => std::path::PathBuf::from(v),
        None => return error_json("missing 'right' parameter"),
    };
    let opts = resolve_document_options(query, profile_options);

    // Locate the plugins root (same logic as the CLI helper).
    let plugins_root = detect_document_plugins_root();

    let result = match compare_document_files(&left, &right, &plugins_root, &opts) {
        Ok(r) => r,
        Err(DocumentCompareError::NoSuitablePlugin { path, mime_hint }) => {
            return error_json(format!(
                "no document plugin for '{path}' (MIME: {mime_hint})"
            ));
        }
        Err(e) => {
            return error_json(e.to_string());
        }
    };

    let text_result = result.text_result.as_ref();
    let is_equal = text_result.map(|t| t.is_equal()).unwrap_or(false);
    let diff_count = text_result.map(|t| t.difference_count()).unwrap_or(0);

    let left_text = text_result.map(|t| {
        t.left_document
            .lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    });
    let right_text = text_result.map(|t| {
        t.right_document
            .lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    });

    let mut json = serde_json::json!({
        "equal": is_equal,
        "left_extractor": result.left_extractor,
        "right_extractor": result.right_extractor,
        "differing_lines": diff_count,
        "mode": document_mode_label(opts.mode),
    });

    if let Some(ref lt) = left_text {
        json["left_text"] = serde_json::Value::String(lt.clone());
    }
    if let Some(ref rt) = right_text {
        json["right_text"] = serde_json::Value::String(rt.clone());
    }

    json_with_schema(json)
}

/// Return the directory where LinSync plugins are installed.
///
/// In a development build the binary is somewhere under `<workspace>/target/`,
/// so we walk up from `current_exe()` until we find `packaging/plugins`.
fn detect_document_plugins_root() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let mut candidate = exe.parent().map(|p| p.to_path_buf());
        while let Some(dir) = candidate {
            let plugins = dir.join("packaging/plugins");
            if plugins.is_dir() {
                return plugins;
            }
            candidate = dir.parent().map(|p| p.to_path_buf());
        }
    }
    std::path::PathBuf::from("/usr/share/linsync/plugins")
}

pub fn apply_gui_setting(
    settings: &mut CoreSettings,
    key: &str,
    value: &str,
) -> Result<(), String> {
    match key {
        "themePreference" => {
            let value = value
                .parse::<u8>()
                .map_err(|_| format!("invalid theme preference: {value}"))?;
            settings.theme_preference = ThemePreference::from_grex_value(value)
                .ok_or_else(|| format!("unsupported theme preference: {value}"))?;
        }
        "fontSize" => settings.pane_font_size = parse_u8_setting(key, value, 8, 28)?,
        "fontFamily" => settings.pane_font_family = value.to_owned(),
        "tabWidth" => settings.pane_tab_width = parse_u8_setting(key, value, 1, 12)?,
        "showLineNumbers" => settings.show_line_numbers = parse_bool_setting(key, value)?,
        "showWhitespace" => settings.show_whitespace = parse_bool_setting(key, value)?,
        "wordWrap" => settings.word_wrap = parse_bool_setting(key, value)?,
        "ignoreCase" => settings.ignore_case = parse_bool_setting(key, value)?,
        "ignoreWhitespace" => settings.ignore_whitespace = parse_bool_setting(key, value)?,
        "ignoreBlankLines" => settings.ignore_blank_lines = parse_bool_setting(key, value)?,
        "ignoreEol" => settings.ignore_eol = parse_bool_setting(key, value)?,
        "detectMoves" => settings.detect_moves = parse_bool_setting(key, value)?,
        "eolNormalization" => settings.eol_normalization = value.to_owned(),
        "defaultCompareMode" => settings.default_compare_mode = value.to_owned(),
        "openLastSession" => settings.open_last_session = parse_bool_setting(key, value)?,
        "confirmOnClose" => settings.confirm_on_close = parse_bool_setting(key, value)?,
        "persistRecentPaths" => settings.persist_recent_paths = parse_bool_setting(key, value)?,
        "reduceMotion" => settings.reduce_motion = parse_bool_setting(key, value)?,
        "maxRecentPaths" => {
            settings.recent_limit = value
                .parse::<usize>()
                .map_err(|_| format!("invalid {key}: {value}"))?
                .clamp(1, 200);
        }
        _ => return Err(format!("unsupported setting key: {key}")),
    }
    Ok(())
}

pub(crate) fn parse_u8_setting(key: &str, value: &str, min: u8, max: u8) -> Result<u8, String> {
    value
        .parse::<u8>()
        .map(|value| value.clamp(min, max))
        .map_err(|_| format!("invalid {key}: {value}"))
}

pub fn parse_bool_setting(key: &str, value: &str) -> Result<bool, String> {
    match value {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(format!("invalid {key}: {value}")),
    }
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::apply_gui_setting;
    use linsync_core::{
        AppPaths, FileFilter, FilterStore, NamedFilters, Settings as CoreSettings, SettingsStore,
    };
    use std::env;
    use std::process;

    pub fn apply_gui_setting_test(key: &str, value: &str) -> Result<(), String> {
        let mut settings = CoreSettings::default();
        apply_gui_setting(&mut settings, key, value)
    }

    /// Save a single GUI setting through the real `SettingsStore::save` path, then
    /// reload from disk and return the resulting `CoreSettings`.
    ///
    /// This mirrors the full `/settings/set` HTTP handler path — it validates,
    /// persists, and re-loads from the on-disk JSON file.
    pub fn save_and_load_setting(
        paths: &AppPaths,
        key: &str,
        value: &str,
    ) -> Result<CoreSettings, String> {
        let store = SettingsStore::new(paths.settings_file());
        let mut settings = store
            .load_or_default()
            .map_err(|e| format!("load failed: {e}"))?;
        apply_gui_setting(&mut settings, key, value)?;
        store
            .save(&settings)
            .map_err(|e| format!("save failed: {e}"))?;
        store
            .load_or_default()
            .map_err(|e| format!("reload failed: {e}"))
    }

    /// Create an isolated `AppPaths` in a temp directory — each test should use a
    /// unique `name` so tests do not share state.
    pub fn temp_app_paths(name: &str) -> AppPaths {
        let root = env::temp_dir().join(format!("linsync-test-support-{name}-{}", process::id()));
        AppPaths::from_base_dirs(
            root.join("config"),
            root.join("data"),
            root.join("cache"),
            root.join("state"),
        )
    }

    // -------------------------------------------------------------------------
    // Walk-option helpers
    // -------------------------------------------------------------------------

    /// Return the persisted walk options from the settings store.
    pub fn load_walk_options(paths: &AppPaths) -> CoreSettings {
        SettingsStore::new(paths.settings_file())
            .load_or_default()
            .expect("walk options should load")
    }

    /// Persist a single walk option by key / value string (mirrors `/walk/set`).
    ///
    /// Recognised keys: `respect_gitignore`, `follow_symlinks`, `max_walk_depth`,
    /// `includes`, `excludes`.
    pub fn set_walk_option(
        paths: &AppPaths,
        key: &str,
        value: &str,
    ) -> Result<CoreSettings, String> {
        let store = SettingsStore::new(paths.settings_file());
        let mut settings = store
            .load_or_default()
            .map_err(|e| format!("load failed: {e}"))?;
        match key {
            "respect_gitignore" => {
                settings.respect_gitignore = super::parse_bool_setting(key, value)?;
            }
            "follow_symlinks" => {
                settings.follow_symlinks = super::parse_bool_setting(key, value)?;
            }
            "max_walk_depth" => {
                settings.max_walk_depth = value
                    .parse::<u32>()
                    .map(|v| v.min(256))
                    .map_err(|_| format!("invalid max_walk_depth: {value}"))?;
            }
            "includes" => {
                settings.session_includes = split_csv(value);
            }
            "excludes" => {
                settings.session_excludes = split_csv(value);
            }
            other => return Err(format!("unknown walk option key: {other}")),
        }
        store
            .save(&settings)
            .map_err(|e| format!("save failed: {e}"))?;
        Ok(settings)
    }

    fn split_csv(value: &str) -> Vec<String> {
        value
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect()
    }

    // -------------------------------------------------------------------------
    // Filter helpers
    // -------------------------------------------------------------------------

    /// List all named filters from the store.
    pub fn list_filters(paths: &AppPaths) -> NamedFilters {
        FilterStore::new(paths.filters_file())
            .load_or_default()
            .expect("filter list should load")
    }

    /// Parse and upsert a named filter (mirrors `/filters/save`).
    pub fn save_filter(paths: &AppPaths, body: &str) -> Result<NamedFilters, String> {
        let parsed = FileFilter::parse(body).map_err(|e| e.to_string())?;
        if parsed.name.is_none() {
            return Err("filter body must include a 'name:' header".to_owned());
        }
        FilterStore::new(paths.filters_file())
            .upsert(parsed)
            .map_err(|e| e.to_string())
    }

    /// Delete a named filter by name (mirrors `/filters/delete`).
    pub fn delete_filter(paths: &AppPaths, name: &str) -> Result<NamedFilters, String> {
        let store = FilterStore::new(paths.filters_file());
        let mut filters = store
            .load_or_default()
            .map_err(|e| format!("load failed: {e}"))?;
        filters.filters.retain(|f| f.name.as_deref() != Some(name));
        store
            .save(&filters)
            .map_err(|e| format!("save failed: {e}"))?;
        Ok(filters)
    }

    /// Validate a filter expression (mirrors `/filters/validate`).
    ///
    /// Returns `Ok(())` if the expression parses cleanly; `Err(description)` otherwise.
    pub fn validate_filter(body: &str) -> Result<(), String> {
        FileFilter::parse(body)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Validate a filter expression and return the raw `FilterParseError` so
    /// callers can inspect the error kind.
    pub fn validate_filter_err(body: &str) -> Result<FileFilter, linsync_core::FilterParseError> {
        FileFilter::parse(body)
    }

    /// Migrate legacy `.flt` text to LinSync filter syntax (mirrors
    /// `/filters/migrate`).
    ///
    /// Returns the [`linsync_core::MigratedFilter`] so callers can inspect
    /// both the migrated text and any warnings.
    pub fn migrate_filter(body: &str) -> linsync_core::MigratedFilter {
        linsync_core::migrate_filter_text(body)
    }

    // -------------------------------------------------------------------------
    // Plugin helpers
    // -------------------------------------------------------------------------

    /// Return the current plugin-enabled map from disk (`plugins.json`).
    pub fn load_plugin_enabled_map(paths: &AppPaths) -> std::collections::HashMap<String, bool> {
        let file = paths.plugins_enabled_file();
        let Ok(text) = std::fs::read_to_string(&file) else {
            return Default::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Persist a single plugin's enabled state.
    pub fn save_plugin_enabled(paths: &AppPaths, id: &str, enabled: bool) -> Result<(), String> {
        let file = paths.plugins_enabled_file();
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create_dir_all failed: {e}"))?;
        }
        let mut map = load_plugin_enabled_map(paths);
        map.insert(id.to_owned(), enabled);
        let text =
            serde_json::to_string_pretty(&map).map_err(|e| format!("serialize failed: {e}"))?;
        std::fs::write(&file, text).map_err(|e| format!("write failed: {e}"))
    }

    /// Write a minimal valid plugin manifest under `<plugins_dir>/<id>/linsync-plugin.json`.
    /// Returns the plugin directory path.
    pub fn write_fixture_plugin(
        plugins_dir: &std::path::Path,
        id: &str,
        name: &str,
    ) -> std::path::PathBuf {
        use linsync_core::plugin::{
            CURRENT_PLUGIN_SCHEMA_VERSION, PLUGIN_MANIFEST_FILE, PluginClass, PluginManifest,
            PluginSandbox,
        };
        let plugin_dir = plugins_dir.join(id);
        std::fs::create_dir_all(&plugin_dir).expect("plugin dir should be created");
        let manifest = PluginManifest {
            schema_version: CURRENT_PLUGIN_SCHEMA_VERSION,
            id: id.to_owned(),
            name: name.to_owned(),
            version: "1.0.0".to_owned(),
            license: "MIT".to_owned(),
            entry: vec!["run.sh".to_owned()],
            classes: vec![PluginClass::Prediffer],
            mime_types: vec![],
            extensions: vec![],
            capabilities: vec![],
            deterministic: false,
            sandbox: PluginSandbox::default(),
            streaming: false,
            options_schema: vec![],
        };
        let text = serde_json::to_string_pretty(&manifest).unwrap();
        std::fs::write(plugin_dir.join(PLUGIN_MANIFEST_FILE), text).unwrap();
        plugin_dir
    }

    // -------------------------------------------------------------------------
    // Image compare helper
    // -------------------------------------------------------------------------

    /// Call the document compare bridge handler directly (mirrors `/compare/document` endpoint).
    /// Returns the JSON response body as a `String`, or `Err` if the handler returned an error.
    pub fn document_compare_test(
        left: &str,
        right: &str,
        mode: &str,
        ocr_language: &str,
    ) -> Result<String, String> {
        let query = format!(
            "left={}&right={}&mode={}&ocr_language={}",
            urlencoding::encode(left),
            urlencoding::encode(right),
            mode,
            urlencoding::encode(ocr_language),
        );
        let body = super::document_compare_bridge_response(&query);
        if serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| value.get("error").cloned())
            .is_some()
        {
            Err(body)
        } else {
            Ok(body)
        }
    }

    /// Call the image compare bridge handler directly (mirrors `/compare/image` endpoint).
    /// Returns the JSON response body as a `String`, or `Err` if the handler returned an error.
    pub fn image_compare_test(
        left: &str,
        right: &str,
        mode: &str,
        tolerance: u8,
        delta_e: f32,
        overlay: bool,
    ) -> Result<String, String> {
        let query = format!(
            "left={}&right={}&mode={}&tolerance={}&delta_e={}&overlay={}",
            urlencoding::encode(left),
            urlencoding::encode(right),
            mode,
            tolerance,
            delta_e,
            overlay,
        );
        let body = super::image_compare_bridge_response(&query).0;
        if serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| value.get("error").cloned())
            .is_some()
        {
            Err(body)
        } else {
            Ok(body)
        }
    }

    // -------------------------------------------------------------------------
    // Plugin options helpers
    // -------------------------------------------------------------------------

    /// Return the persisted option values for a plugin (mirrors `/plugins/options/get` values).
    pub fn load_plugin_options(
        paths: &AppPaths,
        plugin_id: &str,
    ) -> serde_json::Map<std::string::String, serde_json::Value> {
        let file = paths.plugin_options_file(plugin_id);
        let Ok(text) = std::fs::read_to_string(&file) else {
            return serde_json::Map::new();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Persist a single option key for a plugin (mirrors `/plugins/options/set`).
    pub fn save_plugin_option(
        paths: &AppPaths,
        plugin_id: &str,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), std::string::String> {
        let dir = paths.plugin_options_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("create_dir_all failed: {e}"))?;
        let file = paths.plugin_options_file(plugin_id);
        let mut map = load_plugin_options(paths, plugin_id);
        map.insert(key.to_owned(), value);
        let text =
            serde_json::to_string_pretty(&map).map_err(|e| format!("serialize failed: {e}"))?;
        std::fs::write(&file, text).map_err(|e| format!("write failed: {e}"))
    }
}

// ── Webpage compare bridge ────────────────────────────────────────────────────

/// Handle `/compare/webpage?left=…&right=…&mode=html|text|tree`
/// Returns a JSON string: `{"summary":"…"}` on success or `{"error":"…"}` on error.
/// Uses default options. See [`webpage_compare_bridge_response_with_profile`]
/// for the profile-aware variant.
pub fn webpage_compare_bridge_response(query: &str, paths: &linsync_core::AppPaths) -> String {
    webpage_compare_bridge_response_with_profile(
        query,
        paths,
        &linsync_core::WebpageCompareOptions::default(),
    )
}

/// Resolve the effective [`WebpageCompareOptions`](linsync_core::WebpageCompareOptions)
/// for a single `/compare/webpage` request.
///
/// The resolved profile supplies the defaults for `depth` / `timeout` /
/// `max_requests` / `user_agent`; matching query parameters override them
/// field-by-field, mirroring the CLI flags (`--depth`, `--timeout`,
/// `--max-requests`) and the image/document routes. `?depth` is clamped to
/// `1..=3` exactly like `linsync-cli webpage --depth`.
///
/// `confirmed_by_user` is always forced to `true`: the consent gate is owned by
/// the bridge dispatcher (which only reaches this point once the QML dialog has
/// confirmed) and must never be sourced from persisted profile JSON, so a
/// profile such as `webpage-source-safe` cannot bypass the dialog.
pub fn resolve_webpage_options(
    query: &str,
    profile_options: &linsync_core::WebpageCompareOptions,
) -> linsync_core::WebpageCompareOptions {
    let resource_tree_depth = image_query_param(query, "depth")
        .and_then(|v| v.parse::<u8>().ok())
        .map(|d| d.clamp(1, 3))
        .unwrap_or(profile_options.resource_tree_depth);
    let timeout_secs = image_query_param(query, "timeout")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(profile_options.timeout_secs);
    let max_requests = image_query_param(query, "max_requests")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(profile_options.max_requests);
    // A present-but-empty `?user_agent=` is treated as "unset" and inherits the
    // profile value rather than forcing an empty UA header.
    let user_agent = match image_query_param(query, "user_agent") {
        Some(ua) if !ua.is_empty() => Some(ua),
        _ => profile_options.user_agent.clone(),
    };
    linsync_core::WebpageCompareOptions {
        resource_tree_depth,
        timeout_secs,
        max_requests,
        user_agent,
        confirmed_by_user: true,
    }
}

/// Profile-aware variant of [`webpage_compare_bridge_response`].
///
/// `profile_options` supplies the depth / timeout / max-requests / user-agent
/// defaults from the active profile; per-request query params override them via
/// [`resolve_webpage_options`]. The caller is responsible for the consent gate —
/// the profile's `confirmed_by_user` is intentionally ignored (always forced to
/// `true` here) so that a persisted profile cannot bypass the fresh user dialog.
/// Serialize a webpage resource-tree (`FolderCompareResult`) into the compact
/// `{path, state, leftSize, rightSize}` entry list the GUI renders, capping the
/// count to keep the bridge payload bounded. Returns `(entries, truncated)`.
fn webpage_tree_entries_json(
    cmp: &linsync_core::FolderCompareResult,
) -> (Vec<serde_json::Value>, bool) {
    const MAX_ENTRIES: usize = 4000;
    let truncated = cmp.entries.len() > MAX_ENTRIES;
    let entries = cmp
        .entries
        .iter()
        .take(MAX_ENTRIES)
        .map(|entry| {
            serde_json::json!({
                "path": entry.relative_path.display().to_string(),
                "state": serde_json::to_value(entry.state).unwrap_or(serde_json::Value::Null),
                "leftSize": entry.left_size,
                "rightSize": entry.right_size,
            })
        })
        .collect();
    (entries, truncated)
}

pub fn webpage_compare_bridge_response_with_profile(
    query: &str,
    paths: &linsync_core::AppPaths,
    profile_options: &linsync_core::WebpageCompareOptions,
) -> String {
    let left = match image_query_param(query, "left") {
        Some(v) => v,
        None => return error_json("missing 'left' parameter"),
    };
    let right = match image_query_param(query, "right") {
        Some(v) => v,
        None => return error_json("missing 'right' parameter"),
    };
    let mode = image_query_param(query, "mode").unwrap_or_else(|| "html".to_owned());
    let cache_dir = &paths.cache_dir;

    let options = resolve_webpage_options(query, profile_options);

    let result = match mode.as_str() {
        "html" => linsync_core::compare_webpage_html_source(&left, &right, &options, cache_dir),
        "text" => linsync_core::compare_webpage_extracted_text(&left, &right, &options, cache_dir),
        "tree" => linsync_core::compare_webpage_resource_tree(&left, &right, &options, cache_dir),
        other => {
            return error_json(format!("unsupported mode: {other}"));
        }
    };

    match result {
        Ok(linsync_core::WebpageCompareResult::Text(cmp)) => {
            let equal = cmp.is_equal();
            let summary = if equal {
                "identical".to_owned()
            } else {
                format!("different ({} diff blocks)", cmp.blocks.len())
            };
            // Emit the aligned diff rows so the GUI can render a real
            // side-by-side diff instead of just a summary line. Cap the row
            // count to keep the bridge payload bounded for huge pages.
            const MAX_ROWS: usize = 4000;
            let truncated = cmp.lines.len() > MAX_ROWS;
            let rows: Vec<serde_json::Value> = cmp
                .lines
                .iter()
                .take(MAX_ROWS)
                .map(|line| {
                    let state = match line.kind {
                        linsync_core::DiffLineKind::Equal => "equal",
                        linsync_core::DiffLineKind::Changed => "changed",
                        linsync_core::DiffLineKind::LeftOnly => "left_only",
                        linsync_core::DiffLineKind::RightOnly => "right_only",
                    };
                    serde_json::json!({
                        "s": state,
                        "ln": line.left_line,
                        "rn": line.right_line,
                        "l": line.left.clone().unwrap_or_default(),
                        "r": line.right.clone().unwrap_or_default(),
                    })
                })
                .collect();
            json_with_schema(serde_json::json!({
                "summary": summary,
                "equal": equal,
                "truncated": truncated,
                "rows": rows,
            }))
        }
        Ok(linsync_core::WebpageCompareResult::Folder(cmp)) => {
            let equal = cmp.is_equal();
            let summary = if equal {
                "identical".to_owned()
            } else {
                format!(
                    "different (left_only={} right_only={} different={})",
                    cmp.summary.left_only_count,
                    cmp.summary.right_only_count,
                    cmp.summary.different_count
                )
            };
            // Emit the resource entries so the GUI can render a sortable /
            // filterable tree instead of a summary line.
            let (entries, truncated) = webpage_tree_entries_json(&cmp);
            json_with_schema(serde_json::json!({
                "summary": summary,
                "equal": equal,
                "truncated": truncated,
                "entries": entries,
            }))
        }
        #[cfg(feature = "web-engine")]
        Ok(linsync_core::WebpageCompareResult::Rendered(r)) => {
            let equal = r.is_equal();
            let summary = match (&r.image, equal) {
                (Some(img), false) => {
                    format!("different ({:.2}% of pixels)", img.diff_ratio * 100.0)
                }
                (Some(_), true) => "identical (rendered pixels match)".to_owned(),
                (None, true) => "identical (HTML-source fallback)".to_owned(),
                (None, false) => "different (HTML-source fallback)".to_owned(),
            };
            let mut body = serde_json::json!({ "summary": summary, "equal": equal });
            if let Some(img) = &r.image {
                body["image"] = serde_json::json!({
                    "equal": img.equal,
                    "diff_ratio": img.diff_ratio,
                    "differing_pixels": img.differing_pixels,
                    "left_dims": img.left_dims,
                    "right_dims": img.right_dims,
                });
            }
            json_with_schema(body)
        }
        #[cfg(feature = "web-engine")]
        Ok(linsync_core::WebpageCompareResult::Screenshot(img)) => {
            let summary = if img.equal {
                "identical (screenshots match)".to_owned()
            } else {
                format!("different ({:.2}% of pixels)", img.diff_ratio * 100.0)
            };
            json_with_schema(serde_json::json!({
                "summary": summary,
                "equal": img.equal,
                "diff_ratio": img.diff_ratio,
                "differing_pixels": img.differing_pixels,
                "total_pixels": img.total_pixels,
                "left_dims": img.left_dims,
                "right_dims": img.right_dims,
            }))
        }
        Err(e) => error_json(e.to_string()),
    }
}

/// Handle `/compare/webpage/clear-cache` — remove webcompare cache.
pub fn webpage_clear_cache_bridge_response(paths: &linsync_core::AppPaths) -> String {
    match linsync_core::clear_webcompare_cache(&paths.cache_dir) {
        Ok(()) => json_with_schema(serde_json::json!({"ok": true})),
        Err(e) => error_json(e.to_string()),
    }
}

#[cfg(test)]
mod webpage_tree_tests {
    use super::webpage_tree_entries_json;
    use linsync_core::{VirtualNode, compare_virtual_trees};

    #[test]
    fn tree_entries_carry_path_and_state() {
        // A network-free resource tree: build two VirtualNode trees that differ,
        // compare them, and confirm the serialized entries carry path + state.
        let vn = |path: &str, sha: Option<&str>| VirtualNode {
            path: path.to_string(),
            kind: "file".to_string(),
            size: None,
            sha256: sha.map(str::to_string),
        };
        let left = vec![
            vn("index.html", Some("aaa")),
            vn("only-left.css", Some("c")),
        ];
        let right = vec![
            vn("index.html", Some("bbb")),
            vn("only-right.js", Some("d")),
        ];
        let result = compare_virtual_trees(&left, &right);

        let (entries, truncated) = webpage_tree_entries_json(&result);
        assert!(!truncated);
        assert!(!entries.is_empty(), "tree entries should be emitted");
        // Each entry exposes a path string and a recognized state.
        for e in &entries {
            assert!(e["path"].is_string(), "entry needs a path: {e}");
            let state = e["state"].as_str().unwrap_or("");
            assert!(
                matches!(state, "Identical" | "Different" | "LeftOnly" | "RightOnly"),
                "entry has a recognized state, got {state:?}: {e}"
            );
        }
        // The changed index.html is reported as Different.
        let index = entries
            .iter()
            .find(|e| e["path"] == "index.html")
            .expect("index.html present");
        assert_eq!(index["state"], "Different");
    }
}
