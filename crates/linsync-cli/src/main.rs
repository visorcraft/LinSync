use std::env;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use linsync_core::{
    AppPaths, BinaryCompareOptions, CompareMethod, CompareProfile, DiffAlgorithm, DiffBlockKind,
    DiffLineKind, FileFilter, FilterMatchOptions, FilterParseErrorKind, FilterStore,
    FolderCompareOptions, FolderCompareResult, FolderEntryDiff, FolderEntryFilter,
    FolderEntryState, FolderGrouping, FolderQuery, FolderSortKey, FolderTypeFilter, HashAlgorithm,
    InlineGranularity, MergeChoice, MoveDirection, PluginExecutionOptions, PluginInputDescriptor,
    ProfileId, ProfileStore, ProfileStoreError, SymlinkPolicy, TableCellState, TableCompareOptions,
    TextBookmark, TextCompareOptions, TextDocument, TextFindOptions, TextInputEncoding,
    TextRenderMode, TextSubstitution, TextSyntaxMode, ThreeWayConflict, ThreeWayMergeState,
    assess_operation_risks, builtin_profiles, builtin_text_regex_rule_sets, clear_plugin_option,
    compare_binary_files, compare_folders, compare_table_files, compare_text, compare_text_files,
    discover_installed_plugins, find_builtin, is_likely_binary, load_plugin_enabled_map,
    load_plugin_options, merge_three_way, parse_conflict_markers, plan_folder_operation,
    probe_plugin, set_plugin_enabled, set_plugin_option,
};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Vec<String>) -> Result<ExitCode, String> {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(ExitCode::SUCCESS);
    };

    match command {
        "archive" => archive_command(&args[1..]),
        "compare" => compare_command(&args[1..]),
        "compare3" => compare3_command(&args[1..]),
        "conflict" => conflict_command(&args[1..]),
        "completions" => completions_command(&args[1..]),
        "filter" => filter_command(&args[1..]),
        "folders" => folders_command(&args[1..]),
        "hex" => hex_command(&args[1..]),
        "launch" => launch_command(&args[1..]),
        "man" | "manpage" => man_command(&args[1..]),
        "mergetool" => mergetool_command(&args[1..]),
        "open-external" => open_external_command(&args[1..]),
        "patch" => patch_command(&args[1..]),
        "profile" => profile_command(&args[1..]),
        "plugin" | "plugins" => plugin_command(&args[1..]),
        "reveal" => reveal_command(&args[1..]),
        "report" => report_command(&args[1..]),
        "self-compare" => self_compare_command(&args[1..]),
        "table" => table_command(&args[1..]),
        "cache" => cache_command(&args[1..]),
        "webpage" => webpage_command(&args[1..]),
        "help" | "-h" | "--help" => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
        "version" | "-V" | "--version" => {
            println!("linsync-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("unknown command '{other}'")),
    }
}

const CLI_COMMANDS: &[&str] = &[
    "archive",
    "cache",
    "compare",
    "compare3",
    "conflict",
    "completions",
    "filter",
    "folders",
    "hex",
    "launch",
    "man",
    "mergetool",
    "open-external",
    "patch",
    "plugin",
    "profile",
    "reveal",
    "report",
    "self-compare",
    "table",
    "webpage",
    "help",
    "version",
];

const MERGETOOL_FLAGS: &[&str] = &[
    "--base",
    "--local",
    "--remote",
    "--merged",
    "--auto-resolve",
    "--json",
];

const COMPARE_FLAGS: &[&str] = &[
    "--json",
    "--count",
    "--quiet",
    "-q",
    "--profile",
    "--ignore-case",
    "--ignore-whitespace",
    "--ignore-blank-lines",
    "--ignore-eol",
    "--ignore-line-regex",
    "--substitute-regex",
    "--detect-moves",
    "--diff-algorithm",
    "--inline-granularity",
    "--regex-rule-set",
    "--context",
    "--show-only-changes",
    "--render",
    "--syntax",
    "--find",
    "--find-regex",
    "--find-case-sensitive",
    "--bookmark",
    "--encoding",
    "--type",
    "--image-mode",
    "--image-tolerance",
    "--image-delta-e",
    "--ocr-language",
    "--document-mode",
];
const COMPARE3_FLAGS: &[&str] = &["--markers", "--json"];
const CONFLICT_FLAGS: &[&str] = &["--json"];
const COMPLETION_SHELLS: &[&str] = &["bash", "zsh", "fish"];
const FOLDER_FLAGS: &[&str] = &[
    "--profile",
    "--recursive",
    "-r",
    "--method",
    "--timestamp-tolerance-ms",
    "--symlinks",
    "--large-file-threshold-bytes",
    "--large-file-method",
    "--exclude-generated",
    "--filter",
    "--filter-name",
    "--case-insensitive-filter",
    "--hide-skipped",
    "--state",
    "--search",
    "--sort",
    "--desc",
    "--types",
    "--group-by",
    "--limit",
    "--offset",
    "--dry-run",
    "--hash-algorithm",
    "--compare-permissions",
    "--compare-ownership",
    "--json",
    "--csv",
    "--count",
    "--quiet",
    "-q",
];
const HEX_FLAGS: &[&str] = &[
    "--width",
    "--metadata-only",
    "--json",
    "--count",
    "--quiet",
    "-q",
];
const LAUNCH_FLAGS: &[&str] = &["--wait"];
const OPEN_EXTERNAL_FLAGS: &[&str] = &["--wait", "--preset"];
const OPEN_EXTERNAL_PRESETS: &[&str] = &[
    "xdg-open",
    "kate",
    "kwrite",
    "vscode",
    "vscodium",
    "gnome-text-editor",
    "sublime",
    "nvim-terminal",
    "jetbrains-idea",
    "jetbrains-pycharm",
    "jetbrains-webstorm",
    "jetbrains-clion",
    "jetbrains-rider",
    "jetbrains-goland",
    "jetbrains-phpstorm",
    "jetbrains-rubymine",
    "jetbrains-datagrip",
];
const OUTPUT_FLAGS: &[&str] = &["--output", "-o"];
const REPORT_FLAGS: &[&str] = &[
    "--output",
    "-o",
    "--context",
    "--columns",
    "--tree-state",
    "--nested-file-reports",
];
const PATCH_FLAGS: &[&str] = &["--output", "-o", "--format", "--context", "--preview"];
const REVEAL_FLAGS: &[&str] = &["--wait"];
const SELF_COMPARE_FLAGS: &[&str] = &["--json"];
const TABLE_FLAGS: &[&str] = &[
    "--header",
    "--delimiter",
    "-d",
    "--tsv",
    "--table-quote",
    "--table-escape",
    "--table-comment",
    "--table-skip-blank",
    "--numeric-tolerance",
    "--json",
    "--count",
    "--quiet",
    "-q",
];

fn compare_command(args: &[String]) -> Result<ExitCode, String> {
    let compare_args = split_compare_args(args)?;
    if compare_args.paths.len() != 2 {
        return Err(
            "usage: linsync-cli compare [--profile NAME-OR-PATH] [--type auto|text|binary|hex|folder|table|image|document] [--json|--count|--quiet] [--ignore-case] [--ignore-whitespace] [--ignore-blank-lines] [--ignore-eol] [--ignore-line-regex REGEX] [--regex-rule-set NAME] [--substitute-regex REGEX REPLACEMENT] [--detect-moves] [--diff-algorithm lcs|patience|myers] [--inline-granularity char|word|grapheme] [--context LINES] [--show-only-changes] [--render side-by-side|unified|context|normal|html] [--syntax plain|auto|rust|json|html|markdown|shell|toml|yaml] [--find PATTERN] [--find-regex] [--find-case-sensitive] [--bookmark SIDE:LINE[:LABEL]] [--encoding auto|utf8|utf8-bom|utf16le|utf16be|lossy-utf8] [--image-mode exact|tolerance|perceptual] [--image-tolerance F] [--image-delta-e F] [--document-mode text|ocr_text] [--ocr-language LANG] LEFT RIGHT"
                .to_owned(),
        );
    }

    let left = PathBuf::from(&compare_args.paths[0]);
    let right = PathBuf::from(&compare_args.paths[1]);
    validate_compare_inputs(
        &compare_args.paths[0],
        &compare_args.paths[1],
        compare_args.compare_type,
    )?;

    if let Some(profile_id) = &compare_args.effective_profile {
        eprintln!("info: using compare profile {profile_id}");
    }

    match compare_args.compare_type {
        CompareType::Auto => {
            let compare_type = detect_compare_type(
                &left,
                &right,
                &compare_args.text_options,
                compare_args.explicit_text_options,
            )?;
            match compare_type {
                CompareType::Text => compare_text_command(left, right, compare_args),
                CompareType::Binary | CompareType::Hex => {
                    compare_binary_command(&left, &right, compare_args)
                }
                CompareType::Folder => compare_folder_command(&left, &right, compare_args),
                CompareType::Table => compare_table_command(&left, &right, compare_args),
                CompareType::Image => compare_image_command(&left, &right, compare_args),
                CompareType::Document => compare_document_command(&left, &right, compare_args),
                CompareType::Auto => unreachable!("auto detection resolves to a concrete type"),
            }
        }
        CompareType::Text => compare_text_command(left, right, compare_args),
        CompareType::Binary | CompareType::Hex => {
            compare_binary_command(&left, &right, compare_args)
        }
        CompareType::Folder => compare_folder_command(&left, &right, compare_args),
        CompareType::Table => compare_table_command(&left, &right, compare_args),
        CompareType::Image => compare_image_command(&left, &right, compare_args),
        CompareType::Document => compare_document_command(&left, &right, compare_args),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliPathKind {
    File,
    Directory,
}

fn validate_compare_inputs(
    left: &str,
    right: &str,
    compare_type: CompareType,
) -> Result<(CliPathKind, CliPathKind), String> {
    let left_kind = classify_compare_path(left)?;
    let right_kind = classify_compare_path(right)?;

    match compare_type {
        CompareType::Auto => {
            if left_kind != right_kind {
                return Err(
                    "file-vs-folder compare is not supported; compare two files or two folders"
                        .to_owned(),
                );
            }
        }
        CompareType::Folder => {
            if left_kind != CliPathKind::Directory || right_kind != CliPathKind::Directory {
                return Err("compare --type folder requires two directories".to_owned());
            }
        }
        CompareType::Text
        | CompareType::Binary
        | CompareType::Hex
        | CompareType::Table
        | CompareType::Image
        | CompareType::Document => {
            if left_kind != CliPathKind::File || right_kind != CliPathKind::File {
                return Err(format!(
                    "compare --type {} requires two files",
                    compare_type.as_str()
                ));
            }
        }
    }

    Ok((left_kind, right_kind))
}

fn classify_compare_path(value: &str) -> Result<CliPathKind, String> {
    if value.contains("://") {
        return Err(format!(
            "URL or remote input '{value}' is not supported yet; mount it locally before comparing"
        ));
    }

    let path = Path::new(value);
    let metadata = fs::metadata(path).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => format!("missing path '{}'", path.display()),
        std::io::ErrorKind::PermissionDenied => {
            format!("permission denied accessing path '{}'", path.display())
        }
        _ => format!("cannot access path '{}': {err}", path.display()),
    })?;

    if metadata.is_file() {
        Ok(CliPathKind::File)
    } else if metadata.is_dir() {
        Ok(CliPathKind::Directory)
    } else {
        Err(format!(
            "unsupported path type '{}'; expected a regular file or directory",
            path.display()
        ))
    }
}

impl CompareType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Text => "text",
            Self::Binary => "binary",
            Self::Hex => "hex",
            Self::Folder => "folder",
            Self::Table => "table",
            Self::Image => "image",
            Self::Document => "document",
        }
    }
}

fn detect_compare_type(
    left: &Path,
    right: &Path,
    text_options: &TextCompareOptions,
    explicit_text_options: bool,
) -> Result<CompareType, String> {
    if left.is_dir() || right.is_dir() {
        return Ok(CompareType::Folder);
    }

    if explicit_text_options && text_options != &TextCompareOptions::default() {
        return Ok(CompareType::Text);
    }

    // Classify binary-ness from a bounded prefix only; the chosen engine still
    // does its own full read. `is_likely_binary` already caps its control-char
    // scan at 4 KiB, so a prefix of that size yields the same verdict while
    // avoiding loading entire (possibly huge) files just to detect their type.
    let left_sample = read_classification_prefix(left)?;
    let right_sample = read_classification_prefix(right)?;
    if binary_extension(left)
        || binary_extension(right)
        || is_likely_binary(&left_sample)
        || is_likely_binary(&right_sample)
    {
        return Ok(CompareType::Binary);
    }

    if table_extension(left) && table_extension(right) {
        return Ok(CompareType::Table);
    }

    Ok(CompareType::Text)
}

/// Read at most the leading 4 KiB of `path` for binary/text classification.
///
/// `is_likely_binary`'s control-character heuristic already samples only the
/// first 4 KiB, and NUL detection over this prefix matches the standard
/// prefix-based approach, so this bounded read produces the same verdict as
/// reading the whole file without paying to load large inputs twice.
fn read_classification_prefix(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;

    const PREFIX_LEN: u64 = 4096;
    let file = fs::File::open(path).map_err(|err| err.to_string())?;
    let mut buf = Vec::with_capacity(PREFIX_LEN as usize);
    file.take(PREFIX_LEN)
        .read_to_end(&mut buf)
        .map_err(|err| err.to_string())?;
    Ok(buf)
}

fn table_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "csv" | "tsv"))
}

fn binary_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "bin" | "dat" | "exe" | "dll" | "so" | "dylib" | "o" | "a"
            )
        })
}

