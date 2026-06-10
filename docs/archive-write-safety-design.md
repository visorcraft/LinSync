# Writable Archive-Member Editing — Safety Design

> Status: **IMPLEMENTED** (v1, zip only). This document was the safety-design
> gate for writable archive-member editing; all sections through §7 are now
> shipped in `linsync-core/src/archive_write.rs` and the GUI bridge. The design
> remains the authoritative contract for the repack path.

LinSync's archive support is read-only: archives appear as virtual folders,
members are extracted to secure temp locations for compare, and nothing is ever
written back. This design adds exactly one write capability — replacing a
single member of a zip archive with an edited copy — under an atomicity,
sandbox, and UX contract that keeps the original archive unmodifiable by
anything except a final atomic publish step performed by the trusted host.

## 1. Scope v1

In scope:

- **Zip only** (`.zip`, `.jar`, `.war`, `.apk`, `.ipa` — the suffixes the
  built-in `unzip` path in `linsync-cli archive` already recognizes as zip).
- **Single-member replace via repack**: one member's content is replaced; all
  other members, their order, modes, extra fields, and the archive comment are
  preserved byte-for-byte (see §3 working-copy model).

Explicitly out of scope, each with its reason:

- **tar and compressed-tar (`.tar`, `.tgz`, `.tar.gz`, …)** — tar has no
  in-place member replacement; replacing a member requires rewriting the entire
  stream, and the compressed variants additionally require full
  decompress/recompress. That multiplies the partial-write and bomb surface for
  no v1 benefit. Deferred.
- **7z and plugin-provided formats** — replacement depends on helper
  capability we cannot assume. The protocol extension (§5) lets a future
  unpacker opt in; until a repack-capable unpacker exists for a format, that
  format stays read-only.
- **Nested archives** (`outer.zip!/inner.zip!/member`) — writing into an inner
  archive requires recursively repacking every enclosing layer, so a failure at
  layer N corrupts the freshness assumptions of layers N-1..0. Deferred.
- **Multi-member batch edits** — one token edits one member (§8). Batch edits
  need a transaction model across several staged files and one repack; that is
  a separate design once single-member ships.
- **Hex editing of members** — the hex view is read-only by the permanent
  contract in `docs/known-limitations-1.0.md`; this design does not change it.
  Members are edited as files in an external editor, never in-place in LinSync.

## 2. Threat model

