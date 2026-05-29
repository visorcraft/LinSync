use std::path::{Path, PathBuf};

/// Which sub-mode to use for a webpage comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebpageCompareMode {
    /// Fetch raw HTML from both URLs; compare as text.  No browser required.
    HtmlSource,
    /// Fetch HTML, strip tags; compare extracted plain text.  No browser required.
    ExtractedText,
    /// Enumerate linked resources from both URLs; compare as virtual folder.
    /// `depth` is capped at `WebpageCompareOptions::resource_tree_depth` (max 3).
    ResourceTree,
    /// Load both URLs in an isolated Qt WebEngine profile; compare rendered DOM.
    /// Only available when compiled with `--features web-engine`.
    #[cfg(feature = "web-engine")]
    Rendered,
    /// Load both URLs, capture full-page PNGs, forward to image compare (Phase 7).
    /// Only available when compiled with `--features web-engine`.
    #[cfg(feature = "web-engine")]
    Screenshot,
}

/// Options shared across all sub-modes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WebpageCompareOptions {
    /// Maximum link-traversal depth for ResourceTree mode.  Default 1, max 3.
    pub resource_tree_depth: u8,
    /// Per-request network timeout in seconds.  Default 30.
    pub timeout_secs: u32,
    /// Optional custom User-Agent string.  `None` uses the plugin's default.
    pub user_agent: Option<String>,
    /// Maximum number of HTTP requests per resource-tree session.
    /// Prevents unbounded fan-out.  Default 50.
    pub max_requests: u32,
    /// The user has explicitly acknowledged the network fetch dialog.
    /// If `false`, all compare functions return `Err(WebpageCompareError::ConfirmationRequired)`.
    ///
    /// Callers MUST set this from a fresh user interaction immediately
    /// before invoking the compare — never thread the value through
    /// from persisted profile JSON or another long-lived store. The
    /// built-in `webpage-source-safe` profile deliberately ships with
    /// `confirmed_by_user: false` so that consumers cannot accidentally
    /// bypass the dialog by selecting a profile.
    pub confirmed_by_user: bool,
}

impl Default for WebpageCompareOptions {
    fn default() -> Self {
        Self {
            resource_tree_depth: 1,
            timeout_secs: 30,
            user_agent: None,
            max_requests: 50,
            confirmed_by_user: false,
        }
    }
}

/// The result type for a webpage comparison.  Wraps the appropriate
/// existing result depending on the sub-mode.
#[derive(Debug)]
pub enum WebpageCompareResult {
    /// Returned by HtmlSource and ExtractedText modes.
    Text(crate::text::TextCompareResult),
    /// Returned by ResourceTree mode.
    Folder(crate::folder::FolderCompareResult),
    /// Returned by Rendered mode (web-engine feature).
    #[cfg(feature = "web-engine")]
    Rendered(WebpageRenderedResult),
    /// Returned by Screenshot mode (web-engine feature).
    #[cfg(feature = "web-engine")]
    Screenshot(crate::image::ImageCompareResult),
}

/// Structured result from the Rendered sub-mode.
#[cfg(feature = "web-engine")]
#[derive(Debug)]
pub struct WebpageRenderedResult {
    /// Rendered DOM diff as HTML source, or an image diff path if DOM diff is unavailable.
    pub dom_diff: Option<String>,
    /// Raw HTML source compare result used as fallback.
    pub html_fallback: Option<crate::text::TextCompareResult>,
}

/// Errors that can occur during webpage comparison.
#[derive(Debug)]
pub enum WebpageCompareError {
    ConfirmationRequired,
    Plugin(crate::plugin::PluginError),
    InvalidUrl(String),
    Text(String),
    Folder(crate::folder::FolderCompareError),
    Io(std::io::Error),
    Timeout { url: String, timeout_secs: u32 },
    UnexpectedPluginResponse(String),
    Cache(String),
}

