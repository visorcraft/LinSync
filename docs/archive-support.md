# Archive Support Decision

> Status: the read-only archive-as-virtual-folder pipeline (nested-archive
> recursion + member extraction) has shipped. Writable archive-member editing
> for **zip archives** shipped in v1.10.0; tar and 7z remain read-only. The rest
> of this document records the design constraints and the bar future formats
> would have to clear.

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

The archive workflow supports:

- Listing archive members as virtual folder rows.
- Comparing member name, path, size, timestamp, CRC/hash when available, and
  type/status metadata.
- Extracting selected members to secure temporary locations for text, binary,
  table, image, or external-viewer compare paths.
- Showing archive member paths with an explicit virtual-path prefix so users do
  not confuse extracted temporary files with real editable filesystem paths.

**Zip archives additionally support member editing** (see below). Tar and 7z
remain read-only.

## Writable Zip Editing

Zip archive member editing shipped in v1.10.0. The design is documented in
`docs/archive-write-safety-design.md` and covers:

- Atomic replace via tmp-then-rename (original never opened for writing).
- Automatic `.bak` backup until commit succeeds.
- Sandboxed `unzip`/`zip` helper processes under Landlock/seccomp.
- Freshness fingerprinting (SHA-256 + flock) to prevent TOCTOU races.
- Size and compression-ratio caps to mitigate archive bombs.
- Path validation (zip-slip, symlink rejection, encoding round-trip checks).
- Post-repack assertion (member count unchanged, target exactly once).

Edit flow:

1. Right-click a zip member row → "Edit member in left/right archive".
2. The member is extracted to a staging file and opened in the external editor.
3. Save in the editor, then click **Commit** (or **Discard**) in LinSync.
4. Commit repacks the archive atomically; discard cleans up the staging file.

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

Writable archive-member workflows are now implemented for **zip**.
Tar, 7z, and other formats remain read-only until a repack-capable helper or
built-in path is available. The plugin protocol extension for `repack_member` is
specified in `docs/archive-write-safety-design.md` §5 but deferred until
`sandbox_writes_input` support lands in `linsync-sandbox`.
