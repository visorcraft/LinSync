use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TableCompareOptions {
    pub delimiter: char,
    pub has_header: bool,
    pub key_columns: Vec<usize>,
    pub ignore_columns: Vec<usize>,
    pub ignore_row_order: bool,
}

impl Default for TableCompareOptions {
    fn default() -> Self {
        Self {
            delimiter: ',',
            has_header: false,
            key_columns: Vec::new(),
            ignore_columns: Vec::new(),
            ignore_row_order: false,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableCellState {
    Equal,
    Changed,
    LeftOnly,
    RightOnly,
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
    let mut left_rows = parse_delimited(left, options.delimiter)?;
    let mut right_rows = parse_delimited(right, options.delimiter)?;
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
        ));
    }

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
                cells.push(TableCellDiff {
                    column_index,
                    left: left_row.and_then(|r| r.get(column_index)).cloned(),
                    right: right_row.and_then(|r| r.get(column_index)).cloned(),
                    state: TableCellState::Equal,
                });
                continue;
            }

            let left_cell = left_row.and_then(|row| row.get(column_index)).cloned();
            let right_cell = right_row.and_then(|row| row.get(column_index)).cloned();
            let state = match (&left_cell, &right_cell) {
                (Some(left), Some(right)) if left == right => TableCellState::Equal,
                (Some(_), Some(_)) => TableCellState::Changed,
                (Some(_), None) => TableCellState::LeftOnly,
                (None, Some(_)) => TableCellState::RightOnly,
                (None, None) => continue,
            };

            if state != TableCellState::Equal {
                changed_cells += 1;
                has_difference = true;
            }

            cells.push(TableCellDiff {
                column_index,
                left: left_cell,
                right: right_cell,
                state,
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
) -> (TableRowDiff, usize) {
    let max_cols = left_row
        .map_or(0, |r| r.len())
        .max(right_row.map_or(0, |r| r.len()));
    let mut cells = Vec::new();
    let mut has_difference = false;
    let mut changed_cells = 0;

    for column_index in 0..max_cols {
        if ignore_set.contains(&column_index) {
            cells.push(TableCellDiff {
                column_index,
                left: left_row.and_then(|r| r.get(column_index)).cloned(),
                right: right_row.and_then(|r| r.get(column_index)).cloned(),
                state: TableCellState::Equal,
            });
            continue;
        }

        let left_cell = left_row.and_then(|r| r.get(column_index)).cloned();
        let right_cell = right_row.and_then(|r| r.get(column_index)).cloned();
        let state = match (&left_cell, &right_cell) {
            (Some(l), Some(r)) if l == r => TableCellState::Equal,
            (Some(_), Some(_)) => TableCellState::Changed,
            (Some(_), None) => TableCellState::LeftOnly,
            (None, Some(_)) => TableCellState::RightOnly,
            (None, None) => continue,
        };

        if state != TableCellState::Equal {
            changed_cells += 1;
            has_difference = true;
        }

        cells.push(TableCellDiff {
            column_index,
            left: left_cell,
            right: right_cell,
            state,
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
) -> TableCompareResult {
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
}
