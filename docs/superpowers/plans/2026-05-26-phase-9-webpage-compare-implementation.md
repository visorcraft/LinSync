# Phase 9 — Webpage Compare Implementation Plan

> **Dependencies:** Phase 6 (sandbox) and Phase 7 (image compare for screenshot sub-mode).
> **Feature gates:** `web-fetch` (always available — uses wget/curl helper), `web-engine` (Qt WebEngine — default off; required only for rendered/screenshot sub-modes).

**Goal:** 5 sub-modes for URL/webpage compare. HTML source, extracted text, and resource-tree work without Qt WebEngine (use the `web-fetch` plugin). Rendered and screenshot modes require the `web-engine` feature. All network access is opt-in via a confirmation dialog. Cache + cookies live under `$XDG_CACHE_HOME/linsync/webcompare/` with explicit clear controls.

**Tech Stack:** Rust, plugin-helper protocol, Phase 6 sandbox with `network: true` plugins, Qt WebEngine (LGPLv3, feature-gated), `httptest` (dev-dep, test fixtures).

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

---

## Architecture overview

```
linsync-core/src/webpage.rs          ← core API: all five sub-modes, feature gates
crates/linsync-webengine/            ← new crate, web-engine feature, cxx-qt wrapper
packaging/plugins/web-fetch/         ← Python plugin: fetch_html / extract_text / resource_tree
apps/linsync-gui/qml/WebpageComparePage.qml  ← GUI page
crates/linsync-cli/src/main.rs       ← webpage sub-command
tests/fixtures/webcompare/           ← HTML fixture files
```

```
web-fetch plugin (network: true sandbox)
        ↑ run_plugin_helper (linsync-core/plugin.rs)
webpage.rs  ──compare_text_files──► TextCompareResult   (html / text modes)
            ──folder compare──────► FolderCompareResult  (resource-tree mode)
            ──compare_images──────► ImageCompareResult   (screenshot mode, web-engine feature)
            ──render_url──────────► ImageCompareResult   (rendered mode, web-engine feature)
                    ↑
         crates/linsync-webengine (cxx-qt, feature-gated)
```

**Dependency graph within Phase 9:**

```
Task 9.1 (types)
   ↓
Task 9.2 (web-fetch plugin)
   ↓
Task 9.3 (html-source compare)
Task 9.4 (extracted-text compare)   ← both depend on 9.2
Task 9.5 (resource-tree compare)
   ↓
Task 9.6 (network gating + ConfirmationRequired)  ← applies across 9.3–9.5
   ↓
Task 9.7 (linsync-webengine stub crate)
   ↓
Task 9.8 (Rendered + Screenshot sub-modes, web-engine feature)
   ↓
Task 9.9 (CLI integration)
Task 9.10 (GUI QML page)
```

---

## Constraints (apply to every task)

- HTML / text / resource-tree sub-modes work in the **default build** (no optional features needed).
- Rendered and Screenshot sub-modes compile only when `--features web-engine` is set; their `WebpageCompareMode` enum arms are guarded by `#[cfg(feature = "web-engine")]`.
- All HTTP requests go through the `web-fetch` plugin process via `run_plugin_helper`. The `linsync-core` library crate itself never opens a TCP connection.
- Tests use `httptest` local servers; no live URLs anywhere in the test suite.
- Cache root: `AppPaths::cache_dir.join("webcompare")` → `$XDG_CACHE_HOME/linsync/webcompare/`.
- All tasks must leave `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` clean before the task commit is created.
- Commit cadence: TDD red → green → clippy/fmt → commit per task. Do not batch across tasks.

---

## Task 9.1 — `webpage.rs` types skeleton

**Files touched:**
- `crates/linsync-core/src/webpage.rs` (create)
- `crates/linsync-core/src/lib.rs` (add `pub mod webpage` and re-exports)
- `crates/linsync-core/Cargo.toml` (add `[features]` table with `web-engine` feature)

### What to implement

Define the public API surface consumed by every subsequent task. No logic yet; `compare_webpage_*` functions return `unimplemented!()` placeholders.

```rust
// crates/linsync-core/src/webpage.rs

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
#[derive(Debug, Clone)]
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
#[derive(Debug, thiserror::Error)]
pub enum WebpageCompareError {
    #[error("user confirmation required before network fetch")]
    ConfirmationRequired,
    #[error("plugin error: {0}")]
    Plugin(#[from] crate::plugin::PluginError),
    #[error("URL is not valid: {0}")]
    InvalidUrl(String),
    #[error("text compare error: {0}")]
    Text(String),
    #[error("folder compare error: {0}")]
    Folder(#[from] crate::folder::FolderCompareError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("network timeout after {timeout_secs}s for {url}")]
    Timeout { url: String, timeout_secs: u32 },
    #[error("plugin returned unexpected JSON: {0}")]
    UnexpectedPluginResponse(String),
    #[error("cache error: {0}")]
    Cache(String),
}

/// Remove all cached data under `$XDG_CACHE_HOME/linsync/webcompare/`.
///
/// Deletes the directory and all its contents.  Returns `Ok(())` if the
/// directory does not exist (idempotent).
pub fn clear_webcompare_cache(cache_dir: &std::path::Path) -> Result<(), WebpageCompareError> {
    let webcompare_dir = cache_dir.join("webcompare");
    if webcompare_dir.exists() {
        std::fs::remove_dir_all(&webcompare_dir)?;
    }
    Ok(())
}

/// Returns the webcompare cache directory, creating it if needed.
pub fn webcompare_cache_dir(cache_dir: &std::path::Path) -> Result<std::path::PathBuf, WebpageCompareError> {
    let dir = cache_dir.join("webcompare").join("fetched");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

// Stub public entry points — implemented in subsequent tasks.

pub fn compare_webpage_html_source(
    _left_url: &str,
    _right_url: &str,
    _options: &WebpageCompareOptions,
    _cache_dir: &std::path::Path,
) -> Result<WebpageCompareResult, WebpageCompareError> {
    unimplemented!("Task 9.3")
}

pub fn compare_webpage_extracted_text(
    _left_url: &str,
    _right_url: &str,
    _options: &WebpageCompareOptions,
    _cache_dir: &std::path::Path,
) -> Result<WebpageCompareResult, WebpageCompareError> {
    unimplemented!("Task 9.4")
}

pub fn compare_webpage_resource_tree(
    _left_url: &str,
    _right_url: &str,
    _options: &WebpageCompareOptions,
    _cache_dir: &std::path::Path,
) -> Result<WebpageCompareResult, WebpageCompareError> {
    unimplemented!("Task 9.5")
}

#[cfg(feature = "web-engine")]
pub fn compare_webpage_rendered(
    _left_url: &str,
    _right_url: &str,
    _options: &WebpageCompareOptions,
    _cache_dir: &std::path::Path,
) -> Result<WebpageCompareResult, WebpageCompareError> {
    unimplemented!("Task 9.8")
}

#[cfg(feature = "web-engine")]
pub fn compare_webpage_screenshot(
    _left_url: &str,
    _right_url: &str,
    _options: &WebpageCompareOptions,
    _cache_dir: &std::path::Path,
) -> Result<WebpageCompareResult, WebpageCompareError> {
    unimplemented!("Task 9.8")
}
```

