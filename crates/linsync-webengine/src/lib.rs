//! Qt WebEngine wrapper for LinSync webpage compare (rendered + screenshot sub-modes).
//!
//! This crate is only compiled when the `web-engine` feature is enabled on
//! `linsync-core`. In the default build it is not depended on and the
//! rendered/screenshot sub-modes are absent from the public API.
//!
//! # Architecture: out-of-process renderer
//!
//! Rendering a page needs a running Qt event loop and a Chromium compositor.
//! That cannot run inside a synchronous library call when the caller (the GUI
//! bridge) already owns a `QGuiApplication`, and the CLI has no event loop at
//! all. So [`render_url`] spawns a short-lived `qml6` process running a
//! generated `WebEngineView` document that loads the URL, grabs the rendered
//! view to an image, saves it as PNG, and exits — the same out-of-process
//! pattern LinSync already uses for plugins and the external QML host. This
//! works uniformly for both the CLI and the GUI, and renders headlessly under
//! `QT_QPA_PLATFORM=offscreen`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Chromium flags shared by both backends: the QtQml arm forwards them via
/// `QTWEBENGINE_CHROMIUM_FLAGS`, the headless arm passes them on the command
/// line directly. Keep the two arms consistent by editing this list only.
const SHARED_CHROMIUM_FLAGS: &[&str] = &["--disable-gpu", "--disable-dev-shm-usage"];

/// True when running inside a Flatpak/bwrap sandbox, where nested user
/// namespaces are unavailable and Chromium's own sandbox cannot start.
fn inside_flatpak_sandbox() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Errors from the web-engine wrapper.
#[derive(Debug)]
pub enum WebEngineError {
    NotImplemented,
    InitFailed(String),
    PageLoadFailed { url: String, reason: String },
    CaptureFailed(String),
}

impl std::fmt::Display for WebEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotImplemented => {
                write!(
                    f,
                    "no web rendering backend available: the build lacks the web-engine \
                     feature or no QML runner / Chromium binary was found on PATH"
                )
            }
            Self::InitFailed(s) => write!(f, "Qt WebEngine initialization failed: {s}"),
            Self::PageLoadFailed { url, reason } => {
                write!(f, "page load failed for {url}: {reason}")
            }
            Self::CaptureFailed(s) => write!(f, "screenshot capture failed: {s}"),
        }
    }
}

impl std::error::Error for WebEngineError {}

/// Options for a web-engine rendering session.
#[derive(Debug, Clone)]
pub struct WebEngineOptions {
    /// Directory for the isolated `QWebEngineProfile` storage.
    /// Typically `$XDG_CACHE_HOME/linsync/webcompare/profile/`.
    pub profile_storage_dir: PathBuf,
    /// Viewport width in logical pixels.  Default 1280.
    pub viewport_width: u32,
    /// Viewport height in logical pixels.  Default 900.
    pub viewport_height: u32,
    /// Page-load timeout in seconds.  Default 30.
    pub timeout_secs: u32,
}

impl Default for WebEngineOptions {
    fn default() -> Self {
        Self {
            profile_storage_dir: std::env::temp_dir().join("linsync-webengine-profile"),
            viewport_width: 1280,
            viewport_height: 900,
            timeout_secs: 30,
        }
    }
}

/// The renderer backend used to rasterize a page.
#[derive(Clone)]
enum RenderBackend {
    /// Qt WebEngine via a `qml6`/`qml` runner subprocess — the current path.
    QtQml(PathBuf),
    /// Headless Chromium binary (`--headless=new --screenshot=…`).
    ChromiumHeadless(PathBuf),
}

/// Probe whether `candidate` is an executable on `PATH` by running it with a
/// cheap informational flag and checking it produced any response.
fn binary_responds(candidate: &str, probe_arg: &str) -> bool {
    Command::new(candidate)
        .arg(probe_arg)
        .output()
        .map(|o| o.status.success() || !o.stdout.is_empty() || !o.stderr.is_empty())
        .unwrap_or(false)
}

/// Locate a Qt 6 QML runner (`qml6`, then `qml`), honoring `LINSYNC_QML_RUNNER`.
fn resolve_qml_runner() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("LINSYNC_QML_RUNNER") {
        let path = PathBuf::from(explicit);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    for candidate in ["qml6", "qml"] {
        if binary_responds(candidate, "--help") {
            return Some(PathBuf::from(candidate));
        }
    }
    None
}