impl std::fmt::Display for WebpageCompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfirmationRequired => {
                write!(f, "user confirmation required before network fetch")
            }
            Self::Plugin(e) => write!(f, "plugin error: {e}"),
            Self::InvalidUrl(u) => write!(f, "URL is not valid: {u}"),
            Self::Text(s) => write!(f, "text compare error: {s}"),
            Self::Folder(e) => write!(f, "folder compare error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Timeout { url, timeout_secs } => {
                write!(f, "network timeout after {timeout_secs}s for {url}")
            }
            Self::UnexpectedPluginResponse(s) => {
                write!(f, "plugin returned unexpected JSON: {s}")
            }
            Self::Cache(s) => write!(f, "cache error: {s}"),
        }
    }
}

impl std::error::Error for WebpageCompareError {}

impl From<crate::plugin::PluginError> for WebpageCompareError {
    fn from(e: crate::plugin::PluginError) -> Self {
        Self::Plugin(e)
    }
}

impl From<crate::folder::FolderCompareError> for WebpageCompareError {
    fn from(e: crate::folder::FolderCompareError) -> Self {
        Self::Folder(e)
    }
}

impl From<std::io::Error> for WebpageCompareError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Remove all cached data under `$XDG_CACHE_HOME/linsync/webcompare/`.
///
/// Deletes the directory and all its contents.  Returns `Ok(())` if the
/// directory does not exist (idempotent).
pub fn clear_webcompare_cache(cache_dir: &Path) -> Result<(), WebpageCompareError> {
    let webcompare_dir = cache_dir.join("webcompare");
    if webcompare_dir.exists() {
        std::fs::remove_dir_all(&webcompare_dir)?;
    }
    Ok(())
}

/// Returns the webcompare cache directory, creating it if needed.
pub fn webcompare_cache_dir(cache_dir: &Path) -> Result<PathBuf, WebpageCompareError> {
    let dir = cache_dir.join("webcompare").join("fetched");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn guard_confirmed(options: &WebpageCompareOptions) -> Result<(), WebpageCompareError> {
    if options.confirmed_by_user {
        Ok(())
    } else {
        Err(WebpageCompareError::ConfirmationRequired)
    }
}

fn validate_url(url: &str) -> Result<(), WebpageCompareError> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(())
    } else {
        Err(WebpageCompareError::InvalidUrl(url.to_owned()))
    }
}

/// Locate the web-fetch plugin directory by scanning a fixed relative path
/// (relative to CARGO_MANIFEST_DIR in tests, relative to the binary in
/// production).
fn find_web_fetch_plugin() -> Result<PathBuf, WebpageCompareError> {
    // In production: look for the plugin next to the running binary.
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        let candidate = exe_dir.join("plugins").join("web-fetch");
        if candidate.join("linsync-plugin.json").exists() {
            return Ok(candidate);
        }
    }

    // In tests / development: look relative to the workspace root.
    // CARGO_MANIFEST_DIR is set by cargo during test builds.
    // Walk up from any manifest dir we find to the workspace root, then
    // look for packaging/plugins/web-fetch.
    let workspace_candidates: &[&str] = &[env!("CARGO_MANIFEST_DIR")];
    for base in workspace_candidates {
        // linsync-core manifest is at crates/linsync-core/, workspace root is ../..
        let workspace_root = PathBuf::from(base)
            .join("../..")
            .join("packaging/plugins/web-fetch");
        let canonical = match workspace_root.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if canonical.join("linsync-plugin.json").exists() {
            return Ok(canonical);
        }
    }

    Err(WebpageCompareError::Plugin(crate::plugin::PluginError::Io(
        std::io::Error::new(std::io::ErrorKind::NotFound, "web-fetch plugin not found"),
    )))
}

fn make_plugin_exec_options(
    options: &WebpageCompareOptions,
) -> crate::plugin::PluginExecutionOptions {
    crate::plugin::PluginExecutionOptions {
        timeout: std::time::Duration::from_secs(u64::from(options.timeout_secs)),
        stdout_limit: 16 * 1024 * 1024,
        ..Default::default()
    }
}

