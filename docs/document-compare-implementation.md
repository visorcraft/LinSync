# Document & OCR Compare Implementation Design

> Status: design — supersedes `docs/document-ocr-compare.md`'s deferral.

## Goals

Enable `.pdf`, `.docx`, `.odt`, and image-format compare via three new unpacker
plugins (system-discovered, not bundled). OCR-as-text via Tesseract. All helpers
run inside the Phase 6 sandbox; no network access; no new Cargo dependencies.
Satisfies all seven prerequisites from `docs/document-ocr-compare.md`.

## Non-goals

- A pixel-accurate visual overlay of diff highlights on a rendered source page
  (see the carve-out under "OCR with positions" below). Per-word positions are
  now surfaced as data; drawing them back onto a scaled page render is not.
- Remote/cloud OCR of any kind.
- Bundling Tesseract, Poppler, or LibreOffice in the LinSync package.
- Encrypted document support.
- Windows or macOS; this is a Linux-only feature.

## Compare paths

### Document-as-text via unpacker plugins

Three plugins extract text from a source file into a sandboxed temp file; LinSync's
text engine diffs the two outputs. All use the `unpack_text` operation; inputs
passed by path, outputs written to the assigned temp dir.

| Input format | Plugin ID | Helper binary |
|---|---|---|
| PDF | `com.visorcraft.linsync.pdf-to-text` | `pdftotext` (Poppler) |
| Office (docx/odt/rtf) | `com.visorcraft.linsync.libreoffice-extract` | `libreoffice --headless` |
| Image (png/jpg/tiff/webp) | `com.visorcraft.linsync.tesseract-ocr` | `tesseract` |

### Rendered document/image compare

Not a new code path: render pages to PNG with `pdftoppm` (Poppler) as a
pre-processing step inside the `pdf-to-text` plugin, then hand the image pair to
the image compare engine. No additional helper or plugin is needed.

### OCR-as-text

The `tesseract-ocr` plugin shells out to `tesseract <input> stdout -l <lang> txt`
and writes the result to `stdout.txt` in its temp dir. The plugin exposes a
manifest option `language` (default `eng`). LinSync passes the user-selected
language via the `options` block in the `unpack_text` request.

### OCR with positions (implemented)

Tesseract's `tsv` output mode exposes per-word bounding boxes, and LinSync now
captures them through the (optional, backward-compatible) plugin-protocol fields
added for this purpose:

1. The document engine sets `options.want_positions: true` on the `unpack_text`
   request whenever `mode` is `OcrText` (`compare_document_files` in
   `crates/linsync-core/src/document.rs`).
2. The `tesseract-ocr` plugin, seeing `want_positions`, additionally runs the
   `tsv` config, parses the level-5 word rows (grouped by block/paragraph/line
   into per-line arrays in reading order), and attaches them as
   `PluginOperationOutput.word_positions` — a `Vec<Vec<WordPosition>>`. Each
   `WordPosition` carries `text`, the 0-based `line`, an image-pixel bounding box
   (`x`, `y`, `width`, `height`), and an optional integer `confidence` (`0`–`100`).
3. The engine threads each side's positions onto
   `DocumentCompareResult.{left,right}_word_positions`, and the `/compare/document`
   bridge serializes them as `left_word_positions` / `right_word_positions`.

The protocol version stays **`1`** — both `want_positions` (request) and
`word_positions` (response) are purely additive and omitted when unused, so older
plugins are unaffected (see `docs/plugin-protocol.md`).

**Known carve-out — no visual overlay over the rendered page.** Positions are
surfaced as *data* (the per-word boxes plus their counts), not painted as diff
highlights on top of a rendered source page. There is deliberately **no
zoom/scale mapping** from the OCR'd image-pixel coordinate space onto a
display-time page raster: the boxes are in the resolution Tesseract saw, and the
GUI does not rescale or align them to a separately-rendered page. Drawing an
accurate, zoom-aware overlay (matching the image-compare overlay treatment)
remains a documented limitation.

## Helper stack

| Helper | License | Distribution | New `deny.toml` entry needed |
|---|---|---|---|
| `pdftotext` / `pdftoppm` (Poppler) | GPL-2.0+ | system-discovered | Yes — GPL-2.0-or-later |
| `tesseract` | Apache-2.0 | system-discovered | No — already allowed |
| `libreoffice` headless | MPL-2.0 | system-discovered | Yes — MPL-2.0 |