fn compare_text_command(
    left: PathBuf,
    right: PathBuf,
    compare_args: CompareArgs,
) -> Result<ExitCode, String> {
    let result = compare_text_files(&left, &right, &compare_args.text_options)
        .map_err(|err| err.to_string())?;

    match compare_args.output {
        OutputMode::Text => {
            println!(
                "{} vs {}: {} differing lines",
                result.left_name,
                result.right_name,
                result.difference_count()
            );

            if uses_text_rendering_options(&compare_args.text_options) {
                print!("{}", result.render_text(&compare_args.text_options));
            } else {
                for line in result.lines.iter().filter(|line| {
                    matches!(
                        line.kind,
                        DiffLineKind::Changed | DiffLineKind::LeftOnly | DiffLineKind::RightOnly
                    )
                }) {
                    match line.kind {
                        DiffLineKind::LeftOnly => {
                            println!("- {}", line.left.as_deref().unwrap_or(""))
                        }
                        DiffLineKind::RightOnly => {
                            println!("+ {}", line.right.as_deref().unwrap_or(""))
                        }
                        DiffLineKind::Changed => {
                            println!("~ {}", line.left.as_deref().unwrap_or(""));
                            println!("~ {}", line.right.as_deref().unwrap_or(""));
                        }
                        DiffLineKind::Equal => {}
                    }
                }
            }

            if let Some(find) = &compare_args.text_options.find {
                let matches = result
                    .find_matches(find)
                    .map_err(|err| format!("invalid find regex: {err}"))?;
                println!("find_matches={}", matches.len());
                for m in matches.iter().take(20) {
                    println!("  {:?}:{}:{}-{} {}", m.side, m.line, m.start, m.end, m.text);
                }
            }
            if !compare_args.text_options.bookmarks.is_empty() {
                println!("bookmarks={}", compare_args.text_options.bookmarks.len());
                for bookmark in &compare_args.text_options.bookmarks {
                    println!("  {:?}:{} {}", bookmark.side, bookmark.line, bookmark.label);
                }
            }
        }
        OutputMode::Json => {
            let moved_count = result
                .blocks
                .iter()
                .filter(|b| {
                    matches!(
                        b.kind,
                        DiffBlockKind::Moved {
                            direction: MoveDirection::LeftToRight,
                            ..
                        }
                    )
                })
                .count();
            if compare_args.effective_profile.is_some()
                || uses_extended_text_json(&compare_args.text_options)
            {
                let mut json = serde_json::json!({
                    "equal": result.is_equal(),
                    "differences": result.difference_count(),
                    "moved_blocks": moved_count,
                });
                if let Some(profile_id) = compare_args.effective_profile.as_deref() {
                    json["profile"] = serde_json::json!(profile_id);
                }
                if uses_extended_text_json(&compare_args.text_options) {
                    json["encoding"] = serde_json::json!(result.encoding_summary());
                    json["render_mode"] = serde_json::json!(compare_args.text_options.render_mode);
                    json["syntax_mode"] = serde_json::json!(compare_args.text_options.syntax_mode);
                    json["context_lines"] =
                        serde_json::json!(compare_args.text_options.context_lines);
                    json["show_only_changes"] =
                        serde_json::json!(compare_args.text_options.show_only_changes);
                    json["regex_rule_sets"] =
                        serde_json::json!(compare_args.text_options.regex_rule_sets);
                    json["bookmarks"] = serde_json::json!(compare_args.text_options.bookmarks);
                    if let Some(find) = &compare_args.text_options.find {
                        json["find"] = serde_json::json!(find);
                        json["find_matches"] = serde_json::json!(
                            result
                                .find_matches(find)
                                .map_err(|err| format!("invalid find regex: {err}"))?
                        );
                    }
                }
                println!("{json}");
            } else {
                println!(
                    "{{\"equal\":{},\"differences\":{},\"moved_blocks\":{}}}",
                    result.is_equal(),
                    result.difference_count(),
                    moved_count,
                );
            }
        }
        OutputMode::Count => println!("{}", result.difference_count()),
        OutputMode::Quiet => {}
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn compare_binary_command(
    left: &Path,
    right: &Path,
    compare_args: CompareArgs,
) -> Result<ExitCode, String> {
    let result = compare_binary_files(left, right, &compare_args.binary_options)
        .map_err(|err| err.to_string())?;
    let differences = result.differences.len();

    match compare_args.output {
        OutputMode::Text => {
            println!(
                "{} vs {}: {} differing bytes",
                result.left_name, result.right_name, differences
            );
            for row in result.rows.iter().filter(|row| row.has_difference).take(12) {
                println!(
                    "{:08X} | {:<48} | {:<48} | {} | {}",
                    row.offset, row.left_hex, row.right_hex, row.left_ascii, row.right_ascii
                );
            }
        }
        OutputMode::Json => {
            if let Some(profile_id) = compare_args.effective_profile.as_deref() {
                println!(
                    "{}",
                    serde_json::json!({
                        "equal": result.is_equal(),
                        "differences": differences,
                        "profile": profile_id,
                    })
                );
            } else {
                println!(
                    "{{\"equal\":{},\"differences\":{}}}",
                    result.is_equal(),
                    differences
                );
            }
        }
        OutputMode::Count => println!("{differences}"),
        OutputMode::Quiet => {}
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn uses_text_rendering_options(options: &TextCompareOptions) -> bool {
    options.render_mode != TextRenderMode::SideBySide
        || options.context_lines.is_some()
        || options.show_only_changes
}

fn uses_extended_text_json(options: &TextCompareOptions) -> bool {
    uses_text_rendering_options(options)
        || options.syntax_mode != TextSyntaxMode::Plain
        || options.encoding != TextInputEncoding::Auto
        || !options.regex_rule_sets.is_empty()
        || options.find.is_some()
        || !options.bookmarks.is_empty()
}

fn compare_folder_command(
    left: &Path,
    right: &Path,
    compare_args: CompareArgs,
) -> Result<ExitCode, String> {
    let result = compare_folders(left, right, &compare_args.folder_options)
        .map_err(|err| err.to_string())?;
    let summary = &result.summary;
    let differences = summary.different_count + summary.one_sided_count + summary.errors_count;

    match compare_args.output {
        OutputMode::Text => println!(
            "compared={} skipped={} identical={} different={} one_sided={} left_only={} right_only={} errors={} elapsed_ms={} status=complete",
            summary.compared_count,
            summary.skipped_count,
            summary.identical_count,
            summary.different_count,
            summary.one_sided_count,
            summary.left_only_count,
            summary.right_only_count,
            summary.errors_count,
            summary.elapsed.as_millis()
        ),
        OutputMode::Json => {
            let mut json = serde_json::json!({
                "equal": result.is_equal(),
                "compared": summary.compared_count,
                "skipped": summary.skipped_count,
                "identical": summary.identical_count,
                "different": summary.different_count,
                "one_sided": summary.one_sided_count,
                "left_only": summary.left_only_count,
                "right_only": summary.right_only_count,
                "errors": summary.errors_count,
                "elapsed_ms": summary.elapsed.as_millis(),
                "status": "complete",
                "options": folder_options_metadata_json(
                    &compare_args.folder_options,
                    compare_args.effective_profile.as_deref(),
                    FolderEntryFilter::All,
                ),
            });
            if let Some(profile_id) = compare_args.effective_profile.as_deref()
                && let Some(obj) = json.as_object_mut()
            {
                obj.insert("profile".to_owned(), serde_json::json!(profile_id));
            }
            println!("{json}");
        }
        OutputMode::Count => println!("{differences}"),
        OutputMode::Quiet => {}
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn compare_table_command(
    left: &Path,
    right: &Path,
    compare_args: CompareArgs,
) -> Result<ExitCode, String> {
    let mut options = compare_args.table_options.clone();
    if options == TableCompareOptions::default()
        && (has_tsv_extension(left) || has_tsv_extension(right))
    {
        options.delimiter = '\t';
    }
    let result = compare_table_files(left, right, &options).map_err(|err| err.to_string())?;

    match compare_args.output {
        OutputMode::Text => println!(
            "{} vs {}: changed_cells={}",
            result.left_name, result.right_name, result.changed_cells
        ),
        OutputMode::Json => {
            if let Some(profile_id) = compare_args.effective_profile.as_deref() {
                println!(
                    "{}",
                    serde_json::json!({
                        "equal": result.is_equal(),
                        "rows": result.rows.len(),
                        "changed_cells": result.changed_cells,
                        "profile": profile_id,
                    })
                );
            } else {
                println!(
                    "{}",
                    serde_json::json!({
                        "equal": result.is_equal(),
                        "rows": result.rows.len(),
                        "changed_cells": result.changed_cells,
                    })
                );
            }
        }
        OutputMode::Count => println!("{}", result.changed_cells),
        OutputMode::Quiet => {}
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn has_tsv_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("tsv"))
}

fn compare_image_command(left: &Path, right: &Path, args: CompareArgs) -> Result<ExitCode, String> {
    use linsync_core::{ImageCompareMode, ImageCompareOptions, compare_images};

    let mode = match args.image_options.mode.as_str() {
        "tolerance" => ImageCompareMode::Tolerance(args.image_options.tolerance),
        "perceptual" => ImageCompareMode::Perceptual,
        _ => ImageCompareMode::Exact,
    };

    let opts = ImageCompareOptions {
        mode,
        tolerance: args.image_options.tolerance,
        delta_e_threshold: args.image_options.delta_e,
        ..ImageCompareOptions::default()
    };

    let result = compare_images(left, right, &opts).map_err(|e| e.to_string())?;

    match args.output {
        OutputMode::Json => {
            let json = serde_json::json!({
                "equal": result.equal,
                "left_dims": result.left_dims,
                "right_dims": result.right_dims,
                "total_pixels": result.total_pixels,
                "differing_pixels": result.differing_pixels,
                "diff_ratio": result.diff_ratio,
                "mode": args.image_options.mode,
                "diff_bbox": result.diff_bbox,
            });
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        OutputMode::Quiet => {}
        OutputMode::Count => {
            println!("{}", result.differing_pixels);
        }
        OutputMode::Text => {
            if result.equal {
                println!("Images are equal ({} pixels)", result.total_pixels);
            } else {
                println!(
                    "Images differ: {} of {} pixels ({:.2}%)",
                    result.differing_pixels,
                    result.total_pixels,
                    result.diff_ratio * 100.0,
                );
            }
        }
    }

    if result.equal {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(1))
    }
}

fn compare_document_command(
    left: &Path,
    right: &Path,
    args: CompareArgs,
) -> Result<ExitCode, String> {
    use linsync_core::document::{DocumentCompareError, compare_document_files};
    use linsync_core::{DocumentCompareMode, DocumentCompareOptions};

    let mode = match args.document_options.mode.as_str() {
        "ocr_text" => DocumentCompareMode::OcrText,
        _ => DocumentCompareMode::Text,
    };

    // Locate the plugins dir relative to the binary (packaging/plugins in dev,
    // $prefix/share/linsync/plugins in an installed build).
    let plugins_root = detect_plugins_root();

    let opts = DocumentCompareOptions {
        mode,
        ocr_language: args.document_options.ocr_language.clone(),
        ..DocumentCompareOptions::default()
    };

    let result =
        compare_document_files(left, right, &plugins_root, &opts).map_err(|e| match e {
            DocumentCompareError::NoSuitablePlugin { path, mime_hint } => {
                format!(
                    "no document-compare plugin for '{path}' (MIME: {mime_hint}); \
                     install a LinSync document plugin"
                )
            }
            other => other.to_string(),
        })?;

    let text_result = result.text_result.as_ref();
    let is_equal = text_result.map(|t| t.is_equal()).unwrap_or(false);

    match args.output {
        OutputMode::Json => {
            let diff_count = text_result.map(|t| t.difference_count()).unwrap_or(0);
            let json = serde_json::json!({
                "equal": is_equal,
                "left_extractor": result.left_extractor,
                "right_extractor": result.right_extractor,
                "differing_lines": diff_count,
                "mode": args.document_options.mode,
            });
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        OutputMode::Quiet => {}
        OutputMode::Count => {
            let diff_count = text_result.map(|t| t.difference_count()).unwrap_or(0);
            println!("{diff_count}");
        }
        OutputMode::Text => {
            if is_equal {
                println!(
                    "Documents are equal (extracted via {})",
                    result.left_extractor
                );
            } else {
                let diff_count = text_result.map(|t| t.difference_count()).unwrap_or(0);
                println!(
                    "Documents differ: {diff_count} differing lines (extracted via {})",
                    result.left_extractor
                );
            }
        }
    }

    if is_equal {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(1))
    }
}

/// Return the directory where LinSync plugins are installed.
///
/// In a development build, plugins live in `<workspace>/packaging/plugins`.
/// In an installed build, they live in `$prefix/share/linsync/plugins`.
fn detect_plugins_root() -> std::path::PathBuf {
    // Walk up from the binary until we find packaging/plugins (handles both
    // target/debug/ and target/debug/deps/ binary locations).
    if let Ok(exe) = std::env::current_exe() {
        let mut candidate = exe.parent().map(|p| p.to_path_buf());
        while let Some(dir) = candidate {
            let plugins = dir.join("packaging/plugins");
            if plugins.is_dir() {
                return plugins;
            }
            candidate = dir.parent().map(|p| p.to_path_buf());
        }
    }
    // Fallback: system install path
    std::path::PathBuf::from("/usr/share/linsync/plugins")
}

fn compare3_command(args: &[String]) -> Result<ExitCode, String> {
    let mut emit_markers = false;
    let mut json = false;
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--markers" => emit_markers = true,
            "--json" => json = true,
            value => paths.push(value),
        }
    }

    if emit_markers && json {
        return Err("compare3 --markers cannot be combined with --json".to_owned());
    }

    if paths.len() != 3 {
        return Err("usage: linsync-cli compare3 [--markers|--json] LEFT BASE RIGHT".to_owned());
    }

    let left = PathBuf::from(paths[0]);
    let base = PathBuf::from(paths[1]);
    let right = PathBuf::from(paths[2]);
    let left_base = compare_text_files(&left, &base, &TextCompareOptions::default())
        .map_err(|err| err.to_string())?;
    let right_base = compare_text_files(&right, &base, &TextCompareOptions::default())
        .map_err(|err| err.to_string())?;
    let left_text = fs::read_to_string(&left).map_err(|err| err.to_string())?;
    let base_text = fs::read_to_string(&base).map_err(|err| err.to_string())?;
    let right_text = fs::read_to_string(&right).map_err(|err| err.to_string())?;
    let merge = merge_three_way(&base_text, &left_text, &right_text);

    if json {
        println!(
            "{}",
            serde_json::json!({
                "equal": left_base.is_equal() && right_base.is_equal() && !merge.has_conflicts(),
                "left_base_differences": left_base.summary.differences,
                "left_base_blocks": left_base.summary.diff_blocks,
                "right_base_differences": right_base.summary.differences,
                "right_base_blocks": right_base.summary.diff_blocks,
                "conflicts": merge.conflicts.len(),
            })
        );
    } else {
        println!(
            "left/base differences={} blocks={}",
            left_base.summary.differences, left_base.summary.diff_blocks
        );
        println!(
            "right/base differences={} blocks={}",
            right_base.summary.differences, right_base.summary.diff_blocks
        );
        println!("conflicts={}", merge.conflicts.len());
    }

    if emit_markers {
        print!(
            "{}",
            merge.conflict_marker_text(
                &left.display().to_string(),
                &base.display().to_string(),
                &right.display().to_string()
            )
        );
    }

    Ok(
        if left_base.is_equal() && right_base.is_equal() && !merge.has_conflicts() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        },
    )
}

fn conflict_command(args: &[String]) -> Result<ExitCode, String> {
    let mut json = false;
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            value => paths.push(value),
        }
    }

    if paths.len() != 1 {
        return Err("usage: linsync-cli conflict [--json] FILE".to_owned());
    }

    let path = PathBuf::from(paths[0]);
    let text = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let conflicts = parse_conflict_markers(&text).map_err(|err| err.to_string())?;

    if json {
        let conflicts_json = conflicts
            .iter()
            .map(|conflict| {
                serde_json::json!({
                    "index": conflict.index,
                    "start_line": conflict.start_line,
                    "end_line": conflict.end_line,
                    "left_label": conflict.left_label,
                    "base_label": conflict.base_label,
                    "right_label": conflict.right_label,
                    "left_lines": conflict.left_lines.len(),
                    "base_lines": conflict.base_lines.len(),
                    "right_lines": conflict.right_lines.len(),
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::json!({
                "path": path.display().to_string(),
                "conflicts": conflicts.len(),
                "items": conflicts_json,
            })
        );
    } else {
        println!("{}: conflicts={}", path.display(), conflicts.len());
        for conflict in &conflicts {
            println!(
                "conflict={} lines={}-{} left={} base={} right={} left_lines={} base_lines={} right_lines={}",
                conflict.index + 1,
                conflict.start_line,
                conflict.end_line,
                display_label(&conflict.left_label),
                conflict
                    .base_label
                    .as_deref()
                    .map(display_label)
                    .unwrap_or("-"),
                display_label(&conflict.right_label),
                conflict.left_lines.len(),
                conflict.base_lines.len(),
                conflict.right_lines.len()
            );
        }
    }

    Ok(if conflicts.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn display_label(label: &str) -> &str {
    if label.is_empty() { "-" } else { label }
}

fn merge_conflicts_json(conflicts: &[ThreeWayConflict]) -> Vec<serde_json::Value> {
    conflicts
        .iter()
        .map(|conflict| {
            serde_json::json!({
                "id": conflict.id.0,
                "start_line": conflict.start_line,
                "end_line": conflict.end_line,
                "left_lines": conflict.left_lines.len(),
                "base_lines": conflict.base_lines.len(),
                "right_lines": conflict.right_lines.len(),
            })
        })
        .collect()
}

fn mergetool_command(args: &[String]) -> Result<ExitCode, String> {
    let mut base: Option<String> = None;
    let mut local: Option<String> = None;
    let mut remote: Option<String> = None;
    let mut merged: Option<String> = None;
    let mut auto: Option<String> = None;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--base" => {
                base = args.get(i + 1).cloned();
                i += 2;
            }
            "--local" => {
                local = args.get(i + 1).cloned();
                i += 2;
            }
            "--remote" => {
                remote = args.get(i + 1).cloned();
                i += 2;
            }
            "--merged" => {
                merged = args.get(i + 1).cloned();
                i += 2;
            }
            "--auto-resolve" => {
                auto = args.get(i + 1).cloned();
                i += 2;
            }
            "--json" => {
                json = true;
                i += 1;
            }
            other => {
                return Err(format!("unknown mergetool flag: {other}"));
            }
        }
    }

    let (base, local, remote, merged) = match (base, local, remote, merged) {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => {
            return Err("mergetool requires --base, --local, --remote, --merged".to_owned());
        }
    };

    let base_text = fs::read_to_string(&base).map_err(|e| e.to_string())?;
    let local_text = fs::read_to_string(&local).map_err(|e| e.to_string())?;
    let remote_text = fs::read_to_string(&remote).map_err(|e| e.to_string())?;

    let base_doc = TextDocument::from_text("base", &base_text);
    let local_doc = TextDocument::from_text("local", &local_text);
    let remote_doc = TextDocument::from_text("remote", &remote_text);

    let mut state = ThreeWayMergeState::new(base_doc, local_doc, remote_doc);
    let initial_conflicts = state.conflicts();

    if let Some(choice) = auto.as_deref() {
        let mc = match choice {
            "left" => MergeChoice::Left,
            "right" => MergeChoice::Right,
            "base" => MergeChoice::Base,
            other => {
                return Err(format!(
                    "invalid --auto-resolve value '{other}'; expected left, right, or base"
                ));
            }
        };
        let conflict_ids: Vec<_> = initial_conflicts.iter().map(|c| c.id).collect();
        for id in conflict_ids {
            state
                .resolve(id, mc.clone())
                .map_err(|e| format!("resolve failed: {e}"))?;
        }
        state
            .save_to(std::path::Path::new(&merged))
            .map_err(|e| format!("save failed: {e}"))?;
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "status": "resolved",
                    "mode": "auto",
                    "auto_choice": choice,
                    "base": base,
                    "local": local,
                    "remote": remote,
                    "merged": merged,
                    "conflicts": initial_conflicts.len(),
                    "resolved_conflicts": initial_conflicts.len(),
                    "unresolved_conflicts": state.unresolved_count(),
                    "written": true,
                    "items": merge_conflicts_json(&initial_conflicts),
                })
            );
        }
        return Ok(ExitCode::SUCCESS);
    }

    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "unsupported_interactive",
                "mode": "interactive",
                "base": base,
                "local": local,
                "remote": remote,
                "merged": merged,
                "conflicts": initial_conflicts.len(),
                "resolved_conflicts": 0,
                "unresolved_conflicts": state.unresolved_count(),
                "written": false,
                "items": merge_conflicts_json(&initial_conflicts),
            })
        );
    }

    // No --auto-resolve: interactive GUI mode not yet implemented in v1.
    eprintln!(
        "interactive mergetool mode not yet implemented; use --auto-resolve <left|right|base>"
    );
    Ok(ExitCode::from(2))
}

fn completions_command(args: &[String]) -> Result<ExitCode, String> {
    if args.len() != 1 {
        return Err("usage: linsync-cli completions SHELL".to_owned());
    }

    let completions = match args[0].as_str() {
        "bash" => bash_completions(),
        "zsh" => zsh_completions(),
        "fish" => fish_completions(),
        other => {
            return Err(format!(
                "unsupported completion shell '{other}'; expected bash, zsh, or fish"
            ));
        }
    };

    print!("{completions}");
    Ok(ExitCode::SUCCESS)
}

fn archive_command(args: &[String]) -> Result<ExitCode, String> {
    let mut paths = Vec::new();
    let mut keep_temp = false;
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--keep-temp" => keep_temp = true,
            "--json" => json = true,
            value if value.starts_with("--") => {
                return Err(format!("unknown archive flag '{value}'"));
            }
            value => paths.push(value.to_owned()),
        }
    }
    if paths.len() != 2 {
        return Err(
            "usage: linsync-cli archive [--keep-temp] [--json] LEFT.{zip|tar|tgz|...} RIGHT.{...}"
                .to_owned(),
        );
    }

    let cache_root = AppPaths::from_env().comparison_cache_dir();
    fs::create_dir_all(&cache_root).map_err(|err| format!("cannot prepare cache dir: {err}"))?;

    let mut left = extract_archive(&PathBuf::from(&paths[0]), &cache_root, "left")?;
    let mut right = extract_archive(&PathBuf::from(&paths[1]), &cache_root, "right")?;
    if keep_temp {
        left.keep();
        right.keep();
    }

    let result = compare_folders(&left.path, &right.path, &FolderCompareOptions::default())
        .map_err(|err| format!("folder compare failed: {err}"))?;

    if json {
        let body = serde_json::json!({
            "left": { "archive": paths[0], "extracted_to": left.path.display().to_string() },
            "right": { "archive": paths[1], "extracted_to": right.path.display().to_string() },
            "summary": {
                "compared": result.summary.compared_count,
                "identical": result.summary.identical_count,
                "different": result.summary.different_count,
                "one_sided": result.summary.one_sided_count,
                "errors": result.summary.errors_count,
            },
        });
        println!("{body}");
    } else {
        println!(
            "compared={} identical={} different={} one_sided={} errors={}",
            result.summary.compared_count,
            result.summary.identical_count,
            result.summary.different_count,
            result.summary.one_sided_count,
            result.summary.errors_count,
        );
    }

    // `left`/`right` clean their extracted trees on drop (best-effort
    // `remove_dir_all`) unless `keep_temp` flagged them to be retained, so the
    // cache dir is never leaked even on the error paths above.

    let code = if result.summary.different_count > 0
        || result.summary.one_sided_count > 0
        || result.summary.errors_count > 0
    {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    };
    Ok(code)
}

struct ExtractedArchive {
    path: PathBuf,
    temp_root: PathBuf,
    keep: bool,
}

impl ExtractedArchive {
    /// Retain the extracted tree past drop (honors `--keep-temp`).
    fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for ExtractedArchive {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.temp_root);
        }
    }
}

fn extract_archive(
    archive: &Path,
    cache_root: &Path,
    side: &str,
) -> Result<ExtractedArchive, String> {
    if !archive.is_file() {
        return Err(format!("archive '{}' is not a file", archive.display()));
    }
    let stem = archive
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive".to_owned());
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_root = cache_root.join(format!("archive-{side}-{timestamp}-{stem}"));
    let extracted = temp_root.join("extracted");
    fs::create_dir_all(&extracted).map_err(|err| {
        format!(
            "cannot create extraction dir '{}': {err}",
            extracted.display()
        )
    })?;

    let lower = stem.to_ascii_lowercase();
    let status = if lower.ends_with(".zip")
        || lower.ends_with(".jar")
        || lower.ends_with(".war")
        || lower.ends_with(".apk")
        || lower.ends_with(".ipa")
    {
        Command::new("unzip")
            .arg("-q")
            .arg("-o")
            .arg("-d")
            .arg(&extracted)
            .arg(archive)
            .status()
            .map_err(|err| format!("failed to invoke unzip: {err}"))?
    } else if lower.ends_with(".tar")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tbz2")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".txz")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".tzst")
        || lower.ends_with(".tar.zst")
    {
        Command::new("tar")
            .arg("-xf")
            .arg(archive)
            .arg("-C")
            .arg(&extracted)
            .status()
            .map_err(|err| format!("failed to invoke tar: {err}"))?
    } else {
        let _ = fs::remove_dir_all(&temp_root);
        return Err(format!(
            "unsupported archive extension for '{}'; install a plugin or use a supported type (zip, jar, tar, tgz, tar.xz, tar.zst, ...)",
            archive.display()
        ));
    };

    if !status.success() {
        let _ = fs::remove_dir_all(&temp_root);
        return Err(format!(
            "archive extraction failed for '{}': exit status {status}",
            archive.display()
        ));
    }

    Ok(ExtractedArchive {
        path: extracted,
        temp_root,
        keep: false,
    })
}

// ── Profile management ───────────────────────────────────────────────────────

fn plugin_command(args: &[String]) -> Result<ExitCode, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        eprintln!(
            "usage: linsync-cli plugin <list [--json] | inspect ID [--json] | validate ID | enable ID | disable ID | set-option ID KEY VALUE | clear-option ID KEY | run-diagnostic ID [--input FILE] [--timeout-ms MS] [--json]>"
        );
        return Ok(ExitCode::from(2));
    };
    let paths = AppPaths::from_env();
    let rest = &args[1..];
    let wants_json = rest.iter().any(|arg| arg == "--json");
    let first_positional = rest.iter().find(|arg| !arg.starts_with("--"));
    match subcommand {
        "list" => plugin_list(&paths, wants_json),
        "inspect" | "show" => {
            let Some(id) = first_positional else {
                return Err("usage: linsync-cli plugin inspect ID [--json]".to_owned());
            };
            plugin_inspect(&paths, id, wants_json)
        }
        "validate" => {
            let Some(id) = first_positional else {
                return Err("usage: linsync-cli plugin validate ID".to_owned());
            };
            plugin_validate(&paths, id)
        }
        "enable" | "disable" => {
            let Some(id) = first_positional else {
                return Err(format!("usage: linsync-cli plugin {subcommand} ID"));
            };
            let enabled = subcommand == "enable";
            set_plugin_enabled(&paths, id, enabled).map_err(|err| err.to_string())?;
            println!(
                "{} plugin '{id}'",
                if enabled { "enabled" } else { "disabled" }
            );
            Ok(ExitCode::SUCCESS)
        }
        "set-option" => {
            let (Some(id), Some(key), Some(raw)) = (rest.first(), rest.get(1), rest.get(2)) else {
                return Err("usage: linsync-cli plugin set-option ID KEY VALUE".to_owned());
            };
            // Parse VALUE as JSON so `true`/`7`/`"x"` get the right type; fall
            // back to a plain string for un-quoted convenience.
            let value: serde_json::Value = serde_json::from_str(raw)
                .unwrap_or_else(|_| serde_json::Value::String(raw.to_owned()));
            set_plugin_option(&paths, id, key, value).map_err(|err| err.to_string())?;
            println!("set option '{key}' for plugin '{id}'");
            Ok(ExitCode::SUCCESS)
        }
        "clear-option" => {
            let (Some(id), Some(key)) = (rest.first(), rest.get(1)) else {
                return Err("usage: linsync-cli plugin clear-option ID KEY".to_owned());
            };
            clear_plugin_option(&paths, id, key).map_err(|err| err.to_string())?;
            println!("cleared option '{key}' for plugin '{id}'");
            Ok(ExitCode::SUCCESS)
        }
        "run-diagnostic" | "diagnostic" => plugin_run_diagnostic(&paths, rest),
        other => Err(format!(
            "unknown plugin subcommand '{other}'; expected list, inspect, validate, enable, disable, set-option, clear-option, or run-diagnostic"
        )),
    }
}

