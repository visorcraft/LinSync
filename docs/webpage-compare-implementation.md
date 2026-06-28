# Webpage Compare Implementation Design

> Status: design — supersedes the deferral in `docs/webpage-compare.md`.

## Goals

- Provide five URL compare sub-modes (rendered, screenshot, HTML source, extracted
  text, resource-tree) as a post-MVP opt-in feature.
- Satisfy every privacy prerequisite from `docs/webpage-compare.md` before any
  network code ships.
- Route all network I/O through the Phase 6 plugin sandbox so the core process
  never touches the network directly.

## Non-goals

- LinSync remains a native Qt/Kirigami desktop application. The browser engine is
  confined to the compare surface; it is never the application shell.
- No Windows support. The plugin and feature gate are Linux-only.
- No persistent login, form fill, or credential storage of any kind.

---

## Sub-modes

### Rendered page (feature-gated: requires `web-engine` feature)

Loads both URLs in an isolated Qt WebEngine `QWebEngineProfile` (no-persist,
in-memory storage, no cookies carried across sessions). Captures rendered pixels
and feeds the result to the image compare pipeline (Phase 7 dependency). The
feature gate keeps Qt WebEngine out of the default build.

### Screenshot capture → image compare (depends on Phase 7)

Depends on rendered mode: after a full-page screenshot is taken via
`QWebEngineView::grab()`, the PNG is handed to `compare_images()` from Phase 7.
Only available when both `web-engine` and Phase 7 are present.

### HTML source → text compare

Fetches raw HTML via the `web-fetch` plugin (no JS execution, no resource
loading). Passes the bytes to the existing text compare pipeline as UTF-8.
Available in the default build; no browser engine required.

### Extracted text → text compare

Fetches HTML via the `web-fetch` plugin, then strips tags with a
`text/html → text/plain` prediffer plugin. Passes cleaned text to the text
compare pipeline. Available in the default build.

### Resource-tree → folder compare

Uses `wget --spider --recursive --level=1` (depth capped at 1 by default,
configurable up to 3) inside the `web-fetch` plugin to enumerate linked URLs.
Presents the URL list as a virtual folder for folder compare. No binary
downloads; only URL enumeration. Available in the default build.

---

## Browser engine analysis

### Qt WebEngine