**Runner-up for PDF render: mupdf (`mutool`).** Rejected — AGPL-3.0-only is
explicitly blocked by `docs/licensing.md`. Process-boundary argument would
technically avoid AGPL's network clause, but project policy rules it out.

**Runner-up for PDF render: PDFium.** Rejected — BSD-3-Clause but no stable
system package on Linux; bundling would require a separate bundling-policy
review. Deferred.

**Runner-up for office docs: pandoc.** Rejected as primary — GPL-2.0+ (same
deny.toml concern as Poppler), heavier dependency, and LibreOffice is already
installed on most target systems and owns the odt/docx formats natively.

**OCR engine: Tesseract over PaddleOCR.** PaddleOCR requires a large Python/C++
runtime (hundreds of MB) impractical as a system dep. Tesseract is a single
binary, Apache-2.0, and in every major Linux distribution's default repos.

## License compatibility analysis

LinSync is GPL-3.0-only. Helpers shelled out to via `execvp` are **not linked**;
they are separate processes. The process boundary is the license boundary.
GPLv3 §5 covers separately-running processes communicating only through
stdin/stdout. LinSync does not combine helper code into its own executable.

Consequence per helper:

- **Poppler (`pdftotext`)** — GPL-2.0+. Shelled out, not linked; GPL-2.0+ binds
  Poppler's own redistribution, not LinSync's. No Cargo entry needed (no Poppler
  Rust crate added). `third-party-notices.md` must note the runtime dependency.
- **LibreOffice** — MPL-2.0. Same logic: shelled out, not linked. MPL file-level
  copyleft does not propagate to LinSync. No issue.
- **Tesseract** — Apache-2.0. Permissive; no GPL interaction concern.

**Required `docs/third-party-notices.md` update:** add a "Runtime helper
dependencies" section listing all three binaries, their licenses, and the
source-offer note (system packages; source available from upstream).

## Plugin manifests

All three manifests follow the `linsync-plugin.json` schema (schema_version 1).
Key common fields: `"classes": ["unpacker"]`, `"sandbox": {"network": false, "writes_input": false}`.
Plugin-specific fields:

| Field | pdf-to-text | libreoffice-extract | tesseract-ocr |
|---|---|---|---|
| `id` | `com.visorcraft.linsync.pdf-to-text` | `com.visorcraft.linsync.libreoffice-extract` | `com.visorcraft.linsync.tesseract-ocr` |
| `entry` | `["./pdf-to-text.sh"]` | `["./libreoffice-extract.sh"]` | `["./tesseract-ocr.sh"]` |
| `mime_types` | `["application/pdf"]` | docx, odt, rtf MIME types | png, jpeg, tiff, webp, pdf MIME types |
| `extensions` | `["pdf"]` | `["docx","odt","rtf"]` | `["png","jpg","jpeg","tiff","webp","pdf"]` |
| `requires_system_binary` | `pdftotext` | `libreoffice` | `tesseract` |
| `options_schema` | `page_range` (string, default `"all"`) | — | `language` (string, default `"eng"`) |

## Privacy controls

- **No network access.** All three plugins declare `"network": false`; the Phase
  6 sandbox enforces this at the kernel level. No document content leaves the device.
- **Temp-file cleanup.** Plugins write only to the per-invocation temp dir
  (`$XDG_CACHE_HOME/linsync/plugin-tmp/<pid>/`). The host's `Drop`-guarded
  `TempDir` removes it after reading output; nothing persists across runs.
- **Image retention toggle.** Intermediate rendered PNGs are controlled by
  `DocumentCompareOptions::retain_rendered_pages` (default `false`). When false,
  PNGs are deleted immediately after the image engine finishes; when true, kept
  in `$XDG_CACHE_HOME/linsync/rendered-pages/<session-id>/` until session close.
- **No remote OCR.** Remote integrations require a new separately-reviewed plugin
  with explicit consent UI. Out of scope for this phase.

## Sandbox interaction (depends on Phase 6)

Each helper runs in the Phase 6 sandbox: read-only bind-mount of the source
file's directory; write access only to the per-invocation temp dir; network
denied; capabilities dropped to none after `execvp`. Timeout defaults to 30 s;
output cap defaults to 64 MiB (both via `PluginExecutionOptions`).

Flatpak: `pdftotext`, `libreoffice`, and `tesseract` are available inside the
`org.freedesktop.Platform` runtime or as system extensions. No new
`--filesystem=` portal declarations are needed — the file-chooser portal already
grants access to the user's chosen compare target paths.