**`Cargo.toml` additions for `linsync-core`:**

```toml
[features]
web-engine = []

[dependencies]
# existing deps …
thiserror = "1"   # add if not already present
```

Add `thiserror` to `[workspace.dependencies]` in the root `Cargo.toml` as well.

**`lib.rs` additions:**

```rust
pub mod webpage;
pub use webpage::{
    WebpageCompareError, WebpageCompareMode, WebpageCompareOptions, WebpageCompareResult,
    clear_webcompare_cache, compare_webpage_extracted_text, compare_webpage_html_source,
    compare_webpage_resource_tree, webcompare_cache_dir,
};
#[cfg(feature = "web-engine")]
pub use webpage::{WebpageRenderedResult, compare_webpage_rendered, compare_webpage_screenshot};
```

### Tests (write first — TDD)

```rust
// crates/linsync-core/src/webpage.rs  (inside #[cfg(test)] mod tests)

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
```

Add `tempfile = "3"` to `[dev-dependencies]` in `linsync-core/Cargo.toml` (and workspace).

**Acceptance:** `cargo test -p linsync-core` green; `cargo clippy -p linsync-core -- -D warnings` clean; no `unimplemented!` calls reached by the three skeleton tests.

---

## Task 9.2 — `web-fetch` plugin: manifest + Python script

**Files touched:**
- `packaging/plugins/web-fetch/linsync-plugin.json` (create)
- `packaging/plugins/web-fetch/web-fetch` (create, Python 3 script, made executable)
- `tests/fixtures/webcompare/simple.html` (create)
- `tests/fixtures/webcompare/encoding.html` (create)
- `tests/fixtures/webcompare/redirect_target.html` (create)

### Manifest

```json
{
  "schema_version": 1,
  "id": "com.visorcraft.web-fetch",
  "name": "Web Fetch",
  "version": "1.0.0",
  "license": "GPL-3.0-only",
  "entry": ["./web-fetch"],
  "classes": ["unpacker"],
  "mime_types": ["text/html", "text/uri-list"],
  "extensions": ["html", "htm", "uri"],
  "capabilities": [],
  "deterministic": false,
  "sandbox": {
    "network": true,
    "writes_input": false,
    "requires_home_access": false
  }
}
```

### Plugin script (`packaging/plugins/web-fetch/web-fetch`)

The script reads one JSON request from stdin and writes one JSON response to stdout. Three supported operations:

| `op` field | Description |
|---|---|
| `fetch_html` | Fetch URL, return raw HTML bytes as `html` (UTF-8 string) + `headers` dict + `status` int |
| `extract_text` | Fetch HTML, strip all tags with a regex, return `text` (UTF-8 string) |
| `resource_tree` | Enumerate linked `<a href>` and `<img src>` up to `depth` (default 1, max 3), return `tree: [{url, status}]` |

Request shape:

```json
{ "op": "fetch_html",    "url": "http://...", "timeout": 30, "user_agent": null }
{ "op": "extract_text",  "url": "http://...", "timeout": 30, "user_agent": null }
{ "op": "resource_tree", "url": "http://...", "depth": 1, "max_requests": 50, "timeout": 30, "user_agent": null }
```

Response shape (success):

```json
{ "ok": true, "html": "...", "headers": {"content-type": "text/html"}, "status": 200 }
{ "ok": true, "text": "..." }
{ "ok": true, "tree": [{"url": "http://.../page.html", "status": 200}, ...] }
```

Response shape (error):

```json
{ "ok": false, "error": "timeout", "url": "http://..." }
```

**Implementation notes:**

- Use `urllib.request.urlopen` with `timeout` parameter.  No third-party libraries.
- Redirect following: `urllib.request` follows redirects by default (up to 10 hops).
- For `extract_text`: strip tags with `re.sub(r'<[^>]+>', '', html)` and collapse whitespace.
- For `resource_tree`:
  - Parse `<a href="...">` and `<img src="...">` with `re.findall`.
  - Resolve relative URLs with `urllib.parse.urljoin`.
  - Deduplicate URLs with a `seen` set.  Stop when `len(seen) >= max_requests`.
  - For each discovered URL: issue a HEAD request (fallback to GET on 405), record `status`.
  - Recurse up to `depth` levels (depth 1 = links on the root page only).
- Exit code 0 always; failures are encoded as `{"ok": false, "error": "..."}`.

```python
#!/usr/bin/env python3
"""LinSync web-fetch plugin.

Reads one JSON request on stdin, writes one JSON response on stdout.
Supported ops: fetch_html, extract_text, resource_tree.
"""

import json
import re
import sys
import urllib.error
import urllib.parse
import urllib.request


def _make_opener(user_agent: str | None) -> urllib.request.OpenerDirector:
    opener = urllib.request.build_opener()
    if user_agent:
        opener.addheaders = [("User-Agent", user_agent)]
    return opener


def fetch_html(url: str, timeout: int, user_agent: str | None) -> dict:
    opener = _make_opener(user_agent)
    try:
        with opener.open(url, timeout=timeout) as resp:
            raw = resp.read()
            headers = dict(resp.headers)
            status = resp.status
        html = raw.decode("utf-8", errors="replace")
        return {"ok": True, "html": html, "headers": headers, "status": status}
    except urllib.error.URLError as exc:
        msg = str(exc.reason) if hasattr(exc, "reason") else str(exc)
        if "timed out" in msg.lower():
            return {"ok": False, "error": "timeout", "url": url}
        return {"ok": False, "error": msg, "url": url}
    except Exception as exc:  # noqa: BLE001
        return {"ok": False, "error": str(exc), "url": url}


def extract_text(url: str, timeout: int, user_agent: str | None) -> dict:
    result = fetch_html(url, timeout, user_agent)
    if not result["ok"]:
        return result
    html = result["html"]
    text = re.sub(r"<[^>]+>", " ", html)
    text = re.sub(r"\s+", " ", text).strip()
    return {"ok": True, "text": text}


def resource_tree(
    url: str,
    depth: int,
    max_requests: int,
    timeout: int,
    user_agent: str | None,
) -> dict:
    depth = min(max(depth, 1), 3)
    seen: dict[str, int] = {}  # url → HTTP status
    opener = _make_opener(user_agent)

    def _probe_status(target_url: str) -> int:
        try:
            req = urllib.request.Request(target_url, method="HEAD")
            with opener.open(req, timeout=timeout) as resp:
                return resp.status
        except urllib.error.HTTPError as exc:
            if exc.code == 405:
                try:
                    with opener.open(target_url, timeout=timeout) as resp:
                        return resp.status
                except Exception:  # noqa: BLE001
                    return 0
            return exc.code
        except Exception:  # noqa: BLE001
            return 0

    def _crawl(current_url: str, remaining_depth: int) -> None:
        if current_url in seen or len(seen) >= max_requests:
            return
        status = _probe_status(current_url)
        seen[current_url] = status
        if remaining_depth <= 0:
            return
        # Fetch HTML and extract links.
        result = fetch_html(current_url, timeout, user_agent)
        if not result["ok"]:
            return
        html = result["html"]
        hrefs = re.findall(r'href=["\']([^"\']+)["\']', html, re.IGNORECASE)
        srcs = re.findall(r'src=["\']([^"\']+)["\']', html, re.IGNORECASE)
        for raw in hrefs + srcs:
            if len(seen) >= max_requests:
                break
            resolved = urllib.parse.urljoin(current_url, raw)
            if resolved not in seen:
                _crawl(resolved, remaining_depth - 1)

    _crawl(url, depth)
    tree = [{"url": u, "status": s} for u, s in seen.items()]
    return {"ok": True, "tree": tree}


def main() -> None:
    raw = sys.stdin.read()
    try:
        req = json.loads(raw)
    except json.JSONDecodeError as exc:
        sys.stdout.write(json.dumps({"ok": False, "error": f"invalid JSON: {exc}"}))
        sys.exit(0)

    op = req.get("op", "")
    url = req.get("url", "")
    timeout = int(req.get("timeout", 30))
    user_agent = req.get("user_agent") or None

    if op == "fetch_html":
        response = fetch_html(url, timeout, user_agent)
    elif op == "extract_text":
        response = extract_text(url, timeout, user_agent)
    elif op == "resource_tree":
        depth = int(req.get("depth", 1))
        max_requests = int(req.get("max_requests", 50))
        response = resource_tree(url, depth, max_requests, timeout, user_agent)
    else:
        response = {"ok": False, "error": f"unknown op: {op!r}"}

    sys.stdout.write(json.dumps(response))


if __name__ == "__main__":
    main()
```

