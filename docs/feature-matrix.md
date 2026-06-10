# Feature Matrix

This is the maintained per-mode status grid for LinSync. Each row is a
user-facing capability; each column records whether the capability is
exposed in that surface and at what quality level. For one-line
product-area status, see
[`feature-parity.md`](feature-parity.md). For user-facing wording, see
[`known-limitations-1.0.md`](known-limitations-1.0.md).

## Legend

- **complete** — fully implemented, tested, and documented.
- **partial** — implemented end-to-end but with named gaps; see Notes.
- **experimental** — present in code, not yet a release-quality
  surface; behind a feature flag or labelled as such.
- **stub** — public API exists but returns a `NotImplemented`-style
  result.
- **broken** — surface exists at one layer but the wiring to it is
  missing or wrong, so end-to-end use fails.
- **n/a** — not applicable for this surface (e.g. CLI doesn't surface a
  GUI-only feature, or a feature has no HTTP-bridge equivalent on
  purpose).

The "Tests" column refers to in-repo integration / unit / smoke tests,
not manual QA. "Docs" refers to `docs/user-guide.md`,
`docs/known-limitations-1.0.md`, and area-specific decision docs under
`docs/`.

## Compare modes

| Capability | Core API | CLI | HTTP bridge | cxx-qt bridge | QML page | Tests | Docs | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Text compare | complete (`linsync-core::text`) | complete (`compare`) | complete (`/compare`) | partial (`compare_paths`) | complete (Compare page) | complete | complete | `GuiCompareTab` stores options so merge-copy and recompare honor the active profile. |
| Syntax highlighting (text compare) | complete (`linsync-core::syntax`, syntect behind default-on `syntax-rich`) | complete (`--syntax`) | complete (`?syntax=`, spans on `/compare` rows) | n/a | complete (Compare page Syntax selector) | complete | complete (`user-guide.md`, `engine-decisions.md`) | Modes: plain, auto, rust, json, html/xml, markdown, shell, toml, yaml, c, cpp, python, javascript, typescript, go, java, css; `auto` detects from file extension. Line-stateless (multi-line constructs highlight per line); >20,000-byte lines skipped; TOML uses the hand-rolled lexer; TypeScript maps to the JavaScript grammar. |
| Folder compare | complete (`linsync-core::folder`) | complete (`folders`) | complete (`/compare`, `/folder/op/plan`, `/folder/op/execute`) | partial (`compare_paths`) | complete (Compare page, sort/filter buttons, windowed) | complete | complete | Folder view has sort (path/status/size/method) and type filter (changed/left-only/right-only/diff) buttons. `folder_query_bridge_response` uses `resolve_folder_options_for_request` so the active profile / walk options are honored. |
| Binary / hex compare | complete (`linsync-core::binary`) | complete (`hex`, `compare --type binary`) | complete (`/compare`, `/binary/window`) | partial (via `compare_paths`) | partial (Compare page, summary rows only) | complete | partial (`user-guide.md`) | `/binary/window?offset=&limit=` returns a windowed hex row slice for the active binary tab. |
| Table compare (CSV/TSV) | complete (`linsync-core::table`) | complete (`table`, `compare --type table`) | complete (`/compare`) | partial (via `compare_paths`) | partial (Compare page, summary only) | complete | complete | Bridge query params `key_columns`, `ignore_columns`, `numeric_tolerance`, `ignore_row_order` are parsed and applied. |
| Image compare | complete (`linsync-core::image`) | complete (`compare --type image`, `--image-frames first\|all`, `--save-result`) | complete (`/compare/image`, `?frames=`, `?mode=`, `?tolerance=`, `?delta_e=`, `?overlay=`) | n/a | complete (Image Compare page, mode/tolerance/ΔE/frame selector) | complete | complete (`user-guide.md`, `known-limitations-1.0.md`, `image-compare-design.md`) | Real overlay PNG, padded dimension-mismatch canvas, diff-region navigation, format UI, save-overlay endpoint, and frame mode selector (first/all) are present. Decodes GIF/HDR/EXR, compares animated frames, records color-type metadata, and round-trips a saved result through `report --from-json`. Remaining gap: ICC/HDR fidelity (tone-mapped to RGBA8). |
| Document compare | complete (`linsync-core::document`) | complete (`compare --type document`, `--save-result`) | partial (`/compare/document`, emits `left_word_positions`/`right_word_positions`) | n/a | partial (Document Compare page) | complete | partial (`known-limitations-1.0.md`, `document-compare-implementation.md`) | OCR mode now requests per-word positions (`want_positions`) and surfaces them as data; rendered mode does per-page image diffs with a page range. Saved result round-trips through `report --from-json`. Carve-out: no zoom-aware visual overlay of OCR boxes on a rendered page. |
| Webpage compare — source / text / tree | complete (`linsync-core::webpage`) | complete (`webpage --sub-mode html|text|tree`) | complete (`/compare/webpage`, all sub-modes; cache flush via `/compare/webpage/clear-cache`) | n/a | complete (Webpage Compare page, diff rows + tree view) | partial | partial (`known-limitations-1.0.md`) | Bridge bypasses user-consent gate (`confirmed_by_user: true` hard-coded). Diff rows and resource-tree entries surfaced in QML. |
| Webpage compare — rendered / screenshot | complete (`linsync-webengine::render_url`, behind `web-engine`) | complete (`webpage --sub-mode rendered|screenshot`, `web-engine` build) | complete (`/compare/webpage?mode=…`, `web-engine` build) | n/a | complete (Webpage Compare page, gated on `webEngineAvailable`) | partial | partial | Rasterizes each URL to PNG via an out-of-process `qml6` `WebEngineView`. Requires the `web-engine` Cargo feature (on in the packaged builds); a build without it returns a structured unsupported-mode error and the QML modes stay hidden. |
| Archive-as-folder compare | complete (`compare_archives_with_unpacker`, `…_recursive`, `extract_archive_member`) | complete (`archive [--unpacker PLUGIN_ID]`) | n/a | n/a | n/a | partial | partial (`known-limitations-1.0.md`) | Plugin-based path is sandboxed; built-in `tar`/`unzip` fast path now runs under `linsync-sandbox` too. |
| Three-way merge | complete (`linsync-core::merge`) | complete (`compare3`, `conflict`, `mergetool [--auto-resolve]`) | partial (`/merge/conflicts`, etc.) | partial (`start_three_way_merge`, `resolve_three_way_conflict`, `save_three_way_merge`) | partial (Merge page) | complete | partial (`user-guide.md`) | GUI conflict highlight/scroll uses each side's own line ranges; non-auto `mergetool` launches the GUI Merge workspace and validates the saved output. |

