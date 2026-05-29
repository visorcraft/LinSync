# Comparison Behavior Audit

This audit records the comparison workflows LinSync intends to support without
tying the product to a specific upstream application or codebase.

## File Compare

- Side-by-side panes with synchronized scrolling.
- Diff blocks with line-level and inline highlights.
- Navigation between first, previous, next, and last difference.
- Copy current block and copy all differences in either direction.
- Dirty-state tracking, undo/redo, backup-safe save, and preserve-permission
  behavior for existing targets.
- EOL normalization, BOM handling, case/whitespace/blank-line ignore options,
  and substitution filters.

## Folder Compare

- Recursive and non-recursive directory comparison.
- Methods for content, binary content, size, modified time, date+size,
  existence, hash, and normalized text.
- Include/exclude filters, generated-directory presets, symlink policies,
  skipped/error/aborted row states, progress events, and cancellation.
- Operation planning for copy/delete/rename/create/refresh before destructive
  writes.

## Specialized Compare

- Table compare for CSV/TSV with changed-cell summaries.
- Hex compare for binary offsets, byte differences, ASCII preview, and
  metadata-only mode.
- Archive-as-folder, image, document/OCR, and webpage compare remain gated by
  security, privacy, licensing, fixture, and packaging decisions.

## Plugins

Plugins are Linux helper processes using JSON over stdio. The supported classes
are unpacker, prediffer, editor complement, external viewer, and folder
virtualizer. Helpers are bounded by stdout/stderr limits, timeouts, cancellation,
temp-directory confinement, and manifest validation.

## Reports And Automation

LinSync keeps HTML reports for human review and JSON/CSV output for automation.
Patch generation supports unified, context, and normal formats with preview
before write.
