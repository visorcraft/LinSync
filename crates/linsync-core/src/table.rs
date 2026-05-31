use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

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
        }
    }
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
            options.numeric_tolerance,
        ));
    }

    let hdr_ref = header.as_ref();
    let max_rows = left_rows.len().max(right_rows.len());
    let mut rows = Vec::new();
    let mut changed_cells = 0;
    let tolerance = options.numeric_tolerance;

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
                (Some(left), Some(right)) if cells_equal_with_tolerance(left, right, tolerance) => {
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

fn row_key(row: &[String], key_columns: &[usize]) -> Vec<String> {
    key_columns
        .iter()
        .map(|&ci| row.get(ci).cloned().unwrap_or_default())
        .collect()
}

fn compare_rows_by_cell(
    row_index: usize,
    left_row: Option<&[String]>,
    right_row: Option<&[String]>,
    ignore_set: &std::collections::HashSet<usize>,
    header: Option<&Vec<String>>,
    tolerance: Option<f64>,
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
            (Some(l), Some(r)) if cells_equal_with_tolerance(l, r, tolerance) => {
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
    tolerance: Option<f64>,
) -> TableCompareResult {
    let hdr_ref = header.as_ref();
    let mut left_map: HashMap<Vec<String>, usize> = HashMap::new();
    for (i, row) in left_rows.iter().enumerate() {
        let key = row_key(row, key_columns);
        left_map.entry(key).or_insert(i);
    }

    let mut right_map: HashMap<Vec<String>, usize> = HashMap::new();
    for (i, row) in right_rows.iter().enumerate() {
        let key = row_key(row, key_columns);
        right_map.entry(key).or_insert(i);
    }

    let mut rows = Vec::new();
    let mut changed_cells = 0;
    let mut seen_keys = std::collections::HashSet::new();

    if ignore_row_order {
        let mut all_keys: Vec<&Vec<String>> = left_map.keys().chain(right_map.keys()).collect();
        all_keys.sort();
        for key in all_keys {
            if !seen_keys.insert(key.clone()) {
                continue;
            }
            let left_idx = left_map.get(key);
            let right_idx = right_map.get(key);
            let (row_diff, cc) = compare_rows_by_cell(
                rows.len(),
                left_idx.map(|&i| left_rows[i].as_slice()),
                right_idx.map(|&i| right_rows[i].as_slice()),
                ignore_set,
                hdr_ref,
                tolerance,
            );
            changed_cells += cc;
            rows.push(row_diff);
        }
    } else {
        for row in left_rows {
            let key = row_key(row, key_columns);
            seen_keys.insert(key.clone());
            let right_idx = right_map.get(&key);
            let (row_diff, cc) = compare_rows_by_cell(
                rows.len(),
                Some(row.as_slice()),
                right_idx.map(|&ri| right_rows[ri].as_slice()),
                ignore_set,
                hdr_ref,
                tolerance,
            );
            changed_cells += cc;
            rows.push(row_diff);
        }

        for row in right_rows {
            let key = row_key(row, key_columns);
            if seen_keys.contains(&key) {
                continue;
            }
            seen_keys.insert(key);
            let (row_diff, cc) = compare_rows_by_cell(
                rows.len(),
                None,
                Some(row.as_slice()),
                ignore_set,
                hdr_ref,
                tolerance,
            );
            changed_cells += cc;
            rows.push(row_diff);
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
}