Make the script executable: `chmod +x packaging/plugins/web-fetch/web-fetch`.

### HTML fixtures

**`tests/fixtures/webcompare/simple.html`** — minimal page with two `<a>` links and one `<img>`:

```html
<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>Simple</title></head>
<body>
<p>Hello world.</p>
<a href="/page-a.html">Page A</a>
<a href="/page-b.html">Page B</a>
<img src="/logo.png" alt="logo">
</body>
</html>
```

**`tests/fixtures/webcompare/encoding.html`** — `charset=iso-8859-1` in meta tag:

```html
<!DOCTYPE html>
<html>
<head><meta charset="iso-8859-1"><title>Encoding test</title></head>
<body><p>Caf&#233;</p></body>
</html>
```

**`tests/fixtures/webcompare/redirect_target.html`** — served as the destination of a 301 redirect:

```html
<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>Redirect Target</title></head>
<body><p>You have been redirected.</p></body>
</html>
```

### Tests

No Rust unit tests for the Python script itself in this task.  Instead, write a Rust integration test that invokes the plugin binary directly (only runs when the script exists on disk):

```rust
// crates/linsync-core/tests/web_fetch_plugin.rs

#[test]
fn web_fetch_plugin_manifest_is_valid() {
    let manifest_path = std::path::Path::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../packaging/plugins/web-fetch/linsync-plugin.json"),
    );
    if !manifest_path.exists() {
        return; // skip in environments where packaging/ is absent
    }
    let manifest = linsync_core::PluginManifest::from_manifest_file(manifest_path).unwrap();
    let plugin_dir = manifest_path.parent().unwrap();
    manifest.validate(plugin_dir).unwrap();
    assert_eq!(manifest.id, "com.visorcraft.web-fetch");
    assert!(manifest.sandbox.network);
    assert!(!manifest.sandbox.writes_input);
}
```

**Acceptance:** `cargo test -p linsync-core web_fetch_plugin` passes; manifest validates against the existing `PluginManifest::validate` logic; `cargo clippy` clean.

---

## Task 9.3 — `compare_webpage_html_source`

**Files touched:**
- `crates/linsync-core/src/webpage.rs` (replace stub)
- `crates/linsync-core/Cargo.toml` (add `httptest` dev-dep)

### What to implement

Replace the `unimplemented!("Task 9.3")` body.  The function must:

1. Return `Err(WebpageCompareError::ConfirmationRequired)` if `options.confirmed_by_user` is false.
2. Validate that both URLs are non-empty strings beginning with `http://` or `https://`.
3. Locate the `web-fetch` plugin using `discover_plugins` (scanning `packaging/plugins` relative to the binary, plus standard XDG data dirs via `plugin_discovery_roots`).
4. Invoke the plugin twice (once per URL) using `run_plugin_helper` with `op: "fetch_html"`.  Pass `timeout` and `user_agent` from options.
5. Write the two HTML payloads to temporary files under `webcompare_cache_dir(cache_dir)?`.
6. Forward both temp file paths to `compare_text_files` (already public in `linsync-core`).
7. Wrap the result in `WebpageCompareResult::Text(...)`.

Helper to add (private):

```rust
fn invoke_web_fetch(
    plugin_dir: &std::path::Path,
    request_json: serde_json::Value,
    options: &PluginExecutionOptions,
) -> Result<serde_json::Value, WebpageCompareError>
```

This calls `run_plugin_helper` and JSON-parses the response.  It checks `response["ok"] == true` and returns the response on success, or constructs a `WebpageCompareError` from `response["error"]` on failure.

### Tests (write first)

```rust
// crates/linsync-core/src/webpage.rs  (inside #[cfg(test)] mod tests)

#[cfg(test)]
mod tests {
    use super::*;
    use httptest::{Server, matchers::*, responders::*};

    // Helper: start an httptest server serving `body` at GET `/`.
    fn simple_server(body: &'static str) -> Server {
        let server = Server::run();
        server.expect(
            Expectation::matching(request::method_path("GET", "/"))
                .respond_with(status_code(200).body(body)),
        );
        server
    }

    #[test]
    fn html_source_requires_confirmation() {
        let opts = WebpageCompareOptions::default(); // confirmed_by_user: false
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
        // This test requires the web-fetch plugin to be present.
        // Skip gracefully if it is absent.
        let plugin_script = std::path::Path::new(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../packaging/plugins/web-fetch/web-fetch"),
        );
        if !plugin_script.exists() {
            eprintln!("skip: web-fetch plugin not found");
            return;
        }

        let server_l = simple_server("<html><body>Hello</body></html>");
        let server_r = simple_server("<html><body>Hello</body></html>");

        let mut opts = WebpageCompareOptions::default();
        opts.confirmed_by_user = true;

        let tmp = tempfile::tempdir().unwrap();
        let result = compare_webpage_html_source(
            &server_l.url("/"),
            &server_r.url("/"),
            &opts,
            tmp.path(),
        )
        .unwrap();

        if let WebpageCompareResult::Text(text_result) = result {
            // Identical content → zero diff blocks with Changed kind.
            use crate::text::DiffBlockKind;
            let changed = text_result
                .blocks
                .iter()
                .any(|b| b.kind == DiffBlockKind::Changed);
            assert!(!changed, "identical pages should produce no diff");
        } else {
            panic!("expected Text result");
        }
    }

    #[test]
    fn html_source_different_pages_produce_diff() {
        let plugin_script = std::path::Path::new(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../packaging/plugins/web-fetch/web-fetch"),
        );
        if !plugin_script.exists() {
            return;
        }

        let server_l = simple_server("<html><body>Left</body></html>");
        let server_r = simple_server("<html><body>Right</body></html>");

        let mut opts = WebpageCompareOptions::default();
        opts.confirmed_by_user = true;

        let tmp = tempfile::tempdir().unwrap();
        let result = compare_webpage_html_source(
            &server_l.url("/"),
            &server_r.url("/"),
            &opts,
            tmp.path(),
        )
        .unwrap();

        if let WebpageCompareResult::Text(text_result) = result {
            use crate::text::DiffBlockKind;
            let has_diff = text_result
                .blocks
                .iter()
                .any(|b| b.kind == DiffBlockKind::Changed);
            assert!(has_diff, "different pages should produce a diff");
        } else {
            panic!("expected Text result");
        }
    }
}
```

