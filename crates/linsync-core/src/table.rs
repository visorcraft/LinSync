use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::syntax::escape_html;

/// Maximum size of a file the table engine will read into memory. Larger CSV/TSV
/// files are rejected to prevent OOM.
const MAX_TABLE_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TableCompareOptions {
    pub delimiter: char,
    pub has_header: bool,
    pub key_columns: Vec<usize>,
    pub ignore_columns: Vec<usize>,
    pub ignore_row_order: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote_char: Option<char>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escape_char: Option<char>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment_prefix: Option<String>,
    pub skip_blank_rows: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numeric_tolerance: Option<f64>,
    /// Per-column comparison rules (case-folding, whitespace trimming, a
    /// per-column numeric-tolerance override, and regex normalization). Columns
    /// without a rule use the global settings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_rules: Vec<TableColumnRule>,
}

/// A normalization/tolerance rule applied to one column before comparison.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableColumnRule {
    /// Zero-based column index this rule applies to.
    pub column: usize,
    /// Compare this column case-insensitively.
    #[serde(default)]
    pub case_insensitive: bool,
    /// Trim leading/trailing ASCII/Unicode whitespace before comparing.
    #[serde(default)]
    pub trim: bool,
    /// Numeric tolerance for this column, overriding the table-wide value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numeric_tolerance: Option<f64>,
    /// Regex applied to each cell in this column before comparison; every match
    /// is replaced with `normalize_replacement`. Applied before trim/case so a
    /// pattern can strip volatile substrings (timestamps, ids). Compiled once
    /// per compare; an invalid pattern fails the comparison with an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalize_pattern: Option<String>,
    /// Replacement text for `normalize_pattern` matches (default empty: delete).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub normalize_replacement: String,
    /// Treat this column's cells as date/times and compare them equal when they
    /// fall within this many seconds of each other. Cells are parsed as
    /// ISO-8601-ish values (`YYYY-MM-DD`, optional `T`/space + `HH:MM[:SS]`,
    /// optional fractional seconds and timezone, which are ignored); when either
    /// side does not parse the comparison falls back to the normal text/numeric
    /// rules. Applied after regex/trim normalization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_tolerance_seconds: Option<u64>,
}

impl Default for TableCompareOptions {
    fn default() -> Self {
        Self {
            delimiter: ',',
            has_header: false,
            key_columns: Vec::new(),
            ignore_columns: Vec::new(),
            ignore_row_order: false,
            quote_char: None,
            escape_char: None,
            comment_prefix: None,
            skip_blank_rows: true,
            numeric_tolerance: None,
            column_rules: Vec::new(),
        }
    }
}

/// Resolves per-column comparison rules and the table-wide numeric tolerance
/// into a single cell-equality decision. Built once per compare.
struct CellComparer<'a> {
    global_tolerance: Option<f64>,
    rules: HashMap<usize, &'a TableColumnRule>,
    /// Compiled per-column normalization regexes, keyed by column index.
    regexes: HashMap<usize, regex::Regex>,
}

impl<'a> CellComparer<'a> {
    fn new(options: &'a TableCompareOptions) -> Result<Self, TableParseError> {
        let rules: HashMap<usize, &'a TableColumnRule> = options
            .column_rules
            .iter()
            .map(|rule| (rule.column, rule))
            .collect();
        let mut regexes = HashMap::new();
        for rule in &options.column_rules {
            if let Some(pattern) = &rule.normalize_pattern {
                let compiled = regex::Regex::new(pattern).map_err(|err| TableParseError {
                    message: format!("invalid normalize regex for column {}: {err}", rule.column),
                })?;
                regexes.insert(rule.column, compiled);
            }
        }
        Ok(Self {
            global_tolerance: options.numeric_tolerance,
            rules,
            regexes,
        })
    }

    /// Whether two cells in `column` are equal under this column's rule and the
    /// effective numeric tolerance. Normalization order: regex replace → trim →
    /// case-fold.
    fn equal(&self, column: usize, left: &str, right: &str) -> bool {
        let rule = self.rules.get(&column).copied();
        let regex = self.regexes.get(&column);
        let normalize = |value: &str| {
            let replaced = match (regex, rule) {
                (Some(re), Some(r)) => re.replace_all(value, r.normalize_replacement.as_str()),
                _ => std::borrow::Cow::Borrowed(value),
            };
            let trimmed = if rule.is_some_and(|r| r.trim) {
                replaced.trim()
            } else {
                replaced.as_ref()
            };
            if rule.is_some_and(|r| r.case_insensitive) {
                trimmed.to_lowercase()
            } else {
                trimmed.to_owned()
            }
        };
        let left = normalize(left);
        let right = normalize(right);
        // Date/time tolerance wins when configured and both cells parse as
        // datetimes; otherwise fall through to numeric/text comparison.
        if let Some(tolerance) = rule.and_then(|r| r.date_tolerance_seconds)
            && let (Some(a), Some(b)) = (parse_datetime_epoch(&left), parse_datetime_epoch(&right))
        {
            return a.abs_diff(b) <= tolerance;
        }
        let tolerance = rule
            .and_then(|r| r.numeric_tolerance)
            .or(self.global_tolerance);
        cells_equal_with_tolerance(&left, &right, tolerance)
    }
}

