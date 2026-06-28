use super::*;

pub(crate) fn parse_delimiter(value: &str) -> Result<char, String> {
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
pub(crate) enum OutputMode {
    #[default]
    Text,
    Json,
    Count,
    Quiet,
}

pub(crate) struct ImageCompareArgsOptions {
    pub(crate) mode: String,
    pub(crate) tolerance: u8,
    pub(crate) delta_e: f32,
    /// "first" | "all" — animated-image frame comparison mode.
    pub(crate) frames: String,
}

impl Default for ImageCompareArgsOptions {
    fn default() -> Self {
        Self {
            mode: "exact".into(),
            tolerance: 0,
            delta_e: 2.3,
            frames: "first".into(),
        }
    }
}

pub(crate) struct DocumentCompareArgsOptions {
    /// "text" | "ocr_text" | "rendered" (default: "text")
    pub(crate) mode: String,
    /// ISO 639-2 language code for Tesseract (default: "eng")
    pub(crate) ocr_language: String,
    /// 1-based inclusive page range for `rendered` mode (default: all pages).
    pub(crate) page_range: Option<(usize, usize)>,
}

impl Default for DocumentCompareArgsOptions {
    fn default() -> Self {
        Self {
            mode: "text".into(),
            ocr_language: "eng".into(),
            page_range: None,
        }
    }
}

pub(crate) struct CompareArgs {
    pub(crate) output: OutputMode,
    pub(crate) compare_type: CompareType,
    pub(crate) text_options: TextCompareOptions,
    pub(crate) folder_options: FolderCompareOptions,
    pub(crate) table_options: TableCompareOptions,
    pub(crate) binary_options: BinaryCompareOptions,
    pub(crate) image_options: ImageCompareArgsOptions,
    pub(crate) document_options: DocumentCompareArgsOptions,
    pub(crate) paths: Vec<String>,
    /// The effective profile id, if `--profile` was passed. Used only
    /// for echoing the active profile in JSON output; the per-mode
    /// option fields above already incorporate the profile's values.
    pub(crate) effective_profile: Option<String>,
    pub(crate) explicit_text_options: bool,
    /// When set (text compares), write the full result as versioned JSON to this
    /// path so `report --from-json` can re-render it without recomparing.
    pub(crate) save_result: Option<PathBuf>,
    /// Per-profile plugin enable/disable overrides copied from the `--profile`
    /// profile (empty when no profile is passed). Threaded into plugin
    /// resolution so a profile that disables a prediffer/virtualizer is honored
    /// in the CLI exactly as in the GUI.
    pub(crate) plugin_enablement: std::collections::BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum CompareType {
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
pub(crate) fn compare_flag_value_count(flag: &str) -> Option<usize> {
    match flag {
        "--diff-algorithm"
        | "--inline-granularity"
        | "--regex-rule-set"
        | "--prediffer"
        | "--prediffer-conflict-policy"
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
        | "--image-frames"
        | "--document-mode"
        | "--ocr-language"
        | "--document-pages"
        | "--save-result" => Some(1),
        "--substitute-regex" => Some(2),
        _ => None,
    }
}

/// Parse a 1-based inclusive page range like `2-4` (or a single page `3`).
pub(crate) fn parse_page_range(value: &str) -> Result<(usize, usize), String> {
    let parse = |s: &str| -> Result<usize, String> {
        s.trim()
            .parse::<usize>()
            .ok()
            .filter(|n| *n >= 1)
            .ok_or_else(|| format!("invalid page number '{s}' (use 1-based page numbers)"))
    };
    let (first, last) = match value.split_once('-') {
        Some((a, b)) => (parse(a)?, parse(b)?),
        None => {
            let only = parse(value)?;
            (only, only)
        }
    };
    if last < first {
        return Err(format!(
            "--document-pages range {first}-{last} is empty (last page is before first)"
        ));
    }
    Ok((first, last))
}

pub(crate) fn split_compare_args(args: &[String]) -> Result<CompareArgs, String> {
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
    let mut save_result: Option<PathBuf> = None;
    let mut plugin_enablement: std::collections::BTreeMap<String, bool> =
        std::collections::BTreeMap::new();

    // First pass: resolve --profile so the per-mode options are seeded
    // from the profile *before* the rest of the flag parsing overrides
    // individual fields. This ordering means CLI flags always win over
    // profile values, the documented "CLI flags override profile values
    // predictably" rule.
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
                linsync_core::ImageCompareMode::Tolerance { .. } => "tolerance".to_owned(),
                linsync_core::ImageCompareMode::Perceptual => "perceptual".to_owned(),
            };
            image_options.tolerance = profile.image.tolerance;
            image_options.delta_e = profile.image.delta_e_threshold;
            image_options.frames = match profile.image.frame_mode {
                linsync_core::FrameCompareMode::AllFrames => "all".to_owned(),
                linsync_core::FrameCompareMode::FirstFrame => "first".to_owned(),
            };
            document_options.mode = match profile.document.mode {
                linsync_core::DocumentCompareMode::Text => "text".to_owned(),
                linsync_core::DocumentCompareMode::OcrText => "ocr_text".to_owned(),
                linsync_core::DocumentCompareMode::Rendered => "rendered".to_owned(),
            };
            document_options.ocr_language = profile.document.ocr_language.clone();
            document_options.page_range = profile.document.page_range;
            plugin_enablement = profile.plugin_enablement.clone();
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
            "--prediffer" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--prediffer requires a plugin id".to_owned());
                };
                text_options.prediffer_plugins.push(value.clone());
                explicit_text_options = true;
                index += 1;
            }
            "--prediffer-conflict-policy" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(
                        "--prediffer-conflict-policy requires a value: chain | first-wins | last-wins"
                            .to_owned(),
                    );
                };
                text_options.prediffer_conflict_policy = match value.as_str() {
                    "chain" => linsync_core::PredifferConflictPolicy::Chain,
                    "first-wins" => linsync_core::PredifferConflictPolicy::FirstWins,
                    "last-wins" => linsync_core::PredifferConflictPolicy::LastWins,
                    _ => {
                        return Err(format!(
                            "unknown --prediffer-conflict-policy '{value}' (chain | first-wins | last-wins)"
                        ));
                    }
                };
                explicit_text_options = true;
                index += 1;
            }
            "--save-result" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--save-result requires a file path".to_owned());
                };
                save_result = Some(PathBuf::from(value));
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
                    return Err("--syntax requires a value: plain | auto | rust | json | html | markdown | shell | toml | yaml | c | cpp | python | javascript | typescript | go | java | css".to_owned());
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
            "--image-frames" => {
                let Some(v) = args.get(index + 1) else {
                    return Err("--image-frames requires a value: first | all".into());
                };
                if !matches!(v.as_str(), "first" | "all") {
                    return Err(format!("unknown --image-frames '{v}' (first | all)"));
                }
                image_options.frames = v.clone();
                index += 1;
            }
            "--document-mode" => {
                let Some(v) = args.get(index + 1) else {
                    return Err(
                        "--document-mode requires a value: text | ocr_text | rendered".into(),
                    );
                };
                if !matches!(v.as_str(), "text" | "ocr_text" | "rendered") {
                    return Err(format!(
                        "unknown --document-mode '{v}' (expected text | ocr_text | rendered)"
                    ));
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
            "--document-pages" => {
                let Some(v) = args.get(index + 1) else {
                    return Err(
                        "--document-pages requires a 1-based inclusive range, e.g. 2-4".into(),
                    );
                };
                document_options.page_range = Some(parse_page_range(v)?);
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
        save_result,
        plugin_enablement,
    })
}