## CLI integration

```
linsync-cli compare --mode document a.pdf b.pdf
linsync-cli compare --mode document --ocr-language fra a.png b.png
linsync-cli compare --mode document a.docx b.odt
```

The `--mode document` flag selects the best unpacker plugin based on MIME type /
extension. `--ocr-language` is passed through to the `tesseract-ocr` plugin's
`options.language`. Exit codes follow the existing text-compare semantics (0 =
identical, 1 = differs, 2 = error).

## GUI integration

`apps/linsync-gui/qml/DocumentComparePage.qml` structure:

- **Header toolbar:** mode toggle ("Extracted Text" | "OCR Text"); OCR language
  `ComboBox` (visible only in OCR mode, populated from `tesseract --list-langs`);
  `Kirigami.InlineMessage` for helper-not-found errors (e.g. "pdftotext not
  found — install poppler-utils").
- **Body:** `SplitView` with left and right `TextDiffView` panels reusing the
  existing text diff component.
- **Settings subsection** (in `SettingsPage.qml`, document group):
  - `ocrLanguage` — string, default `"eng"`.
  - Temp-file location — display-only, resolves to `$XDG_CACHE_HOME/linsync/plugin-tmp/`.
  - `retainRenderedPages` — bool toggle, default off.
  - `documentHelperMissingBehavior` — enum: `warn` (inline banner, disable mode)
    or `error` (block compare, surface dialog).

## Fixture plan

All fixtures under `tests/fixtures/document/`; project-created, no third-party
content. Provenance entries required in `docs/fixture-provenance.md`.

| Fixture | How |
|---|---|
| Text extraction success | Single-page PDF with known text |
| Text extraction failure | Corrupt PDF header |
| Multi-page PDF | Three pages, distinct text per page |
| SVG/PDF renderer failure | Intact xref, truncated object stream; `pdftotext` returns nonzero |
| OCR unavailable | `PATH=` override → `plugin-error: binary-not-found` |
| OCR timeout | 8000×6000 white PNG + `--timeout 1ms` → timeout error |
| OCR oversized output | Mock emitting > 64 MiB → `PluginError::OutputTooLarge` |
| OCR malformed output | Mock emitting non-UTF-8 bytes → `plugin-error: invalid-utf8` |
| Privacy temp-file cleanup | Assert temp dir absent after compare; verified via `TempDir` drop guard |

## Test plan

1. **Unit tests** (`crates/linsync-core/tests/document_*.rs`): `probe` and
   `unpack_text` roundtrip for each plugin script against all seven fixture categories.
2. **Integration tests**: `linsync-cli compare --mode document` against fixture
   pairs; assert exit codes and diff output shape.
3. **Privacy test**: assert plugin temp dir is absent after compare; assert no
   rendered PNGs when `retainRenderedPages` is false.
4. **Negative tests**: missing binary, corrupt file, timeout, oversized, malformed
   output — each surfaces a structured `plugin-error`, never a panic.
5. **Sandbox tests** (Phase 6): helpers cannot write outside assigned temp dir;
   no network syscalls succeed.

## Migration / rollout

Behind a `document-compare` feature flag in `linsync-core`; off by default until
Phase 6 sandbox passes. GUI sidebar section appears only when the flag is active.
AppStream metainfo gets a new `<feature>` tag after the flag is lifted. Flatpak
permissions delta: none. `docs/third-party-notices.md` gains a "Runtime helper
dependencies" section before any binary release activating this flag.

## Blocking dependencies

- **Phase 6 (sandbox):** required before the flag is lifted; helpers must not run
  without sandbox enforcement.
- **Phase 4 (plugin protocol):** `unpack_text` and plugin discovery must be live;
  Tasks 4.1 and 4.3 provide the scaffolding these plugins reuse.

## Open issues

- `libreoffice --headless` warm-up is 1–3 s on some systems; document as known
  latency or explore `--norestore` daemon mode.
- `pdftotext` column ordering on multi-column PDFs is approximate; surface a
  "column order may differ from reading order" notice in the GUI.
- Tesseract language packs vary by distribution; populate the language ComboBox
  from `tesseract --list-langs` at runtime, not a hardcoded list.
- If a Poppler Rust crate is ever added as a Cargo dep, `deny.toml` must gain
  `"GPL-2.0-or-later"`. The current shell-out design avoids this; record the
  caveat in `docs/licensing.md`.
