# Command-Line Coverage Matrix

This matrix maps LinSync's command-line surface to release coverage. The CLI
exit-code contract is:

- `0`: no differences
- `1`: differences found
- `2`: command or runtime error

| Command | Purpose | Status |
| --- | --- | --- |
| `compare` | Text/table/binary auto-detected file comparison with JSON/count/quiet/report options | Supported |
| `compare3` | Three-way text comparison and conflict-marker output | Supported |
| `folders` | Folder comparison with recursion, filters, methods, state filtering, JSON/CSV/count/quiet output | Supported |
| `hex` | Binary/hex comparison with width and metadata-only options | Supported |
| `table` | CSV/TSV comparison with header-aware changed-cell summaries | Supported |
| `patch` | Unified/context/normal patch output and preview/write modes | Supported |
| `report` | HTML file/folder report generation | Supported |
| `merge` / `conflict` | Conflict-marker parsing and merge helpers | Partial |
| `launch` | Start the GUI and optionally wait for completion | Supported |
| `open-external` | Open unsupported paths with configured or preset external tools | Supported |
| `reveal` | Reveal paths in a file manager or containing folder | Supported |
| `self-compare` | Compare a file against a temporary copy | Supported |
| `completions` | Generate shell completions | Supported |
| `man` | Generate the roff man page | Supported |

Helper commands that wait on an external process map non-zero helper exits to
exit code `2`, preserving `1` for comparison differences only.

## Selected `compare` / `report` flags

These flags extend the comparison commands beyond the per-command summaries above:

| Flag | Command | Values | Purpose |
| --- | --- | --- | --- |
| `--prediffer-conflict-policy` | `compare` | `chain` (default) \| `first-wins` \| `last-wins` | Resolve prediffers whose `normalization_categories` overlap (`chain` runs all; `first-wins`/`last-wins` drop the later/earlier overlapping prediffer). |
| `--image-frames` | `compare --type image` | `first` (default) \| `all` | Compare only the first frame of an animated image, or every frame pairwise (reports `frame_count` + per-frame breakdown). |
| `--save-result FILE` | `compare` | path | Write a `{schema_version: 1, kind, result}` envelope for later re-rendering. Now supports `kind` `image` and `document` in addition to `text`, `folder`, `table`, and `binary`. |
| `--from-json FILE` | `report` | path | Re-render a previously saved result to HTML without recomparing. Now accepts `kind` `image` (`ImageCompareResult::to_html_report`) and `document` (`DocumentCompareResult::to_html_report`) alongside the existing kinds. Requires `--output FILE`. |

## New HTTP bridge endpoint

| Endpoint | Purpose |
| --- | --- |
| `GET /profiles/active/plugin-enabled?id=<plugin>&enabled=true\|false` | Set a per-profile enable/disable override for a plugin on the active user profile (persists `CompareProfile.plugin_enablement`). Returns `409 Conflict` when the active profile is a built-in or when no profile is selected. |