/// Locate a headless-capable Chromium binary on `PATH`.
fn resolve_chromium_binary() -> Option<PathBuf> {
    for candidate in ["chromium", "chromium-browser", "google-chrome-stable"] {
        if binary_responds(candidate, "--version") {
            return Some(PathBuf::from(candidate));
        }
    }
    None
}

/// Pick a renderer backend.
///
/// 1. `LINSYNC_WEB_RENDERER=qml|chromium` forces a backend (`None` if the
///    forced backend's binary is absent; any other non-empty value warns and
///    falls through to auto-detection).
/// 2. A QML runner (`qml6`/`qml`, honoring `LINSYNC_QML_RUNNER`).
/// 3. A Chromium binary on `PATH`.
///
/// The result is cached per `(LINSYNC_WEB_RENDERER, LINSYNC_QML_RUNNER)` pair:
/// auto-detection spawns up to five probe subprocesses, and both `/capabilities`
/// and every rendered/screenshot compare resolve a backend. Keying by the env
/// values (rather than an unconditional cache) keeps in-process overrides — the
/// forcing vars, chiefly in tests — honest while still caching the expensive
/// PATH probes within one configuration.
fn resolve_backend() -> Option<RenderBackend> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    /// Cache of resolved backends keyed by the two forcing env vars.
    type BackendCache = Mutex<HashMap<(String, String), Option<RenderBackend>>>;
    static CACHE: OnceLock<BackendCache> = OnceLock::new();
    let key = (
        std::env::var("LINSYNC_WEB_RENDERER").unwrap_or_default(),
        std::env::var("LINSYNC_QML_RUNNER").unwrap_or_default(),
    );
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock()
        && let Some(cached) = guard.get(&key)
    {
        return cached.clone();
    }
    let resolved = resolve_backend_uncached();
    if let Ok(mut guard) = cache.lock() {
        guard.insert(key, resolved.clone());
    }
    resolved
}

fn resolve_backend_uncached() -> Option<RenderBackend> {
    match std::env::var("LINSYNC_WEB_RENDERER").as_deref() {
        Ok("qml") => return resolve_qml_runner().map(RenderBackend::QtQml),
        Ok("chromium") => return resolve_chromium_binary().map(RenderBackend::ChromiumHeadless),
        Ok("") | Err(_) => {}
        Ok(other) => {
            eprintln!(
                "linsync-webengine: warning: unknown LINSYNC_WEB_RENDERER value {other:?} \
                 (expected \"qml\" or \"chromium\"); auto-detecting a backend"
            );
        }
    }
    if let Some(runner) = resolve_qml_runner() {
        return Some(RenderBackend::QtQml(runner));
    }
    resolve_chromium_binary().map(RenderBackend::ChromiumHeadless)
}

/// Report which renderer backend [`render_url`] would use right now, as a
/// stable string for capability reporting: `"qml"` (Qt WebEngine via a QML
/// runner), `"chromium"` (headless Chromium binary), or `"none"` (no usable
/// backend — rendered/screenshot modes are unavailable). Honors the same
/// `LINSYNC_WEB_RENDERER` override as [`render_url`].
pub fn active_renderer_kind() -> &'static str {
    match resolve_backend() {
        Some(RenderBackend::QtQml(_)) => "qml",
        Some(RenderBackend::ChromiumHeadless(_)) => "chromium",
        None => "none",
    }
}