pub(crate) fn parse_compare_type(value: &str) -> Result<CompareType, String> {
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

pub(crate) fn parse_text_render_mode(value: &str) -> Result<TextRenderMode, String> {
    match value {
        "side-by-side" | "side_by_side" | "side" => Ok(TextRenderMode::SideBySide),
        "unified" => Ok(TextRenderMode::Unified),
        "context" => Ok(TextRenderMode::Context),
        "normal" => Ok(TextRenderMode::Normal),
        "html" => Ok(TextRenderMode::Html),
        other => Err(format!("unknown --render '{other}'")),
    }
}

pub(crate) fn parse_text_syntax_mode(value: &str) -> Result<TextSyntaxMode, String> {
    // Token set lives in core (`TextSyntaxMode: FromStr`), shared with the
    // GUI bridge — same precedent as `FolderGrouping` / `--group-by`.
    value.parse().map_err(|e| format!("{e} (for --syntax)"))
}

pub(crate) fn parse_text_input_encoding(value: &str) -> Result<TextInputEncoding, String> {
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

pub(crate) fn parse_text_bookmark(value: &str) -> Result<TextBookmark, String> {
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

pub(crate) fn set_output_mode(
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

pub(crate) struct FolderArgs {
    pub(crate) effective_profile: Option<String>,
    pub(crate) recursive: bool,
    pub(crate) compare_method: CompareMethod,
    pub(crate) timestamp_tolerance: Duration,
    pub(crate) symlink_policy: SymlinkPolicy,
    pub(crate) large_file_threshold: Option<u64>,
    pub(crate) large_file_fallback_method: CompareMethod,
    pub(crate) filters: Vec<FileFilter>,
    pub(crate) filter_match_options: FilterMatchOptions,
    pub(crate) hide_skipped: bool,
    pub(crate) state_filter: Option<FolderEntryFilter>,
    pub(crate) type_filter: FolderTypeFilter,
    pub(crate) search: Option<String>,
    pub(crate) sort: FolderSortKey,
    pub(crate) descending: bool,
    pub(crate) group_by: FolderGrouping,
    pub(crate) offset: usize,
    pub(crate) limit: Option<usize>,
    pub(crate) hash_algorithm: HashAlgorithm,
    pub(crate) compare_permissions: bool,
    pub(crate) compare_ownership: bool,
    pub(crate) compare_xattrs: bool,
    pub(crate) output: FolderOutput,
    pub(crate) dry_run: bool,
    pub(crate) paths: Vec<String>,
}

impl FolderArgs {
    pub(crate) fn compare_options(&self) -> FolderCompareOptions {
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
            compare_xattrs: self.compare_xattrs,
        }
    }

    pub(crate) fn query(&self) -> FolderQuery {
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
    pub(crate) fn query_is_restricting(&self) -> bool {
        self.state_filter.is_some()
            || !self.type_filter.is_unrestricted()
            || self
                .search
                .as_deref()
                .is_some_and(|needle| !needle.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FolderOutput {
    Csv,
    Structured(OutputMode),
}

pub(crate) fn split_folder_args(args: &[String]) -> Result<FolderArgs, String> {
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
    let mut compare_xattrs = false;
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
            compare_xattrs = profile.folder.compare_xattrs;
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
            "--compare-xattrs" => {
                compare_xattrs = true;
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
        compare_xattrs,
        output,
        dry_run,
        paths,
    })
}

pub(crate) fn load_named_filter(name: &str) -> Result<FileFilter, String> {
    let store = FilterStore::new(AppPaths::from_env().filters_file());
    let filters = store.load_or_default().map_err(|err| err.to_string())?;
    filters
        .filters
        .into_iter()
        .find(|filter| filter.name.as_deref() == Some(name))
        .ok_or_else(|| format!("saved filter '{name}' was not found"))
}

#[derive(Debug, Clone)]
pub(crate) struct ReportArgs {
    pub(crate) output: Option<PathBuf>,
    pub(crate) context: Option<usize>,
    pub(crate) columns: Vec<FolderReportColumn>,
    pub(crate) tree_state: ReportTreeState,
    pub(crate) nested_file_reports: bool,
    pub(crate) relative_paths: bool,
    pub(crate) from_json: Option<PathBuf>,
    pub(crate) paths: Vec<String>,
}

pub(crate) fn split_report_args(args: &[String]) -> Result<ReportArgs, String> {
    let mut output = None;
    let mut context = None;
    let mut columns = FolderReportColumn::default_columns();
    let mut tree_state = ReportTreeState::Expanded;
    let mut nested_file_reports = false;
    let mut relative_paths = false;
    let mut from_json = None;
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
            "--relative-paths" => {
                relative_paths = true;
                index += 1;
            }
            "--from-json" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--from-json requires a file path".to_owned());
                };
                from_json = Some(PathBuf::from(value));
                index += 2;
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
        relative_paths,
        from_json,
        paths,
    })
}

/// Display `path` relative to the current directory when it lives under it,
/// else unchanged. Used by `report --relative-paths` so reports don't embed
/// absolute, machine-specific paths.
pub(crate) fn display_path_relative_to_cwd(path: &Path) -> String {
    if let Ok(cwd) = std::env::current_dir()
        && let Ok(relative) = path.strip_prefix(&cwd)
    {
        return relative.display().to_string();
    }
    path.display().to_string()
}

pub(crate) fn parse_compare_method(value: &str) -> Result<CompareMethod, String> {
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

pub(crate) fn parse_symlink_policy(value: &str) -> Result<SymlinkPolicy, String> {
    match value {
        "target" | "link-target" | "compare-target" => Ok(SymlinkPolicy::CompareTarget),
        "follow" => Ok(SymlinkPolicy::Follow),
        "special" | "special-file" => Ok(SymlinkPolicy::SpecialFile),
        other => Err(format!(
            "unknown symlink policy '{other}'; expected target, follow, or special"
        )),
    }
}

pub(crate) fn parse_large_file_method(value: &str) -> Result<CompareMethod, String> {
    match value {
        "quick" | "quick-contents" => Ok(CompareMethod::QuickContents),
        "binary" | "binary-contents" => Ok(CompareMethod::BinaryContents),
        other => Err(format!(
            "unknown large-file fallback method '{other}'; expected quick or binary"
        )),
    }
}

pub(crate) fn parse_folder_entry_filter(value: &str) -> Result<FolderEntryFilter, String> {
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

pub(crate) fn parse_folder_sort_key(value: &str) -> Result<FolderSortKey, String> {
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

pub(crate) fn parse_folder_grouping(value: &str) -> Result<FolderGrouping, String> {
    value
        .parse()
        .map_err(|err| format!("invalid --group-by value: {err}"))
}

pub(crate) fn parse_folder_type_filter(value: &str) -> Result<FolderTypeFilter, String> {
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

pub(crate) fn parse_cli_bool(value: &str, flag: &str) -> Result<bool, String> {
    match value {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("{flag} requires true or false")),
    }
}

pub(crate) fn parse_single_char(value: &str, flag: &str) -> Result<char, String> {
    let mut chars = value.chars();
    let Some(ch) = chars.next() else {
        return Err(format!("{flag} requires a character"));
    };
    if chars.next().is_some() {
        return Err(format!("{flag} requires exactly one character"));
    }
    Ok(ch)
}

pub(crate) fn split_output_flag(args: &[String]) -> Result<(Option<PathBuf>, Vec<String>), String> {
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

pub(crate) struct PatchArgs {
    pub(crate) output: Option<PathBuf>,
    pub(crate) preview: bool,
    pub(crate) format: PatchFormat,
    pub(crate) context: usize,
    pub(crate) paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatchFormat {
    Unified,
    Context,
    Normal,
}

pub(crate) fn split_patch_args(args: &[String]) -> Result<PatchArgs, String> {
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

pub(crate) fn parse_patch_format(value: &str) -> Result<PatchFormat, String> {
    match value {
        "unified" => Ok(PatchFormat::Unified),
        "context" => Ok(PatchFormat::Context),
        "normal" => Ok(PatchFormat::Normal),
        other => Err(format!(
            "unsupported patch format '{other}'; expected unified, context, or normal"
        )),
    }
}
