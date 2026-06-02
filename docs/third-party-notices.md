# Third-Party Notices

Last regenerated: 2026-06-02

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
compatibility review. Where a crate offers `Zlib` alongside `MIT`/`Apache-2.0`,
the `MIT` or `Apache-2.0` option is used; `foldhash` (Zlib-only) is used under
the Zlib license, whose full text is bundled in the in-app Licenses page.

| Package | Version | License expression |
| --- | --- | --- |
| `adler2` | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| `aho-corasick` | 1.1.4 | Unlicense OR MIT |
| `anyhow` | 1.0.102 | MIT OR Apache-2.0 |
| `arrayref` | 0.3.9 | BSD-2-Clause |
| `arrayvec` | 0.7.6 | MIT OR Apache-2.0 |
| `autocfg` | 1.5.1 | Apache-2.0 OR MIT |
| `bit_field` | 0.10.3 | Apache-2.0/MIT |
| `bitflags` | 2.11.1 | MIT OR Apache-2.0 |
| `blake3` | 1.8.5 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception |
| `block-buffer` | 0.10.4 | MIT OR Apache-2.0 |
| `bytemuck` | 1.25.0 | Zlib OR Apache-2.0 OR MIT |
| `byteorder-lite` | 0.1.0 | Unlicense OR MIT |
| `cc` | 1.2.62 | MIT OR Apache-2.0 |
| `cfg-if` | 1.0.4 | MIT OR Apache-2.0 |
| `clang-format` | 0.3.0 | MIT OR Apache-2.0 |
| `codespan-reporting` | 0.11.1 | Apache-2.0 |
| `codespan-reporting` | 0.13.1 | Apache-2.0 |
| `color_quant` | 1.1.0 | MIT |
| `constant_time_eq` | 0.4.2 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| `convert_case` | 0.6.0 | MIT |
| `cpufeatures` | 0.2.17 | MIT OR Apache-2.0 |
| `cpufeatures` | 0.3.0 | MIT OR Apache-2.0 |
| `crc32fast` | 1.5.0 | MIT OR Apache-2.0 |
| `crypto-common` | 0.1.7 | MIT OR Apache-2.0 |
| `cxx` | 1.0.194 | MIT OR Apache-2.0 |
| `cxxbridge-flags` | 1.0.194 | MIT OR Apache-2.0 |
| `cxxbridge-macro` | 1.0.194 | MIT OR Apache-2.0 |
| `cxx-gen` | 0.7.194 | MIT OR Apache-2.0 |
| `cxx-qt` | 0.8.1 | MIT OR Apache-2.0 |
| `cxx-qt-build` | 0.8.1 | MIT OR Apache-2.0 |
| `cxx-qt-gen` | 0.8.1 | MIT OR Apache-2.0 |
| `cxx-qt-lib` | 0.8.1 | MIT OR Apache-2.0 |
| `cxx-qt-macro` | 0.8.1 | MIT OR Apache-2.0 |
| `digest` | 0.10.7 | MIT OR Apache-2.0 |
| `enumflags2` | 0.7.12 | MIT OR Apache-2.0 |
| `enumflags2_derive` | 0.7.12 | MIT OR Apache-2.0 |
| `equivalent` | 1.0.2 | Apache-2.0 OR MIT |
| `exr` | 1.74.0 | BSD-3-Clause |
| `fax` | 0.2.7 | MIT |
| `fdeflate` | 0.3.7 | MIT OR Apache-2.0 |
| `find-msvc-tools` | 0.1.9 | MIT OR Apache-2.0 |
| `flate2` | 1.1.9 | MIT OR Apache-2.0 |
| `foldhash` | 0.2.0 | Zlib |
| `generic-array` | 0.14.7 | MIT |
| `gif` | 0.14.2 | MIT OR Apache-2.0 |
| `half` | 2.7.1 | MIT OR Apache-2.0 |
| `hashbrown` | 0.17.1 | MIT OR Apache-2.0 |
| `image` | 0.25.10 | MIT OR Apache-2.0 |
| `image-webp` | 0.2.4 | MIT OR Apache-2.0 |
| `indexmap` | 2.14.0 | Apache-2.0 OR MIT |
| `indoc` | 2.0.7 | MIT OR Apache-2.0 |
| `itoa` | 1.0.18 | MIT OR Apache-2.0 |
| `jobserver` | 0.1.34 | MIT OR Apache-2.0 |
| `lab` | 0.11.0 | MIT |
| `landlock` | 0.4.5 | MIT OR Apache-2.0 |
| `lazy_static` | 1.5.0 | MIT OR Apache-2.0 |
| `lebe` | 0.5.3 | BSD-3-Clause |
| `libc` | 0.2.186 | MIT OR Apache-2.0 |
| `link-cplusplus` | 1.0.12 | MIT OR Apache-2.0 |
| `log` | 0.4.29 | MIT OR Apache-2.0 |
| `memchr` | 2.8.0 | Unlicense OR MIT |
| `miniz_oxide` | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| `moxcms` | 0.8.1 | BSD-3-Clause OR Apache-2.0 |
| `nu-ansi-term` | 0.50.3 | MIT |
| `num-traits` | 0.2.19 | MIT OR Apache-2.0 |
| `once_cell` | 1.21.4 | MIT OR Apache-2.0 |
| `pin-project-lite` | 0.2.17 | Apache-2.0 OR MIT |
| `png` | 0.18.1 | MIT OR Apache-2.0 |
| `proc-macro2` | 1.0.106 | MIT OR Apache-2.0 |
| `pxfm` | 0.1.29 | BSD-3-Clause OR Apache-2.0 |
| `qt-build-utils` | 0.8.1 | MIT OR Apache-2.0 |
| `quick-error` | 2.0.1 | MIT/Apache-2.0 |
| `quote` | 1.0.45 | MIT OR Apache-2.0 |
| `regex` | 1.12.3 | MIT OR Apache-2.0 |
| `regex-automata` | 0.4.14 | MIT OR Apache-2.0 |
| `regex-syntax` | 0.8.10 | MIT OR Apache-2.0 |
| `rustversion` | 1.0.22 | MIT OR Apache-2.0 |
| `seccompiler` | 0.4.0 | Apache-2.0 OR BSD-3-Clause |
| `semver` | 1.0.28 | MIT OR Apache-2.0 |
| `serde` | 1.0.228 | MIT OR Apache-2.0 |
| `serde_core` | 1.0.228 | MIT OR Apache-2.0 |
| `serde_derive` | 1.0.228 | MIT OR Apache-2.0 |
| `serde_json` | 1.0.149 | MIT OR Apache-2.0 |
| `serde_repr` | 0.1.20 | MIT OR Apache-2.0 |
| `sha2` | 0.10.9 | MIT OR Apache-2.0 |
| `sharded-slab` | 0.1.7 | MIT |
| `shlex` | 1.3.0 | MIT OR Apache-2.0 |
| `simd-adler32` | 0.3.9 | MIT |
| `smallvec` | 1.15.1 | MIT OR Apache-2.0 |
| `static_assertions` | 1.1.0 | MIT OR Apache-2.0 |
| `syn` | 2.0.117 | MIT OR Apache-2.0 |
| `termcolor` | 1.4.1 | Unlicense OR MIT |
| `thiserror` | 1.0.69 | MIT OR Apache-2.0 |
| `thiserror` | 2.0.18 | MIT OR Apache-2.0 |
| `thiserror-impl` | 1.0.69 | MIT OR Apache-2.0 |
| `thiserror-impl` | 2.0.18 | MIT OR Apache-2.0 |
| `thread_local` | 1.1.9 | MIT OR Apache-2.0 |
| `tiff` | 0.11.3 | MIT |
| `tracing` | 0.1.44 | MIT |
| `tracing-attributes` | 0.1.31 | MIT |
| `tracing-core` | 0.1.36 | MIT |
| `tracing-log` | 0.2.0 | MIT |
| `tracing-serde` | 0.2.0 | MIT |
| `tracing-subscriber` | 0.3.23 | MIT |
| `typenum` | 1.20.1 | MIT OR Apache-2.0 |
| `unicode-ident` | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| `unicode-segmentation` | 1.13.2 | MIT OR Apache-2.0 |
| `unicode-width` | 0.1.14 | MIT OR Apache-2.0 |
| `unicode-width` | 0.2.2 | MIT OR Apache-2.0 |
| `urlencoding` | 2.1.3 | MIT |
| `version_check` | 0.9.5 | MIT/Apache-2.0 |
| `weezl` | 0.1.12 | MIT OR Apache-2.0 |
| `zerocopy` | 0.8.48 | BSD-2-Clause OR Apache-2.0 OR MIT |
| `zerocopy-derive` | 0.8.48 | BSD-2-Clause OR Apache-2.0 OR MIT |
| `zmij` | 1.0.21 | MIT |
| `zune-core` | 0.5.1 | MIT OR Apache-2.0 OR Zlib |
| `zune-inflate` | 0.2.54 | MIT OR Apache-2.0 OR Zlib |
| `zune-jpeg` | 0.5.15 | MIT OR Apache-2.0 OR Zlib |

The table above is generated by `just credits`: `cargo tree` over the shipped
feature set (`cxxqt`, `cxxqt-app`, `web-engine`) on the Linux target, including
build dependencies and excluding dev-only crates — i.e. every third-party crate
distributed in the released binaries. It is verified during pre-release with the
`just deny` (cargo-deny) and `just audit` (cargo-audit) workflows. Re-run
`just credits` after any dependency change and update the in-app Credits
(`CreditsPage.qml`) and Licenses (`LicensesPage.qml`) pages to match.

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