/// `plugin run-diagnostic ID [--input FILE] [--timeout-ms MS] [--json]` — probe
/// a discovered plugin's helper and report exit / timeout / stdout / stderr and
/// the parsed protocol response. Exit 0 when healthy, 1 when the helper ran but
/// reported a problem, 2 on a transport/encoding error.
fn plugin_run_diagnostic(paths: &AppPaths, rest: &[String]) -> Result<ExitCode, String> {
    const USAGE: &str =
        "usage: linsync-cli plugin run-diagnostic ID [--input FILE] [--timeout-ms MS] [--json]";
    let mut id: Option<&str> = None;
    let mut input: Option<&str> = None;
    let mut timeout_ms: Option<u64> = None;
    let mut as_json = false;
    let mut index = 0;
    while index < rest.len() {
        match rest[index].as_str() {
            "--json" => {
                as_json = true;
                index += 1;
            }
            "--input" => {
                input = Some(
                    rest.get(index + 1)
                        .ok_or("--input requires a FILE path")?
                        .as_str(),
                );
                index += 2;
            }
            "--timeout-ms" => {
                let raw = rest.get(index + 1).ok_or("--timeout-ms requires a value")?;
                timeout_ms =
                    Some(raw.parse::<u64>().map_err(|_| {
                        format!("invalid --timeout-ms '{raw}': expected an integer")
                    })?);
                index += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag '{other}'; {USAGE}"));
            }
            other => {
                if id.is_some() {
                    return Err(format!(
                        "plugin run-diagnostic takes a single plugin ID; {USAGE}"
                    ));
                }
                id = Some(other);
                index += 1;
            }
        }
    }
    let id = id.ok_or(USAGE)?;

    let discovery = discover_installed_plugins(paths);
    let plugin = discovery
        .plugins
        .iter()
        .find(|p| p.manifest.id == id)
        .ok_or_else(|| format!("no installed plugin with id '{id}'"))?;

    let mut inputs = Vec::new();
    if let Some(path) = input {
        let path_buf = PathBuf::from(path);
        if !path_buf.exists() {
            return Err(format!("--input file '{path}' does not exist"));
        }
        let extension = path_buf
            .extension()
            .map(|ext| ext.to_string_lossy().into_owned());
        inputs.push(PluginInputDescriptor {
            role: "input".to_owned(),
            path: path_buf,
            display_name: None,
            mime_type: None,
            extension,
            read_only: true,
        });
    }

    let mut options = PluginExecutionOptions::default();
    if let Some(ms) = timeout_ms {
        options.timeout = Duration::from_millis(ms);
    }

    let outcome = probe_plugin(&plugin.root, &plugin.manifest, inputs, &options)
        .map_err(|err| err.to_string())?;

    if as_json {
        let response = outcome.response.as_ref().map(|r| {
            serde_json::json!({
                "status": format!("{:?}", r.status).to_lowercase(),
                "diagnostics": r
                    .diagnostics
                    .iter()
                    .map(|d| serde_json::json!({"severity": d.severity, "message": d.message}))
                    .collect::<Vec<_>>(),
                "error": r
                    .error
                    .as_ref()
                    .map(|e| serde_json::json!({"code": e.code, "message": e.message})),
            })
        });
        let body = serde_json::json!({
            "id": id,
            "healthy": outcome.is_healthy(),
            "exit_code": outcome.exit_code,
            "timed_out": outcome.timed_out,
            "stdout": outcome.stdout,
            "stderr": outcome.stderr,
            "response": response,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?
        );
    } else {
        println!("plugin:    {id}");
        println!("healthy:   {}", outcome.is_healthy());
        match outcome.exit_code {
            Some(code) => println!("exit:      {code}"),
            None => println!("exit:      (none)"),
        }
        println!("timed_out: {}", outcome.timed_out);
        if let Some(response) = &outcome.response {
            println!("status:    {:?}", response.status);
            for d in &response.diagnostics {
                println!("  diagnostic [{}]: {}", d.severity, d.message);
            }
            if let Some(err) = &response.error {
                println!("  error [{}]: {}", err.code, err.message);
            }
        } else if !outcome.stdout.trim().is_empty() {
            println!("stdout:    {}", outcome.stdout.trim());
        }
        if !outcome.stderr.trim().is_empty() {
            println!("stderr:    {}", outcome.stderr.trim());
        }
    }

    Ok(if outcome.is_healthy() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn plugin_class_names(classes: &[linsync_core::PluginClass]) -> Vec<String> {
    classes.iter().map(|c| format!("{c:?}")).collect()
}

fn plugin_list(paths: &AppPaths, as_json: bool) -> Result<ExitCode, String> {
    let discovery = discover_installed_plugins(paths);
    let enabled = load_plugin_enabled_map(paths);
    if as_json {
        let plugins: Vec<serde_json::Value> = discovery
            .plugins
            .iter()
            .map(|p| {
                let m = &p.manifest;
                serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                    "version": m.version,
                    "classes": plugin_class_names(&m.classes),
                    "enabled": enabled.get(&m.id).copied().unwrap_or(true),
                    "has_options": !m.options_schema.is_empty(),
                })
            })
            .collect();
        let errors: Vec<serde_json::Value> = discovery
            .errors
            .iter()
            .map(
                |e| serde_json::json!({"path": e.path.display().to_string(), "message": e.message}),
            )
            .collect();
        let body = serde_json::json!({ "plugins": plugins, "errors": errors });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?
        );
    } else {
        if discovery.plugins.is_empty() {
            println!("No plugins discovered.");
        }
        for p in &discovery.plugins {
            let m = &p.manifest;
            let state = if enabled.get(&m.id).copied().unwrap_or(true) {
                "enabled"
            } else {
                "disabled"
            };
            let opts = if m.options_schema.is_empty() {
                ""
            } else {
                " [options]"
            };
            println!(
                "{}\t{}\t{}{}",
                m.id,
                state,
                plugin_class_names(&m.classes).join(","),
                opts
            );
        }
        for e in &discovery.errors {
            eprintln!("error: {}: {}", e.path.display(), e.message);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn plugin_inspect(paths: &AppPaths, id: &str, as_json: bool) -> Result<ExitCode, String> {
    let discovery = discover_installed_plugins(paths);
    let plugin = discovery
        .plugins
        .iter()
        .find(|p| p.manifest.id == id)
        .ok_or_else(|| format!("no installed plugin with id '{id}'"))?;
    let m = &plugin.manifest;
    let enabled = load_plugin_enabled_map(paths)
        .get(id)
        .copied()
        .unwrap_or(true);
    let values = load_plugin_options(paths, id);
    if as_json {
        let schema: Vec<serde_json::Value> = m
            .options_schema
            .iter()
            .map(|o| {
                serde_json::json!({
                    "key": o.key,
                    "label": o.label,
                    "kind": format!("{:?}", o.kind).to_lowercase(),
                    "default": o.default,
                    "choices": o.choices,
                })
            })
            .collect();
        let body = serde_json::json!({
            "id": m.id,
            "name": m.name,
            "version": m.version,
            "license": m.license,
            "classes": plugin_class_names(&m.classes),
            "enabled": enabled,
            "root": plugin.root.display().to_string(),
            "options_schema": schema,
            "values": values,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?
        );
    } else {
        println!("id:       {}", m.id);
        println!("name:     {}", m.name);
        println!("version:  {}", m.version);
        println!("license:  {}", m.license);
        println!("classes:  {}", plugin_class_names(&m.classes).join(", "));
        println!("enabled:  {enabled}");
        println!("root:     {}", plugin.root.display());
        if m.options_schema.is_empty() {
            println!("options:  (none)");
        } else {
            println!("options:");
            for o in &m.options_schema {
                let current = values
                    .get(&o.key)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "<unset>".to_owned());
                let choices = if o.choices.is_empty() {
                    String::new()
                } else {
                    format!(" choices=[{}]", o.choices.join(","))
                };
                println!("  {} ({:?}){}  current={current}", o.key, o.kind, choices);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn plugin_validate(paths: &AppPaths, id: &str) -> Result<ExitCode, String> {
    let discovery = discover_installed_plugins(paths);
    let plugin = discovery
        .plugins
        .iter()
        .find(|p| p.manifest.id == id)
        .ok_or_else(|| format!("no installed plugin with id '{id}'"))?;
    let values = load_plugin_options(paths, id);
    match plugin.manifest.validate_options(&values) {
        Ok(()) => {
            println!(
                "plugin '{id}' options are valid ({} option(s) set)",
                values.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            eprintln!("invalid: {err}");
            Ok(ExitCode::from(1))
        }
    }
}

fn profile_command(args: &[String]) -> Result<ExitCode, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        eprintln!(
            "usage: linsync-cli profile <list | show ID | validate (ID|PATH) | import PATH | export ID [--output PATH] | delete ID>"
        );
        return Ok(ExitCode::from(2));
    };

    let store = profile_store();
    match subcommand {
        "list" => profile_list(&store),
        "show" => {
            let Some(target) = args.get(1) else {
                return Err("usage: linsync-cli profile show ID".to_owned());
            };
            profile_show(target)
        }
        "validate" => {
            let Some(target) = args.get(1) else {
                return Err("usage: linsync-cli profile validate (ID|PATH)".to_owned());
            };
            profile_validate(target)
        }
        "import" => {
            let Some(path) = args.get(1) else {
                return Err("usage: linsync-cli profile import PATH".to_owned());
            };
            profile_import(&store, path)
        }
        "export" => profile_export(&store, &args[1..]),
        "delete" => {
            let Some(id) = args.get(1) else {
                return Err("usage: linsync-cli profile delete ID".to_owned());
            };
            profile_delete(&store, id)
        }
        other => Err(format!(
            "unknown profile subcommand '{other}'; expected list, show, validate, import, export, or delete"
        )),
    }
}

fn profile_store() -> ProfileStore {
    let paths = AppPaths::from_env();
    ProfileStore::with_builtins(paths.profiles_dir(), paths.active_profile_pointer_file())
}

fn profile_list(store: &ProfileStore) -> Result<ExitCode, String> {
    for p in builtin_profiles() {
        println!("{}\t[built-in]\t{}", p.id, p.name);
    }
    let user_ids = store.list_user_ids().map_err(|err| err.to_string())?;
    for id in user_ids {
        match store.load(&id) {
            Ok(p) => println!("{}\t[user]\t{}", id, p.name),
            Err(err) => println!("{}\t[user, error]\t{err}", id),
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn profile_show(target: &str) -> Result<ExitCode, String> {
    let profile = resolve_profile_arg(target)?;
    let json = serde_json::to_string_pretty(&profile)
        .map_err(|err| format!("failed to serialize profile: {err}"))?;
    println!("{json}");
    Ok(ExitCode::SUCCESS)
}

fn profile_validate(target: &str) -> Result<ExitCode, String> {
    match resolve_profile_arg(target) {
        Ok(p) => match p.validate() {
            Ok(()) => {
                println!("profile {} is valid", p.id);
                Ok(ExitCode::SUCCESS)
            }
            Err(err) => Err(format!("profile {} is invalid: {err}", p.id)),
        },
        Err(err) => Err(err),
    }
}

fn profile_import(store: &ProfileStore, src: &str) -> Result<ExitCode, String> {
    let bytes = fs::read(src).map_err(|err| format!("failed to read '{src}': {err}"))?;
    let profile: CompareProfile =
        serde_json::from_slice(&bytes).map_err(|err| format!("failed to parse '{src}': {err}"))?;
    profile
        .validate()
        .map_err(|err| format!("profile in '{src}' is invalid: {err}"))?;
    store
        .save(&profile)
        .map_err(|err| format!("failed to save profile: {err}"))?;
    println!("imported profile {} as user profile", profile.id);
    Ok(ExitCode::SUCCESS)
}

fn profile_export(store: &ProfileStore, args: &[String]) -> Result<ExitCode, String> {
    let mut target: Option<&str> = None;
    let mut output: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--output" => {
                let Some(v) = args.get(i + 1) else {
                    return Err("--output requires a path".to_owned());
                };
                output = Some(v.as_str());
                i += 2;
            }
            value if !value.starts_with("--") && target.is_none() => {
                target = Some(value);
                i += 1;
            }
            other => return Err(format!("unexpected profile export argument '{other}'")),
        }
    }
    let Some(target) = target else {
        return Err("usage: linsync-cli profile export ID [--output PATH]".to_owned());
    };
    let profile = resolve_profile_id_only(store, target)?;
    let bytes = serde_json::to_vec_pretty(&profile)
        .map_err(|err| format!("failed to serialize profile: {err}"))?;
    match output {
        Some(path) => {
            fs::write(path, &bytes).map_err(|err| format!("failed to write '{path}': {err}"))?;
            println!("exported profile {} to {path}", profile.id);
        }
        None => {
            std::io::Write::write_all(&mut std::io::stdout(), &bytes)
                .map_err(|err| format!("failed to write to stdout: {err}"))?;
            println!();
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn profile_delete(store: &ProfileStore, id_str: &str) -> Result<ExitCode, String> {
    let id = ProfileId::new(id_str.to_owned())
        .map_err(|err| format!("invalid profile id '{id_str}': {err}"))?;
    match store.delete(&id) {
        Ok(()) => {
            println!("deleted profile {id}");
            Ok(ExitCode::SUCCESS)
        }
        Err(ProfileStoreError::NotFound(_)) => {
            // Maybe the user is trying to delete a built-in; report a
            // clearer error in that case.
            if find_builtin(&id).is_some() {
                Err(format!("profile {id} is a built-in and cannot be deleted"))
            } else {
                Err(format!("no user profile named {id}"))
            }
        }
        Err(err) => Err(err.to_string()),
    }
}

/// Resolve a `--profile X` argument to a [`CompareProfile`]. Accepts
/// a built-in id, a user-store id, or a path to a JSON file.
fn resolve_profile_arg(value: &str) -> Result<CompareProfile, String> {
    // Heuristic: anything containing '/' or ending in `.json` is a file
    // path. Otherwise treat it as a profile id.
    let looks_like_path = value.contains('/') || value.ends_with(".json");
    if looks_like_path {
        let bytes = fs::read(value)
            .map_err(|err| format!("failed to read profile file '{value}': {err}"))?;
        let profile: CompareProfile = serde_json::from_slice(&bytes)
            .map_err(|err| format!("failed to parse profile file '{value}': {err}"))?;
        profile
            .validate()
            .map_err(|err| format!("profile in '{value}' is invalid: {err}"))?;
        return Ok(profile);
    }
    let id = ProfileId::new(value.to_owned())
        .map_err(|err| format!("invalid profile id '{value}': {err}"))?;
    if let Some(p) = find_builtin(&id) {
        return Ok(p);
    }
    let store = profile_store();
    match store.load(&id) {
        Ok(p) => Ok(p),
        Err(ProfileStoreError::NotFound(_)) => {
            let known: Vec<String> = builtin_profiles()
                .into_iter()
                .map(|p| p.id.to_string())
                .collect();
            Err(format!(
                "no profile named '{value}'. Known built-ins: {}. Use `linsync-cli profile list` for the full list.",
                known.join(", ")
            ))
        }
        Err(err) => Err(format!("failed to load user profile '{value}': {err}")),
    }
}

fn resolve_profile_id_only(store: &ProfileStore, value: &str) -> Result<CompareProfile, String> {
    let id = ProfileId::new(value.to_owned())
        .map_err(|err| format!("invalid profile id '{value}': {err}"))?;
    if let Some(p) = find_builtin(&id) {
        return Ok(p);
    }
    store
        .load(&id)
        .map_err(|err| format!("failed to load profile '{value}': {err}"))
}

fn filter_command(args: &[String]) -> Result<ExitCode, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        eprintln!(
            "usage: linsync-cli filter <validate RULE | validate-file PATH | list | migrate INPUT [--out OUTPUT | --in-place]>"
        );
        return Ok(ExitCode::from(2));
    };

    match subcommand {
        "validate" => {
            let Some(rule) = args.get(1) else {
                return Err("usage: linsync-cli filter validate RULE".to_owned());
            };
            print_filter_validation(rule)
        }
        "validate-file" => {
            let Some(path) = args.get(1) else {
                return Err("usage: linsync-cli filter validate-file PATH".to_owned());
            };
            let body = fs::read_to_string(path)
                .map_err(|err| format!("failed to read filter file '{path}': {err}"))?;
            print_filter_validation(&body)
        }
        "list" => {
            let store = FilterStore::new(AppPaths::from_env().filters_file());
            let filters = store.load_or_default().map_err(|err| err.to_string())?;
            if filters.filters.is_empty() {
                println!("(no saved filters)");
            } else {
                for filter in &filters.filters {
                    println!(
                        "{}: {} rule(s)",
                        filter.name.as_deref().unwrap_or("(unnamed)"),
                        filter.rules.len()
                    );
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        "migrate" => filter_migrate_command(&args[1..]),
        other => Err(format!(
            "unknown filter subcommand '{other}'; expected validate, validate-file, list, or migrate"
        )),
    }
}

fn filter_migrate_command(args: &[String]) -> Result<ExitCode, String> {
    let mut input: Option<&str> = None;
    let mut out: Option<&str> = None;
    let mut in_place = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("filter migrate --out requires a file path".to_owned());
                };
                out = Some(value.as_str());
                index += 2;
            }
            "--in-place" => {
                in_place = true;
                index += 1;
            }
            value if value.starts_with("--") => {
                return Err(format!("unknown filter migrate flag '{value}'"));
            }
            value => {
                if input.is_some() {
                    return Err(
                        "usage: linsync-cli filter migrate INPUT [--out OUTPUT | --in-place]"
                            .to_owned(),
                    );
                }
                input = Some(value);
                index += 1;
            }
        }
    }

    let Some(input) = input else {
        return Err(
            "usage: linsync-cli filter migrate INPUT [--out OUTPUT | --in-place]".to_owned(),
        );
    };

    if out.is_some() && in_place {
        return Err(
            "filter migrate: --out and --in-place cannot be combined; use one or the other"
                .to_owned(),
        );
    }

    let text = fs::read_to_string(input)
        .map_err(|err| format!("failed to read filter file '{input}': {err}"))?;
    let result = linsync_core::migrate_filter_text(&text);

    // Print any warnings to stderr so they don't pollute stdout output.
    for warning in &result.warnings {
        eprintln!("warning: {warning}");
    }

    if let Some(out_path) = out {
        fs::write(out_path, &result.migrated)
            .map_err(|err| format!("failed to write output file '{out_path}': {err}"))?;
    } else if in_place {
        // Atomic replace: write to temp file then rename.
        let temp_path = format!("{input}.migrate-tmp");
        fs::write(&temp_path, &result.migrated)
            .map_err(|err| format!("failed to write temp file '{temp_path}': {err}"))?;
        fs::rename(&temp_path, input).map_err(|err| {
            let _ = fs::remove_file(&temp_path);
            format!("failed to replace '{input}' with migrated content: {err}")
        })?;
    } else {
        print!("{}", result.migrated);
    }

    Ok(ExitCode::SUCCESS)
}

fn print_filter_validation(body: &str) -> Result<ExitCode, String> {
    match FileFilter::parse(body) {
        Ok(filter) => {
            println!(
                "ok: parsed filter '{}' with {} rule(s)",
                filter.name.as_deref().unwrap_or("(unnamed)"),
                filter.rules.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        Err(err) => {
            let prefix = match err.kind {
                FilterParseErrorKind::UnsupportedLegacyExpression => "migration",
                FilterParseErrorKind::UnsupportedWindowsMetadata => "migration",
                _ => "error",
            };
            eprintln!("{prefix}: line {}: {}", err.line, err.message);
            Ok(ExitCode::from(if err.is_migration_hint() { 3 } else { 2 }))
        }
    }
}

fn folders_command(args: &[String]) -> Result<ExitCode, String> {
    let folder_args = split_folder_args(args)?;

    if folder_args.paths.len() != 2 {
        return Err(
            "usage: linsync-cli folders [--recursive] [--profile NAME-OR-PATH] [--method METHOD] [--timestamp-tolerance-ms MS] [--symlinks target|follow|special] [--large-file-threshold-bytes BYTES] [--large-file-method quick|binary] [--hash-algorithm blake3|sha256|crc32] [--compare-permissions] [--compare-ownership] [--dry-run] [--exclude-generated] [--filter RULE] [--filter-name NAME] [--case-insensitive-filter] [--hide-skipped] [--state STATE] [--types LIST] [--search SUBSTR] [--sort KEY] [--desc] [--group-by GROUP] [--offset N] [--limit N] [--json|--csv|--count|--quiet] LEFT RIGHT"
                .to_owned(),
        );
    }

    let options = folder_args.compare_options();
    let result = compare_folders(
        PathBuf::from(&folder_args.paths[0]).as_path(),
        PathBuf::from(&folder_args.paths[1]).as_path(),
        &options,
    )
    .map_err(|err| err.to_string())?;

    let summary = &result.summary;
    let differences = summary.different_count + summary.one_sided_count + summary.errors_count;
    let query = folder_args.query();
    let entry_filter = query.state;
    let page = result.query(&query);
    let filtered_entries: Vec<&FolderEntryDiff> = page
        .groups
        .iter()
        .flat_map(|group| group.entries.iter().copied())
        .collect();
    let count = if folder_args.query_is_restricting() {
        page.total_matched
    } else {
        differences
    };

    match folder_args.output {
        FolderOutput::Structured(OutputMode::Text) => {
            println!(
                "compared={} skipped={} identical={} different={} one_sided={} left_only={} right_only={} errors={} aborted={} method_downgrades={} filtered={} returned={} offset={} has_more={} elapsed_ms={} status={}",
                summary.compared_count,
                summary.skipped_count,
                summary.identical_count,
                summary.different_count,
                summary.one_sided_count,
                summary.left_only_count,
                summary.right_only_count,
                summary.errors_count,
                summary.aborted_count,
                summary.method_downgrade_count,
                page.total_matched,
                page.returned,
                page.offset,
                page.has_more,
                summary.elapsed.as_millis(),
                folder_status(summary.status)
            );
            if folder_args.dry_run {
                let selected: Vec<PathBuf> = result
                    .entries
                    .iter()
                    .filter(|e| {
                        matches!(
                            e.state,
                            FolderEntryState::Different
                                | FolderEntryState::LeftOnly
                                | FolderEntryState::RightOnly
                        )
                    })
                    .map(|e| e.relative_path.clone())
                    .collect();
                let mut plan = plan_folder_operation(
                    &result,
                    linsync_core::FolderOperationKind::CopyLeftToRight,
                    &selected,
                );
                let left_path = Path::new(&folder_args.paths[0]);
                let right_path = Path::new(&folder_args.paths[1]);
                let _ = assess_operation_risks(&mut plan, left_path, right_path);
                let risk = plan.risk_summary();
                println!(
                    "dry_run: {} operations ({} overwrites, {} deletes, {} high-risk warnings)",
                    risk.total_operations,
                    risk.overwrite_count,
                    risk.delete_count,
                    risk.high_risk_count,
                );
                for warning in &risk.warnings {
                    println!(
                        "  warning: {:?}: {} ({})",
                        warning.kind,
                        warning.message,
                        warning.relative_path.display()
                    );
                }
            }
        }
        FolderOutput::Structured(OutputMode::Json) => {
            let entries = filtered_entries
                .iter()
                .map(|entry| {
                    serde_json::json!({
                        "path": entry.relative_path.display().to_string(),
                        "name": entry.name,
                        "extension": entry.extension,
                        "state": folder_state(entry.state),
                        "left_size": entry.left_size,
                        "right_size": entry.right_size,
                        "left_modified_ms": entry.left_modified.and_then(system_time_millis),
                        "right_modified_ms": entry.right_modified.and_then(system_time_millis),
                        "type": entry.entry_type.as_str(),
                        "effective_method": entry.effective_method.map(CompareMethod::as_str),
                        "method_note": entry.method_note,
                        "error": entry.error,
                    })
                })
                .collect::<Vec<_>>();
            let risk_metadata = if folder_args.dry_run {
                let selected: Vec<PathBuf> = result
                    .entries
                    .iter()
                    .filter(|e| {
                        matches!(
                            e.state,
                            FolderEntryState::Different
                                | FolderEntryState::LeftOnly
                                | FolderEntryState::RightOnly
                        )
                    })
                    .map(|e| e.relative_path.clone())
                    .collect();
                let mut plan = plan_folder_operation(
                    &result,
                    linsync_core::FolderOperationKind::CopyLeftToRight,
                    &selected,
                );
                let left_path = Path::new(&folder_args.paths[0]);
                let right_path = Path::new(&folder_args.paths[1]);
                let _ = assess_operation_risks(&mut plan, left_path, right_path);
                let risk = plan.risk_summary();
                Some(serde_json::json!({
                    "total_operations": risk.total_operations,
                    "overwrite_count": risk.overwrite_count,
                    "delete_count": risk.delete_count,
                    "high_risk_count": risk.high_risk_count,
                    "warnings": risk.warnings.iter().map(|w| serde_json::json!({
                        "relative_path": w.relative_path.display().to_string(),
                        "kind": format!("{:?}", w.kind),
                        "message": w.message,
                    })).collect::<Vec<_>>(),
                }))
            } else {
                None
            };
            let mut output = serde_json::json!({
                "equal": result.is_equal(),
                "compared": summary.compared_count,
                "skipped": summary.skipped_count,
                "identical": summary.identical_count,
                "different": summary.different_count,
                "one_sided": summary.one_sided_count,
                "left_only": summary.left_only_count,
                "right_only": summary.right_only_count,
                "errors": summary.errors_count,
                "aborted": summary.aborted_count,
                "method_downgrades": summary.method_downgrade_count,
                "filtered": page.total_matched,
                "returned": page.returned,
                "offset": page.offset,
                "has_more": page.has_more,
                "elapsed_ms": summary.elapsed.as_millis(),
                "status": folder_status(summary.status),
                "options": folder_options_metadata_json(
                    &options,
                    folder_args.effective_profile.as_deref(),
                    entry_filter,
                ),
                "entries": entries,
            });
            if let Some(profile_id) = folder_args.effective_profile.as_deref() {
                output["profile"] = serde_json::json!(profile_id);
            }
            if let Some(risk) = risk_metadata {
                output["risk"] = risk;
            }
            println!("{output}");
        }
        FolderOutput::Structured(OutputMode::Count) => println!("{count}"),
        FolderOutput::Structured(OutputMode::Quiet) => {}
        FolderOutput::Csv => {
            println!(
                "path,state,left_size,right_size,name,extension,type,left_modified_ms,right_modified_ms,effective_method,method_note,error"
            );
            for entry in filtered_entries {
                println!(
                    "{},{},{},{},{},{},{},{},{},{},{},{}",
                    csv_escape(&entry.relative_path.display().to_string()),
                    folder_state(entry.state),
                    entry
                        .left_size
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    entry
                        .right_size
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    csv_escape(&entry.name),
                    csv_escape(entry.extension.as_deref().unwrap_or("")),
                    entry.entry_type.as_str(),
                    entry
                        .left_modified
                        .and_then(system_time_millis)
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    entry
                        .right_modified
                        .and_then(system_time_millis)
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    entry
                        .effective_method
                        .map(CompareMethod::as_str)
                        .unwrap_or(""),
                    csv_escape(entry.method_note.as_deref().unwrap_or("")),
                    csv_escape(entry.error.as_deref().unwrap_or(""))
                );
            }
        }
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn man_command(args: &[String]) -> Result<ExitCode, String> {
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

fn launch_command(args: &[String]) -> Result<ExitCode, String> {
    let mut wait = false;
    let mut gui_args = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--wait" => {
                wait = true;
                index += 1;
            }
            "--" => {
                gui_args.extend(args[index + 1..].iter().cloned());
                break;
            }
            value => {
                gui_args.push(value.to_owned());
                index += 1;
            }
        }
    }

    let gui = resolve_gui_binary();
    let mut command = Command::new(&gui);
    command.args(&gui_args);

    if wait {
        let status = command
            .status()
            .map_err(|err| format!("failed to launch GUI '{}': {err}", gui.display()))?;
        return Ok(if status.success() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(2)
        });
    }

    command
        .spawn()
        .map_err(|err| format!("failed to launch GUI '{}': {err}", gui.display()))?;
    Ok(ExitCode::SUCCESS)
}

fn resolve_gui_binary() -> PathBuf {
    if let Some(value) = env::var_os("LINSYNC_GUI") {
        return PathBuf::from(value);
    }

    if let Ok(current_exe) = env::current_exe()
        && let Some(directory) = current_exe.parent()
    {
        let sibling = directory.join("linsync");
        if sibling.exists() {
            return sibling;
        }
    }

    PathBuf::from("linsync")
}

fn open_external_command(args: &[String]) -> Result<ExitCode, String> {
    let external_args = split_open_external_args(args)?;
    if external_args.paths.is_empty() {
        return Err(
            "usage: linsync-cli open-external [--wait] [--preset PRESET] PATH...".to_owned(),
        );
    }

    let opener = resolve_external_opener(external_args.preset)?;
    let mut exit_code = ExitCode::SUCCESS;

    for path in external_args.paths {
        let mut command = Command::new(&opener.program);
        command.args(&opener.args);
        command.arg(path);
        if external_args.wait {
            let status = command.status().map_err(|err| {
                format!(
                    "failed to open '{}' with '{}': {err}",
                    path,
                    opener.program.display()
                )
            })?;
            if !status.success() {
                // Exit code 1 means "differences found" per the documented
                // contract. A failed/signalled external opener is a runtime
                // error and must surface as 2.
                exit_code = ExitCode::from(2);
            }
        } else {
            command.spawn().map_err(|err| {
                format!(
                    "failed to open '{}' with '{}': {err}",
                    path,
                    opener.program.display()
                )
            })?;
        }
    }

    Ok(exit_code)
}

#[derive(Debug, Clone)]
struct OpenExternalArgs<'a> {
    wait: bool,
    preset: Option<&'a str>,
    paths: Vec<&'a str>,
}

#[derive(Debug, Clone)]
struct CommandTemplate {
    program: PathBuf,
    args: Vec<String>,
}

fn split_open_external_args(args: &[String]) -> Result<OpenExternalArgs<'_>, String> {
    let mut wait = false;
    let mut preset = None;
    let mut paths = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--wait" => {
                wait = true;
                index += 1;
            }
            "--preset" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("open-external --preset requires a preset name".to_owned());
                };
                preset = Some(value.as_str());
                index += 2;
            }
            value if let Some(value) = value.strip_prefix("--preset=") => {
                if value.is_empty() {
                    return Err("open-external --preset requires a preset name".to_owned());
                }
                preset = Some(value);
                index += 1;
            }
            value => {
                paths.push(value);
                index += 1;
            }
        }
    }

    Ok(OpenExternalArgs {
        wait,
        preset,
        paths,
    })
}

