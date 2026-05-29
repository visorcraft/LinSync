# Third-Party Notices

Last regenerated: 2026-05-26

This notice file covers the current LinSync source tree and Cargo dependency
set. It must be regenerated or reviewed before every public binary release.

LinSync itself is licensed as GPL-3.0-only. The repository root `LICENSE` file
contains the GNU General Public License version 3 text.

## Source Offer

For source releases, the complete corresponding source is the LinSync repository
contents, including `Cargo.lock`, packaging metadata, scripts, documentation,
tests, and generated metadata committed to the release tag.

For binary releases, distributors must provide the corresponding source for the
GPL-3.0-only LinSync binaries and preserve this notice file. Cargo dependencies
are fetched from crates.io sources recorded in `Cargo.lock`; if a distributor
vendors or patches any dependency, the vendored source and license text must be
included with the release materials.

## Cargo Dependencies

The current third-party Cargo dependency set is permissively licensed. Where a
crate offers `MIT OR Apache-2.0`, either license may be used. Where a crate
offers `Unlicense OR MIT`, LinSync uses the MIT option for GPL-3.0-only
compatibility review. Where a crate offers CC0/MIT-0/LLVM-exception alternatives
alongside Apache-2.0, LinSync uses the Apache-2.0 option for GPL-3.0-only
compatibility review.

| Package | Version | License expression |
| --- | --- | --- |
| `aho-corasick` | 1.1.4 | Unlicense OR MIT |
| `arrayref` | 0.3.9 | BSD-2-Clause |
| `arrayvec` | 0.7.6 | MIT OR Apache-2.0 |
| `blake3` | 1.8.5 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception |
| `cc` | 1.2.62 | MIT OR Apache-2.0 |
| `cfg-if` | 1.0.4 | MIT OR Apache-2.0 |
| `constant_time_eq` | 0.4.2 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| `cpufeatures` | 0.3.0 | MIT OR Apache-2.0 |
| `find-msvc-tools` | 0.1.9 | MIT OR Apache-2.0 |
| `itoa` | 1.0.18 | MIT OR Apache-2.0 |
| `lazy_static` | 1.5.0 | MIT OR Apache-2.0 |
| `libc` | 0.2.186 | MIT OR Apache-2.0 |
| `log` | 0.4.29 | MIT OR Apache-2.0 |
| `memchr` | 2.8.0 | Unlicense OR MIT |
| `nu-ansi-term` | 0.50.3 | MIT |
| `once_cell` | 1.21.4 | MIT OR Apache-2.0 |
| `pin-project-lite` | 0.2.17 | Apache-2.0 OR MIT |
| `proc-macro2` | 1.0.106 | MIT OR Apache-2.0 |
| `quote` | 1.0.45 | MIT OR Apache-2.0 |
| `regex` | 1.12.3 | MIT OR Apache-2.0 |
| `regex-automata` | 0.4.14 | MIT OR Apache-2.0 |
| `regex-syntax` | 0.8.10 | MIT OR Apache-2.0 |
| `serde` | 1.0.228 | MIT OR Apache-2.0 |
| `serde_core` | 1.0.228 | MIT OR Apache-2.0 |
| `serde_derive` | 1.0.228 | MIT OR Apache-2.0 |
| `serde_json` | 1.0.149 | MIT OR Apache-2.0 |
| `serde_repr` | 0.1.20 | MIT OR Apache-2.0 |
| `sharded-slab` | 0.1.7 | MIT |
| `shlex` | 1.3.0 | MIT OR Apache-2.0 |
| `smallvec` | 1.15.1 | MIT OR Apache-2.0 |
| `syn` | 2.0.117 | MIT OR Apache-2.0 |
| `thread_local` | 1.1.9 | MIT OR Apache-2.0 |
| `tracing` | 0.1.44 | MIT |
| `tracing-attributes` | 0.1.31 | MIT |
| `tracing-core` | 0.1.36 | MIT |
| `tracing-log` | 0.2.0 | MIT |
| `tracing-serde` | 0.2.0 | MIT |
| `tracing-subscriber` | 0.3.23 | MIT |
| `unicode-ident` | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| `zmij` | 1.0.21 | MIT |

The table above reflects `cargo tree --workspace` against `Cargo.lock` and is
verified during pre-release with the `just deny` (cargo-deny) and `just audit`
(cargo-audit) workflows. Entries that no longer appear in `Cargo.lock`
(e.g. removed `valuable`, `windows-link`, `windows-sys`) have been dropped from
this list.

## Copyleft Dependency Tracking

No third-party copyleft Cargo dependencies are present in the current dependency
tree. If GPL-compatible copyleft dependencies are added later, their linkage
model, distribution obligations, source-offer requirements, and any
secondary-license constraints must be recorded here before release.

## Optional Runtime Helpers (Document Compare, OCR)

The document-compare feature shells out to external system binaries at runtime.
These binaries are **not** linked into the LinSync binary; they are discovered on
the user's PATH and invoked as child processes. LinSync does not distribute,
bundle, or modify them. Installation is the user's responsibility.

| Binary | Typical package | License | Notes |
| --- | --- | --- | --- |
| `pdftotext` | `poppler-utils` (Debian/Ubuntu), `poppler` (Arch/CachyOS) | GPL-2.0-or-later | Part of the Poppler project |
| `tesseract` | `tesseract-ocr` (Debian/Ubuntu), `tesseract` (Arch/CachyOS) | Apache-2.0 | Tesseract OCR engine by Google |
| `libreoffice` | `libreoffice` | MPL-2.0 (core) | Used headless for ODT/DOCX/RTF text extraction |

When these binaries are absent, the corresponding LinSync plugin emits a
structured `binary-not-found` error and the compare operation fails gracefully
without panicking.

Distributors who bundle or package these binaries alongside LinSync must satisfy
their respective license requirements independently of LinSync's GPL-3.0-only
terms.
