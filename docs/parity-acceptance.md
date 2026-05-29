# Parity Acceptance

Current status: not parity-complete.

Before a parity-complete release, every supported behavior must have at least
one concrete acceptance signal: fixture coverage, CLI coverage, GUI smoke or
screenshot coverage, packaging validation, or a documented non-applicability
decision. Every area in `docs/feature-parity.md` must have a row here.

| Area | Status | Evidence now | Remaining acceptance work |
| --- | --- | --- | --- |
| File compare | Partial | Core and CLI tests cover equal/different files, ignore flags, line endings, BOM handling, inline spans, reports, patches, and text-tab save safety. | Editable panes, save-as/reload workflow, UI screenshot coverage, richer keyboard navigation. |
| Folder compare | Partial | Core and CLI tests cover row states, recursion, methods, symlink policy, filters, progress/cancel, operation planning, and HTML/JSON/CSV output. | GUI folder tree/table controls, operation execution UI, conflict prompts, large fixture smoke. |
| Three-way merge | Partial | Core and CLI tests cover base-aware merge, conflict markers, same-change handling, and append-only changes. | GUI three-pane workflow, conflict navigation, result-path save checks. |
| Filters | Partial | Core and CLI tests cover wildcard/regex include/exclude rules, portable metadata expressions, saved filters, generated-directory excludes, substitution filters, and diagnostics. | Full filter editor wiring, grouped mask semantics, side-specific attributes, content predicates, migration diagnostics. |
| Reports and patches | Partial | CLI tests cover patch formats, patch preview, folder-level patch sets, report context, folder columns/tree state, nested text reports, and HTML reports. | Patch apply safety design, GUI export workflow, report screenshots. |
| CLI | Partial | Integration tests cover commands, JSON/count/quiet output, completions, man page, open/reveal/launch helper behavior, and exit codes. | Packaged binary smoke across release targets. |
| Settings and sessions | Partial | Core storage tests cover schema migration, import/export, backup, reset, recent paths/sessions, projects, and concurrent writes. | GUI load/save wiring for every setting key and session restore workflow. |
| Specialized compare | Partial | CLI table compare and core/CLI hex compare are covered; archive, OCR, image, and webpage decisions document safety gates. | Dedicated GUI views and fixture coverage for deferred surfaces. |
| Plugins | Partial | Core tests cover discovery, manifest validation, helper execution, timeout/output limits, file-backed outputs, protocol mismatch, and sandbox declarations. | GUI discovery wiring, enable/disable persistence, packaged sandbox validation, helper security fixtures. |
| GUI shell | Partial | GUI unit tests and offscreen smoke cover launch contexts, bridge endpoints, tab/session state, merge-copy actions, undo/redo/save, origin checks, and QML loading. | Screenshot-based layout checks, accessibility pass, packaged runtime smoke. |
| Settings UI | Partial | QML exposes stable setting keys and core storage is schema-versioned. | Bridge `SettingsPage` signals to `SettingsStore`, load persisted values, test every key. |
| Third-party notices | Partial | `docs/third-party-notices.md`, in-app Credits page (crate table) and Licenses page (tabbed reader), `cargo deny`, and release smoke cover current license metadata. | Automate drift detection between `Cargo.lock`, docs, and QML before binary release. |