fn invoke_web_fetch(
    plugin_dir: &Path,
    request_json: serde_json::Value,
    exec_opts: &crate::plugin::PluginExecutionOptions,
) -> Result<serde_json::Value, WebpageCompareError> {
    let manifest_path = plugin_dir.join("linsync-plugin.json");
    let manifest = crate::plugin::PluginManifest::from_manifest_file(&manifest_path)?;
    let request_str = request_json.to_string();
    let result = crate::plugin::run_plugin_helper(plugin_dir, &manifest, &request_str, exec_opts)?;
    let response: serde_json::Value = serde_json::from_str(&result.stdout)
        .map_err(|e| WebpageCompareError::UnexpectedPluginResponse(e.to_string()))?;
    if response
        .get("ok")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        Ok(response)
    } else {
        let err_msg = response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown plugin error")
            .to_owned();
        Err(WebpageCompareError::Plugin(crate::plugin::PluginError::Io(
            std::io::Error::other(err_msg),
        )))
    }
}

fn write_temp_text(dir: &Path, prefix: &str, text: &str) -> Result<PathBuf, WebpageCompareError> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("{prefix}-{timestamp}.txt"));
    std::fs::write(&path, text)?;
    Ok(path)
}

fn invoke_fetch_html(
    plugin_dir: &Path,
    url: &str,
    options: &WebpageCompareOptions,
    exec_opts: &crate::plugin::PluginExecutionOptions,
) -> Result<String, WebpageCompareError> {
    let req = serde_json::json!({
        "op": "fetch_html",
        "url": url,
        "timeout": options.timeout_secs,
        "user_agent": options.user_agent,
    });
    let resp = invoke_web_fetch(plugin_dir, req, exec_opts)?;
    resp.get("html")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            WebpageCompareError::UnexpectedPluginResponse(
                "missing 'html' field in fetch_html response".to_owned(),
            )
        })
}

fn invoke_extract_text(
    plugin_dir: &Path,
    url: &str,
    options: &WebpageCompareOptions,
    exec_opts: &crate::plugin::PluginExecutionOptions,
) -> Result<String, WebpageCompareError> {
    let req = serde_json::json!({
        "op": "extract_text",
        "url": url,
        "timeout": options.timeout_secs,
        "user_agent": options.user_agent,
    });
    let resp = invoke_web_fetch(plugin_dir, req, exec_opts)?;
    resp.get("text")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            WebpageCompareError::UnexpectedPluginResponse(
                "missing 'text' field in extract_text response".to_owned(),
            )
        })
}

fn invoke_resource_tree(
    plugin_dir: &Path,
    url: &str,
    options: &WebpageCompareOptions,
    exec_opts: &crate::plugin::PluginExecutionOptions,
) -> Result<Vec<(String, u16)>, WebpageCompareError> {
    let req = serde_json::json!({
        "op": "resource_tree",
        "url": url,
        "depth": options.resource_tree_depth.clamp(1, 3),
        "max_requests": options.max_requests,
        "timeout": options.timeout_secs,
        "user_agent": options.user_agent,
    });
    let resp = invoke_web_fetch(plugin_dir, req, exec_opts)?;
    let tree = resp.get("tree").and_then(|v| v.as_array()).ok_or_else(|| {
        WebpageCompareError::UnexpectedPluginResponse(
            "missing 'tree' array in resource_tree response".to_owned(),
        )
    })?;

    let mut result = Vec::with_capacity(tree.len());
    for item in tree {
        let url_val = item
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let status = item.get("status").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
        result.push((url_val, status));
    }
    Ok(result)
}

