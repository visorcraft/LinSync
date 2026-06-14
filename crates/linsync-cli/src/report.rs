use super::*;

pub(crate) fn report_command(args: &[String]) -> Result<ExitCode, String> {
    let report_args = split_report_args(args)?;

    // Re-render a previously saved text result instead of comparing afresh.
    if let Some(from_json) = &report_args.from_json {
        let output = report_args
            .output
            .clone()
            .ok_or_else(|| "report --from-json requires --output FILE".to_owned())?;
        let raw = fs::read_to_string(from_json)
            .map_err(|err| format!("cannot read '{}': {err}", from_json.display()))?;
        let envelope: serde_json::Value =
            serde_json::from_str(&raw).map_err(|err| format!("invalid result JSON: {err}"))?;
        let result_value = envelope.get("result").cloned().unwrap_or_default();
        let (html, equal) = match envelope.get("kind").and_then(|k| k.as_str()) {
            Some("text") => {
                let result: linsync_core::TextCompareResult = serde_json::from_value(result_value)
                    .map_err(|err| format!("invalid saved text result: {err}"))?;
                (
                    result.to_html_report_with_context(report_args.context),
                    result.is_equal(),
                )
            }
            Some("folder") => {
                let result: FolderCompareResult = serde_json::from_value(result_value)
                    .map_err(|err| format!("invalid saved folder result: {err}"))?;
                (
                    folder_html_report(
                        &result,
                        &report_args.columns,
                        report_args.tree_state,
                        report_args.nested_file_reports,
                        report_args.context,
                    ),
                    result.is_equal(),
                )
            }
            Some("table") => {
                let result: TableCompareResult = serde_json::from_value(result_value)
                    .map_err(|err| format!("invalid saved table result: {err}"))?;
                (result.to_html_report(), result.is_equal())
            }
            Some("binary") => {
                let result: linsync_core::BinaryCompareResult =
                    serde_json::from_value(result_value)
                        .map_err(|err| format!("invalid saved binary result: {err}"))?;
                (result.to_html_report(), result.is_equal())
            }
            Some("image") => {
                let result: linsync_core::ImageCompareResult = serde_json::from_value(result_value)
                    .map_err(|err| format!("invalid saved image result: {err}"))?;
                (result.to_html_report(), result.equal)
            }
            Some("document") => {
                let result: linsync_core::DocumentCompareResult =
                    serde_json::from_value(result_value)
                        .map_err(|err| format!("invalid saved document result: {err}"))?;
                (result.to_html_report(), result.is_equal())
            }
            other => {
                return Err(format!(
                    "report --from-json: unsupported result kind {other:?} (expected text, folder, table, binary, image, or document)"
                ));
            }
        };
        fs::write(&output, html).map_err(|err| err.to_string())?;
        return Ok(if equal {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        });
    }

    if report_args.paths.len() != 2 {
        return Err(
            "usage: linsync-cli report LEFT RIGHT --output FILE [--context LINES] [--columns COLS] [--tree-state expanded|collapsed] [--nested-file-reports] [--relative-paths] [--from-json FILE]"
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

        let mut result = compare_folders(&left, &right, &FolderCompareOptions::default())
            .map_err(|err| err.to_string())?;
        if report_args.relative_paths {
            // Per-entry paths are already relative; relabel the roots so the
            // report carries no absolute, machine-specific paths.
            result.left_root = PathBuf::from(display_path_relative_to_cwd(&left));
            result.right_root = PathBuf::from(display_path_relative_to_cwd(&right));
        }
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

    let mut result = compare_text_files(
        left.as_path(),
        right.as_path(),
        &TextCompareOptions::default(),
    )
    .map_err(|err| err.to_string())?;
    if report_args.relative_paths {
        result.left_name = display_path_relative_to_cwd(&left);
        result.right_name = display_path_relative_to_cwd(&right);
    }
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

/// `project <validate | show | run> PATH` — operate on a project file: a named
/// bundle of saved comparisons (`ProjectFile`). `run` executes each comparison
/// (auto-detecting the compare mode like `compare`) and exits 0 (all equal),
/// 1 (some differ), or 2 (error) for CI use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FolderReportColumn {
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
pub(crate) enum ReportTreeState {
    Expanded,
    Collapsed,
}

impl ReportTreeState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Expanded => "expanded",
            Self::Collapsed => "collapsed",
        }
    }

    pub(crate) fn details_open_attr(self) -> &'static str {
        match self {
            Self::Expanded => " open",
            Self::Collapsed => "",
        }
    }
}

pub(crate) fn parse_report_tree_state(value: &str) -> Result<ReportTreeState, String> {
    match value {
        "expanded" => Ok(ReportTreeState::Expanded),
        "collapsed" => Ok(ReportTreeState::Collapsed),
        other => Err(format!(
            "unknown report tree state '{other}'; expected expanded or collapsed"
        )),
    }
}

impl FolderReportColumn {
    pub(crate) fn default_columns() -> Vec<Self> {
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

    pub(crate) fn header(self) -> &'static str {
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

pub(crate) fn parse_folder_report_columns(value: &str) -> Result<Vec<FolderReportColumn>, String> {
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

pub(crate) fn folder_html_report(
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
    // Note: the wall-clock elapsed is deliberately omitted here. An HTML report
    // must be a pure function of the comparison *content* so that re-rendering a
    // saved result (`report --from-json`) is byte-identical to a fresh direct
    // report; embedding timing made the artifact non-reproducible. The elapsed
    // is still reported in the live text/JSON summary output.
    output.push_str(&format!(
        "<p>compared={} skipped={} identical={} different={} one-sided={} errors={} status=complete</p>\n",
        summary.compared_count,
        summary.skipped_count,
        summary.identical_count,
        summary.different_count,
        summary.one_sided_count,
        summary.errors_count,
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

pub(crate) fn folder_tree_html(
    result: &FolderCompareResult,
    tree_state: ReportTreeState,
) -> String {
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

pub(crate) fn nested_file_reports_html(
    result: &FolderCompareResult,
    context: Option<usize>,
) -> String {
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

pub(crate) fn nested_text_report(
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
