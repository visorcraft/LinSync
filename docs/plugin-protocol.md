# Plugin Protocol

This document defines the first LinSync plugin protocol. Core manifest types,
manifest validation, filesystem discovery helpers, and bounded helper process
execution exist in `linsync-core`. Core operation APIs for inline or
temp-file-backed `unpack_text` and `prediff` text responses also exist. Plugin
settings UI, packaged sandbox behavior, streaming lifetimes for very large
outputs, and archive-helper security stress tests are still pending.

LinSync does not support Windows-only in-process plugins. Linux plugins are external helper
processes with a JSON protocol over stdin/stdout. This keeps the core process
isolated from plugin crashes, language runtimes, and unsafe parsers.

## Plugin Classes

The protocol reserves these plugin classes:

- `unpacker`: converts an input file into text, a virtual folder listing, or a
  extracted member stream.
- `prediffer`: normalizes text before comparison while preserving original input
  for display and merge output.
- `editor_complement`: supplies read-only annotations, navigation metadata, or
  external editor hints for an already loaded comparison.
- `external_viewer`: opens a selected file/member in another application.
- `folder_virtualizer`: presents non-directory inputs such as archives or
  document containers as virtual folders.

The first implementation should target `unpacker` and `prediffer` only. Other
classes are protocol placeholders until the UI and security model need them.

## Locations

User plugins are discovered under:

```text
$XDG_DATA_HOME/linsync/plugins/<plugin-id>/
```

System plugins may be discovered under standard data directories:

```text
/usr/share/linsync/plugins/<plugin-id>/
/usr/local/share/linsync/plugins/<plugin-id>/
```

Each plugin directory contains a manifest named `linsync-plugin.json`.

## Manifest

Manifest JSON is UTF-8 and human-readable. Unknown fields must be ignored by
older LinSync versions unless the manifest declares a newer required schema.

```json
{
  "schema_version": 1,
  "id": "example.text-normalizer",
  "name": "Example Text Normalizer",
  "version": "1.0.0",
  "license": "MIT",
  "entry": ["./normalize-text"],
  "classes": ["prediffer"],
  "mime_types": ["text/plain"],
  "extensions": ["txt", "log"],
  "capabilities": ["streaming-output", "deterministic-output"],
  "deterministic": true,
  "sandbox": {
    "network": false,
    "writes_input": false,
    "requires_home_access": false
  }
}
```

Required fields:

- `schema_version`: currently `1`.
- `id`: stable reverse-DNS-like identifier.
- `name`: display name.
- `version`: plugin version string.
- `license`: SPDX license expression.
- `entry`: executable path plus fixed arguments, relative to the plugin
  directory unless absolute.
- `classes`: one or more plugin classes.
- `mime_types`: MIME types the plugin can handle.
- `extensions`: lowercase filename extensions without leading dots.
- `capabilities`: protocol features supported by the helper.
- `deterministic`: whether identical inputs should produce identical outputs.
- `sandbox`: declared trust and access requirements.

Manifest validation must reject unknown required schema versions, missing entry
executables, path traversal in relative entries, duplicate plugin IDs, and
licenses blocked by project policy.

## Process Model

LinSync starts a plugin process per operation. The initial implementation should
not load dynamic libraries in-process.

The core helper runner already sends stdin, captures stdout/stderr, enforces
timeout and cancellation, limits stdout/stderr size, and removes its temporary
directory after execution.

Host responsibilities:

- Send exactly one JSON request on stdin.
- Close stdin after writing the request.
- Read exactly one JSON response from stdout.
- Capture stderr for diagnostics.
- Enforce timeout, cancellation, stdout size, stderr size, and temp-file cleanup
  limits.
- Treat malformed JSON, nonzero exit status, timeout, and oversized output as
  plugin errors.

Plugin responsibilities:

- Never modify input files.
- Write machine-readable JSON to stdout only.
- Write human diagnostics to stderr only.
- Return nonzero on unrecoverable plugin failures.
- Keep output deterministic when the manifest says it is deterministic.

## Requests

Every request has a protocol version, operation, input descriptors, and options.
Large file contents are passed by path or file descriptor, not embedded in JSON.

