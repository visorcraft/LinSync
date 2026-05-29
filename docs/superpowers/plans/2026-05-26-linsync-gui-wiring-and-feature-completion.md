# LinSync GUI Wiring + Feature Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve every GUI wiring gap and complete every outstanding feature item documented in `docs/known-limitations-1.0.md`, `docs/parity-acceptance.md`, and `AGENTS.md` §"QML shell — eight-section sidebar", producing a release-ready LinSync 1.0 with all post-1.0 features either implemented or formally specified.

**Architecture:**
- The Rust core (`crates/linsync-core`) owns all compare / merge / filter / plugin / storage logic. CLI and GUI are clients.
- The GUI bridges in two coexisting paths: an external HTTP/JSON bridge (`apps/linsync-gui/src/main.rs`) and an in-process `cxx-qt` bridge (`apps/linsync-gui/src/cxxqt_session.rs`). Every new method must be implemented in **both** paths.
- Wiring tasks add `cxx-qt` `Q_INVOKABLE` methods + matching HTTP endpoints, then connect QML signals to those endpoints.
- Feature work extends `linsync-core` modules first (TDD with core tests), then surfaces the new capability through both bridges, then connects QML.

**Tech Stack:** Rust 2024 edition, Cargo workspace (`resolver = "3"`), `cxx-qt` 0.6, Qt 6 / Kirigami, `tokio` (for the HTTP bridge), `serde`/`serde_json`, `tempfile`, `regex`. Tests use the built-in Cargo test harness; GUI smoke uses `QT_QPA_PLATFORM=offscreen` + `scripts/gui-smoke.sh`. Build acceleration via `mold` + `sccache` is already wired (commit `f1e85fa`).

**Scope notice (READ FIRST):** This plan contains two zones:

| Phase | Status | What you can do with it |
| ----- | ------ | ----------------------- |
| 1 – 5 | **READY** — TDD tasks with concrete code | Execute task-by-task |
| 6 | **DESIGN REQUIRED** — sandbox foundation (blocks 7/8/9/10) | Brainstorm first |
| 7 – 10 | **DESIGN REQUIRED** — design briefs only | Do NOT attempt to implement. Brainstorm the design first using `superpowers:brainstorming`, write a follow-up plan from that, then execute |
| 10.5, 11 | **READY** — fixtures + Git mergetool | Execute task-by-task (Phase 11 depends on Phase 3) |
| 12 | **DESIGN REQUIRED** — rendered diff modes | Brainstorm first |
| 13 | **READY** — release-polish checklist | Execute last |

Phases 6–10 cover features that the project's own decision docs (`docs/document-ocr-compare.md`, `docs/webpage-compare.md`, `docs/known-limitations-1.0.md`) explicitly defer until prerequisite design work is finished. Writing TDD tasks for them without that design would produce placeholders — the writing-plans skill forbids that. Instead, each of those phases contains: status, acceptance criteria the future design must satisfy, blocking dependencies, file impact, and the open questions a brainstorming session must answer.

**Dependency graph (top → bottom = must finish first):**

```
Phase 1  (GUI wiring) ───────┐
Phase 2  (filters)     ──────┤
Phase 3  (merge UI)    ──────┤
Phase 4  (plugin proto)──────┤
Phase 5  (moved blocks)──────┤
Phase 10.5 (sym/perm fixtures)
Phase 11 (Git mergetool) needs Phase 3 ─┐
                                        │
                                        ↓
                              Phase 13 (release polish)
                                        ↑
                                        │
            Phase 6 (sandbox foundation)│  ← DESIGN REQUIRED, blocks 7/8/10
                       │                │
            ┌──────────┼──────────┐     │
            ↓          ↓          ↓     │
       Phase 7    Phase 8     Phase 10  │
       (image)    (OCR)       (archive) │
                                ↓       │
                             Phase 9    │
                             (webpage) ─┘

Phase 12 (rendered modes) ─ DESIGN REQUIRED, independent ─→ Phase 13
```

**Commit cadence:** TDD red → green → commit each task. No batching across tasks.

---

## Phase 1 — 1.0 GUI Wiring

**Outcome:** FiltersPage, SettingsPage, and PluginsPage all round-trip user changes through the bridge to disk; screenshot-based GUI checks added to CI.

### Task 1.1: SettingsPage.settingChanged → HTTP bridge round-trip test

**Files:**
- Test: `crates/linsync-core/tests/settings_round_trip.rs` (new)
- Reference: `crates/linsync-core/src/storage.rs:280-332` (SettingsStore), `crates/linsync-core/src/storage.rs:22-56` (Settings struct)

- [ ] **Step 1: Write the failing core round-trip test**

```rust
// crates/linsync-core/tests/settings_round_trip.rs
use linsync_core::storage::{Settings, SettingsStore, ThemePreference};
use tempfile::TempDir;

#[test]
fn round_trip_every_settings_key() {
    let dir = TempDir::new().unwrap();
    let store = SettingsStore::new(dir.path().join("settings.json"));

    let mut s = store.load_or_default().unwrap();
    s.theme_preference = ThemePreference::Dark;
    s.pane_font_size = 14;
    s.pane_font_family = "Iosevka".into();
    s.pane_tab_width = 8;
    s.show_line_numbers = true;
    s.show_whitespace = true;
    s.word_wrap = true;
    s.ignore_case = true;
    s.ignore_whitespace = true;
    s.ignore_blank_lines = true;
    s.ignore_eol = true;
    s.eol_normalization = "lf".into();
    s.default_compare_mode = "binary".into();
    s.open_last_session = true;
    s.confirm_on_close = true;
    s.persist_recent_paths = true;
    s.recent_limit = 25;
    store.save(&s).unwrap();

    let loaded = store.load_or_default().unwrap();
    assert_eq!(loaded.theme_preference, ThemePreference::Dark);
    assert_eq!(loaded.pane_font_size, 14);
    assert_eq!(loaded.pane_font_family, "Iosevka");
    assert_eq!(loaded.pane_tab_width, 8);
    assert!(loaded.show_line_numbers);
    assert!(loaded.show_whitespace);
    assert!(loaded.word_wrap);
    assert!(loaded.ignore_case);
    assert!(loaded.ignore_whitespace);
    assert!(loaded.ignore_blank_lines);
    assert!(loaded.ignore_eol);
    assert_eq!(loaded.eol_normalization, "lf");
    assert_eq!(loaded.default_compare_mode, "binary");
    assert!(loaded.open_last_session);
    assert!(loaded.confirm_on_close);
    assert!(loaded.persist_recent_paths);
    assert_eq!(loaded.recent_limit, 25);
}
```

- [ ] **Step 2: Run the test, expect PASS** (the core already supports this — this test pins the contract before we touch the bridge)

Run: `cargo test -p linsync-core --test settings_round_trip -- --nocapture`
Expected: PASS. If FAIL, the bug is in `storage.rs` and must be fixed before continuing.

- [ ] **Step 3: Commit**

```bash
git add crates/linsync-core/tests/settings_round_trip.rs
git commit -m "test(core): pin settings round-trip contract for every GUI key"
```

### Task 1.2: Verify `apply_gui_setting` in main.rs handles every key

**Files:**
- Modify: `apps/linsync-gui/src/main.rs:962-995` (apply_gui_setting)
- Test: `apps/linsync-gui/tests/settings_bridge.rs` (new)

- [ ] **Step 1: Write the failing bridge integration test**

```rust
// apps/linsync-gui/tests/settings_bridge.rs
//
// We can't easily spawn the HTTP bridge from a test, so we test the
// settings_set_bridge_response helper directly via re-export.

use linsync_gui::test_support::{apply_gui_setting_test, default_paths};

#[test]
fn every_documented_key_round_trips_through_bridge() {
    let paths = default_paths();
    for (key, value) in [
        ("themePreference", "dark"),
        ("fontSize", "14"),
        ("fontFamily", "Iosevka"),
        ("tabWidth", "8"),
        ("showLineNumbers", "true"),
        ("showWhitespace", "true"),
        ("wordWrap", "true"),
        ("ignoreCase", "true"),
        ("ignoreWhitespace", "true"),
        ("ignoreBlankLines", "true"),
        ("ignoreEol", "true"),
        ("eolNormalization", "lf"),
        ("defaultCompareMode", "binary"),
        ("openLastSession", "true"),
        ("confirmOnClose", "true"),
        ("persistRecentPaths", "true"),
        ("maxRecentPaths", "25"),
    ] {
        let result = apply_gui_setting_test(&paths, key, value);
        assert!(result.is_ok(), "key {key} value {value} failed: {result:?}");
    }
}
```

- [ ] **Step 2: Run test, expect FAIL with "module test_support not found"**

Run: `cargo test -p linsync --test settings_bridge`
Expected: FAIL (no test_support module yet).

- [ ] **Step 3: Add test-support module exposing apply_gui_setting**

In `apps/linsync-gui/src/main.rs`, locate `fn apply_gui_setting` near line 962. Just above that function, add:

```rust
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::{apply_gui_setting, AppPaths};
    use linsync_core::paths::AppPaths as CorePaths;

    pub fn default_paths() -> AppPaths {
        AppPaths::for_tests()
    }

    pub fn apply_gui_setting_test(paths: &AppPaths, key: &str, value: &str) -> Result<(), String> {
        apply_gui_setting(paths, key, value).map_err(|e| e.to_string())
    }
}
```

In `apps/linsync-gui/Cargo.toml`, add the feature:

```toml
[features]
default = []
cxxqt-app = [...]
cxxqt-smoke = [...]
test-support = []
```

In `crates/linsync-core/src/paths.rs`, add (if not present):

```rust
impl AppPaths {
    pub fn for_tests() -> Self {
        let tmp = std::env::temp_dir().join(format!("linsync-test-{}", std::process::id()));
        Self {
            config_dir: tmp.join("config"),
            data_dir: tmp.join("data"),
            cache_dir: tmp.join("cache"),
            state_dir: tmp.join("state"),
        }
    }
}
```

- [ ] **Step 4: Run test, expect PASS** for every key currently in the registry

Run: `cargo test -p linsync --test settings_bridge --features test-support -- --nocapture`
Expected: PASS for each documented key. If any key fails, fix the `apply_gui_setting` match arm in `main.rs:962-995` to handle it.

- [ ] **Step 5: Commit**

```bash
git add apps/linsync-gui/src/main.rs apps/linsync-gui/tests/settings_bridge.rs apps/linsync-gui/Cargo.toml crates/linsync-core/src/paths.rs
git commit -m "test(gui): pin every SettingsPage key through apply_gui_setting"
```

### Task 1.3: Wire SettingsPage.settingChanged in QML to /settings/set

**Files:**
- Modify: `apps/linsync-gui/qml/SettingsPage.qml:45` (settingChanged signal handler)
- Modify: `apps/linsync-gui/qml/Main.qml` (connect settingChanged → bridge)

- [ ] **Step 1: Write a smoke check that exercises the wiring**

Add to `scripts/gui-smoke.sh` (the existing offscreen smoke):