| Threat | Vector | Mitigation |
|---|---|---|
| Zip-slip on repack | Member path containing `..`, leading `/`, drive prefix, or NUL escapes the staging root when staged, or smuggles a path into the `zip` invocation | Member paths are validated with the same rules `unpack_folder` imposes on `tree` paths (`docs/plugin-protocol.md` §Security: path traversal). The staged path is `staging_root.join(member)`, canonicalized, and prefix-checked against the staging root. Paths beginning with `-` or `@` are rejected so they can never be parsed as `zip` flags or Info-ZIP `@filelist` specifiers. These argument checks are defense-in-depth, not the only gate — the Landlock/bwrap policy (§4) already contains the blast radius of a misparsed argument. |
| Symlink member as edit target | User asks to edit a member whose zip entry is a symlink (`S_IFLNK` in external attributes); extraction would create a link, and "editing" it edits the link target | `/archive/member/edit` refuses symlink (and any non-regular-file) members with 400. |
| Symlink as repack content | The staged file is replaced by a symlink (by the editor, or by an attacker with staging access) before repack, exfiltrating or injecting the link target | Immediately before repack the host `lstat`s the staged file and requires a regular file. The staging dir lives under app-private `AppPaths::cache_dir` with `0700` parent perms. |
| Mode/permission loss | Repack records the staging file's mode instead of the member's original mode | Host restores the original member's recorded Unix mode (zip external attributes) onto the staged file before repack, so the mode round-trips. Untouched members keep their bytes (working-copy model, §3). |
| Duplicate entry via name re-encoding | Info-ZIP re-encodes member names lacking the UTF-8 flag (cp437 legacy entries); a name mismatch makes `zip` *add* a second member instead of replacing the target | Members whose names fail a UTF-8 round-trip are rejected at edit time with 400. After repack the host asserts member count unchanged and the target member present exactly once before publishing (§5 built-in zip path); violation aborts the commit, original untouched. |
| Archive bomb on staging | A crafted member decompresses to enormous size during the edit-extract | Caps enforced at extraction: max uncompressed member size (default 1 GiB) and max compressed:uncompressed ratio (default 200:1), both checked against the zip central-directory sizes before extraction and re-checked against actual bytes written. Exceeding either aborts with a clear error; nothing is staged. |
| TOCTOU: archive modified externally mid-edit | Another program rewrites the archive between extract and commit; repacking would publish a stale or mixed archive | A freshness fingerprint — size + mtime (nanosecond) + SHA-256 of the whole archive — is captured at `/archive/member/edit` time and stored server-side with the token. Edit-time extraction and fingerprinting run under a shared `flock(LOCK_SH)` on the archive, so the fingerprint provably matches the bytes that were extracted. At commit, size+mtime are checked first (cheap), then the full hash, all under the commit lock. Mismatch → 409, token invalidated, original untouched. |
| Partial-write corruption | Crash or kill mid-repack leaves a truncated archive | The original is never opened for writing. All writing happens to `<archive>.linsync-tmp`; publish is a single atomic `rename(2)` (§3). A crash at any earlier step leaves the original byte-identical. |
| Concurrent edits of the same archive | Two tabs/windows (or two LinSync processes) commit competing repacks | One outstanding edit token per canonical archive path in the bridge's token table (second `/archive/member/edit` → 409). Cross-process: commit takes a non-blocking `flock(LOCK_EX)` on the original for the fingerprint-check + rename window; contention → 409, retry later. |
| Helper escape | The sandboxed `zip` (or plugin) writes outside its grants | Landlock/bwrap policy in §4: write grants are the staging dir and the `.linsync-tmp` file only. The original archive path is never in `write_paths`. |

## 3. Atomicity protocol

The unit of publish is a whole-file rename. Steps, in order:

1. Host copies the original archive to `<archive>.linsync-tmp` in the **same
   directory** (same filesystem ⇒ step 6's `rename(2)` is atomic and cannot
   degrade to copy+delete).
2. Host `fchmod`s the tmp file to the original's mode and, best-effort,
   `fchown`s to the original's uid/gid (`EPERM` for foreign-owned files is
   logged, not fatal — the committing user becomes the owner, which is the
   only possible outcome for an unprivileged process).
3. The sandboxed repack helper updates the **tmp file** — `zip` replaces the
   one member, using the staging dir for its own work files (`-b <staging>`),
   with cwd set to the staging extract root so the member's relative path
   matches its archive path. The original is read-only to the helper.
4. Host re-verifies the freshness fingerprint against the original under
   `flock` (§2 TOCTOU row).
5. Host `fsync`s the tmp file, then hard-links the original to
   `<archive>.bak` (no data copy; same filesystem). Filesystems without hard
   links (FAT32/exFAT, many FUSE mounts) fail the `link(2)` with
   `EXDEV`/`EOPNOTSUPP`; the host then **copies** the original to `.bak`
   instead — the backup is degraded to a copy, never silently skipped. Then
6. `rename("<archive>.linsync-tmp", "<archive>")` and `fsync` the parent
   directory.
7. On confirmed success the `.bak` is **deleted by default**; the boolean
   setting `keepArchiveBackup` (default `false`) retains it for users who want
   a one-edit undo. The success response names the `.bak` path when retained.

Failure at any step ≤ 5 deletes the tmp file and leaves the original
byte-identical; the `.bak` link (created in step 5) is removed only if the
rename did not happen. If step 6's `rename(2)` itself fails (`EACCES`,
`EROFS`, …), the original is untouched — a failed rename changes nothing —
and the host **retains** both the `.bak` and the `.linsync-tmp` for retry: the
token stays valid and the error body names both paths. Failure *after* a
successful rename cannot occur in this
protocol (step 7 is cleanup only); if `.bak` deletion itself fails, the commit
still reports success and the stale `.bak` is reported in diagnostics.