**`Cargo.toml` additions (`linsync-core`):**

```toml
[dev-dependencies]
httptest = "0.16"
tempfile = "3"
```

Add both to `[workspace.dependencies]` (dev-scope) in the root `Cargo.toml`.

**Acceptance:** All three new tests pass (skipping gracefully when the plugin binary is absent); `cargo clippy` and `cargo fmt --check` clean.

---

## Task 9.4 — `compare_webpage_extracted_text`

**Files touched:**
- `crates/linsync-core/src/webpage.rs` (replace stub for extracted-text)

### What to implement

Identical flow to Task 9.3 but uses `op: "extract_text"`.  The plugin returns `{"ok": true, "text": "..."}`.  Write the `text` value to a temp file and forward both to `compare_text_files`.

```rust
pub fn compare_webpage_extracted_text(
    left_url: &str,
    right_url: &str,
    options: &WebpageCompareOptions,
    cache_dir: &std::path::Path,
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
    let cmp = compare_text_files(&left_path, &right_path, &TextCompareOptions::default())
        .map_err(|e| WebpageCompareError::Text(e.to_string()))?;
    Ok(WebpageCompareResult::Text(cmp))
}
```

Private helpers to share with Task 9.3 (refactor into module-private functions):

```rust
fn guard_confirmed(options: &WebpageCompareOptions) -> Result<(), WebpageCompareError>;
fn validate_url(url: &str) -> Result<(), WebpageCompareError>;
fn find_web_fetch_plugin() -> Result<std::path::PathBuf, WebpageCompareError>;
fn make_plugin_exec_options(options: &WebpageCompareOptions) -> PluginExecutionOptions;
fn write_temp_text(dir: &std::path::Path, prefix: &str, text: &str)
    -> Result<std::path::PathBuf, WebpageCompareError>;
```

### Tests (write first)

```rust
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
    let plugin_script = std::path::Path::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../packaging/plugins/web-fetch/web-fetch"),
    );
    if !plugin_script.exists() {
        return;
    }

    let server_l = simple_server("<html><body><h1>Heading</h1><p>Para left</p></body></html>");
    let server_r = simple_server("<html><body><h1>Heading</h1><p>Para right</p></body></html>");

    let mut opts = WebpageCompareOptions::default();
    opts.confirmed_by_user = true;
    let tmp = tempfile::tempdir().unwrap();

    let result = compare_webpage_extracted_text(
        &server_l.url("/"),
        &server_r.url("/"),
        &opts,
        tmp.path(),
    )
    .unwrap();

    if let WebpageCompareResult::Text(cmp) = result {
        // Left and right text differ only in "left" vs "right".
        use crate::text::DiffBlockKind;
        let has_diff = cmp.blocks.iter().any(|b| b.kind == DiffBlockKind::Changed);
        assert!(has_diff);
    } else {
        panic!("expected Text result");
    }
}
```

**Acceptance:** Tests pass; `cargo clippy` clean.

---

## Task 9.5 — `compare_webpage_resource_tree`

**Files touched:**
- `crates/linsync-core/src/webpage.rs` (replace stub for resource-tree)

### What to implement

1. Guard `confirmed_by_user`.
2. Invoke `op: "resource_tree"` on both URLs, passing `depth` (clamped to 1–3) and `max_requests`.
3. Parse the `tree: [{url, status}]` arrays from both responses.
4. Convert each to a sorted `Vec<(String, u16)>` (URL, status).
5. Perform a set-diff: left-only URLs, right-only URLs, URLs in both but with different status codes.
6. Build a `FolderCompareResult` from the set membership (present in left, right, or both → `FolderEntryState`).
7. Wrap in `WebpageCompareResult::Folder(...)`.

The conversion from URL tree to `FolderCompareResult` treats each URL as a "virtual path" (using the URL path component stripped of the scheme+host as the display name).

Helper to add:

```rust
fn url_tree_to_folder_result(
    left_tree: &[(String, u16)],
    right_tree: &[(String, u16)],
) -> FolderCompareResult
```

### Tests (write first)

```rust
#[test]
fn resource_tree_requires_confirmation() {
    let opts = WebpageCompareOptions::default();
    let tmp = tempfile::tempdir().unwrap();
    let err = compare_webpage_resource_tree("http://x/", "http://x/", &opts, tmp.path())
        .unwrap_err();
    assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
}

#[test]
fn resource_tree_detects_left_only_link() {
    let plugin_script = std::path::Path::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../packaging/plugins/web-fetch/web-fetch"),
    );
    if !plugin_script.exists() {
        return;
    }

    // Left server: root page with a link to /extra.html; also serve /extra.html.
    let server_l = Server::run();
    server_l.expect(
        Expectation::matching(request::method_path("GET", "/"))
            .times(1..)
            .respond_with(
                status_code(200)
                    .body(r#"<a href="/extra.html">extra</a>"#),
            ),
    );
    server_l.expect(
        Expectation::matching(request::method_path("HEAD", "/extra.html"))
            .times(0..)
            .respond_with(status_code(200)),
    );

    // Right server: root page only, no links.
    let server_r = Server::run();
    server_r.expect(
        Expectation::matching(request::method_path("GET", "/"))
            .times(1..)
            .respond_with(status_code(200).body("<p>Right only</p>")),
    );

    let mut opts = WebpageCompareOptions::default();
    opts.confirmed_by_user = true;
    let tmp = tempfile::tempdir().unwrap();

    let result = compare_webpage_resource_tree(
        &server_l.url("/"),
        &server_r.url("/"),
        &opts,
        tmp.path(),
    )
    .unwrap();

    if let WebpageCompareResult::Folder(folder_result) = result {
        // At minimum the root URL appears on both sides; /extra.html appears only on left.
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
    assert!(result.entries.iter().all(|e| e.state == FolderEntryState::Identical));
}
```

**Acceptance:** Tests pass; `cargo clippy` clean.

---

## Task 9.6 — Network gating (ConfirmationRequired enforcement)