/// Convert two URL trees into a `FolderCompareResult`.
///
/// Each URL is treated as a "virtual path".  URLs present in both sides with
/// the same status are `Identical`; if status differs they are `Different`;
/// left-only → `LeftOnly`; right-only → `RightOnly`.
pub(crate) fn url_tree_to_folder_result(
    left_tree: &[(String, u16)],
    right_tree: &[(String, u16)],
) -> crate::folder::FolderCompareResult {
    use crate::folder::{
        CompareMethod, FolderCompareResult, FolderCompareStatus, FolderCompareSummary,
        FolderEntryDiff, FolderEntryState, FolderEntryType,
    };
    use std::collections::BTreeMap;

    let left_map: BTreeMap<&str, u16> = left_tree.iter().map(|(u, s)| (u.as_str(), *s)).collect();
    let right_map: BTreeMap<&str, u16> = right_tree.iter().map(|(u, s)| (u.as_str(), *s)).collect();

    let mut all_urls: Vec<&str> = left_map.keys().chain(right_map.keys()).copied().collect();
    all_urls.sort_unstable();
    all_urls.dedup();

    let mut entries: Vec<FolderEntryDiff> = Vec::with_capacity(all_urls.len());
    let mut identical_count = 0usize;
    let mut different_count = 0usize;
    let mut left_only_count = 0usize;
    let mut right_only_count = 0usize;

    for url in all_urls {
        let state = match (left_map.get(url), right_map.get(url)) {
            (Some(ls), Some(rs)) => {
                if ls == rs {
                    identical_count += 1;
                    FolderEntryState::Identical
                } else {
                    different_count += 1;
                    FolderEntryState::Different
                }
            }
            (Some(_), None) => {
                left_only_count += 1;
                FolderEntryState::LeftOnly
            }
            (None, Some(_)) => {
                right_only_count += 1;
                FolderEntryState::RightOnly
            }
            (None, None) => unreachable!(),
        };

        let name = url.rsplit('/').next().unwrap_or(url).to_owned();
        entries.push(FolderEntryDiff {
            relative_path: std::path::PathBuf::from(url),
            name: name.clone(),
            extension: std::path::Path::new(&name)
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_owned),
            state,
            left_size: None,
            right_size: None,
            left_modified: None,
            right_modified: None,
            entry_type: FolderEntryType::File,
            effective_method: Some(CompareMethod::Existence),
            method_note: None,
            is_dir: false,
            error: None,
        });
    }

    let compared_count = identical_count + different_count + left_only_count + right_only_count;
    let one_sided_count = left_only_count + right_only_count;

    FolderCompareResult {
        left_root: PathBuf::from("(url)"),
        right_root: PathBuf::from("(url)"),
        entries,
        summary: FolderCompareSummary {
            compared_count,
            skipped_count: 0,
            identical_count,
            different_count,
            one_sided_count,
            left_only_count,
            right_only_count,
            errors_count: 0,
            aborted_count: 0,
            method_downgrade_count: 0,
            elapsed: std::time::Duration::ZERO,
            status: FolderCompareStatus::Complete,
        },
    }
}

// ── Public entry points ───────────────────────────────────────────────────────

pub fn compare_webpage_html_source(
    left_url: &str,
    right_url: &str,
    options: &WebpageCompareOptions,
    cache_dir: &Path,
) -> Result<WebpageCompareResult, WebpageCompareError> {
    guard_confirmed(options)?;
    validate_url(left_url)?;
    validate_url(right_url)?;
    let plugin_dir = find_web_fetch_plugin()?;
    let exec_opts = make_plugin_exec_options(options);
    let left_html = invoke_fetch_html(&plugin_dir, left_url, options, &exec_opts)?;
    let right_html = invoke_fetch_html(&plugin_dir, right_url, options, &exec_opts)?;
    let fetch_dir = webcompare_cache_dir(cache_dir)?;
    let left_path = write_temp_text(&fetch_dir, "left-html", &left_html)?;
    let right_path = write_temp_text(&fetch_dir, "right-html", &right_html)?;
    let cmp = crate::text::compare_text_files(
        &left_path,
        &right_path,
        &crate::text::TextCompareOptions::default(),
    )
    .map_err(|e| WebpageCompareError::Text(e.to_string()))?;
    Ok(WebpageCompareResult::Text(cmp))
}

pub fn compare_webpage_extracted_text(
    left_url: &str,
    right_url: &str,
    options: &WebpageCompareOptions,
    cache_dir: &Path,
) -> Result<WebpageCompareResult, WebpageCompareError> {
    guard_confirmed(options)?;
    validate_url(left_url)?;
    validate_url(right_url)?;
    let plugin_dir = find_web_fetch_plugin()?;
    let exec_opts = make_plugin_exec_options(options);
    let left_text = invoke_extract_text(&plugin_dir, left_url, options, &exec_opts)?;
    let right_text = invoke_extract_text(&plugin_dir, right_url, options, &exec_opts)?;
    let fetch_dir = webcompare_cache_dir(cache_dir)?;
    let left_path = write_temp_text(&fetch_dir, "left-text", &left_text)?;
    let right_path = write_temp_text(&fetch_dir, "right-text", &right_text)?;
    let cmp = crate::text::compare_text_files(
        &left_path,
        &right_path,
        &crate::text::TextCompareOptions::default(),
    )
    .map_err(|e| WebpageCompareError::Text(e.to_string()))?;
    Ok(WebpageCompareResult::Text(cmp))
}