/// Escape a string for embedding inside a QML double-quoted string literal.
///
/// Besides backslashes and quotes, line terminators and NUL must be escaped:
/// QML/JS string literals reject raw newlines, so a hostile URL containing one
/// would otherwise turn into a `qml6` parse error (or worse, inject QML).
fn qml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\u0000"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// A stable, filesystem-safe hash of a URL for naming its PNG.
fn url_hash(url: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Render `url` in an isolated Qt WebEngine session and return the path to a PNG
/// screenshot written to `output_dir/<url_hash>.png`.
///
/// Dispatches to a renderer backend (see [`resolve_backend`]). The Qt backend
/// spawns a `qml6` subprocess running a generated `WebEngineView` that loads the
/// URL, grabs the rendered view, and saves a PNG; the Chromium backend spawns
/// `chromium --headless=new --screenshot=…` against an ephemeral profile. Returns
/// [`WebEngineError::NotImplemented`] when no usable backend is available (so
/// callers can fall back to HTML-source compare), and a specific error on
/// load/capture failure or timeout.
pub fn render_url(
    url: &str,
    output_dir: &Path,
    options: &WebEngineOptions,
) -> Result<PathBuf, WebEngineError> {
    let Some(backend) = resolve_backend() else {
        return Err(WebEngineError::NotImplemented);
    };
    match backend {
        RenderBackend::QtQml(runner) => render_url_qtqml(url, output_dir, options, &runner),
        RenderBackend::ChromiumHeadless(binary) => {
            render_url_chromium(&binary, url, output_dir, options)
        }
    }
}

/// Wait for `child` with a hard wall-clock `deadline`. Returns `Ok(None)` when
/// the deadline passed (the child has been killed and reaped).
fn wait_with_deadline(child: &mut Child, deadline: Instant) -> std::io::Result<Option<ExitStatus>> {
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(Some(status)),
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Read at most the last `max_bytes` bytes of the file at `path` as lossy
/// UTF-8, trimmed. Returns an empty string when the file cannot be read.
fn read_tail(path: &Path, max_bytes: usize) -> String {
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    let start = bytes.len().saturating_sub(max_bytes);
    String::from_utf8_lossy(&bytes[start..]).trim().to_owned()
}

/// Render `url` with a headless Chromium `binary` and return the path to the
/// PNG screenshot written to `output_dir/<url_hash>.png`.
///
/// Privacy contract: the browser runs against a LinSync-owned profile
/// (`--user-data-dir` under [`WebEngineOptions::profile_storage_dir`]) — it
/// never touches the user's real browser profile. "Ephemeral" here means
/// wiped-by-[`clear_profile`]: the directory persists between renders (so
/// per-URL cache survives), and concurrent renders would contend on
/// Chromium's `SingletonLock` in it — fine today because core renders
/// sequentially (left, then right).
fn render_url_chromium(
    binary: &Path,
    url: &str,
    output_dir: &Path,
    options: &WebEngineOptions,
) -> Result<PathBuf, WebEngineError> {
    std::fs::create_dir_all(output_dir).map_err(|e| WebEngineError::InitFailed(e.to_string()))?;
    let user_data_dir = options.profile_storage_dir.join("chromium-profile");
    std::fs::create_dir_all(&user_data_dir)
        .map_err(|e| WebEngineError::InitFailed(e.to_string()))?;

    let output_png = output_dir.join(format!("{}.png", url_hash(url)));
    let _ = std::fs::remove_file(&output_png);

    let width = options.viewport_width.max(1);
    let height = options.viewport_height.max(1);
    let timeout_ms = u64::from(options.timeout_secs.max(1)).saturating_mul(1000);

    // Capture stderr into a file (not a pipe) so a chatty browser can never
    // fill a pipe buffer and deadlock against our wait loop.
    let stderr_log = user_data_dir.join(format!("stderr-{}.log", url_hash(url)));
    let stderr_file = std::fs::File::create(&stderr_log)
        .map_err(|e| WebEngineError::InitFailed(e.to_string()))?;

    let mut command = Command::new(binary);
    command
        .arg("--headless=new")
        .args(SHARED_CHROMIUM_FLAGS)
        .arg(format!("--user-data-dir={}", user_data_dir.display()))
        .arg(format!("--window-size={width},{height}"))
        .arg(format!("--virtual-time-budget={timeout_ms}"))
        .arg(format!("--screenshot={}", output_png.display()));
    if inside_flatpak_sandbox() {
        command.arg("--no-sandbox");
    }
    command
        // `--` terminates flag parsing so a hostile URL beginning with `-`
        // can never be interpreted as a Chromium switch (defense-in-depth:
        // callers validate URLs upstream).
        .arg("--")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file));

    let mut child = command
        .spawn()
        .map_err(|e| WebEngineError::InitFailed(format!("failed to launch {binary:?}: {e}")))?;

    // Same backstop as the QtQml arm: the page-load budget plus grace for
    // browser startup/teardown, then a hard kill.
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.saturating_add(5_000));
    let status = match wait_with_deadline(&mut child, deadline) {
        Ok(Some(status)) => status,
        Ok(None) => {
            // Surface whatever the browser logged before the kill — it is
            // usually the only clue to why a render hung.
            let tail = read_tail(&stderr_log, 400);
            let _ = std::fs::remove_file(&stderr_log);
            let reason = if tail.is_empty() {
                "render timed out".to_owned()
            } else {
                format!("render timed out; chromium stderr tail: {tail}")
            };
            return Err(WebEngineError::PageLoadFailed {
                url: url.to_owned(),
                reason,
            });
        }
        Err(e) => {
            let _ = std::fs::remove_file(&stderr_log);
            return Err(WebEngineError::InitFailed(e.to_string()));
        }
    };

    let stderr_tail = read_tail(&stderr_log, 400);
    let _ = std::fs::remove_file(&stderr_log);

    if !status.success() {
        let reason = if stderr_tail.is_empty() {
            format!("chromium exited with code {}", status.code().unwrap_or(-1))
        } else {
            stderr_tail
        };
        return Err(WebEngineError::PageLoadFailed {
            url: url.to_owned(),
            reason,
        });
    }

    match std::fs::metadata(&output_png) {
        Ok(meta) if meta.len() > 0 => Ok(output_png),
        _ => Err(WebEngineError::CaptureFailed(
            "chromium exited successfully but no screenshot PNG was produced".to_owned(),
        )),
    }
}

