use std::env;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use linsync_core::{
    AppPaths, BinaryCompareOptions, CompareMethod, CompareOptions, CompareProfile, CompareSession,
    CompareViewMode, DiffAlgorithm, DiffBlockKind, DiffLineKind, FileFilter, FilterMatchOptions,
    FilterParseErrorKind, FilterStore, FolderCompareOptions, FolderCompareResult, FolderEntryDiff,
    FolderEntryFilter, FolderEntryState, FolderGrouping, FolderQuery, FolderSortKey,
    FolderTypeFilter, HashAlgorithm, InlineGranularity, MergeChoice, MoveDirection,
    PluginExecutionOptions, PluginInputDescriptor, ProfileId, ProfileStore, ProfileStoreError,
    ProjectFileStore, RecentSessionStore, SessionFile, SettingsStore, SymlinkPolicy,
    TableCellState, TableCompareOptions, TableCompareResult, TextBookmark, TextCompareOptions,
    TextDocument, TextFindOptions, TextInputEncoding, TextRenderMode, TextSubstitution,
    TextSyntaxMode, ThreeWayConflict, ThreeWayMergeState, active_sandbox_status,
    assess_operation_risks, builtin_profiles, builtin_text_regex_rule_sets, clear_plugin_option,
    compare_archives_with_unpacker, compare_binary_files, compare_folders, compare_table_files,
    compare_text, compare_text_files, compare_text_files_with_prediffer_chain,
    discover_installed_plugins, find_builtin, install_plugin, is_likely_binary,
    load_plugin_enabled_map, load_plugin_options, load_plugin_trusted_map, merge_three_way,
    parse_conflict_markers, plan_folder_operation, probe_plugin, remove_plugin,
    resolve_enabled_prediffers, resolve_enabled_virtualizer_for_extension, set_plugin_enabled,
    set_plugin_option, set_plugin_trusted,
};
use linsync_sandbox::{SandboxPolicy, SandboxedCommand};

mod archive;
mod cache_cmd;
mod compare;
mod completions;
mod filter_cmd;
mod folders;
mod format;
mod hex_cmd;
mod man;
mod merge;
mod parsing;
mod patch_cmd;
mod plugin_cmd;
mod profile_cmd;
mod project_cmd;
mod report;
mod reveal;
mod session_cmd;
mod table_cmd;
mod webpage_cmd;
#[allow(unused_imports)]
pub(crate) use archive::*;
#[allow(unused_imports)]
pub(crate) use cache_cmd::*;
#[allow(unused_imports)]
pub(crate) use compare::*;
#[allow(unused_imports)]
pub(crate) use completions::*;
#[allow(unused_imports)]
pub(crate) use filter_cmd::*;
#[allow(unused_imports)]
pub(crate) use folders::*;
#[allow(unused_imports)]
pub(crate) use format::*;
#[allow(unused_imports)]
pub(crate) use hex_cmd::*;
#[allow(unused_imports)]
pub(crate) use man::*;
#[allow(unused_imports)]
pub(crate) use merge::*;
#[allow(unused_imports)]
pub(crate) use parsing::*;
#[allow(unused_imports)]
pub(crate) use patch_cmd::*;
#[allow(unused_imports)]
pub(crate) use plugin_cmd::*;
#[allow(unused_imports)]
pub(crate) use profile_cmd::*;
#[allow(unused_imports)]
pub(crate) use project_cmd::*;
#[allow(unused_imports)]
pub(crate) use report::*;
#[allow(unused_imports)]
pub(crate) use reveal::*;
#[allow(unused_imports)]
pub(crate) use session_cmd::*;
#[allow(unused_imports)]
pub(crate) use table_cmd::*;
#[allow(unused_imports)]
pub(crate) use webpage_cmd::*;

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
        man::print_help();
        return Ok(ExitCode::SUCCESS);
    };

    match command {
        "archive" => archive::archive_command(&args[1..]),
        "compare" => compare::compare_command(&args[1..]),
        "compare3" => merge::compare3_command(&args[1..]),
        "conflict" => merge::conflict_command(&args[1..]),
        "completions" => completions::completions_command(&args[1..]),
        "filter" => filter_cmd::filter_command(&args[1..]),
        "folders" => folders::folders_command(&args[1..]),
        "hex" => hex_cmd::hex_command(&args[1..]),
        "launch" => reveal::launch_command(&args[1..]),
        "man" | "manpage" => man::man_command(&args[1..]),
        "mergetool" => merge::mergetool_command(&args[1..]),
        "open-external" => reveal::open_external_command(&args[1..]),
        "patch" => patch_cmd::patch_command(&args[1..]),
        "profile" => profile_cmd::profile_command(&args[1..]),
        "plugin" | "plugins" => plugin_cmd::plugin_command(&args[1..]),
        "reveal" => reveal::reveal_command(&args[1..]),
        "report" => report::report_command(&args[1..]),
        "project" | "projects" => project_cmd::project_command(&args[1..]),
        "session" | "sessions" => session_cmd::session_command(&args[1..]),
        "self-compare" => compare::self_compare_command(&args[1..]),
        "table" => table_cmd::table_command(&args[1..]),
        "cache" => cache_cmd::cache_command(&args[1..]),
        "webpage" => webpage_cmd::webpage_command(&args[1..]),
        "help" | "-h" | "--help" => {
            man::print_help();
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
    "project",
    "session",
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
    "--prediffer",
    "--prediffer-conflict-policy",
    "--save-result",
    "--from-json",
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
    "--image-frames",
    "--ocr-language",
    "--document-mode",
    "--document-pages",
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
    "--compare-xattrs",
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
const ARCHIVE_FLAGS: &[&str] = &["--keep-temp", "--json", "--unpacker"];
const CACHE_FLAGS: &[&str] = &["--scope"];
const FILTER_FLAGS: &[&str] = &["--out", "--in-place"];
const PROFILE_FLAGS: &[&str] = &["--output", "-o"];
const PLUGIN_FLAGS: &[&str] = &["--json", "--input", "--timeout-ms"];
const PROJECT_FLAGS: &[&str] = &["--json", "--output", "-o"];
const SESSION_FLAGS: &[&str] = &["--base", "--title", "--profile", "--view"];
const WEBPAGE_FLAGS: &[&str] = &[
    "--sub-mode",
    "--depth",
    "--timeout",
    "--max-requests",
    "--accept-network-fetch",
];
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

    #[test]
    fn parse_text_syntax_mode_accepts_extended_language_tokens() {
        let cases = [
            ("c", TextSyntaxMode::C),
            ("cpp", TextSyntaxMode::Cpp),
            ("python", TextSyntaxMode::Python),
            ("javascript", TextSyntaxMode::JavaScript),
            ("typescript", TextSyntaxMode::TypeScript),
            ("go", TextSyntaxMode::Go),
            ("java", TextSyntaxMode::Java),
            ("css", TextSyntaxMode::Css),
        ];
        for (token, expected) in cases {
            assert_eq!(
                parse_text_syntax_mode(token),
                Ok(expected),
                "token '{token}' should parse"
            );
        }
        assert!(parse_text_syntax_mode("fortran").is_err());
    }
}
