# Document And OCR Compare Decision

> Status: historical decision record. The licensing, sandbox, privacy, and
> fixture prerequisites it sets out are all met and document/PDF/image-rendered
> and OCR compare have shipped — see `docs/document-compare-implementation.md`
> for the implemented design.

Document, PDF, SVG, image-as-rendered, and OCR compare were post-MVP specialized
workflows. They were gated until helper licensing, sandbox behavior, privacy
controls, and fixtures were in place.

## Compare Paths

The shipped paths:

- Document-as-text: use unpacker plugins/helpers to extract text from office
  documents, PDFs, or other structured formats, then compare through the text
  engine.
- Rendered document/image compare: render PDF/SVG/document pages to images and
  compare through the image compare path.
- OCR-as-text: run an OCR helper against image/PDF inputs and compare extracted
  text through the text engine.
- OCR with positions: per-word position data (image-pixel bounding boxes) is
  surfaced so the UI can correlate text differences back to source pages.

## Licensing Boundary

No OCR engine, PDF renderer, office-document parser, or SVG/PDF renderer is part
of the default dependency set. Before adding one, record:

- Exact license and source distribution obligations.
- Whether the helper is bundled, system-discovered, or plugin-provided.
- Third-party notices and source-offer changes.
- Flatpak permissions and sandbox limitations.
- Security review for untrusted document parsing.

## Privacy Boundary

OCR and document helpers can expose sensitive document contents. They must be
local by default. Network OCR services are not permitted in the default build.
If an optional remote OCR integration is ever considered, it must be opt-in,
disabled by default, and documented with clear privacy warnings.

Required user-visible controls before enabling OCR:

- Language/model selection.
- Temporary-file location and cleanup behavior.
- Whether images/pages are retained for debugging.
- Explicit error handling for unsupported formats or missing helper binaries.

## Test Requirements

Before enabling document/OCR compare, add fixtures or controlled generated
inputs for:

- Text extraction success and failure.
- Multi-page PDF/rendered image selection.
- SVG/PDF renderer failures.
- OCR unavailable, timeout, oversized output, and malformed output.
- Privacy-sensitive temp-file cleanup.