/// Render `url` with the Qt WebEngine backend via the given QML `runner`.
fn render_url_qtqml(
    url: &str,
    output_dir: &Path,
    options: &WebEngineOptions,
    runner: &Path,
) -> Result<PathBuf, WebEngineError> {
    std::fs::create_dir_all(output_dir).map_err(|e| WebEngineError::InitFailed(e.to_string()))?;
    std::fs::create_dir_all(&options.profile_storage_dir)
        .map_err(|e| WebEngineError::InitFailed(e.to_string()))?;

    let output_png = output_dir.join(format!("{}.png", url_hash(url)));
    let _ = std::fs::remove_file(&output_png);

    let width = options.viewport_width.max(1);
    let height = options.viewport_height.max(1);
    let timeout_ms = u64::from(options.timeout_secs.max(1)).saturating_mul(1000);

    // The renderer document: load the URL, grab the view to the configured
    // size, save the PNG, and exit. A guard timer exits non-zero if the page
    // never finishes loading.
    let renderer_qml = format!(
        r#"import QtQuick
import QtWebEngine

Item {{
    width: {width}; height: {height}
    Timer {{
        interval: {timeout_ms}; running: true; repeat: false
        onTriggered: Qt.exit(4)
    }}
    WebEngineView {{
        id: view
        anchors.fill: parent
        url: "{url}"
        onLoadingChanged: function(info) {{
            if (info.status === WebEngineView.LoadSucceededStatus) {{
                view.grabToImage(function(result) {{
                    var ok = result.saveToFile("{output}");
                    Qt.exit(ok ? 0 : 3);
                }}, Qt.size({width}, {height}));
            }} else if (info.status === WebEngineView.LoadFailedStatus) {{
                Qt.exit(2);
            }}
        }}
    }}
}}
"#,
        width = width,
        height = height,
        timeout_ms = timeout_ms,
        url = qml_escape(url),
        output = qml_escape(&output_png.to_string_lossy()),
    );

    // Write the renderer into the profile dir (it contains only the public URL
    // and output path, so no owner-only handling is required).
    let renderer_path = options
        .profile_storage_dir
        .join(format!("render-{}.qml", url_hash(url)));
    {
        let mut file = std::fs::File::create(&renderer_path)
            .map_err(|e| WebEngineError::InitFailed(e.to_string()))?;
        file.write_all(renderer_qml.as_bytes())
            .map_err(|e| WebEngineError::InitFailed(e.to_string()))?;
    }

    let mut command = Command::new(runner);
    command
        .arg(&renderer_path)
        // Headless rendering: offscreen platform + GPU-less Chromium.
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QTWEBENGINE_DISABLE_SANDBOX", "1")
        .env(
            "QTWEBENGINE_CHROMIUM_FLAGS",
            // QtWebEngine's bundled Chromium runs with its sandbox disabled
            // here (offscreen embedding requires it); the standalone Chromium
            // arm keeps its sandbox on outside Flatpak.
            format!("{} --no-sandbox", SHARED_CHROMIUM_FLAGS.join(" ")),
        );

    let mut child = command
        .spawn()
        .map_err(|e| WebEngineError::InitFailed(format!("failed to launch {runner:?}: {e}")))?;

    // Wait with a hard wall-clock ceiling (the QML guard timer should exit
    // first; this is a backstop against a hung runner).
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.saturating_add(5_000));
    let status = match wait_with_deadline(&mut child, deadline) {
        Ok(Some(status)) => status,
        Ok(None) => {
            let _ = std::fs::remove_file(&renderer_path);
            return Err(WebEngineError::PageLoadFailed {
                url: url.to_owned(),
                reason: "render timed out".to_owned(),
            });
        }
        Err(e) => {
            let _ = std::fs::remove_file(&renderer_path);
            return Err(WebEngineError::InitFailed(e.to_string()));
        }
    };
    let _ = std::fs::remove_file(&renderer_path);

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        return Err(match code {
            2 => WebEngineError::PageLoadFailed {
                url: url.to_owned(),
                reason: "page reported a load failure".to_owned(),
            },
            3 => WebEngineError::CaptureFailed("grabToImage could not save the PNG".to_owned()),
            4 => WebEngineError::PageLoadFailed {
                url: url.to_owned(),
                reason: "render timed out".to_owned(),
            },
            other => WebEngineError::InitFailed(format!("renderer exited with code {other}")),
        });
    }

    if output_png.is_file() {
        Ok(output_png)
    } else {
        Err(WebEngineError::CaptureFailed(
            "renderer exited successfully but no PNG was produced".to_owned(),
        ))
    }
}