Why a working copy instead of extract-everything-and-rebuild: only the edited
member is recompressed; every other member's compressed bytes, order, extra
fields, and the archive comment survive untouched; there is no full-tree
extraction (smaller bomb surface); and large archives are copied once, not
extracted and re-deflated. The full-rebuild alternative was rejected.

## 4. Sandbox policy for the repack helper

The repack helper runs under `SandboxedCommand` (`linsync-sandbox`), exactly
like the built-in `unzip`/`tar` extraction in `linsync-cli archive`. The
policy is built with `SandboxPolicy::builder()`:

```text
read_paths:  [<staging_dir>, <archive>.linsync-tmp]    // write grants imply
write_paths: [<staging_dir>, <archive>.linsync-tmp]    // read in landlock.rs
network:     false                                      // seccomp socket block
fd_limit / proc_limit: defaults (256 / 8192)
```

Mapping onto the existing primitives: on the Landlock path read paths get
`AccessFs::from_read(abi) | AccessFs::Execute` and write paths get
`AccessFs::from_all(abi)` (see `crates/linsync-sandbox/src/landlock.rs`); on
the `bwrap` fallback they become `--ro-bind`/`--bind` plus `--unshare-net`.
`SandboxStrategy::detect()` ordering (Landlock ≥ ABI 1 → bubblewrap →
degraded-fail-closed) applies unchanged.

Notable tightenings versus the obvious policy:

- The write grant on the publish side is the **`.linsync-tmp` file**, not the
  archive's parent directory — Landlock `PathBeneath` accepts file paths (the
  extract path already grants `.read(archive)` on a file). The helper can
  update the working copy but cannot create or touch siblings, including the
  original archive and the `.bak`.
- The original archive is **not** in the policy at all. The host copies it to
  the tmp before the helper starts, so the helper never needs to read it.
- The atomic publish (steps 4–7 in §3) is performed by the trusted host
  process, never by the helper.

> **Implementation note (v1, as shipped).** Info-ZIP `zip` publishes its
> output by re-creating the destination file (unlink + create), which a
> Landlock *file* grant on `<archive>.linsync-tmp` cannot authorize —
> creation rights live on the parent directory, which is never granted. The
> implementation therefore confines the helpers to the **staging dir alone**
> (a strict subset of the policy above): `zip` repacks a working copy inside
> staging, the post-repack listing runs against that working copy, and the
> trusted host copies the verified bytes to `<archive>.linsync-tmp` and
> performs steps 4–7 itself. No helper receives any grant in the archive's
> directory; every safety property of §3 is preserved.