pub fn compare_webpage_resource_tree(
    left_url: &str,
    right_url: &str,
    options: &WebpageCompareOptions,
    cache_dir: &Path,
) -> Result<WebpageCompareResult, WebpageCompareError> {
    guard_confirmed(options)?;
    validate_url(left_url)?;
    validate_url(right_url)?;
    let plugin_dir = find_web_fetch_plugin()?;
    let exec_opts = make_plugin_exec_options(options);
    let left_tree = invoke_resource_tree(&plugin_dir, left_url, options, &exec_opts)?;
    let right_tree = invoke_resource_tree(&plugin_dir, right_url, options, &exec_opts)?;
    // Ensure cache_dir is created (side-effect required by the public contract).
    webcompare_cache_dir(cache_dir)?;
    let folder_result = url_tree_to_folder_result(&left_tree, &right_tree);
    Ok(WebpageCompareResult::Folder(folder_result))
}

#[cfg(feature = "web-engine")]
pub fn compare_webpage_rendered(
    left_url: &str,
    right_url: &str,
    options: &WebpageCompareOptions,
    cache_dir: &Path,
) -> Result<WebpageCompareResult, WebpageCompareError> {
    guard_confirmed(options)?;
    validate_url(left_url)?;
    validate_url(right_url)?;
    let profile_dir = cache_dir.join("webcompare").join("profile");
    std::fs::create_dir_all(&profile_dir)?;
    let engine_opts = linsync_webengine::WebEngineOptions {
        profile_storage_dir: profile_dir,
        timeout_secs: options.timeout_secs,
        ..Default::default()
    };
    let fetch_dir = webcompare_cache_dir(cache_dir)?;
    let left_result = linsync_webengine::render_url(left_url, &fetch_dir, &engine_opts);
    let right_result = linsync_webengine::render_url(right_url, &fetch_dir, &engine_opts);
    match (left_result, right_result) {
        (Ok(_left_png), Ok(_right_png)) => {
            // Phase 9.7-bis: forward PNGs to image compare.
            unimplemented!("Phase 9.7-bis: compare rendered PNGs via image compare")
        }
        (Err(linsync_webengine::WebEngineError::NotImplemented), _)
        | (_, Err(linsync_webengine::WebEngineError::NotImplemented)) => {
            let fallback = compare_webpage_html_source(left_url, right_url, options, cache_dir)?;
            if let WebpageCompareResult::Text(text_cmp) = fallback {
                Ok(WebpageCompareResult::Rendered(WebpageRenderedResult {
                    dom_diff: None,
                    html_fallback: Some(text_cmp),
                }))
            } else {
                unreachable!()
            }
        }
        (Err(e), _) | (_, Err(e)) => Err(WebpageCompareError::Plugin(
            crate::plugin::PluginError::Io(std::io::Error::other(e.to_string())),
        )),
    }
}

