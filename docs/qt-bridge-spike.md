# Qt Bridge Spike

This note records the bridge rules for the future Qt 6/QML/Kirigami shell. It
does not claim the GUI is implemented; it defines the architecture boundaries
the implementation must satisfy.

## Preferred Bridge

`cxx-qt` remains the preferred bridge between the Rust core and QML because it
keeps domain logic in Rust while presenting Qt-friendly objects and models to
the UI.

Before committing large UI surfaces to `cxx-qt`, validate:

- QObjects for compare/session state.
- List models for recent paths, tabs, filters, and plugin lists.
- Table/tree models for folder compare rows.
- Editor state models for line numbers, diff blocks, inline spans, dirty state,
  and read-only side state.
- Progress, cancellation, and error delivery for long-running compares.

## Validation Result - 2026-05-17

A throwaway Cargo probe outside the workspace validated the first bridge gate
against the local Linux toolchain:

- Environment: Qt 6.11.1, `qml6`, `qmake6`, `QT_VERSION_MAJOR=6`, and
  `cxx-qt`/`cxx-qt-build`/`cxx-qt-lib` 0.8.1.
- Command: `QT_VERSION_MAJOR=6 cargo check` in `/tmp/linsync-cxxqt-probe`.
- Repository smoke: `apps/linsync-gui` also provides a `cxxqt-smoke` feature
  that builds `src/cxxqt_smoke.rs` and `qml/CxxQtSmoke.qml` as a real QML
  module. `QT_VERSION_MAJOR=6 cargo check -p linsync --features cxxqt-smoke`
  and `QT_VERSION_MAJOR=6 cargo clippy -p linsync --features cxxqt-smoke
  --all-targets -- -D warnings` pass in the current environment.
- QObject coverage: generated a `CompareSession` QObject with a
  `difference_count` property and invokable setter.
- List-model coverage: generated a `RecentPathModel` subclass of
  `QAbstractListModel` with `rowCount`, `data`, and `roleNames` overrides.
- Table-model coverage: generated a `FolderRowModel` subclass of
  `QAbstractTableModel` with `rowCount`, `columnCount`, and `data` overrides.
- GUI-thread handoff coverage: enabled `cxx_qt::Threading` and compiled a
  background-thread method that uses `CxxQtThread::queue` to mutate the QObject
  on the Qt thread.
- Result: the probe completed successfully. The build emitted a non-fatal
  `-Wsfinae-incomplete` warning from the Qt/GCC header combination around
  `QChar`, but code generation, Qt header discovery, model inheritance,
  QVariant/QString data return, and linking all passed.

Decision: keep `cxx-qt` as the preferred in-process bridge for the durable
compare/session model. Keep the current local HTTP/JSON bridge only as a
temporary compatibility path for the already-working QML launcher and two-path
compare requests. Do not expand the HTTP bridge into the long-lived app model
unless a later editable-editor or virtualized-folder-table spike fails in a way
that `cxx-qt` cannot address cleanly.

## Repository Integration - 2026-05-17

`apps/linsync-gui` now has a feature-gated in-process Qt host:

- Feature: `cxxqt-app`.
- Host: the `linsync` binary constructs `QGuiApplication` and
  `QQmlApplicationEngine`, initializes the `com.visorcraft.LinSync` QML module,
  and loads the existing `Main.qml` component through the Qt resource system.
- Bridge object: both the in-process cxx-qt host and the external `qml6`
  host drive the UI over the same local HTTP/JSON bridge. An in-process
  `LinSyncSessionBridge` QObject transport was removed as dead code (it was
  never registered with the QML engine).
- Fallback: the default build still launches QML through `qml6`/`qml` and uses
  the local HTTP/JSON bridge. A `cxxqt-app` build can force that route with
  `LINSYNC_QML_HOST=external`.
- Smoke: `LINSYNC_GUI_SMOKE_CXXQT=1 bash scripts/gui-smoke.sh` runs text and
  folder fixtures through the in-process host when Qt development headers and
  `qmake6` are installed.

This is not the final typed model layer. The current QObject removes HTTP from
the validated `cxx-qt` path and exposes active tab id, tab count, side paths,
mode, status, difference count, dirty flags, active validation
compatible/message/path-kind fields, indexed active summary labels/values,
indexed recent paths, and indexed tab id/title/dirty/undo/redo metadata. The same
bridge now subclasses `QAbstractListModel` with active left/right row-pair roles
for row id, number, text, and state, and QML visible panes consume that model
when `sessionBridge` is present. Full tab payloads, inactive tab rows, the local
fallback bridge payloads, and durable row/table models still use launch-context
JSON so the existing QML workspace can be reused. The next bridge work is to add
typed `cxx-qt` list and table models for full tabs, inactive compare rows,
folder rows, and richer recent/session state.

## Bridge Hardening - 2026-05-18

The local HTTP/JSON bridge accepts only loopback callers. The HTTP layer:

- binds to `127.0.0.1:0` (random port, never an externally routable address);
- emits no `Access-Control-Allow-Origin` header, so a browser tab on the user's
  machine cannot use the bridge as a read/write primitive against arbitrary
  files;
- rejects any request whose `Origin:` header is present and not loopback
  (`localhost`, `127.0.0.1`, `[::1]`, `::1`) with HTTP 403;
- caps each connection at a 5-second read/write timeout, 32 KiB of request
  body, and 64 header lines so a stalled or oversized client cannot block
  the single-threaded accept loop or exhaust memory;
- forbids row indices in `/copy` that would force the pane row vectors to
  grow by more than 1024 entries past the current size — the previous
  behavior would loop allocating blank rows for any `row` value.

The launch-context JSON (`$XDG_CACHE_HOME/linsync/gui/launch-<pid>.json`)
which carries every open file path is written mode `0o600` so other local
users cannot read it.

These rules apply only to the HTTP fallback path. The `cxxqt-app` host has
no socket and is not subject to them.

## QML Shell Sections - 2026-05-19

The QML shell hosts eight sections in a single `StackLayout` driven by
`root.activeSection` in `apps/linsync-gui/qml/Main.qml`.  Sections 0–5 sit
in the sidebar; Credits (6) and Licenses (7) are reachable only via the
About page (`AppAboutPage`)'s `creditsRequested()` / `licensesRequested()` signals.

| Section | File | Bridge usage |
| --- | --- | --- |
| Compare (0) | `Main.qml` body | Active — text/folder rows, browse, navigation, find. |
| Sessions (1) | `SessionsPage.qml` | Read-only over `sessionState.tabs` / `recent_paths`. |
| Filters (2) | `FiltersPage.qml` | None yet; emits signals (`includesEdited`, `excludesEdited`, etc.). |
| Plugins (3) | `PluginsPage.qml` | None yet; static demonstration list. |
| Settings (4) | `SettingsPage.qml` | None yet; emits `settingChanged(key, value)`. |
| About (5) | `AboutPage.qml` | None; brand hero + Credits / Licenses deep links. |
| Credits (6) | `CreditsPage.qml` | None; static crate manifest mirroring `docs/third-party-notices.md`. |
| Licenses (7) | `LicensesPage.qml` | None; tabbed reader (LinSync License / Third-party / Acknowledgements) with line-filtered search and a Dialog popup of the GPL v3 text. |

All eight pages share `AppCard.qml` and a locally-computed `separator` color
derived from `Kirigami.ColorUtils.tintWithAlpha` because `Kirigami.Theme`
does not expose `separatorColor`. The next bridge work owns wiring
Filters, Plugins, and Settings to real Rust slots so edits round-trip
through XDG storage rather than staying in QML-only state.

## GUI Thread Rule

Rust worker tasks may use scoped threads or a runtime later, but QML-visible
state must be mutated only on the GUI thread. Worker code must report progress,
partial rows, cancellation, and completion through message channels or bridge
callbacks that marshal back to the GUI thread before touching QObject properties
or emitting model signals.

Model updates must be batched enough to avoid flooding QML, but cancellation and
error events must still be delivered promptly. Any folder-table implementation
must keep stable row identity so QML selection and sorting do not corrupt user
focus during incremental updates.

## Fallback Host

If `cxx-qt` cannot expose the editor, table, tree, or cancellation model cleanly,
use a thin C++/QML host with one of these Rust integration paths:

- Direct Rust library calls through a small C ABI or C++ wrapper for synchronous
  operations and model adapters.
- A local JSON-RPC sidecar for long-running compares, plugin execution, and
  cancellation if the QObject model becomes too costly to maintain directly.

The default Rust GUI launcher still has an intentionally small local HTTP/JSON
bridge used by the QML shell for two-path compare requests. This is not the
final model bridge; it is a working fallback-style seam for validating
QML-to-Rust request flow while the feature-gated `cxx-qt` host grows typed
models and the broader editor, tree-model, progress, and cancellation spikes
remain open.

Fallback acceptance criteria:

- The Rust core remains the source of compare, merge, filter, storage, plugin,
  and folder-operation behavior.
- The fallback host owns only Qt application lifecycle, QML resource loading,
  visual models, and desktop integration.
- Cancellation, partial results, and errors remain testable without driving the
  full QML UI.
- The bridge keeps file paths and generated temporary paths explicit so archive,
  plugin, and self-compare workflows remain auditable.

## Editor Rendering Direction

The first editor spike should avoid custom painting until needed. Start with a
QML text surface plus separate gutter/overview models for line numbers, current
difference, dirty state, and inline spans. Move to a custom `QQuickItem` or
KTextEditor/KSyntaxHighlighting-backed component only if the QML text surface
cannot keep synchronized scrolling, stable gutters, and inline highlight
performance on large files.