The v1 built-in repack does **not** go through `policy_for_plugin`: like the
built-in `unzip`/`tar` extraction in `linsync-cli archive`, it constructs the
policy above directly with `SandboxPolicy::builder()`. This is deliberate —
the manifest declares a `writes_input` field (core's `PluginSandbox`), but
`PluginSandboxFields` does not mirror it and `policy_for_plugin` adds
`source_path` to read paths only, so the plugin spawn path cannot express a
writable source today. Plugin repack helpers are blocked on closing that gap
(§5).

## 5. Plugin protocol extension: `repack_member`

A new operation for the `unpacker` class, following the `unpack_folder` style.

**v1 ships the built-in zip path only; plugin `repack_member` is specified
here but deferred, blocked on sandbox API support.** `policy_for_plugin` makes
`source_path` read-only and `PluginSandboxFields` has no `writes_input` field,
so today's plugin spawn path cannot grant a writable working copy. Required
API change before any plugin may repack: add `writes_input` to
`PluginSandboxFields` (mirroring the existing manifest field) and have
`policy_for_plugin` move `source_path` into `write_paths` when it is set —
scoped to the `.linsync-tmp` working copy, plugin dir read-only, staging dir
as the writable temp dir. Until that lands, a manifest declaring
`supports_repack: true` parses cleanly but is never offered the op.

Manifest opt-in (defaults to `false`; absent field keeps every existing
manifest valid and read-only):

```json
{
  "classes": ["unpacker"],
  "supports_repack": true
}
```

### Request

```json
{
  "op": "repack_member",
  "source": "/path/to/archive.zip.linsync-tmp",
  "member": "docs/readme.txt",
  "replacement": "/path/to/staging/extract/docs/readme.txt"
}
```

Fields:

- `op` — must be `"repack_member"`.
- `source` — absolute path to the **working copy** of the archive (writable in
  the sandbox policy). Never the original.
- `member` — member path relative to the archive root, `/`-separated, subject
  to the same traversal rules as `unpack_folder` `tree` paths.
- `replacement` — absolute path to the staged regular file whose content
  replaces the member.

### Response

```json
{ "ok": true }
```

```json
{ "ok": false, "error": "member not found in archive" }
```

The plugin must update `source` in place (or via a temp file inside its
assigned temp dir followed by a same-filesystem rename onto `source`), must
not touch any path outside its grants, and must leave `source` either fully
updated or unmodified — the host discards the working copy on any failure, so
a half-written working copy is never published.

### Built-in zip path

The built-in implementation does not use a plugin: it invokes the system
`zip` binary under `SandboxedCommand`, mirroring how built-in extraction uses
`unzip` (`crates/linsync-cli/src/main.rs::extract_archive`):

```text
cwd=<staging extract root>  zip -q -b <staging_dir> <tmp-archive> <member>
```

`-b` keeps zip's own temp file inside the staging grant.

Two guards close Info-ZIP's name-encoding hole (a cp437 legacy name that
`zip` re-encodes would be *added* as a second entry rather than replaced):

- **Edit-time:** members whose names fail a UTF-8 round-trip are rejected at
  `/archive/member/edit` with 400 "member name encoding not supported for
  editing" — simpler and safer than attempting a replacement `zip` cannot
  address by name.
- **Post-repack:** before §3 step 4, the host lists the working copy
  (`unzip -l`/`zipinfo` parse, under the same sandbox policy) and asserts the
  member count is unchanged and the target member appears exactly once.
  Violation aborts the commit; the original is untouched.

### Capability detection

A member's "Edit a copy and repack" action is offered only when the archive
format has a repack path: built-in zip, or an installed-and-enabled unpacker
whose manifest declares `supports_repack: true` for the extension. Otherwise
the action is **hidden** (not greyed) — read-only formats simply do not grow a
write affordance.

## 6. UX contract

- Archives stay read-only by default. There is no global "writable archives"
  mode and no edit-on-double-click.
- Flow, per member:
  1. Context action **"Edit a copy and repack…"** on a zip member row.
  2. `POST /archive/member/edit` extracts the member (caps from §2) into
     app-private staging under `AppPaths::cache_dir()/archive-edits/<token>/`
     (portal backups under `state_dir/archive-edit/<token>.bak`) and returns
     the token.
  3. The GUI opens the staged file with the existing `/open-external` route
     (`xdg-open`). The member row shows an "editing" badge with the staging
     path, making explicit that the user is editing a copy.
  4. The user signals done with an explicit **"Repack…"** button on the badge.
     Explicit signal is chosen over file-watching: the bridge is HTTP/JSON
     polled by QML, so a host-side inotify watch would still need a polling
     round-trip to surface; editors write through temp-file-rename dances that
     make watch events ambiguous; and saving in the editor must not be the
     destructive trigger — the user may save several drafts before intending
     to commit. The explicit button matches the staged-plan-before-write rule
     in `docs/SECURITY.md`.
  5. *(As built — amends the original `confirm_repack` dialog design.)* The
     edit banner carries explicit **Commit** and **Discard** buttons naming
     the member and side; commit sends `GET /archive/member/commit?token=…`
     directly. The separate confirm dialog (and the 409-without-
     `confirm_repack=1` gate it implied) was dropped: the banner is already a
     persistent, explicit, member-named affordance, and the destructive
     trigger remains a deliberate click distinct from saving in the editor.
  6. On success the compare refreshes (re-runs the virtual-folder compare so
     the new member content/hash shows immediately).