```json
{
  "protocol_version": 1,
  "operation": "prediff",
  "request_id": "8fd58b42-8f4d-4ca8-a6f2-40d2757f1a63",
  "inputs": [
    {
      "role": "left",
      "path": "/tmp/linsync/input-left.txt",
      "display_name": "left.txt",
      "mime_type": "text/plain",
      "extension": "txt",
      "read_only": true
    }
  ],
  "options": {
    "encoding": "utf-8",
    "line_ending": "lf",
    "language": "eng"
  }
}
```

`options.language` is an optional ISO 639-2 language hint (omitted when unset).
Text-extractor / OCR plugins (e.g. `tesseract-ocr`) use it to select the
recognition language; plugins that do not need it ignore the field.

Supported initial operations:

- `probe`: ask whether the plugin supports the provided input descriptors.
- `prediff`: produce normalized text for one or more text inputs.
- `unpack_text`: extract text from a non-text input.
- `list_virtual_folder`: list members for a virtual folder input.
- `unpack_folder`: produce a virtual folder tree from an archive or archive-like
  file (see below).

## Responses

Successful responses include `status: "ok"` and one or more outputs. Outputs
may reference temp files created by the plugin under the host-provided temp
directory or may include small inline strings when the host allows it. The
current core unpacker/prediffer MVP accepts both `inline_text` text outputs and
text `path` outputs confined to the assigned plugin temp directory. File-backed
text outputs are read before the host removes the plugin temp directory and are
subject to the configured output size limit.

```json
{
  "protocol_version": 1,
  "request_id": "8fd58b42-8f4d-4ca8-a6f2-40d2757f1a63",
  "status": "ok",
  "outputs": [
    {
      "role": "left",
      "kind": "text",
      "path": "/tmp/linsync/plugin-output-left.txt",
      "encoding": "utf-8",
      "line_ending": "lf"
    }
  ],
  "diagnostics": []
}
```

Error responses use `status: "error"` and must include a stable code:

```json
{
  "protocol_version": 1,
  "request_id": "8fd58b42-8f4d-4ca8-a6f2-40d2757f1a63",
  "status": "error",
  "error": {
    "code": "unsupported-input",
    "message": "PDF text extraction is not supported by this plugin"
  },
  "diagnostics": [
    {
      "severity": "warning",
      "message": "Skipped embedded image content"
    }
  ]
}
```

Common error codes:

- `unsupported-input`
- `invalid-options`
- `temporary-file-failed`
- `output-too-large`
- `cancelled`
- `internal-error`

## Security Boundaries

Plugins are untrusted unless the user or distributor explicitly enables them.
The host must not execute downloaded plugins automatically.

Minimum host safeguards:

- Resolve plugin entries inside the plugin directory unless an absolute path is
  deliberately allowed by policy.
- Reject manifests with unknown or incompatible licenses.
- Run helpers with timeouts and cancellation.
- Limit stdout, stderr, extracted files, and virtual folder entry counts.
- Use secure temporary directories.
- Reject plugin output paths outside the assigned temp/output directory.
- Treat archive paths, symlinks, and member names as untrusted data.
- Preserve original inputs for display and merge output when prediffers
  normalize text.

Flatpak builds may not be able to execute arbitrary host plugins without extra
permissions or portals. Flatpak-specific plugin support must be documented and
tested before plugin execution is enabled in packaged builds.

## Sandboxing

Plugin helpers run inside a `linsync-sandbox` policy whenever the core is
built with the `sandbox` feature (default-on). The policy is derived from the
manifest's `sandbox` block and the request's `source` path:

| Manifest field            | Sandbox effect                                                       |
| ------------------------- | -------------------------------------------------------------------- |
| `network: false` (default) | seccomp-bpf blocks `socket()` family calls inside the helper        |
| `network: true`            | network syscalls are permitted (use only for `web-fetch`-style ops) |
| `writes_input: false`      | Landlock makes the source path read-only                            |
| `writes_input: true`       | Landlock permits writes to the source path                          |
| `requires_home_access: …`  | Reserved for future per-helper home-tree access (not yet enforced)  |

The host enforces the policy through Landlock + seccomp-bpf on Linux
kernels ≥ 5.13 (the primary path). On older kernels it falls back to
`bwrap` (bubblewrap). If neither is available — minimal containers,
exotic kernels, or `LINSYNC_SANDBOX_SKIP=1` in the environment — the
host enters **degraded mode**: it logs a `tracing::warn!` and runs the
helper unsandboxed. Degraded mode preserves the per-invocation temp
directory, timeout, and stdout/stderr caps; only the kernel-level
filesystem/network policy is unenforced.