#[cfg(feature = "web-engine")]
pub fn compare_webpage_screenshot(
    left_url: &str,
    right_url: &str,
    options: &WebpageCompareOptions,
    cache_dir: &Path,
) -> Result<WebpageCompareResult, WebpageCompareError> {
    guard_confirmed(options)?;
    validate_url(left_url)?;
    validate_url(right_url)?;
    let profile_dir = cache_dir.join("webcompare").join("profile");
    std::fs::create_dir_all(&profile_dir)?;
    let engine_opts = linsync_webengine::WebEngineOptions {
        profile_storage_dir: profile_dir,
        timeout_secs: options.timeout_secs,
        ..Default::default()
    };
    let fetch_dir = webcompare_cache_dir(cache_dir)?;
    let left_png =
        linsync_webengine::render_url(left_url, &fetch_dir, &engine_opts).map_err(|e| {
            WebpageCompareError::Plugin(crate::plugin::PluginError::Io(std::io::Error::other(
                e.to_string(),
            )))
        })?;
    let right_png =
        linsync_webengine::render_url(right_url, &fetch_dir, &engine_opts).map_err(|e| {
            WebpageCompareError::Plugin(crate::plugin::PluginError::Io(std::io::Error::other(
                e.to_string(),
            )))
        })?;
    let img_result = crate::image::compare_images(
        &left_png,
        &right_png,
        &crate::image::ImageCompareOptions::default(),
    )
    .map_err(|e| WebpageCompareError::Text(e.to_string()))?;
    Ok(WebpageCompareResult::Screenshot(img_result))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Task 9.1 tests ────────────────────────────────────────────────────────

    #[test]
    fn default_options_not_confirmed() {
        let opts = WebpageCompareOptions::default();
        assert!(!opts.confirmed_by_user);
        assert_eq!(opts.resource_tree_depth, 1);
        assert_eq!(opts.timeout_secs, 30);
        assert_eq!(opts.max_requests, 50);
    }

    #[test]
    fn clear_cache_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        // Non-existent dir: should not error.
        clear_webcompare_cache(tmp.path()).unwrap();
        // Create dir, then clear.
        let wc = tmp.path().join("webcompare");
        std::fs::create_dir_all(&wc).unwrap();
        std::fs::write(wc.join("dummy.txt"), b"x").unwrap();
        clear_webcompare_cache(tmp.path()).unwrap();
        assert!(!wc.exists());
    }

    #[test]
    fn webcompare_cache_dir_creates_fetched_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = webcompare_cache_dir(tmp.path()).unwrap();
        assert!(dir.is_dir());
        assert!(dir.ends_with("webcompare/fetched"));
    }

    // ── Task 9.3 tests ────────────────────────────────────────────────────────

    fn plugin_script_exists() -> bool {
        let p = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packaging/plugins/web-fetch/web-fetch"
        ));
        if p.exists() {
            // SAFETY: All webpage tests need this env var; tests run in the
            // same process and the value never changes once set, so the
            // race is benign. Setting it here keeps the production SSRF
            // defenses intact while allowing httptest fixture servers
            // bound to loopback to be reached.
            unsafe { std::env::set_var("LINSYNC_WEB_FETCH_ALLOW_LOOPBACK", "1") };
            true
        } else {
            false
        }
    }

    #[cfg(any())]
    fn simple_server(_body: &'static str) -> () {}

    #[test]
    fn html_source_requires_confirmation() {
        let opts = WebpageCompareOptions::default();
        let tmp = tempfile::tempdir().unwrap();
        let err = compare_webpage_html_source(
            "http://127.0.0.1:9999/",
            "http://127.0.0.1:9999/",
            &opts,
            tmp.path(),
        )
        .unwrap_err();
        assert!(
            matches!(err, WebpageCompareError::ConfirmationRequired),
            "got: {err:?}"
        );
    }

    #[test]
    fn html_source_identical_pages_produce_no_diff() {
        if !plugin_script_exists() {
            eprintln!("skip: web-fetch plugin not found");
            return;
        }
        use httptest::{Expectation, Server, matchers::*, responders::*};
        let server_l = Server::run();
        server_l.expect(
            Expectation::matching(request::method_path("GET", "/"))
                .respond_with(status_code(200).body("<html><body>Hello</body></html>")),
        );
        let server_r = Server::run();
        server_r.expect(
            Expectation::matching(request::method_path("GET", "/"))
                .respond_with(status_code(200).body("<html><body>Hello</body></html>")),
        );
        let opts = WebpageCompareOptions {
            confirmed_by_user: true,
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let result = compare_webpage_html_source(
            &server_l.url_str("/"),
            &server_r.url_str("/"),
            &opts,
            tmp.path(),
        )
        .unwrap();
        if let WebpageCompareResult::Text(text_result) = result {
            use crate::text::DiffBlockKind;
            let changed = text_result
                .blocks
                .iter()
                .any(|b| b.kind == DiffBlockKind::Difference);
            assert!(!changed, "identical pages should produce no diff");
        } else {
            panic!("expected Text result");
        }
    }

    #[test]
    fn html_source_different_pages_produce_diff() {
        if !plugin_script_exists() {
            return;
        }
        use httptest::{Expectation, Server, matchers::*, responders::*};
        let server_l = Server::run();
        server_l.expect(
            Expectation::matching(request::method_path("GET", "/"))
                .respond_with(status_code(200).body("<html><body>Left</body></html>")),
        );
        let server_r = Server::run();
        server_r.expect(
            Expectation::matching(request::method_path("GET", "/"))
                .respond_with(status_code(200).body("<html><body>Right</body></html>")),
        );
        let opts = WebpageCompareOptions {
            confirmed_by_user: true,
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let result = compare_webpage_html_source(
            &server_l.url_str("/"),
            &server_r.url_str("/"),
            &opts,
            tmp.path(),
        )
        .unwrap();
        if let WebpageCompareResult::Text(text_result) = result {
            use crate::text::DiffBlockKind;
            let has_diff = text_result
                .blocks
                .iter()
                .any(|b| b.kind == DiffBlockKind::Difference);
            assert!(has_diff, "different pages should produce a diff");
        } else {
            panic!("expected Text result");
        }
    }

    // ── Task 9.4 tests ────────────────────────────────────────────────────────

    #[test]
    fn extracted_text_requires_confirmation() {
        let opts = WebpageCompareOptions::default();
        let tmp = tempfile::tempdir().unwrap();
        let err = compare_webpage_extracted_text("http://x/", "http://x/", &opts, tmp.path())
            .unwrap_err();
        assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
    }

    #[test]
    fn extracted_text_strips_tags() {
        if !plugin_script_exists() {
            return;
        }
        use httptest::{Expectation, Server, matchers::*, responders::*};
        let server_l = Server::run();
        server_l.expect(
            Expectation::matching(request::method_path("GET", "/")).respond_with(
                status_code(200).body("<html><body><h1>Heading</h1><p>Para left</p></body></html>"),
            ),
        );
        let server_r = Server::run();
        server_r.expect(
            Expectation::matching(request::method_path("GET", "/")).respond_with(
                status_code(200)
                    .body("<html><body><h1>Heading</h1><p>Para right</p></body></html>"),
            ),
        );
        let opts = WebpageCompareOptions {
            confirmed_by_user: true,
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let result = compare_webpage_extracted_text(
            &server_l.url_str("/"),
            &server_r.url_str("/"),
            &opts,
            tmp.path(),
        )
        .unwrap();
        if let WebpageCompareResult::Text(cmp) = result {
            use crate::text::DiffBlockKind;
            let has_diff = cmp
                .blocks
                .iter()
                .any(|b| b.kind == DiffBlockKind::Difference);
            assert!(has_diff);
        } else {
            panic!("expected Text result");
        }
    }

    // ── Task 9.5 tests ────────────────────────────────────────────────────────

    #[test]
    fn resource_tree_requires_confirmation() {
        let opts = WebpageCompareOptions::default();
        let tmp = tempfile::tempdir().unwrap();
        let err =
            compare_webpage_resource_tree("http://x/", "http://x/", &opts, tmp.path()).unwrap_err();
        assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
    }

    #[test]
    fn resource_tree_detects_left_only_link() {
        if !plugin_script_exists() {
            return;
        }
        use httptest::{Expectation, Server, matchers::*, responders::*};
        let server_l = Server::run();
        server_l.expect(
            Expectation::matching(request::method_path("HEAD", "/"))
                .times(0..)
                .respond_with(status_code(200)),
        );
        server_l.expect(
            Expectation::matching(request::method_path("GET", "/"))
                .times(1..)
                .respond_with(status_code(200).body(r#"<a href="/extra.html">extra</a>"#)),
        );
        server_l.expect(
            Expectation::matching(request::method_path("HEAD", "/extra.html"))
                .times(0..)
                .respond_with(status_code(200)),
        );

        let server_r = Server::run();
        server_r.expect(
            Expectation::matching(request::method_path("HEAD", "/"))
                .times(0..)
                .respond_with(status_code(200)),
        );
        server_r.expect(
            Expectation::matching(request::method_path("GET", "/"))
                .times(1..)
                .respond_with(status_code(200).body("<p>Right only</p>")),
        );

        let opts = WebpageCompareOptions {
            confirmed_by_user: true,
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let result = compare_webpage_resource_tree(
            &server_l.url_str("/"),
            &server_r.url_str("/"),
            &opts,
            tmp.path(),
        )
        .unwrap();
        if let WebpageCompareResult::Folder(folder_result) = result {
            use crate::folder::FolderEntryState;
            let left_only = folder_result
                .entries
                .iter()
                .any(|e| e.state == FolderEntryState::LeftOnly);
            assert!(left_only, "expected a left-only entry for /extra.html");
        } else {
            panic!("expected Folder result");
        }
    }

    #[test]
    fn url_tree_to_folder_result_both_same_url() {
        let left = vec![("/index".to_string(), 200u16)];
        let right = vec![("/index".to_string(), 200u16)];
        let result = url_tree_to_folder_result(&left, &right);
        use crate::folder::FolderEntryState;
        assert!(
            result
                .entries
                .iter()
                .all(|e| e.state == FolderEntryState::Identical)
        );
    }

    // ── Task 9.6 tests ────────────────────────────────────────────────────────

    #[test]
    fn html_source_confirmation_guard_fires_before_io() {
        let opts = WebpageCompareOptions {
            confirmed_by_user: false,
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let err =
            compare_webpage_html_source("not-a-url", "not-a-url", &opts, tmp.path()).unwrap_err();
        assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
    }

    #[test]
    fn extracted_text_confirmation_guard_fires_before_io() {
        let opts = WebpageCompareOptions {
            confirmed_by_user: false,
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let err = compare_webpage_extracted_text("not-a-url", "not-a-url", &opts, tmp.path())
            .unwrap_err();
        assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
    }

    #[test]
    fn resource_tree_confirmation_guard_fires_before_io() {
        let opts = WebpageCompareOptions {
            confirmed_by_user: false,
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let err =
            compare_webpage_resource_tree("not-a-url", "not-a-url", &opts, tmp.path()).unwrap_err();
        assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
    }

    // ── Task 9.8 tests ────────────────────────────────────────────────────────

    #[cfg(feature = "web-engine")]
    #[test]
    fn rendered_requires_confirmation() {
        let opts = WebpageCompareOptions::default();
        let tmp = tempfile::tempdir().unwrap();
        let err =
            compare_webpage_rendered("http://x/", "http://x/", &opts, tmp.path()).unwrap_err();
        assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
    }

    #[cfg(feature = "web-engine")]
    #[test]
    fn rendered_stub_falls_back_to_html_source() {
        if !plugin_script_exists() {
            return;
        }
        use httptest::{Expectation, Server, matchers::*, responders::*};
        let server_l = Server::run();
        server_l.expect(
            Expectation::matching(request::method_path("GET", "/"))
                .times(1..)
                .respond_with(status_code(200).body("<html><body>A</body></html>")),
        );
        let server_r = Server::run();
        server_r.expect(
            Expectation::matching(request::method_path("GET", "/"))
                .times(1..)
                .respond_with(status_code(200).body("<html><body>A</body></html>")),
        );
        let opts = WebpageCompareOptions {
            confirmed_by_user: true,
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let result = compare_webpage_rendered(
            &server_l.url_str("/"),
            &server_r.url_str("/"),
            &opts,
            tmp.path(),
        )
        .unwrap();
        assert!(
            matches!(result, WebpageCompareResult::Rendered(_)),
            "expected Rendered variant"
        );
    }

    #[cfg(feature = "web-engine")]
    #[test]
    fn screenshot_requires_confirmation() {
        let opts = WebpageCompareOptions::default();
        let tmp = tempfile::tempdir().unwrap();
        let err =
            compare_webpage_screenshot("http://x/", "http://x/", &opts, tmp.path()).unwrap_err();
        assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
    }
}
