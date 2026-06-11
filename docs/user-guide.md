# User Guide

This guide describes the workflows LinSync currently documents for
users (release 1.11.0). The CLI ships the full subcommand surface; the
Rust core covers text, folder, binary, table, image, document,
webpage, merge, filter, plugin, storage, paths, trash, sandbox, and
logging behavior; and the GUI exposes a nine-section QML / Kirigami
shell (Compare, Image Compare, Webpage Compare, Document Compare,
Sessions, Filters, Plugins, Settings, About — plus Credits and
Licenses reached from the About page).

For the things LinSync intentionally does *not* do yet, see
[`docs/known-limitations-1.0.md`](known-limitations-1.0.md).

## Quick Start

Build and test the workspace:

```sh
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Run the CLI:

```sh
cargo run -p linsync-cli -- compare left.txt right.txt
```

CLI exit codes:

- `0`: no differences
- `1`: differences found
- `2`: command or runtime error

## GUI

Launch the GUI:

```sh
cargo run -p linsync
cargo run -p linsync -- left.txt right.txt
```

The shell uses a `StackLayout` for nine sidebar entries plus the
Credits and Licenses pages reached from About. The Merge workspace is
opened from the Compare toolbar rather than the sidebar.

- **Compare** — text and folder pairs with browse buttons, difference
  navigation, find, and read-only summary cards. The page is backed by
  either the local JSON bridge or the feature-gated `cxx-qt` host. A
  toolbar entry opens the three-way Merge workspace.
- **Image Compare** — dedicated workspace for image diff (exact,
  tolerance, perceptual modes).
- **Webpage Compare** — dedicated workspace for source HTML, extracted
  text, and resource-tree modes.
- **Document Compare** — dedicated workspace for PDF / DOCX / ODT
  comparison via plugin extractors.
- **Sessions** — open tabs and recent paths surfaced from the active
  bridge. Switch and close tabs from the list; copy recent paths to the
  clipboard.
- **Filters** — include/exclude glob editors with chip removal,
  quick-add presets (`.git/**`, `target/**`, `node_modules/**`,
  `*.lock`, `*.tmp`), `.gitignore` toggle, follow-symlinks toggle,
  maximum-depth spin box, named filter save/load/delete, and a legacy
  `.flt` migrator. Filters persist via `/filters/*` bridge endpoints
  and apply to folder compares run from the same tab.
- **Plugins** — populated from `discover_plugins()` via `/plugins/list`.
  Per-plugin enable/disable persists via `/plugins/toggle`. The XDG
  discovery paths (`$XDG_DATA_HOME/linsync/plugins/<id>/`,
  `/usr/share/linsync/plugins/<id>/`,
  `/usr/local/share/linsync/plugins/<id>/`) are documented inline. A
  per-plugin options dialog drives `/plugins/options/{get,set}`.
- **Settings** — four `Kirigami.FormLayout` cards covering appearance
  (color schemes, pane font, tab width, line numbers, whitespace, word
  wrap), comparison behavior (default mode, ignore case / whitespace /
  blank lines / EOL, EOL on save), session (restore last, confirm on
  close, persist recent paths, max recent), and storage (open config
  folder, reset to defaults). Settings persist to
  `$XDG_CONFIG_HOME/linsync/settings.json`; storage shape is described
  in `docs/settings-storage-decision.md`.
- **About** — version, license, and platform pills, a 2×2 feature
  grid, a LinSync information card, and a "Licenses & Credits" card
  that deep-links into Credits and Licenses.
- **Credits** — searchable table of every Cargo crate from
  `docs/third-party-notices.md` with license-family tinting and
  per-row `crates.io` navigation, plus a runtime-component list (Qt 6,
  KDE Frameworks 6, FreeDesktop). Reached from About → Credits.
- **Licenses** — tabbed full-text reader: a *LinSync License* tab with
  the GPL v3 text (and a "Dialog" button to pop it out), a
  *Third-party* tab bundling the crate manifest plus the MIT,
  Apache-2.0, BSD-2-Clause, Unlicense, and Unicode-3.0 license texts,
  and an *Acknowledgements* tab. Each tab supports line-number
  filtering, a wrap toggle, copy, and clear. Reached from About →
  Licenses.

Pre-existing bridge-URL parsing warnings from
`qml6 -- --linsync-bridge <url>` are harmless and unrelated to the
sidebar pages.

The Compare toolbar's Stop button cancels an in-flight compare from the
GUI: it flips the bridge-side cancel flag via `/cancel?id=X`, and the
compare reports "Compare cancelled" while preserving any partial result.

## File Compare

Compare two text files:

```sh
cargo run -p linsync-cli -- compare left.txt right.txt
```

Useful output modes:

```sh
cargo run -p linsync-cli -- compare --json left.txt right.txt
cargo run -p linsync-cli -- compare --count left.txt right.txt
cargo run -p linsync-cli -- compare --quiet left.txt right.txt
```

Text ignore options:

```sh
cargo run -p linsync-cli -- compare --ignore-case --ignore-whitespace left.txt right.txt
cargo run -p linsync-cli -- compare --ignore-blank-lines --ignore-line-regex '^Generated:' left.txt right.txt
cargo run -p linsync-cli -- compare --substitute-regex 'id=\d+' 'id=<id>' left.txt right.txt
```

Explicit compare type overrides:

```sh
cargo run -p linsync-cli -- compare --type auto left.csv right.csv
cargo run -p linsync-cli -- compare --type text left.csv right.csv
cargo run -p linsync-cli -- compare --type binary left.bin right.bin
cargo run -p linsync-cli -- compare --type folder left-dir right-dir
cargo run -p linsync-cli -- compare --type table left.csv right.csv
```

`--type auto` is the default. It routes directories to folder compare,
likely-binary files to binary compare, and `.csv`/`.tsv` files to table
compare.

Syntax highlighting:

```sh
cargo run -p linsync-cli -- compare --render html --syntax auto left.rs right.rs
cargo run -p linsync-cli -- compare --render html --syntax python left.py right.py
```

`--syntax` accepts `plain` (default), `auto`, `rust`, `json`, `html`,
`markdown`, `shell`, `toml`, `yaml`, `c`, `cpp`, `python`, `javascript`,
`typescript`, `go`, `java`, and `css`. `auto` picks the mode from the
file extension (`.rs`, `.json`, `.html`/`.htm`/`.xml`, `.md`,
`.sh`/`.bash`/`.zsh`/`.fish`, `.toml`, `.yaml`/`.yml`, `.c`/`.h`,
`.cc`/`.cpp`/`.cxx`/`.hpp`/`.hh`, `.py`, `.js`/`.mjs`/`.jsx`,
`.ts`/`.tsx`, `.go`, `.java`, `.css`) and falls back to plain for
anything else. Highlighting is line-at-a-time — multi-line constructs
such as block comments render per line — and lines over 20,000 bytes
are skipped. The GUI Compare page exposes the same modes through its
Syntax selector.

## Folder Compare

Compare two folders:

```sh
cargo run -p linsync-cli -- folders left-dir right-dir
```

Machine-readable output:

```sh
cargo run -p linsync-cli -- folders --json left-dir right-dir
cargo run -p linsync-cli -- folders --csv left-dir right-dir
```

Compare method examples:

```sh
cargo run -p linsync-cli -- folders --method binary left-dir right-dir
cargo run -p linsync-cli -- folders --method size left-dir right-dir
cargo run -p linsync-cli -- folders --method hash-blake3 left-dir right-dir
cargo run -p linsync-cli -- folders --method normalized-text left-dir right-dir
cargo run -p linsync-cli -- folders --method date-size --timestamp-tolerance-ms 2000 left-dir right-dir
cargo run -p linsync-cli -- folders --method full --large-file-threshold-bytes 10485760 --large-file-method binary left-dir right-dir
cargo run -p linsync-cli -- folders --symlinks target left-dir right-dir
cargo run -p linsync-cli -- folders --symlinks follow left-dir right-dir
cargo run -p linsync-cli -- folders --symlinks special left-dir right-dir
```

Filter examples:

```sh
cargo run -p linsync-cli -- folders --exclude-generated left-dir right-dir
cargo run -p linsync-cli -- folders --filter 'f!:generated' --state skipped left-dir right-dir
cargo run -p linsync-cli -- folders --filter-name Generated --state skipped left-dir right-dir
cargo run -p linsync-cli -- folders --filter 'd!:target' --hide-skipped left-dir right-dir
```

The folder compare engine supports core entry states, skipped filter
rows, error rows, content/metadata methods, symlink target/follow/
special policies, recursive symlink loop detection, special-file
guards, and large-file method downgrades with per-entry notes in
JSON/CSV output. Rows expose name, extension, type, per-side size,
per-side modified time, compare result, and error/status metadata. The
engine exposes progress events and a cancellable entry point that
preserves partial results and marks unvisited rows as aborted. The
folder operation planner can stage copy, delete, rename,
create-missing-folder, and refresh operations with overwrite,
permission, conflict, and invalid-selection warnings. Delete uses the
FreeDesktop Trash when available and can produce restore guidance for
trashed items while making permanent-delete results visibly
non-restorable.

> **Note:** Folder operation re-comparisons currently invoke
> `FolderCompareOptions::default()` in the HTTP bridge, ignoring active
> filters / walk options. CLI runs honour all options. See
> `docs/known-limitations-1.0.md`.

## Image, Document, and Webpage Compare

LinSync 1.1 added three specialized compare engines.

### Image

```sh
cargo run -p linsync-cli -- compare --type image left.png right.png
cargo run -p linsync-cli -- compare --type image --image-mode tolerance --image-tolerance 0.02 left.png right.png
cargo run -p linsync-cli -- compare --type image --image-mode perceptual --image-delta-e 2.0 left.png right.png
cargo run -p linsync-cli -- compare --type image --image-frames all left.gif right.gif
cargo run -p linsync-cli -- compare --type image --json left.png right.png
```

`--type image` is required to invoke image compare from the CLI;
`--type auto` (the default) routes image files to `binary` instead.
The engine offers three modes: exact, tolerance (per-channel
threshold), and perceptual (CIEDE2000). Reports pixel deltas and a
bounding box of the diff region. Dimension mismatches are padded to a
common transparent canvas and reported as unequal. The GUI Image
Compare page surfaces the same modes, loads the supported image format
list from the running build, renders a red diff overlay with region
navigation, and can save the generated overlay PNG to a user-selected
path.

Supported formats now include GIF, Radiance HDR (`.hdr`), and OpenEXR
(`.exr`) alongside PNG, JPEG, WebP, and TIFF. HDR/EXR inputs are
tone-mapped to 8-bit RGBA before comparison, so they are suitable for
release-artifact checks rather than HDR-mastering fidelity. For animated
GIF / APNG / WebP, add `--image-frames all` (default `first`) to compare
every frame pairwise; the result reports the total frame count and which
frames differ. Animation timing and disposal modes are not modelled —
frames are compared by index.

### Document

```sh
cargo run -p linsync-cli -- compare --type document left.pdf right.pdf
cargo run -p linsync-cli -- compare --type document --document-mode ocr_text --ocr-language eng left.pdf right.pdf
cargo run -p linsync-cli -- compare --type document --document-mode rendered --document-pages 2-4 left.pdf right.pdf
```

Routes through helper plugins (Tesseract, Poppler, LibreOffice). Text,
OCR-text, and rendered (per-page image diff, optionally narrowed with
`--document-pages FIRST-LAST`) modes are all functional. In OCR mode the
engine also asks the OCR engine for per-word bounding boxes (when the
plugin supports it) and surfaces them as positional data on the result
(`left_word_positions` / `right_word_positions`); these are exposed as
data and word counts, not as a visual overlay drawn on a rendered page.

### Webpage

```sh
cargo run -p linsync-cli -- webpage --sub-mode html --accept-network-fetch https://example.com/a https://example.com/b
cargo run -p linsync-cli -- webpage --sub-mode text --accept-network-fetch https://example.com/a https://example.com/b
cargo run -p linsync-cli -- webpage --sub-mode tree --depth 2 --accept-network-fetch https://example.com/a https://example.com/b
```

`webpage` requires the explicit `--accept-network-fetch` flag because
it performs outbound HTTP requests. Source HTML, extracted visible
text, and resource-tree sub-modes work end-to-end. Rendered DOM diff
and screenshot diff are produced by an out-of-process Qt WebEngine
renderer; they require a build with the `web-engine` feature enabled.

## Merge

Compare three files against a base:

```sh
cargo run -p linsync-cli -- compare3 left.txt base.txt right.txt
```

Emit conflict markers:

```sh
cargo run -p linsync-cli -- compare3 --markers left.txt base.txt right.txt
cargo run -p linsync-cli -- compare3 --json left.txt base.txt right.txt
```

Inspect an already-conflicted Git worktree file:

```sh
cargo run -p linsync-cli -- conflict src/file-with-conflicts.txt
cargo run -p linsync-cli -- conflict --json src/file-with-conflicts.txt
```

The GUI Merge page presents a base/left/right/result layout with
choose-side controls and per-conflict navigation. Conflict
highlight/scroll uses each side's own line ranges, so next/previous both
cycles through conflicts and scrolls each pane to the matching lines.

## Filters

The filter grammar supports wildcard rules, Rust `regex` rules
(`f:`, `f!:`, `d:`, `d!:`), file-expression rules (`fe:`, `fe!:`,
`de:`, `de!:`, `e:`, `e!:`), and diagnostics for unsupported
Windows-specific prefixes. File expressions cover `type == text|binary`,
byte-size comparisons (e.g. `size >= 10KB`), and Unix epoch millisecond
modified-time comparisons (e.g. `modified_ms >= 0`). The Filters page
includes a legacy `.flt` migrator with a preview of converted rules.

Named filters persist under `$XDG_CONFIG_HOME/linsync/filters.json` and
are referenced from the CLI with `--filter-name <name>`.

## Patch And Report Export

Create a diff patch:

```sh
cargo run -p linsync-cli -- patch left.txt right.txt --format unified
cargo run -p linsync-cli -- patch left.txt right.txt --format context --context 5
cargo run -p linsync-cli -- patch left.txt right.txt --format normal
cargo run -p linsync-cli -- patch left-dir right-dir --format unified
cargo run -p linsync-cli -- patch left.txt right.txt --preview
cargo run -p linsync-cli -- patch left.txt right.txt --output changes.patch
```

Write an HTML report:

```sh
cargo run -p linsync-cli -- report left.txt right.txt --output report.html
cargo run -p linsync-cli -- report left-dir right-dir --output folder-report.html
cargo run -p linsync-cli -- report left-dir right-dir --output folder-report.html --columns path,state --tree-state collapsed --nested-file-reports
```

Patch export targets text file output and folder-level patch sets where
changed members are representable UTF-8 text. Use `--preview` to print
the patch without writing a file. HTML reports support text and folder
comparisons, selected folder columns, expanded/collapsed tree state,
and optional nested text file reports.

You can also save a comparison result and re-render it later without
recomparing. `compare … --save-result FILE` writes a small JSON envelope,
and `report --from-json FILE --output report.html` re-renders it to HTML:

```sh
cargo run -p linsync-cli -- compare --type image --save-result result.json left.png right.png
cargo run -p linsync-cli -- report --from-json result.json --output report.html
```

The round-trip works for text, folder, table, binary, image, and
document results.

## Specialized Compare Commands

Binary/hex compare:

```sh
cargo run -p linsync-cli -- hex --width 16 left.bin right.bin
cargo run -p linsync-cli -- hex --json left.bin right.bin
cargo run -p linsync-cli -- hex --metadata-only --json left.bin right.bin
```

CSV/table compare:

```sh
cargo run -p linsync-cli -- table --header left.csv right.csv
cargo run -p linsync-cli -- table --json left.csv right.csv
```

Archive-as-folder compare:

```sh
cargo run -p linsync-cli -- archive left.zip right.zip
cargo run -p linsync-cli -- archive left.tar.zst right.tar.zst
```

The `archive` command can route each archive through a folder-virtualizer
/unpacker plugin (`--unpacker PLUGIN_ID`), which handles nested-archive
recursion and member extraction into a virtual folder before the folder
compare. Without a plugin it falls back to extracting via `unzip`/`tar`
subprocesses for the built-in archive formats.

**Editing zip archive members (GUI only):** when comparing two zip archives,
right-click any member row and choose "Edit member in left archive" or
"Edit member in right archive". The member is extracted to a staging file
and opened in your external editor. Save the file, then click **Commit** in
LinSync to repack the archive atomically, or **Discard** to cancel. The
original archive is preserved as a `.bak` during the commit; by default it
is removed on success, but enabling **Keep a .bak of the archive after
repack** (Settings → Comparison behavior) retains it as a one-edit undo. A
commit that fails never deletes your staged edit — retry or discard from the
banner. Tar and 7z archives remain read-only.

Self-compare:

```sh
cargo run -p linsync-cli -- self-compare file.txt
cargo run -p linsync-cli -- self-compare --json file.txt
```

Temporary self-compare copies are created below
`$XDG_CACHE_HOME/linsync/comparisons` and cleaned up on exit.

GUI handoff from scripts:

```sh
cargo run -p linsync-cli -- launch -- left.txt right.txt
cargo run -p linsync-cli -- launch --wait -- left-dir right-dir
```

External viewer fallback for unsupported files:

```sh
cargo run -p linsync-cli -- open-external unsupported.custom
cargo run -p linsync-cli -- open-external --wait unsupported.custom
cargo run -p linsync-cli -- open-external --preset kate source.rs
cargo run -p linsync-cli -- open-external --preset nvim-terminal notes.txt
cargo run -p linsync-cli -- reveal path/to/file.txt
cargo run -p linsync-cli -- reveal --wait path/to/file.txt
```

`open-external` uses `xdg-open` by default. Set `LINSYNC_OPEN` to use a
specific viewer command from scripts, or pass `--preset` for `kate`,
`kwrite`, `vscode`, `vscodium`, `gnome-text-editor`, `sublime`,
`nvim-terminal`, `xdg-open`, or JetBrains launcher presets such as
`jetbrains-idea` and `jetbrains-pycharm`. `nvim-terminal` uses
`x-terminal-emulator` by default; set `LINSYNC_TERMINAL` to choose
another terminal wrapper.

`reveal` first asks `org.freedesktop.FileManager1.ShowItems` to reveal
the selected path. If that desktop DBus API is unavailable, it opens
the containing folder with `xdg-open`. Set `LINSYNC_REVEAL` to use a
file-manager-specific reveal command from scripts.

When `open-external` or `reveal` runs the external command
synchronously (`--wait`), a non-zero exit code from the helper is
propagated as exit code `2` ("error"), not `1` ("differences"), so
wrapper scripts can distinguish a tool failure from a diff result.

## Plugins

LinSync does not support Windows-only in-process or scriptlet plugins.
Linux plugins are external helper processes using JSON over
stdin/stdout. The plugin host runs helpers under the
`linsync-sandbox` policy (Landlock + seccompiler with a bubblewrap
fallback).

Plugin classes today (`PluginClass`, serialized as snake_case):

- `unpacker`
- `prediffer`
- `editor_complement`
- `external_viewer`
- `folder_virtualizer`
- `document_text_extractor`
- `ocr_engine`
- `pdf_renderer`

Plugins that fetch web resources (e.g. the bundled `web-fetch` plugin
under `packaging/plugins/`) are not a distinct class — they are
helpers invoked by core engines under the same `linsync-sandbox`
policy.

The Plugins page lists discovered plugins with class chips, license
expression, source badge, and an enable/disable toggle. Per-plugin
options are edited through a dialog wired to
`/plugins/options/{get,set}`.

In addition to the global enable/disable toggle, a plugin can be enabled
or disabled **per compare profile**: select a user profile and set the
per-profile override, which is stored on the profile and takes precedence
over the global setting. (Built-in profiles are read-only — copy one to a
user profile first.)

When more than one prediffer runs in a chain, two prediffers can
normalize the same thing. A prediffer's manifest can declare
`normalization_categories`; when two prediffers share a category, the
`--prediffer-conflict-policy` flag on `compare` (or the profile's
equivalent setting) decides what happens:

```sh
cargo run -p linsync-cli -- compare --prediffer-conflict-policy first-wins left.txt right.txt
```

`chain` (the default) runs every prediffer; `first-wins` keeps the first
of two overlapping prediffers and drops the later one(s); `last-wins`
keeps the last. Prediffers that declare no categories never conflict.

See `docs/plugin-protocol.md` for the helper protocol.

## Git Integration

Generate shell completion or a man page:

```sh
cargo run -p linsync-cli -- completions bash
cargo run -p linsync-cli -- man --output linsync-cli.1
```

Use LinSync as a Git difftool or mergetool through the CLI commands
documented in `docs/git-integration.md`. The difftool path is usable
for text comparisons. The mergetool path supports `--auto-resolve` and
can launch the GUI Merge workspace interactively for hands-on conflict
resolution, validating the saved output before returning.

## Troubleshooting

If a command exits with `1`, differences were found. This is expected
and should not be treated as a runtime failure.

If a command exits with `2`, check stderr for command usage, missing
files, invalid output-mode combinations, or unsupported options.

If plugin execution fails, inspect the helper stderr and verify:

- the manifest entry is relative to the plugin directory
- the helper is executable
- the helper writes one JSON response to stdout
- diagnostics go to stderr, not stdout
- output stays within configured size limits

If trashing is unavailable (or disabled in Settings), folder-operation
deletes become permanent. The plan dialog then shows a warning that the
selected entries will be deleted permanently and cannot be recovered,
and the Apply button stays disabled until the "Permanently delete —
this cannot be undone" checkbox is ticked. The checkbox resets every
time the dialog opens. The bridge enforces this server-side too:
`/folder/op/execute` returns 409 for a permanent delete unless the
request carries `confirm_permanent=1`.

If Flatpak packaging is used later, filesystem access, external
editors, and helper/plugin execution may require portals or extra
permissions.

For current project status, see `docs/feature-matrix.md` and
`docs/known-limitations-1.0.md`.