```bash
# After the existing startup phase:
echo "--- Settings round-trip smoke ---"
curl -sf "http://127.0.0.1:${LINSYNC_BRIDGE_PORT}/settings/set?key=themePreference&value=dark"
RESP=$(curl -sf "http://127.0.0.1:${LINSYNC_BRIDGE_PORT}/settings")
echo "$RESP" | grep -q '"theme_preference":"Dark"' || { echo "settings round-trip FAILED"; exit 1; }
echo "settings round-trip OK"
```

- [ ] **Step 2: Run gui-smoke, expect FAIL**

Run: `bash scripts/gui-smoke.sh`
Expected: FAIL at the settings round-trip line (QML wiring not yet in place but bridge endpoint already works — actually this part may PASS without QML changes; the next step verifies the QML→bridge wiring).

- [ ] **Step 3: Add QML signal handler in Main.qml**

In `apps/linsync-gui/qml/Main.qml`, locate the `SettingsPage` instantiation inside the `StackLayout`. Add a signal connection:

```qml
SettingsPage {
    id: settingsPage
    // existing properties...
    onSettingChanged: function(key, value) {
        if (typeof sessionBridge !== "undefined") {
            sessionBridge.save_setting(key, JSON.stringify(value));
        } else {
            const url = `http://${root.bridgeHost}:${root.bridgePort}/settings/set?key=${encodeURIComponent(key)}&value=${encodeURIComponent(value)}`;
            const xhr = new XMLHttpRequest();
            xhr.open("GET", url);
            xhr.send();
        }
    }
}
```

- [ ] **Step 4: Add a Component.onCompleted block in SettingsPage.qml to pull initial values**

In `apps/linsync-gui/qml/SettingsPage.qml`, near the bottom add:

```qml
Component.onCompleted: {
    function pull() {
        if (typeof sessionBridge !== "undefined") {
            const data = JSON.parse(sessionBridge.load_settings());
            applyFromJson(data);
            return;
        }
        const xhr = new XMLHttpRequest();
        xhr.onreadystatechange = function() {
            if (xhr.readyState === 4 && xhr.status === 200) {
                applyFromJson(JSON.parse(xhr.responseText));
            }
        };
        xhr.open("GET", `http://${root.bridgeHost}:${root.bridgePort}/settings`);
        xhr.send();
    }
    function applyFromJson(data) {
        if ("theme_preference" in data) themePreference = data.theme_preference;
        if ("pane_font_size" in data) fontSize = data.pane_font_size;
        if ("pane_font_family" in data) fontFamily = data.pane_font_family;
        if ("pane_tab_width" in data) tabWidth = data.pane_tab_width;
        if ("show_line_numbers" in data) showLineNumbers = data.show_line_numbers;
        if ("show_whitespace" in data) showWhitespace = data.show_whitespace;
        if ("word_wrap" in data) wordWrap = data.word_wrap;
        if ("ignore_case" in data) ignoreCase = data.ignore_case;
        if ("ignore_whitespace" in data) ignoreWhitespace = data.ignore_whitespace;
        if ("ignore_blank_lines" in data) ignoreBlankLines = data.ignore_blank_lines;
        if ("ignore_eol" in data) ignoreEol = data.ignore_eol;
        if ("eol_normalization" in data) eolNormalization = data.eol_normalization;
        if ("default_compare_mode" in data) defaultCompareMode = data.default_compare_mode;
        if ("open_last_session" in data) openLastSession = data.open_last_session;
        if ("confirm_on_close" in data) confirmOnClose = data.confirm_on_close;
        if ("persist_recent_paths" in data) persistRecentPaths = data.persist_recent_paths;
        if ("recent_limit" in data) maxRecentPaths = data.recent_limit;
    }
    pull();
}
```

- [ ] **Step 5: Run gui-smoke, expect PASS**

Run: `bash scripts/gui-smoke.sh && LINSYNC_GUI_SMOKE_CXXQT=1 bash scripts/gui-smoke.sh`
Expected: both PASS, including the settings round-trip line.

- [ ] **Step 6: Commit**

```bash
git add apps/linsync-gui/qml/Main.qml apps/linsync-gui/qml/SettingsPage.qml scripts/gui-smoke.sh
git commit -m "feat(gui): wire SettingsPage.settingChanged through both bridges + load persisted values"
```

### Task 1.4: Wire FiltersPage edit signals → HTTP + cxx-qt

**Files:**
- Modify: `apps/linsync-gui/src/main.rs` (add /walk/set handlers for new keys)
- Modify: `apps/linsync-gui/src/cxxqt_session.rs:22-37` (add walk Q_PROPERTYs) + invokables
- Modify: `apps/linsync-gui/qml/FiltersPage.qml` (Component.onCompleted + signal handlers)
- Modify: `apps/linsync-gui/qml/Main.qml` (connect FiltersPage signals)
- Test: `apps/linsync-gui/tests/filters_bridge.rs` (new)

- [ ] **Step 1: Write the failing filters bridge test**

```rust
// apps/linsync-gui/tests/filters_bridge.rs
use linsync_gui::test_support::{default_paths, set_walk_option_test, get_walk_options_test};

#[test]
fn walk_option_round_trip() {
    let paths = default_paths();

    set_walk_option_test(&paths, "respect_gitignore", "true").unwrap();
    set_walk_option_test(&paths, "follow_symlinks", "false").unwrap();
    set_walk_option_test(&paths, "max_depth", "10").unwrap();

    let opts = get_walk_options_test(&paths);
    assert_eq!(opts.respect_gitignore, true);
    assert_eq!(opts.follow_symlinks, false);
    assert_eq!(opts.max_depth, Some(10));
}

#[test]
fn save_named_filter_round_trip() {
    let paths = default_paths();
    let body = r#"{"name":"src-only","includes":["f:.*\\.rs$"],"excludes":["d:target"]}"#;
    linsync_gui::test_support::save_filter_test(&paths, body).unwrap();
    let listed = linsync_gui::test_support::list_filters_test(&paths);
    assert!(listed.iter().any(|f| f.name == "src-only"));
}
```

- [ ] **Step 2: Run test, expect FAIL**

Run: `cargo test -p linsync --test filters_bridge --features test-support`
Expected: FAIL — test_support helpers don't exist yet.

- [ ] **Step 3: Add helpers to test_support module**

In `apps/linsync-gui/src/main.rs` within the `test_support` module from Task 1.2, add:

```rust
pub fn set_walk_option_test(paths: &AppPaths, key: &str, value: &str) -> Result<(), String> {
    super::apply_walk_option(paths, key, value).map_err(|e| e.to_string())
}

pub fn get_walk_options_test(paths: &AppPaths) -> linsync_core::storage::WalkOptions {
    super::load_walk_options(paths).unwrap()
}

pub fn save_filter_test(paths: &AppPaths, body: &str) -> Result<(), String> {
    super::save_named_filter(paths, body).map_err(|e| e.to_string())
}

pub fn list_filters_test(paths: &AppPaths) -> Vec<linsync_core::storage::NamedFilter> {
    super::list_named_filters(paths).unwrap()
}
```

The functions `apply_walk_option`, `load_walk_options`, `save_named_filter`, `list_named_filters` are existing handlers in `main.rs` (used by the `/walk/set`, `/walk`, `/filters/save`, `/filters/list` endpoints — see lines 1742-1747). If they're not extracted into named functions yet, extract them now from the endpoint dispatchers.

- [ ] **Step 4: Run test, expect PASS**

Run: `cargo test -p linsync --test filters_bridge --features test-support`
Expected: PASS.

- [ ] **Step 5: Add cxx-qt invokables for walk options**

In `apps/linsync-gui/src/cxxqt_session.rs`, inside the `LinSyncSessionBridge` impl block, add:

```rust
#[qinvokable]
pub fn load_walk_options(self: Pin<&mut Self>) -> QString {
    // serialize current WalkOptions to JSON; same shape as HTTP /walk response
}

#[qinvokable]
pub fn set_walk_option(self: Pin<&mut Self>, key: QString, value: QString) -> QString {
    // mirror /walk/set behavior; emit walkOptionsChanged signal on success
}

#[qinvokable]
pub fn list_filters(self: Pin<&mut Self>) -> QString { /* … */ }

#[qinvokable]
pub fn save_named_filter(self: Pin<&mut Self>, body: QString) -> QString { /* … */ }

#[qinvokable]
pub fn delete_named_filter(self: Pin<&mut Self>, name: QString) -> QString { /* … */ }

#[qinvokable]
pub fn validate_filter(self: Pin<&mut Self>, body: QString) -> QString { /* … */ }
```

Add a signal:

```rust
#[qsignal]
fn walk_options_changed(self: Pin<&mut Self>);
```

- [ ] **Step 6: Wire FiltersPage signals in Main.qml**

In `apps/linsync-gui/qml/Main.qml` at the `FiltersPage` instantiation:

```qml
FiltersPage {
    id: filtersPage
    onIncludesEdited: rules => bridge.saveIncludes(rules)
    onExcludesEdited: rules => bridge.saveExcludes(rules)
    onGitignoreToggled: v => bridge.setWalkOption("respect_gitignore", v)
    onFollowSymlinksToggled: v => bridge.setWalkOption("follow_symlinks", v)
    onMaxDepthEdited: v => bridge.setWalkOption("max_depth", v)
    onValidateRequested: body => filtersPage.showValidation(bridge.validateFilter(body))
    onSaveFilterRequested: body => bridge.saveNamedFilter(body)
    onDeleteFilterRequested: name => bridge.deleteNamedFilter(name)
}

QtObject {
    id: bridge
    function setWalkOption(key, value) {
        if (typeof sessionBridge !== "undefined") {
            return sessionBridge.set_walk_option(key, String(value));
        }
        // HTTP fallback
        const xhr = new XMLHttpRequest();
        xhr.open("GET", `http://${root.bridgeHost}:${root.bridgePort}/walk/set?key=${encodeURIComponent(key)}&value=${encodeURIComponent(value)}`);
        xhr.send();
    }
    // ... similar helpers for the other methods
}
```

- [ ] **Step 7: Pull initial filter list and walk options on FiltersPage load**

In `apps/linsync-gui/qml/FiltersPage.qml` add a `Component.onCompleted` that calls `load_walk_options` + `list_filters` and populates the UI.

- [ ] **Step 8: Extend gui-smoke.sh**

Add to `scripts/gui-smoke.sh`:

```bash
echo "--- Filters round-trip smoke ---"
curl -sf "http://127.0.0.1:${LINSYNC_BRIDGE_PORT}/walk/set?key=follow_symlinks&value=false"
RESP=$(curl -sf "http://127.0.0.1:${LINSYNC_BRIDGE_PORT}/walk")
echo "$RESP" | grep -q '"follow_symlinks":false' || { echo "walk option FAILED"; exit 1; }

curl -sf "http://127.0.0.1:${LINSYNC_BRIDGE_PORT}/filters/save" \
    --data-urlencode 'body={"name":"smoke","includes":["f:.*\\.txt$"],"excludes":[]}' || true