## Workflow surfaces

| Capability | Core API | CLI | HTTP bridge | cxx-qt bridge | QML page | Tests | Docs | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Filters | complete (`linsync-core::filter`) | complete (`filter`, `--filter`, `--filter-name`) | complete (`/filters/*`, `/walk`, `/walk/set`) | n/a | complete (Filters page) | complete | complete | Per-grammar; further grammar work is a separate roadmap item. |
| Filter migration (legacy `.flt`) | complete | n/a (GUI-only) | complete (`/filters/migrate`) | n/a | complete (Filters page button) | complete | complete | Diagnostics for unsupported Windows prefixes are wired. |
| Plugins — discovery & toggle | complete (`linsync-core::plugin`) | n/a | complete (`/plugins/list`, `/plugins/toggle`) | n/a | complete (Plugins page) | complete | complete | XDG discovery paths documented. |
| Plugins — per-profile enable/disable | complete (`CompareProfile.plugin_enablement`) | n/a | complete (`/profiles/active/plugin-enabled?id=&enabled=`) | n/a | complete (Plugins page, per-profile toggle) | complete | complete | Endpoint returns 409 on a built-in profile or no active selection; a present per-profile entry overrides the global enabled map. |
| Prediffer conflict policy | complete (`PredifferConflictPolicy`, `resolve_prediffer_conflicts`) | complete (`--prediffer-conflict-policy chain\|first-wins\|last-wins`) | partial (via profile `prediffer_conflict_policy`) | n/a | n/a | complete | complete (`plugin-protocol.md`) | Prediffers declare `normalization_categories`; overlapping ones are kept/dropped per policy (default `chain` runs all). |
| Plugins — per-plugin options | complete | n/a | complete (`/plugins/options/{get,set}`) | n/a | complete (Plugins dialog) | partial | partial | Option schema from manifest is validated (`PluginManifest::validate_options`) before persisting. |
| Plugin sandbox (`linsync-sandbox`) | complete (Landlock + seccompiler + bubblewrap fallback) | n/a | n/a (transparent) | n/a | n/a | complete | complete (`docs/SECURITY.md`) | Consumed by plugin host and by `linsync-cli archive` built-in `tar`/`unzip` fast path. |
| Settings | complete (`linsync-core::storage`, `paths`) | n/a | complete (`/settings`, `/settings/set`, `/settings/reset`) | complete (`load_settings`, `save_setting`, `reset_settings`) | complete (Settings page) | complete | complete | |
| Sessions / recent paths | complete | partial (`launch`) | complete (`/session`, `/sessions/recent`, `/sessions/reopen`, `/sessions/delete`, `/sessions/rename`) | partial (`session_json`, etc.) | complete (Sessions page, delete/rename wired) | partial | partial | Sessions page has delete and rename buttons for recent sessions. |
| Reports & patches | complete (`linsync-core::report`, `patch`) | complete (`patch`, `report`, `compare --save-result`, `report --from-json`) | complete (`/report?format=summary\|folder-plan\|full-json\|unified`) | n/a | complete (Export button + format chooser + preview dialog in Compare toolbar) | complete | complete | `--save-result` / `report --from-json` round-trips text, folder, table, binary, **image, and document** results to HTML. GUI Export button calls `/report` and shows preview-before-copy. |
| Cancellation (Stop button) | complete (core cancel hooks) | n/a | complete (`/cancel?id=X`) | not wired | complete (Stop button cancels the in-flight compare) | partial | partial | Stop button in the Compare toolbar is enabled while a compare runs and calls `/cancel?id=X` to abort that exact request. |
| Trash | complete (`linsync-core::trash`) | n/a (consumed by folder ops) | partial (via `/folder/op/execute`) | n/a | partial | complete | complete | FreeDesktop xdg-trash; permanent-delete fallback documented. |

