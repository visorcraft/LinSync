use super::*;

pub(crate) fn man_command(args: &[String]) -> Result<ExitCode, String> {
    let (output, paths) = split_output_flag(args)?;
    if !paths.is_empty() {
        return Err("usage: linsync-cli man [--output FILE]".to_owned());
    }

    let man_page = man_page();
    if let Some(output) = output {
        fs::write(output, man_page).map_err(|err| err.to_string())?;
    } else {
        print!("{man_page}");
    }

    Ok(ExitCode::SUCCESS)
}

pub(crate) fn man_page() -> String {
    format!(
        r#".TH LINSYNC-CLI 1
.SH NAME
linsync-cli \- command-line file and folder comparison tools
.SH SYNOPSIS
.B linsync-cli
.I COMMAND
.RI [ OPTIONS ]
.SH DESCRIPTION
.B linsync-cli
provides scriptable access to LinSync comparison primitives.
.SH COMMANDS
.TP
.B archive [--keep-temp] [--json] [--unpacker PLUGIN_ID] LEFT RIGHT
Compare two archive files by extracting them (via tar / unzip subprocesses) and running a folder compare on the extracted trees. Supported extensions: .zip, .jar, .war, .apk, .ipa, .tar, .tgz, .tar.gz, .tbz2, .tar.bz2, .txz, .tar.xz, .tzst, .tar.zst. --unpacker PLUGIN_ID instead routes both archives through an installed unpacker / folder_virtualizer plugin (its unpack_folder operation) and compares the resulting virtual trees by SHA-256/size — useful for formats the built-in extractor cannot read.
.TP
.B cache clear [--scope webcompare]
Clear LinSync cache directories. Currently the only supported scope is webcompare (the webpage compare HTTP fetch cache under $XDG_CACHE_HOME/linsync/webcompare).
.TP
.B compare [--profile NAME-OR-PATH] [--type auto|text|binary|hex|folder|table|image|document] [--json|--count|--quiet] [--ignore-case] [--ignore-whitespace] [--ignore-blank-lines] [--ignore-eol] [--ignore-line-regex REGEX] [--regex-rule-set NAME] [--prediffer PLUGIN_ID] [--prediffer-conflict-policy chain|first-wins|last-wins] [--substitute-regex REGEX REPLACEMENT] [--detect-moves] [--diff-algorithm lcs|patience|myers] [--inline-granularity char|word|grapheme] [--context LINES] [--show-only-changes] [--render side-by-side|unified|context|normal|html] [--syntax plain|auto|rust|json|html|markdown|shell|toml|yaml|c|cpp|python|javascript|typescript|go|java|css] [--find PATTERN] [--find-regex] [--find-case-sensitive] [--bookmark SIDE:LINE[:LABEL]] [--encoding auto|utf8|utf8-bom|utf16le|utf16be|lossy-utf8] [--image-mode exact|tolerance|perceptual] [--image-tolerance F] [--image-delta-e F] [--image-frames first|all] [--document-mode text|ocr_text|rendered] [--document-pages FIRST-LAST] [--ocr-language LANG] [--save-result FILE] LEFT RIGHT
Compare two files and exit with 0 for equal files or 1 for differences. The --type auto default routes Folder/Binary/Table/Text; --type image and --type document must be selected explicitly because auto-detection does not route to those engines. --profile seeds every per-mode option from a built-in id (default, strict-bytes, ignore-formatting, code-review, prose-review, folder-sync-preview, webpage-source-safe), a saved user profile id, or a path to a profile JSON file; explicit CLI flags override the profile values regardless of argument order. --prediffer PLUGIN_ID (repeatable; also settable per profile as text.prediffer_plugins) routes enabled, installed prediffer plugins to normalize each side before diffing. Multiple ids form an ordered chain — each stage normalizes the previous stage's output. Ids that are missing, the wrong class, or disabled are skipped with a note and the comparison proceeds without them. --save-result FILE (text/folder/table/binary/image/document compares) writes the full result as versioned JSON so "report --from-json FILE" can re-render it later without recomparing.
.TP
.B compare3 [--markers|--json] LEFT BASE RIGHT
Compare left and right against a base file and optionally print conflict markers or JSON.
.TP
.B conflict [--json] FILE
Inspect a Git-style conflict-marker file and report conflict sections.
.TP
.B filter <validate RULE | validate-file PATH | list | migrate INPUT [--out OUTPUT | --in-place]>
Manage named filters and validate filter expressions. `validate` checks a single filter rule grammar; `validate-file` checks a filter file; `list` reports stored named filters; `migrate` converts a legacy .flt file to the LinSync filter grammar, writing to --out, in-place with --in-place, or stdout by default.
.TP
.B folders [--recursive] [--profile NAME-OR-PATH] [--method METHOD] [--timestamp-tolerance-ms MS] [--symlinks target|follow|special] [--large-file-threshold-bytes BYTES] [--large-file-method quick|binary] [--hash-algorithm blake3|sha256|crc32] [--compare-permissions] [--compare-ownership] [--compare-xattrs] [--dry-run] [--exclude-generated] [--filter RULE] [--filter-name NAME] [--case-insensitive-filter] [--hide-skipped] [--state STATE] [--types LIST] [--search SUBSTR] [--sort KEY] [--desc] [--group-by GROUP] [--offset N] [--limit N] [--json|--csv|--count|--quiet] LEFT RIGHT
Compare two folders and summarize identical, different, left-only, and right-only entries. --profile seeds folder options from a compare profile, and --json includes the effective profile, filters, and folder options used for the run. The result view is driven by the core query API: --state filters by comparison state, --types restricts to a comma-separated set of entry types (file,dir,symlink,special), --search keeps entries whose relative path contains a case-insensitive substring, --sort (name|path|state|type|size|modified) with --desc orders the rows, --group-by (none|state|type|directory) buckets them, and --offset/--limit paginate. JSON and text output report filtered (total matches), returned, offset, and has_more.
.TP
.B hex [--width BYTES] [--metadata-only] [--json|--count|--quiet] LEFT RIGHT
Compare two binary files and print differing hex rows or metadata-only differences.
.TP
.B launch [--wait] [--] [ARGS...]
Launch the LinSync GUI and pass through any remaining arguments.
.TP
.B open-external [--wait] [--preset PRESET] PATH...
Open files or folders through the configured external viewer, xdg-open, or a named Linux editor preset.
.TP
.B patch LEFT RIGHT [--format unified|context|normal] [--context LINES] [--preview|--output FILE]
Generate or preview a unified, context, or normal diff from two text files or text-only folder changes.
.TP
.B plugin <list [--json] | inspect ID [--json] | validate ID | enable ID | disable ID | trust ID | untrust ID | set-option ID KEY VALUE | clear-option ID KEY | install PATH | remove ID | run-diagnostic ID [--input FILE] [--timeout-ms MS] [--json]>
Manage discovered plugins. list shows installed plugins with enabled state; inspect shows a plugin's manifest, option schema, and current values; validate checks the persisted options against the manifest schema; enable/disable toggle a plugin; trust/untrust record whether a discovered plugin is authorized to run (the GUI prompts for trust before a plugin's first run and will not enable an untrusted plugin; explicitly invoking a plugin from the CLI is itself consent); set-option validates a value against the schema before persisting it; clear-option removes a stored option; install copies a plugin directory from PATH into $XDG_DATA_HOME/linsync/plugins after validating its manifest (refusing an id that is already installed); remove deletes a user-installed plugin and its persisted options/enabled flag (system plugin directories are never touched); run-diagnostic probes a plugin's helper with an optional sample --input and reports exit/timeout/stdout/stderr, the parsed protocol response, and the active sandbox confinement (exit 0 healthy, 1 unhealthy, 2 transport error). Enabled state lives in $XDG_CONFIG_HOME/linsync/plugins.json, trust flags in plugins-trusted.json, and option values under $XDG_CONFIG_HOME/linsync/plugin-options/.
.TP
.B profile <list | show ID | validate (ID|PATH) | import PATH | export ID [--output PATH] | delete ID>
Manage compare profiles — named bundles of per-mode comparison options. Built-in profiles ship with the binary; user profiles live under $XDG_CONFIG_HOME/linsync/profiles/. Use --profile NAME-OR-PATH on a compare command to source options from a profile; CLI flags override profile values.
.TP
.B report LEFT RIGHT --output FILE [--context LINES] [--columns COLS] [--tree-state expanded|collapsed] [--nested-file-reports] [--relative-paths] [--from-json FILE]
Generate an HTML file or folder comparison report with optional text context, folder columns, tree state, or nested file reports. --relative-paths labels the compared paths relative to the current directory (when they live under it) so the report carries no absolute, machine-specific paths. --from-json FILE re-renders a text/folder/table/binary/image/document result previously saved with "compare --save-result" instead of comparing afresh (requires --output).
.TP
.B project <validate PATH | show PATH [--json] | run PATH [--json] | report PATH --output DIR | list [DIR] [--json]>
Operate on a project file (a named bundle of saved comparisons). validate loads and schema-checks it; show lists its comparisons; run executes each one (auto-detecting folder / text / binary / table like the compare command, with default options) and exits 0 when all are equal, 1 when some differ, or 2 on error, for CI use; report writes an HTML report per comparison (text or folder) into DIR with the same exit codes; list shows the *.linsync-project files in DIR (default the current directory) with their name and comparison count.
.TP
.B reveal [--wait] PATH...
Reveal files or folders through org.freedesktop.FileManager1.ShowItems, falling back to xdg-open for the containing folder.
.TP
.B session <save LEFT RIGHT [--base BASE] [--title T] [--view MODE] [--profile ID] | list [--json] | show [INDEX] [--json] | clear>
Manage the recent-session history shared with the GUI ($XDG_DATA_HOME/linsync/recent-sessions.json). save records a compare (left/right, optional base, title, view mode, and a compare profile id) at the front of the history; list shows the saved sessions newest-first with their index; show prints one (INDEX defaults to 0, the most recent); clear empties the history. Sessions can be reopened explicitly from the GUI Sessions page.
.TP
.B table [--header] [--delimiter CHAR|--tsv] [--table-quote CHAR] [--table-escape CHAR] [--table-comment PREFIX] [--table-skip-blank BOOL] [--numeric-tolerance FLOAT] [--json|--count|--quiet] LEFT RIGHT
Compare delimited table files.
.TP
.B webpage --sub-mode html|text|tree|rendered|screenshot --accept-network-fetch [--depth N] [--timeout SECS] [--max-requests N] LEFT_URL RIGHT_URL
Compare two URLs. --accept-network-fetch is mandatory because outbound HTTP requests are made. Sub-modes html, text, and tree are fully implemented; rendered and screenshot require the web-engine Cargo feature and currently return NotImplemented.
.TP
.B completions SHELL
Generate shell completions for bash, zsh, or fish.
.TP
.B man [--output FILE]
Generate this manual page.
.TP
.B mergetool --base BASE --local LOCAL --remote REMOTE --merged MERGED [--auto-resolve left|right|base] [--json]
Invoke linsync-cli as a Git mergetool. With --auto-resolve, all conflicts are resolved to
the chosen side and the result is written to the MERGED file. With --json, a
machine-readable merge summary is printed. Without --auto-resolve, the GUI is launched for
interactive resolution (opening the Merge workspace on the three inputs); after it exits the
command checks the MERGED file and exits 0 only when a fully-resolved, conflict-marker-free
result was written, 1 if conflict markers remain, or 2 if no output was written.
.SH EXIT STATUS
.TP
.B 0
No differences were found or the command generated output successfully.
.TP
.B 1
Differences were found.
.TP
.B 2
An error occurred.
.SH VERSION
{version}
"#,
        version = env!("CARGO_PKG_VERSION")
    )
}