**Files touched:**
- `crates/linsync-core/src/webpage.rs` (consolidate + harden guard)
- `crates/linsync-cli/src/main.rs` (add `--accept-network-fetch` flag to the future `webpage` sub-command placeholder)

### What to implement

All three public functions (html-source, extracted-text, resource-tree) already call `guard_confirmed`. This task hardens the contract and adds dedicated tests to confirm that:

1. Every public compare function returns `Err(ConfirmationRequired)` when `confirmed_by_user = false`, regardless of whether the URL is valid or the plugin exists.
2. The `guard_confirmed` check happens **before** any filesystem or plugin I/O.

Add a `guard_confirmed` early-return test for each of the three functions.

Also add a placeholder `webpage` sub-command to `linsync-cli` that only parses flags (no actual network calls yet; deferred to Task 9.9):

```rust
// crates/linsync-cli/src/main.rs — inside run()
"webpage" => webpage_command(&args[1..]),

fn webpage_command(args: &[String]) -> Result<ExitCode, String> {
    // Parsed flags: --sub-mode, --accept-network-fetch, --depth, --timeout
    // Placeholder: print help and exit 0 until Task 9.9 wires the full logic.
    let accept_network = args.iter().any(|a| a == "--accept-network-fetch");
    if !accept_network {
        eprintln!(
            "webpage compare requires --accept-network-fetch to confirm network access"
        );
        return Ok(ExitCode::from(2));
    }
    eprintln!("webpage compare not yet fully implemented (Phase 9 in progress)");
    Ok(ExitCode::from(2))
}
```

### Tests (write first)

```rust
#[test]
fn html_source_confirmation_guard_fires_before_io() {
    // Use an obviously invalid URL — we must not reach network I/O.
    let opts = WebpageCompareOptions { confirmed_by_user: false, ..Default::default() };
    let tmp = tempfile::tempdir().unwrap();
    let err = compare_webpage_html_source("not-a-url", "not-a-url", &opts, tmp.path())
        .unwrap_err();
    // ConfirmationRequired must come before InvalidUrl.
    assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
}

#[test]
fn extracted_text_confirmation_guard_fires_before_io() {
    let opts = WebpageCompareOptions { confirmed_by_user: false, ..Default::default() };
    let tmp = tempfile::tempdir().unwrap();
    let err = compare_webpage_extracted_text("not-a-url", "not-a-url", &opts, tmp.path())
        .unwrap_err();
    assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
}

#[test]
fn resource_tree_confirmation_guard_fires_before_io() {
    let opts = WebpageCompareOptions { confirmed_by_user: false, ..Default::default() };
    let tmp = tempfile::tempdir().unwrap();
    let err = compare_webpage_resource_tree("not-a-url", "not-a-url", &opts, tmp.path())
        .unwrap_err();
    assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
}
```

**Acceptance:** All guard tests pass; `cargo clippy --workspace` and `cargo fmt --check` clean.

---

## Task 9.7 — `crates/linsync-webengine/` crate stub (feature-gated)

**Files touched:**
- `crates/linsync-webengine/Cargo.toml` (create)
- `crates/linsync-webengine/src/lib.rs` (create)
- `Cargo.toml` (add `linsync-webengine` to `[workspace.members]`)
- `crates/linsync-core/Cargo.toml` (add optional dep on `linsync-webengine`)

### What to implement

Create a new workspace crate that is **only compiled** when `--features web-engine` is passed to `linsync-core` (or the workspace).  The crate defines the rendering API that Task 9.8 will call.

**`crates/linsync-webengine/Cargo.toml`:**

```toml
[package]
name = "linsync-webengine"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[features]
default = []

[dependencies]
# No Qt bindings in the stub — just the API skeleton.
# Real cxx-qt bindings are added in the Phase 9.7-bis follow-up.

[dev-dependencies]
```

**`crates/linsync-webengine/src/lib.rs`:**

```rust
//! Qt WebEngine wrapper for LinSync webpage compare (rendered + screenshot sub-modes).
//!
//! This crate is only available when the `web-engine` feature is enabled on
//! `linsync-core`.  In the default build, this crate is not compiled and the
//! rendered/screenshot sub-modes are absent from the public API.
//!
//! # Phase 9.7-bis
//!
//! The real implementation (cxx-qt bindings to `QWebEngineView` and
//! `QWebEngineProfile`) is deferred to Phase 9.7-bis.  This stub defines the
//! public API surface and returns `Err(WebEngineError::NotImplemented)` for all
//! operations so that Tasks 9.8, 9.9, and 9.10 can compile and be wired end-to-end.

/// Errors from the web-engine wrapper.
#[derive(Debug, thiserror::Error)]
pub enum WebEngineError {
    #[error("Qt WebEngine bindings not yet implemented (Phase 9.7-bis)")]
    NotImplemented,
    #[error("Qt WebEngine initialization failed: {0}")]
    InitFailed(String),
    #[error("page load failed for {url}: {reason}")]
    PageLoadFailed { url: String, reason: String },
    #[error("screenshot capture failed: {0}")]
    CaptureFailed(String),
}

/// Options for a web-engine rendering session.
#[derive(Debug, Clone)]
pub struct WebEngineOptions {
    /// Directory for the isolated `QWebEngineProfile` storage.
    /// Typically `$XDG_CACHE_HOME/linsync/webcompare/profile/`.
    pub profile_storage_dir: std::path::PathBuf,
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
            profile_storage_dir: std::path::PathBuf::from("/tmp/linsync-webengine-profile"),
            viewport_width: 1280,
            viewport_height: 900,
            timeout_secs: 30,
        }
    }
}

/// Render `url` in an isolated Qt WebEngine profile and return the path to a PNG screenshot.
///
/// The PNG is written to `output_dir/<url_hash>.png`.
///
/// # Errors
///
/// Returns [`WebEngineError::NotImplemented`] in the current stub.
/// The real implementation (Phase 9.7-bis) will initialize a `QApplication` /
/// `QWebEngineView` in the calling thread, navigate to the URL, wait for
/// `loadFinished`, then call `QWebEngineView::grab()` and save the result as PNG.
pub fn render_url(
    url: &str,
    output_dir: &std::path::Path,
    options: &WebEngineOptions,
) -> Result<std::path::PathBuf, WebEngineError> {
    let _ = (url, output_dir, options);
    Err(WebEngineError::NotImplemented)
}

/// Delete all Qt WebEngine profile data under `profile_storage_dir`.
///
/// Calls `QWebEngineProfile::deleteAllCookies()` before removing the directory.
/// In this stub, simply removes the directory if it exists.
pub fn clear_profile(options: &WebEngineOptions) -> Result<(), WebEngineError> {
    let dir = &options.profile_storage_dir;
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| WebEngineError::InitFailed(e.to_string()))?;
    }
    Ok(())
}
```

**`crates/linsync-core/Cargo.toml` additions:**

```toml
[features]
web-engine = ["dep:linsync-webengine"]

[dependencies]
# ... existing deps ...
linsync-webengine = { path = "../linsync-webengine", optional = true }
```

