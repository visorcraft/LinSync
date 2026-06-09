# Feature Coverage

This file is a one-line-per-area summary of LinSync's own feature
coverage. For the detailed per-mode status grid (core API, CLI, HTTP
bridge, cxx-qt bridge, QML page, tests, docs), see
[`feature-matrix.md`](feature-matrix.md).

Rows are intentionally product-native: each area must be implemented,
Linux-replaced, deferred with a reason, or declared non-applicable
before a parity-complete release.

| Area | Target behavior | Status |
| --- | --- | --- |
| File compare | Side-by-side text comparison, inline highlights, navigation, merge-copy actions, save safety, encoding and EOL handling | Partial |
| Folder compare | Recursive and non-recursive comparison, selectable methods, filters, row states, copy/delete/rename plans, refresh and cancel behavior | Partial |
| Three-way merge | Base/left/right comparison, conflict markers, conflict navigation, result save workflow | Partial |
| Specialized compare — image | Pixel-level exact / tolerance / perceptual (CIEDE2000) compare with bounding-box reporting, padded dimension mismatches, diff-region navigation, a real saveable diff overlay, animated GIF/APNG/WebP frame-by-frame compare, and Radiance HDR / OpenEXR decode (tone-mapped to RGBA8) | Complete — full ICC color-management interpretation remains an explicit out-of-scope carve-out |
| Specialized compare — document | Text extraction and OCR (with per-word positions) via helper plugins; rendered-document compare | Complete |
| Specialized compare — webpage | Source HTML, extracted text, resource-tree, rendered DOM, screenshot | Complete — rendered / screenshot via out-of-process Qt WebEngine |
| Specialized compare — archive | Archive-as-folder compare via plugin virtual-folder pipeline | Complete — CLI built-in and plugin paths both sandboxed |
| Filters | Portable wildcard, regex, and metadata-expression filters with clear diagnostics for unsupported rule families | Complete for current grammar |
| Reports and patches | Unified/context/normal patches, HTML reports, JSON/CSV automation output, preview-before-write, GUI export | Complete |
| CLI | Stable commands, shell completions, man page, documented exit-code contract, launch/open/reveal helpers | Partial |
| Settings and sessions | XDG JSON stores, recent paths/sessions, project files, migration, import/export, reset, and concurrency safety | Partial |
| Plugins | JSON-over-stdio helpers, manifest validation, bounded execution, discovery, classes, settings UI, sandbox policy | Partial |
| Plugin sandbox | Landlock + seccompiler + bubblewrap fallback applied to helper processes | Complete (consumed by plugin host and `linsync-cli archive` built-in fast path) |
| GUI shell | Qt 6 / Kirigami sidebar with Compare, Image / Webpage / Document Compare workspaces, Sessions, Filters, Plugins, Settings, About — plus Credits and Licenses reached from About | Partial |
| Settings UI | Appearance, comparison, session, and storage controls wired to XDG settings | Complete |
| Third-party notices | In-app Credits page (crate table) + Licenses page (tabbed reader) + regenerated `docs/third-party-notices.md` | Complete |
