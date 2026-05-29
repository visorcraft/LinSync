# Known Limitations

This document is the user-facing summary of what LinSync does *not* do
in the current release (1.1.1). It is not a substitute for `PLAN.md` —
that file tracks the long-form project plan. Items here fall into two
categories:

1. **Currently unavailable** features that may ship in a future release.
2. **Permanent non-goals** that LinSync will not pursue.

This file is updated each release. The 1.0 polish backlog, the 1.0.1
pre-tag punch list (three-pane merge, filter-grammar migrator, plugin
`unpack_folder` op, accessibility audit, real-screenshot capture), and
the 1.1.0 specialized-engine scaffolds (image, document, webpage
compare entry points; `linsync-sandbox` crate) have all shipped; they
are no longer listed here. The `linsync-sandbox` crate ships and is
consumed by the plugin host, but `linsync-cli archive` does not yet
route through it (see "Archive compare" below).

## Specialized compare — partial implementations

The image, document, and webpage compare surfaces are present end-to-end
but several pieces are still summary-only or stubbed. See
`docs/feature-matrix.md` for a per-mode status table.

### Image compare

- The image **diff overlay** that the GUI requests via
  `/compare/image?overlay=true` is currently a *transparent* placeholder
  PNG (`apps/linsync-gui/src/lib.rs::build_overlay_png`). The pixel-diff
  result data is accurate (changed-pixel count, bbox, perceptual ΔE),
  but the overlay image itself does not visualize the differences yet.
- The **Save overlay** button in `ImageComparePage.qml` is disabled in
  this release with a tooltip pointing at PLAN.md Phase 5. The bridge
  has no `/overlay/save` handler yet; even if it did, the overlay PNG
  is a placeholder until the previous bullet is resolved.
- Dimension mismatch is reported as a hard error rather than padded to a
  common canvas with metadata.

### Document compare

- The bridge returns only summary fields (`equal`, `left_extractor`,
  `right_extractor`, `differing_lines`, `mode`). **Extracted left/right
  text** is not yet exposed to the GUI, so the Document Compare page
  shows only counts, not the side-by-side extracted content.
- Rendered document comparison (PDF/DOCX → image diff) is not
  implemented — only the text-extraction and OCR-text modes route
  through the plugin host.
- OCR language defaults to `eng` if the GUI does not pass one through;
  the bridge accepts an `ocr_language` query parameter but per-plugin
  OCR options beyond language are not yet plumbed.

### Webpage compare

- `crates/linsync-webengine` is an **explicit stub** that returns
  `WebEngineError::NotImplemented` for every render/screenshot call.
  Rendered DOM diff and screenshot diff modes are accessible in the
  QML toolbar but currently return an unsupported-mode error.
- Source HTML, extracted visible text, and resource-tree modes work
  end-to-end against the core webpage compare functions.
- The HTTP bridge currently hard-codes `confirmed_by_user: true` on
  every `/compare/webpage` request rather than threading an explicit
  user-consent gate through the GUI. The CLI honours
  `WebpageCompareOptions::confirmed_by_user` correctly; bridge parity is
  tracked in PLAN.md Phase 5 "Webpage".

## GUI surface — partial wiring

- The **Stop button** in the Compare toolbar (`Main.qml`) is present but
  disabled in this release. Long-running compares cannot currently be
  cancelled from the GUI; the core cancellation hooks exist
  (`linsync-core::folder` and `linsync-core::merge`) but are not wired
  through the HTTP or cxx-qt bridges. Re-enabling the button requires
  the bridge-side `/cancel` endpoint and per-request tokens tracked in
  PLAN.md Phase 3.
- **Text compare options** sent by QML (case/whitespace/blank-line
  ignores, substitutions) are not yet applied by the HTTP bridge —
  `compare_text_files` is invoked with `TextCompareOptions::default()`
  in the current bridge code. The CLI honours all options correctly;
  parity work is tracked in PLAN.md Phase 1.
- **Folder operations** (copy, delete, rename, refresh) re-compare with
  `FolderCompareOptions::default()`, ignoring the user's active filters,
  walk depth, symlink policy, and large-file threshold. Plan and execute
  operations are correct on the entries selected, but the underlying
  comparison they consult for plan validation is unfiltered.
- The **Merge page** conflict navigation indexes line *text* as if it
  were a line number (`c.left_lines[0] - 1` where `left_lines` is
  `Vec<String>` in the bridge response). Conflict next/previous still
  works because the index moves through the conflict array, but the
  scroll-to-line behavior is broken.
- Folder rows in the Compare page are capped to a fixed row count and
  rendered as text rows rather than a sortable/filterable folder table.

## Archive compare — not yet using the plugin pipeline

- `linsync-cli archive` extracts archives by shelling out to `unzip` and
  `tar` directly, then running a folder compare on the extracted trees.
  It does **not** route through the core plugin / virtual-folder
  architecture even though plugin scaffolds exist under
  `packaging/plugins/`. Until that wiring lands, archive compare is
  unaffected by plugin sandbox policy and cannot be invoked from the
  GUI as a virtual-folder source.

## Packaging caveats

- **Ubuntu 24.04 (noble)** does not ship
  `qml6-module-org-kde-kirigami` in its stock repos. The `.deb` artifact
  builds against Debian trixie (which does ship it). Ubuntu users need
  either a KDE neon source for the Kirigami package or to install via
  AppImage / Flatpak until the dependency lands in noble-updates.
- **Webpage compare** requires the `webengine` feature, which depends
  on the Qt WebEngine package on the host. The default `.deb` / `.rpm`
  / `.pkg.tar.zst` builds *do not* enable it; even when enabled, the
  rendered/screenshot sub-modes return `NotImplemented` (see above).
- **Document compare** requires Tesseract OCR, Poppler utilities, and
  LibreOffice on the host (used by helper plugins under
  `packaging/plugins/`). Installation is best-effort: distro repos
  satisfy this on Arch, Fedora, and Debian, but the helpers are not
  bundled inside the LinSync package.

## Accessibility

- The automated a11y CI gate (focus order grep, `Accessible.name`
  presence) passes. A formal **Orca screen reader walkthrough** of every
  sidebar section is not yet logged as a release artifact — see
  issue #3.
- Post-1.0 a11y polish work is tracked separately (overview-pane
  keyboard nav, Swap-sides action, broader `Accessible.description`
  coverage) — see issue #4.

## Filters

- The legacy `attr:` / `dos:` / `ctime:` / `version:` / `shell:`
  metadata prefixes are Windows-specific and remain unsupported on
  Linux. They return a structured `FilterParseErrorKind` with
  migration guidance via the GUI `/filters/validate` endpoint and the
  CLI `linsync-cli filter validate` subcommand.

## Editing

- **Writable archive-member editing** is deliberately deferred until a
  separate helper plus Flatpak-portal safety design exists. Archive
  contents are read-only in this release.
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