fn resolve_external_opener(preset: Option<&str>) -> Result<CommandTemplate, String> {
    if let Some(preset) = preset {
        return external_opener_preset(preset);
    }

    Ok(CommandTemplate {
        program: env::var_os("LINSYNC_OPEN")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("xdg-open")),
        args: Vec::new(),
    })
}

fn external_opener_preset(preset: &str) -> Result<CommandTemplate, String> {
    let template = match preset {
        "xdg-open" => CommandTemplate {
            program: PathBuf::from("xdg-open"),
            args: Vec::new(),
        },
        "kate" => CommandTemplate {
            program: PathBuf::from("kate"),
            args: Vec::new(),
        },
        "kwrite" => CommandTemplate {
            program: PathBuf::from("kwrite"),
            args: Vec::new(),
        },
        "vscode" => CommandTemplate {
            program: PathBuf::from("code"),
            args: Vec::new(),
        },
        "vscodium" => CommandTemplate {
            program: PathBuf::from("codium"),
            args: Vec::new(),
        },
        "gnome-text-editor" => CommandTemplate {
            program: PathBuf::from("gnome-text-editor"),
            args: Vec::new(),
        },
        "sublime" => CommandTemplate {
            program: PathBuf::from("subl"),
            args: Vec::new(),
        },
        "nvim-terminal" => CommandTemplate {
            program: env::var_os("LINSYNC_TERMINAL")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("x-terminal-emulator")),
            args: vec!["-e".to_owned(), "nvim".to_owned()],
        },
        "jetbrains-idea" => jetbrains_template("idea"),
        "jetbrains-pycharm" => jetbrains_template("pycharm"),
        "jetbrains-webstorm" => jetbrains_template("webstorm"),
        "jetbrains-clion" => jetbrains_template("clion"),
        "jetbrains-rider" => jetbrains_template("rider"),
        "jetbrains-goland" => jetbrains_template("goland"),
        "jetbrains-phpstorm" => jetbrains_template("phpstorm"),
        "jetbrains-rubymine" => jetbrains_template("rubymine"),
        "jetbrains-datagrip" => jetbrains_template("datagrip"),
        _ => {
            return Err(format!(
                "unknown external editor preset '{preset}'; expected one of: {}",
                OPEN_EXTERNAL_PRESETS.join(", ")
            ));
        }
    };

    Ok(template)
}

fn jetbrains_template(program: &str) -> CommandTemplate {
    CommandTemplate {
        program: PathBuf::from(program),
        args: Vec::new(),
    }
}

fn reveal_command(args: &[String]) -> Result<ExitCode, String> {
    let mut wait = false;
    let mut paths = Vec::new();

    for arg in args {
        if arg == "--wait" {
            wait = true;
        } else {
            paths.push(arg);
        }
    }

    if paths.is_empty() {
        return Err("usage: linsync-cli reveal [--wait] PATH...".to_owned());
    }

    let configured_revealer = env::var_os("LINSYNC_REVEAL").map(PathBuf::from);
    let mut exit_code = ExitCode::SUCCESS;

    for path in paths {
        let code = if let Some(revealer) = configured_revealer.as_ref() {
            reveal_with_command(path, revealer, wait)?
        } else if reveal_with_file_manager1(path)? {
            ExitCode::SUCCESS
        } else {
            reveal_containing_folder(path, wait)?
        };
        if code != ExitCode::SUCCESS {
            exit_code = code;
        }
    }

    Ok(exit_code)
}

fn reveal_with_command(path: &str, revealer: &Path, wait: bool) -> Result<ExitCode, String> {
    let target = PathBuf::from(path);
    let mut command = Command::new(revealer);
    command.arg(&target);
    if wait {
        let status = command.status().map_err(|err| {
            format!(
                "failed to reveal '{}' with '{}': {err}",
                target.display(),
                revealer.display()
            )
        })?;
        Ok(if status.success() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(2)
        })
    } else {
        command.spawn().map_err(|err| {
            format!(
                "failed to reveal '{}' with '{}': {err}",
                target.display(),
                revealer.display()
            )
        })?;
        Ok(ExitCode::SUCCESS)
    }
}

fn reveal_with_file_manager1(path: &str) -> Result<bool, String> {
    let uri = file_uri(Path::new(path))?;
    let status = Command::new("dbus-send")
        .args([
            "--session",
            "--dest=org.freedesktop.FileManager1",
            "--type=method_call",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
            &format!("array:string:{uri}"),
            "string:",
        ])
        .status();

    Ok(status.is_ok_and(|status| status.success()))
}

fn reveal_containing_folder(path: &str, wait: bool) -> Result<ExitCode, String> {
    let target = containing_folder(path);
    let opener = PathBuf::from("xdg-open");
    let mut command = Command::new(&opener);
    command.arg(&target);
    if wait {
        let status = command.status().map_err(|err| {
            format!(
                "failed to reveal '{}' with '{}': {err}",
                target.display(),
                opener.display()
            )
        })?;
        Ok(if status.success() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(2)
        })
    } else {
        command.spawn().map_err(|err| {
            format!(
                "failed to reveal '{}' with '{}': {err}",
                target.display(),
                opener.display()
            )
        })?;
        Ok(ExitCode::SUCCESS)
    }
}

fn file_uri(path: &Path) -> Result<String, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|err| format!("cannot resolve current directory for reveal: {err}"))?
            .join(path)
    };
    Ok(path_to_file_uri(&absolute))
}

fn path_to_file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.as_os_str().as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'.' | b'_' | b'~' => {
                uri.push(*byte as char)
            }
            byte => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

