# Writable Archive-Member Editing Design

> Status: design — supersedes the deferral in `docs/known-limitations-1.0.md` lines 42–43
> and notes lines 104–105. This document proves feasibility; it does **not** commit to v1.0
> or v1.1 inclusion. The feature may still never ship.

## Goals

1. Let a user edit a single member of a ZIP or tar archive through the LinSync GUI, without
   requiring a manual extract → edit → repack cycle.
2. Guarantee that the original archive is never mutated unless the replacement write
   completes successfully (atomic two-phase commit).
3. Preserve permissions, timestamps, and all other metadata for unchanged members,
   bit-exactly where possible.
4. Scope helper write access to its own temp directory and the single target archive path —
   no broader filesystem access.
5. On Flatpak, obtain transient archive access through the file portal; the helper inherits
   only that token, not broad host-filesystem access.

## Non-goals

- **Encrypted archives** stay read-only. Password prompting is a security-review item deferred
  indefinitely.
- **Binary/hex editing of archive members** is out of scope (per `docs/known-limitations-1.0.md`
  lines 104–105, which this design does not retract for that sub-case).
- **Member deletion, rename, or reorder** — the first `replace_member` operation covers
  content replacement only.
- **Archives nested inside archives** — the outer archive must be unpacked first.

## Per-format vs generic

**Recommendation: per-format helpers**, extending the existing plugin pair under
`packaging/plugins/zip-unpacker/` and `packaging/plugins/tar-unpacker/`.

Rationale:

- The `unpack_folder` operation in `docs/plugin-protocol.md` is already per-format; a
  per-format `replace_member` is consistent.
- ZIP and tar have fundamentally different mutation paths: ZIP rewrites only the central
  directory; tar requires a full stream rewrite. One generic helper handling both adds
  complexity without benefit.
- Python's `zipfile` (used by `zip-unpacker`) and `tarfile` (used by `tar-unpacker`) each
  expose the right primitives for their format.

## Plugin protocol extension

### New op: `replace_member`

**Request:** `{op, archive_path, member_path, new_content_temp_path}` where
`archive_path` is absolute, `member_path` must not contain `..` or a leading `/`, and
`new_content_temp_path` is the replacement content pre-staged by the host in the helper's
assigned temp directory.

**Response (success):** `{ok: true, new_archive_temp_path}` — the helper writes the
completed new archive to `<archive>.linsync.tmp.<pid>` inside the archive's parent
directory (same filesystem, enabling atomic rename). The host calls `rename(2)`.

**Response (failure):** `{ok: false, error}`. The helper removes any temp file before
responding.

## Atomic-safety flow

1. **Host stages content.** The host copies the user's edited content into the helper's
   dedicated temp directory (`$XDG_CACHE_HOME/linsync/plugin-tmp/<pid>/`).
2. **Host launches helper** with `replace_member` request.
3. **Helper writes new archive** to `<archive_path>.linsync.tmp.<helper-pid>` in the
   archive's parent directory. The helper does not touch the original.
4. **Helper responds** with `new_archive_temp_path` on success, or `ok: false` plus cleanup
   on failure. If the helper exits non-zero or times out, the host discards any partial temp
   file.
5. **Host fsync.** On success, the host calls `fsync(2)` on the temp file fd before rename.
6. **Host rename.** `rename(tmp_path, archive_path)` — atomic on any POSIX filesystem.
7. **On crash between steps 5 and 6.** The original archive is intact. The `.linsync.tmp.*`
   file is an orphan and is collected on next startup (any file matching
   `*.linsync.tmp.*` older than 1 hour in the archive's directory is removed).

**Cancellation:** the host sends `SIGTERM` to the helper if the user cancels. The helper
installs a signal handler that removes its temp file before exiting. The host treats
non-zero exit as failure and performs no rename.

## Metadata preservation

**ZIP:** Python's `ZipInfo` objects are copied verbatim for unchanged members (`compress_type`,
`date_time`, `external_attr`, extra fields). The replacement member gets a fresh `ZipInfo`
with current `date_time` and the archive's dominant `compress_type`.

**Tar:** Python's `TarInfo` objects are copied verbatim via `addfile(tarinfo, fileobj)`.
UID/GID values are copied numerically from source headers; the helper does not call `chown`.

> **Known limitation (Flatpak):** the sandbox cannot `chown`. UID/GID values are preserved
> numerically inside the archive headers but will map to the sandbox user on extraction.
> This matches the behavior of any tar-rewrite tool running sandboxed and is documented,
> not treated as a bug.

