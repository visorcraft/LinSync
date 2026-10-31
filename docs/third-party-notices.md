# Third-Party Notices

Last regenerated: 2026-10-31

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
<!-- BEGIN GENERATED CREDITS (do not edit inside; run `just credits-update`) -->
| `adler2` | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| `aho-corasick` | 1.1.5 | Unlicense OR MIT |
| `anyhow` | 1.0.104 | MIT OR Apache-2.0 |
| `arrayvec` | 0.7.8 | MIT OR Apache-2.0 |
| `autocfg` | 1.5.1 | Apache-2.0 OR MIT |
| `bincode` | 1.3.3 | MIT |
| `bit_field` | 0.10.3 | Apache-2.0/MIT |
| `bitflags` | 2.13.2 | MIT OR Apache-2.0 |
| `blake3` | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception |
| `block-buffer` | 0.12.1 | MIT OR Apache-2.0 |
| `bytemuck` | 1.25.2 | Zlib OR Apache-2.0 OR MIT |
| `byteorder-lite` | 0.1.0 | Unlicense OR MIT |
| `cc` | 1.10.0 | MIT OR Apache-2.0 |
| `cfg-if` | 1.0.5 | MIT OR Apache-2.0 |
| `clang-format` | 0.3.0 | MIT OR Apache-2.0 |
| `codespan-reporting` | 0.11.1 | Apache-2.0 |
| `codespan-reporting` | 0.13.1 | Apache-2.0 |
| `color_quant` | 1.1.0 | MIT |
| `constant_time_eq` | 0.4.2 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| `const-oid` | 0.10.2 | Apache-2.0 OR MIT |
| `convert_case` | 0.6.0 | MIT |
| `cpufeatures` | 0.3.1 | MIT OR Apache-2.0 |
| `crc32fast` | 1.5.3 | MIT OR Apache-2.0 |
| `crypto-common` | 0.2.2 | MIT OR Apache-2.0 |
| `cxx` | 1.0.205 | MIT OR Apache-2.0 |
| `cxxbridge-flags` | 1.0.205 | MIT OR Apache-2.0 |
| `cxxbridge-macro` | 1.0.205 | MIT OR Apache-2.0 |
| `cxx-gen` | 0.7.205 | MIT OR Apache-2.0 |
| `cxx-qt` | 0.10.0 | MIT OR Apache-2.0 |
| `cxx-qt-build` | 0.10.0 | MIT OR Apache-2.0 |
| `cxx-qt-gen` | 0.10.0 | MIT OR Apache-2.0 |
| `cxx-qt-lib` | 0.10.0 | MIT OR Apache-2.0 |
| `cxx-qt-macro` | 0.10.0 | MIT OR Apache-2.0 |
| `digest` | 0.11.3 | MIT OR Apache-2.0 |
| `enumflags2` | 0.7.12 | MIT OR Apache-2.0 |
| `enumflags2_derive` | 0.7.12 | MIT OR Apache-2.0 |
| `equivalent` | 1.0.3 | Apache-2.0 OR MIT |
| `exr` | 1.74.2 | BSD-3-Clause |
| `fastrand` | 2.5.0 | Apache-2.0 OR MIT |
| `fax` | 0.2.7 | MIT |
| `fdeflate` | 0.3.7 | MIT OR Apache-2.0 |
| `find-msvc-tools` | 0.1.15 | MIT OR Apache-2.0 |
| `flate2` | 1.1.10 | MIT OR Apache-2.0 |
| `fnv` | 1.0.7 | Apache-2.0 / MIT |
| `foldhash` | 0.2.0 | Zlib |
| `getrandom` | 0.4.3 | MIT OR Apache-2.0 |
| `gif` | 0.14.2 | MIT OR Apache-2.0 |
| `half` | 2.7.1 | MIT OR Apache-2.0 |
| `hashbrown` | 0.17.1 | MIT OR Apache-2.0 |
| `hybrid-array` | 0.4.15 | MIT OR Apache-2.0 |
| `image` | 0.25.10 | MIT OR Apache-2.0 |
| `image-webp` | 0.2.4 | MIT OR Apache-2.0 |
| `indexmap` | 2.14.2 | Apache-2.0 OR MIT |
| `indoc` | 2.0.8 | MIT OR Apache-2.0 |
| `itoa` | 1.0.18 | MIT OR Apache-2.0 |
| `jobserver` | 0.1.35 | MIT OR Apache-2.0 |
| `lab` | 0.11.0 | MIT |
| `landlock` | 0.4.7 | MIT OR Apache-2.0 |
| `lazy_static` | 1.5.1 | MIT OR Apache-2.0 |
| `lebe` | 0.5.3 | BSD-3-Clause |
| `libc` | 0.2.191 | MIT OR Apache-2.0 |
| `libm` | 0.2.16 | MIT |
| `link-cplusplus` | 1.0.12 | MIT OR Apache-2.0 |
| `linux-raw-sys` | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| `log` | 0.4.35 | MIT OR Apache-2.0 |
| `memchr` | 2.8.3 | Unlicense OR MIT |
| `miniz_oxide` | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| `miniz_oxide` | 0.9.1 | MIT OR Zlib OR Apache-2.0 |
| `moxcms` | 0.8.1 | BSD-3-Clause OR Apache-2.0 |
| `nu-ansi-term` | 0.50.3 | MIT |
| `num-complex` | 0.4.6 | MIT OR Apache-2.0 |
| `num-traits` | 0.2.19 | MIT OR Apache-2.0 |
| `once_cell` | 1.21.4 | MIT OR Apache-2.0 |
| `onig` | 6.5.3 | MIT |
| `onig_sys` | 69.9.3 | MIT |
| `paste` | 1.0.15 | MIT OR Apache-2.0 |
| `pin-project-lite` | 0.2.17 | Apache-2.0 OR MIT |
| `pkg-config` | 0.3.34 | MIT OR Apache-2.0 |
| `png` | 0.18.1 | MIT OR Apache-2.0 |
| `proc-macro2` | 1.0.107 | MIT OR Apache-2.0 |
| `pulp` | 0.22.3 | MIT |
| `pulp-wasm-simd-flag` | 0.1.1 | MIT |
| `pxfm` | 0.1.30 | BSD-3-Clause OR Apache-2.0 |
| `qt-build-utils` | 0.10.0 | MIT OR Apache-2.0 |
| `quick-error` | 2.0.1 | MIT/Apache-2.0 |
| `quote` | 1.0.47 | MIT OR Apache-2.0 |
| `raw-cpuid` | 11.6.0 | MIT |
| `reborrow` | 0.5.5 | MIT |
| `regex` | 1.13.1 | MIT OR Apache-2.0 |
| `regex-automata` | 0.4.18 | MIT OR Apache-2.0 |
| `regex-syntax` | 0.8.11 | MIT OR Apache-2.0 |
| `rustix` | 1.1.6 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| `rustversion` | 1.0.23 | MIT OR Apache-2.0 |
| `same-file` | 1.0.6 | Unlicense/MIT |
| `seccompiler` | 0.5.0 | Apache-2.0 OR BSD-3-Clause |
| `semver` | 1.0.28 | MIT OR Apache-2.0 |
| `serde` | 1.0.229 | MIT OR Apache-2.0 |
| `serde_core` | 1.0.229 | MIT OR Apache-2.0 |
| `serde_derive` | 1.0.229 | MIT OR Apache-2.0 |
| `serde_json` | 1.0.154 | MIT OR Apache-2.0 |
| `serde_repr` | 0.1.21 | MIT OR Apache-2.0 |
| `sha2` | 0.11.1 | MIT OR Apache-2.0 |
| `sharded-slab` | 0.1.7 | MIT |
| `shlex` | 2.0.1 | MIT OR Apache-2.0 |
| `simd-adler32` | 0.3.10 | MIT |
| `smallvec` | 1.16.4 | MIT OR Apache-2.0 |
| `static_assertions` | 1.1.0 | MIT OR Apache-2.0 |
| `syn` | 2.0.119 | MIT OR Apache-2.0 |
| `syn` | 3.0.9 | MIT OR Apache-2.0 |
| `syntect` | 5.3.0 | MIT |
| `tempfile` | 3.27.0 | MIT OR Apache-2.0 |
| `termcolor` | 1.4.1 | Unlicense OR MIT |
| `thiserror` | 1.0.69 | MIT OR Apache-2.0 |
| `thiserror` | 2.0.21 | MIT OR Apache-2.0 |
| `thiserror-impl` | 1.0.69 | MIT OR Apache-2.0 |
| `thiserror-impl` | 2.0.21 | MIT OR Apache-2.0 |
| `thread_local` | 1.1.10 | MIT OR Apache-2.0 |
| `tiff` | 0.11.3 | MIT |
| `tracing` | 0.1.44 | MIT |
| `tracing-attributes` | 0.1.31 | MIT |
| `tracing-core` | 0.1.36 | MIT |
| `tracing-log` | 0.2.0 | MIT |
| `tracing-serde` | 0.2.0 | MIT |
| `tracing-subscriber` | 0.3.23 | MIT |
| `typenum` | 1.20.1 | MIT OR Apache-2.0 |
| `unicode-ident` | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| `unicode-segmentation` | 1.13.3 | MIT OR Apache-2.0 |
| `unicode-width` | 0.1.14 | MIT OR Apache-2.0 |
| `unicode-width` | 0.2.3 | MIT OR Apache-2.0 |
| `urlencoding` | 2.1.3 | MIT |
| `version_check` | 0.9.5 | MIT/Apache-2.0 |
| `walkdir` | 2.5.0 | Unlicense/MIT |
| `weezl` | 0.1.12 | MIT OR Apache-2.0 |
| `which` | 8.0.6 | MIT |
| `zerocopy` | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| `zerocopy-derive` | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| `zmij` | 1.0.23 | MIT |
| `zune-core` | 0.5.3 | MIT OR Apache-2.0 OR Zlib |
| `zune-inflate` | 0.2.54 | MIT OR Apache-2.0 OR Zlib |
| `zune-jpeg` | 0.5.15 | MIT OR Apache-2.0 OR Zlib |
<!-- END GENERATED CREDITS -->
The table above is generated by `just credits`: `cargo tree` over the shipped
feature set (`cxxqt`, `cxxqt-app`, `web-engine`) on the Linux target, including
build dependencies and excluding dev-only crates — i.e. every third-party crate
distributed in the released binaries. It is verified during pre-release with the
`just deny` (cargo-deny) and `just audit` (cargo-audit) workflows. Re-run
`just credits` after any dependency change and update the in-app Credits
(`CreditsPage.qml`) and Licenses (`LicensesPage.qml`) pages to match.

## Bundled native code and data

Some release binaries also contain third-party code and data that are not
separate Cargo crates:

| Library / data set | Version / source | License | Notes |
| --- | --- | --- | --- |
| Oniguruma regular expression engine | bundled by `onig_sys` 69.9.3 | BSD-2-Clause | Statically linked through `syntect` → `onig` → `onig_sys`. The upstream license text is in `oniguruma/COPYING`. |
| Sublime Text default syntax definitions | bundled by `syntect` 5.3.0 | custom permissive (HPND-style) | Shipped as a `SyntaxSet` dump; derived from `sublimehq/Packages`. No attribution list is required by the upstream grant. |

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