fn containing_folder(path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_dir() {
        return path.to_path_buf();
    }

    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn patch_command(args: &[String]) -> Result<ExitCode, String> {
    let patch_args = split_patch_args(args)?;
    if patch_args.paths.len() != 2 {
        return Err(
            "usage: linsync-cli patch LEFT RIGHT [--format unified|context|normal] [--context LINES] [--preview|--output FILE]"
                .to_owned(),
        );
    }

    let left = PathBuf::from(&patch_args.paths[0]);
    let right = PathBuf::from(&patch_args.paths[1]);
    if left.is_dir() || right.is_dir() {
        if !(left.is_dir() && right.is_dir()) {
            return Err("patch paths must both be files or both be directories".to_owned());
        }
        return patch_folder_command(&left, &right, patch_args);
    }

    let result = compare_text_files(
        left.as_path(),
        right.as_path(),
        &TextCompareOptions::default(),
    )
    .map_err(|err| err.to_string())?;
    let patch = render_text_patch(&result, patch_args.format, patch_args.context);

    if patch_args.preview {
        print!("{patch}");
    } else if let Some(output) = patch_args.output {
        fs::write(output, patch).map_err(|err| err.to_string())?;
    } else {
        print!("{patch}");
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn patch_folder_command(
    left: &Path,
    right: &Path,
    patch_args: PatchArgs,
) -> Result<ExitCode, String> {
    let result = compare_folders(left, right, &FolderCompareOptions::default())
        .map_err(|err| err.to_string())?;
    let mut patch = String::new();

    for entry in &result.entries {
        if entry.is_dir
            || matches!(
                entry.state,
                FolderEntryState::Identical | FolderEntryState::Skipped | FolderEntryState::Aborted
            )
        {
            continue;
        }
        if entry.state == FolderEntryState::Error {
            return Err(format!(
                "cannot generate patch for '{}': {}",
                entry.relative_path.display(),
                entry.error.as_deref().unwrap_or("folder compare error")
            ));
        }

        let left_path = left.join(&entry.relative_path);
        let right_path = right.join(&entry.relative_path);
        let text_result = match entry.state {
            FolderEntryState::Different => {
                let left_text = read_representable_text(&left_path)?;
                let right_text = read_representable_text(&right_path)?;
                compare_text(
                    &left_path.display().to_string(),
                    &left_text,
                    &right_path.display().to_string(),
                    &right_text,
                    &TextCompareOptions::default(),
                )
            }
            FolderEntryState::LeftOnly => {
                let left_text = read_representable_text(&left_path)?;
                compare_text(
                    &left_path.display().to_string(),
                    &left_text,
                    "/dev/null",
                    "",
                    &TextCompareOptions::default(),
                )
            }
            FolderEntryState::RightOnly => {
                let right_text = read_representable_text(&right_path)?;
                compare_text(
                    "/dev/null",
                    "",
                    &right_path.display().to_string(),
                    &right_text,
                    &TextCompareOptions::default(),
                )
            }
            FolderEntryState::Identical
            | FolderEntryState::Skipped
            | FolderEntryState::Error
            | FolderEntryState::Aborted => continue,
        };

        patch.push_str(&render_text_patch(
            &text_result,
            patch_args.format,
            patch_args.context,
        ));
    }

    if patch_args.preview {
        print!("{patch}");
    } else if let Some(output) = patch_args.output {
        fs::write(output, patch).map_err(|err| err.to_string())?;
    } else {
        print!("{patch}");
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn render_text_patch(
    result: &linsync_core::TextCompareResult,
    format: PatchFormat,
    context: usize,
) -> String {
    match format {
        PatchFormat::Unified => result.to_unified_diff(context),
        PatchFormat::Context => result.to_context_diff(context),
        PatchFormat::Normal => result.to_normal_diff(),
    }
}

fn read_representable_text(path: &Path) -> Result<String, String> {
    read_utf8_text_for_export(path, "folder patch")
}

fn read_nested_report_text(path: &Path) -> Result<String, String> {
    read_utf8_text_for_export(path, "nested file report")
}

fn read_utf8_text_for_export(path: &Path, purpose: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    if is_likely_binary(&bytes) {
        return Err(format!(
            "{purpose} cannot represent binary file '{}'",
            path.display()
        ));
    }
    String::from_utf8(bytes).map_err(|err| {
        format!(
            "{purpose} requires UTF-8 text for '{}': {err}",
            path.display()
        )
    })
}

fn hex_command(args: &[String]) -> Result<ExitCode, String> {
    let mut bytes_per_row = 16;
    let mut metadata_only = false;
    let mut output = OutputMode::Text;
    let mut paths = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                set_output_mode(&mut output, OutputMode::Json, "--json")?;
                index += 1;
            }
            "--count" => {
                set_output_mode(&mut output, OutputMode::Count, "--count")?;
                index += 1;
            }
            "--quiet" | "-q" => {
                set_output_mode(&mut output, OutputMode::Quiet, "--quiet")?;
                index += 1;
            }
            "--width" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--width requires a byte count".to_owned());
                };
                bytes_per_row = value
                    .parse::<usize>()
                    .map_err(|_| "--width must be a positive integer".to_owned())?
                    .max(1);
                index += 2;
            }
            "--metadata-only" => {
                metadata_only = true;
                index += 1;
            }
            value => {
                paths.push(value.to_owned());
                index += 1;
            }
        }
    }

    if paths.len() != 2 {
        return Err(
            "usage: linsync-cli hex [--width BYTES] [--metadata-only] LEFT RIGHT".to_owned(),
        );
    }

    let result = compare_binary_files(
        PathBuf::from(&paths[0]).as_path(),
        PathBuf::from(&paths[1]).as_path(),
        &BinaryCompareOptions {
            bytes_per_row,
            compare_content: !metadata_only,
            compare_metadata: metadata_only,
        },
    )
    .map_err(|err| err.to_string())?;
    let difference_count = result.differences.len() + result.metadata_differences.len();

    match output {
        OutputMode::Text => {
            println!(
                "left_len={} right_len={} differing_bytes={} metadata_differences={} content_compared={}",
                result.left_len,
                result.right_len,
                result.differences.len(),
                result.metadata_differences.len(),
                result.content_compared
            );
            if !result.metadata_differences.is_empty() {
                println!(
                    "metadata: {}",
                    result
                        .metadata_differences
                        .iter()
                        .map(|difference| difference.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }
            for row in result.rows.iter().filter(|row| row.has_difference).take(12) {
                println!(
                    "{:08X} | {:<48} | {:<48} | {} | {}",
                    row.offset, row.left_hex, row.right_hex, row.left_ascii, row.right_ascii
                );
            }
        }
        OutputMode::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "equal": result.is_equal(),
                    "left_len": result.left_len,
                    "right_len": result.right_len,
                    "differences": difference_count,
                    "differing_bytes": result.differences.len(),
                    "metadata_differences": result.metadata_differences.iter().map(|difference| difference.as_str()).collect::<Vec<_>>(),
                    "content_compared": result.content_compared,
                    "metadata": result.metadata.as_ref().map(|metadata| {
                        serde_json::json!({
                            "left": {
                                "len": metadata.left.len,
                                "modified_ms": metadata.left.modified.and_then(system_time_millis),
                                "readonly": metadata.left.readonly,
                            },
                            "right": {
                                "len": metadata.right.len,
                                "modified_ms": metadata.right.modified.and_then(system_time_millis),
                                "readonly": metadata.right.readonly,
                            },
                        })
                    }),
                })
            );
        }
        OutputMode::Count => println!("{difference_count}"),
        OutputMode::Quiet => {}
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn report_command(args: &[String]) -> Result<ExitCode, String> {
    let report_args = split_report_args(args)?;
    if report_args.paths.len() != 2 {
        return Err(
            "usage: linsync-cli report LEFT RIGHT --output FILE [--context LINES] [--columns COLS] [--tree-state expanded|collapsed] [--nested-file-reports]"
                .to_owned(),
        );
    }

    let output = report_args
        .output
        .ok_or_else(|| "report requires --output FILE".to_owned())?;
    let left = PathBuf::from(&report_args.paths[0]);
    let right = PathBuf::from(&report_args.paths[1]);

    if left.is_dir() || right.is_dir() {
        if !(left.is_dir() && right.is_dir()) {
            return Err("report paths must both be files or both be directories".to_owned());
        }

        let result = compare_folders(&left, &right, &FolderCompareOptions::default())
            .map_err(|err| err.to_string())?;
        fs::write(
            output,
            folder_html_report(
                &result,
                &report_args.columns,
                report_args.tree_state,
                report_args.nested_file_reports,
                report_args.context,
            ),
        )
        .map_err(|err| err.to_string())?;
        return Ok(if result.is_equal() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        });
    }

    let result = compare_text_files(
        left.as_path(),
        right.as_path(),
        &TextCompareOptions::default(),
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        output,
        result.to_html_report_with_context(report_args.context),
    )
    .map_err(|err| err.to_string())?;

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn table_command(args: &[String]) -> Result<ExitCode, String> {
    let mut delimiter = ',';
    let mut has_header = false;
    let mut output = OutputMode::Text;
    let mut paths = Vec::new();
    let mut index = 0;
    let mut table_opts = TableCompareOptions::default();

    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                set_output_mode(&mut output, OutputMode::Json, "--json")?;
                index += 1;
            }
            "--count" => {
                set_output_mode(&mut output, OutputMode::Count, "--count")?;
                index += 1;
            }
            "--quiet" | "-q" => {
                set_output_mode(&mut output, OutputMode::Quiet, "--quiet")?;
                index += 1;
            }
            "--delimiter" | "-d" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--delimiter requires a character".to_owned());
                };
                delimiter = parse_delimiter(value)?;
                index += 2;
            }
            "--tsv" => {
                delimiter = '\t';
                index += 1;
            }
            "--header" => {
                has_header = true;
                index += 1;
            }
            "--table-quote" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--table-quote requires a character".to_owned());
                };
                table_opts.quote_char = Some(parse_single_char(value, "--table-quote")?);
                index += 2;
            }
            "--table-escape" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--table-escape requires a character".to_owned());
                };
                table_opts.escape_char = Some(parse_single_char(value, "--table-escape")?);
                index += 2;
            }
            "--table-comment" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--table-comment requires a prefix string".to_owned());
                };
                table_opts.comment_prefix = Some(value.clone());
                index += 2;
            }
            "--table-skip-blank" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--table-skip-blank requires true/false".to_owned());
                };
                table_opts.skip_blank_rows = parse_cli_bool(value, "--table-skip-blank")?;
                index += 2;
            }
            "--numeric-tolerance" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--numeric-tolerance requires a number".to_owned());
                };
                let tolerance = value
                    .parse::<f64>()
                    .map_err(|_| format!("--numeric-tolerance requires a number, got '{value}'"))?;
                if !tolerance.is_finite() || tolerance < 0.0 {
                    return Err(
                        "--numeric-tolerance requires a finite non-negative number".to_owned()
                    );
                }
                table_opts.numeric_tolerance = Some(tolerance);
                index += 2;
            }
            value => {
                paths.push(value.to_owned());
                index += 1;
            }
        }
    }

    if paths.len() != 2 {
        return Err(
            "usage: linsync-cli table [--header] [--delimiter CHAR|--tsv] [--table-quote CHAR] [--table-escape CHAR] [--table-comment PREFIX] [--table-skip-blank BOOL] [--numeric-tolerance FLOAT] LEFT RIGHT".to_owned(),
        );
    }

    table_opts.delimiter = delimiter;
    table_opts.has_header = has_header;

    let result = compare_table_files(
        PathBuf::from(&paths[0]).as_path(),
        PathBuf::from(&paths[1]).as_path(),
        &table_opts,
    )
    .map_err(|err| err.to_string())?;

    match output {
        OutputMode::Text => {
            println!(
                "rows={} changed_cells={}",
                result.rows.len(),
                result.changed_cells
            );
            for row in result.rows.iter().filter(|row| row.has_difference).take(20) {
                for cell in row
                    .cells
                    .iter()
                    .filter(|cell| cell.state != TableCellState::Equal)
                {
                    println!(
                        "row={} col={} left={} right={}",
                        row.row_index + 1,
                        cell.column_index + 1,
                        cell.left.as_deref().unwrap_or(""),
                        cell.right.as_deref().unwrap_or("")
                    );
                }
            }
        }
        OutputMode::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "equal": result.is_equal(),
                    "rows": result.rows.len(),
                    "changed_cells": result.changed_cells,
                })
            );
        }
        OutputMode::Count => println!("{}", result.changed_cells),
        OutputMode::Quiet => {}
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderReportColumn {
    Name,
    Path,
    State,
    Extension,
    LeftSize,
    RightSize,
    LeftModified,
    RightModified,
    Type,
    Method,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReportTreeState {
    Expanded,
    Collapsed,
}

impl ReportTreeState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Expanded => "expanded",
            Self::Collapsed => "collapsed",
        }
    }

    fn details_open_attr(self) -> &'static str {
        match self {
            Self::Expanded => " open",
            Self::Collapsed => "",
        }
    }
}

fn parse_report_tree_state(value: &str) -> Result<ReportTreeState, String> {
    match value {
        "expanded" => Ok(ReportTreeState::Expanded),
        "collapsed" => Ok(ReportTreeState::Collapsed),
        other => Err(format!(
            "unknown report tree state '{other}'; expected expanded or collapsed"
        )),
    }
}

impl FolderReportColumn {
    fn default_columns() -> Vec<Self> {
        vec![
            Self::Name,
            Self::Path,
            Self::State,
            Self::Extension,
            Self::LeftSize,
            Self::RightSize,
            Self::LeftModified,
            Self::RightModified,
            Self::Type,
            Self::Method,
            Self::Error,
        ]
    }

    fn header(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Path => "Path",
            Self::State => "State",
            Self::Extension => "Extension",
            Self::LeftSize => "Left Size",
            Self::RightSize => "Right Size",
            Self::LeftModified => "Left Modified",
            Self::RightModified => "Right Modified",
            Self::Type => "Type",
            Self::Method => "Compare Result",
            Self::Error => "Error",
        }
    }
}

fn parse_folder_report_columns(value: &str) -> Result<Vec<FolderReportColumn>, String> {
    let mut columns = Vec::new();
    for raw_column in value.split(',') {
        let column = match raw_column.trim() {
            "name" => FolderReportColumn::Name,
            "path" => FolderReportColumn::Path,
            "state" => FolderReportColumn::State,
            "extension" | "ext" => FolderReportColumn::Extension,
            "left-size" | "left_size" => FolderReportColumn::LeftSize,
            "right-size" | "right_size" => FolderReportColumn::RightSize,
            "left-modified" | "left_modified" | "left-mtime" | "left_mtime" => {
                FolderReportColumn::LeftModified
            }
            "right-modified" | "right_modified" | "right-mtime" | "right_mtime" => {
                FolderReportColumn::RightModified
            }
            "type" => FolderReportColumn::Type,
            "method" | "compare-result" | "compare_result" => FolderReportColumn::Method,
            "error" => FolderReportColumn::Error,
            "" => continue,
            other => return Err(format!("unknown folder report column '{other}'")),
        };
        columns.push(column);
    }

    if columns.is_empty() {
        return Err("--columns must include at least one known column".to_owned());
    }

    Ok(columns)
}

fn folder_html_report(
    result: &FolderCompareResult,
    columns: &[FolderReportColumn],
    tree_state: ReportTreeState,
    nested_file_reports: bool,
    nested_context: Option<usize>,
) -> String {
    let summary = &result.summary;
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">\n");
    output.push_str("<title>LinSync Folder Report</title>\n");
    output.push_str(
        "<style>body{font-family:sans-serif}table{border-collapse:collapse;width:100%}\
td,th{border:1px solid #bbb;padding:0.25rem 0.4rem}td.path,td.error{font-family:monospace;white-space:pre-wrap}\
.folder-tree{margin:1rem 0}.folder-tree li{font-family:monospace;line-height:1.5}.nested-file-report iframe{width:100%;min-height:16rem;border:1px solid #bbb}\
.identical{background:#fff}.different{background:#fff4c2}.left-only{background:#ffd9d9}.right-only{background:#daf5d7}.skipped{background:#eee}.error{background:#ffd1d1}</style>\n",
    );
    output.push_str("</head><body>\n");
    output.push_str(&format!(
        "<h1>{} vs {}</h1>\n",
        escape_html(&result.left_root.display().to_string()),
        escape_html(&result.right_root.display().to_string())
    ));
    output.push_str(&format!(
        "<p>compared={} skipped={} identical={} different={} one-sided={} errors={} elapsed_ms={} status=complete</p>\n",
        summary.compared_count,
        summary.skipped_count,
        summary.identical_count,
        summary.different_count,
        summary.one_sided_count,
        summary.errors_count,
        summary.elapsed.as_millis()
    ));
    output.push_str(&folder_tree_html(result, tree_state));
    output.push_str("<table><thead><tr>");
    for column in columns {
        output.push_str(&format!("<th>{}</th>", column.header()));
    }
    output.push_str("</tr></thead><tbody>\n");
    for entry in &result.entries {
        let state = folder_state(entry.state);
        output.push_str(&format!("<tr class=\"{}\">", state));
        for column in columns {
            match column {
                FolderReportColumn::Name => output.push_str(&format!(
                    "<td class=\"path\">{}</td>",
                    escape_html(&entry.name)
                )),
                FolderReportColumn::Path => output.push_str(&format!(
                    "<td class=\"path\">{}</td>",
                    escape_html(&entry.relative_path.display().to_string())
                )),
                FolderReportColumn::State => output.push_str(&format!("<td>{state}</td>")),
                FolderReportColumn::Extension => output.push_str(&format!(
                    "<td>{}</td>",
                    escape_html(entry.extension.as_deref().unwrap_or(""))
                )),
                FolderReportColumn::LeftSize => output.push_str(&format!(
                    "<td>{}</td>",
                    entry
                        .left_size
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                )),
                FolderReportColumn::RightSize => output.push_str(&format!(
                    "<td>{}</td>",
                    entry
                        .right_size
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                )),
                FolderReportColumn::LeftModified => output.push_str(&format!(
                    "<td>{}</td>",
                    entry
                        .left_modified
                        .and_then(system_time_millis)
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                )),
                FolderReportColumn::RightModified => output.push_str(&format!(
                    "<td>{}</td>",
                    entry
                        .right_modified
                        .and_then(system_time_millis)
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                )),
                FolderReportColumn::Type => output.push_str(&format!(
                    "<td>{}</td>",
                    escape_html(entry.entry_type.as_str())
                )),
                FolderReportColumn::Method => output.push_str(&format!(
                    "<td>{}</td>",
                    escape_html(
                        entry
                            .effective_method
                            .map(CompareMethod::as_str)
                            .unwrap_or("")
                    )
                )),
                FolderReportColumn::Error => output.push_str(&format!(
                    "<td class=\"error\">{}</td>",
                    escape_html(entry.error.as_deref().unwrap_or(""))
                )),
            }
        }
        output.push_str("</tr>\n");
    }
    output.push_str("</tbody></table>\n");
    if nested_file_reports {
        output.push_str(&nested_file_reports_html(result, nested_context));
    }
    output.push_str("</body></html>\n");
    output
}

fn folder_tree_html(result: &FolderCompareResult, tree_state: ReportTreeState) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "<details class=\"folder-tree\" data-tree-state=\"{}\"{}><summary>Folder Tree</summary><ul>\n",
        tree_state.as_str(),
        tree_state.details_open_attr()
    ));
    for entry in &result.entries {
        let state = folder_state(entry.state);
        let depth = entry.relative_path.components().count().saturating_sub(1);
        output.push_str(&format!(
            "<li class=\"{}\" style=\"margin-left:{}rem\">{} <span class=\"path\">{}</span></li>\n",
            state,
            depth,
            state,
            escape_html(&entry.relative_path.display().to_string())
        ));
    }
    output.push_str("</ul></details>\n");
    output
}

fn nested_file_reports_html(result: &FolderCompareResult, context: Option<usize>) -> String {
    let mut output = String::new();
    output.push_str("<h2>Nested File Reports</h2>\n");

    for entry in &result.entries {
        if entry.is_dir
            || matches!(
                entry.state,
                FolderEntryState::Identical | FolderEntryState::Skipped | FolderEntryState::Aborted
            )
        {
            continue;
        }

        let title = entry.relative_path.display().to_string();
        match nested_text_report(result, entry, context) {
            Ok(report) => output.push_str(&format!(
                "<section class=\"nested-file-report\"><h3>{}</h3><iframe title=\"{}\" srcdoc=\"{}\"></iframe></section>\n",
                escape_html(&title),
                escape_html(&title),
                escape_html(&report)
            )),
            Err(err) => output.push_str(&format!(
                "<section class=\"nested-file-report\"><h3>{}</h3><p>{}</p></section>\n",
                escape_html(&title),
                escape_html(&err)
            )),
        }
    }

    output
}

fn nested_text_report(
    result: &FolderCompareResult,
    entry: &linsync_core::FolderEntryDiff,
    context: Option<usize>,
) -> Result<String, String> {
    let left_path = result.left_root.join(&entry.relative_path);
    let right_path = result.right_root.join(&entry.relative_path);
    let text_result = match entry.state {
        FolderEntryState::Different => {
            let left_text = read_nested_report_text(&left_path)?;
            let right_text = read_nested_report_text(&right_path)?;
            compare_text(
                &left_path.display().to_string(),
                &left_text,
                &right_path.display().to_string(),
                &right_text,
                &TextCompareOptions::default(),
            )
        }
        FolderEntryState::LeftOnly => {
            let left_text = read_nested_report_text(&left_path)?;
            compare_text(
                &left_path.display().to_string(),
                &left_text,
                "/dev/null",
                "",
                &TextCompareOptions::default(),
            )
        }
        FolderEntryState::RightOnly => {
            let right_text = read_nested_report_text(&right_path)?;
            compare_text(
                "/dev/null",
                "",
                &right_path.display().to_string(),
                &right_text,
                &TextCompareOptions::default(),
            )
        }
        FolderEntryState::Identical
        | FolderEntryState::Skipped
        | FolderEntryState::Error
        | FolderEntryState::Aborted => return Err("no nested text report for this row".to_owned()),
    };

    Ok(text_result.to_html_report_with_context(context))
}