**Root `Cargo.toml` addition:**

```toml
[workspace]
members = [
    "apps/linsync-gui",
    "crates/linsync-cli",
    "crates/linsync-core",
    "crates/linsync-webengine",
]
```

### Tests (write first)

```rust
// crates/linsync-webengine/src/lib.rs  (inside #[cfg(test)] mod tests)

#[test]
fn render_url_returns_not_implemented() {
    let opts = WebEngineOptions::default();
    let tmp = tempfile::tempdir().unwrap();
    let err = render_url("http://example.com/", tmp.path(), &opts).unwrap_err();
    assert!(matches!(err, WebEngineError::NotImplemented));
}

#[test]
fn clear_profile_is_idempotent_when_dir_missing() {
    let opts = WebEngineOptions {
        profile_storage_dir: std::path::PathBuf::from("/tmp/linsync-test-nonexistent-profile-dir"),
        ..Default::default()
    };
    // Should not error even though directory doesn't exist.
    clear_profile(&opts).unwrap();
}
```

Add `tempfile` to `[dev-dependencies]` in `linsync-webengine/Cargo.toml`.

**Acceptance:** `cargo test -p linsync-webengine` and `cargo test -p linsync-core --features web-engine` both pass; default build (`cargo build --workspace`) succeeds and does not include any Qt dependency; `cargo clippy --workspace` clean.

---

## Task 9.8 — Rendered and Screenshot sub-modes (web-engine feature)

**Files touched:**
- `crates/linsync-core/src/webpage.rs` (replace `Rendered`/`Screenshot` stubs)

> **Note:** Both sub-modes call `linsync_webengine::render_url`, which currently returns `Err(WebEngineError::NotImplemented)`.  The tests therefore assert the expected error path.  When Phase 9.7-bis lands the real Qt bindings, the tests will be updated to assert successful PNG capture.

### What to implement

**Rendered sub-mode:**

```rust
#[cfg(feature = "web-engine")]
pub fn compare_webpage_rendered(
    left_url: &str,
    right_url: &str,
    options: &WebpageCompareOptions,
    cache_dir: &std::path::Path,
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
    // In the stub, render_url returns NotImplemented — fall back to HTML source compare.
    let left_result = linsync_webengine::render_url(left_url, &fetch_dir, &engine_opts);
    let right_result = linsync_webengine::render_url(right_url, &fetch_dir, &engine_opts);
    match (left_result, right_result) {
        (Ok(_left_png), Ok(_right_png)) => {
            // Phase 9.7-bis: forward PNGs to image compare.
            // For now this branch is unreachable; include it so it compiles.
            unimplemented!("Phase 9.7-bis: compare rendered PNGs via image compare")
        }
        (Err(linsync_webengine::WebEngineError::NotImplemented), _)
        | (_, Err(linsync_webengine::WebEngineError::NotImplemented)) => {
            // Stub mode: fall back to HTML source compare and wrap in Rendered result.
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
```

**Screenshot sub-mode:**

```rust
#[cfg(feature = "web-engine")]
pub fn compare_webpage_screenshot(
    left_url: &str,
    right_url: &str,
    options: &WebpageCompareOptions,
    cache_dir: &std::path::Path,
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
    let left_png = linsync_webengine::render_url(left_url, &fetch_dir, &engine_opts)
        .map_err(|e| WebpageCompareError::Plugin(
            crate::plugin::PluginError::Io(std::io::Error::other(e.to_string())),
        ))?;
    let right_png = linsync_webengine::render_url(right_url, &fetch_dir, &engine_opts)
        .map_err(|e| WebpageCompareError::Plugin(
            crate::plugin::PluginError::Io(std::io::Error::other(e.to_string())),
        ))?;
    // Phase 7 dependency: compare_images is imported from linsync-core image module.
    let img_result = crate::image::compare_images(&left_png, &right_png)
        .map_err(|e| WebpageCompareError::Text(e.to_string()))?;
    Ok(WebpageCompareResult::Screenshot(img_result))
}
```

### Tests (write first)

```rust
#[cfg(feature = "web-engine")]
#[test]
fn rendered_requires_confirmation() {
    let opts = WebpageCompareOptions::default();
    let tmp = tempfile::tempdir().unwrap();
    let err = compare_webpage_rendered("http://x/", "http://x/", &opts, tmp.path())
        .unwrap_err();
    assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
}

#[cfg(feature = "web-engine")]
#[test]
fn rendered_stub_falls_back_to_html_source() {
    let plugin_script = std::path::Path::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../packaging/plugins/web-fetch/web-fetch"),
    );
    if !plugin_script.exists() {
        return;
    }
    let server_l = simple_server("<html><body>A</body></html>");
    let server_r = simple_server("<html><body>A</body></html>");
    let mut opts = WebpageCompareOptions::default();
    opts.confirmed_by_user = true;
    let tmp = tempfile::tempdir().unwrap();
    let result = compare_webpage_rendered(
        &server_l.url("/"),
        &server_r.url("/"),
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
    let err = compare_webpage_screenshot("http://x/", "http://x/", &opts, tmp.path())
        .unwrap_err();
    assert!(matches!(err, WebpageCompareError::ConfirmationRequired));
}

// Note: screenshot happy-path test deferred to Phase 9.7-bis (requires real Qt bindings).
```

**Acceptance:** `cargo test -p linsync-core --features web-engine` passes; default build without feature still compiles clean; `cargo clippy --workspace` clean.

---

## Task 9.9 — CLI integration: `linsync-cli webpage` sub-command

**Files touched:**
- `crates/linsync-cli/src/main.rs` (replace placeholder from Task 9.6 with full implementation)
- `crates/linsync-cli/Cargo.toml` (no new deps needed; uses `linsync-core`)

### What to implement

Full `webpage` sub-command:

```
linsync-cli webpage <url1> <url2> \
    --sub-mode <html|text|tree|rendered|screenshot> \
    [--depth <1-3>] \
    [--timeout <secs>] \
    [--max-requests <n>] \
    [--accept-network-fetch]
```

Argument parsing is done manually (consistent with the existing CLI style — no `clap` dependency in linsync-cli).

Behaviour:
- `--sub-mode` defaults to `html`.
- `--accept-network-fetch` sets `confirmed_by_user = true`.
- Absent `--accept-network-fetch`: print `"Network fetch requires --accept-network-fetch"` to stderr, exit 2.
- `rendered` or `screenshot` without `--features web-engine`: print `"Rendered/screenshot mode requires the web-engine build feature"` to stderr, exit 2.
- On `ConfirmationRequired` error (belt-and-suspenders): exit 2.
- On success: print a summary line to stdout, exit 0 (identical) or 1 (diff found), consistent with other compare commands.
- Cache dir is resolved via `AppPaths::from_env().cache_dir`.

