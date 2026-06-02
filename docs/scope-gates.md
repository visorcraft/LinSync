# LinSync Scope Gates

This document defines LinSync's release gates. A gate is complete only when the
listed behavior is implemented, documented where user-visible, and covered by the
relevant unit, integration, smoke, fixture, or screenshot checks. For the
implemented status of each gate, see `docs/feature-matrix.md`.

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

Post-1.0 tracked features that needed deeper safety, licensing, performance, or
UI design before they could be promised as stable. Each was held to the exit
criterion below before being promoted into a release. As of 1.9.0 every
candidate listed here has shipped (see `docs/feature-matrix.md`); the gate is
retained as a record of the promotion bar.

Promoted:

- Three-way file/folder merge UX beyond inspection and marker export, including
  the interactive Git mergetool launched from the GUI.
- Folder tree view, virtualized huge-tree performance gates, cancellation with
  preserved partial results, and staged folder sync execution.
- Image compare (pixel/perceptual diff with overlay, animated frame and HDR
  decode), archive-as-folder compare (nested recursion + member extraction),
  document/OCR compare (including per-word positional data), and webpage compare
  (out-of-process Qt WebEngine rendered/screenshot plus a filterable resource
  tree). Writable archive-member editing remains a deliberate non-goal.
- Read-only hex/binary compare and corruption-warning UX. Freeform hex/binary
  editing remains a deliberate non-goal.
- Full legacy filter expression grammar and migration diagnostics.
- Plugin sandboxing decisions for Flatpak portals, Bubblewrap, distributor trust,
  and explicit user trust.
- Localization breadth, AT-SPI coverage (screen-reader announcements,
  high-contrast change bars), and the per-mode performance/memory budgets in
  `docs/performance-budgets.md` for very large files and folder trees.

Exit criteria:

- Each promoted feature had a security/licensing decision, fixtures or controlled
  test data, user documentation, and regression tests before it moved into a
  release gate.