- Failure surface *(as built — amends the original token-invalidation rule)*:
  any commit failure reports "original archive untouched" in the error body
  and names the retained `.bak`/staging paths when they exist. The staging
  dir — the user's only copy of their edit — is **never deleted on failure**,
  and the token stays registered (`token_retained` in the error body) so the
  edit remains owned: retry is meaningful after `RenameFailed`
  (`retryable: true`), while a freshness mismatch keeps failing with 409
  until the user discards (the stale fingerprint can never publish, so
  re-extraction is still required to commit — but the staged bytes stay
  recoverable instead of being destroyed with the token). A "Discard" action
  on the banner abandons the edit and deletes the staging dir and any portal
  backup. Staging dirs and portal backups orphaned by a crash are reclaimed
  by an age-gated sweep at GUI startup.

## 7. Flatpak

The shipped manifest grants `--filesystem=home` and `--filesystem=/run/media`,
so for most archives the Flatpak behaves like the native build: the parent
directory is directly writable and the full §3 atomic protocol applies.

The hard case is a **portal-granted file**: a distributor-stripped manifest, or
an archive outside the granted set opened through the FileChooser portal. The
document portal grants access **per file, not per directory** — the app sees
the archive at `/run/user/<uid>/doc/<doc-id>/<filename>` on the portal's FUSE
mount. Consequences:

- The visible parent directory is the doc-portal mount, not the real one. The
  app cannot create `<archive>.linsync-tmp` or `<archive>.bak` as siblings,
  and `rename(2)` over the original is impossible. The atomic publish path
  **cannot work** for portal-granted archives. No portal call fixes this short
  of asking the user to grant the whole directory, which the FileChooser
  portal does not offer for a file selection.
- Chosen mitigation: **detect and degrade explicitly.** Paths under
  `/run/user/<uid>/doc/` are detected at `/archive/member/edit` time. For
  them, commit falls back to a **non-atomic copy-back through the portal FD**:
  the working copy is built in app-private staging, fsynced, then written over
  the original via `O_TRUNC` + write + fsync. Because no sibling `.bak` can
  exist, the backup copy is kept in app-private XDG state
  (`AppPaths::state_dir()/archive-edit/<token>.bak`) and is **always retained**
  for portal commits regardless of `keepArchiveBackup`. The confirm dialog for
  this path carries distinct wording: atomic replace is unavailable, a crash
  mid-write can corrupt the archive, and the backup's exact path. The 409
  warning body differs accordingly so the GUI cannot show the atomic-path text
  by mistake. The portal grant may also be **read-only** (FileChooser does not
  guarantee write access): when `open(O_WRONLY)` on the portal path fails,
  commit returns 500 with the original untouched, and the error body names the
  state-dir backup path so the edited bytes remain recoverable from staging.
- The alternative — requiring users to grant directory access — was rejected:
  it pushes a Flatpak permission decision onto every edit and trains users to
  widen sandbox holes.
- Staging needs nothing new: `AppPaths::cache_dir`/`state_dir` resolve inside
  the app's own XDG dirs, already covered by the existing
  `--persist=.cache/linsync` / `--persist=.local/state/linsync` finish-args.

## 8. Bridge endpoints (sketch)

```text
POST /archive/member/edit?archive=<path>&member=<path>
  → 200 {"ok":true,"token":"<128-bit hex>","staging_path":"…",
          "member":"…","atomic":true|false}        // atomic=false ⇒ portal path
  → 400 invalid/symlink/non-regular member, non-UTF-8 member name,
        caps exceeded, non-zip
  → 404 archive or member not found
  → 409 an edit token is already outstanding for this archive

POST /archive/member/commit?token=<token>&confirm_repack=1
  → 409 + warning body when confirm_repack is absent (Phase 2 pattern)
  → 200 {"ok":true,"bak_path":…|null}
  → 409 freshness fingerprint mismatch (token invalidated) or flock contention
  → 410 unknown/expired token
  → 500 repack failed; body states the original is untouched

POST /archive/member/discard?token=<token>
  → 200; deletes the staging dir, frees the token
```

