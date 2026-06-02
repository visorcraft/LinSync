# Security

This document tracks security requirements for features that execute helpers,
read untrusted files, extract archives, fetch URLs, or modify user data.

Initial rules:

- Archive extraction must reject path traversal and symlink escapes.
  `docs/archive-support.md` defines the read-only helper strategy and extraction
  security tests required before archive compare is enabled.
- Helper processes must have timeouts, cancellation, size limits, stderr capture,
  and cleanup. The core plugin runner enforces these limits for inline and
  temp-file-backed unpacker and prediffer text operation MVPs, and rejects
  plugin output paths outside the assigned temp directory.
- Webpage/URL compare must be opt-in and document network/cache/cookie behavior.
  The detailed privacy boundary is in `docs/webpage-compare.md`. The
  bundled `packaging/plugins/web-fetch/web-fetch` helper enforces an SSRF
  policy: only `http` / `https` schemes are honoured, the host must resolve
  to a publicly-routable address (no private, loopback, link-local,
  multicast, reserved, or unspecified ranges), and HTTP redirects are
  revalidated against the same policy. The env var
  `LINSYNC_WEB_FETCH_ALLOW_LOOPBACK=1` exists **only** to let
  `crates/linsync-core/src/webpage.rs` integration tests reach their local
  `httptest` fixture servers; it relaxes loopback (127.0.0.0/8 and ::1)
  while keeping every other restriction in place. Do not set this in any
  production install or release-build environment.
- Document/OCR helpers are local by default and document their temporary-file
  cleanup, privacy behavior, and helper limits. The implemented design is in
  `docs/document-compare-implementation.md`.
- Folder sync and merge operations must stage a plan before destructive writes.
- Delete operations must prefer FreeDesktop Trash and clearly confirm permanent
  deletion when Trash is unavailable.
- The GUI's local HTTP/JSON bridge (`apps/linsync-gui/src/main.rs`) binds only
  to `127.0.0.1`, refuses requests whose `Origin:` header is not loopback,
  emits no `Access-Control-Allow-Origin` header, caps request size at 32 KiB
  and header count at 64, and applies a 5-second read/write timeout per
  connection. Mutating endpoints (`/copy`, `/copy-all`, `/save`, `/undo`,
  `/redo`, `/tab/activate`, `/tab/close`) must continue to reject row indices
  that would force unbounded pane growth. The launch-context JSON written
  under `$XDG_CACHE_HOME/linsync/gui/launch-<pid>.json` is created mode `0o600`
  to keep recent-paths and session metadata out of other local users' view.
  These gates are covered by the bridge tests in
  `apps/linsync-gui/src/main.rs` (`bridge_responses_do_not_advertise_wildcard_cors`,
  `bridge_rejects_cross_origin_requests`, `bridge_accepts_loopback_origin`,
  `copy_row_rejects_out_of_range_index`).
