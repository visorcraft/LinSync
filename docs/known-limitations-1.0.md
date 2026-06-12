# Known Limitations

This document is the user-facing summary of what LinSync does *not* do
in the current release (1.13.0). For the shipped feature record, see
`docs/feature-matrix.md`. Items here fall into two
categories:

1. **Currently unavailable** features that may ship in a future release.
2. **Permanent non-goals** that LinSync will not pursue.

This file is updated each release. As of 1.13.0 all roadmap phases (0–44)
are shipped. The image (frame selector), document, and webpage compare
surfaces, the three-pane merge, the filter-grammar migrator, the plugin
host with sandboxed helpers, sessions/projects, accessibility, localization,
windowing, bridge option propagation, table compare enhancements, archive
sandboxing, session delete/rename UI, navigable hex view, image animation
UI, document OCR word boxes, webpage rich results GUI, folder sort/filter
UI, plugin option schema validation, GUI export, folder group-by, and
multi-type folder filter, fixed bridge-info $TMPDIR desync, removed dead code, feature-parity docs cleanup, and merge conflict scroll line ranges are all complete.

## Specialized compare — remaining gaps

The image, document, and webpage compare surfaces are implemented
end-to-end. See `docs/feature-matrix.md` for a per-mode status table.
The only remaining gaps are noted below.

### Image compare

- Image comparison is based on decoded RGBA8 samples. Animated
  GIF/APNG/WebP frame-by-frame compare (`--image-frames first|all`,
  bridge `?frames=`) and Radiance HDR + OpenEXR decode (tone-mapped to
  RGBA8) ship, with the decoded color type reported on each result.
  Exact and tolerance modes include the alpha channel; perceptual mode
  compares RGB only. **Full ICC color-management interpretation is out
  of scope** — pixels are compared in their decoded color space without
  profile conversion. See `docs/image-compare-design.md` for the
  detailed contract.

### Document compare

- Text, OCR-text, and rendered (PDF/DOCX → per-page image diff) modes
  are all available. Text and OCR-text modes return the extracted
  `left_text`/`right_text` to the GUI side-by-side; rendered mode (via a
  `pdf_renderer` plugin) reports per-page pixel equality with a
  page-range field and emits `rendered_pages` carrying `file://` URIs for
  the left/right PNGs. The Document Compare page shows a thumbnail page
  list and side-by-side zoom/pan image panes for the selected page.
- A **visual word-box overlay drawn over a rendered source page** is out
  of scope: OCR word positions are returned as structured data, but
  LinSync does not paint bounding boxes onto a rendered page image.
- OCR language defaults to `eng` if the GUI does not pass one through;
  the bridge accepts an `ocr_language` query parameter. Per-plugin OCR
  options beyond language are left to the helper plugin.

### Webpage compare

- Rendered DOM diff and screenshot diff modes work end-to-end through
  `crates/linsync-webengine`, which rasterizes pages out-of-process via a
  short-lived headless `qml6` `WebEngineView`. These sub-modes require
  the `web-engine` build feature and a Qt 6 QML runner on the host; when
  no runner is found `render_url` returns `WebEngineError::NotImplemented`
  so callers fall back to HTML-source compare.
- Source HTML, extracted visible text, and resource-tree modes work
  end-to-end against the core webpage compare functions regardless of the
  `web-engine` feature.

## GUI surface

- The **Stop button** in the Compare toolbar (`Main.qml`) cancels an
  in-flight compare: each `/compare` carries a per-request id and the
  bridge `/cancel?id=X` endpoint flips its cancel flag, after which the
  response handler reports "Compare cancelled".
- **Text compare options** sent by QML (case/whitespace/blank-line
  ignores, substitutions) are applied by the HTTP bridge via
  `resolve_profile_for_request`, which merges per-request query overrides
  (`?ignore_case`, `?ignore_whitespace`, …) over the active profile. The
  CLI honours all options too.
- **Syntax highlighting** in text compare is computed line-at-a-time
  without cross-line state: multi-line constructs (block comments, raw
  strings) highlight per line, and lines over 20,000 bytes are skipped.
  TOML uses LinSync's hand-rolled lexer (syntect's default grammar set
  has no TOML grammar); TypeScript is highlighted with the JavaScript
  grammar.
- **Folder operations** (copy, delete, rename, refresh) and the folder
  result view are served by `/folder/query`, which applies the user's
  active filters, sort, type filter, and path search server-side and
  pages the tree so it never loads in full.
- **Permanent-delete confirmation** is implemented: when trash is
  disabled or unavailable, `/folder/op/plan` reports
  `permanent_delete`/`permanent_warning`, the folder operations dialog
  shows the warning and gates its Apply button on an explicit
  "Permanently delete" checkbox (reset on every open), and
  `/folder/op/execute` rejects permanent deletes with 409 unless
  `confirm_permanent=1` is sent.
- The **Merge page** conflict navigation uses per-side numeric line
  ranges (`currentConflictStart`/`End` derive from each side's `*_lines`
  array), so conflict next/previous scrolls each pane to the correct
  line.