pub(crate) fn print_help() {
    println!(
        "\
linsync-cli {}

USAGE:
    linsync-cli archive [--keep-temp] [--json] [--unpacker PLUGIN_ID] LEFT RIGHT
    linsync-cli cache clear [--scope webcompare]
    linsync-cli compare [--profile NAME-OR-PATH] [--type auto|text|binary|hex|folder|table|image|document] [--json|--count|--quiet] [--ignore-case] [--ignore-whitespace] [--ignore-blank-lines] [--ignore-eol] [--ignore-line-regex REGEX] [--regex-rule-set NAME] [--prediffer PLUGIN_ID] [--prediffer-conflict-policy chain|first-wins|last-wins] [--substitute-regex REGEX REPLACEMENT] [--detect-moves] [--diff-algorithm lcs|patience|myers] [--inline-granularity char|word|grapheme] [--context LINES] [--show-only-changes] [--render side-by-side|unified|context|normal|html] [--syntax plain|auto|rust|json|html|markdown|shell|toml|yaml|c|cpp|python|javascript|typescript|go|java|css] [--find PATTERN] [--find-regex] [--find-case-sensitive] [--bookmark SIDE:LINE[:LABEL]] [--encoding auto|utf8|utf8-bom|utf16le|utf16be|lossy-utf8] [--image-mode exact|tolerance|perceptual] [--image-tolerance F] [--image-delta-e F] [--image-frames first|all] [--document-mode text|ocr_text|rendered] [--document-pages FIRST-LAST] [--ocr-language LANG] [--save-result FILE] LEFT RIGHT
    linsync-cli compare3 [--markers|--json] LEFT BASE RIGHT
    linsync-cli conflict [--json] FILE
    linsync-cli completions SHELL
    linsync-cli filter <validate RULE | validate-file PATH | list | migrate INPUT [--out OUTPUT | --in-place]>
    linsync-cli folders [--recursive] [--profile NAME-OR-PATH] [--method METHOD] [--timestamp-tolerance-ms MS] [--symlinks target|follow|special] [--large-file-threshold-bytes BYTES] [--large-file-method quick|binary] [--hash-algorithm blake3|sha256|crc32] [--compare-permissions] [--compare-ownership] [--compare-xattrs] [--dry-run] [--exclude-generated] [--filter RULE] [--filter-name NAME] [--case-insensitive-filter] [--hide-skipped] [--state STATE] [--types LIST] [--search SUBSTR] [--sort KEY] [--desc] [--group-by GROUP] [--offset N] [--limit N] [--json|--csv|--count|--quiet] LEFT RIGHT
    linsync-cli hex [--width BYTES] [--metadata-only] [--json|--count|--quiet] LEFT RIGHT
    linsync-cli launch [--wait] [--] [ARGS...]
    linsync-cli man [--output FILE]
    linsync-cli mergetool --base BASE --local LOCAL --remote REMOTE --merged MERGED [--auto-resolve left|right|base] [--json]
    linsync-cli open-external [--wait] [--preset PRESET] PATH...
    linsync-cli patch LEFT RIGHT [--format unified|context|normal] [--context LINES] [--preview|--output FILE]
    linsync-cli plugin <list [--json] | inspect ID [--json] | validate ID | enable ID | disable ID | trust ID | untrust ID | set-option ID KEY VALUE | clear-option ID KEY | install PATH | remove ID | run-diagnostic ID [--input FILE] [--timeout-ms MS] [--json]>
    linsync-cli profile <list | show ID | validate (ID|PATH) | import PATH | export ID [--output PATH] | delete ID>
    linsync-cli project <validate PATH | show PATH [--json] | run PATH [--json] | report PATH --output DIR | list [DIR] [--json]>
    linsync-cli reveal [--wait] PATH...
    linsync-cli report LEFT RIGHT --output FILE [--context LINES] [--columns COLS] [--tree-state expanded|collapsed] [--nested-file-reports] [--relative-paths] [--from-json FILE]
    linsync-cli session <save LEFT RIGHT [--base BASE] [--title T] [--view MODE] [--profile ID] | list [--json] | show [INDEX] [--json] | clear>
    linsync-cli table [--header] [--delimiter CHAR|--tsv] [--table-quote CHAR] [--table-escape CHAR] [--table-comment PREFIX] [--table-skip-blank BOOL] [--numeric-tolerance FLOAT] [--json|--count|--quiet] LEFT RIGHT
    linsync-cli webpage --sub-mode html|text|tree|rendered|screenshot --accept-network-fetch [--depth N] [--timeout SECS] [--max-requests N] LEFT_URL RIGHT_URL

mergetool:
    Run linsync-cli as a Git mergetool. Requires --base, --local, --remote, and --merged.
    With --auto-resolve <left|right|base>, all conflicts are resolved automatically and
    the result is written to --merged. Add --json to print a machine-readable merge
    summary. Without --auto-resolve, the GUI opens for interactive resolution and the
    command exits 0 only once a conflict-marker-free result is written to --merged
    (1 if markers remain, 2 if nothing was written).

plugin:
    Manage discovered plugins. `list [--json]` shows installed plugins and their
    enabled state; `inspect ID [--json]` prints the manifest, option schema, and
    current values; `validate ID` checks the persisted options against the
    schema; `enable`/`disable ID` toggle a plugin; `trust`/`untrust ID` record
    whether a discovered plugin is authorized to run; `set-option ID KEY VALUE`
    validates the value against the schema before persisting it (VALUE is parsed
    as JSON, falling back to a string); `clear-option ID KEY` removes it;
    `install PATH` copies a plugin directory into $XDG_DATA_HOME/linsync/plugins
    after validating its manifest (an already-installed id is refused); `remove
    ID` deletes a user-installed plugin and its stored options/enabled flag
    (system plugin directories are never touched);
    `run-diagnostic ID [--input FILE] [--timeout-ms MS] [--json]` probes the
    helper and reports exit/timeout/stdout/stderr, the parsed response, and the
    active sandbox confinement (exit 0 healthy, 1 unhealthy, 2 transport error).

profile:
    Manage compare profiles — named bundles of per-mode options stored under
    $XDG_CONFIG_HOME/linsync/profiles/. Use --profile NAME-OR-PATH on a compare
    command to seed every option from the profile; CLI flags override profile
    values. Subcommands: list (built-ins + user profiles), show ID, validate
    (ID|PATH), import PATH, export ID [--output PATH], delete ID. Built-ins
    cannot be deleted or overwritten — copy them to a new id to customise.

webpage:
    Compare two URLs. --accept-network-fetch is mandatory because outbound HTTP requests
    are made. Sub-modes html/text/tree are fully implemented; rendered and screenshot
    require the web-engine Cargo feature and currently return NotImplemented.

compare image / document types:
    --type image uses the pure-Rust image comparison engine. Mode is one of exact,
    tolerance (per-channel threshold), or perceptual (CIEDE2000). The --image-tolerance
    and --image-delta-e flags tune each respective mode.
    --type document routes through helper plugins (Tesseract OCR, Poppler, LibreOffice).
    Mode is one of text, ocr_text, or rendered. --ocr-language sets the Tesseract language code. --document-pages FIRST-LAST restricts rendered mode to a page range.

Exit codes:
    0  no differences
    1  differences found
    2  error",
        env!("CARGO_PKG_VERSION")
    );
}
