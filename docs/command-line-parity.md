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