fn self_compare_command(args: &[String]) -> Result<ExitCode, String> {
    let mut json = false;
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            value => paths.push(value),
        }
    }

    if paths.len() != 1 {
        return Err("usage: linsync-cli self-compare [--json] FILE".to_owned());
    }

    let path = PathBuf::from(paths[0]);
    let bytes = fs::read(&path).map_err(|err| err.to_string())?;
    let cache_dir = TemporaryComparisonDir::new(&AppPaths::from_env().comparison_cache_dir())?;
    let copy_path = cache_dir.path().join(
        path.file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("copy")),
    );
    fs::write(&copy_path, &bytes).map_err(|err| err.to_string())?;

    if is_likely_binary(&bytes) {
        let result = compare_binary_files(&path, &copy_path, &BinaryCompareOptions::default())
            .map_err(|err| err.to_string())?;

        if json {
            println!(
                "{}",
                serde_json::json!({
                    "equal": result.is_equal(),
                    "type": "binary",
                    "differences": result.differences.len(),
                })
            );
        } else {
            println!(
                "{} vs temporary copy: {} differing bytes",
                path.display(),
                result.differences.len()
            );
        }

        return Ok(if result.is_equal() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        });
    }

    let result = compare_text_files(&path, &copy_path, &TextCompareOptions::default())
        .map_err(|err| err.to_string())?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "equal": result.is_equal(),
                "type": "text",
                "differences": result.difference_count(),
            })
        );
    } else {
        println!(
            "{} vs temporary copy: {} differing lines",
            path.display(),
            result.difference_count()
        );
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

struct TemporaryComparisonDir {
    path: PathBuf,
}

impl TemporaryComparisonDir {
    fn new(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(|err| err.to_string())?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_nanos();

        for attempt in 0..100 {
            let path = root.join(format!(
                "linsync-compare-{}-{now}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                            .map_err(|err| err.to_string())?;
                    }
                    return Ok(Self { path });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err.to_string()),
            }
        }

        Err("could not create unique temporary comparison directory".to_owned())
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryComparisonDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn parse_delimiter(value: &str) -> Result<char, String> {
    if value == "\\t" {
        return Ok('\t');
    }

    let mut chars = value.chars();
    let Some(delimiter) = chars.next() else {
        return Err("--delimiter cannot be empty".to_owned());
    };
    if chars.next().is_some() {
        return Err("--delimiter must be a single character".to_owned());
    }
    Ok(delimiter)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum OutputMode {
    #[default]
    Text,
    Json,
    Count,
    Quiet,
}

struct ImageCompareArgsOptions {
    mode: String,
    tolerance: u8,
    delta_e: f32,
}

impl Default for ImageCompareArgsOptions {
    fn default() -> Self {
        Self {
            mode: "exact".into(),
            tolerance: 0,
            delta_e: 2.3,
        }
    }
}

struct DocumentCompareArgsOptions {
    /// "text" | "ocr_text" (default: "text")
    mode: String,
    /// ISO 639-2 language code for Tesseract (default: "eng")
    ocr_language: String,
}

impl Default for DocumentCompareArgsOptions {
    fn default() -> Self {
        Self {
            mode: "text".into(),
            ocr_language: "eng".into(),
        }
    }
}

struct CompareArgs {
    output: OutputMode,
    compare_type: CompareType,
    text_options: TextCompareOptions,
    folder_options: FolderCompareOptions,
    table_options: TableCompareOptions,
    binary_options: BinaryCompareOptions,
    image_options: ImageCompareArgsOptions,
    document_options: DocumentCompareArgsOptions,
    paths: Vec<String>,
    /// The effective profile id, if `--profile` was passed. Used only
    /// for echoing the active profile in JSON output; the per-mode
    /// option fields above already incorporate the profile's values.
    effective_profile: Option<String>,
    explicit_text_options: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum CompareType {
    #[default]
    Auto,
    Text,
    Binary,
    Hex,
    Folder,
    Table,
    Image,
    Document,
}

/// Number of value tokens the named `compare` option consumes after itself in
/// the main option parser, or `None` for flags/positional tokens that take no
/// value. Kept in lockstep with the `index += N` branches in
/// [`split_compare_args`]; `--profile` is intentionally excluded because the
/// first pass resolves it directly.
fn compare_flag_value_count(flag: &str) -> Option<usize> {
    match flag {
        "--diff-algorithm"
        | "--inline-granularity"
        | "--regex-rule-set"
        | "--context"
        | "--render"
        | "--syntax"
        | "--find"
        | "--bookmark"
        | "--encoding"
        | "--type"
        | "--ignore-line-regex"
        | "--image-mode"
        | "--image-tolerance"
        | "--image-delta-e"
        | "--document-mode"
        | "--ocr-language" => Some(1),
        "--substitute-regex" => Some(2),
        _ => None,
    }
}

fn split_compare_args(args: &[String]) -> Result<CompareArgs, String> {
    let mut output = OutputMode::Text;
    let mut compare_type = CompareType::Auto;
    let mut text_options = TextCompareOptions::default();
    let mut folder_options = FolderCompareOptions::default();
    let mut table_options = TableCompareOptions::default();
    let mut binary_options = BinaryCompareOptions::default();
    let mut image_options = ImageCompareArgsOptions::default();
    let mut document_options = DocumentCompareArgsOptions::default();
    let mut paths = Vec::new();
    let mut effective_profile: Option<String> = None;
    let mut explicit_text_options = false;

    // First pass: resolve --profile so the per-mode options are seeded
    // from the profile *before* the rest of the flag parsing overrides
    // individual fields. This ordering means CLI flags always win over
    // profile values, matching PLAN.md's "CLI flags override profile
    // values predictably" rule.
    let mut filtered: Vec<&String> = Vec::with_capacity(args.len());
    let mut profile_seek = 0;
    while profile_seek < args.len() {
        // Skip past the value token(s) of any other value-taking flag so a
        // `--profile` that is actually *another* flag's argument (e.g.
        // `--ignore-line-regex --profile`) is not misread as the profile
        // selector. This mirrors the value-consumption (`index += N`) of the
        // main option parser below.
        if let Some(values) = compare_flag_value_count(args[profile_seek].as_str()) {
            filtered.push(&args[profile_seek]);
            for offset in 1..=values {
                if let Some(token) = args.get(profile_seek + offset) {
                    filtered.push(token);
                }
            }
            profile_seek += 1 + values;
            continue;
        }
        if args[profile_seek] == "--profile" {
            let Some(value) = args.get(profile_seek + 1) else {
                return Err(
                    "--profile requires a value (name of a built-in / saved profile, or a path to a profile JSON file)"
                        .to_owned(),
                );
            };
            let profile = resolve_profile_arg(value)?;
            text_options = profile.text.clone();
            folder_options = profile.folder.clone();
            table_options = profile.table.clone();
            binary_options = profile.binary.clone();
            // image_options / document_options use CLI-side helper
            // structs; copy the relevant fields out of the profile.
            // linsync-cli always pulls in image-compare and
            // document-compare through linsync-core's feature list, so
            // we can unconditionally read those fields here.
            image_options.mode = match profile.image.mode {
                linsync_core::ImageCompareMode::Exact => "exact".to_owned(),
                linsync_core::ImageCompareMode::Tolerance(_) => "tolerance".to_owned(),
                linsync_core::ImageCompareMode::Perceptual => "perceptual".to_owned(),
            };
            image_options.tolerance = profile.image.tolerance;
            image_options.delta_e = profile.image.delta_e_threshold;
            document_options.mode = match profile.document.mode {
                linsync_core::DocumentCompareMode::Text => "text".to_owned(),
                linsync_core::DocumentCompareMode::OcrText => "ocr_text".to_owned(),
                linsync_core::DocumentCompareMode::Rendered => "text".to_owned(),
            };
            document_options.ocr_language = profile.document.ocr_language.clone();
            effective_profile = Some(profile.id.to_string());
            profile_seek += 2;
            continue;
        }
        filtered.push(&args[profile_seek]);
        profile_seek += 1;
    }
    // Re-collect into an owned Vec so the rest of the parser doesn't
    // need to be retrofitted to borrowed slices.
    let args: Vec<String> = filtered.into_iter().cloned().collect();
    let args = args.as_slice();

    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--json" => set_output_mode(&mut output, OutputMode::Json, "--json")?,
            "--count" => set_output_mode(&mut output, OutputMode::Count, "--count")?,
            "--quiet" | "-q" => set_output_mode(&mut output, OutputMode::Quiet, "--quiet")?,
            "--ignore-case" => {
                text_options.ignore_case = true;
                explicit_text_options = true;
            }
            "--ignore-whitespace" => {
                text_options.ignore_whitespace = true;
                explicit_text_options = true;
            }
            "--ignore-blank-lines" => {
                text_options.ignore_blank_lines = true;
                explicit_text_options = true;
            }
            "--ignore-eol" => {
                text_options.ignore_eol = true;
                explicit_text_options = true;
            }
            "--detect-moves" => {
                text_options.detect_moves = true;
                explicit_text_options = true;
            }
            "--diff-algorithm" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(
                        "--diff-algorithm requires a value: lcs | patience | myers".to_owned()
                    );
                };
                explicit_text_options = true;
                text_options.diff_algorithm = match value.as_str() {
                    "lcs" => DiffAlgorithm::Lcs,
                    "patience" => DiffAlgorithm::Patience,
                    "myers" => DiffAlgorithm::Myers,
                    _ => return Err(format!("unknown --diff-algorithm '{value}'")),
                };
                index += 1;
            }
            "--inline-granularity" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(
                        "--inline-granularity requires a value: char | word | grapheme".to_owned(),
                    );
                };
                explicit_text_options = true;
                text_options.inline_granularity = match value.as_str() {
                    "char" => InlineGranularity::Char,
                    "word" => InlineGranularity::Word,
                    "grapheme" => InlineGranularity::Grapheme,
                    _ => return Err(format!("unknown --inline-granularity '{value}'")),
                };
                index += 1;
            }
            "--regex-rule-set" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--regex-rule-set requires a named rule set".to_owned());
                };
                text_options.regex_rule_sets.push(value.clone());
                explicit_text_options = true;
                index += 1;
            }
            "--context" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--context requires a non-negative integer".to_owned());
                };
                text_options.context_lines = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| "--context requires a non-negative integer".to_owned())?,
                );
                explicit_text_options = true;
                index += 1;
            }
            "--show-only-changes" => {
                text_options.show_only_changes = true;
                explicit_text_options = true;
            }
            "--render" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--render requires a value: side-by-side | unified | context | normal | html".to_owned());
                };
                text_options.render_mode = parse_text_render_mode(value)?;
                explicit_text_options = true;
                index += 1;
            }
            "--syntax" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--syntax requires a value: plain | auto | rust | json | html | markdown | shell | toml | yaml".to_owned());
                };
                text_options.syntax_mode = parse_text_syntax_mode(value)?;
                explicit_text_options = true;
                index += 1;
            }
            "--find" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--find requires a search pattern".to_owned());
                };
                text_options.find = Some(TextFindOptions {
                    pattern: value.clone(),
                    regex: text_options.find.as_ref().is_some_and(|f| f.regex),
                    case_sensitive: text_options.find.as_ref().is_some_and(|f| f.case_sensitive),
                });
                explicit_text_options = true;
                index += 1;
            }
            "--find-regex" => {
                let find = text_options.find.get_or_insert_with(|| TextFindOptions {
                    pattern: String::new(),
                    regex: false,
                    case_sensitive: false,
                });
                find.regex = true;
                explicit_text_options = true;
            }
            "--find-case-sensitive" => {
                let find = text_options.find.get_or_insert_with(|| TextFindOptions {
                    pattern: String::new(),
                    regex: false,
                    case_sensitive: false,
                });
                find.case_sensitive = true;
                explicit_text_options = true;
            }
            "--bookmark" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--bookmark requires SIDE:LINE[:LABEL]".to_owned());
                };
                text_options.bookmarks.push(parse_text_bookmark(value)?);
                explicit_text_options = true;
                index += 1;
            }
            "--encoding" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--encoding requires a value: auto | utf8 | utf8-bom | utf16le | utf16be | lossy-utf8".to_owned());
                };
                text_options.encoding = parse_text_input_encoding(value)?;
                explicit_text_options = true;
                index += 1;
            }
            "--type" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--type requires a value".to_owned());
                };
                compare_type = parse_compare_type(value)?;
                index += 1;
            }
            "--ignore-line-regex" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--ignore-line-regex requires a value".to_owned());
                };
                text_options.ignore_line_patterns.push(value.clone());
                explicit_text_options = true;
                index += 1;
            }
            "--substitute-regex" => {
                let Some(pattern) = args.get(index + 1) else {
                    return Err("--substitute-regex requires a regex pattern".to_owned());
                };
                let Some(replacement) = args.get(index + 2) else {
                    return Err("--substitute-regex requires a replacement".to_owned());
                };
                text_options.substitutions.push(TextSubstitution {
                    pattern: pattern.clone(),
                    replacement: replacement.clone(),
                });
                explicit_text_options = true;
                index += 2;
            }
            "--image-mode" => {
                let Some(v) = args.get(index + 1) else {
                    return Err(
                        "--image-mode requires a value: exact | tolerance | perceptual".into(),
                    );
                };
                if !matches!(v.as_str(), "exact" | "tolerance" | "perceptual") {
                    return Err(format!("unknown --image-mode '{v}'"));
                }
                image_options.mode = v.clone();
                index += 1;
            }
            "--image-tolerance" => {
                let Some(v) = args.get(index + 1) else {
                    return Err("--image-tolerance requires a value (0–255)".into());
                };
                image_options.tolerance = v
                    .parse::<u8>()
                    .map_err(|_| format!("invalid tolerance '{v}'"))?;
                index += 1;
            }
            "--image-delta-e" => {
                let Some(v) = args.get(index + 1) else {
                    return Err("--image-delta-e requires a float value".into());
                };
                image_options.delta_e = v
                    .parse::<f32>()
                    .map_err(|_| format!("invalid delta-e '{v}'"))?;
                index += 1;
            }
            "--document-mode" => {
                let Some(v) = args.get(index + 1) else {
                    return Err("--document-mode requires a value: text | ocr_text".into());
                };
                if !matches!(v.as_str(), "text" | "ocr_text") {
                    return Err(format!("unknown --document-mode '{v}'"));
                }
                document_options.mode = v.clone();
                index += 1;
            }
            "--ocr-language" => {
                let Some(v) = args.get(index + 1) else {
                    return Err("--ocr-language requires a language code (e.g. eng)".into());
                };
                document_options.ocr_language = v.clone();
                index += 1;
            }
            _ => paths.push(args[index].clone()),
        }
        index += 1;
    }
    text_options
        .validate_rule_sets()
        .map_err(|err| format!("invalid compare regex option: {err}"))?;
    if text_options
        .find
        .as_ref()
        .is_some_and(|find| find.pattern.is_empty())
    {
        return Err("--find-regex and --find-case-sensitive require --find PATTERN".to_owned());
    }
    text_options
        .validate_regex_options()
        .map_err(|err| format!("invalid compare regex option: {err}"))?;
    if explicit_text_options && !matches!(compare_type, CompareType::Auto | CompareType::Text) {
        return Err("text ignore and substitution options require --type text".to_owned());
    }

    Ok(CompareArgs {
        output,
        compare_type,
        text_options,
        folder_options,
        table_options,
        binary_options,
        image_options,
        document_options,
        paths,
        effective_profile,
        explicit_text_options,
    })
}

fn parse_compare_type(value: &str) -> Result<CompareType, String> {
    match value {
        "auto" => Ok(CompareType::Auto),
        "text" => Ok(CompareType::Text),
        "binary" => Ok(CompareType::Binary),
        "hex" => Ok(CompareType::Hex),
        "folder" => Ok(CompareType::Folder),
        "table" => Ok(CompareType::Table),
        "image" => Ok(CompareType::Image),
        "document" => Ok(CompareType::Document),
        other => Err(format!("unknown compare type '{other}'")),
    }
}

fn parse_text_render_mode(value: &str) -> Result<TextRenderMode, String> {
    match value {
        "side-by-side" | "side_by_side" | "side" => Ok(TextRenderMode::SideBySide),
        "unified" => Ok(TextRenderMode::Unified),
        "context" => Ok(TextRenderMode::Context),
        "normal" => Ok(TextRenderMode::Normal),
        "html" => Ok(TextRenderMode::Html),
        other => Err(format!("unknown --render '{other}'")),
    }
}

fn parse_text_syntax_mode(value: &str) -> Result<TextSyntaxMode, String> {
    match value {
        "plain" | "none" => Ok(TextSyntaxMode::Plain),
        "auto" => Ok(TextSyntaxMode::Auto),
        "rust" | "rs" => Ok(TextSyntaxMode::Rust),
        "json" => Ok(TextSyntaxMode::Json),
        "html" | "xml" => Ok(TextSyntaxMode::Html),
        "markdown" | "md" => Ok(TextSyntaxMode::Markdown),
        "shell" | "sh" | "bash" => Ok(TextSyntaxMode::Shell),
        "toml" => Ok(TextSyntaxMode::Toml),
        "yaml" | "yml" => Ok(TextSyntaxMode::Yaml),
        other => Err(format!("unknown --syntax '{other}'")),
    }
}

fn parse_text_input_encoding(value: &str) -> Result<TextInputEncoding, String> {
    match value {
        "auto" => Ok(TextInputEncoding::Auto),
        "utf8" | "utf-8" => Ok(TextInputEncoding::Utf8),
        "utf8-bom" | "utf-8-bom" => Ok(TextInputEncoding::Utf8Bom),
        "utf16le" | "utf-16le" | "utf-16-le" => Ok(TextInputEncoding::Utf16Le),
        "utf16be" | "utf-16be" | "utf-16-be" => Ok(TextInputEncoding::Utf16Be),
        "lossy-utf8" | "lossy-utf-8" => Ok(TextInputEncoding::LossyUtf8),
        other => Err(format!("unknown --encoding '{other}'")),
    }
}

fn parse_text_bookmark(value: &str) -> Result<TextBookmark, String> {
    let mut parts = value.splitn(3, ':');
    let side = match parts.next().unwrap_or_default() {
        "left" | "l" => linsync_core::CompareSide::Left,
        "right" | "r" => linsync_core::CompareSide::Right,
        other => {
            return Err(format!(
                "bookmark side '{other}' must be left or right; expected SIDE:LINE[:LABEL]"
            ));
        }
    };
    let Some(line_raw) = parts.next() else {
        return Err("--bookmark requires SIDE:LINE[:LABEL]".to_owned());
    };
    let line = line_raw
        .parse::<usize>()
        .map_err(|_| "--bookmark line must be a positive integer".to_owned())?;
    if line == 0 {
        return Err("--bookmark line must be a positive integer".to_owned());
    }
    let label = parts.next().unwrap_or_default().to_owned();
    Ok(TextBookmark { side, line, label })
}

fn set_output_mode(
    current: &mut OutputMode,
    requested: OutputMode,
    flag: &'static str,
) -> Result<(), String> {
    if *current != OutputMode::Text {
        return Err(format!(
            "output mode flag '{flag}' cannot be combined with another output mode"
        ));
    }

    *current = requested;
    Ok(())
}

struct FolderArgs {
    effective_profile: Option<String>,
    recursive: bool,
    compare_method: CompareMethod,
    timestamp_tolerance: Duration,
    symlink_policy: SymlinkPolicy,
    large_file_threshold: Option<u64>,
    large_file_fallback_method: CompareMethod,
    filters: Vec<FileFilter>,
    filter_match_options: FilterMatchOptions,
    hide_skipped: bool,
    state_filter: Option<FolderEntryFilter>,
    type_filter: FolderTypeFilter,
    search: Option<String>,
    sort: FolderSortKey,
    descending: bool,
    group_by: FolderGrouping,
    offset: usize,
    limit: Option<usize>,
    hash_algorithm: HashAlgorithm,
    compare_permissions: bool,
    compare_ownership: bool,
    output: FolderOutput,
    dry_run: bool,
    paths: Vec<String>,
}

impl FolderArgs {
    fn compare_options(&self) -> FolderCompareOptions {
        FolderCompareOptions {
            recursive: self.recursive,
            compare_method: self.compare_method,
            timestamp_tolerance: self.timestamp_tolerance,
            filters: self.filters.clone(),
            filter_match_options: self.filter_match_options,
            include_skipped: !self.hide_skipped,
            symlink_policy: self.symlink_policy,
            large_file_threshold: self.large_file_threshold,
            large_file_fallback_method: self.large_file_fallback_method,
            hash_algorithm: self.hash_algorithm,
            compare_permissions: self.compare_permissions,
            compare_ownership: self.compare_ownership,
        }
    }

    fn query(&self) -> FolderQuery {
        FolderQuery {
            state: self.state_filter.unwrap_or(FolderEntryFilter::All),
            types: self.type_filter,
            search: self.search.clone(),
            sort: self.sort,
            descending: self.descending,
            group_by: self.group_by,
            offset: self.offset,
            limit: self.limit,
        }
    }

