# LinSync Scope Gates

This document defines the release gates used by `PLAN.md`. A gate is complete
only when the listed behavior is implemented, documented where user-visible, and
covered by the relevant unit, integration, smoke, fixture, or screenshot checks.

## MVP Gate

The MVP proves LinSync's local Linux compare workflow without claiming complete
feature parity.

Required:

- Rust workspace, core crate, CLI crate, GUI crate, packaging metadata, and CI
  checks build on Linux.
- Two-way text compare loads local files, reports line and inline differences,
  exposes navigation data, and supports copy-left/copy-right merge primitives.
- Save planning preserves line endings and encodings already supported by the
  loader, writes through an atomic temp-file path where possible, and creates
  backups before overwrites.
- Two-folder compare traverses recursively or non-recursively, reports
  identical, different, left-only, right-only, skipped, and error states, and
  supports core filtering.
- Unified patch export, basic HTML reports, XDG settings/recent/log storage, and
  CLI compare/folder/patch/report/self-compare commands work from scripts.
- Flatpak metadata, AppImage scaffolding, desktop file, AppStream metadata, MIME
  associations, and release smoke checks exist.
- Destructive operations remain previewed or disabled unless a tested plan and
  confirmation path exists.

Exit criteria:

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `bash scripts/release-smoke.sh`

## 1.0 Gate

The 1.0 release is the first user-facing Linux desktop release intended for
regular file and folder compare work.

Required:

- Qt 6/QML/Kirigami GUI provides the compare workspace, path selection,
  side-by-side text panes, folder table view, status bar, and keyboard-driven
  navigation.
- Text compare includes line numbers, synchronized scrolling, current
  difference highlighting, inline highlights, save/save-as, dirty markers,
  overwrite safeguards, search, and read-only side behavior.
- Folder compare includes sortable/filterable table rows, stable columns,
  statistics, opening selected file pairs, refresh, and staged copy/delete/rename
  operation plans.
- Settings, filters, recent sessions, project/session files, and plugin
  configuration are surfaced through the app without requiring opaque storage.
- Binary/hex read-only compare and CSV/TSV table compare are usable through the
  GUI or explicitly documented as CLI-only limitations for the release.
- Accessibility, Wayland/X11/fractional-scaling, and packaging smoke checks cover
  the custom controls and packaged desktop metadata.
- Feature coverage and acceptance matrices identify every supported,
  Linux-replaced, deferred, and explicitly non-applicable feature with evidence.

Exit criteria:

- MVP gate remains green.
- GUI smoke and screenshot checks pass for representative desktop and mobile-ish
  narrow laptop widths.
- Packaging validation passes for local source builds and the chosen release
  artifacts.
- Known limitations are accurate and linked from user-facing documentation.

## Post-1.0 Gate

Post-1.0 tracks features that need deeper safety, licensing, performance, or UI
design before they should be promised as stable.

Candidates:

- Three-way file/folder merge UX beyond inspection and marker export.
- Folder tree view, virtualized huge-tree performance gates, cancellation with
  preserved partial results, and staged folder sync execution.
- Image compare, archive-as-folder compare, document/OCR compare, webpage
  compare, and writable archive-member workflows.
- Hex editing, binary save behavior, and corruption-warning UX.
- Full legacy filter expression grammar and migration diagnostics.
- Plugin sandboxing decisions for Flatpak portals, Bubblewrap, distributor trust,
  or explicit user trust.
- Localization breadth, AT-SPI coverage, and the per-mode performance/memory
  budgets in `docs/performance-budgets.md` for very large files and folder
  trees.

Exit criteria:

- Each promoted post-1.0 feature has a security/licensing decision, fixtures or
  controlled test data, user documentation, and regression tests before it moves
  into a release gate.