/// Days from 1970-01-01 for a proleptic-Gregorian date (Howard Hinnant's
/// `days_from_civil`). Valid for any in-range `(y, m, d)`.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Parse an ISO-8601-ish datetime to epoch seconds (UTC; any timezone
/// offset / `Z` and fractional seconds are ignored). Returns `None` when the
/// value does not match `YYYY-MM-DD` optionally followed by `T`/space and
/// `HH:MM[:SS]`.
fn parse_datetime_epoch(value: &str) -> Option<i64> {
    let value = value.trim();
    let (date_part, time_part) = match value.find(['T', ' ']) {
        Some(index) => (&value[..index], Some(&value[index + 1..])),
        None => (value, None),
    };
    let mut date = date_part.split('-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: i64 = date.next()?.parse().ok()?;
    let day: i64 = date.next()?.parse().ok()?;
    if date.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (mut hour, mut minute, mut second) = (0i64, 0i64, 0i64);
    if let Some(time) = time_part {
        // Strip timezone (`Z`/`+hh:mm`/`-hh:mm`) and fractional seconds.
        let core = time
            .trim()
            .split(['Z', 'z', '+', '-'])
            .next()
            .unwrap_or("")
            .split('.')
            .next()
            .unwrap_or("");
        let mut parts = core.trim().split(':');
        hour = parts.next()?.parse().ok()?;
        minute = parts.next()?.parse().ok()?;
        if let Some(sec) = parts.next() {
            second = sec.parse().ok()?;
        }
        if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) || !(0..=60).contains(&second) {
            return None;
        }
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCompareResult {
    pub left_name: String,
    pub right_name: String,
    pub header: Option<Vec<String>>,
    pub rows: Vec<TableRowDiff>,
    pub changed_cells: usize,
}

impl TableCompareResult {
    pub fn is_equal(&self) -> bool {
        self.changed_cells == 0
    }

    /// Render a self-contained HTML report of the table comparison: a table
    /// where changed cells show `left → right`, left-only cells are marked
    /// removed and right-only cells added.
    pub fn to_html_report(&self) -> String {
        let mut html = String::new();
        html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">\n");
        html.push_str(&format!(
            "<title>LinSync table report: {} vs {}</title>\n",
            escape_html(&self.left_name),
            escape_html(&self.right_name)
        ));
        html.push_str(
            "<style>\n\
             body{font-family:system-ui,sans-serif;margin:1.5rem;}\n\
             table{border-collapse:collapse;}\n\
             td,th{border:1px solid #ccc;padding:2px 6px;font-family:monospace;text-align:left;}\n\
             .changed{background:#fff3b0;}\n\
             .added{background:#c8f7c5;}\n\
             .removed{background:#f7c5c5;}\n\
             </style>\n</head><body>\n",
        );
        html.push_str(&format!(
            "<h1>{} vs {}</h1>\n",
            escape_html(&self.left_name),
            escape_html(&self.right_name)
        ));
        html.push_str(&format!(
            "<p>{} changed cell(s) across {} row(s).</p>\n",
            self.changed_cells,
            self.rows.len()
        ));
        html.push_str("<table>\n");
        if let Some(header) = &self.header {
            html.push_str("<thead><tr><th>#</th>");
            for cell in header {
                html.push_str(&format!("<th>{}</th>", escape_html(cell)));
            }
            html.push_str("</tr></thead>\n");
        }
        html.push_str("<tbody>\n");
        for row in &self.rows {
            html.push_str(&format!("<tr><td>{}</td>", row.row_index + 1));
            for cell in &row.cells {
                let (class, content) = match cell.state {
                    TableCellState::Equal => (
                        "",
                        escape_html(cell.right.as_deref().or(cell.left.as_deref()).unwrap_or("")),
                    ),
                    TableCellState::Changed => (
                        "changed",
                        format!(
                            "{} → {}",
                            escape_html(cell.left.as_deref().unwrap_or("")),
                            escape_html(cell.right.as_deref().unwrap_or(""))
                        ),
                    ),
                    TableCellState::LeftOnly => {
                        ("removed", escape_html(cell.left.as_deref().unwrap_or("")))
                    }
                    TableCellState::RightOnly => {
                        ("added", escape_html(cell.right.as_deref().unwrap_or("")))
                    }
                };
                if class.is_empty() {
                    html.push_str(&format!("<td>{content}</td>"));
                } else {
                    html.push_str(&format!("<td class=\"{class}\">{content}</td>"));
                }
            }
            html.push_str("</tr>\n");
        }
        html.push_str("</tbody></table>\n</body></html>\n");
        html
    }

    pub fn column_summaries(&self) -> Vec<TableColumnSummary> {
        let mut map: std::collections::BTreeMap<usize, TableColumnSummary> =
            std::collections::BTreeMap::new();
        for row in &self.rows {
            for cell in &row.cells {
                let entry = map
                    .entry(cell.column_index)
                    .or_insert_with(|| TableColumnSummary {
                        column_index: cell.column_index,
                        column_name: cell.column_name.clone(),
                        total_cells: 0,
                        equal_cells: 0,
                        changed_cells: 0,
                        added_cells: 0,
                        removed_cells: 0,
                    });
                entry.total_cells += 1;
                match cell.state {
                    TableCellState::Equal => entry.equal_cells += 1,
                    TableCellState::Changed => entry.changed_cells += 1,
                    TableCellState::LeftOnly => entry.removed_cells += 1,
                    TableCellState::RightOnly => entry.added_cells += 1,
                }
            }
        }
        map.into_values().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRowDiff {
    pub row_index: usize,
    pub cells: Vec<TableCellDiff>,
    pub has_difference: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCellDiff {
    pub column_index: usize,
    pub left: Option<String>,
    pub right: Option<String>,
    pub state: TableCellState,
    #[serde(default)]
    pub column_name: Option<String>,
    #[serde(default)]
    pub diff_type: CellDiffType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_diff: Option<Vec<CellInlineSegment>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableCellState {
    Equal,
    Changed,
    LeftOnly,
    RightOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CellDiffType {
    #[default]
    ValueChanged,
    Added,
    Removed,
    TypeChanged,
    NumericDifference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellInlineSegment {
    pub text: String,
    pub side: CellSegmentSide,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellSegmentSide {
    Both,
    LeftOnly,
    RightOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableColumnSummary {
    pub column_index: usize,
    pub column_name: Option<String>,
    pub total_cells: usize,
    pub equal_cells: usize,
    pub changed_cells: usize,
    pub added_cells: usize,
    pub removed_cells: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableParseError {
    pub message: String,
}

impl std::fmt::Display for TableParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TableParseError {}

fn compute_diff_type(
    state: TableCellState,
    left: &Option<String>,
    right: &Option<String>,
) -> CellDiffType {
    match state {
        TableCellState::Equal => CellDiffType::ValueChanged,
        TableCellState::LeftOnly => CellDiffType::Removed,
        TableCellState::RightOnly => CellDiffType::Added,
        TableCellState::Changed => {
            let l = left.as_deref().unwrap_or("");
            let r = right.as_deref().unwrap_or("");
            let ln = l.parse::<f64>().is_ok();
            let rn = r.parse::<f64>().is_ok();
            match (ln, rn) {
                (true, true) => CellDiffType::NumericDifference,
                (true, false) | (false, true) => CellDiffType::TypeChanged,
                (false, false) => CellDiffType::ValueChanged,
            }
        }
    }
}

pub fn cell_inline_diff(left: &str, right: &str) -> Vec<CellInlineSegment> {
    if left == right {
        if left.is_empty() {
            return Vec::new();
        }
        return vec![CellInlineSegment {
            text: left.to_owned(),
            side: CellSegmentSide::Both,
            changed: false,
        }];
    }
    let lc: Vec<char> = left.chars().collect();
    let rc: Vec<char> = right.chars().collect();
    let prefix = lc.iter().zip(rc.iter()).take_while(|(a, b)| a == b).count();
    let max_suf = (lc.len() - prefix).min(rc.len() - prefix);
    let suffix = (0..max_suf)
        .take_while(|i| lc[lc.len() - 1 - i] == rc[rc.len() - 1 - i])
        .count();
    let mut segs = Vec::new();
    if prefix > 0 {
        segs.push(CellInlineSegment {
            text: lc[..prefix].iter().collect(),
            side: CellSegmentSide::Both,
            changed: false,
        });
    }
    let le = lc.len().saturating_sub(suffix);
    let re = rc.len().saturating_sub(suffix);
    if prefix < le {
        segs.push(CellInlineSegment {
            text: lc[prefix..le].iter().collect(),
            side: CellSegmentSide::LeftOnly,
            changed: true,
        });
    }
    if prefix < re {
        segs.push(CellInlineSegment {
            text: rc[prefix..re].iter().collect(),
            side: CellSegmentSide::RightOnly,
            changed: true,
        });
    }
    if suffix > 0 {
        segs.push(CellInlineSegment {
            text: lc[lc.len() - suffix..].iter().collect(),
            side: CellSegmentSide::Both,
            changed: false,
        });
    }
    segs
}

fn resolve_column_name(header: Option<&Vec<String>>, index: usize) -> Option<String> {
    header.and_then(|h| h.get(index).cloned())
}

fn build_inline_diff(
    state: TableCellState,
    left: &Option<String>,
    right: &Option<String>,
) -> Option<Vec<CellInlineSegment>> {
    if state == TableCellState::Changed {
        Some(cell_inline_diff(
            left.as_deref().unwrap_or(""),
            right.as_deref().unwrap_or(""),
        ))
    } else {
        None
    }
}

pub fn compare_table_files(
    left: &Path,
    right: &Path,
    options: &TableCompareOptions,
) -> Result<TableCompareResult, TableError> {
    for path in [left, right] {
        let len = fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX);
        if len > MAX_TABLE_FILE_BYTES {
            return Err(TableError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("file size {len} exceeds {MAX_TABLE_FILE_BYTES} byte table-file limit"),
            )));
        }
    }
    let left_text = fs::read_to_string(left)?;
    let right_text = fs::read_to_string(right)?;
    Ok(compare_tables(
        &left.display().to_string(),
        &left_text,
        &right.display().to_string(),
        &right_text,
        options,
    )?)
}

#[derive(Debug)]
pub enum TableError {
    Io(io::Error),
    Parse(TableParseError),
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Parse(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for TableError {}

impl From<io::Error> for TableError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<TableParseError> for TableError {
    fn from(value: TableParseError) -> Self {
        Self::Parse(value)
    }
}

pub fn compare_tables(
    left_name: &str,
    left: &str,
    right_name: &str,
    right: &str,
    options: &TableCompareOptions,
) -> Result<TableCompareResult, TableParseError> {
    let mut left_rows = parse_with_options(left, options)?;
    let mut right_rows = parse_with_options(right, options)?;
    let header = if options.has_header {
        let left_header = (!left_rows.is_empty()).then(|| left_rows.remove(0));
        let right_header = (!right_rows.is_empty()).then(|| right_rows.remove(0));
        left_header.or(right_header)
    } else {
        None
    };

    let ignore_set: std::collections::HashSet<usize> =
        options.ignore_columns.iter().copied().collect();
    let comparer = CellComparer::new(options)?;

    if !options.key_columns.is_empty() {
        return Ok(compare_by_key(
            left_name,
            right_name,
            header,
            &left_rows,
            &right_rows,
            &options.key_columns,
            &ignore_set,
            options.ignore_row_order,
            &comparer,
        ));
    }

    let hdr_ref = header.as_ref();
    let max_rows = left_rows.len().max(right_rows.len());
    let mut rows = Vec::new();
    let mut changed_cells = 0;

    for row_index in 0..max_rows {
        let left_row = left_rows.get(row_index);
        let right_row = right_rows.get(row_index);
        let max_cols = left_row
            .map_or(0, Vec::len)
            .max(right_row.map_or(0, Vec::len));
        let mut cells = Vec::new();
        let mut has_difference = false;

        for column_index in 0..max_cols {
            if ignore_set.contains(&column_index) {
                let left_cell = left_row.and_then(|r| r.get(column_index)).cloned();
                let right_cell = right_row.and_then(|r| r.get(column_index)).cloned();
                cells.push(TableCellDiff {
                    column_index,
                    left: left_cell,
                    right: right_cell,
                    state: TableCellState::Equal,
                    column_name: resolve_column_name(hdr_ref, column_index),
                    diff_type: CellDiffType::ValueChanged,
                    inline_diff: None,
                });
                continue;
            }

            let left_cell = left_row.and_then(|row| row.get(column_index)).cloned();
            let right_cell = right_row.and_then(|row| row.get(column_index)).cloned();
            let state = match (&left_cell, &right_cell) {
                (Some(left), Some(right)) if comparer.equal(column_index, left, right) => {
                    TableCellState::Equal
                }
                (Some(_), Some(_)) => TableCellState::Changed,
                (Some(_), None) => TableCellState::LeftOnly,
                (None, Some(_)) => TableCellState::RightOnly,
                (None, None) => continue,
            };

            if state != TableCellState::Equal {
                changed_cells += 1;
                has_difference = true;
            }

            let diff_type = compute_diff_type(state, &left_cell, &right_cell);
            let inline_diff = build_inline_diff(state, &left_cell, &right_cell);
            cells.push(TableCellDiff {
                column_index,
                left: left_cell,
                right: right_cell,
                state,
                column_name: resolve_column_name(hdr_ref, column_index),
                diff_type,
                inline_diff,
            });
        }

        rows.push(TableRowDiff {
            row_index,
            cells,
            has_difference,
        });
    }

    Ok(TableCompareResult {
        left_name: left_name.to_owned(),
        right_name: right_name.to_owned(),
        header,
        rows,
        changed_cells,
    })
}

/// Builds the join key for a row, or `None` when the row is too short to
/// contain every key column. Ragged rows missing a key column must not
/// silently collapse onto one another (or onto a genuinely empty value), so
/// they yield no key and are treated as unmatched by the caller.
fn row_key(row: &[String], key_columns: &[usize]) -> Option<Vec<String>> {
    key_columns.iter().map(|&ci| row.get(ci).cloned()).collect()
}

fn compare_rows_by_cell(
    row_index: usize,
    left_row: Option<&[String]>,
    right_row: Option<&[String]>,
    ignore_set: &std::collections::HashSet<usize>,
    header: Option<&Vec<String>>,
    comparer: &CellComparer,
) -> (TableRowDiff, usize) {
    let max_cols = left_row
        .map_or(0, |r| r.len())
        .max(right_row.map_or(0, |r| r.len()));
    let mut cells = Vec::new();
    let mut has_difference = false;
    let mut changed_cells = 0;

    for column_index in 0..max_cols {
        if ignore_set.contains(&column_index) {
            let left_cell = left_row.and_then(|r| r.get(column_index)).cloned();
            let right_cell = right_row.and_then(|r| r.get(column_index)).cloned();
            cells.push(TableCellDiff {
                column_index,
                left: left_cell,
                right: right_cell,
                state: TableCellState::Equal,
                column_name: resolve_column_name(header, column_index),
                diff_type: CellDiffType::ValueChanged,
                inline_diff: None,
            });
            continue;
        }

        let left_cell = left_row.and_then(|r| r.get(column_index)).cloned();
        let right_cell = right_row.and_then(|r| r.get(column_index)).cloned();
        let state = match (&left_cell, &right_cell) {
            (Some(l), Some(r)) if comparer.equal(column_index, l, r) => TableCellState::Equal,
            (Some(_), Some(_)) => TableCellState::Changed,
            (Some(_), None) => TableCellState::LeftOnly,
            (None, Some(_)) => TableCellState::RightOnly,
            (None, None) => continue,
        };

        if state != TableCellState::Equal {
            changed_cells += 1;
            has_difference = true;
        }

        let diff_type = compute_diff_type(state, &left_cell, &right_cell);
        let inline_diff = build_inline_diff(state, &left_cell, &right_cell);
        cells.push(TableCellDiff {
            column_index,
            left: left_cell,
            right: right_cell,
            state,
            column_name: resolve_column_name(header, column_index),
            diff_type,
            inline_diff,
        });
    }

    (
        TableRowDiff {
            row_index,
            cells,
            has_difference,
        },
        changed_cells,
    )
}

#[allow(clippy::too_many_arguments)]
fn compare_by_key(
    left_name: &str,
    right_name: &str,
    header: Option<Vec<String>>,
    left_rows: &[Vec<String>],
    right_rows: &[Vec<String>],
    key_columns: &[usize],
    ignore_set: &std::collections::HashSet<usize>,
    ignore_row_order: bool,
    comparer: &CellComparer,
) -> TableCompareResult {
    let hdr_ref = header.as_ref();

    // Group right-row indices by key, preserving file order within each key so
    // duplicate keys can be paired by occurrence (the Nth left row with key K
    // matches the Nth right row with key K). Ragged rows lacking a key column
    // (`row_key` returns `None`) are never matched and are emitted as
    // right-only at the end.
    let mut right_by_key: HashMap<Vec<String>, std::collections::VecDeque<usize>> = HashMap::new();
    let mut right_unkeyed: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for (i, row) in right_rows.iter().enumerate() {
        match row_key(row, key_columns) {
            Some(key) => right_by_key.entry(key).or_default().push_back(i),
            None => right_unkeyed.push_back(i),
        }
    }

    let mut rows = Vec::new();
    let mut changed_cells = 0;

    let emit = |rows: &mut Vec<TableRowDiff>,
                changed_cells: &mut usize,
                left: Option<&[String]>,
                right: Option<&[String]>| {
        let (row_diff, cc) =
            compare_rows_by_cell(rows.len(), left, right, ignore_set, hdr_ref, comparer);
        *changed_cells += cc;
        rows.push(row_diff);
    };

    if ignore_row_order {
        // Pair left rows against right rows sharing the same key in occurrence
        // order; collect leftovers (unmatched left, unmatched right, and rows
        // lacking a key column on either side) afterwards.
        let mut left_unmatched: Vec<usize> = Vec::new();
        for (i, row) in left_rows.iter().enumerate() {
            match row_key(row, key_columns) {
                Some(key) => match right_by_key.get_mut(&key).and_then(|q| q.pop_front()) {
                    Some(ri) => emit(
                        &mut rows,
                        &mut changed_cells,
                        Some(row.as_slice()),
                        Some(right_rows[ri].as_slice()),
                    ),
                    None => left_unmatched.push(i),
                },
                None => left_unmatched.push(i),
            }
        }

        for i in left_unmatched {
            emit(
                &mut rows,
                &mut changed_cells,
                Some(left_rows[i].as_slice()),
                None,
            );
        }

        let mut leftover_right: Vec<usize> = right_by_key
            .into_values()
            .flatten()
            .chain(right_unkeyed)
            .collect();
        leftover_right.sort_unstable();
        for ri in leftover_right {
            emit(
                &mut rows,
                &mut changed_cells,
                None,
                Some(right_rows[ri].as_slice()),
            );
        }
    } else {
        for row in left_rows {
            let right_slice = match row_key(row, key_columns) {
                Some(key) => right_by_key
                    .get_mut(&key)
                    .and_then(|q| q.pop_front())
                    .map(|ri| right_rows[ri].as_slice()),
                None => None,
            };
            emit(
                &mut rows,
                &mut changed_cells,
                Some(row.as_slice()),
                right_slice,
            );
        }

        // Any right rows left unpaired (unmatched keys, surplus duplicates, or
        // ragged rows) become right-only, emitted in original file order.
        let mut leftover_right: Vec<usize> = right_by_key
            .into_values()
            .flatten()
            .chain(right_unkeyed)
            .collect();
        leftover_right.sort_unstable();
        for ri in leftover_right {
            emit(
                &mut rows,
                &mut changed_cells,
                None,
                Some(right_rows[ri].as_slice()),
            );
        }
    }

    TableCompareResult {
        left_name: left_name.to_owned(),
        right_name: right_name.to_owned(),
        header,
        rows,
        changed_cells,
    }
}

pub fn parse_delimited(input: &str, delimiter: char) -> Result<Vec<Vec<String>>, TableParseError> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        if in_quotes {
            match ch {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                '"' => in_quotes = false,
                _ => cell.push(ch),
            }
            continue;
        }

        if ch == '"' {
            in_quotes = true;
        } else if ch == delimiter {
            row.push(std::mem::take(&mut cell));
        } else if ch == '\n' {
            row.push(std::mem::take(&mut cell));
            rows.push(std::mem::take(&mut row));
        } else if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut cell));
            rows.push(std::mem::take(&mut row));
        } else {
            cell.push(ch);
        }
    }

    if in_quotes {
        return Err(TableParseError {
            message: "unterminated quoted field".to_owned(),
        });
    }

    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }

    Ok(rows)
}

fn cells_equal_with_tolerance(left: &str, right: &str, tolerance: Option<f64>) -> bool {
    if left == right {
        return true;
    }
    let Some(tol) = tolerance else {
        return false;
    };
    let Ok(ln) = left.parse::<f64>() else {
        return false;
    };
    let Ok(rn) = right.parse::<f64>() else {
        return false;
    };
    (ln - rn).abs() <= tol
}

fn parse_with_options(
    input: &str,
    options: &TableCompareOptions,
) -> Result<Vec<Vec<String>>, TableParseError> {
    let quote = options.quote_char.unwrap_or('"');
    let escape = options.escape_char;
    let comment = options.comment_prefix.as_deref();
    let skip_blank = options.skip_blank_rows;
    let delim = options.delimiter;

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        if in_quotes {
            if let Some(esc) = escape
                && ch == esc
                && let Some(&next) = chars.peek()
                && next == quote
            {
                chars.next();
                cell.push(quote);
                continue;
            }
            if ch == quote {
                if escape.is_none() && chars.peek() == Some(&quote) {
                    chars.next();
                    cell.push(quote);
                } else {
                    in_quotes = false;
                }
            } else {
                cell.push(ch);
            }
            continue;
        }

        if ch == quote {
            in_quotes = true;
        } else if ch == delim {
            row.push(std::mem::take(&mut cell));
        } else if ch == '\n' {
            row.push(std::mem::take(&mut cell));
            finish_row(&mut rows, &mut row, comment, skip_blank);
        } else if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut cell));
            finish_row(&mut rows, &mut row, comment, skip_blank);
        } else {
            cell.push(ch);
        }
    }

    if in_quotes {
        return Err(TableParseError {
            message: "unterminated quoted field".to_owned(),
        });
    }

    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        finish_row(&mut rows, &mut row, comment, skip_blank);
    }

    Ok(rows)
}

