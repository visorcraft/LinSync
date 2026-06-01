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
| Text compare | complete (`linsync-core::text`) | complete (`compare`) | partial (`/compare`) | partial (`compare_paths`) | complete (Compare page) | complete | complete | HTTP and cxx-qt bridges currently invoke `TextCompareOptions::default()` and ignore options sent from QML. |
| Folder compare | complete (`linsync-core::folder`) | complete (`folders`) | partial (`/compare`, `/folder/op/plan`, `/folder/op/execute`) | partial (`compare_paths`) | partial (Compare page, capped row count) | complete | complete | Bridge folder-op endpoints re-compare with `FolderCompareOptions::default()` instead of the active filter / walk options. |
| Binary / hex compare | complete (`linsync-core::binary`) | complete (`hex`, `compare --type binary`) | partial (`/compare`) | partial (via `compare_paths`) | partial (Compare page, summary rows only) | complete | partial (`user-guide.md`) | No navigable hex view yet; row count capped. |
| Table compare (CSV/TSV) | complete (`linsync-core::table`) | complete (`table`, `compare --type table`) | partial (`/compare`) | partial (via `compare_paths`) | partial (Compare page, summary only) | complete | complete | No key-column matching, ignore-columns, or numeric tolerance yet. |
| Image compare | complete (`linsync-core::image`) | complete (`compare --type image`) | partial (`/compare/image`) | n/a | partial (Image Compare page) | complete | complete (`user-guide.md`, `known-limitations-1.0.md`, `image-compare-design.md`) | Real overlay PNG, padded dimension-mismatch canvas, diff-region navigation, format UI, and save-overlay endpoint are present. Remaining gap: advanced color/HDR/animation handling. |
| Document compare | partial (`linsync-core::document`) | complete (`compare --type document`) | partial (`/compare/document`) | n/a | partial (Document Compare page) | partial | partial (`known-limitations-1.0.md`) | Extracted left/right text is not yet surfaced to the GUI; rendered-document mode unimplemented. (Dispatcher registration fixed in this release.) |
| Webpage compare — source / text / tree | complete (`linsync-core::webpage`) | complete (`webpage --sub-mode html|text|tree`) | partial (`/compare/webpage`, hard-coded `confirmed_by_user: true`; cache flush via `/compare/webpage/clear-cache`) | n/a | partial (Webpage Compare page) | partial | partial (`known-limitations-1.0.md`) | Bridge bypasses user-consent gate; GUI shows summary rather than rich result views. |
| Webpage compare — rendered / screenshot | **stub** (`linsync-webengine` returns `NotImplemented`) | partial (`webpage --sub-mode rendered|screenshot`, returns error) | stub | n/a | hidden in default-build QML | partial | partial | Direct bridge/CLI callers get structured unsupported-mode errors until `web-engine` has a real path. |
| Archive-as-folder compare | partial (no virtual-folder integration) | complete (`archive`) | n/a | n/a | n/a | partial | partial (`known-limitations-1.0.md`) | CLI shells out to `unzip` / `tar` directly; does not route through plugin pipeline or sandbox. |
| Three-way merge | complete (`linsync-core::merge`) | complete (`compare3`, `conflict`, `mergetool --auto-resolve`) | partial (`/merge/conflicts`, etc.) | partial (`start_three_way_merge`, `resolve_three_way_conflict`, `save_three_way_merge`) | partial (Merge page) | complete | partial (`user-guide.md`) | GUI conflict navigation indexes line text as if it were a line number; non-auto `mergetool` not yet GUI-driven. |

## Workflow surfaces