    /// True when the query restricts the result set beyond the default
    /// (used to decide whether `--count` reports matches vs. raw differences).
    fn query_is_restricting(&self) -> bool {
        self.state_filter.is_some()
            || !self.type_filter.is_unrestricted()
            || self
                .search
                .as_deref()
                .is_some_and(|needle| !needle.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderOutput {
    Csv,
    Structured(OutputMode),
}

fn split_folder_args(args: &[String]) -> Result<FolderArgs, String> {
    let mut recursive = false;
    let mut compare_method = CompareMethod::BinaryContents;
    let mut timestamp_tolerance = Duration::ZERO;
    let mut symlink_policy = SymlinkPolicy::CompareTarget;
    let mut large_file_threshold = None;
    let mut large_file_fallback_method = CompareMethod::BinaryContents;
    let mut filters = Vec::new();
    let mut filter_match_options = FilterMatchOptions::default();
    let mut hide_skipped = false;
    let mut state_filter = None;
    let mut type_filter = FolderTypeFilter::default();
    let mut search = None;
    let mut sort = FolderSortKey::default();
    let mut descending = false;
    let mut group_by = FolderGrouping::default();
    let mut offset = 0usize;
    let mut limit = None;
    let mut hash_algorithm = HashAlgorithm::default();
    let mut compare_permissions = false;
    let mut compare_ownership = false;
    let mut output_mode = OutputMode::Text;
    let mut csv = false;
    let mut dry_run = false;
    let mut paths = Vec::new();

    let mut effective_profile: Option<String> = None;
    let mut filtered: Vec<&String> = Vec::with_capacity(args.len());
    let mut profile_seek = 0;
    while profile_seek < args.len() {
        if args[profile_seek] == "--profile" {
            let Some(value) = args.get(profile_seek + 1) else {
                return Err(
                    "--profile requires a value (name of a built-in / saved profile, or a path to a profile JSON file)"
                        .to_owned(),
                );
            };
            let profile = resolve_profile_arg(value)?;
            recursive = profile.folder.recursive;
            compare_method = profile.folder.compare_method;
            timestamp_tolerance = profile.folder.timestamp_tolerance;
            symlink_policy = profile.folder.symlink_policy;
            large_file_threshold = profile.folder.large_file_threshold;
            large_file_fallback_method = profile.folder.large_file_fallback_method;
            filters = profile.folder.filters.clone();
            filter_match_options = profile.folder.filter_match_options;
            hide_skipped = !profile.folder.include_skipped;
            hash_algorithm = profile.folder.hash_algorithm;
            compare_permissions = profile.folder.compare_permissions;
            compare_ownership = profile.folder.compare_ownership;
            effective_profile = Some(profile.id.to_string());
            profile_seek += 2;
            continue;
        }
        filtered.push(&args[profile_seek]);
        profile_seek += 1;
    }

    let args: Vec<String> = filtered.into_iter().cloned().collect();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--recursive" | "-r" => {
                recursive = true;
                index += 1;
            }
            "--json" => {
                if csv {
                    return Err(
                        "output mode flag '--json' cannot be combined with another output mode"
                            .to_owned(),
                    );
                }
                set_output_mode(&mut output_mode, OutputMode::Json, "--json")?;
                index += 1;
            }
            "--csv" => {
                if output_mode != OutputMode::Text {
                    return Err(
                        "output mode flag '--csv' cannot be combined with another output mode"
                            .to_owned(),
                    );
                }
                csv = true;
                index += 1;
            }
            "--count" => {
                if csv {
                    return Err(
                        "output mode flag '--count' cannot be combined with another output mode"
                            .to_owned(),
                    );
                }
                set_output_mode(&mut output_mode, OutputMode::Count, "--count")?;
                index += 1;
            }
            "--quiet" | "-q" => {
                if csv {
                    return Err(
                        "output mode flag '--quiet' cannot be combined with another output mode"
                            .to_owned(),
                    );
                }
                set_output_mode(&mut output_mode, OutputMode::Quiet, "--quiet")?;
                index += 1;
            }
            "--method" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--method requires a value".to_owned());
                };
                compare_method = parse_compare_method(value)?;
                index += 2;
            }
            "--timestamp-tolerance-ms" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--timestamp-tolerance-ms requires a value".to_owned());
                };
                let millis = value.parse::<u64>().map_err(|_| {
                    "--timestamp-tolerance-ms requires a non-negative integer".to_owned()
                })?;
                timestamp_tolerance = Duration::from_millis(millis);
                index += 2;
            }
            "--symlinks" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--symlinks requires target, follow, or special".to_owned());
                };
                symlink_policy = parse_symlink_policy(value)?;
                index += 2;
            }
            "--large-file-threshold-bytes" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--large-file-threshold-bytes requires a byte count".to_owned());
                };
                large_file_threshold = Some(value.parse::<u64>().map_err(|_| {
                    "--large-file-threshold-bytes requires a non-negative integer".to_owned()
                })?);
                index += 2;
            }
            "--large-file-method" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--large-file-method requires quick or binary".to_owned());
                };
                large_file_fallback_method = parse_large_file_method(value)?;
                index += 2;
            }
            "--exclude-generated" => {
                filters.push(FileFilter::generated_directories());
                index += 1;
            }
            "--filter" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(
                        "--filter requires a rule such as wf:*.rs, d!:target, or fe:size >= 10KB"
                            .to_owned(),
                    );
                };
                filters.push(FileFilter::parse(value).map_err(|err| err.to_string())?);
                index += 2;
            }
            "--filter-name" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--filter-name requires a saved filter name".to_owned());
                };
                filters.push(load_named_filter(value)?);
                index += 2;
            }
            "--case-insensitive-filter" => {
                filter_match_options.case_sensitive = false;
                index += 1;
            }
            "--hide-skipped" => {
                hide_skipped = true;
                index += 1;
            }
            "--state" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--state requires a value".to_owned());
                };
                state_filter = Some(parse_folder_entry_filter(value)?);
                index += 2;
            }
            "--search" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--search requires a value".to_owned());
                };
                search = Some(value.clone());
                index += 2;
            }
            "--sort" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(
                        "--sort requires a value: name | path | state | type | size | modified"
                            .to_owned(),
                    );
                };
                sort = parse_folder_sort_key(value)?;
                index += 2;
            }
            "--desc" => {
                descending = true;
                index += 1;
            }
            "--types" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(
                        "--types requires a comma-separated value: file,dir,symlink,special"
                            .to_owned(),
                    );
                };
                type_filter = parse_folder_type_filter(value)?;
                index += 2;
            }
            "--group-by" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(
                        "--group-by requires a value: none | state | type | directory".to_owned(),
                    );
                };
                group_by = parse_folder_grouping(value)?;
                index += 2;
            }
            "--offset" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--offset requires a non-negative integer".to_owned());
                };
                offset = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --offset '{value}': expected an integer"))?;
                index += 2;
            }
            "--limit" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--limit requires a non-negative integer".to_owned());
                };
                limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| format!("invalid --limit '{value}': expected an integer"))?,
                );
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            "--hash-algorithm" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(
                        "--hash-algorithm requires a value: blake3 | sha256 | crc32".to_owned()
                    );
                };
                hash_algorithm = match value.as_str() {
                    "blake3" => HashAlgorithm::Blake3,
                    "sha256" => HashAlgorithm::Sha256,
                    "crc32" => HashAlgorithm::Crc32,
                    _ => return Err(format!("unknown --hash-algorithm '{value}'")),
                };
                index += 2;
            }
            "--compare-permissions" => {
                compare_permissions = true;
                index += 1;
            }
            "--compare-ownership" => {
                compare_ownership = true;
                index += 1;
            }
            value => {
                paths.push(value.to_owned());
                index += 1;
            }
        }
    }

    let output = if csv {
        FolderOutput::Csv
    } else {
        FolderOutput::Structured(output_mode)
    };

    Ok(FolderArgs {
        effective_profile,
        recursive,
        compare_method,
        timestamp_tolerance,
        symlink_policy,
        large_file_threshold,
        large_file_fallback_method,
        filters,
        filter_match_options,
        hide_skipped,
        state_filter,
        type_filter,
        search,
        sort,
        descending,
        group_by,
        offset,
        limit,
        hash_algorithm,
        compare_permissions,
        compare_ownership,
        output,
        dry_run,
        paths,
    })
}

fn load_named_filter(name: &str) -> Result<FileFilter, String> {
    let store = FilterStore::new(AppPaths::from_env().filters_file());
    let filters = store.load_or_default().map_err(|err| err.to_string())?;
    filters
        .filters
        .into_iter()
        .find(|filter| filter.name.as_deref() == Some(name))
        .ok_or_else(|| format!("saved filter '{name}' was not found"))
}

#[derive(Debug, Clone)]
struct ReportArgs {
    output: Option<PathBuf>,
    context: Option<usize>,
    columns: Vec<FolderReportColumn>,
    tree_state: ReportTreeState,
    nested_file_reports: bool,
    paths: Vec<String>,
}

fn split_report_args(args: &[String]) -> Result<ReportArgs, String> {
    let mut output = None;
    let mut context = None;
    let mut columns = FolderReportColumn::default_columns();
    let mut tree_state = ReportTreeState::Expanded;
    let mut nested_file_reports = false;
    let mut paths = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--output" | "-o" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--output requires a file path".to_owned());
                };
                output = Some(PathBuf::from(value));
                index += 2;
            }
            "--context" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--context requires a non-negative integer".to_owned());
                };
                context = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| "--context requires a non-negative integer".to_owned())?,
                );
                index += 2;
            }
            "--columns" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--columns requires a comma-separated column list".to_owned());
                };
                columns = parse_folder_report_columns(value)?;
                index += 2;
            }
            "--tree-state" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--tree-state requires expanded or collapsed".to_owned());
                };
                tree_state = parse_report_tree_state(value)?;
                index += 2;
            }
            "--nested-file-reports" => {
                nested_file_reports = true;
                index += 1;
            }
            value => {
                paths.push(value.to_owned());
                index += 1;
            }
        }
    }

    Ok(ReportArgs {
        output,
        context,
        columns,
        tree_state,
        nested_file_reports,
        paths,
    })
}

fn parse_compare_method(value: &str) -> Result<CompareMethod, String> {
    match value {
        "full" | "full-contents" => Ok(CompareMethod::FullContents),
        "quick" | "quick-contents" => Ok(CompareMethod::QuickContents),
        "binary" | "binary-contents" => Ok(CompareMethod::BinaryContents),
        "modified-date" | "date" => Ok(CompareMethod::ModifiedDate),
        "date-size" | "date-and-size" => Ok(CompareMethod::DateAndSize),
        "size" => Ok(CompareMethod::Size),
        "existence" => Ok(CompareMethod::Existence),
        "hash" | "checksum" | "blake3" | "hash-blake3" => Ok(CompareMethod::HashBlake3),
        "normalized-text" | "normalized" => Ok(CompareMethod::NormalizedText),
        other => Err(format!("unknown folder compare method '{other}'")),
    }
}

fn parse_symlink_policy(value: &str) -> Result<SymlinkPolicy, String> {
    match value {
        "target" | "link-target" | "compare-target" => Ok(SymlinkPolicy::CompareTarget),
        "follow" => Ok(SymlinkPolicy::Follow),
        "special" | "special-file" => Ok(SymlinkPolicy::SpecialFile),
        other => Err(format!(
            "unknown symlink policy '{other}'; expected target, follow, or special"
        )),
    }
}

fn parse_large_file_method(value: &str) -> Result<CompareMethod, String> {
    match value {
        "quick" | "quick-contents" => Ok(CompareMethod::QuickContents),
        "binary" | "binary-contents" => Ok(CompareMethod::BinaryContents),
        other => Err(format!(
            "unknown large-file fallback method '{other}'; expected quick or binary"
        )),
    }
}

fn parse_folder_entry_filter(value: &str) -> Result<FolderEntryFilter, String> {
    match value {
        "all" => Ok(FolderEntryFilter::All),
        "differences" | "diffs" => Ok(FolderEntryFilter::Differences),
        "identical" => Ok(FolderEntryFilter::Identical),
        "different" => Ok(FolderEntryFilter::Different),
        "left-only" => Ok(FolderEntryFilter::LeftOnly),
        "right-only" => Ok(FolderEntryFilter::RightOnly),
        "errors" => Ok(FolderEntryFilter::Errors),
        "skipped" => Ok(FolderEntryFilter::Skipped),
        "aborted" => Ok(FolderEntryFilter::Aborted),
        other => Err(format!("unknown folder state filter '{other}'")),
    }
}

fn parse_folder_sort_key(value: &str) -> Result<FolderSortKey, String> {
    match value {
        "name" => Ok(FolderSortKey::Name),
        "path" => Ok(FolderSortKey::Path),
        "state" => Ok(FolderSortKey::State),
        "type" => Ok(FolderSortKey::Type),
        "size" => Ok(FolderSortKey::Size),
        "modified" | "mtime" => Ok(FolderSortKey::Modified),
        other => Err(format!(
            "unknown --sort key '{other}': expected name | path | state | type | size | modified"
        )),
    }
}

fn parse_folder_grouping(value: &str) -> Result<FolderGrouping, String> {
    match value {
        "none" => Ok(FolderGrouping::None),
        "state" => Ok(FolderGrouping::State),
        "type" => Ok(FolderGrouping::Type),
        "directory" | "dir" => Ok(FolderGrouping::Directory),
        other => Err(format!(
            "unknown --group-by value '{other}': expected none | state | type | directory"
        )),
    }
}

fn parse_folder_type_filter(value: &str) -> Result<FolderTypeFilter, String> {
    let mut filter = FolderTypeFilter {
        files: false,
        directories: false,
        symlinks: false,
        special: false,
    };
    for token in value.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        match token {
            "file" | "files" => filter.files = true,
            "dir" | "directory" | "directories" => filter.directories = true,
            "symlink" | "symlinks" | "link" => filter.symlinks = true,
            "special" => filter.special = true,
            other => {
                return Err(format!(
                    "unknown --types entry '{other}': expected file, dir, symlink, or special"
                ));
            }
        }
    }
    if filter
        == (FolderTypeFilter {
            files: false,
            directories: false,
            symlinks: false,
            special: false,
        })
    {
        return Err("--types requires at least one of: file, dir, symlink, special".to_owned());
    }
    Ok(filter)
}

fn parse_cli_bool(value: &str, flag: &str) -> Result<bool, String> {
    match value {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("{flag} requires true or false")),
    }
}

fn parse_single_char(value: &str, flag: &str) -> Result<char, String> {
    let mut chars = value.chars();
    let Some(ch) = chars.next() else {
        return Err(format!("{flag} requires a character"));
    };
    if chars.next().is_some() {
        return Err(format!("{flag} requires exactly one character"));
    }
    Ok(ch)
}

fn split_output_flag(args: &[String]) -> Result<(Option<PathBuf>, Vec<String>), String> {
    let mut output = None;
    let mut paths = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--output" | "-o" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--output requires a file path".to_owned());
                };
                output = Some(PathBuf::from(value));
                index += 2;
            }
            value => {
                paths.push(value.to_owned());
                index += 1;
            }
        }
    }

    Ok((output, paths))
}

struct PatchArgs {
    output: Option<PathBuf>,
    preview: bool,
    format: PatchFormat,
    context: usize,
    paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatchFormat {
    Unified,
    Context,
    Normal,
}

fn split_patch_args(args: &[String]) -> Result<PatchArgs, String> {
    let mut output = None;
    let mut preview = false;
    let mut format = PatchFormat::Unified;
    let mut context = 3;
    let mut paths = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--output" | "-o" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--output requires a file path".to_owned());
                };
                output = Some(PathBuf::from(value));
                index += 2;
            }
            "--preview" => {
                preview = true;
                index += 1;
            }
            "--format" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--format requires a value".to_owned());
                };
                format = parse_patch_format(value)?;
                index += 2;
            }
            "--context" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--context requires a non-negative integer".to_owned());
                };
                context = value
                    .parse::<usize>()
                    .map_err(|_| "--context requires a non-negative integer".to_owned())?;
                index += 2;
            }
            value => {
                paths.push(value.to_owned());
                index += 1;
            }
        }
    }

    if preview && output.is_some() {
        return Err("patch --preview cannot be combined with --output".to_owned());
    }

    Ok(PatchArgs {
        output,
        preview,
        format,
        context,
        paths,
    })
}

fn parse_patch_format(value: &str) -> Result<PatchFormat, String> {
    match value {
        "unified" => Ok(PatchFormat::Unified),
        "context" => Ok(PatchFormat::Context),
        "normal" => Ok(PatchFormat::Normal),
        other => Err(format!(
            "unsupported patch format '{other}'; expected unified, context, or normal"
        )),
    }
}

fn folder_state(state: FolderEntryState) -> &'static str {
    match state {
        FolderEntryState::Identical => "identical",
        FolderEntryState::Different => "different",
        FolderEntryState::LeftOnly => "left-only",
        FolderEntryState::RightOnly => "right-only",
        FolderEntryState::Skipped => "skipped",
        FolderEntryState::Error => "error",
        FolderEntryState::Aborted => "aborted",
    }
}

fn folder_status(status: linsync_core::FolderCompareStatus) -> &'static str {
    match status {
        linsync_core::FolderCompareStatus::Complete => "complete",
        linsync_core::FolderCompareStatus::Cancelled => "cancelled",
    }
}

fn folder_options_metadata_json(
    options: &FolderCompareOptions,
    effective_profile: Option<&str>,
    state_filter: FolderEntryFilter,
) -> serde_json::Value {
    let timestamp_tolerance_ms =
        u64::try_from(options.timestamp_tolerance.as_millis()).unwrap_or(u64::MAX);
    serde_json::json!({
        "profile": effective_profile,
        "recursive": options.recursive,
        "compare_method": options.compare_method.as_str(),
        "timestamp_tolerance_ms": timestamp_tolerance_ms,
        "symlink_policy": symlink_policy_cli_value(options.symlink_policy),
        "large_file_threshold_bytes": options.large_file_threshold,
        "large_file_fallback_method": options.large_file_fallback_method.as_str(),
        "hash_algorithm": hash_algorithm_cli_value(options.hash_algorithm),
        "compare_permissions": options.compare_permissions,
        "compare_ownership": options.compare_ownership,
        "include_skipped": options.include_skipped,
        "state_filter": folder_entry_filter_value(state_filter),
        "filter_match_options": options.filter_match_options,
        "filters": &options.filters,
    })
}

fn symlink_policy_cli_value(policy: SymlinkPolicy) -> &'static str {
    match policy {
        SymlinkPolicy::CompareTarget => "target",
        SymlinkPolicy::Follow => "follow",
        SymlinkPolicy::SpecialFile => "special",
    }
}

fn hash_algorithm_cli_value(algorithm: HashAlgorithm) -> &'static str {
    match algorithm {
        HashAlgorithm::Blake3 => "blake3",
        HashAlgorithm::Sha256 => "sha256",
        HashAlgorithm::Crc32 => "crc32",
    }
}

fn folder_entry_filter_value(filter: FolderEntryFilter) -> &'static str {
    match filter {
        FolderEntryFilter::All => "all",
        FolderEntryFilter::Differences => "differences",
        FolderEntryFilter::Identical => "identical",
        FolderEntryFilter::Different => "different",
        FolderEntryFilter::LeftOnly => "left-only",
        FolderEntryFilter::RightOnly => "right-only",
        FolderEntryFilter::Errors => "errors",
        FolderEntryFilter::Skipped => "skipped",
        FolderEntryFilter::Aborted => "aborted",
    }
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn system_time_millis(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| duration.as_millis().try_into().ok())
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn bash_completions() -> String {
    format!(
        r#"# bash completion for linsync-cli
_linsync_cli() {{
    local cur prev cmd
    COMPREPLY=()
    cur="${{COMP_WORDS[COMP_CWORD]}}"
    prev="${{COMP_WORDS[COMP_CWORD-1]}}"
    cmd="${{COMP_WORDS[1]}}"

    if [[ $COMP_CWORD -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "{}" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--method" ]]; then
        COMPREPLY=( $(compgen -W "full quick binary modified-date date-size size existence hash-blake3 normalized-text" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--type" ]]; then
        COMPREPLY=( $(compgen -W "auto text binary hex folder table image document" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--diff-algorithm" ]]; then
        COMPREPLY=( $(compgen -W "lcs patience myers" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--inline-granularity" ]]; then
        COMPREPLY=( $(compgen -W "char word grapheme" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--regex-rule-set" ]]; then
        COMPREPLY=( $(compgen -W "{}" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--render" ]]; then
        COMPREPLY=( $(compgen -W "side-by-side unified context normal html" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--syntax" ]]; then
        COMPREPLY=( $(compgen -W "plain auto rust json html markdown shell toml yaml" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--encoding" ]]; then
        COMPREPLY=( $(compgen -W "auto utf8 utf8-bom utf16le utf16be lossy-utf8" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--image-mode" ]]; then
        COMPREPLY=( $(compgen -W "exact tolerance perceptual" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--document-mode" ]]; then
        COMPREPLY=( $(compgen -W "text ocr_text" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--ignore-line-regex" || "$prev" == "--substitute-regex" ]]; then
        COMPREPLY=()
        return 0
    fi

    if [[ "$prev" == "--timestamp-tolerance-ms" ]]; then
        COMPREPLY=()
        return 0
    fi

    if [[ "$prev" == "--large-file-threshold-bytes" ]]; then
        COMPREPLY=()
        return 0
    fi

    if [[ "$prev" == "--large-file-method" ]]; then
        COMPREPLY=( $(compgen -W "quick binary" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--hash-algorithm" ]]; then
        COMPREPLY=( $(compgen -W "blake3 sha256 crc32" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--table-skip-blank" ]]; then
        COMPREPLY=( $(compgen -W "true false" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--symlinks" ]]; then
        COMPREPLY=( $(compgen -W "target follow special" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--format" ]]; then
        COMPREPLY=( $(compgen -W "unified context normal" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--columns" ]]; then
        COMPREPLY=( $(compgen -W "name path state extension left-size right-size left-modified right-modified type method error" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--tree-state" ]]; then
        COMPREPLY=( $(compgen -W "expanded collapsed" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--context" ]]; then
        COMPREPLY=()
        return 0
    fi

    if [[ "$prev" == "--preset" ]]; then
        COMPREPLY=( $(compgen -W "{}" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--filter" || "$prev" == "--filter-name" ]]; then
        COMPREPLY=()
        return 0
    fi

    if [[ "$prev" == "--state" ]]; then
        COMPREPLY=( $(compgen -W "all differences identical different left-only right-only errors skipped aborted" -- "$cur") )
        return 0
    fi

    if [[ "$prev" == "--auto-resolve" ]]; then
        COMPREPLY=( $(compgen -W "left right base" -- "$cur") )
        return 0
    fi

    case "$cmd" in
        compare) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        compare3) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        conflict) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        completions) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        folders) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        hex) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        launch) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        man|manpage) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        mergetool) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        report) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        open-external) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        patch) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        reveal) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        self-compare) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
        table) COMPREPLY=( $(compgen -W "{}" -- "$cur") ) ;;
    esac
}}
complete -F _linsync_cli linsync-cli
"#,
        CLI_COMMANDS.join(" "),
        builtin_text_regex_rule_sets()
            .into_iter()
            .map(|rule_set| rule_set.id)
            .collect::<Vec<_>>()
            .join(" "),
        OPEN_EXTERNAL_PRESETS.join(" "),
        COMPARE_FLAGS.join(" "),
        COMPARE3_FLAGS.join(" "),
        CONFLICT_FLAGS.join(" "),
        COMPLETION_SHELLS.join(" "),
        FOLDER_FLAGS.join(" "),
        HEX_FLAGS.join(" "),
        LAUNCH_FLAGS.join(" "),
        OUTPUT_FLAGS.join(" "),
        REPORT_FLAGS.join(" "),
        OPEN_EXTERNAL_FLAGS.join(" "),
        PATCH_FLAGS.join(" "),
        REVEAL_FLAGS.join(" "),
        SELF_COMPARE_FLAGS.join(" "),
        TABLE_FLAGS.join(" "),
        MERGETOOL_FLAGS.join(" ")
    )
}

fn zsh_completions() -> String {
    format!(
        r#"#compdef linsync-cli

_linsync_cli() {{
    local -a commands
    commands=(
        {}
    )

    _arguments -C \
        '1:command:->command' \
        '*::arg:->args'

    case $state in
        command)
            _describe 'command' commands
            ;;
        args)
            case $words[2] in
                compare) _values 'compare option' {} ;;
                compare3) _values 'compare3 option' {} ;;
                conflict) _values 'conflict option' {} ;;
                completions) _values 'shell' {} ;;
                folders) _values 'folder option' {} ;;
                hex) _values 'hex option' {} ;;
                launch) _values 'launch option' {} ;;
                man|manpage) _values 'output option' {} ;;
                mergetool) _values 'mergetool option' {} ;;
                report) _values 'report option' {} ;;
                open-external) _values 'open-external option' {} ;;
                patch) _values 'patch option' {} ;;
                reveal) _values 'reveal option' {} ;;
                self-compare) _values 'self-compare option' {} ;;
                table) _values 'table option' {} ;;
            esac
            ;;
    esac
}}

_linsync_cli "$@"
"#,
        CLI_COMMANDS
            .iter()
            .map(|command| format!("'{command}:{command}'"))
            .collect::<Vec<_>>()
            .join("\n        "),
        zsh_values(COMPARE_FLAGS),
        zsh_values(COMPARE3_FLAGS),
        zsh_values(CONFLICT_FLAGS),
        zsh_values(COMPLETION_SHELLS),
        zsh_values(FOLDER_FLAGS),
        zsh_values(HEX_FLAGS),
        zsh_values(LAUNCH_FLAGS),
        zsh_values(OUTPUT_FLAGS),
        zsh_values(REPORT_FLAGS),
        zsh_values(OPEN_EXTERNAL_FLAGS),
        zsh_values(PATCH_FLAGS),
        zsh_values(REVEAL_FLAGS),
        zsh_values(SELF_COMPARE_FLAGS),
        zsh_values(TABLE_FLAGS),
        zsh_values(MERGETOOL_FLAGS)
    )
}

fn zsh_values(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn fish_completions() -> String {
    let mut output = String::new();
    for command in CLI_COMMANDS {
        output.push_str(&format!("complete -c linsync-cli -f -a {command}\n"));
    }
    for flag in COMPARE_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from compare' {}\n",
            fish_option(flag)
        ));
    }
    for flag in FOLDER_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from folders' {}\n",
            fish_option(flag)
        ));
    }
    for flag in COMPARE3_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from compare3' {}\n",
            fish_option(flag)
        ));
    }
    for flag in CONFLICT_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from conflict' {}\n",
            fish_option(flag)
        ));
    }
    for flag in HEX_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from hex' {}\n",
            fish_option(flag)
        ));
    }
    for flag in PATCH_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from patch' {}\n",
            fish_option(flag)
        ));
    }
    for flag in REPORT_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from report' {}\n",
            fish_option(flag)
        ));
    }
    for flag in SELF_COMPARE_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from self-compare' {}\n",
            fish_option(flag)
        ));
    }
    for flag in TABLE_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from table' {}\n",
            fish_option(flag)
        ));
    }
    for flag in LAUNCH_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from launch' {}\n",
            fish_option(flag)
        ));
    }
    for flag in OPEN_EXTERNAL_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from open-external' {}\n",
            fish_option(flag)
        ));
    }
    for flag in REVEAL_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from reveal' {}\n",
            fish_option(flag)
        ));
    }
    for flag in MERGETOOL_FLAGS {
        output.push_str(&format!(
            "complete -c linsync-cli -n '__fish_seen_subcommand_from mergetool' {}\n",
            fish_option(flag)
        ));
    }
    output.push_str(
        "complete -c linsync-cli -n '__fish_seen_subcommand_from completions' -a 'bash zsh fish'\n",
    );
    output
}