`docs/sandbox-design.md` and the `linsync-sandbox` crate documentation
cover the strategy detection logic and the Flatpak portal interaction.

## `unpack_folder` Operation

The `unpack_folder` operation asks a plugin to inspect an archive (or
archive-like file) and return a virtual folder tree that LinSync can display and
compare as if it were a real directory.

### Request

```json
{
  "op": "unpack_folder",
  "source": "/path/to/archive.zip"
}
```

Fields:

- `op` — must be `"unpack_folder"`.
- `source` — absolute path to the file to unpack.

### Response

```json
{
  "ok": true,
  "tree": [
    { "path": "docs/readme.txt", "kind": "file", "size": 1024, "sha256": "deadbeef..." },
    { "path": "docs",            "kind": "dir" }
  ]
}
```

On failure:

```json
{
  "ok": false,
  "error": "unsupported archive format"
}
```

Fields:

- `ok` — `true` on success, `false` on failure.
- `tree` — list of `VirtualNode` objects describing the archive members (present
  when `ok` is `true`; may be absent or empty on failure).
- `error` — human-readable description of the failure (present when `ok` is
  `false`; may be absent when `ok` is `true`).

Each `VirtualNode` has:

- `path` — member path relative to the archive root, using `/` as the separator.
- `kind` — `"file"` or `"dir"`.
- `size` — uncompressed byte size (optional, files only).
- `sha256` — lowercase hex SHA-256 of the uncompressed content (optional, files
  only).

### Security: path traversal

Paths in `tree` **MUST NOT** contain `..` components, leading `/`, or Windows
drive prefixes. This is a plugin responsibility — the host does not sanitise
`tree` paths before use in order to avoid silently discarding entries. Plugins
that produce archive trees must validate every member path and skip or reject
any entry that would escape the archive root (zipslip/symlink-slip protection).

## Streaming Responses

Plugins that need to emit incremental progress or paginate large results can opt
in to a length-prefixed chunk protocol by declaring `"streaming": true` in their
manifest.  The field defaults to `false`; existing plugins with no `streaming`
key continue to use the single-shot one-JSON-response path unchanged.

When `streaming` is `true` the host calls `run_streaming_plugin` instead of the
normal helper runner.  The plugin emits zero or more chunks on stdout; each
chunk is framed as:

```
[4-byte little-endian u32 length][chunk JSON bytes]
```

The host reads frames until:

- **EOF** — the plugin closed stdout; all chunks collected so far are returned.
- **Timeout or cancellation** — the host kills the child and returns an error.
- **Total-bytes cap** — when the cumulative payload size would exceed
  `PluginExecutionOptions::max_total_bytes` (default 64 MiB) the host kills
  the child and returns `PluginError::StreamTotalBytesExceeded`.
- **Truncated frame** — if the plugin closes stdout mid-chunk the host returns
  `PluginError::TruncatedChunk`.

### Chunk schema

Chunk JSON is **op-specific and opaque to the host**.  The host forwards raw
bytes to the caller as `PluginChunk` values.  Callers decode chunks with
`PluginChunk::parse_json::<T>()`.  There is no envelope or protocol version
wrapper at the chunk level.

### Minimal streaming plugin example

```bash
#!/usr/bin/env bash
read REQ
emit() {
    local json="$1"
    local len=${#json}
    printf '%b' "$(printf '\\x%02x\\x%02x\\x%02x\\x%02x' \
        $(( len        & 0xff )) \
        $(( (len >> 8) & 0xff )) \
        $(( (len >> 16) & 0xff )) \
        $(( (len >> 24) & 0xff )))"
    printf '%s' "$json"
}
emit '{"index":0,"msg":"first"}'
emit '{"index":1,"msg":"second"}'
emit '{"index":2,"msg":"third"}'
```

Corresponding manifest fragment:

```json
{
  "streaming": true
}
```

### Error variants added for streaming

| Variant | Meaning |
|---|---|
| `NotStreaming` | `run_streaming_plugin` called on a manifest without `streaming: true`. |
| `StreamTotalBytesExceeded` | Cumulative chunk bytes exceeded `max_total_bytes`. |
| `TruncatedChunk` | Plugin closed stdout inside a chunk frame. |

## Compatibility Notes

Third-party plugins are behavioral references only. LinSync must not copy external
plugin code, filters, bundled examples, or translations unless a later
file-specific licensing review proves GPL-3.0-only compatibility.