| Capability | Core API | CLI | HTTP bridge | cxx-qt bridge | QML page | Tests | Docs | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Filters | complete (`linsync-core::filter`) | complete (`filter`, `--filter`, `--filter-name`) | complete (`/filters/*`, `/walk`, `/walk/set`) | n/a | complete (Filters page) | complete | complete | Per-grammar; further grammar work is a separate roadmap item. |
| Filter migration (legacy `.flt`) | complete | n/a (GUI-only) | complete (`/filters/migrate`) | n/a | complete (Filters page button) | complete | complete | Diagnostics for unsupported Windows prefixes are wired. |
| Plugins — discovery & toggle | complete (`linsync-core::plugin`) | n/a | complete (`/plugins/list`, `/plugins/toggle`) | n/a | complete (Plugins page) | complete | complete | XDG discovery paths documented. |
| Plugins — per-plugin options | complete | n/a | complete (`/plugins/options/{get,set}`) | n/a | complete (Plugins dialog) | partial | partial | Option schema validation against manifest is not yet enforced. |
| Plugin sandbox (`linsync-sandbox`) | complete (Landlock + seccompiler + bubblewrap fallback) | n/a | n/a (transparent) | n/a | n/a | complete | complete (`docs/security.md`) | Consumed by plugin host; **not** consumed by `linsync-cli archive`. |
| Settings | complete (`linsync-core::storage`, `paths`) | n/a | complete (`/settings`, `/settings/set`, `/settings/reset`) | complete (`load_settings`, `save_setting`, `reset_settings`) | complete (Settings page) | complete | complete | |
| Sessions / recent paths | complete | partial (`launch`) | partial (`/session`, `/sessions/recent`, `/sessions/reopen`) | partial (`session_json`, etc.) | partial (Sessions page) | partial | partial | Project files in storage; UI is read-only. |
| Reports & patches | complete (`linsync-core::report`, `patch`) | complete (`patch`, `report`) | n/a | n/a | n/a | complete | complete | No GUI export yet. |
| Cancellation (Stop button) | partial (core has cancel hooks) | n/a | not wired | not wired | partial (Stop button explicitly disabled with tooltip) | none | partial | Stop button in Compare toolbar is disabled with a tooltip pointing at PLAN.md Phase 3. |
| Trash | complete (`linsync-core::trash`) | n/a (consumed by folder ops) | partial (via `/folder/op/execute`) | n/a | partial | complete | complete | FreeDesktop xdg-trash; permanent-delete fallback documented. |

## GUI shell sections

| Section | QML file | Bridge usage | Status |
| --- | --- | --- | --- |
| Compare | `Main.qml` | text / folder via `/compare` and `compare_paths`; Stop button disabled | partial |
| Image Compare | `ImageComparePage.qml` | `/compare/image`, `/compare/image/regions`, `/compare/image/formats`, `/compare/image/save-overlay` | partial |
| Webpage Compare | `WebpageComparePage.qml` | `/compare/webpage`, `/compare/webpage/clear-cache` | partial |
| Document Compare | `DocumentComparePage.qml` | `/compare/document` | partial — extracted text not surfaced |
| Sessions | `SessionsPage.qml` | `/session`, `/sessions/recent`, `/sessions/reopen` | partial |
| Filters | `FiltersPage.qml` | `/filters/*`, `/walk`, `/walk/set` | complete |
| Plugins | `PluginsPage.qml` | `/plugins/*` | complete |
| Settings | `SettingsPage.qml` | `/settings*` and `LinSyncSessionBridge` settings methods | complete |
| About | `AboutPage.qml` | static, deep-links to Credits / Licenses | complete |
| Credits | `CreditsPage.qml` | static (`docs/third-party-notices.md` mirror) | complete |
| Licenses | `LicensesPage.qml` | static (full-text reader) | complete |
| Merge (toolbar entry) | `MergePage.qml` | `/merge/conflicts`, `/merge3/*`; conflict scroll-to-line uses string-as-line-number | partial |

## How to update this matrix

When a row's status changes, update both this file and the
release-relevant section of `known-limitations-1.0.md` in the same
commit. CI does not gate on this file today; reviewer responsibility
is to keep it accurate.