## Sandbox + Flatpak portal

### Native (Phase 6 sandbox)

The Phase 6 sandbox wraps every helper via Landlock/seccomp-bpf before exec. For
`replace_member`, write capability is scoped to:

- The helper's own temp directory (`$XDG_CACHE_HOME/linsync/plugin-tmp/<pid>/`)
- The archive's parent directory (to write `<archive>.linsync.tmp.<pid>`)

Read access is granted to the archive path itself.

### Flatpak (file portal)

The archive must be selected through a file-chooser dialog backed by
`org.freedesktop.portal.FileChooser`. That dialog returns a document handle granting
transient read+write access to the specific file. The host passes the handle to
`flatpak-spawn --host` when launching the helper, so the helper inherits exactly that
file's access — no broader host-filesystem access.

`ArchiveEditDialog.qml` must always route archive opening through a portal-backed chooser.
Direct-path editing (CLI-passed paths) is restricted to native builds.

## Per-format helpers

### `zip-editor` (`packaging/plugins/zip-editor/`)

Class `editor`; entry `./zip-editor` (Python script, same pattern as
`packaging/plugins/zip-unpacker/zip-unpacker`); MIME types from
`packaging/plugins/zip-unpacker/linsync-plugin.json`; capability `replace-member`.
Uses `import zipfile`.

### `tar-editor` (`packaging/plugins/tar-editor/`)

Class `editor`; entry `./tar-editor`; MIME types from
`packaging/plugins/tar-unpacker/linsync-plugin.json`; capability `replace-member`.
Uses `import tarfile`.

## API surface

New function in `crates/linsync-core/src/plugin.rs`:

```rust
pub fn replace_archive_member(
    archive: &Path,
    member: &str,
    new_content: &Path,
    options: &PluginExecutionOptions,
) -> Result<(), ArchiveEditError>;
```

`ArchiveEditError` variants: `HelperNotFound`, `MemberNotFound`, `AtomicRenameFailed`,
`HelperError(String)`, `Cancelled`. The function dispatches to the correct editor plugin
by MIME type, fsyncs the temp path, then calls `rename(2)`. On any failure the temp is
removed and the original is unchanged.

## CLI integration

```
linsync-cli archive replace --archive <path> --member <member> --from <new-content>
```

Calls `replace_archive_member` directly. Exits 0 on success, non-zero on any error.

## GUI integration

`ArchiveEditDialog.qml` — context menu entry "Edit member…" on any file row inside a
virtual archive folder. Editable text area (same size limits as text compare); Save invokes
the bridge; Cancel cleans up without mutation.

Bridge: `POST /archive/replace` (`{archive, member, new_content_tmp}`) + cxx-qt invokable
`replace_archive_member(archive, member, content)`.

## Test plan

- **Round-trip:** fixture ZIP and tar archives; replace one member; re-extract; assert
  byte-equality of all unchanged members and correct replacement content.
- **Crash-safety:** `SIGKILL` helper mid-write; verify original archive intact and no
  orphan temp file persists past next startup.
- **Cancellation:** `SIGTERM`; verify temp cleaned up and original untouched.
- **Member-not-found:** non-existent member path; verify `ok: false`, no mutation.
- **Flatpak portal:** mock portal stub; verify helper cannot open paths outside the grant.
- **UID/GID round-trip:** tar with non-root UID/GID; replace a member; verify unchanged
  members still carry original header values.

## Blocking dependencies

- **Phase 6 (sandbox)** — write-scoped sandboxing must exist before shipping.
- **Phase 4 (plugin protocol)** — `replace_member` op and the `editor` manifest class
  must be added alongside `unpacker` and `prediffer`.
- **Flatpak portal spike** — the portal interaction above is design-level; a concrete
  implementation spike is required before a follow-up plan can be written.

## Open issues

- **Encrypted archives** — explicitly deferred; read-only forever until a password-prompt
  security review is done (out of scope for any v1.x milestone).
- **Large archives** — the full-stream rewrite path (tar) may be slow for archives with
  many or large members. Phase 4 streaming progress reporting should be wired before
  shipping.
- **`editor` manifest class** — not yet in `docs/plugin-protocol.md`; must land alongside
  this implementation.
- **Ship decision** — remains "not in v1.x" until a Phase 6 sandbox review concludes.