fn finish_row(
    rows: &mut Vec<Vec<String>>,
    row: &mut Vec<String>,
    comment_prefix: Option<&str>,
    skip_blank: bool,
) {
    let is_comment = comment_prefix
        .filter(|prefix| !prefix.is_empty())
        .is_some_and(|prefix| row.first().is_some_and(|first| first.starts_with(prefix)));
    let is_blank = row.iter().all(|c| c.is_empty());

    if !(is_comment || is_blank && skip_blank) {
        rows.push(std::mem::take(row));
    } else {
        row.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_changed_cells() {
        let result = compare_tables(
            "left",
            "name,count\nalpha,1\nbeta,2\n",
            "right",
            "name,count\nalpha,1\nbeta,3\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                ..Default::default()
            },
        )
        .unwrap();

        assert!(!result.is_equal());
        assert_eq!(
            result.header.as_ref().unwrap(),
            &vec!["name".to_owned(), "count".to_owned()]
        );
        assert_eq!(result.changed_cells, 1);
        assert_eq!(result.rows[1].cells[1].state, TableCellState::Changed);
    }

    #[test]
    fn supports_quoted_multiline_fields() {
        let rows = parse_delimited("name,body\nalpha,\"one\ntwo\"\n", ',').unwrap();

        assert_eq!(rows[1][1], "one\ntwo");
    }

    #[test]
    fn supports_tsv() {
        let result = compare_tables(
            "left",
            "a\tb\n1\t2\n",
            "right",
            "a\tb\n1\t3\n",
            &TableCompareOptions {
                delimiter: '\t',
                has_header: false,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.changed_cells, 1);
    }

    #[test]
    fn key_columns_match_reordered_rows() {
        let result = compare_tables(
            "left",
            "id,val\n1,aaa\n2,bbb\n3,ccc\n",
            "right",
            "id,val\n3,ccc\n1,aaa\n2,bbb\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                key_columns: vec![0],
                ..Default::default()
            },
        )
        .unwrap();

        assert!(result.is_equal());
        assert_eq!(result.rows.len(), 3);
        for row in &result.rows {
            assert!(!row.has_difference);
        }
    }

    #[test]
    fn key_columns_detect_inserted_deleted() {
        let result = compare_tables(
            "left",
            "id,val\n1,a\n2,b\n4,d\n",
            "right",
            "id,val\n1,a\n3,c\n4,d\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                key_columns: vec![0],
                ..Default::default()
            },
        )
        .unwrap();

        assert!(!result.is_equal());
        assert_eq!(result.rows.len(), 4);

        let left_only: Vec<_> = result
            .rows
            .iter()
            .filter(|r| r.cells.iter().any(|c| c.state == TableCellState::LeftOnly))
            .collect();
        let right_only: Vec<_> = result
            .rows
            .iter()
            .filter(|r| r.cells.iter().any(|c| c.state == TableCellState::RightOnly))
            .collect();

        assert_eq!(left_only.len(), 1);
        assert_eq!(right_only.len(), 1);
    }

    #[test]
    fn ignore_columns_skips_during_compare() {
        let result = compare_tables(
            "left",
            "a,b,c\n1,2,3\n",
            "right",
            "a,b,c\n1,99,3\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                ignore_columns: vec![1],
                ..Default::default()
            },
        )
        .unwrap();

        assert!(result.is_equal());
        assert_eq!(result.rows[0].cells[1].state, TableCellState::Equal);
    }

    #[test]
    fn key_columns_with_ignore_columns() {
        let result = compare_tables(
            "left",
            "id,name,score\n1,alice,90\n2,bob,80\n",
            "right",
            "id,name,score\n2,bob,99\n1,alice,90\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                key_columns: vec![0],
                ignore_columns: vec![2],
                ..Default::default()
            },
        )
        .unwrap();

        assert!(result.is_equal());
        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn cell_diff_detects_value_change() {
        let result = compare_tables(
            "left",
            "name,count\nalpha,1\n",
            "right",
            "name,count\nalpha,2\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                ..Default::default()
            },
        )
        .unwrap();

        let cell = &result.rows[0].cells[1];
        assert_eq!(cell.state, TableCellState::Changed);
        assert_eq!(cell.diff_type, CellDiffType::NumericDifference);
    }

    #[test]
    fn cell_diff_detects_type_change() {
        let result = compare_tables(
            "left",
            "name,count\nalpha,42\n",
            "right",
            "name,count\nalpha,hello\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                ..Default::default()
            },
        )
        .unwrap();

        let cell = &result.rows[0].cells[1];
        assert_eq!(cell.state, TableCellState::Changed);
        assert_eq!(cell.diff_type, CellDiffType::TypeChanged);
    }

    #[test]
    fn cell_inline_diff_segments() {
        let segs = cell_inline_diff("hello", "hallo");
        assert_eq!(segs.len(), 4);
        assert_eq!(
            &segs[0],
            &CellInlineSegment {
                text: "h".to_owned(),
                side: CellSegmentSide::Both,
                changed: false,
            }
        );
        assert_eq!(
            &segs[1],
            &CellInlineSegment {
                text: "e".to_owned(),
                side: CellSegmentSide::LeftOnly,
                changed: true,
            }
        );
        assert_eq!(
            &segs[2],
            &CellInlineSegment {
                text: "a".to_owned(),
                side: CellSegmentSide::RightOnly,
                changed: true,
            }
        );
        assert_eq!(
            &segs[3],
            &CellInlineSegment {
                text: "llo".to_owned(),
                side: CellSegmentSide::Both,
                changed: false,
            }
        );

        let segs_equal = cell_inline_diff("abc", "abc");
        assert_eq!(segs_equal.len(), 1);
        assert_eq!(segs_equal[0].side, CellSegmentSide::Both);
        assert!(!segs_equal[0].changed);
    }

    #[test]
    fn column_summaries_counts_correctly() {
        let result = compare_tables(
            "left",
            "name,val\nalpha,1\nbeta,2\n",
            "right",
            "name,val\nalpha,1\nbeta,3\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                ..Default::default()
            },
        )
        .unwrap();

        let summaries = result.column_summaries();
        assert_eq!(summaries.len(), 2);

        let name_col = summaries.iter().find(|s| s.column_index == 0).unwrap();
        assert_eq!(name_col.column_name.as_deref(), Some("name"));
        assert_eq!(name_col.total_cells, 2);
        assert_eq!(name_col.equal_cells, 2);
        assert_eq!(name_col.changed_cells, 0);

        let val_col = summaries.iter().find(|s| s.column_index == 1).unwrap();
        assert_eq!(val_col.column_name.as_deref(), Some("val"));
        assert_eq!(val_col.total_cells, 2);
        assert_eq!(val_col.equal_cells, 1);
        assert_eq!(val_col.changed_cells, 1);
    }

    #[test]
    fn table_result_serialization_includes_cell_metadata() {
        let result = compare_tables(
            "left",
            "name,count\nalpha,1\n",
            "right",
            "name,count\nalpha,2\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                ..Default::default()
            },
        )
        .unwrap();

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("column_name"));
        assert!(json.contains("diff_type"));
        assert!(json.contains("numeric_difference"));
        assert!(json.contains("inline_diff"));

        let deserialized: TableCompareResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.rows.len(), result.rows.len());
        let orig_cell = &result.rows[0].cells[1];
        let de_cell = &deserialized.rows[0].cells[1];
        assert_eq!(orig_cell.column_name, de_cell.column_name);
        assert_eq!(orig_cell.diff_type, de_cell.diff_type);
        assert_eq!(orig_cell.inline_diff, de_cell.inline_diff);
    }

    #[test]
    fn skip_blank_rows_removes_empty_lines() {
        let result = compare_tables(
            "left",
            "a,b\n\n1,2\n\n",
            "right",
            "a,b\n\n1,2\n\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                skip_blank_rows: true,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.rows.len(), 1);
        assert!(result.is_equal());
    }

    #[test]
    fn comment_prefix_skips_comment_lines() {
        let result = compare_tables(
            "left",
            "# comment\na,b\n1,2\n",
            "right",
            "# comment\na,b\n1,2\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                comment_prefix: Some("#".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.rows.len(), 1);
        assert!(result.is_equal());
    }

    #[test]
    fn numeric_tolerance_compares_close_values() {
        let result = compare_tables(
            "left",
            "val\n1.000\n",
            "right",
            "val\n1.001\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                numeric_tolerance: Some(0.01),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(result.is_equal());
        assert_eq!(result.changed_cells, 0);
    }

    #[test]
    fn column_rules_normalize_per_column() {
        // Column 0 differs only by case + surrounding whitespace; column 1 is a
        // genuine value change. With a case-insensitive + trim rule on column 0,
        // only column 1 should register as changed.
        let opts = TableCompareOptions {
            has_header: false,
            column_rules: vec![TableColumnRule {
                column: 0,
                case_insensitive: true,
                trim: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let result = compare_tables("left", "Alpha,1\n", "right", "  alpha ,2\n", &opts).unwrap();
        assert_eq!(result.changed_cells, 1, "only column 1 should differ");
        let cells = &result.rows[0].cells;
        assert_eq!(
            cells[0].state,
            TableCellState::Equal,
            "col 0 normalized equal"
        );
        assert_eq!(cells[1].state, TableCellState::Changed, "col 1 changed");

        // Without the rule the case/whitespace difference is a real change.
        let strict = compare_tables(
            "left",
            "Alpha,1\n",
            "right",
            "  alpha ,1\n",
            &TableCompareOptions::default(),
        )
        .unwrap();
        assert_eq!(strict.changed_cells, 1, "col 0 differs without a rule");
    }

    #[test]
    fn column_rules_per_column_numeric_tolerance_overrides_global() {
        // Column 0 tolerates 0.5; column 1 has no per-column tolerance and no
        // global tolerance, so its 0.4 delta is a real change.
        let opts = TableCompareOptions {
            has_header: false,
            column_rules: vec![TableColumnRule {
                column: 0,
                numeric_tolerance: Some(0.5),
                ..Default::default()
            }],
            ..Default::default()
        };
        let result = compare_tables("left", "1.0,1.0\n", "right", "1.4,1.4\n", &opts).unwrap();
        let cells = &result.rows[0].cells;
        assert_eq!(cells[0].state, TableCellState::Equal, "col 0 within 0.5");
        assert_eq!(
            cells[1].state,
            TableCellState::Changed,
            "col 1 no tolerance"
        );
    }

    #[test]
    fn column_rule_regex_normalizes_before_comparison() {
        // Column 0 carries a volatile "[12:00:00] " timestamp prefix that should
        // be stripped before comparing; column 1 is a genuine change.
        let opts = TableCompareOptions {
            has_header: false,
            column_rules: vec![TableColumnRule {
                column: 0,
                normalize_pattern: Some(r"^\[\d{2}:\d{2}:\d{2}\]\s*".to_string()),
                // normalize_replacement defaults to "" (delete the match).
                ..Default::default()
            }],
            ..Default::default()
        };
        let result = compare_tables(
            "left",
            "[12:00:00] login,1\n",
            "right",
            "[09:30:15] login,2\n",
            &opts,
        )
        .unwrap();
        assert_eq!(result.rows[0].cells[0].state, TableCellState::Equal);
        assert_eq!(result.rows[0].cells[1].state, TableCellState::Changed);
        assert_eq!(result.changed_cells, 1);
    }

    #[test]
    fn parse_datetime_epoch_matches_known_values() {
        assert_eq!(parse_datetime_epoch("1970-01-01"), Some(0));
        assert_eq!(parse_datetime_epoch("1970-01-02"), Some(86_400));
        assert_eq!(
            parse_datetime_epoch("2000-01-01T00:00:00Z"),
            Some(946_684_800)
        );
        assert_eq!(
            parse_datetime_epoch("2024-01-01 00:00:00"),
            Some(1_704_067_200)
        );
        // Fractional seconds and timezone offset are ignored (wall-clock UTC).
        assert_eq!(
            parse_datetime_epoch("2000-01-01T00:00:00.500+05:00"),
            Some(946_684_800)
        );
        assert_eq!(parse_datetime_epoch("not a date"), None);
        assert_eq!(
            parse_datetime_epoch("2024-13-01"),
            None,
            "month out of range"
        );
    }

    #[test]
    fn column_rule_date_tolerance_groups_near_timestamps() {
        let opts = |tol: u64| TableCompareOptions {
            has_header: false,
            column_rules: vec![TableColumnRule {
                column: 0,
                date_tolerance_seconds: Some(tol),
                ..Default::default()
            }],
            ..Default::default()
        };
        // The two timestamps are 30 seconds apart.
        let within = compare_tables(
            "l",
            "2024-01-01T12:00:00\n",
            "r",
            "2024-01-01T12:00:30\n",
            &opts(60),
        )
        .unwrap();
        assert!(within.is_equal(), "within tolerance compares equal");
        let outside = compare_tables(
            "l",
            "2024-01-01T12:00:00\n",
            "r",
            "2024-01-01T12:00:30\n",
            &opts(10),
        )
        .unwrap();
        assert!(!outside.is_equal(), "beyond tolerance compares different");
        // Non-date cells fall back to the normal text comparison.
        let fallback = compare_tables("l", "alpha\n", "r", "beta\n", &opts(60)).unwrap();
        assert!(
            !fallback.is_equal(),
            "non-date cells fall back to text compare"
        );
    }

    #[test]
    fn column_rule_invalid_regex_is_reported() {
        let opts = TableCompareOptions {
            has_header: false,
            column_rules: vec![TableColumnRule {
                column: 0,
                normalize_pattern: Some("(unclosed".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let err = compare_tables("left", "a\n", "right", "a\n", &opts).unwrap_err();
        assert!(
            err.message.contains("invalid normalize regex for column 0"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn key_columns_pair_duplicate_keys_by_occurrence() {
        // Two left rows and two right rows share key "1"; the first left pairs
        // with the first right, the second with the second. No rows are dropped
        // and no false add/remove is produced.
        let result = compare_tables(
            "left",
            "id,val\n1,a\n1,b\n2,c\n",
            "right",
            "id,val\n1,a\n1,b\n2,c\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                key_columns: vec![0],
                ..Default::default()
            },
        )
        .unwrap();

        assert!(result.is_equal(), "duplicate keys should pair 1:1");
        assert_eq!(result.rows.len(), 3);
        for row in &result.rows {
            assert!(!row.has_difference);
            assert!(
                row.cells.iter().all(|c| c.state == TableCellState::Equal),
                "no row should be dropped or marked add/remove"
            );
        }
    }

    #[test]
    fn key_columns_duplicate_keys_with_changed_value() {
        // The second occurrence of key "1" differs; it must surface as a Changed
        // cell rather than collapsing onto the first occurrence.
        let result = compare_tables(
            "left",
            "id,val\n1,a\n1,b\n",
            "right",
            "id,val\n1,a\n1,z\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                key_columns: vec![0],
                ..Default::default()
            },
        )
        .unwrap();

        assert!(!result.is_equal());
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.changed_cells, 1);
        assert_eq!(result.rows[0].cells[1].state, TableCellState::Equal);
        assert_eq!(result.rows[1].cells[1].state, TableCellState::Changed);
    }

    #[test]
    fn key_columns_unbalanced_duplicate_keys_emit_add_remove() {
        // Two left rows with key "1" but only one right row with key "1": the
        // surplus left row becomes left-only, and the unmatched right key "2"
        // becomes right-only.
        let result = compare_tables(
            "left",
            "id,val\n1,a\n1,b\n",
            "right",
            "id,val\n1,a\n2,c\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                key_columns: vec![0],
                ..Default::default()
            },
        )
        .unwrap();

        assert!(!result.is_equal());
        let left_only = result
            .rows
            .iter()
            .filter(|r| r.cells.iter().any(|c| c.state == TableCellState::LeftOnly))
            .count();
        let right_only = result
            .rows
            .iter()
            .filter(|r| r.cells.iter().any(|c| c.state == TableCellState::RightOnly))
            .count();
        assert_eq!(left_only, 1);
        assert_eq!(right_only, 1);
    }

    #[test]
    fn key_columns_ragged_rows_do_not_collapse() {
        // Two left rows are too short to contain the key column (index 1). They
        // must not share a key with each other or match the right "blank"-keyed
        // rows; each ragged row stays unmatched (left-only / right-only).
        let result = compare_tables(
            "left",
            "a,key\nx\ny\n",
            "right",
            "a,key\np\nq\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                key_columns: vec![1],
                ..Default::default()
            },
        )
        .unwrap();

        assert!(!result.is_equal());
        // 2 ragged left rows (left-only) + 2 ragged right rows (right-only).
        assert_eq!(result.rows.len(), 4);
        let left_only = result
            .rows
            .iter()
            .filter(|r| r.cells.iter().any(|c| c.state == TableCellState::LeftOnly))
            .count();
        let right_only = result
            .rows
            .iter()
            .filter(|r| r.cells.iter().any(|c| c.state == TableCellState::RightOnly))
            .count();
        assert_eq!(left_only, 2, "ragged left rows must not collapse together");
        assert_eq!(
            right_only, 2,
            "ragged right rows must not collapse together"
        );
    }

    #[test]
    fn key_columns_ragged_row_does_not_match_empty_key() {
        // A row missing the key column must not match a row whose key column is
        // present but empty. `row_key` distinguishes "absent" from "empty".
        let result = compare_tables(
            "left",
            "a,key\nx\n",
            "right",
            "a,key\ny,\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                key_columns: vec![1],
                ..Default::default()
            },
        )
        .unwrap();

        assert!(!result.is_equal());
        // Left row (no key column) is left-only; right row (empty key) is
        // right-only — they must not pair.
        assert_eq!(result.rows.len(), 2);
        let left_only = result
            .rows
            .iter()
            .filter(|r| r.cells.iter().any(|c| c.state == TableCellState::LeftOnly))
            .count();
        let right_only = result
            .rows
            .iter()
            .filter(|r| r.cells.iter().any(|c| c.state == TableCellState::RightOnly))
            .count();
        assert_eq!(left_only, 1);
        assert_eq!(right_only, 1);
    }

    #[test]
    fn custom_quote_char_parses_correctly() {
        let result = compare_tables(
            "left",
            "a,b\n'hello','world'\n",
            "right",
            "a,b\n'hello','earth'\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
                quote_char: Some('\''),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.rows[0].cells[1].left.as_deref(), Some("world"));
        assert_eq!(result.rows[0].cells[1].right.as_deref(), Some("earth"));
        assert!(!result.is_equal());
    }

    #[test]
    fn compare_table_files_rejects_oversize_files() {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left.csv");
        let right = tmp.path().join("right.csv");
        let f = std::fs::File::create(&left).unwrap();
        f.set_len(MAX_TABLE_FILE_BYTES + 1).unwrap();
        drop(f);
        std::fs::write(&right, "a,b\n1,2\n").unwrap();
        let err = compare_table_files(&left, &right, &TableCompareOptions::default()).unwrap_err();
        assert!(
            err.to_string().contains("table-file limit"),
            "error should mention table-file limit; got: {err}"
        );
    }
}