fn fish_option(flag: &str) -> String {
    if let Some(long) = flag.strip_prefix("--") {
        format!("-l {long}")
    } else if let Some(short) = flag.strip_prefix('-') {
        format!("-s {short}")
    } else {
        format!("-a {flag}")
    }
}

fn man_page() -> String {
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
.B archive [--keep-temp] [--json] LEFT RIGHT
Compare two archive files by extracting them (via tar / unzip subprocesses) and running a folder compare on the extracted trees. Supported extensions: .zip, .jar, .war, .apk, .ipa, .tar, .tgz, .tar.gz, .tbz2, .tar.bz2, .txz, .tar.xz, .tzst, .tar.zst.
.TP
.B cache clear [--scope webcompare]
Clear LinSync cache directories. Currently the only supported scope is webcompare (the webpage compare HTTP fetch cache under $XDG_CACHE_HOME/linsync/webcompare).
.TP
.B compare [--profile NAME-OR-PATH] [--type auto|text|binary|hex|folder|table|image|document] [--json|--count|--quiet] [--ignore-case] [--ignore-whitespace] [--ignore-blank-lines] [--ignore-eol] [--ignore-line-regex REGEX] [--regex-rule-set NAME] [--substitute-regex REGEX REPLACEMENT] [--detect-moves] [--diff-algorithm lcs|patience|myers] [--inline-granularity char|word|grapheme] [--context LINES] [--show-only-changes] [--render side-by-side|unified|context|normal|html] [--syntax plain|auto|rust|json|html|markdown|shell|toml|yaml] [--find PATTERN] [--find-regex] [--find-case-sensitive] [--bookmark SIDE:LINE[:LABEL]] [--encoding auto|utf8|utf8-bom|utf16le|utf16be|lossy-utf8] [--image-mode exact|tolerance|perceptual] [--image-tolerance F] [--image-delta-e F] [--document-mode text|ocr_text] [--ocr-language LANG] LEFT RIGHT
Compare two files and exit with 0 for equal files or 1 for differences. The --type auto default routes Folder/Binary/Table/Text; --type image and --type document must be selected explicitly because auto-detection does not route to those engines. --profile seeds every per-mode option from a built-in id (default, strict-bytes, ignore-formatting, code-review, prose-review, folder-sync-preview, webpage-source-safe), a saved user profile id, or a path to a profile JSON file; explicit CLI flags override the profile values regardless of argument order.
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
.B folders [--recursive] [--profile NAME-OR-PATH] [--method METHOD] [--timestamp-tolerance-ms MS] [--symlinks target|follow|special] [--large-file-threshold-bytes BYTES] [--large-file-method quick|binary] [--hash-algorithm blake3|sha256|crc32] [--compare-permissions] [--compare-ownership] [--dry-run] [--exclude-generated] [--filter RULE] [--filter-name NAME] [--case-insensitive-filter] [--hide-skipped] [--state STATE] [--types LIST] [--search SUBSTR] [--sort KEY] [--desc] [--group-by GROUP] [--offset N] [--limit N] [--json|--csv|--count|--quiet] LEFT RIGHT
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
.B plugin <list [--json] | inspect ID [--json] | validate ID | enable ID | disable ID | set-option ID KEY VALUE | clear-option ID KEY | run-diagnostic ID [--input FILE] [--timeout-ms MS] [--json]>
Manage discovered plugins. list shows installed plugins with enabled state; inspect shows a plugin's manifest, option schema, and current values; validate checks the persisted options against the manifest schema; enable/disable toggle a plugin; set-option validates a value against the schema before persisting it; clear-option removes a stored option; run-diagnostic probes a plugin's helper with an optional sample --input and reports exit/timeout/stdout/stderr plus the parsed protocol response (exit 0 healthy, 1 unhealthy, 2 transport error). Enabled state lives in $XDG_CONFIG_HOME/linsync/plugins.json and option values under $XDG_CONFIG_HOME/linsync/plugin-options/.
.TP
.B profile <list | show ID | validate (ID|PATH) | import PATH | export ID [--output PATH] | delete ID>
Manage compare profiles — named bundles of per-mode comparison options. Built-in profiles ship with the binary; user profiles live under $XDG_CONFIG_HOME/linsync/profiles/. Use --profile NAME-OR-PATH on a compare command to source options from a profile; CLI flags override profile values.
.TP
.B report LEFT RIGHT --output FILE [--context LINES] [--columns COLS] [--tree-state expanded|collapsed] [--nested-file-reports]
Generate an HTML file or folder comparison report with optional text context, folder columns, tree state, or nested file reports.
.TP
.B reveal [--wait] PATH...
Reveal files or folders through org.freedesktop.FileManager1.ShowItems, falling back to xdg-open for the containing folder.
.TP
.B self-compare [--json] FILE
Compare a file against a temporary cached copy.
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
machine-readable merge summary is printed. Without --auto-resolve, the command exits
with code 2 (interactive GUI integration is deferred to a future release).
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

fn print_help() {
    println!(
        "\
linsync-cli {}

USAGE:
    linsync-cli archive [--keep-temp] [--json] LEFT RIGHT
    linsync-cli cache clear [--scope webcompare]
    linsync-cli compare [--profile NAME-OR-PATH] [--type auto|text|binary|hex|folder|table|image|document] [--json|--count|--quiet] [--ignore-case] [--ignore-whitespace] [--ignore-blank-lines] [--ignore-eol] [--ignore-line-regex REGEX] [--regex-rule-set NAME] [--substitute-regex REGEX REPLACEMENT] [--detect-moves] [--diff-algorithm lcs|patience|myers] [--inline-granularity char|word|grapheme] [--context LINES] [--show-only-changes] [--render side-by-side|unified|context|normal|html] [--syntax plain|auto|rust|json|html|markdown|shell|toml|yaml] [--find PATTERN] [--find-regex] [--find-case-sensitive] [--bookmark SIDE:LINE[:LABEL]] [--encoding auto|utf8|utf8-bom|utf16le|utf16be|lossy-utf8] [--image-mode exact|tolerance|perceptual] [--image-tolerance F] [--image-delta-e F] [--document-mode text|ocr_text] [--ocr-language LANG] LEFT RIGHT
    linsync-cli compare3 [--markers|--json] LEFT BASE RIGHT
    linsync-cli conflict [--json] FILE
    linsync-cli completions SHELL
    linsync-cli filter <validate RULE | validate-file PATH | list | migrate INPUT [--out OUTPUT | --in-place]>
    linsync-cli folders [--recursive] [--profile NAME-OR-PATH] [--method METHOD] [--timestamp-tolerance-ms MS] [--symlinks target|follow|special] [--large-file-threshold-bytes BYTES] [--large-file-method quick|binary] [--hash-algorithm blake3|sha256|crc32] [--compare-permissions] [--compare-ownership] [--dry-run] [--exclude-generated] [--filter RULE] [--filter-name NAME] [--case-insensitive-filter] [--hide-skipped] [--state STATE] [--types LIST] [--search SUBSTR] [--sort KEY] [--desc] [--group-by GROUP] [--offset N] [--limit N] [--json|--csv|--count|--quiet] LEFT RIGHT
    linsync-cli hex [--width BYTES] [--metadata-only] [--json|--count|--quiet] LEFT RIGHT
    linsync-cli launch [--wait] [--] [ARGS...]
    linsync-cli man [--output FILE]
    linsync-cli mergetool --base BASE --local LOCAL --remote REMOTE --merged MERGED [--auto-resolve left|right|base] [--json]
    linsync-cli open-external [--wait] [--preset PRESET] PATH...
    linsync-cli patch LEFT RIGHT [--format unified|context|normal] [--context LINES] [--preview|--output FILE]
    linsync-cli plugin <list [--json] | inspect ID [--json] | validate ID | enable ID | disable ID | set-option ID KEY VALUE | clear-option ID KEY | run-diagnostic ID [--input FILE] [--timeout-ms MS] [--json]>
    linsync-cli profile <list | show ID | validate (ID|PATH) | import PATH | export ID [--output PATH] | delete ID>
    linsync-cli reveal [--wait] PATH...
    linsync-cli report LEFT RIGHT --output FILE [--context LINES] [--columns COLS] [--tree-state expanded|collapsed] [--nested-file-reports]
    linsync-cli self-compare [--json] FILE
    linsync-cli table [--header] [--delimiter CHAR|--tsv] [--table-quote CHAR] [--table-escape CHAR] [--table-comment PREFIX] [--table-skip-blank BOOL] [--numeric-tolerance FLOAT] [--json|--count|--quiet] LEFT RIGHT
    linsync-cli webpage --sub-mode html|text|tree|rendered|screenshot --accept-network-fetch [--depth N] [--timeout SECS] [--max-requests N] LEFT_URL RIGHT_URL

mergetool:
    Run linsync-cli as a Git mergetool. Requires --base, --local, --remote, and --merged.
    With --auto-resolve <left|right|base>, all conflicts are resolved automatically and
    the result is written to --merged. Add --json to print a machine-readable merge
    summary. Without --auto-resolve, returns exit code 2 (GUI integration deferred to a
    future release).

plugin:
    Manage discovered plugins. `list [--json]` shows installed plugins and their
    enabled state; `inspect ID [--json]` prints the manifest, option schema, and
    current values; `validate ID` checks the persisted options against the
    schema; `enable`/`disable ID` toggle a plugin; `set-option ID KEY VALUE`
    validates the value against the schema before persisting it (VALUE is parsed
    as JSON, falling back to a string); `clear-option ID KEY` removes it;
    `run-diagnostic ID [--input FILE] [--timeout-ms MS] [--json]` probes the
    helper and reports exit/timeout/stdout/stderr plus the parsed response
    (exit 0 healthy, 1 unhealthy, 2 transport error).

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
    Mode is one of text or ocr_text. --ocr-language sets the Tesseract language code.

Exit codes:
    0  no differences
    1  differences found
    2  error",
        env!("CARGO_PKG_VERSION")
    );
}

fn cache_command(args: &[String]) -> Result<ExitCode, String> {
    let subcommand = args.first().map(String::as_str).unwrap_or("");
    match subcommand {
        "clear" => {
            let scope = args.get(1).map(String::as_str).unwrap_or("all");
            match scope {
                "--scope" => {
                    let scope_val = args.get(2).map(String::as_str).unwrap_or("");
                    match scope_val {
                        "webcompare" => {
                            let cache_dir = linsync_core::AppPaths::from_env().cache_dir;
                            linsync_core::clear_webcompare_cache(&cache_dir)
                                .map_err(|e| e.to_string())?;
                            println!("webcompare cache cleared");
                            Ok(ExitCode::SUCCESS)
                        }
                        other => Err(format!("unknown cache scope '{other}'")),
                    }
                }
                "webcompare" => {
                    let cache_dir = linsync_core::AppPaths::from_env().cache_dir;
                    linsync_core::clear_webcompare_cache(&cache_dir).map_err(|e| e.to_string())?;
                    println!("webcompare cache cleared");
                    Ok(ExitCode::SUCCESS)
                }
                other => Err(format!("unknown cache clear scope '{other}'")),
            }
        }
        other => Err(format!(
            "unknown cache subcommand '{other}'; expected: clear [--scope webcompare]"
        )),
    }
}

fn webpage_command(args: &[String]) -> Result<ExitCode, String> {
    let mut urls: Vec<&str> = Vec::new();
    let mut sub_mode = "html";
    let mut depth: u8 = 1;
    let mut timeout: u32 = 30;
    let mut max_requests: u32 = 50;
    let mut accept_network = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--sub-mode" => {
                i += 1;
                sub_mode = args.get(i).map(String::as_str).unwrap_or("html");
            }
            "--depth" => {
                i += 1;
                depth = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1u8)
                    .clamp(1, 3);
            }
            "--timeout" => {
                i += 1;
                timeout = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(30);
            }
            "--max-requests" => {
                i += 1;
                max_requests = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(50);
            }
            "--accept-network-fetch" => accept_network = true,
            other if !other.starts_with('-') => urls.push(other),
            other => return Err(format!("unknown flag: {other}")),
        }
        i += 1;
    }

    if urls.len() != 2 {
        return Err("webpage requires exactly two URL arguments".to_string());
    }

    if !accept_network {
        eprintln!("error: network fetch requires --accept-network-fetch");
        return Ok(ExitCode::from(2));
    }

    let options = linsync_core::WebpageCompareOptions {
        resource_tree_depth: depth,
        timeout_secs: timeout,
        max_requests,
        confirmed_by_user: true,
        user_agent: None,
    };
    let cache_dir = linsync_core::AppPaths::from_env().cache_dir;

    let result = match sub_mode {
        "html" => linsync_core::compare_webpage_html_source(urls[0], urls[1], &options, &cache_dir),
        "text" => {
            linsync_core::compare_webpage_extracted_text(urls[0], urls[1], &options, &cache_dir)
        }
        "tree" => {
            linsync_core::compare_webpage_resource_tree(urls[0], urls[1], &options, &cache_dir)
        }
        #[cfg(feature = "web-engine")]
        "rendered" => {
            linsync_core::compare_webpage_rendered(urls[0], urls[1], &options, &cache_dir)
        }
        #[cfg(feature = "web-engine")]
        "screenshot" => {
            linsync_core::compare_webpage_screenshot(urls[0], urls[1], &options, &cache_dir)
        }
        #[cfg(not(feature = "web-engine"))]
        "rendered" | "screenshot" => {
            eprintln!("error: {sub_mode} mode requires the web-engine build feature");
            return Ok(ExitCode::from(2));
        }
        other => return Err(format!("unknown sub-mode: {other}")),
    };

    match result {
        Ok(linsync_core::WebpageCompareResult::Text(cmp)) => {
            if cmp.is_equal() {
                println!("identical");
                Ok(ExitCode::SUCCESS)
            } else {
                println!("different");
                Ok(ExitCode::from(1))
            }
        }
        Ok(linsync_core::WebpageCompareResult::Folder(cmp)) => {
            if cmp.is_equal() {
                println!("identical");
                Ok(ExitCode::SUCCESS)
            } else {
                println!("different");
                Ok(ExitCode::from(1))
            }
        }
        #[cfg(feature = "web-engine")]
        Ok(linsync_core::WebpageCompareResult::Rendered(r)) => {
            if r.html_fallback.as_ref().is_some_and(|t| t.is_equal()) || r.dom_diff.is_none() {
                println!("identical (rendered fallback)");
                Ok(ExitCode::SUCCESS)
            } else {
                println!("different");
                Ok(ExitCode::from(1))
            }
        }
        #[cfg(feature = "web-engine")]
        Ok(linsync_core::WebpageCompareResult::Screenshot(_img)) => {
            // Phase 9.7-bis: inspect image diff result.
            println!("screenshot captured");
            Ok(ExitCode::SUCCESS)
        }
        Err(linsync_core::WebpageCompareError::ConfirmationRequired) => {
            eprintln!("error: network fetch requires --accept-network-fetch");
            Ok(ExitCode::from(2))
        }
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn profile_token_consumed_by_value_flag_is_not_the_profile_selector() {
        // `--profile` here is the *value* of `--ignore-line-regex`, so the
        // first pass must leave the profile unresolved and feed the literal
        // string through to the regex option rather than misrouting parsing.
        let args = owned(&["--ignore-line-regex", "--profile", "left.txt", "right.txt"]);
        let parsed = split_compare_args(&args).expect("parse should succeed");
        assert!(parsed.effective_profile.is_none());
        assert_eq!(parsed.text_options.ignore_line_patterns, vec!["--profile"]);
        assert_eq!(parsed.paths, vec!["left.txt", "right.txt"]);
    }

    #[test]
    fn profile_in_flag_position_is_still_resolved() {
        let args = owned(&["--profile", "default", "left.txt", "right.txt"]);
        let parsed = split_compare_args(&args).expect("parse should succeed");
        assert_eq!(parsed.effective_profile.as_deref(), Some("default"));
        assert_eq!(parsed.paths, vec!["left.txt", "right.txt"]);
    }

    #[test]
    fn two_value_flag_does_not_swallow_following_profile() {
        // `--substitute-regex` takes two value tokens; a real `--profile`
        // immediately after them must still be honored.
        let args = owned(&[
            "--substitute-regex",
            "a",
            "b",
            "--profile",
            "default",
            "left.txt",
            "right.txt",
        ]);
        let parsed = split_compare_args(&args).expect("parse should succeed");
        assert_eq!(parsed.effective_profile.as_deref(), Some("default"));
        assert_eq!(parsed.text_options.substitutions.len(), 1);
        assert_eq!(parsed.paths, vec!["left.txt", "right.txt"]);
    }
}