**License:** Qt WebEngine is dual-licensed. The open-source release is available
under LGPLv3 (with Qt's additional LGPL exception) and GPLv2.

**Compatibility with GPL-3.0-only:** LGPLv3 permits dynamic linkage from a
GPL-3.0-only program without imposing its own terms on the GPL program, provided
the user can relink against a modified Qt WebEngine. Dynamic linkage is the
standard deployment on Linux (shared `.so` libraries). LinSync does not
statically link Qt WebEngine. Therefore LGPLv3 dynamic linkage is compatible
with GPL-3.0-only.

The GPLv2 alternative is not compatible with GPL-3.0-only (GPLv2 and GPLv3 are
not interchangeable), but LinSync links against the LGPLv3 variant exclusively.

**`cargo deny` impact:** Qt WebEngine is a C++ shared library, not a Cargo
crate. `deny.toml` governs Cargo crate licenses; no entry is needed for it.
The wrapper crate (`crates/linsync-webengine`) will be MIT. `cargo deny check`
continues to pass.

### `chromium --headless` helper

Spawning `chromium --headless=new --screenshot=…` as a subprocess avoids all
LGPL linkage concerns (process boundary = no linkage), but requires the user to
have Chromium installed and gives less profile-isolation control (the renderer
uses a dedicated `--user-data-dir` under LinSync's cache to compensate).

---

## Recommendation

**Ship Qt WebEngine behind the `web-engine` cargo feature.** Default build
excludes it; rendered and screenshot modes are only available when a distributor
enables `--features web-engine`. HTML source, extracted text, and resource-tree
modes ship in the default build via the `web-fetch` plugin with no browser
engine. The `chromium --headless` path, originally deferred from Phase 9, is
now implemented as an auto-detected fallback backend: `render_url` prefers a
QML runner (`qml6`, then `qml`, honoring `LINSYNC_QML_RUNNER`) and falls back
to a headless Chromium binary on `PATH` (`chromium`, `chromium-browser`,
`google-chrome-stable`); `LINSYNC_WEB_RENDERER=qml|chromium` forces a backend.

---

## Privacy architecture

**Separate profile:** Qt WebEngine uses `QWebEngineProfile("linsync-webcompare",
parent)` with `setPersistentStoragePath(cache_dir)`. No default profile; no
cookies, credentials, or cache reused from any personal browser.

**Default-no-cookies:** cookies are in-memory only; `deleteAllCookies()` is
called at profile creation and at compare-session teardown.

**Opt-in network:** URL compare is only reachable via an explicit user action
("Compare URLs…" dialog or `--mode webpage` on the CLI). Plain file/folder
compare paths contain no URL-dispatch code.

**Explicit start gesture:** the GUI shows a confirmation dialog naming both URLs
and warning about third-party resources before any network request. The CLI
requires `--confirm` or an interactive prompt.

**Cache location:** `$XDG_CACHE_HOME/linsync/webcompare/` — subdirs: `profile/`
(Qt WebEngine storage), `fetched/` (plugin HTML and resource lists).

**Clear controls:** Settings page exposes "Clear webpage compare data" (deletes
the cache dir). CLI exposes `linsync-cli cache clear --scope webcompare`.

**No personal profile reuse:** the code never reads `~/.config/chromium`,
`~/.mozilla`, or any other browser profile directory.

---

## Plugin integration

A new `web-fetch` plugin handles all non-rendered URL access:

```
packaging/plugins/web-fetch/
  linsync-plugin.json
  web-fetch          (compiled binary or shell script)
```

Manifest fragment:

```json
{
  "id": "linsync.web-fetch",
  "classes": ["unpacker"],
  "mime_types": ["text/html", "text/uri-list"],
  "capabilities": [],
  "deterministic": false,
  "sandbox": {
    "network": true,
    "writes_input": false,
    "requires_home_access": false
  }
}
```

The plugin receives an `unpack_text` request with `mime_type: "text/uri-list"`
pointing to a file containing the target URL, writes fetched HTML (or a URL
list for tree mode) to its temp directory, and returns a `path` output. For
resource-tree mode it uses `wget --spider --recursive --level=<depth>`. The
core process itself never calls any HTTP client; `network: true` is the Phase 6
sandbox opt-in.

---

## Sandbox interaction

Phase 6 is a hard dependency. The `web-fetch` plugin requires the Phase 6
sandbox with `network=true`; it is runtime-disabled until Phase 6 ships.
Qt WebEngine runs in the Qt process; Flatpak builds with `web-engine` must
declare `--share=network`. Default Flatpak builds need no network permission.

---

## API surface

```rust
pub enum WebpageMode {
    Rendered,      // requires web-engine feature
    Screenshot,    // requires web-engine feature + Phase 7
    HtmlSource,
    ExtractedText,
    ResourceTree,
}

pub struct WebpageCompareOptions {
    pub resource_tree_depth: u8,      // default 1, max 3
    pub timeout_secs: u32,            // default 30
    pub user_agent: Option<String>,   // default: None (use plugin default)
}

pub fn compare_webpages(
    left: &str,
    right: &str,
    mode: WebpageMode,
    options: &WebpageCompareOptions,
) -> Result<WebpageCompareResult, WebpageCompareError>;
```

`WebpageCompareResult` wraps the appropriate existing result type:
`TextCompareResult` for source/text modes, `ImageCompareResult` for
rendered/screenshot, `FolderCompareResult` for resource-tree.

---

## CLI integration

```
linsync-cli compare --mode webpage <url1> <url2> \
    --sub-mode <rendered|screenshot|html|text|tree> \
    [--depth <1-3>] [--timeout <secs>] [--confirm]
```

`--sub-mode` defaults to `html`. `--confirm` suppresses the interactive prompt
for use in scripts. Rendered and screenshot sub-modes exit with error code 2 if
built without `--features web-engine`.

---

## GUI integration

New QML file: `apps/linsync-gui/qml/WebpageComparePage.qml`

Sketch:

```qml
// WebpageComparePage.qml
Page {
    property string leftUrl
    property string rightUrl
    property string subMode: "html"   // html | text | tree | rendered | screenshot

    ColumnLayout {
        // URL inputs (two text fields)
        // Sub-mode selector (ComboBox)
        // Privacy notice banner (always visible when page is active)
        // "Fetch and compare" button — triggers confirmFetch()
        // Progress indicator
        // Result view: embeds TextComparePage / FolderComparePage / ImageComparePage
    }

    function confirmFetch() {
        confirmDialog.open()  // blocks until user accepts
    }

    Dialog {
        id: confirmDialog
        title: "Fetch from the internet?"
        // Shows both URLs; warns about third-party resources
        // OK → bridge.compareWebpages(leftUrl, rightUrl, subMode)
    }
}
```

The page is added as a tab in the main `StackLayout` alongside existing compare
pages. It is only reachable via explicit navigation; it is never opened
automatically.

---

## Fixture / test plan

All tests use a local HTTP server (the `httptest` crate). No live URLs in the
test suite.

Fixture files under `tests/fixtures/webcompare/`:
- `simple.html` — minimal page with two `<a>` links and one `<img>`.
- `encoding.html` — `charset=iso-8859-1` header to verify encoding handling.
- `redirect.html` — served with a 301 redirect by the fixture server.

Key test cases: (1) HTML source returns raw bytes unchanged; (2) extracted text
strips all tags; (3) resource-tree at depth 1 enumerates only direct links;
(4) redirects are followed; (5) a stalling server triggers `Timeout`; (6)
rendered mode (feature-gated) produces a non-empty PNG.

---

## Flatpak permissions

| Feature | Flatpak permission |
|---|---|
| HTML source / text / tree (`web-fetch` plugin) | `--share=network` (feature-flag-gated) |
| Rendered / screenshot (Qt WebEngine) | `--share=network` |
| Default build | None — no network code compiled in |

Flatpak builds that enable either feature must add `--share=network` to
`org.visorcraft.LinSync.yml`. Builds without the feature flags must not include
the permission.

---

## Blocking dependencies

- **Phase 6 (sandbox foundation):** required for `web-fetch` plugin runtime.
  Without Phase 6, all webpage compare modes are compile-time present but
  runtime-disabled.
- **Phase 7 (image compare):** required for screenshot sub-mode. HTML source,
  extracted text, and resource-tree modes do not depend on Phase 7.

---

## Open issues

1. **`wget` in Flatpak:** `wget` may not be present in the Flatpak runtime.
   Preferred alternative: implement URL enumeration natively in the `web-fetch`
   plugin binary using `reqwest` + `scraper`.
2. **Request cap:** resource-tree at depth > 1 can fan out. Enforce a default
   50-request cap per session; expose it in `WebpageCompareOptions`.
3. **Cookie lifetime:** current design is per-operation in-memory. Per-pair
   ephemeral profile with an explicit cookie-file opt-in is out of scope for
   Phase 9.

### Resolved: Flatpak network scope

An earlier draft of this list suggested using
`org.freedesktop.portal.NetworkMonitor` to restrict outbound access to the two
target domains only. Evaluation showed that is not possible: the NetworkMonitor
portal is connectivity *signaling* only (online/offline, metered) — it cannot
filter or restrict egress — and Flatpak has no per-domain firewall mechanism at
any layer.

**Decision:** keep `--share=network` with its scope documented in the manifest
(`packaging/flatpak/com.visorcraft.LinSync.yml`): the permission exists solely
for webpage compare — the `web-fetch` plugin (html / extracted-text /
resource-tree modes) and the Qt WebEngine (Chromium) renderer (rendered /
screenshot modes). Distributors who strip it lose exactly those features;
everything else works offline.

Genuine per-domain restriction would require routing all webpage traffic
through an in-app HTTP(S) proxy that filters by host. That is declared a
**permanent non-goal** — the complexity and attack surface of a bundled proxy
outweigh the benefit, given the runtime safeguards already in place: the
opt-in, explicit-consent flow and isolated profile described under
"Privacy architecture" above.
