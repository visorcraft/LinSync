# Licensing

LinSync is licensed as GPL-3.0-only. The repository root `LICENSE` file contains
the GNU General Public License version 3 text.

## Project Policy

- GPL-3.0-compatible permissive licenses are allowed by default.
- Copyleft dependencies are allowed only when their obligations are tracked.
- GPL-2.0-only, AGPL, SSPL, Commons Clause, non-commercial, research-only,
  personal-use-only, unknown-license, and no-license inputs are blocked.
- Third-party application source, assets, translations, bundled filters, and plugins are not
  copied into LinSync unless a later file-specific review proves compatibility
  with GPL-3.0-only.

The default source build must remain redistributable under GPL-3.0-only. Any
non-free SDK, proprietary service, or unclear binary helper may only be
documented as an optional integration and must not be required to build, test,
package, or run the open-source application.

## Dependency Classes

Permissive code licenses such as MIT, BSD-2-Clause, BSD-3-Clause, ISC, 0BSD,
Zlib, libpng, BSL-1.0, Unicode-DFS-2016, Unicode-3.0, and Apache-2.0 are
allowed through `deny.toml`.

Copyleft licenses that can be GPL-3.0-compatible, including GPL-3.0-only,
GPL-3.0-or-later, LGPL-2.1-or-later, LGPL-3.0-only, LGPL-3.0-or-later, and
MPL-2.0, require a dependency note before they are introduced. The note must
identify the package, linkage/distribution mode, source-offer obligations, and
whether the license has incompatible secondary-license terms.

Licenses that require case-by-case review before use include GPL-2.0-or-later,
GPL-only Qt/KDE modules, EPL-2.0, CDDL-1.0, CDDL-1.1, OFL-1.1, Creative Commons
asset licenses, helper binaries, OCR engines, PDF renderers, archive tools, and
codec/media libraries.

Blocked inputs include GPL-2.0-only, AGPL family licenses, SSPL, Commons Clause,
BUSL before its change date, PolyForm licenses, non-commercial licenses,
research-only licenses, personal-use-only licenses, unknown licenses, and
no-license code.

## Qt And KDE

Qt and KDE components should prefer LGPL-compatible modules distributed in the
normal system/Flatpak runtime model. GPL-only Qt/KDE modules require explicit
review before use because they can change distribution obligations. KConfig may
be considered later if it wins over the current JSON/XDG storage direction.

Qt WebEngine is not part of the default application shell. If webpage compare
uses it later, it must stay feature-gated until licensing, security, sandbox,
and binary-distribution obligations are reviewed.

## Optional Helpers

PCRE2 is deferred unless Rust `regex`/`fancy-regex` cannot cover required
legacy-compatible filters. If introduced, it needs a feature gate and license
entry.

Archive support through 7z, libarchive, or other helper processes must document
the exact helper license, whether the helper is bundled or discovered from the
system, and how source/offers/notices are shipped. The current implementation
decision is in `docs/archive-support.md`: start with system helper processes,
keep the interface generic enough for a later libarchive-backed helper, and do
not bundle helpers before review.

OCR engines, Poppler/PDF renderers, SVG/PDF/image renderers, codecs, and media
helpers need explicit license and security review before becoming build or
runtime dependencies. The shipped document/OCR compare paths use
system-discovered helper processes (no new Cargo dependency); the design and
helper review requirements are recorded in
`docs/document-compare-implementation.md`.

## Assets And Examples

Fonts, icons, screenshots, sample filters, sample plugins, fixture data, and
Creative Commons assets need file-level provenance. Prefer project-created
assets or assets with clear GPL-3.0-compatible terms. Do not copy third-party
sample filters, icons, translations, plugins, or screenshots into LinSync unless
a file-specific review proves the exact material is GPL-3.0-compatible.
Current test fixture provenance is tracked in `docs/fixture-provenance.md`.

If legacy-compatible data files are ever imported, they must live in an
isolated compatibility directory with attribution, license notes, and tests that
prove LinSync does not depend on them for the default build.

## Syntax definitions

The planned syntax-highlighting dependency is the `syntect` crate (current
stable 5.3.0, MIT). `deny.toml` covers the crate itself and its notable
transitive paths: `onig`/`onig_sys` (MIT bindings bundling the Oniguruma C
library, BSD-2-Clause) and `fancy-regex` (MIT) are all on the allow-list.

`deny.toml` only gates Cargo crate licenses, so the syntax definitions bundled
inside syntect's default `SyntaxSet` need this separate record. They derive
from sublimehq/Packages, whose `LICENSE` is a short custom permissive grant:
"Permission to copy, use, modify, sell and distribute this software is
granted…" with a warranty disclaimer (HPND-style, no attribution or copyleft
obligations). This is GPL-3.0-compatible, so shipping the bundled definitions
inside LinSync is acceptable. The repository's license carves out files that
carry their own license text; the defaults syntect bundles fall under the
blanket grant.

Recorded fallback if that provenance were ever judged unacceptable: build
syntect with its default-syntaxes feature disabled and load definitions from
the `two-face` crate (MIT OR Apache-2.0) instead.

## Release Materials

The current source tree ships `docs/third-party-notices.md` with the Cargo
dependency notices, source-offer notes, and copyleft dependency tracking status.
Before each release, regenerate or review that file and verify the final package
contents, not only Cargo manifests.
