use super::*;

pub(crate) fn compare_command(args: &[String]) -> Result<ExitCode, String> {
    let compare_args = split_compare_args(args)?;
    if compare_args.paths.len() != 2 {
        return Err(
            "usage: linsync-cli compare [--profile NAME-OR-PATH] [--type auto|text|binary|hex|folder|table|image|document] [--json|--count|--quiet] [--ignore-case] [--ignore-whitespace] [--ignore-blank-lines] [--ignore-eol] [--ignore-line-regex REGEX] [--regex-rule-set NAME] [--prediffer PLUGIN_ID] [--prediffer-conflict-policy chain|first-wins|last-wins] [--substitute-regex REGEX REPLACEMENT] [--detect-moves] [--diff-algorithm lcs|patience|myers] [--inline-granularity char|word|grapheme] [--context LINES] [--show-only-changes] [--render side-by-side|unified|context|normal|html] [--syntax plain|auto|rust|json|html|markdown|shell|toml|yaml|c|cpp|python|javascript|typescript|go|java|css] [--find PATTERN] [--find-regex] [--find-case-sensitive] [--bookmark SIDE:LINE[:LABEL]] [--encoding auto|utf8|utf8-bom|utf16le|utf16be|lossy-utf8] [--image-mode exact|tolerance|perceptual] [--image-tolerance F] [--image-delta-e F] [--image-frames first|all] [--document-mode text|ocr_text|rendered] [--ocr-language LANG] [--document-pages FIRST-LAST] [--save-result FILE] LEFT RIGHT"
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
pub(crate) enum CliPathKind {
    File,
    Directory,
}

pub(crate) fn validate_compare_inputs(
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

pub(crate) fn classify_compare_path(value: &str) -> Result<CliPathKind, String> {
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
    pub(crate) fn as_str(self) -> &'static str {
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

pub(crate) fn detect_compare_type(
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
pub(crate) fn read_classification_prefix(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;

    const PREFIX_LEN: u64 = 4096;
    let file = fs::File::open(path).map_err(|err| err.to_string())?;
    let mut buf = Vec::with_capacity(PREFIX_LEN as usize);
    file.take(PREFIX_LEN)
        .read_to_end(&mut buf)
        .map_err(|err| err.to_string())?;
    Ok(buf)
}

pub(crate) fn table_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "csv" | "tsv"))
}

pub(crate) fn binary_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "bin" | "dat" | "exe" | "dll" | "so" | "dylib" | "o" | "a"
            )
        })
}

/// Compare two text files, routing a profile's `prediffer_plugins` through the
/// plugin pipeline. Every enabled, installed prediffer in the list is applied
/// as an ordered chain (each stage normalizes the previous stage's output)
/// before diffing. Falls back to a plain comparison when no prediffer is
/// requested or none resolve.
pub(crate) fn run_text_compare(
    left: &Path,
    right: &Path,
    options: &TextCompareOptions,
    plugin_enablement: &std::collections::BTreeMap<String, bool>,
) -> Result<linsync_core::TextCompareResult, String> {
    if options.prediffer_plugins.is_empty() {
        return compare_text_files(left, right, options).map_err(|err| err.to_string());
    }
    let paths = AppPaths::from_env();
    let chain =
        resolve_enabled_prediffers(&paths, &options.prediffer_plugins, Some(plugin_enablement));
    if chain.is_empty() {
        eprintln!(
            "info: profile requested prediffer(s) {:?} but none are installed + enabled; comparing without",
            options.prediffer_plugins
        );
        return compare_text_files(left, right, options).map_err(|err| err.to_string());
    }
    let chain_ids: Vec<&str> = chain.iter().map(|p| p.manifest.id.as_str()).collect();
    eprintln!(
        "info: applying prediffer chain before diffing: {} (sandbox: {})",
        chain_ids.join(" -> "),
        active_sandbox_status().label
    );
    compare_text_files_with_prediffer_chain(
        left,
        right,
        options,
        &chain,
        &PluginExecutionOptions::default(),
    )
    .map_err(|err| err.to_string())
}

