# Archive Support Decision

> Status: the read-only archive-as-virtual-folder pipeline (nested-archive
> recursion + member extraction) has shipped. Writable archive-member editing
> remains an explicit out-of-scope carve-out; the rest of this document records
> the design constraints it shipped under and the bar a future writable
> milestone would have to clear.

Archive compare is a specialized view. The shipped implementation is read-only
and presents archives as virtual folders; writable archive-member editing is
deferred until a separate safety design exists.

## Helper Strategy

LinSync should use helper processes for the first archive-as-folder
implementation instead of linking archive parsers directly into the core. The
helper protocol keeps unsafe parsers and format-specific crashes out of the main
process and fits the existing plugin/helper process model.

Preferred order:

- Start with a system-provided 7z-compatible helper when licensing, packaging,
  and Flatpak permissions allow it.
- Keep the helper interface generic enough that a libarchive-backed helper can
  be added later without changing the compare model.
- Do not bundle archive helpers until their license, source-offer, notices, and
  sandbox behavior are reviewed.

## Read-Only First

The first archive workflow should support:

- Listing archive members as virtual folder rows.
- Comparing member name, path, size, timestamp, CRC/hash when available, and
  type/status metadata.
- Extracting selected members to secure temporary locations for text, binary,
  table, image, or external-viewer compare paths.
- Showing archive member paths with an explicit virtual-path prefix so users do
  not confuse extracted temporary files with real editable filesystem paths.

The first archive workflow must not:

- Save edits back into an archive.
- Delete, rename, move, or overwrite archive members.
- Treat archive members as writable folder-sync targets.
- Silently extract or execute helper-produced files outside the assigned temp
  directory.

## Security Requirements

Archive extraction is untrusted input handling. Before enabling it, tests must
cover:

- Path traversal entries such as `../outside`.
- Absolute member paths.
- Symlink and hardlink escapes.
- Nested archive or zip-bomb style expansion limits.
- Helper timeout, cancellation, stderr capture, and output-size limits.
- Cleanup of temporary extraction directories.

Extraction must use unique temporary directories, reject member paths that escape
the extraction root after normalization, and keep destructive folder operations
disabled for virtual archive folders.

## Writable Archive Milestone

Writable archive-member workflows are not non-applicable forever, but they
remain an out-of-scope carve-out. Before promotion, the project needs a separate design covering helper
capability detection, atomic update behavior where possible, backup/restore
behavior, failed-update recovery, conflict handling, Flatpak limitations, and
clear corruption warnings.