/// Delete all Qt WebEngine profile data under `profile_storage_dir`.
pub fn clear_profile(options: &WebEngineOptions) -> Result<(), WebEngineError> {
    let dir = &options.profile_storage_dir;
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| WebEngineError::InitFailed(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_profile_is_idempotent_when_dir_missing() {
        let opts = WebEngineOptions {
            profile_storage_dir: std::env::temp_dir().join("linsync-test-nonexistent-profile-dir"),
            ..Default::default()
        };
        clear_profile(&opts).unwrap();
    }

    #[test]
    fn active_renderer_kind_is_one_of_known_values() {
        // Host-dependent (depends on which binaries are on PATH), so assert
        // membership in the contract set rather than a fixed value. This test
        // does not set LINSYNC_WEB_RENDERER — env vars are process-global and
        // the suite runs in parallel.
        let kind = active_renderer_kind();
        assert!(
            ["qml", "chromium", "none"].contains(&kind),
            "active_renderer_kind must be qml | chromium | none, got: {kind}"
        );
        // Repeated resolution is cached and stable for a fixed environment.
        assert_eq!(kind, active_renderer_kind());
    }

    #[test]
    fn qml_escape_quotes_and_backslashes() {
        assert_eq!(qml_escape(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn qml_escape_control_characters() {
        assert_eq!(
            qml_escape("a\nb\rc\td\0e"),
            r"a\nb\rc\td\u0000e",
            "newline, carriage return, tab, and NUL must all be escaped"
        );
    }

    /// Live chromium-headless screenshot of a local HTML file. Self-skips
    /// (with a stderr note) on hosts without a Chromium binary, so it runs for
    /// real wherever one is installed.
    #[test]
    fn chromium_backend_screenshots_local_file() {
        let Some(chromium) = resolve_chromium_binary() else {
            eprintln!("skip: no chromium binary on PATH");
            return;
        };
        let tmp = tempfile::tempdir().unwrap();
        let html = tmp.path().join("page.html");
        std::fs::write(
            &html,
            "<!doctype html><html><body style='background:#0a0'>hi</body></html>",
        )
        .unwrap();
        let url = format!("file://{}", html.display());
        let out_dir = tmp.path().join("out");
        let options = WebEngineOptions {
            profile_storage_dir: tmp.path().join("profile"),
            ..Default::default()
        };
        let png = render_url_chromium(&chromium, &url, &out_dir, &options)
            .expect("chromium render should produce a PNG");
        let bytes = std::fs::read(&png).unwrap();
        assert!(bytes.len() > 100, "PNG should be non-trivial");
        assert_eq!(&bytes[1..4], b"PNG", "output is a PNG");
    }

    /// Live render of a local HTML page to PNG. Ignored by default because it
    /// requires `qml6` + qt6-webengine and an offscreen GL context; run with
    /// `--ignored` where those are present.
    #[test]
    #[ignore = "requires qml6 + qt6-webengine and an offscreen GL context"]
    fn render_local_html_produces_png() {
        let tmp = tempfile::tempdir().unwrap();
        let html = tmp.path().join("page.html");
        std::fs::write(
            &html,
            "<!doctype html><html><body style='background:#0a0'>hi</body></html>",
        )
        .unwrap();
        let url = format!("file://{}", html.display());
        let out_dir = tmp.path().join("out");
        let png = render_url(&url, &out_dir, &WebEngineOptions::default())
            .expect("render should produce a PNG");
        let bytes = std::fs::read(&png).unwrap();
        assert!(bytes.len() > 100, "PNG should be non-trivial");
        assert_eq!(&bytes[1..4], b"PNG", "output is a PNG");
    }
}