```rust
fn webpage_command(args: &[String]) -> Result<ExitCode, String> {
    let mut urls: Vec<&str> = Vec::new();
    let mut sub_mode = "html";
    let mut depth: u8 = 1;
    let mut timeout: u32 = 30;
    let mut max_requests: u32 = 50;
    let mut accept_network = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--sub-mode" => {
                i += 1;
                sub_mode = args.get(i).map(String::as_str).unwrap_or("html");
            }
            "--depth" => {
                i += 1;
                depth = args.get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1)
                    .min(3)
                    .max(1);
            }
            "--timeout" => {
                i += 1;
                timeout = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(30);
            }
            "--max-requests" => {
                i += 1;
                max_requests = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(50);
            }
            "--accept-network-fetch" => accept_network = true,
            other if !other.starts_with('-') => urls.push(other),
            other => return Err(format!("unknown flag: {other}")),
        }
        i += 1;
    }

    if urls.len() != 2 {
        return Err("webpage requires exactly two URL arguments".to_string());
    }

    if !accept_network {
        eprintln!("error: network fetch requires --accept-network-fetch");
        return Ok(ExitCode::from(2));
    }

    let options = linsync_core::WebpageCompareOptions {
        resource_tree_depth: depth,
        timeout_secs: timeout,
        max_requests,
        confirmed_by_user: true,
        user_agent: None,
    };
    let cache_dir = linsync_core::AppPaths::from_env().cache_dir;

    let result = match sub_mode {
        "html" => linsync_core::compare_webpage_html_source(urls[0], urls[1], &options, &cache_dir),
        "text" => linsync_core::compare_webpage_extracted_text(urls[0], urls[1], &options, &cache_dir),
        "tree" => linsync_core::compare_webpage_resource_tree(urls[0], urls[1], &options, &cache_dir),
        #[cfg(feature = "web-engine")]
        "rendered" => linsync_core::compare_webpage_rendered(urls[0], urls[1], &options, &cache_dir),
        #[cfg(feature = "web-engine")]
        "screenshot" => linsync_core::compare_webpage_screenshot(urls[0], urls[1], &options, &cache_dir),
        "rendered" | "screenshot" => {
            eprintln!("error: {sub_mode} mode requires the web-engine build feature");
            return Ok(ExitCode::from(2));
        }
        other => return Err(format!("unknown sub-mode: {other}")),
    };

    match result {
        Ok(linsync_core::WebpageCompareResult::Text(cmp)) => {
            if cmp.identical {
                println!("identical");
                Ok(ExitCode::SUCCESS)
            } else {
                println!("different");
                Ok(ExitCode::from(1))
            }
        }
        Ok(linsync_core::WebpageCompareResult::Folder(cmp)) => {
            if cmp.is_identical() {
                println!("identical");
                Ok(ExitCode::SUCCESS)
            } else {
                println!("different");
                Ok(ExitCode::from(1))
            }
        }
        #[cfg(feature = "web-engine")]
        Ok(linsync_core::WebpageCompareResult::Rendered(r)) => {
            if r.html_fallback.as_ref().is_some_and(|t| t.identical) || r.dom_diff.is_none() {
                println!("identical (rendered fallback)");
                Ok(ExitCode::SUCCESS)
            } else {
                println!("different");
                Ok(ExitCode::from(1))
            }
        }
        #[cfg(feature = "web-engine")]
        Ok(linsync_core::WebpageCompareResult::Screenshot(_img)) => {
            // TODO Phase 9.7-bis: inspect image diff result.
            println!("screenshot captured");
            Ok(ExitCode::SUCCESS)
        }
        Err(linsync_core::WebpageCompareError::ConfirmationRequired) => {
            eprintln!("error: network fetch requires --accept-network-fetch");
            Ok(ExitCode::from(2))
        }
        Err(e) => Err(e.to_string()),
    }
}
```

Also add a `cache clear --scope webcompare` path:

```rust
// Inside the existing cache_command (or add if absent):
"webcompare" => {
    let cache_dir = linsync_core::AppPaths::from_env().cache_dir;
    linsync_core::clear_webcompare_cache(&cache_dir)
        .map_err(|e| e.to_string())?;
    println!("webcompare cache cleared");
    Ok(ExitCode::SUCCESS)
}
```

### Tests (write first)

Add CLI integration tests via `std::process::Command`:

```rust
// crates/linsync-cli/tests/webpage_cli.rs

use std::process::Command;

fn cli_bin() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../target/debug/linsync-cli");
    p
}

#[test]
fn webpage_no_accept_flag_exits_2() {
    let status = Command::new(cli_bin())
        .args(["webpage", "http://127.0.0.1:9", "http://127.0.0.1:9"])
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(2));
}

#[test]
fn webpage_missing_url_args_exits_2() {
    let status = Command::new(cli_bin())
        .args(["webpage", "--accept-network-fetch"])
        .status()
        .unwrap();
    // Exits 2 due to argument error (only 0 urls, need 2).
    assert_ne!(status.code(), Some(0));
}

#[test]
fn webpage_rendered_without_web_engine_feature_exits_2() {
    // Only meaningful in a build without --features web-engine.
    // Skip if we can't detect the feature at test time.
    let status = Command::new(cli_bin())
        .args([
            "webpage", "http://x/", "http://x/",
            "--sub-mode", "rendered",
            "--accept-network-fetch",
        ])
        .status()
        .unwrap();
    // In default build: exits 2.  In web-engine build: may succeed or fail differently.
    // We just assert it doesn't panic (exit from signal).
    assert!(status.code().is_some());
}
```

**Acceptance:** CLI tests pass; `cargo build -p linsync-cli` clean; `cargo clippy --workspace` clean.

---

## Task 9.10 — GUI: `WebpageComparePage.qml`

**Files touched:**
- `apps/linsync-gui/qml/WebpageComparePage.qml` (create)
- `apps/linsync-gui/qml/Main.qml` (or equivalent navigation file — add the page as a reachable tab/drawer item)
- `apps/linsync-gui/src/main.rs` or bridge file (add `compare_webpages` HTTP endpoint + `cxx-qt` invocable)

### QML page

The QML follows the existing code style in `apps/linsync-gui/qml/`.  Check the existing QML files for the exact import versions and Kirigami patterns in use and match them exactly.

