# Performance And Memory Budgets

> Status: baseline budget contract for the performance/a11y/l10n work. The
> budgets define target behavior and fallback rules; benchmark and enforcement
> coverage is tracked alongside each mode's tests.

LinSync handles local files, folders, helper-plugin output, and network-fetched
webpage content. The comparison result should stay responsive on a typical Linux
desktop without unbounded allocations. These budgets are intentionally expressed
as release gates rather than micro-optimizations: if an input exceeds a budget,
the mode must switch to a streaming, paged, summarized, or explicitly rejected
path instead of silently exhausting memory.

## Baseline Assumptions

- Desktop target: 8 GiB RAM, local SSD, Qt 6/Kirigami session.
- Soft per-operation Rust heap budget: 512 MiB unless a mode-specific budget is
  lower.
- GUI model budget: visible rows plus a bounded prefetch window; do not copy an
  entire large result into QML when a Rust-side query/window API exists.
- Helper/plugin output budget: bounded by the plugin host's stdout/stderr and
  streaming-output limits, never by trusting the helper.
- A compare may return a partial/summarized result only if the response says so
  explicitly.

## Per-Mode Budgets

| Mode | Target memory budget | Fallback behavior |
| --- | --- | --- |
| Text | O(input bytes) plus O(min(lines_left, lines_right)) for large LCS inputs; GUI row windows should stay under 64 MiB. | Use Hirschberg for large line counts, context/show-only-changes views for display, and the paged text-result window API (`/compare/text/window`) for very large panes. |
| Folder | O(number of entries) metadata only; file content and hashes must be streamed. GUI folders should render from a windowed/queryable result, not two text panes. | Stream walking/hashing, keep progress/cancel hooks active, and summarize or page large result sets through a Rust-side folder query API. |
| Binary/hex | Compare content in bounded chunks; rendered hex rows should stay page-sized, normally under 16 MiB per view. | Use `hex_page()` and byte-search/navigation APIs. Avoid freeform binary editing and avoid materializing a full hex dump in QML. |
| Table | O(rows * visible columns) for parsed cell metadata, with row windows under 64 MiB in GUI. | Use key-column matching and ignored-column rules to limit result size; large spreadsheet/helper formats must route through plugins or fail with a clear limit. |
| Image | Decoded RGBA working set should stay under 512 MiB for normal compares; overlay artifacts are written to temp files rather than embedded in QML JSON. | Use the large-image stripe path for oversized inputs, pad dimension mismatches to a common canvas, decode HDR/animated frames into the RGBA8 working set, and report any unsupported input explicitly. |
| Document/OCR | Extracted text should follow the text budget; rendered-page images should follow the image budget per page. | Sandbox helpers, cap helper output, compare page ranges when available, and report helper capability or page-limit failures instead of loading an entire rendered document at once. |
| Webpage | Source/text/tree data should stay under 128 MiB per compare; rendered/screenshot modes (the `web-engine` feature) follow the image budget. | Enforce fetch controls (`depth`, `timeout`, `max_requests`, user agent), cache artifacts under XDG cache, and render resource trees through sortable/filterable views rather than summary-only strings. |
| Archive | Virtual-folder manifests should follow the folder budget; extracted member content should be streamed or temp-file backed. | Route archive work through sandboxed unpacker/virtualizer plugins, bound nested archive recursion, and reject password/helper failures with structured diagnostics. |
| Merge | Text merge state should follow the text budget plus conflict metadata; GUI panes should not duplicate full documents more than necessary. | Keep unresolved-conflict metadata numeric and stable; for very large merges, prefer windowed display and machine-readable summaries over all-lines duplication. |
| Reports/artifacts | JSON/HTML/report generation should avoid embedding binary artifacts; bundles should reference files by relative artifact paths. | Store overlays, screenshots, extracted text, and manifests as artifacts; clean them through `/artifacts/cleanup` or XDG cache/state retention policy. |
| Plugins | Helper stdout/stderr and declared outputs must stay within `PluginExecutionOptions` byte caps. | Use length-prefixed streaming output for large plugin results, record diagnostics, and fail closed when the sandbox or output budget is exceeded. |

## Fallback Rules

- **Prefer paging over truncation.** If a complete result exists but is too large
  for GUI rendering, expose a query/window API and say which window is visible.
- **Prefer artifacts over JSON blobs.** Binary overlays, screenshots, rendered
  pages, and extracted helper outputs should be file-backed artifacts with
  manifest metadata.
- **Make degradation visible.** A response that uses a large-input fallback,
  summary-only view, skipped helper, sandbox downgrade, or output cap must expose
  that state in structured data and user-facing status text.
- **Keep cancellation live.** Any fallback path expected to exceed one second of
  wall time should poll cancellation and publish progress snapshots where the
  bridge surface supports it.
- **Fail before memory pressure.** If a mode cannot satisfy its budget without a
  streaming/paged implementation, return a structured error that names the
  unsupported input size or helper requirement.

## Verification Expectations

Performance work is complete only when the relevant budget has evidence:

- A unit, integration, GUI-smoke, or benchmark fixture covers the mode's large
  input path.
- The test exercises the fallback mechanism, not just the happy path.
- The output states whether the result is complete, summarized, paged, or
  rejected.
- Any remaining enforcement gaps are named in the relevant mode's tests or docs,
  separately from the budget definition itself.