## GUI shell sections

| Section | QML file | Bridge usage | Status |
| --- | --- | --- | --- |
| Compare | `Main.qml` | text / folder / table / hex via `/compare`, `compare_paths`; folder sort/filter via `/folder/query`; export via `/report`; Stop via `/cancel?id=X` | complete |
| Image Compare | `ImageComparePage.qml` | `/compare/image` with mode/tolerance/ΔE/frames params; `/compare/image/regions`, `/compare/image/formats`, `/compare/image/save-overlay` | complete |
| Webpage Compare | `WebpageComparePage.qml` | `/compare/webpage` (all sub-modes); `/compare/webpage/clear-cache` | complete |
| Document Compare | `DocumentComparePage.qml` | `/compare/document` (text/ocr_text/rendered modes) | complete — left/right extracted-text panes surfaced; no rendered-page OCR box overlay |
| Sessions | `SessionsPage.qml` | `/session`, `/sessions/recent`, `/sessions/reopen`, `/sessions/delete`, `/sessions/rename` | complete |
| Filters | `FiltersPage.qml` | `/filters/*`, `/walk`, `/walk/set` | complete |
| Plugins | `PluginsPage.qml` | `/plugins/*` | complete |
| Settings | `SettingsPage.qml` | `/settings*` and `LinSyncSessionBridge` settings methods | complete |
| About | `AboutPage.qml` | static, deep-links to Credits / Licenses | complete |
| Credits | `CreditsPage.qml` | static (`docs/third-party-notices.md` mirror) | complete |
| Licenses | `LicensesPage.qml` | static (full-text reader) | complete |
| Merge (toolbar entry) | `MergePage.qml` | `/merge/conflicts`, `/merge3/*`; conflict scroll uses per-side line ranges from `*_lines` arrays | complete |

## How to update this matrix

When a row's status changes, update both this file and the
release-relevant section of `known-limitations-1.0.md` in the same
commit. CI does not gate on this file today; reviewer responsibility
is to keep it accurate.