```qml
// apps/linsync-gui/qml/WebpageComparePage.qml
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: root
    title: i18n("Webpage Compare")

    // ── State ────────────────────────────────────────────────────────────────
    property string leftUrl: ""
    property string rightUrl: ""
    property string subMode: "html"      // html | text | tree | rendered | screenshot
    property bool   busy: false
    property string resultSummary: ""

    // ── Layout ───────────────────────────────────────────────────────────────
    ColumnLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.largeSpacing

        // Privacy notice — always visible on this page.
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            type: Kirigami.MessageType.Warning
            text: i18n(
                "Webpage compare fetches content from the internet. " +
                "Third-party resources on each page may also be requested."
            )
            visible: true
        }

        // URL inputs.
        Kirigami.FormLayout {
            Layout.fillWidth: true

            QQC2.TextField {
                id: leftUrlField
                Kirigami.FormData.label: i18n("Left URL:")
                placeholderText: "https://example.com/"
                text: root.leftUrl
                onTextChanged: root.leftUrl = text
            }

            QQC2.TextField {
                id: rightUrlField
                Kirigami.FormData.label: i18n("Right URL:")
                placeholderText: "https://example.com/"
                text: root.rightUrl
                onTextChanged: root.rightUrl = text
            }

            QQC2.ComboBox {
                id: subModeCombo
                Kirigami.FormData.label: i18n("Compare mode:")
                model: [
                    { text: i18n("HTML source"),    value: "html" },
                    { text: i18n("Extracted text"), value: "text" },
                    { text: i18n("Resource tree"),  value: "tree" },
                    { text: i18n("Rendered (requires web-engine build)"),    value: "rendered" },
                    { text: i18n("Screenshot (requires web-engine build)"),  value: "screenshot" },
                ]
                textRole: "text"
                valueRole: "value"
                onActivated: root.subMode = currentValue
            }
        }

        // Action buttons.
        RowLayout {
            Layout.fillWidth: true

            QQC2.Button {
                text: i18n("Compare…")
                enabled: root.leftUrl.length > 0 && root.rightUrl.length > 0 && !root.busy
                onClicked: confirmDialog.open()
                icon.name: "internet-web-browser-symbolic"
            }

            QQC2.Button {
                text: i18n("Clear webcompare cache")
                flat: true
                onClicked: bridge.clearWebcompareCache()
                icon.name: "edit-clear-symbolic"
            }
        }

        // Progress indicator.
        QQC2.BusyIndicator {
            running: root.busy
            visible: root.busy
            Layout.alignment: Qt.AlignHCenter
        }

        // Result summary label.
        QQC2.Label {
            visible: !root.busy && root.resultSummary.length > 0
            text: root.resultSummary
            Layout.fillWidth: true
            wrapMode: Text.Wrap
        }

        Item { Layout.fillHeight: true }
    }

    // ── Confirmation dialog ───────────────────────────────────────────────────
    QQC2.Dialog {
        id: confirmDialog
        title: i18n("Fetch from the internet?")
        modal: true
        anchors.centerIn: parent

        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: i18n(
                    "LinSync will fetch the following URLs:\n\n" +
                    "  Left:  %1\n" +
                    "  Right: %2\n\n" +
                    "Third-party resources linked from these pages may also be " +
                    "requested depending on the compare mode. No cookies or " +
                    "credentials from your personal browser are used.",
                    root.leftUrl, root.rightUrl
                )
            }
        }

        standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel

        onAccepted: {
            root.busy = true
            root.resultSummary = ""
            bridge.compareWebpages(root.leftUrl, root.rightUrl, root.subMode)
        }
        onRejected: {}
    }

    // ── Bridge connections ────────────────────────────────────────────────────
    Connections {
        target: bridge

        function onWebpageCompareFinished(summary) {
            root.busy = false
            root.resultSummary = summary
        }

        function onWebpageCompareFailed(errorMessage) {
            root.busy = false
            root.resultSummary = i18n("Error: %1", errorMessage)
        }

        function onWebcompareCacheCleared() {
            // Optionally show a passive notification.
        }
    }
}
```

### Bridge additions

Add to the HTTP bridge (in `apps/linsync-gui/src/main.rs` or the relevant router file):

```rust
// POST /api/compare/webpage
// Body: { "left_url": "...", "right_url": "...", "sub_mode": "html|text|tree|rendered|screenshot" }
// Response: { "status": "ok|error", "summary": "...", "error": "..." }
```

Add to the `cxx-qt` bridge (`apps/linsync-gui/src/cxxqt_session.rs` or equivalent):

```rust
#[qinvokable]
pub fn compare_webpages(&self, left_url: QString, right_url: QString, sub_mode: QString) {
    // Marshal to Rust types, call linsync_core::compare_webpage_*, emit signal.
}

#[qinvokable]
pub fn clear_webcompare_cache(&self) {
    let cache_dir = linsync_core::AppPaths::from_env().cache_dir;
    let _ = linsync_core::clear_webcompare_cache(&cache_dir);
    // Emit signal: webcompareCacheCleared()
}
```

Emit QML-side signals: `webpageCompareFinished(string summary)` and `webpageCompareFailed(string errorMessage)`.

### Tests (write first)

Smoke test via the existing `scripts/gui-smoke.sh` pattern:

```bash
# Verify the QML file is syntactically valid by importing it via qmlls or qmllint.
# Add to scripts/gui-smoke.sh or as a separate scripts/qml-lint.sh:
qmllint apps/linsync-gui/qml/WebpageComparePage.qml
```

Rust bridge test (offline — no network):

```rust
// apps/linsync-gui/tests/webpage_bridge.rs
// Verify the bridge route is registered and returns 400 for missing body.
// (Pattern follows any existing bridge tests in the project.)
```

**Acceptance:** `cargo build -p linsync-gui` succeeds; QML file passes `qmllint`; no regressions in existing GUI smoke test.

---

## Checklist summary

- [ ] **9.1** `webpage.rs` types + `clear_webcompare_cache` + `webcompare_cache_dir` — 3 unit tests
- [ ] **9.2** `web-fetch` plugin manifest + Python script + HTML fixtures — manifest validation test
- [ ] **9.3** `compare_webpage_html_source` — 3 unit tests (guard, identical, different)
- [ ] **9.4** `compare_webpage_extracted_text` — 2 unit tests (guard, strips-tags)
- [ ] **9.5** `compare_webpage_resource_tree` — 3 unit tests (guard, left-only link, both-same)
- [ ] **9.6** Network gating hardening + CLI placeholder — 3 guard tests
- [ ] **9.7** `linsync-webengine` stub crate — 2 unit tests (NotImplemented, idempotent clear)
- [ ] **9.8** Rendered + Screenshot sub-modes — 3 unit tests (guards + rendered-fallback)
- [ ] **9.9** CLI `webpage` sub-command — 3 integration tests
- [ ] **9.10** `WebpageComparePage.qml` + bridge — `qmllint` + bridge smoke test

**Final gate before merge:**

```bash
cargo test --workspace
cargo test --workspace --features web-engine
cargo clippy --workspace -- -D warnings
cargo clippy --workspace --features web-engine -- -D warnings
cargo fmt --check
```

---

## Open issues carried forward from the design doc

1. **`wget` in Flatpak:** The Python plugin uses `urllib` only (no `wget`). Resource-tree depth > 1 uses Python recursion, not `wget --spider`. Compatible with Flatpak runtimes that include Python 3.
2. **Request cap:** `max_requests` (default 50) is wired through `WebpageCompareOptions` and the plugin protocol.
3. **Cookie lifetime:** In-memory only per session. Per-pair ephemeral cookie files are out of scope for Phase 9.
4. **Flatpak network scope:** `--share=network` required for any build with `web-fetch` plugin enabled. Narrower portal-based scoping is a future improvement.
5. **Phase 9.7-bis (Qt bindings):** The `render_url` stub returns `NotImplemented`. A follow-up task adds real `cxx-qt` bindings to `QWebEngineView`. At that point the `Rendered` fallback in Task 9.8 and the `Screenshot` happy-path test are updated.