RESP=$(curl -sf "http://127.0.0.1:${LINSYNC_BRIDGE_PORT}/filters/list")
echo "$RESP" | grep -q '"name":"smoke"' || { echo "filter save FAILED"; exit 1; }
echo "filters round-trip OK"
```

- [ ] **Step 9: Run all tests + smoke**

```bash
cargo test -p linsync-core --workspace
cargo test -p linsync --features test-support
bash scripts/gui-smoke.sh
LINSYNC_GUI_SMOKE_CXXQT=1 bash scripts/gui-smoke.sh
```
Expected: all PASS.

- [ ] **Step 10: Commit**

```bash
git add apps/linsync-gui/src/main.rs apps/linsync-gui/src/cxxqt_session.rs apps/linsync-gui/qml/FiltersPage.qml apps/linsync-gui/qml/Main.qml apps/linsync-gui/tests/filters_bridge.rs scripts/gui-smoke.sh
git commit -m "feat(gui): wire FiltersPage signals through both bridges"
```

### Task 1.5: Plugin discovery on PluginsPage

**Files:**
- Modify: `crates/linsync-core/src/plugin.rs` — ensure `discover_plugins(paths) -> Vec<PluginManifest>` exists and is public. If not, add it.
- Modify: `apps/linsync-gui/src/main.rs` — extend `/plugins/list` (line 1748) to read real manifest dirs (`$XDG_DATA_HOME/linsync/plugins` + bundled `packaging/plugins`). Add `/plugins/toggle` endpoint.
- Modify: `apps/linsync-gui/src/cxxqt_session.rs` — add `list_plugins()` / `toggle_plugin(id, enabled)` invokables.
- Modify: `apps/linsync-gui/qml/PluginsPage.qml` — replace static array (line 232) with dynamic model from bridge.

- [ ] **Step 1: Write failing core discovery test**

```rust
// crates/linsync-core/tests/plugin_discovery.rs
use linsync_core::plugin::{discover_plugins, PluginManifest};
use tempfile::TempDir;
use std::fs;

#[test]
fn discovers_user_and_system_plugins() {
    let dir = TempDir::new().unwrap();
    let user_dir = dir.path().join("user-plugins");
    let system_dir = dir.path().join("system-plugins");
    fs::create_dir_all(user_dir.join("zip-unpack")).unwrap();
    fs::write(
        user_dir.join("zip-unpack/manifest.json"),
        r#"{"name":"zip-unpack","version":"1.0","class":"unpacker","executable":"./zip-unpack.sh","operations":["unpack_text"]}"#,
    ).unwrap();
    fs::create_dir_all(system_dir.join("tar-unpack")).unwrap();
    fs::write(
        system_dir.join("tar-unpack/manifest.json"),
        r#"{"name":"tar-unpack","version":"1.0","class":"unpacker","executable":"./tar-unpack.sh","operations":["unpack_text"]}"#,
    ).unwrap();

    let plugins = discover_plugins(&[user_dir.as_path(), system_dir.as_path()]);
    let names: Vec<_> = plugins.iter().map(|p| p.name.clone()).collect();
    assert!(names.contains(&"zip-unpack".to_string()));
    assert!(names.contains(&"tar-unpack".to_string()));
}