pub(crate) fn compare_text_command(
    left: PathBuf,
    right: PathBuf,
    compare_args: CompareArgs,
) -> Result<ExitCode, String> {
    let result = run_text_compare(
        &left,
        &right,
        &compare_args.text_options,
        &compare_args.plugin_enablement,
    )?;

    if let Some(path) = &compare_args.save_result {
        let result_json = serde_json::to_value(&result).map_err(|err| err.to_string())?;
        let envelope = serde_json::json!({
            "schema_version": 1,
            "kind": "text",
            "result": result_json,
        });
        fs::write(
            path,
            serde_json::to_string_pretty(&envelope).map_err(|err| err.to_string())?,
        )
        .map_err(|err| {
            format!(
                "cannot write --save-result file '{}': {err}",
                path.display()
            )
        })?;
    }

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

pub(crate) fn compare_binary_command(
    left: &Path,
    right: &Path,
    compare_args: CompareArgs,
) -> Result<ExitCode, String> {
    let result = compare_binary_files(left, right, &compare_args.binary_options)
        .map_err(|err| err.to_string())?;

    if let Some(path) = &compare_args.save_result {
        let result_json = serde_json::to_value(&result).map_err(|err| err.to_string())?;
        let envelope = serde_json::json!({
            "schema_version": 1,
            "kind": "binary",
            "result": result_json,
        });
        fs::write(
            path,
            serde_json::to_string_pretty(&envelope).map_err(|err| err.to_string())?,
        )
        .map_err(|err| {
            format!(
                "cannot write --save-result file '{}': {err}",
                path.display()
            )
        })?;
    }

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

pub(crate) fn uses_text_rendering_options(options: &TextCompareOptions) -> bool {
    options.render_mode != TextRenderMode::SideBySide
        || options.context_lines.is_some()
        || options.show_only_changes
}

pub(crate) fn uses_extended_text_json(options: &TextCompareOptions) -> bool {
    uses_text_rendering_options(options)
        || options.syntax_mode != TextSyntaxMode::Plain
        || options.encoding != TextInputEncoding::Auto
        || !options.regex_rule_sets.is_empty()
        || options.find.is_some()
        || !options.bookmarks.is_empty()
}

pub(crate) fn compare_folder_command(
    left: &Path,
    right: &Path,
    compare_args: CompareArgs,
) -> Result<ExitCode, String> {
    let result = compare_folders(left, right, &compare_args.folder_options)
        .map_err(|err| err.to_string())?;

    if let Some(path) = &compare_args.save_result {
        let result_json = serde_json::to_value(&result).map_err(|err| err.to_string())?;
        let envelope = serde_json::json!({
            "schema_version": 1,
            "kind": "folder",
            "result": result_json,
        });
        fs::write(
            path,
            serde_json::to_string_pretty(&envelope).map_err(|err| err.to_string())?,
        )
        .map_err(|err| {
            format!(
                "cannot write --save-result file '{}': {err}",
                path.display()
            )
        })?;
    }

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

pub(crate) fn compare_table_command(
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

    if let Some(path) = &compare_args.save_result {
        let result_json = serde_json::to_value(&result).map_err(|err| err.to_string())?;
        let envelope = serde_json::json!({
            "schema_version": 1,
            "kind": "table",
            "result": result_json,
        });
        fs::write(
            path,
            serde_json::to_string_pretty(&envelope).map_err(|err| err.to_string())?,
        )
        .map_err(|err| {
            format!(
                "cannot write --save-result file '{}': {err}",
                path.display()
            )
        })?;
    }

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

pub(crate) fn has_tsv_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("tsv"))
}

pub(crate) fn compare_image_command(
    left: &Path,
    right: &Path,
    args: CompareArgs,
) -> Result<ExitCode, String> {
    use linsync_core::{FrameCompareMode, ImageCompareMode, ImageCompareOptions, compare_images};

    let mode = match args.image_options.mode.as_str() {
        "tolerance" => ImageCompareMode::Tolerance {
            tolerance: args.image_options.tolerance,
        },
        "perceptual" => ImageCompareMode::Perceptual,
        _ => ImageCompareMode::Exact,
    };
    let frame_mode = match args.image_options.frames.as_str() {
        "all" => FrameCompareMode::AllFrames,
        _ => FrameCompareMode::FirstFrame,
    };

    let opts = ImageCompareOptions {
        mode,
        tolerance: args.image_options.tolerance,
        delta_e_threshold: args.image_options.delta_e,
        frame_mode,
        // Bound the CLI compare so a pathological image cannot hang the process.
        timeout_secs: 300,
        ..ImageCompareOptions::default()
    };

    let result = compare_images(left, right, &opts).map_err(|e| e.to_string())?;

    if let Some(path) = &args.save_result {
        let envelope = serde_json::json!({
            "schema_version": 1,
            "kind": "image",
            "result": serde_json::to_value(&result).map_err(|err| err.to_string())?,
        });
        fs::write(
            path,
            serde_json::to_string_pretty(&envelope).map_err(|err| err.to_string())?,
        )
        .map_err(|err| {
            format!(
                "cannot write --save-result file '{}': {err}",
                path.display()
            )
        })?;
    }

    match args.output {
        OutputMode::Json => {
            let mut json = serde_json::json!({
                "equal": result.equal,
                "left_dims": result.left_dims,
                "right_dims": result.right_dims,
                "total_pixels": result.total_pixels,
                "differing_pixels": result.differing_pixels,
                "diff_ratio": result.diff_ratio,
                "mode": args.image_options.mode,
                "diff_bbox": result.diff_bbox,
            });
            // Animated (AllFrames) compares report the per-frame breakdown.
            if let Some(count) = result.frame_count {
                json["frame_count"] = serde_json::json!(count);
                json["differing_frames"] = serde_json::json!(
                    result
                        .per_frame_summaries
                        .iter()
                        .filter(|f| !f.equal)
                        .count()
                );
            }
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

pub(crate) fn compare_document_command(
    left: &Path,
    right: &Path,
    args: CompareArgs,
) -> Result<ExitCode, String> {
    use linsync_core::document::{DocumentCompareError, compare_document_files};
    use linsync_core::{DocumentCompareMode, DocumentCompareOptions};

    let mode = match args.document_options.mode.as_str() {
        "ocr_text" => DocumentCompareMode::OcrText,
        "rendered" => DocumentCompareMode::Rendered,
        _ => DocumentCompareMode::Text,
    };

    // Locate the plugins dir relative to the binary (packaging/plugins in dev,
    // $prefix/share/linsync/plugins in an installed build).
    let plugins_root = detect_plugins_root();

    let opts = DocumentCompareOptions {
        mode,
        ocr_language: args.document_options.ocr_language.clone(),
        page_range: args.document_options.page_range,
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

    if let Some(path) = &args.save_result {
        let envelope = serde_json::json!({
            "schema_version": 1,
            "kind": "document",
            "result": serde_json::to_value(&result).map_err(|err| err.to_string())?,
        });
        fs::write(
            path,
            serde_json::to_string_pretty(&envelope).map_err(|err| err.to_string())?,
        )
        .map_err(|err| {
            format!(
                "cannot write --save-result file '{}': {err}",
                path.display()
            )
        })?;
    }

    let text_result = result.text_result.as_ref();
    let is_equal = result.is_equal();
    let rendered = !result.rendered_pages.is_empty();
    // For rendered mode the "difference count" is differing pages, otherwise
    // differing text lines.
    let diff_count = if rendered {
        result.rendered_pages.iter().filter(|p| !p.equal).count()
    } else {
        text_result.map(|t| t.difference_count()).unwrap_or(0)
    };

    match args.output {
        OutputMode::Json => {
            let mut json = serde_json::json!({
                "equal": is_equal,
                "left_extractor": result.left_extractor,
                "right_extractor": result.right_extractor,
                "mode": args.document_options.mode,
            });
            if rendered {
                json["differing_pages"] = serde_json::json!(diff_count);
                json["pages"] = serde_json::to_value(&result.rendered_pages).unwrap_or_default();
            } else {
                json["differing_lines"] = serde_json::json!(diff_count);
            }
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        OutputMode::Quiet => {}
        OutputMode::Count => {
            println!("{diff_count}");
        }
        OutputMode::Text => {
            let unit = if rendered { "pages" } else { "lines" };
            if is_equal {
                println!(
                    "Documents are equal (rendered via {})",
                    result.left_extractor
                );
            } else {
                println!(
                    "Documents differ: {diff_count} differing {unit} (via {})",
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
pub(crate) fn detect_plugins_root() -> std::path::PathBuf {
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