- **Table compare** renders as a real grid on the Compare page with
  column headers, row numbers, per-cell state highlighting (Equal /
  Changed / LeftOnly / RightOnly), and inline left/right values for
  changed cells. Large tables page through `/compare/table/window` so
  the full grid never loads at once.

## Archive compare

- `linsync-cli archive` routes through the core plugin / virtual-folder
  pipeline: with `--unpacker PLUGIN_ID` (or when a matching `unpacker`
  plugin is discovered) it extracts via the sandboxed helper and runs a
  folder compare on the resulting virtual folder, including nested-archive
  recursion. For archive types the built-in extractor recognizes, the
  `unzip`/`tar` subprocess now runs under `linsync-sandbox` with a policy
  scoped to the archive and extraction directory.
- The GUI auto-routes archive pairs to a folder view: a matching
  folder-virtualizer plugin takes precedence; otherwise supported
  `zip`/`tar` archives are extracted to a per-tab cache directory and
  compared as folders. The mode selector on the Compare page includes an
  explicit "Archive" entry.

## Packaging caveats

- **Ubuntu 24.04 (noble)** does not ship
  `qml6-module-org-kde-kirigami` in its stock repos. The `.deb` artifact
  builds against Debian trixie (which does ship it). Ubuntu users need
  either a KDE neon source for the Kirigami package or to install via
  AppImage / Flatpak until the dependency lands in noble-updates.
- **Webpage rendered/screenshot compare** requires the `web-engine`
  feature, which depends on the Qt WebEngine package and a Qt 6 QML
  runner on the host. The Arch, RPM, AppImage, and Flatpak recipes
  enable it; the Debian `.deb` does not (it builds the external QML host
  against Debian's stable Qt). Source/text/resource-tree webpage compare
  works without the feature.
- **Flatpak `--share=network`** is used only by webpage compare; Flatpak
  cannot restrict egress per-domain, and the in-app proxy that real
  restriction would require is a permanent non-goal (see
  `docs/webpage-compare-implementation.md`, "Resolved: Flatpak network
  scope"). Stripping the permission disables only webpage compare.
- **Document compare** requires Tesseract OCR, Poppler utilities, and
  LibreOffice on the host (used by helper plugins under
  `packaging/plugins/`). Installation is best-effort: distro repos
  satisfy this on Arch, Fedora, and Debian, but the helpers are not
  bundled inside the LinSync package.

## Accessibility and Localization

- The automated a11y CI gate (focus order grep, `Accessible.name`
  presence) passes, and the shipped a11y work includes screen-reader
  announcements (`Accessible.announce`), high-contrast diff change bars,
  verified keyboard focus order, a Swap-sides action, and broad
  `Accessible.name`/`description` coverage. `scripts/gui-screenshot.sh`
  captures offscreen screenshots of every sidebar section for regression
  visibility. A formal **Orca screen reader walkthrough** of every sidebar
  section is not yet logged as a release artifact.
- Translation catalogs ship for German (`de`), French (`fr`), Spanish
  (`es`), Japanese (`ja`), and Simplified Chinese (`zh_CN`). The active
  locale is auto-detected; untranslated strings fall back to English.

## Filters

- The legacy `attr:` / `dos:` / `ctime:` / `version:` / `shell:`
  metadata prefixes are Windows-specific and remain unsupported on
  Linux. They return a structured `FilterParseErrorKind` with
  migration guidance via the GUI `/filters/validate` endpoint and the
  CLI `linsync-cli filter validate` subcommand.

## Editing

- **Writable archive-member editing** is implemented for **zip archives**
  (v1.10.0) via the GUI context menu or the
  `/archive/member/edit` and `/archive/member/commit` bridge endpoints.
  Tar and 7z remain read-only.
- **Freeform binary / hex editing** is out of scope. The hex view is
  read-only.

## Permanent non-goals

These will not change with future releases:

- **Windows / macOS builds.** LinSync targets Linux only.
- **Windows-only in-process plugins**, **Windows Explorer shell
  extensions**, and **registry-backed settings** are not supported.
  Linux uses external helper processes (`docs/plugin-protocol.md`),
  Dolphin service menus, and XDG JSON settings.
- **In-place editing of archive members** and **freeform binary / hex
  editing** are deferred to later safety designs and may never ship.
- **Network / cloud provider integrations.** LinSync compares
  filesystem content. Cloud providers must appear as mounted Linux
  paths (rclone, GVFS, etc.) before LinSync treats them as comparable.
- The application shell is a **native Qt / Kirigami desktop UI**, not a
  web or browser-based shell.

## Licensing boundary

- LinSync is **GPL-3.0-only**.
- Third-party application source, icons, translations, bundled filters,
  and plugin implementations must not be copied unless a later
  file-specific review proves GPL-3.0 compatibility
  (see `docs/licensing.md`).
- `deny.toml` enforces the allow-list of license expressions for
  Cargo dependencies; `just deny` is the gate.