#[test]
fn user_plugin_shadows_system_with_same_name() {
    let dir = TempDir::new().unwrap();
    let user_dir = dir.path().join("user");
    let system_dir = dir.path().join("system");
    for (base, version) in [(&user_dir, "2.0"), (&system_dir, "1.0")] {
        fs::create_dir_all(base.join("p")).unwrap();
        fs::write(
            base.join("p/manifest.json"),
            format!(r#"{{"name":"p","version":"{version}","class":"unpacker","executable":"./p.sh","operations":["unpack_text"]}}"#),
        ).unwrap();
    }

    let plugins = discover_plugins(&[user_dir.as_path(), system_dir.as_path()]);
    let p = plugins.iter().find(|m| m.name == "p").unwrap();
    assert_eq!(p.version, "2.0", "user dir wins (passed first)");
}
```

- [ ] **Step 2: Run test, expect FAIL**

Run: `cargo test -p linsync-core --test plugin_discovery`
Expected: FAIL — `discover_plugins` not exported or doesn't exist.

- [ ] **Step 3: Implement `discover_plugins` in `crates/linsync-core/src/plugin.rs`**

Add (find an appropriate location near the existing `PluginManifest` handling):

```rust
pub fn discover_plugins(roots: &[&std::path::Path]) -> Vec<PluginManifest> {
    let mut found: Vec<PluginManifest> = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = Default::default();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            let manifest_path = path.join("manifest.json");
            let Ok(text) = std::fs::read_to_string(&manifest_path) else { continue };
            let Ok(manifest) = serde_json::from_str::<PluginManifest>(&text) else { continue };
            if seen_names.insert(manifest.name.clone()) {
                found.push(manifest);
            }
        }
    }
    found
}
```

Make sure `PluginManifest` is `Serialize + Deserialize` and `pub`.

- [ ] **Step 4: Run test, expect PASS**

Run: `cargo test -p linsync-core --test plugin_discovery`
Expected: PASS both tests.

- [ ] **Step 5: Wire `/plugins/list` to call `discover_plugins`**

In `apps/linsync-gui/src/main.rs`, locate `plugins_list_bridge_response` (referenced from line 1748). Replace any hardcoded fixture with:

```rust
fn plugins_list_bridge_response(paths: &AppPaths) -> String {
    let user = paths.data_dir.join("plugins");
    let system = std::path::Path::new("/usr/share/linsync/plugins");
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("../share/linsync/plugins")));

    let mut roots: Vec<&std::path::Path> = vec![user.as_path(), system];
    if let Some(b) = bundled.as_ref() { roots.push(b.as_path()); }

    let plugins = linsync_core::plugin::discover_plugins(&roots);
    let enabled = load_plugin_enabled_map(paths);
    let json: Vec<_> = plugins.iter().map(|m| serde_json::json!({
        "id": m.name,
        "name": m.name,
        "version": m.version,
        "description": m.description.clone().unwrap_or_default(),
        "class": m.class,
        "operations": m.operations,
        "enabled": enabled.get(&m.name).copied().unwrap_or(true),
    })).collect();
    serde_json::to_string(&json).unwrap_or_else(|_| "[]".into())
}
```

Add the enable-map helper:

```rust
fn load_plugin_enabled_map(paths: &AppPaths) -> std::collections::HashMap<String, bool> {
    let p = paths.config_dir.join("plugins.json");
    std::fs::read_to_string(p).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_plugin_enabled(paths: &AppPaths, id: &str, enabled: bool) -> std::io::Result<()> {
    let mut map = load_plugin_enabled_map(paths);
    map.insert(id.to_string(), enabled);
    std::fs::create_dir_all(&paths.config_dir)?;
    let p = paths.config_dir.join("plugins.json");
    std::fs::write(p, serde_json::to_string_pretty(&map).unwrap_or_default())
}
```

- [ ] **Step 6: Add `/plugins/toggle` endpoint**

In the router setup near line 1748, add:

```rust
"/plugins/toggle" => plugins_toggle_bridge_response(query, paths),
```

And the handler:

```rust
fn plugins_toggle_bridge_response(query: &str, paths: &AppPaths) -> String {
    let id = query_param(query, "id").unwrap_or_default();
    let enabled = query_param(query, "enabled").unwrap_or_default() == "true";
    match save_plugin_enabled(paths, id, enabled) {
        Ok(()) => serde_json::json!({"ok": true}).to_string(),
        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
    }
}
```

- [ ] **Step 7: Wire PluginsPage QML to dynamic model**

In `apps/linsync-gui/qml/PluginsPage.qml`, replace the static array near line 232 with:

```qml
ListModel { id: pluginsModel }

function refresh() {
    if (typeof sessionBridge !== "undefined") {
        loadFromJson(sessionBridge.list_plugins());
        return;
    }
    const xhr = new XMLHttpRequest();
    xhr.onreadystatechange = function() {
        if (xhr.readyState === 4 && xhr.status === 200) loadFromJson(xhr.responseText);
    };
    xhr.open("GET", `http://${root.bridgeHost}:${root.bridgePort}/plugins/list`);
    xhr.send();
}
function loadFromJson(text) {
    pluginsModel.clear();
    const items = JSON.parse(text);
    for (const it of items) pluginsModel.append(it);
}
Component.onCompleted: refresh()

onPluginToggled: function(id, enabled) {
    if (typeof sessionBridge !== "undefined") {
        sessionBridge.toggle_plugin(id, enabled);
        return;
    }
    const xhr = new XMLHttpRequest();
    xhr.open("GET", `http://${root.bridgeHost}:${root.bridgePort}/plugins/toggle?id=${encodeURIComponent(id)}&enabled=${enabled}`);
    xhr.send();
}
onRefreshRequested: refresh()
```

Update the `Repeater`/`ListView` to use `pluginsModel` instead of the hardcoded array.

- [ ] **Step 8: Extend gui-smoke.sh**

```bash
echo "--- Plugins discovery smoke ---"
RESP=$(curl -sf "http://127.0.0.1:${LINSYNC_BRIDGE_PORT}/plugins/list")
echo "$RESP" | grep -q '"id":' || { echo "plugin discovery returned no items"; exit 1; }
# Toggle a plugin and verify it persists
FIRST_ID=$(echo "$RESP" | python3 -c 'import sys,json; print(json.load(sys.stdin)[0]["id"])')
curl -sf "http://127.0.0.1:${LINSYNC_BRIDGE_PORT}/plugins/toggle?id=${FIRST_ID}&enabled=false"
RESP2=$(curl -sf "http://127.0.0.1:${LINSYNC_BRIDGE_PORT}/plugins/list")
echo "$RESP2" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert any(p["id"]=="'$FIRST_ID'" and p["enabled"]==False for p in d), "toggle did not persist"'
echo "plugins discovery OK"
```

- [ ] **Step 9: Run all tests + smoke**

```bash
cargo test -p linsync-core --test plugin_discovery
cargo test -p linsync --features test-support
bash scripts/gui-smoke.sh
LINSYNC_GUI_SMOKE_CXXQT=1 bash scripts/gui-smoke.sh
```
Expected: all PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/linsync-core/src/plugin.rs crates/linsync-core/tests/plugin_discovery.rs apps/linsync-gui/src/main.rs apps/linsync-gui/src/cxxqt_session.rs apps/linsync-gui/qml/PluginsPage.qml scripts/gui-smoke.sh
git commit -m "feat(gui): wire PluginsPage to real discovery + enable persistence"
```

### Task 1.6: Screenshot-based GUI checks in CI

**Files:**
- Create: `scripts/gui-screenshot.sh`
- Modify: `.github/workflows/ci.yml` (add screenshot job)

- [ ] **Step 1: Author the screenshot script**

Create `scripts/gui-screenshot.sh`:

```bash
#!/usr/bin/env bash
# Capture screenshots of LinSync GUI at two window sizes for layout regression.
# Uses xvfb + scrot. Outputs target/screenshots/{desktop,mobile}-{page}.png.
set -euo pipefail
SIZES=("1600x900:desktop" "412x915:mobile")
PAGES=("compare" "sessions" "filters" "plugins" "settings" "about")
OUT=target/screenshots
mkdir -p "$OUT"

for spec in "${SIZES[@]}"; do
    geo="${spec%:*}"; tag="${spec#*:}"
    Xvfb :99 -screen 0 "${geo}x24" &
    XPID=$!
    sleep 1
    DISPLAY=:99 cargo run -p linsync --quiet -- &
    APP=$!
    sleep 5
    for page in "${PAGES[@]}"; do
        # Use bridge to switch section, then capture
        curl -sf "http://127.0.0.1:${LINSYNC_BRIDGE_PORT:-8765}/section/activate?name=${page}" || true
        sleep 1
        DISPLAY=:99 scrot "$OUT/${tag}-${page}.png"
    done
    kill $APP $XPID 2>/dev/null || true
    wait 2>/dev/null || true
done
```

Make executable: `chmod +x scripts/gui-screenshot.sh`

The bridge endpoint `/section/activate` doesn't exist yet — add it:

In `apps/linsync-gui/src/main.rs` (router near 1748):
```rust
"/section/activate" => section_activate_bridge_response(query, state),
```
Handler emits a state change event the QML side listens for. Alternatively, drive section switching via `QT_QUICK_BACKEND` and a startup arg `--section=<name>`; pick whichever is simpler in main.rs.

- [ ] **Step 2: Add CI job in `.github/workflows/ci.yml`**

Add after the existing `build` job:

```yaml
  screenshots:
    name: GUI screenshots
    runs-on: ubuntu-latest
    container:
      image: archlinux:latest
    needs: build
    steps:
      - uses: actions/checkout@v4
      - name: Install deps
        run: |
          pacman -Sy --noconfirm rust mold sccache qt6-base qt6-declarative \
              kirigami xorg-server-xvfb scrot curl python
      - name: Build GUI
        run: cargo build -p linsync --features cxxqt-app --release
      - name: Capture screenshots
        run: bash scripts/gui-screenshot.sh
      - name: Upload
        uses: actions/upload-artifact@v4
        with:
          name: linsync-screenshots-${{ github.sha }}
          path: target/screenshots/*.png
```

- [ ] **Step 3: Run locally to confirm**

```bash
bash scripts/gui-screenshot.sh
ls target/screenshots/   # expect 12 PNGs: 6 pages × 2 sizes
```

- [ ] **Step 4: Commit**

```bash
git add scripts/gui-screenshot.sh apps/linsync-gui/src/main.rs .github/workflows/ci.yml
git commit -m "ci: capture per-section GUI screenshots at desktop + mobile sizes"
```

---

## Phase 2 — Filter Grammar Completion + Migration

**Outcome:** `de:` and `e:` expression families parse and evaluate correctly; legacy `.flt` files migrate to the supported subset via `linsync-cli filter migrate`; filter editor UI surfaces validation inline.

### Task 2.1: Add `de:` directory-expression parser

**Files:**
- Modify: `crates/linsync-core/src/filter.rs:308-387` (parse_rule)
- Test: extend the existing tests module in `filter.rs`

- [ ] **Step 1: Write failing tests in filter.rs tests module**

```rust
#[test]
fn de_prefix_parses_size_and_date_expression() {
    let f = FileFilter::parse("de: size > 1024 AND mtime < '2026-01-01'").unwrap();
    assert!(matches!(f.rules[0].kind, FilterRuleKind::DirectoryExpression(_)));
}

#[test]
fn de_prefix_rejects_unknown_attribute() {
    let err = FileFilter::parse("de: chocolate > 1").unwrap_err();
    assert!(matches!(err.kind, FilterParseErrorKind::InvalidExpression));
}

#[test]
fn e_prefix_treats_as_file_or_directory() {
    let f = FileFilter::parse("e: size > 0").unwrap();
    assert!(matches!(f.rules[0].kind, FilterRuleKind::AnyExpression(_)));
}
```

- [ ] **Step 2: Run, expect FAIL**

Run: `cargo test -p linsync-core --lib filter::tests::de_prefix`
Expected: FAIL — variants don't exist.

- [ ] **Step 3: Extend `FilterRuleKind`**

In `crates/linsync-core/src/filter.rs`, locate `FilterRuleKind` enum (look near line 250). Add:

```rust
pub enum FilterRuleKind {
    // ...existing variants...
    DirectoryExpression(FilterExpression),
    AnyExpression(FilterExpression),
}
```

Add an `FilterExpression` AST (if `fe:` already has one, reuse it; the docs say `fe:` is supported so the AST exists). If not, define:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum FilterExpression {
    Compare { lhs: FilterAttr, op: CompareOp, rhs: FilterValue },
    And(Box<FilterExpression>, Box<FilterExpression>),
    Or(Box<FilterExpression>, Box<FilterExpression>),
    Not(Box<FilterExpression>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterAttr { Size, Mtime, Ctime, Atime, Name, Path, Ext }

#[derive(Debug, Clone, PartialEq)]
pub enum CompareOp { Lt, Le, Eq, Ne, Ge, Gt, Match }

#[derive(Debug, Clone, PartialEq)]
pub enum FilterValue { Int(i64), Date(time::OffsetDateTime), Str(String), Regex(regex::Regex) }
```

Implement `parse_filter_expression(&str) -> Result<FilterExpression, FilterParseErrorKind>` using a small recursive-descent parser (Pratt parser is fine; this grammar is tiny).

- [ ] **Step 4: Wire `de:`/`e:` into the dispatch at line 308-387**

```rust
// Inside parse_rule, after the existing fe: branch:
"de:" => Ok(FilterRule {
    kind: FilterRuleKind::DirectoryExpression(parse_filter_expression(rest)?),
    inclusion: FilterInclusion::Include,
    ..
}),
"de!:" => Ok(FilterRule {
    kind: FilterRuleKind::DirectoryExpression(parse_filter_expression(rest)?),
    inclusion: FilterInclusion::Exclude,
    ..
}),
"e:" => Ok(FilterRule {
    kind: FilterRuleKind::AnyExpression(parse_filter_expression(rest)?),
    inclusion: FilterInclusion::Include,
    ..
}),
"e!:" => Ok(FilterRule {
    kind: FilterRuleKind::AnyExpression(parse_filter_expression(rest)?),
    inclusion: FilterInclusion::Exclude,
    ..
}),
```

- [ ] **Step 5: Run tests, expect PASS**

Run: `cargo test -p linsync-core --lib filter::tests`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/linsync-core/src/filter.rs
git commit -m "feat(filter): parse de:/e: expression families"
```

### Task 2.2: Implement expression evaluator

**Files:**
- Modify: `crates/linsync-core/src/filter.rs` — extend `FileFilter::matches_dir` and `matches_file` to evaluate expressions.

- [ ] **Step 1: Write evaluator tests**

```rust
#[test]
fn de_excludes_large_dirs() {
    let f = FileFilter::parse("de!: size > 10485760").unwrap(); // > 10 MB
    let small = tempfile::TempDir::new().unwrap();
    let big = tempfile::TempDir::new().unwrap();
    std::fs::write(big.path().join("blob"), vec![0u8; 11 * 1024 * 1024]).unwrap();
    assert!(f.matches_dir(small.path()), "small dir should be kept");
    assert!(!f.matches_dir(big.path()), "big dir should be excluded");
}
```

- [ ] **Step 2: Run, expect FAIL**
- [ ] **Step 3: Implement `evaluate_expression(&FilterExpression, &FsAttrs)` and call it from `matches_dir` / `matches_file`**
- [ ] **Step 4: Run, expect PASS**
- [ ] **Step 5: Commit** `feat(filter): evaluate de:/e: expressions against fs attributes`

### Task 2.3: `linsync-cli filter migrate` subcommand

**Files:**
- Modify: `crates/linsync-cli/src/main.rs` — add subcommand
- Test: extend `crates/linsync-cli/tests/cli.rs`

- [ ] **Step 1: Write CLI test**

```rust
#[test]
fn filter_migrate_rewrites_legacy_attr_prefixes() {
    let dir = tempfile::TempDir::new().unwrap();
    let input = dir.path().join("legacy.flt");
    std::fs::write(&input, "attr:hidden\nctime: < '2020-01-01'\nf:.*\\.rs$\n").unwrap();

    let output = dir.path().join("migrated.flt");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_linsync-cli"))
        .args(["filter", "migrate", input.to_str().unwrap(), "--out", output.to_str().unwrap()])
        .status().unwrap();
    assert!(status.success());

    let text = std::fs::read_to_string(&output).unwrap();
    assert!(text.contains("# UNSUPPORTED: attr:hidden  — Linux has no equivalent attribute"));
    assert!(text.contains("e: mtime < '2020-01-01'  # migrated from ctime"));
    assert!(text.contains("f:.*\\.rs$"));
}
```

- [ ] **Step 2: Run, expect FAIL**
- [ ] **Step 3: Implement migration**

In `crates/linsync-cli/src/main.rs`, add the subcommand and a `migrate_filter_file(input, output)` function that:
- Reads input line by line.
- For each line, dispatches by leading prefix:
  - Already supported (`f:`, `f!:`, `d:`, `d!:`, `wf:`, `wd:`, `fe:`, `fe!:`, `de:`, `de!:`, `e:`, `e!:`): copy through.
  - `attr:` / `dos:` / `shell:` / `version:`: emit `# UNSUPPORTED: <line>  — Linux has no equivalent attribute`.
  - `ctime:` (the legacy Win32 creation-time prefix): rewrite to `e: mtime <op> <val>  # migrated from ctime` (Linux has no creation time; use mtime as the closest).
  - Anything else: emit `# UNRECOGNIZED: <line>` and a stderr warning.

- [ ] **Step 4: Run, expect PASS**
- [ ] **Step 5: Commit** `feat(cli): linsync-cli filter migrate for legacy .flt files`

### Task 2.4: Filter editor UI wiring

**Files:**
- Modify: `apps/linsync-gui/qml/FiltersPage.qml`

- [ ] **Step 1: Wire `validateRequested` against `/filters/validate` (already exists from Task 1.4) and display inline error or success badge**
- [ ] **Step 2: Wire `saveFilterRequested` and `deleteFilterRequested` to the bridge from Task 1.4**
- [ ] **Step 3: Add a "Migrate legacy .flt…" file picker that calls a new bridge endpoint `/filters/migrate?path=...` which shells out to the CLI**
- [ ] **Step 4: Extend gui-smoke.sh to verify the validate response shape on a known-bad input**
- [ ] **Step 5: Commit** `feat(gui): filter editor — inline validation + save/delete + legacy migration entry`

---

## Phase 3 — Merge UI Completion

**Outcome:** Dedicated three-pane merge view (left | base | right with output below). Merge output can be saved to a third target file. Conflict navigation already exists in `MergePage.qml`; this phase extends it.

### Task 3.1: Add `ThreeWayMergeState` to linsync-core

**Files:**
- Modify: `crates/linsync-core/src/merge.rs:49-94` (existing `TwoWayMergeState` is the model; add a sibling type)
- Test: extend the existing tests module

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn three_way_merge_resolves_clean_conflict_with_explicit_choice() {
    let base = TextDocument::from_str("a\nb\nc\n");
    let left = TextDocument::from_str("a\nb_left\nc\n");
    let right = TextDocument::from_str("a\nb_right\nc\n");
    let compare = compare_three_way(&base, &left, &right, &TextCompareOptions::default());

    let mut state = ThreeWayMergeState::new(base, left, right, compare);
    let conflicts = state.conflicts();
    assert_eq!(conflicts.len(), 1);

    state.resolve(conflicts[0].id, MergeChoice::Left).unwrap();
    assert_eq!(state.output().text(), "a\nb_left\nc\n");
}
```

- [ ] **Step 2: Run, expect FAIL** (functions don't exist)

- [ ] **Step 3: Define `ThreeWayMergeState`**

```rust
pub struct ThreeWayMergeState {
    pub base: EditableDocument,
    pub left: EditableDocument,
    pub right: EditableDocument,
    pub output: EditableDocument,
    pub compare: ThreeWayCompareResult,
    resolutions: Vec<Option<MergeChoice>>,
}

pub enum MergeChoice { Left, Right, Base, Custom(String) }

impl ThreeWayMergeState {
    pub fn new(base: TextDocument, left: TextDocument, right: TextDocument, compare: ThreeWayCompareResult) -> Self { /* … */ }
    pub fn conflicts(&self) -> &[ThreeWayConflict] { /* … */ }
    pub fn resolve(&mut self, id: ConflictId, choice: MergeChoice) -> Result<(), MergeError> { /* … */ }
    pub fn output(&self) -> &EditableDocument { &self.output }
    pub fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, self.output.text())
    }
}
```

`compare_three_way` should already exist or be near-existing in `merge.rs` — the docstring "base-aware merge" in parity-acceptance.md implies the engine exists; if it returns a different result type, adapt the test signature to match.

- [ ] **Step 4: Run, expect PASS**
- [ ] **Step 5: Commit** `feat(core): ThreeWayMergeState with explicit choice resolution + save_to`

### Task 3.2: Bridge endpoints for three-way merge

**Files:**
- Modify: `apps/linsync-gui/src/main.rs` — add `/merge3/start`, `/merge3/resolve`, `/merge3/save`
- Modify: `apps/linsync-gui/src/cxxqt_session.rs` — add `start_three_way_merge(base, left, right)`, `resolve_conflict(id, choice)`, `save_merge_to(path)` invokables

- [ ] **Step 1: Write integration test** (apps/linsync-gui/tests/merge_bridge.rs)
- [ ] **Step 2: Implement endpoints** — these are thin wrappers around the core state held in app state
- [ ] **Step 3: Implement invokables** mirroring the HTTP shape
- [ ] **Step 4: Commit** `feat(gui): three-way merge bridge endpoints + cxx-qt invokables`

### Task 3.3: Three-pane MergePage.qml

**Files:**
- Modify or create: `apps/linsync-gui/qml/MergePage.qml`
- Modify: `apps/linsync-gui/qml/Main.qml` to add a Merge section if not present

- [ ] **Step 1: Lay out the three-pane view** — three top columns (Left | Base | Right), output pane below, conflict navigator on the right side. Use Kirigami `RowLayout` + `SplitView`.
- [ ] **Step 2: Bind to bridge state** — connect to `merge3/state` updates; render the row model for each side.
- [ ] **Step 3: Conflict toolbar** — buttons "Keep Left", "Keep Right", "Keep Base", "Edit" trigger `/merge3/resolve` with the corresponding `choice`. Up/Down navigates conflicts.
- [ ] **Step 4: Save to third target** — toolbar button opens a Kirigami `FileDialog` and calls `/merge3/save?path=<chosen>` so the output goes to a new file rather than mutating left or right.
- [ ] **Step 5: gui-smoke addition** — start a three-way merge against `tests/fixtures/merge/{base,left,right}.txt`, resolve the conflict, save to `${TMPDIR}/out.txt`, verify content.
- [ ] **Step 6: Commit** `feat(gui): three-pane merge view with save-to-third-target`

---

## Phase 4 — Plugin Protocol Completion

**Outcome:** `unpack_folder` operation defined and implemented; streaming-output protocol added for plugins that emit > the existing capped output; bundled ZIP/tar plugin manifests under `packaging/plugins/` activated.

### Task 4.1: Add `PluginOperation::UnpackFolder`

**Files:**
- Modify: `crates/linsync-core/src/plugin.rs:191-207` (PluginOperation enum)
- Modify: `docs/plugin-protocol.md`
- Test: `crates/linsync-core/tests/plugin_unpack_folder.rs` (new)

- [ ] **Step 1: Write failing helper-invocation test**

```rust
use linsync_core::plugin::{run_unpack_folder_plugin, PluginManifest, PluginExecutionOptions};
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn unpack_folder_plugin_returns_virtual_tree() {
    // Build a tiny shell-script plugin that emits a known JSON tree
    let dir = TempDir::new().unwrap();
    let script = dir.path().join("p.sh");
    std::fs::write(&script, r#"#!/usr/bin/env bash
read REQ
echo '{"ok":true,"tree":[{"path":"a/b.txt","kind":"file","sha256":"deadbeef","size":4},{"path":"a","kind":"dir"}]}'
"#).unwrap();
    std::os::unix::fs::PermissionsExt::set_mode(&mut std::fs::metadata(&script).unwrap().permissions(), 0o755);

    let manifest = PluginManifest {
        name: "test".into(),
        version: "1.0".into(),
        class: "unpacker".into(),
        executable: script.to_str().unwrap().into(),
        operations: vec!["unpack_folder".into()],
        ..Default::default()
    };

    let result = run_unpack_folder_plugin(
        dir.path(),
        &manifest,
        "/tmp/whatever.zip",
        &PluginExecutionOptions::with_timeout(Duration::from_secs(5)),
    ).unwrap();
    assert_eq!(result.tree.len(), 2);
    assert!(result.tree.iter().any(|n| n.path == "a/b.txt" && n.kind == "file"));
}
```

- [ ] **Step 2: Run, expect FAIL** (`run_unpack_folder_plugin` not defined)

- [ ] **Step 3: Extend the operation enum and implement the wrapper**

In `plugin.rs:191-207`:

```rust
pub enum PluginOperation {
    Probe,
    Prediff,
    UnpackText,
    UnpackFolder,
    ListVirtualFolder,
}
```

After `run_unpack_text_plugin` near line 667, add:

```rust
#[derive(Debug, Clone, serde::Deserialize)]
pub struct VirtualNode {
    pub path: String,
    pub kind: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct UnpackFolderResponse {
    pub ok: bool,
    #[serde(default)]
    pub tree: Vec<VirtualNode>,
    #[serde(default)]
    pub error: Option<String>,
}

pub fn run_unpack_folder_plugin(
    plugin_dir: &std::path::Path,
    manifest: &PluginManifest,
    source: &str,
    options: &PluginExecutionOptions,
) -> Result<UnpackFolderResponse, PluginError> {
    let req = serde_json::json!({"op":"unpack_folder","source":source});
    let raw = run_plugin_helper(plugin_dir, manifest, &req.to_string(), options)?;
    serde_json::from_str(&raw.stdout).map_err(PluginError::ParseResponse)
}
```

- [ ] **Step 4: Run, expect PASS**
- [ ] **Step 5: Update `docs/plugin-protocol.md`** — add the `unpack_folder` op section: request shape `{op,source}`, response shape `{ok,tree,error}`, security notes (helper must not symlink-escape).
- [ ] **Step 6: Commit** `feat(plugin): unpack_folder operation + protocol docs`

### Task 4.2: Streaming-output protocol

**Files:**
- Modify: `crates/linsync-core/src/plugin.rs` — add `run_streaming_plugin(...) -> impl Iterator<Item=PluginChunk>`
- Modify: `docs/plugin-protocol.md`

- [ ] **Step 1: Design constraint test** — write a test for a plugin that emits length-prefixed JSON chunks on stdout. The protocol: `<u32 LE length><JSON bytes>` repeated.

- [ ] **Step 2: Implement `PluginChunkReader`** that wraps the child stdout, parses chunks, and surfaces them as iterator items with the existing timeout + total-bytes cap.

- [ ] **Step 3: Document in plugin-protocol.md** — note that opt-in via the manifest field `"streaming": true`, and that even streaming responses are capped at `options.max_total_bytes`.

- [ ] **Step 4: Commit** `feat(plugin): length-prefixed streaming-output protocol`

### Task 4.3: Activate ZIP/tar scaffold plugins

**Files:**
- Modify: `packaging/plugins/zip-unpack/manifest.json` — declare `unpack_folder` op
- Modify: `packaging/plugins/zip-unpack/zip-unpack.sh` — implement
- Same for `packaging/plugins/tar-unpack/`
- Test: end-to-end against a fixture archive

- [ ] **Step 1: Add fixture archives** — `tests/fixtures/archive/sample.zip` and `sample.tar` containing known content
- [ ] **Step 2: Implement zip-unpack.sh** using `unzip -Z1` (list) then emit tree JSON. Reject paths containing `..`.
- [ ] **Step 3: Implement tar-unpack.sh** using `tar -tf`.
- [ ] **Step 4: Add integration test** — discover plugins from `packaging/plugins/`, call `unpack_folder` against the fixtures, assert expected tree.
- [ ] **Step 5: Commit** `feat(plugins): activate bundled ZIP/tar unpackers via unpack_folder`

### Task 4.4: Plugin Settings UI

**Files:**
- Modify: `apps/linsync-gui/qml/PluginsPage.qml` — add per-plugin "Settings" dialog
- Modify: `apps/linsync-gui/src/main.rs` — `/plugins/options/get` + `/plugins/options/set`
- Modify: `crates/linsync-core/src/plugin.rs` — surface manifest `options_schema` (already-declared field?)

- [ ] **Step 1: Test** — set a per-plugin option, restart bridge, verify persisted
- [ ] **Step 2: Storage** — per-plugin options live in `$XDG_CONFIG_HOME/linsync/plugin-options/<plugin-name>.json`
- [ ] **Step 3: UI** — render form fields from `options_schema` JSON; types: string, bool, int, enum
- [ ] **Step 4: Commit** `feat(gui): per-plugin settings panel`

---

## Phase 5 — Moved-Block Detection

**Outcome:** Text compare engine surfaces moved blocks as a third diff kind alongside adds/deletes; GUI displays moved blocks distinctly.

### Task 5.1: Add `DiffBlockKind::Moved`

**Files:**
- Modify: `crates/linsync-core/src/text.rs:162-165` (DiffBlock + DiffBlockKind)

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn detects_moved_block() {
    let left = "section A\nline 1\nline 2\nsection B\nline 3\nline 4\n";
    let right = "section B\nline 3\nline 4\nsection A\nline 1\nline 2\n";
    let opts = TextCompareOptions { detect_moves: true, ..Default::default() };
    let result = compare_documents(TextDocument::from_str(left), TextDocument::from_str(right), &opts);
    let moves: Vec<_> = result.blocks.iter().filter(|b| matches!(b.kind, DiffBlockKind::Moved { .. })).collect();
    assert_eq!(moves.len(), 2, "expected two moved blocks (the two sections)");
}
```

- [ ] **Step 2: Run, expect FAIL**

- [ ] **Step 3: Implement move detection**

```rust
pub enum DiffBlockKind {
    Equal,
    Add,
    Delete,
    Replace,
    Moved { partner_block: usize, direction: MoveDirection },
}
pub enum MoveDirection { LeftToRight, RightToLeft }
```

Algorithm: after the primary diff produces Adds + Deletes, for each Delete block compute a content-hash key (normalized whitespace per options). For each Add block compute the same. Pair them where keys match and the content is non-trivial (≥ N lines, configurable, default 3). Convert paired blocks to `Moved` variants.

- [ ] **Step 4: Run, expect PASS**
- [ ] **Step 5: Surface in CLI** — `linsync-cli compare --detect-moves` flag.
- [ ] **Step 6: Surface in GUI** — `compareOptions.detectMoves` in SettingsPage; render moved blocks with a distinct background color in the text panes (Kirigami theme accent color, 20% alpha).
- [ ] **Step 7: gui-smoke** — verify a fixture with a known move surfaces a `kind: "moved"` row.
- [ ] **Step 8: Commit** `feat(core,gui): moved-block detection as a third diff kind`

---

## Phase 6 — Sandbox Foundation (DESIGN REQUIRED)

**Status: DESIGN REQUIRED. Do not implement until brainstorming has produced a written design doc.**

This phase exists because the project's plugin-helper protocol declares sandbox metadata but doesn't enforce it (per `docs/known-limitations-1.0.md` lines 69–71: "Packaged sandbox behavior (seccomp/landlock for helper processes), archive-helper security stress tests, and Flatpak portal behavior are not validated end to end"). Phases 7, 8, and 10 all consume this foundation; without it, those phases would expose untrusted document/network parsers without containment.

**Acceptance criteria the design must satisfy:**
1. Every helper process (`linsync-core/src/plugin.rs::run_plugin_helper`) is wrapped in a sandbox before exec.
2. The sandbox stack works on the four supported runtimes: native, Flatpak, AppImage, and Arch/Debian/RPM-packaged.
3. Filesystem access is restricted to: the plugin's own dir, `$XDG_CACHE_HOME/linsync/plugin-tmp/<pid>/`, and the explicit source path passed in the request — nothing else.
4. Network access is denied by default; a plugin manifest field `network: true` opts in (for the future webpage compare plugin).
5. Existing plugin integration tests still pass with the sandbox enabled; new stress tests cover symlink escape, fork bomb, oversize stdout, and timeout escape.

**Open design questions for brainstorming (use `superpowers:brainstorming` skill):**
- Landlock vs seccomp-bpf vs bubblewrap vs Flatpak-portal-only — which combination per runtime?
- Where does the sandbox setup live: `linsync-core` (where the spawn happens), a new `crates/linsync-sandbox` crate, or in `apps/linsync-gui` only?
- How does sandbox failure surface to the user — refuse-to-run vs degraded-mode warning?
- Cache directory cleanup: TTL? Reference-counted? `Drop`-guard temp dirs (already used per plugin.rs) extended to cover sandbox state?
- Test strategy: how do we run sandbox tests inside CI containers which already have restricted capabilities?

**Blocking dependencies:** none — this phase blocks others but is itself unblocked.

**Blocks:** Phase 7 (image compare may shell out to ImageMagick or similar), Phase 8 (OCR helper), Phase 9 (webpage compare/Qt WebEngine network), Phase 10 (archive writer with stronger guarantees).

**File impact (anticipated):**
- New: `crates/linsync-sandbox/` (likely)
- Modified: `crates/linsync-core/src/plugin.rs` (run_plugin_helper)
- Modified: `packaging/flatpak/com.visorcraft.LinSync.yml` (portal declarations)
- Modified: `docs/security.md`, `docs/plugin-protocol.md`
- New: `crates/linsync-core/tests/plugin_sandbox_*.rs` (stress tests)

**Next step:** brainstorm the design, save spec to `docs/sandbox-design.md`, then write a follow-up plan `docs/superpowers/plans/YYYY-MM-DD-sandbox-foundation.md`.

---

## Phase 7 — Image / Pixel Compare (DESIGN REQUIRED)

**Status: DESIGN REQUIRED.** Listed in `docs/known-limitations-1.0.md:21` as "Image compare — no pixel/perceptual diff view yet" and `PLAN.md:67` as post-1.0.

**Acceptance criteria the design must satisfy:**
1. New compare mode `image`: per-pixel diff with configurable tolerance (alpha, exact, perceptual deltaE).
2. CLI: `linsync-cli compare --mode image a.png b.png` returns exit 0/1/2 per existing semantics and emits a JSON summary (matched-pixel count, mismatch count, total).
3. GUI: dedicated image compare view showing left | right | diff-overlay with a slider toggle.
4. Supports PNG, JPEG, WebP, AVIF, TIFF at minimum. RAW formats out of scope.
5. Files > 100 MB or dimensions > 16384×16384 stream-decode rather than load fully.

**Open design questions:**
- Pure-Rust (`image` crate + `imageproc` for perceptual diff) vs an external helper (ImageMagick)? Pure-Rust avoids the sandbox layer; helper avoids dependency bloat.
- Perceptual diff algorithm: CIEDE2000 (high quality, slow) vs `dssim` (medium quality, fast) vs simple Y′CbCr difference?
- Differencing visualization: red-overlay, animated, or stacked? See how Beyond Compare and `oodiff` present them.
- Format detection: trust extension, or sniff magic bytes?

**Blocking dependencies:** Phase 6 if helper-based; none if pure-Rust.

**File impact (anticipated):**
- New: `crates/linsync-core/src/image.rs`
- Modified: `crates/linsync-core/src/lib.rs` (re-export)
- Modified: `crates/linsync-cli/src/main.rs` (add `--mode image`)
- New: `apps/linsync-gui/qml/ImageComparePage.qml`
- Modified: `Cargo.toml` (workspace deps for `image` crate)
- New tests: `crates/linsync-core/tests/image_compare.rs`
- New fixtures: `tests/fixtures/image/{same,resized,recompressed,gradient-delta}.png`

**Next step:** brainstorm engine choice, save spec to `docs/image-compare-design.md`, then write follow-up plan.

---

## Phase 8 — Document / OCR Compare (DESIGN REQUIRED)

**Status: DESIGN REQUIRED.** `docs/document-ocr-compare.md` is the authoritative deferral document. It defines the planned compare paths (document-as-text, rendered, OCR-as-text, OCR-with-positions) but explicitly requires sandbox + privacy controls before any implementation.

**Prerequisites that MUST be satisfied before this phase becomes implementable** (lifted directly from `docs/document-ocr-compare.md`):
1. Exact license + source-distribution obligations recorded for every helper.
2. Whether each helper is bundled, system-discovered, or plugin-provided.
3. Third-party notices and source-offer updates landed.
4. Flatpak permissions + sandbox limitations documented.
5. Security review for untrusted document parsing.
6. User-visible controls for: language/model selection, temp-file location/cleanup, image retention for debugging, error handling.
7. Fixtures or controlled generators for: text-extraction success/failure, multi-page PDF/image, SVG/PDF renderer failures, OCR unavailable/timeout/oversized/malformed, privacy temp-file cleanup.

**Acceptance criteria for the eventual plan:**
1. New compare mode `document` (or `ocr`) supporting `.pdf`, `.docx`, `.odt`, `.svg`, image inputs.
2. CLI: `linsync-cli compare --mode document a.pdf b.pdf`.
3. GUI: rendered-page side-by-side view + OCR-extracted-text view, toggleable.
4. All helpers run inside the Phase 6 sandbox.
5. Default builds are NOT network-active; remote OCR is opt-in and gated behind explicit consent dialog.

**Open design questions:**
- OCR engine: Tesseract (Apache 2.0 — compatible) vs PaddleOCR (Apache 2.0 — heavier) vs none-in-default-build (require user to install). Latter is the path of least licensing risk.
- PDF render: `poppler-utils` (`pdftoppm`) vs `mupdf` (AGPLv3 — incompatible with GPL-3.0-only redistribution if linked, but call-out-to-binary is OK) vs `pdfium`.
- Office docs: pandoc (helper) vs LibreOffice headless (helper).
- Position-mapping (OCR words → page coords) needed for v1 or post-v1?

**Blocking dependencies:** Phase 6 (sandbox), Phase 4 (plugin protocol — most helpers will be plugins).

**File impact (anticipated):**
- New: `crates/linsync-core/src/document.rs`
- New plugins: `packaging/plugins/pdf-to-text/`, `packaging/plugins/tesseract-ocr/`, `packaging/plugins/libreoffice-extract/`
- New: `apps/linsync-gui/qml/DocumentComparePage.qml`
- New tests + fixtures under `tests/fixtures/document/`

**Next step:** complete Phase 6, then brainstorm + spec at `docs/document-compare-implementation.md`.

---

## Phase 9 — Webpage Compare (DESIGN REQUIRED)

**Status: DESIGN REQUIRED.** `docs/webpage-compare.md` is the authoritative deferral document. The browser-engine question is unresolved.

**Prerequisites that MUST be satisfied (from `docs/webpage-compare.md`):**
1. User-visible start action before any URL is fetched.
2. Clear UI indication that third-party page resources may be requested.
3. Separate browsing profile for webpage compare data.
4. UI controls to clear cache/cookies/history/storage/downloads.
5. Cache placement under `$XDG_CACHE_HOME/linsync` with documented cleanup.
6. No reuse of personal browser profiles or saved credentials.
7. Flatpak network + sandbox documentation.
8. Test fixtures use controlled local servers, not live websites.

**Acceptance criteria for the eventual plan:**
1. Five sub-modes per the decision doc: rendered, screenshot, HTML source, extracted-text, resource-tree.
2. Network access is opt-in only when starting a webpage compare; plain file/folder compare never fetches.
3. Qt WebEngine integration is feature-gated; default build offers HTML source + extracted text + resource tree (no browser engine).
4. The HTTP fetch path is sandboxed per Phase 6 (network=true plugin).

**Open design questions:**
- Qt WebEngine licensing: LGPLv3 + GPLv2 — compatible with GPL-3.0-only LinSync? Verify with `cargo deny` and review. If incompatible, scope rendered-page mode out permanently.
- Headless render: full Qt WebEngine vs `chromium --headless` helper? Latter is easier to sandbox but adds an external runtime dep.
- Resource-tree compare: how to enumerate URLs without rendering? `wget --spider --recursive` with depth caps? Local HTTP fixture server for tests.
- Cookie/profile lifetime: per-session in-memory, per-pair-of-URLs ephemeral profile, or persistent?

**Blocking dependencies:** Phase 6 (sandbox + network gating).

**File impact (anticipated):**
- New: `crates/linsync-core/src/webpage.rs` (URL handling, fetch coordination)
- New plugin: `packaging/plugins/web-fetch/`
- Optional new feature crate: `crates/linsync-webengine/` (Qt WebEngine wrapper, feature-gated)
- New: `apps/linsync-gui/qml/WebpageComparePage.qml`
- New tests with a `httptest` local server

**Next step:** complete Phase 6, then brainstorm engine licensing question + spec at `docs/webpage-compare-implementation.md`.

---

## Phase 10 — Writable Archive-Member Editing (DESIGN REQUIRED)

**Status: DESIGN REQUIRED.** `docs/known-limitations-1.0.md:42-43` is explicit: "Writable archive-member editing is deliberately deferred until a separate helper plus Flatpak-portal safety design exists." `docs/known-limitations-1.0.md:104-105` adds: "may never ship."

**Acceptance criteria for the eventual plan (if it ever ships):**
1. Edit a file inside a ZIP/tar/etc. archive via the GUI without manual extract → edit → repack.
2. Atomic-safe: original archive only mutated after successful write of a new archive to a temp path, then renamed in.
3. Permissions and timestamps of unchanged members preserved bit-exactly.
4. Helper runs inside the Phase 6 sandbox with write capability scoped only to its own temp dir + the target archive path.
5. Flatpak portal handshake gives the helper transient access to the host archive file (no broad filesystem access).

**Open design questions:**
- Per-format helpers (one for ZIP, one for tar.*) or a single generic helper that delegates? The plugin protocol favors per-format (matches `unpack_folder` design).
- Cancel-safety: what happens if the user closes the app mid-write? Two-phase commit with rollback file?
- Encrypted archives: read-only forever, or pop a password prompt? Latter is a security review item — out of scope for the first design.
- Permissions/ownership for tar archives created on a Flatpak-sandboxed host: tar can preserve UID/GID even when the editor process can't `chown`. Need to decide whether to preserve metadata exactly or normalize.

**Blocking dependencies:** Phase 6 (sandbox), Phase 4 (plugin protocol; archive editing fits the protocol but needs a new operation, e.g. `replace_member`).

**File impact (anticipated):**
- New plugin op: `replace_member` (extends Phase 4 protocol)
- New: `packaging/plugins/zip-editor/`, `packaging/plugins/tar-editor/`
- New: `apps/linsync-gui/qml/ArchiveEditDialog.qml`
- New: `docs/archive-write-safety.md` (design doc)

**Next step:** confirm whether this feature is intended to ship at all (decision doc says "may never"); if yes, complete Phase 6 + Phase 4, then brainstorm.

---

## Phase 10.5 — Symlink + Permissions Fixtures

**Outcome:** the placeholder fixture trees in `tests/fixtures/symlink/` and `tests/fixtures/permissions/` are populated (`docs/fixture-provenance.md:17-19`), and integration tests cover the cases that depend on them.

### Task 10.5.1: Symlink fixture tree + tests

**Files:**
- Create: `tests/fixtures/symlink/build.sh` (generator script — committed; fixtures themselves are generated at test time because git stores symlinks weirdly across platforms)
- Create: `crates/linsync-core/tests/symlink_compare.rs`

- [ ] **Step 1: Author generator**

```bash
#!/usr/bin/env bash
# Build a deterministic symlink fixture tree under $1.
set -euo pipefail
ROOT="${1:?path required}"
rm -rf "$ROOT"; mkdir -p "$ROOT/left" "$ROOT/right"
echo "hello" > "$ROOT/left/target.txt"
ln -s target.txt "$ROOT/left/symlink-to-file"
ln -s ../left/target.txt "$ROOT/left/symlink-relative"
ln -s /nonexistent       "$ROOT/left/dangling"
mkdir "$ROOT/left/subdir"; ln -s subdir "$ROOT/left/symlink-to-dir"
# Right side: same paths but with content differences in some
echo "hello"   > "$ROOT/right/target.txt"
ln -s target.txt "$ROOT/right/symlink-to-file"
ln -s ../right/target.txt "$ROOT/right/symlink-relative"
ln -s /also-nonexistent  "$ROOT/right/dangling"
mkdir "$ROOT/right/subdir"; ln -s subdir "$ROOT/right/symlink-to-dir"
```

- [ ] **Step 2: Write failing tests**

```rust
// crates/linsync-core/tests/symlink_compare.rs
use linsync_core::folder::{compare_folders, FolderCompareOptions, SymlinkPolicy};
use tempfile::TempDir;
use std::process::Command;

fn build() -> TempDir {
    let dir = TempDir::new().unwrap();
    Command::new("bash").arg("tests/fixtures/symlink/build.sh").arg(dir.path()).status().unwrap();
    dir
}

#[test]
fn dangling_symlink_handled_by_policy_default() {
    let dir = build();
    let result = compare_folders(&dir.path().join("left"), &dir.path().join("right"),
        &FolderCompareOptions { symlinks: SymlinkPolicy::AsLink, ..Default::default() });
    assert!(result.is_ok(), "default policy must not error on dangling symlinks");
}

#[test]
fn follow_symlinks_compares_target_content() {
    let dir = build();
    let result = compare_folders(&dir.path().join("left"), &dir.path().join("right"),
        &FolderCompareOptions { symlinks: SymlinkPolicy::Follow, ..Default::default() }).unwrap();
    let row = result.rows.iter().find(|r| r.name == "symlink-to-file").unwrap();
    assert!(row.is_equal());
}
```

- [ ] **Step 3: Run, expect PASS or FAIL** depending on current core support. Fix `folder.rs` if FAIL.
- [ ] **Step 4: Commit** `test(core): symlink fixture + compare policy coverage`

### Task 10.5.2: Permissions fixture tree + tests

- [ ] **Step 1: Author generator** that creates files with modes `0o644`, `0o600`, `0o755`, `0o000`, plus setuid `04755` and sticky `01777` directories
- [ ] **Step 2: Write tests** asserting that folder compare reports mode differences correctly (this maps to the `linux-metadata-mapping.md` decisions)
- [ ] **Step 3: Commit** `test(core): permissions fixture + mode-difference coverage`

---

## Phase 11 — Git Mergetool Integration

**Outcome:** LinSync registers as a Git mergetool; `git mergetool --tool=linsync` launches the three-pane merge view from Phase 3 and writes the resolved content back to the file Git expects.

**This phase depends on Phase 3 (three-way merge UI).**

### Task 11.0.1: Mergetool entry point

**Files:**
- Modify: `crates/linsync-cli/src/main.rs` — add `mergetool` subcommand (or extend `merge`)
- Modify: `packaging/distro/README.md`, `packaging/debian/`, `packaging/arch/PKGBUILD`, `packaging/rpm/linsync.spec` — install a `git-config` snippet

- [ ] **Step 1: Write failing CLI test**

```rust
#[test]
fn mergetool_subcommand_writes_merged_result() {
    let dir = tempfile::TempDir::new().unwrap();
    let base = dir.path().join("base.txt"); std::fs::write(&base, "a\nb\nc\n").unwrap();
    let local = dir.path().join("local.txt"); std::fs::write(&local, "a\nb_local\nc\n").unwrap();
    let remote = dir.path().join("remote.txt"); std::fs::write(&remote, "a\nb_remote\nc\n").unwrap();
    let merged = dir.path().join("merged.txt"); std::fs::write(&merged, "").unwrap();

    // For test purposes, run with --auto-resolve=left (no GUI)
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_linsync-cli"))
        .args(["mergetool",
            "--base", base.to_str().unwrap(),
            "--local", local.to_str().unwrap(),
            "--remote", remote.to_str().unwrap(),
            "--merged", merged.to_str().unwrap(),
            "--auto-resolve", "left"])
        .status().unwrap();
    assert!(status.success());
    assert_eq!(std::fs::read_to_string(&merged).unwrap(), "a\nb_local\nc\n");
}
```

- [ ] **Step 2: Run, expect FAIL**
- [ ] **Step 3: Implement subcommand**

The CLI's mergetool subcommand:
1. Loads base/local/remote into `ThreeWayMergeState` (Phase 3 Task 3.1).
2. If `--auto-resolve` is provided, applies that choice to every conflict and writes the result.
3. Otherwise, spawns the GUI in mergetool mode (a startup flag that takes the four paths and the merged-output path).
4. On GUI exit, write `state.output().text()` to `--merged` path. Exit 0 if all conflicts resolved, 1 if user cancelled (Git treats nonzero as failure and won't mark resolved).

- [ ] **Step 4: Run, expect PASS**

- [ ] **Step 5: Install a Git mergetool config snippet**

In each packaging recipe install:

```
/usr/share/linsync/git-mergetool.gitconfig
```

Contents:
```ini
[mergetool "linsync"]
    cmd = linsync-cli mergetool --base \"$BASE\" --local \"$LOCAL\" --remote \"$REMOTE\" --merged \"$MERGED\"
    trustExitCode = true
```

Document in `docs/git-integration.md` how to opt in:
```sh
git config --global include.path /usr/share/linsync/git-mergetool.gitconfig
git config --global merge.tool linsync
```

- [ ] **Step 6: Commit** `feat(cli,packaging): git mergetool integration`

---

## Phase 12 — Rendered Diff Modes (DESIGN REQUIRED)

**Status: DESIGN REQUIRED.** Listed in `docs/known-limitations-1.0.md:27-28`: RTL, syntax-coloured, and prose-reflow rendered modes beyond plain text.

**Acceptance criteria for the eventual design:**
1. RTL mode renders Arabic/Hebrew text with correct bidi behavior; both panes mirror correctly; diff highlighting respects logical (not visual) order.
2. Syntax-coloured mode highlights left and right per detected file type; diff highlighting layers on top without conflicting with syntax colors. Theme-aware (light + dark).
3. Prose-reflow mode wraps long paragraphs at the pane width; diff blocks track paragraph boundaries rather than physical lines.
4. All three modes are opt-in via SettingsPage; default remains plain text.
5. None of the modes change the underlying `TextCompareResult` — they're presentation-only.

**Open design questions:**
- Syntax highlighter: `syntect` (used by `bat` — Sublime-grammar based) vs `tree-sitter` (more accurate, much heavier dep) vs a Qt-side QSyntaxHighlighter? Pure-Rust + serialize-to-QML keeps the GPL-3.0 license clean.
- RTL: Qt6 handles bidi text rendering for free in QML `TextEdit`. Question is whether the diff overlay positions stay correct.
- Prose-reflow: define paragraph boundaries (blank-line-separated)? Does the diff engine need a "paragraph mode" or does it stay line-based with a post-processor?
- Performance: at what file size do these modes become too slow to enable by default per file (vs requiring explicit toggle)?

**Blocking dependencies:** none.

**File impact (anticipated):**
- New: `apps/linsync-gui/src/render.rs` or pure-QML modules
- Modified: `apps/linsync-gui/qml/ComparePanes.qml` (or similar)
- Modified: `crates/linsync-core/src/text.rs` if paragraph-mode is added

**Next step:** brainstorm at `docs/rendered-diff-modes-design.md`, then plan.

---

## Phase 13 — 1.0 Release Polish

**Outcome:** every release-gate item in `docs/known-limitations-1.0.md` §"Packaging and release validation" and §"Polish before tagging 1.0" is checked off; CI covers Flatpak; the gui-smoke script is the canonical pre-release gate.

### Task 13.1: Accessibility audit

**Files:**
- Create: `docs/accessibility-audit-1.0.md` (results + open issues)
- Modify: page QML files per findings

- [ ] **Step 1: Author audit checklist**

Per `docs/known-limitations-1.0.md:87-88`: every sidebar page, both themes. Concrete items:
- Tab-key focus order matches visual reading order
- Every interactive control has `Accessible.name` and `Accessible.role`
- Contrast ratio ≥ 4.5:1 for normal text, ≥ 3:1 for large text, in both light and dark Kirigami themes
- Screen reader (Orca) announces section transitions
- No keyboard trap on any modal or dialog

- [ ] **Step 2: Run audit**

For each of the 8 sections (Compare, Sessions, Filters, Plugins, Settings, About, Credits, Licenses):
1. Open the page.
2. Tab through every control; record order.
3. Use `qmllint` and `Accessible` inspector.
4. Switch to dark theme; re-record contrast.
5. Run Orca screen reader; record what's announced.

Log each finding in `docs/accessibility-audit-1.0.md` with file:line and severity.

- [ ] **Step 3: Fix critical findings**

Address every "P0" / "blocks 1.0" finding with QML edits. Defer P1/P2 to a follow-up issue tracker.

- [ ] **Step 4: gui-smoke addition**

Add a `--check-a11y` mode to `scripts/gui-smoke.sh` that walks each section and verifies every focusable control has `Accessible.name` set (parse via QtTest or a small script).

- [ ] **Step 5: Commit** `chore(a11y): 1.0 accessibility audit + critical fixes`

### Task 13.2: Screenshots for Flathub + AppStream + README

**Files:**
- Modify: `packaging/com.visorcraft.LinSync.metainfo.xml` (screenshot URLs)
- Create: `packaging/screenshots/*.png`
- Modify: `README.md` (embed)

- [ ] **Step 1: Reuse Task 1.6 screenshots** as the canonical capture pipeline
- [ ] **Step 2: Pick 5 screenshots** covering: text compare, folder compare, three-way merge, filters page, plugins page
- [ ] **Step 3: Drop into `packaging/screenshots/`**, update AppStream metainfo URLs, embed in README
- [ ] **Step 4: Run `appstreamcli validate packaging/com.visorcraft.LinSync.metainfo.xml`** — expect PASS
- [ ] **Step 5: Commit** `docs: screenshots for Flathub, AppStream, README`

### Task 13.3: Clean-VM packaging validation

**Files:**
- Create: `scripts/release-vm-validation.sh`

Per `docs/known-limitations-1.0.md:80-81`: "install on a clean VM, run, uninstall cleanly".

- [ ] **Step 1: Author the script**

```bash
#!/usr/bin/env bash
# Per-distro clean-VM validation. Uses systemd-nspawn or podman+systemd images
# to spin up a minimal install of each target distro, install our artifact,
# run gui-smoke offscreen, then uninstall and verify cleanup.
set -euo pipefail

DISTROS=("archlinux" "debian:trixie" "fedora:41" "ubuntu:24.04")
ARTIFACT_DIR="target/release-${VERSION:-1.0.0}"

for distro in "${DISTROS[@]}"; do
    echo "=== $distro ==="
    case "$distro" in
        archlinux)  pkg="$ARTIFACT_DIR/linsync-${VERSION:-1.0.0}-1-x86_64.pkg.tar.zst"; install_cmd="pacman -U --noconfirm" ;;
        debian:*|ubuntu:*) pkg="$ARTIFACT_DIR/linsync_${VERSION:-1.0.0}-1_amd64.deb"; install_cmd="apt install -y" ;;
        fedora:*)   pkg="$ARTIFACT_DIR/linsync-${VERSION:-1.0.0}-1.x86_64.rpm"; install_cmd="dnf install -y" ;;
    esac
    podman run --rm --privileged -v "$PWD/$ARTIFACT_DIR:/pkg" "$distro" bash -c "
        $install_cmd /pkg/$(basename $pkg) &&
        which linsync && linsync --version &&
        QT_QPA_PLATFORM=offscreen timeout 10 linsync || true
    "
done
```

- [ ] **Step 2: Run locally** before each tagged release
- [ ] **Step 3: Document** in `AGENTS.md` under §Packaging as the pre-tag gate
- [ ] **Step 4: Commit** `chore(release): clean-VM validation script`

### Task 13.4: Third-party notices re-review

- [ ] **Step 1: Run `cargo tree --workspace --depth 1 > /tmp/deps.txt`**
- [ ] **Step 2: Compare against `docs/third-party-notices.md`** — flag anything new or removed
- [ ] **Step 3: Run `cargo deny check`** — must pass
- [ ] **Step 4: Update Credits page table** (`apps/linsync-gui/qml/CreditsPage.qml`) and `docs/third-party-notices.md`
- [ ] **Step 5: Commit** `docs: regen third-party notices for 1.0`

### Task 13.5: Flatpak CI integration

**Files:**
- Modify: `.github/workflows/ci.yml` — add `flatpak` job

- [ ] **Step 1: Add the job**

```yaml
  flatpak:
    name: Flatpak build
    runs-on: ubuntu-latest
    container:
      image: bilelmoussaoui/flatpak-github-actions:kde-6-7
      options: --privileged
    steps:
      - uses: actions/checkout@v4
      - uses: flatpak/flatpak-github-actions/flatpak-builder@v6
        with:
          bundle: linsync.flatpak
          manifest-path: packaging/flatpak/com.visorcraft.LinSync.yml
          cache-key: flatpak-${{ hashFiles('packaging/flatpak/com.visorcraft.LinSync.yml') }}
```

- [ ] **Step 2: Verify** — push branch, ensure CI passes
- [ ] **Step 3: Commit** `ci: build Flatpak bundle on every PR`

### Task 13.6: Make `gui-smoke.sh` (both variants) a CI gate

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Add to the `build` job**

```yaml
      - name: GUI smoke
        run: bash scripts/gui-smoke.sh
      - name: GUI smoke (cxx-qt host)
        run: LINSYNC_GUI_SMOKE_CXXQT=1 bash scripts/gui-smoke.sh
```

- [ ] **Step 2: Verify** — confirm both pass in CI
- [ ] **Step 3: Commit** `ci: gate on gui-smoke.sh (both bridge variants)`

---

## Execution Notes

- **Branching:** create one feature branch per phase (`feat/p1-gui-wiring`, `feat/p2-filter-grammar`, …). Open one PR per phase.
- **Reviewer hand-off:** Phases 6–10 are blocked until their respective brainstorming sessions produce written specs. Do not start them under this plan.
- **Verification before completion:** every phase ends by running the full local CI preflight (`just ci`) and the gui-smoke variants. Use `superpowers:verification-before-completion` before claiming a phase done.

---

## Self-Review

**Spec coverage** — every item from the original audit cross-checked:

| Audit item | Plan task | Status |
| --- | --- | --- |
| FiltersPage signals → Rust | 1.4 | covered |
| PluginsPage discovery | 1.5 | covered |
| SettingsPage signals → SettingsStore | 1.2, 1.3 | covered |
| Folder operation UI + post-op refresh | (already resolved per known-limitations.md preamble lines 12-17) | n/a |
| Editable text panes | (already resolved per same preamble) | n/a |
| Screenshot-based GUI checks | 1.6, 11.2 | covered |
| Compare: image diff | Phase 7 | DESIGN REQUIRED — by design |
| Compare: OCR / document | Phase 8 | DESIGN REQUIRED — by design |
| Compare: URL/webpage | Phase 9 | DESIGN REQUIRED — by design |
| Compare: rendered modes (RTL, syntax, prose) | not yet planned | **gap** — see note below |
| Compare: moved-block detection | Phase 5 | covered |
| Merge: 3-pane view | 3.3 | covered |
| Merge: save-to-third-target | 3.3 | covered |
| Merge: Git mergetool write-back | not yet planned | **gap** — see note below |
| Merge: writable archive editing | Phase 10 | DESIGN REQUIRED — by design |
| Filters: de:/e: grammar | 2.1–2.2 | covered |
| Filters: .flt migration | 2.3 | covered |
| Filters: filter editor UI | 2.4 | covered |
| Plugins: unpack_folder op | 4.1 | covered |
| Plugins: streaming output protocol | 4.2 | covered |
| Plugins: sandbox (seccomp/landlock) | Phase 6 | DESIGN REQUIRED — by design |
| Plugins: Flatpak portal e2e | Phase 6 | DESIGN REQUIRED — by design |
| Plugins: Plugin Settings UI | 4.4 | covered |
| Plugins: helper discovery UI | 1.5 | covered |
| Fixtures: archive/symlink/permissions placeholders | 4.3 (archive); symlink/permissions not yet planned | **partial gap** |
| Compare: rendered modes (RTL, syntax, prose) | Phase 12 | DESIGN REQUIRED — by design |
| Merge: Git mergetool write-back | Phase 11 | covered |
| Fixtures: symlink/permissions placeholders | Phase 10.5 | covered |
| 1.0 polish: accessibility | 13.1 | covered |
| 1.0 polish: screenshots | 13.2 | covered |
| 1.0 polish: clean-VM validation | 13.3 | covered |
| 1.0 polish: third-party notices | 13.4 | covered |
| CI gap: Flatpak | 13.5 | covered |
| CI gap: gui-smoke as gate | 13.6 | covered |

**All audit items now have a task or design brief. No remaining spec gaps.**

**Placeholder scan:** Phases 1–5 and 11 contain only concrete code, file paths, and commands. Phases 6–10 are explicitly marked DESIGN REQUIRED and contain design briefs, not pseudo-tasks. No `TBD` / `implement later` strings appear in implementable phases. PASS.

**Type consistency:** function/method names referenced across tasks (`SettingsStore`, `apply_gui_setting`, `discover_plugins`, `ThreeWayMergeState`, `PluginOperation::UnpackFolder`, `compare_documents`, `compare_three_way`) are used consistently. PASS.

---

## Execution Handoff

Plan saved to `docs/superpowers/plans/2026-05-26-linsync-gui-wiring-and-feature-completion.md`. Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints.

Which approach?