The token is 128 bits drawn from the OS CSPRNG (`OsRng`/getrandom-backed),
hex-encoded — unguessable, not merely unique. It is an opaque server-side id
mapping to
`{archive_path, member_path, staging_path, fingerprint, atomic}` held in the
bridge process. **Commit and discard never accept client-supplied paths** —
the token is the only handle, so a confused or malicious bridge client cannot
redirect a repack at an arbitrary file. Tokens die with the bridge process
(staging dirs under the cache dir are swept on next launch, like other cache
content).

## 9. Testing strategy

Implementation must ship at least these tests (names indicative):

- `archive_edit_commit_replaces_member_atomically` — e2e happy path: edit,
  commit, re-list members, untouched members byte-identical, edited member
  updated, mode preserved.
- `archive_edit_commit_rejects_without_confirm_409` — Phase 2 pattern reuse.
- `archive_edit_commit_rejects_stale_fingerprint` — TOCTOU: rewrite the
  archive between edit and commit; commit → 409, original (new) bytes intact,
  token invalidated.
- `archive_edit_failure_leaves_original_untouched` — inject a failing repack
  (helper exits nonzero / is killed mid-write); original byte-identical, tmp
  removed, error surfaced.
- `archive_edit_rejects_zip_slip_member_paths` — `../`, absolute, drive
  prefix, NUL, leading `-`, leading `@` members all → 400, nothing staged.
- `archive_edit_rejects_non_utf8_member_name_and_asserts_count` — cp437
  member name → 400 at edit time; a simulated repack that adds a duplicate
  entry fails the post-repack member-count assertion, original untouched.
- `archive_edit_bak_falls_back_to_copy_on_exdev` — `link(2)` failure
  (FAT32/exFAT/FUSE-style `EXDEV`/`EOPNOTSUPP`) yields a copied `.bak`, never
  a missing backup.
- `archive_edit_rejects_symlink_member_and_symlinked_replacement` — symlink
  zip entry refused at edit; staged file swapped for a symlink refused at
  commit.
- `archive_edit_staging_enforces_size_and_ratio_caps` — bomb member (declared
  and actual) aborts extraction with a clear error.
- `archive_edit_concurrent_second_edit_rejected` — second edit token for the
  same archive → 409; cross-process flock contention → 409.
- `archive_edit_bak_deleted_on_success_kept_via_setting` — both
  `keepArchiveBackup` states.
- `archive_edit_portal_path_detected_and_degrades` — `/run/user/<uid>/doc/…`
  style path yields `atomic:false`, state-dir `.bak`, copy-back commit.
- `repack_helper_cannot_write_outside_grants` — real-enforcement test in the
  `just test-sandbox` suite: helper attempts writes to the original archive,
  a sibling, and `$HOME`; all denied under Landlock and under the bwrap
  fallback.
- `repack_member_plugin_roundtrip` — deferred with plugin repack (§5): fixture
  unpacker with `supports_repack: true` performs the protocol exchange;
  manifest without the field never receives the op and the GUI hides the
  action. For v1, only the manifest-parses-but-no-op-offered half applies.

## 10. Open questions for the reviewer

1. **`.bak` default:** delete-on-success is proposed (§3) to avoid littering
   user directories; is a retained-by-default first release safer for early
   adopters, flipping the default later?
2. **Cap defaults:** 1 GiB member / 200:1 ratio are starting points; should
   they share configuration with the existing plugin output-size limits
   instead of introducing new knobs?
3. **Token lifetime:** tokens currently die with the bridge process; should a
   long-lived edit survive a GUI restart (persist the token table to state
   dir), or is forcing re-extract after restart acceptable for v1?
4. **`flock` scope:** advisory `flock` protects against other LinSync
   instances only; non-cooperating writers are covered solely by the
   fingerprint check. Is that sufficient, or should commit also compare the
   original against the `.bak` link post-rename as a belt-and-braces audit?
5. **CLI parity:** should `linsync-cli archive` grow a matching
   `--repack-member` subcommand in the same milestone, or is GUI-only
   acceptable for v1 (CLI can follow once the core API exists)?
